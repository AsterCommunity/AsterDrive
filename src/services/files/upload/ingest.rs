//! Upload data planes. Every entry point consumes an already-created session.

mod chunk;
pub(crate) mod provider_relay;
pub(crate) mod staging;

use actix_web::web;

use crate::api::api_error_code::ApiErrorCode;
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::ops::audit::{self, AuditContext};
use crate::services::workspace::models::FileInfo;
use crate::services::workspace::storage::{self, WorkspaceStorageScope};
use aster_drive_model::types::{UploadSessionKind, UploadSessionStatus};

#[cfg(debug_assertions)]
pub use chunk::test_support;
pub use chunk::{
    upload_chunk, upload_chunk_bytes, upload_chunk_bytes_for_team, upload_chunk_for_team,
    upload_chunk_payload, upload_chunk_payload_for_team,
};

pub(crate) async fn ingest_stream(
    state: &PrimaryAppState,
    upload_id: &str,
    scope: WorkspaceStorageScope,
    payload: web::Payload,
    audit_ctx: &AuditContext,
) -> Result<FileInfo> {
    let session = super::session::scope::load_upload_session(state, scope, upload_id).await?;
    if session.session_kind != UploadSessionKind::Stream {
        return Err(AsterError::validation_error(
            "upload session does not accept a stream body",
        ));
    }
    let claim_time = chrono::Utc::now();
    if session.expires_at <= claim_time {
        return Err(AsterError::upload_session_expired("upload session expired"));
    }
    if session.status != UploadSessionStatus::Uploading {
        return Err(
            AsterError::conflict("stream upload session is no longer accepting a body")
                .with_api_error_code(ApiErrorCode::UploadStatusConflict),
        );
    }

    let plan = super::plan::UploadPlan::try_from_session(&session)?;
    let policy = state.policy_snapshot().get_policy_or_err(plan.policy_id)?;
    let actor_username = storage::load_scope_actor_username_cached(state, scope).await?;
    let claimed = crate::db::repository::upload_session_repo::try_transition_status_before_expiry(
        state.writer_db(),
        upload_id,
        UploadSessionStatus::Uploading,
        UploadSessionStatus::Assembling,
        claim_time,
    )
    .await?;
    if !claimed {
        let latest =
            crate::db::repository::upload_session_repo::find_by_id(state.writer_db(), upload_id)
                .await?;
        if latest.expires_at <= chrono::Utc::now() {
            return Err(AsterError::upload_session_expired("upload session expired"));
        }
        return Err(
            AsterError::conflict("stream upload session is already being processed")
                .with_api_error_code(ApiErrorCode::UploadStatusConflict),
        );
    }

    let file = match storage::ingest_stream(
        state,
        payload,
        storage::IngestStreamRequest {
            scope,
            folder_id: session.folder_id,
            filename: &plan.filename,
            mime_type: &plan.mime_type,
            policy: &policy,
            declared_size: plan.total_size,
            actor_username: Some(&actor_username),
            upload_id,
        },
    )
    .await
    {
        Ok(file) => file,
        Err(error) => {
            let transition = crate::db::repository::upload_session_repo::try_fail_with_expiration(
                state.writer_db(),
                upload_id,
                UploadSessionStatus::Assembling,
                chrono::Utc::now() + chrono::Duration::seconds(15),
            )
            .await;
            record_stream_completion_metric(state, false);
            transition?;
            return Err(error);
        }
    };

    record_stream_completion_metric(state, true);

    let details =
        crate::services::files::file::audit_location_details_for_model(state, scope, &file).await;
    audit::log_with_details(
        state,
        audit_ctx,
        audit::AuditAction::FileUpload,
        crate::services::ops::audit::AuditEntityType::File,
        Some(file.id),
        Some(&file.name),
        || details.clone(),
    )
    .await;
    Ok(file.into())
}

fn record_stream_completion_metric(state: &impl SharedRuntimeState, success: bool) {
    let status = if success { "success" } else { "failure" };
    state
        .metrics()
        .record_upload_session_event("stream", "complete", status);
    state.metrics().record_file_upload("stream", status);
}

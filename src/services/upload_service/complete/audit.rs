use std::time::Instant;

use crate::entities::upload_session;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::audit_service::{self, AuditContext};
use crate::services::workspace_models::FileInfo;
use crate::services::workspace_storage_service::WorkspaceStorageScope;
use crate::types::UploadSessionStatus;

use super::complete_upload_impl_with_hints;

#[derive(Clone, Copy, Default)]
pub(super) struct CompleteUploadHints<'a> {
    pub actor_username: Option<&'a str>,
}

pub(super) async fn complete_upload_impl_with_audit(
    state: &PrimaryAppState,
    session: upload_session::Model,
    parts: Option<Vec<(i32, String)>>,
    audit_ctx: &AuditContext,
    hints: CompleteUploadHints<'_>,
) -> Result<FileInfo> {
    // Completed sessions are idempotent replays; only the first successful
    // completion writes the upload audit event.
    let should_log = should_log_upload_completion(&session);
    let upload_id = session.id.clone();
    let complete_started_at = Instant::now();
    let file = complete_upload_impl_with_hints(state, session, parts, hints).await?;
    let complete_elapsed_ms = complete_started_at.elapsed().as_millis();
    if should_log {
        let audit_started_at = Instant::now();
        let scope = match file.team_id {
            Some(team_id) => WorkspaceStorageScope::Team {
                team_id,
                actor_user_id: audit_ctx.user_id,
            },
            None => WorkspaceStorageScope::Personal {
                user_id: audit_ctx.user_id,
            },
        };
        let details =
            crate::services::file_service::audit_location_details_for_model(state, scope, &file)
                .await;
        audit_service::log_with_details(
            state,
            audit_ctx,
            audit_service::AuditAction::FileUpload,
            crate::services::audit_service::AuditEntityType::File,
            Some(file.id),
            Some(&file.name),
            || details.clone(),
        )
        .await;
        tracing::debug!(
            upload_id = %upload_id,
            file_id = file.id,
            complete_elapsed_ms,
            audit_elapsed_ms = audit_started_at.elapsed().as_millis(),
            total_elapsed_ms = complete_started_at.elapsed().as_millis(),
            "audited upload completion finished"
        );
    } else {
        tracing::debug!(
            upload_id = %upload_id,
            file_id = file.id,
            complete_elapsed_ms,
            total_elapsed_ms = complete_started_at.elapsed().as_millis(),
            "upload completion returned completed session without audit"
        );
    }
    Ok(file.into())
}

pub(super) fn should_log_upload_completion(session: &upload_session::Model) -> bool {
    session.status != UploadSessionStatus::Completed
}

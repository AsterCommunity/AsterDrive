use chrono::{Duration, Utc};

use crate::api::constants::HOUR_SECS;
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::upload_service::responses::InitUploadResponse;
use crate::services::upload_service::shared::{
    UniqueUuidAttempt, delete_upload_session_record_after_init_error, with_unique_upload_id,
};
use crate::services::workspace_storage_service::{
    PolicyUploadTransport, resolve_policy_upload_transport,
};
use crate::types::{RemoteUploadStrategy, UploadMode, UploadSessionStatus};
use crate::utils::numbers;

use super::context::{
    InitUploadContext, MultipartSessionInitParams, UploadSessionRecordParams,
    direct_upload_response, init_multipart_session_with_retry, try_persist_upload_session,
};

pub(super) async fn init_remote_upload(
    state: &PrimaryAppState,
    ctx: &InitUploadContext,
) -> Result<Option<InitUploadResponse>> {
    let transport = resolve_policy_upload_transport(&ctx.policy);
    let PolicyUploadTransport::Remote(strategy) = transport else {
        return Ok(None);
    };
    match strategy {
        RemoteUploadStrategy::RelayStream => init_relay_stream_remote_upload(state, ctx, transport)
            .await
            .map(Some),
        RemoteUploadStrategy::Presigned => init_presigned_remote_upload(state, ctx, transport)
            .await
            .map(Some),
    }
}

async fn init_relay_stream_remote_upload(
    state: &PrimaryAppState,
    ctx: &InitUploadContext,
    transport: PolicyUploadTransport,
) -> Result<InitUploadResponse> {
    let chunk_size = transport.effective_chunk_size(&ctx.policy);

    if transport.resolve_init_mode(&ctx.policy, ctx.total_size) == UploadMode::Direct {
        tracing::debug!(
            scope = ?ctx.scope,
            policy_id = ctx.policy.id,
            mode = ?UploadMode::Direct,
            folder_id = ctx.target.folder_id,
            "selected remote relay stream direct upload mode"
        );
        return Ok(direct_upload_response());
    }

    let multipart = state.driver_registry().get_multipart_driver(&ctx.policy)?;
    let total_chunks =
        numbers::calc_total_chunks(ctx.total_size, chunk_size, "remote relay multipart upload")?;

    init_multipart_session_with_retry(
        state,
        ctx,
        multipart.as_ref(),
        MultipartSessionInitParams {
            mode: UploadMode::Chunked,
            status: UploadSessionStatus::Uploading,
            chunk_size,
            total_chunks,
            expires_in: Duration::hours(24),
            log_label: "remote relay multipart",
            abort_db_error_context: "remote upload session DB initialization error",
            abort_db_error_message:
                "failed to abort remote multipart upload after DB initialization error",
            abort_collision_context: "remote upload session id collision",
        },
    )
    .await
}

async fn init_presigned_remote_upload(
    state: &PrimaryAppState,
    ctx: &InitUploadContext,
    transport: PolicyUploadTransport,
) -> Result<InitUploadResponse> {
    let driver = state.driver_registry().get_driver(&ctx.policy)?;
    let chunk_size = transport.effective_chunk_size(&ctx.policy);

    if transport.resolve_init_mode(&ctx.policy, ctx.total_size) == UploadMode::Presigned {
        return init_remote_presigned_single_upload(state, ctx, driver.as_ref()).await;
    }

    let multipart = state.driver_registry().get_multipart_driver(&ctx.policy)?;
    let total_chunks = numbers::calc_total_chunks(
        ctx.total_size,
        chunk_size,
        "remote presigned multipart upload",
    )?;

    init_multipart_session_with_retry(
        state,
        ctx,
        multipart.as_ref(),
        MultipartSessionInitParams {
            mode: UploadMode::PresignedMultipart,
            status: UploadSessionStatus::Presigned,
            chunk_size,
            total_chunks,
            expires_in: Duration::hours(24),
            log_label: "remote presigned multipart",
            abort_db_error_context: "remote upload session DB initialization error",
            abort_db_error_message:
                "failed to abort remote multipart upload after DB initialization error",
            abort_collision_context: "remote upload session id collision",
        },
    )
    .await
}

async fn init_remote_presigned_single_upload(
    state: &PrimaryAppState,
    ctx: &InitUploadContext,
    driver: &dyn crate::storage::StorageDriver,
) -> Result<InitUploadResponse> {
    with_unique_upload_id(|upload_id| async {
        let temp_key = format!("files/{upload_id}");
        let inserted = try_persist_upload_session(
            state.writer_db(),
            UploadSessionRecordParams {
                upload_id: &upload_id,
                scope: ctx.scope,
                filename: &ctx.target.filename,
                total_size: ctx.total_size,
                chunk_size: 0,
                total_chunks: 0,
                folder_id: ctx.target.folder_id,
                policy_id: ctx.policy.id,
                frontend_client_id: ctx.frontend_client_id.as_deref(),
                status: UploadSessionStatus::Presigned,
                object_temp_key: Some(&temp_key),
                object_multipart_id: None,
                expires_at: Utc::now() + Duration::hours(1),
            },
        )
        .await?;
        if !inserted {
            return Ok(UniqueUuidAttempt::Collision);
        }

        let presigned_url = match remote_presigned_put_url(driver, &temp_key).await {
            Ok(url) => url,
            Err(error) => {
                delete_upload_session_record_after_init_error(
                    state.writer_db(),
                    &upload_id,
                    "remote presigned URL initialization error",
                )
                .await;
                return Err(error);
            }
        };

        tracing::debug!(
            scope = ?ctx.scope,
            upload_id = %upload_id,
            policy_id = ctx.policy.id,
            mode = ?UploadMode::Presigned,
            folder_id = ctx.target.folder_id,
            "initialized remote presigned upload session"
        );

        Ok(UniqueUuidAttempt::Accepted(InitUploadResponse {
            mode: UploadMode::Presigned,
            upload_id: Some(upload_id),
            chunk_size: None,
            total_chunks: None,
            presigned_url: Some(presigned_url),
            presigned_headers: Default::default(),
            presigned_require_etag: Some(true),
        }))
    })
    .await
}

async fn remote_presigned_put_url(
    driver: &dyn crate::storage::StorageDriver,
    temp_key: &str,
) -> Result<String> {
    let presigned_driver = driver.as_presigned().ok_or_else(|| {
        AsterError::storage_driver_error("remote driver does not implement presigned PUT")
    })?;
    presigned_driver
        .presigned_put_url(temp_key, std::time::Duration::from_secs(HOUR_SECS))
        .await?
        .ok_or_else(|| {
            AsterError::storage_driver_error("remote driver returned no presigned PUT URL")
        })
}

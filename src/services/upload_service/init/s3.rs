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
use crate::types::{S3UploadStrategy, UploadMode, UploadSessionStatus};
use crate::utils::numbers;

use super::context::{
    InitUploadContext, MultipartSessionInitParams, UploadSessionRecordParams,
    direct_upload_response, init_multipart_session_with_retry, try_persist_upload_session,
};

pub(super) async fn init_s3_upload(
    state: &PrimaryAppState,
    ctx: &InitUploadContext,
) -> Result<Option<InitUploadResponse>> {
    let transport = resolve_policy_upload_transport(&ctx.policy);
    let PolicyUploadTransport::ObjectStorage(strategy) = transport else {
        return Ok(None);
    };
    match strategy {
        S3UploadStrategy::Presigned => init_presigned_s3_upload(state, ctx, transport)
            .await
            .map(Some),
        S3UploadStrategy::RelayStream => init_relay_stream_s3_upload(state, ctx, transport)
            .await
            .map(Some),
    }
}

async fn init_presigned_s3_upload(
    state: &PrimaryAppState,
    ctx: &InitUploadContext,
    transport: PolicyUploadTransport,
) -> Result<InitUploadResponse> {
    let driver = state.driver_registry().get_driver(&ctx.policy)?;
    let chunk_size = transport.effective_chunk_size(&ctx.policy);

    // 小文件 presigned：客户端直接 PUT 到最终 temp object，不经过服务端 relay，
    // 也不需要 chunk bookkeeping。
    if transport.resolve_init_mode(&ctx.policy, ctx.total_size) == UploadMode::Presigned {
        return init_presigned_s3_single_upload(state, ctx, driver.as_ref()).await;
    }

    // 大文件 presigned multipart：服务端仍然不接管数据流，但必须保留 session，
    // 用来记录 multipart upload_id、分片总数以及后续 complete 阶段的收口点。
    let multipart = state.driver_registry().get_multipart_driver(&ctx.policy)?;
    let total_chunks =
        numbers::calc_total_chunks(ctx.total_size, chunk_size, "presigned multipart upload")?;

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
            log_label: "presigned multipart",
            abort_db_error_context: "upload session DB initialization error",
            abort_db_error_message:
                "failed to abort multipart upload after DB initialization error",
            abort_collision_context: "upload session id collision",
        },
    )
    .await
}

async fn init_presigned_s3_single_upload(
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
                s3_temp_key: Some(&temp_key),
                s3_multipart_id: None,
                expires_at: Utc::now() + Duration::hours(1),
            },
        )
        .await?;
        if !inserted {
            return Ok(UniqueUuidAttempt::Collision);
        }

        let (presigned_url, presigned_headers, presigned_require_etag) =
            match presigned_put_request(driver, &temp_key).await {
                Ok(request) => request,
                Err(error) => {
                    delete_upload_session_record_after_init_error(
                        state.writer_db(),
                        &upload_id,
                        "presigned URL initialization error",
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
            "initialized presigned upload session"
        );

        Ok(UniqueUuidAttempt::Accepted(InitUploadResponse {
            mode: UploadMode::Presigned,
            upload_id: Some(upload_id),
            chunk_size: None,
            total_chunks: None,
            presigned_url: Some(presigned_url),
            presigned_headers,
            presigned_require_etag: Some(presigned_require_etag),
        }))
    })
    .await
}

async fn init_relay_stream_s3_upload(
    state: &PrimaryAppState,
    ctx: &InitUploadContext,
    transport: PolicyUploadTransport,
) -> Result<InitUploadResponse> {
    let chunk_size = transport.effective_chunk_size(&ctx.policy);

    // relay_stream + 小文件：直接走普通上传接口，让服务端把字节流转发到驱动。
    if transport.resolve_init_mode(&ctx.policy, ctx.total_size) == UploadMode::Direct {
        tracing::debug!(
            scope = ?ctx.scope,
            policy_id = ctx.policy.id,
            mode = ?UploadMode::Direct,
            folder_id = ctx.target.folder_id,
            "selected direct relay upload mode"
        );
        return Ok(direct_upload_response());
    }

    // relay_stream + 大文件：客户端仍然分片传给服务端，服务端再逐片上传到 S3 multipart。
    let multipart = state.driver_registry().get_multipart_driver(&ctx.policy)?;
    let total_chunks =
        numbers::calc_total_chunks(ctx.total_size, chunk_size, "relay multipart upload")?;

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
            log_label: "relay multipart",
            abort_db_error_context: "upload session DB initialization error",
            abort_db_error_message:
                "failed to abort multipart upload after DB initialization error",
            abort_collision_context: "upload session id collision",
        },
    )
    .await
}

async fn presigned_put_request(
    driver: &dyn crate::storage::StorageDriver,
    temp_key: &str,
) -> Result<(String, std::collections::BTreeMap<String, String>, bool)> {
    let presigned_driver = driver
        .as_presigned()
        .ok_or_else(|| AsterError::storage_driver_error("presigned PUT not supported by driver"))?;
    let url = presigned_driver
        .presigned_put_url(temp_key, std::time::Duration::from_secs(HOUR_SECS))
        .await?
        .ok_or_else(|| AsterError::storage_driver_error("presigned PUT not supported by driver"))?;
    Ok((
        url,
        presigned_driver.presigned_put_headers(),
        presigned_driver.presigned_put_requires_etag(),
    ))
}

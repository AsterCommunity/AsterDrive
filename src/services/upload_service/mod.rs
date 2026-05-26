//! 上传服务聚合入口。
//!
//! 这组模块负责“先协商上传模式，再按对应协议落盘，最后把 upload session
//! 转成正式文件”这条链路。调用方通常只关心 init / chunk / complete / cancel，
//! 具体是本地分片、S3 relay multipart 还是 presigned multipart，由内部按策略决定。

mod chunk;
mod complete;
mod init;
mod lifecycle;
mod progress;
mod responses;
mod scope;
mod shared;

use std::time::Instant;

use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::audit_service::{self, AuditContext};
use crate::services::workspace_models::FileInfo;
use crate::services::workspace_storage_service::{self, WorkspaceStorageScope};

pub use chunk::{
    upload_chunk, upload_chunk_bytes, upload_chunk_bytes_for_team, upload_chunk_for_team,
    upload_chunk_payload, upload_chunk_payload_for_team,
};
pub use complete::{
    complete_upload, complete_upload_for_team, complete_upload_for_team_with_audit,
    complete_upload_with_audit,
};
pub use init::{
    InitUploadParams, init_upload, init_upload_for_team, init_upload_for_team_with_frontend_client,
    init_upload_with_frontend_client,
};
pub use lifecycle::{
    ForceCleanupByPolicyResult, cancel_upload, cancel_upload_for_team, cleanup_expired,
    force_cleanup_by_policy,
};
pub use progress::{
    get_progress, get_progress_for_team, list_recoverable_sessions,
    list_recoverable_sessions_for_team, presign_parts, presign_parts_for_team,
};
pub use responses::{
    ChunkUploadResponse, InitUploadResponse, RecoverableUploadSessionResponse,
    UploadProgressResponse,
};

#[derive(Clone, Copy)]
pub(crate) struct UploadInScopeParams<'a> {
    pub scope: WorkspaceStorageScope,
    pub folder_id: Option<i64>,
    pub relative_path: Option<&'a str>,
    pub declared_size: Option<i64>,
}

// 审计包装放在聚合层，避免 init/chunk/complete 这些核心流程混入 route 级副作用。
pub(crate) async fn upload_in_scope_with_audit(
    state: &PrimaryAppState,
    payload: &mut actix_multipart::Multipart,
    params: UploadInScopeParams<'_>,
    audit_ctx: &AuditContext,
) -> Result<FileInfo> {
    let upload_started_at = Instant::now();
    let actor_username =
        workspace_storage_service::load_scope_actor_username_cached(state, params.scope).await?;
    let file = workspace_storage_service::upload_with_hints(
        state,
        params.scope,
        payload,
        params.folder_id,
        params.relative_path,
        params.declared_size,
        workspace_storage_service::WorkspaceUploadHints {
            actor_username: Some(&actor_username),
        },
    )
    .await
    .inspect(|_| record_direct_upload_metric(state, "success"))
    .inspect_err(|_| record_direct_upload_metric(state, "failure"))?;
    let store_elapsed_ms = upload_started_at.elapsed().as_millis();

    let audit_started_at = Instant::now();
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::FileUpload,
        crate::services::audit_service::AuditEntityType::File,
        Some(file.id),
        Some(&file.name),
        None,
    )
    .await;
    let audit_elapsed_ms = audit_started_at.elapsed().as_millis();
    tracing::debug!(
        scope = ?params.scope,
        file_id = file.id,
        size = file.size,
        store_elapsed_ms,
        audit_elapsed_ms,
        total_elapsed_ms = upload_started_at.elapsed().as_millis(),
        "direct upload completed"
    );
    Ok(file.into())
}

fn record_direct_upload_metric(state: &PrimaryAppState, status: &'static str) {
    state.metrics.record_file_upload("direct", status);
}

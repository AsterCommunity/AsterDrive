//! 文件服务聚合入口。

mod common;
mod content;
mod deletion;
mod download;
mod lock;
mod metadata;
mod thumbnail;
mod transfer;

use std::future::Future;

use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::services::audit_service::{self, AuditContext};
use crate::services::workspace_models::FileInfo;
use crate::services::workspace_storage_service::{self, WorkspaceStorageScope};
use crate::types::NullablePatch;

pub use crate::services::media_metadata_service::{MediaMetadataInfo, MediaMetadataLookup};
pub(crate) use common::{
    DownloadDisposition, ensure_personal_file_scope, if_none_match_matches,
    if_none_match_matches_value, inline_sandbox_csp, requires_inline_sandbox,
};
pub use content::{
    StoreFromTempRequest, create_empty, resolve_policy_for_size, store_from_temp, update_content,
    upload,
};
pub(crate) use content::{
    StreamedTempUpload, stream_request_body_to_temp_upload, update_content_stream_in_scope,
};
pub(crate) use deletion::{
    BatchPurgeSummary, batch_purge_in_resource_scope, batch_purge_in_resource_scope_silent,
    batch_purge_in_scope, cleanup_unreferenced_blob, cleanup_unreferenced_blob_with_driver,
    delete_in_scope, ensure_blob_cleanup_if_unreferenced,
};
pub use deletion::{batch_purge, delete, purge};
pub use download::range::ResolvedDownloadRange;
pub(crate) use download::range::parse_range_header;
pub use download::{DownloadOutcome, StreamedFile, download, download_raw};
pub(crate) use download::{
    build_download_outcome_with_disposition_and_range, build_stream_outcome_with_disposition,
    build_stream_outcome_with_disposition_and_range, download_in_scope_with_range_and_file,
    outcome_to_response,
};
pub use lock::set_lock;
pub(crate) use lock::set_lock_in_scope;
pub use metadata::{get_info, move_file, update};
pub(crate) use metadata::{
    get_info_in_scope, get_info_with_storage_used_in_scope, update_in_scope,
};
pub use thumbnail::{ImagePreviewResult, ThumbnailResult, get_thumbnail_data};
pub(crate) use thumbnail::{
    get_image_preview_data_in_scope, get_thumbnail_data_in_scope, image_preview_for_file,
};
pub(crate) use transfer::{
    BatchDuplicateFileRecordSpec, BatchDuplicateFileRecordTargetSpec,
    batch_duplicate_file_records_to_mixed_folders_in_scope,
    batch_duplicate_file_records_with_specs_in_scope, copy_file_in_scope,
};
pub use transfer::{batch_duplicate_file_records, copy_file, duplicate_file_record};

pub(crate) async fn create_empty_in_scope_with_audit(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder_id: Option<i64>,
    name: &str,
    audit_ctx: &AuditContext,
) -> Result<FileInfo> {
    let file = workspace_storage_service::create_empty(state, scope, folder_id, name).await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::FileCreate,
        crate::services::audit_service::AuditEntityType::File,
        Some(file.id),
        Some(&file.name),
        None,
    )
    .await;
    Ok(file.into())
}

pub(crate) async fn delete_in_scope_with_audit(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_id: i64,
    audit_ctx: &AuditContext,
) -> Result<()> {
    delete_in_scope(state, scope, file_id).await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::FileDelete,
        crate::services::audit_service::AuditEntityType::File,
        Some(file_id),
        None,
        None,
    )
    .await;
    Ok(())
}

pub(crate) async fn update_in_scope_with_audit(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_id: i64,
    name: Option<String>,
    folder_id: NullablePatch<i64>,
    audit_ctx: &AuditContext,
) -> Result<FileInfo> {
    let action = if folder_id.is_present() {
        audit_service::AuditAction::FileMove
    } else {
        audit_service::AuditAction::FileRename
    };
    let file = update_in_scope(state, scope, file_id, name, folder_id).await?;
    audit_service::log(
        state,
        audit_ctx,
        action,
        crate::services::audit_service::AuditEntityType::File,
        Some(file.id),
        Some(&file.name),
        None,
    )
    .await;
    Ok(file.into())
}

pub(crate) async fn update_content_stream_in_scope_with_audit(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_id: i64,
    payload: &mut actix_web::web::Payload,
    declared_size: Option<i64>,
    if_match: Option<&str>,
    audit_ctx: &AuditContext,
) -> Result<(FileInfo, String)> {
    let (file, new_hash) =
        update_content_stream_in_scope(state, scope, file_id, payload, declared_size, if_match)
            .await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::FileEdit,
        crate::services::audit_service::AuditEntityType::File,
        Some(file.id),
        Some(&file.name),
        None,
    )
    .await;
    Ok((file.into(), new_hash))
}

pub(crate) async fn set_lock_in_scope_with_audit(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_id: i64,
    locked: bool,
    audit_ctx: &AuditContext,
) -> Result<FileInfo> {
    let file = set_lock_in_scope(state, scope, file_id, locked).await?;
    audit_service::log(
        state,
        audit_ctx,
        if locked {
            audit_service::AuditAction::FileLock
        } else {
            audit_service::AuditAction::FileUnlock
        },
        crate::services::audit_service::AuditEntityType::File,
        Some(file.id),
        Some(&file.name),
        None,
    )
    .await;
    Ok(file.into())
}

pub(crate) async fn copy_file_in_scope_with_audit(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_id: i64,
    target_folder_id: Option<i64>,
    audit_ctx: &AuditContext,
) -> Result<FileInfo> {
    let file = copy_file_in_scope(state, scope, file_id, target_folder_id).await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::FileCopy,
        crate::services::audit_service::AuditEntityType::File,
        Some(file.id),
        Some(&file.name),
        None,
    )
    .await;
    Ok(file.into())
}

pub(crate) async fn download_in_scope_with_file_and_audit(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file: crate::entities::file::Model,
    if_none_match: Option<&str>,
    range: Option<download::range::ResolvedDownloadRange>,
    audit_ctx: &AuditContext,
) -> Result<DownloadOutcome> {
    let file_id = file.id;
    let has_range = range.is_some();
    let outcome = record_download_result(
        state,
        "authenticated",
        has_range,
        download_in_scope_with_range_and_file(
            state,
            scope,
            file_id,
            Some(file),
            if_none_match,
            range,
        ),
    )
    .await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::FileDownload,
        crate::services::audit_service::AuditEntityType::File,
        Some(file_id),
        None,
        None,
    )
    .await;
    Ok(outcome)
}

pub fn record_download_metric(
    state: &PrimaryAppState,
    source: &'static str,
    outcome: &DownloadOutcome,
) {
    state
        .metrics
        .record_file_download(source, outcome.metrics_outcome(), outcome.has_range());
}

pub fn record_download_failure_metric(state: &PrimaryAppState, source: &'static str) {
    state.metrics.record_file_download(source, "failure", false);
}

pub fn record_download_failure_metric_with_reason(
    state: &PrimaryAppState,
    source: &'static str,
    reason: &'static str,
    has_range: bool,
) {
    let outcome = format!("failure:{reason}");
    state
        .metrics
        .record_file_download(source, outcome.as_str(), has_range);
}

pub async fn record_download_result<Fut>(
    state: &PrimaryAppState,
    source: &'static str,
    has_range: bool,
    fut: Fut,
) -> Result<DownloadOutcome>
where
    Fut: Future<Output = Result<DownloadOutcome>>,
{
    match fut.await {
        Ok(outcome) => {
            record_download_metric(state, source, &outcome);
            Ok(outcome)
        }
        Err(error) => {
            record_download_failure_metric_with_reason(
                state,
                source,
                download_failure_reason(&error),
                has_range,
            );
            Err(error)
        }
    }
}

fn download_failure_reason(error: &AsterError) -> &'static str {
    if let Some(kind) = error.storage_error_kind() {
        return kind.as_str();
    }

    match error {
        AsterError::FileNotFound(_)
        | AsterError::RecordNotFound(_)
        | AsterError::ShareNotFound(_) => "not_found",
        AsterError::ShareExpired(_) => "expired",
        AsterError::SharePasswordRequired(_) => "password_required",
        AsterError::ShareDownloadLimit(_) => "download_limit",
        AsterError::AuthForbidden(_) | AsterError::AuthPendingActivation(_) => "forbidden",
        AsterError::AuthTokenMissing(_) => "token_missing",
        AsterError::AuthTokenExpired(_) => "token_expired",
        AsterError::AuthTokenInvalid(_) => "token_invalid",
        AsterError::ValidationError(_) => "validation",
        AsterError::RateLimited(_) => "rate_limited",
        _ => "error",
    }
}

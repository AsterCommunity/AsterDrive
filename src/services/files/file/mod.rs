//! 文件服务聚合入口。

mod common;
mod content;
mod deletion;
mod download;
mod lock;
mod metadata;
mod resource_handle;
mod thumbnail;
mod transfer;

use std::future::Future;

use serde_json::json;

use crate::entities::file;
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState, StorageChangeRuntimeState};
use crate::services::audit_service::{self, AuditContext};
use crate::services::workspace::models::FileInfo;
use crate::services::workspace::storage::{self, WorkspaceStorageScope};
use crate::types::NullablePatch;

pub(crate) use crate::services::download_headers::DownloadDisposition;
pub use crate::services::media_metadata_service::{MediaMetadataInfo, MediaMetadataLookup};
pub(crate) use common::{
    ensure_personal_file_scope, if_none_match_matches, if_none_match_matches_value,
    inline_sandbox_csp, requires_inline_sandbox,
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
pub(crate) use resource_handle::{FileResourcePathSet, resolve_file_resource_handle};
pub use thumbnail::{ImagePreviewResult, ThumbnailResult, get_thumbnail_data};
pub(crate) use thumbnail::{
    get_image_preview_data_in_scope, get_thumbnail_data_in_scope, image_preview_for_file,
};
pub(crate) use transfer::{
    BatchDuplicateFileRecordSpec, BatchDuplicateFileRecordTargetSpec,
    batch_duplicate_file_records_to_mixed_folders_in_scope,
    batch_duplicate_file_records_with_specs_in_scope, copy_file_in_scope,
    duplicate_file_record_in_scope,
};
pub use transfer::{batch_duplicate_file_records, copy_file, duplicate_file_record};

pub(crate) async fn create_empty_in_scope_with_audit(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder_id: Option<i64>,
    name: &str,
    audit_ctx: &AuditContext,
) -> Result<FileInfo> {
    let file = storage::create_empty(state, scope, folder_id, name).await?;
    let details = audit_location_details_for_model(state, scope, &file).await;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::FileCreate,
        crate::services::audit_service::AuditEntityType::File,
        Some(file.id),
        Some(&file.name),
        || details.clone(),
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
    let file = get_info_in_scope(state, scope, file_id).await?;
    let details = audit_location_details_for_model(state, scope, &file).await;
    delete_in_scope(state, scope, file_id).await?;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::FileDelete,
        crate::services::audit_service::AuditEntityType::File,
        Some(file_id),
        Some(&file.name),
        || details.clone(),
    )
    .await;
    Ok(())
}

pub(crate) async fn update_in_scope_with_audit(
    state: &impl StorageChangeRuntimeState,
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
    let previous_file = get_info_in_scope(state, scope, file_id).await?;
    let file = update_in_scope(state, scope, file_id, name, folder_id).await?;
    let details = audit_transfer_details_for_models(state, scope, &previous_file, &file).await;
    audit_service::log_with_details(
        state,
        audit_ctx,
        action,
        crate::services::audit_service::AuditEntityType::File,
        Some(file.id),
        Some(&file.name),
        || details.clone(),
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
    let details = audit_location_details_for_model(state, scope, &file).await;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::FileEdit,
        crate::services::audit_service::AuditEntityType::File,
        Some(file.id),
        Some(&file.name),
        || details.clone(),
    )
    .await;
    Ok((file.into(), new_hash))
}

pub(crate) async fn set_lock_in_scope_with_audit(
    state: &impl StorageChangeRuntimeState,
    scope: WorkspaceStorageScope,
    file_id: i64,
    locked: bool,
    audit_ctx: &AuditContext,
) -> Result<FileInfo> {
    let file = set_lock_in_scope(state, scope, file_id, locked).await?;
    let details = audit_location_details_for_model(state, scope, &file).await;
    audit_service::log_with_details(
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
        || details.clone(),
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
    let source_file = get_info_in_scope(state, scope, file_id).await?;
    let file = copy_file_in_scope(state, scope, file_id, target_folder_id).await?;
    let details = audit_transfer_details_for_models(state, scope, &source_file, &file).await;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::FileCopy,
        crate::services::audit_service::AuditEntityType::File,
        Some(file.id),
        Some(&file.name),
        || details.clone(),
    )
    .await;
    Ok(file.into())
}

pub(crate) async fn download_in_scope_with_file_and_audit(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file: crate::entities::file::Model,
    disposition: DownloadDisposition,
    if_none_match: Option<&str>,
    range: Option<download::range::ResolvedDownloadRange>,
    audit_ctx: &AuditContext,
) -> Result<DownloadOutcome> {
    let file_id = file.id;
    let entity_name = file.name.clone();
    let details = audit_location_details_for_model(state, scope, &file).await;
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
            disposition,
        ),
    )
    .await?;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::FileDownload,
        crate::services::audit_service::AuditEntityType::File,
        Some(file_id),
        Some(&entity_name),
        || details.clone(),
    )
    .await;
    Ok(outcome)
}

pub(crate) async fn audit_location_details_for_model(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    file: &file::Model,
) -> Option<serde_json::Value> {
    match file_path_for_audit(state, file).await {
        Ok(path) => Some(json!({
            "folder_id": file.folder_id,
            "path": path,
            "team_id": scope_team_id(scope),
        })),
        Err(error) => {
            tracing::warn!(
                file_id = file.id,
                "failed to build file audit location details: {error}"
            );
            None
        }
    }
}

pub(crate) async fn audit_transfer_details_for_models(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    source_file: &file::Model,
    target_file: &file::Model,
) -> Option<serde_json::Value> {
    let source_path = match file_path_for_audit(state, source_file).await {
        Ok(path) => path,
        Err(error) => {
            tracing::warn!(
                file_id = source_file.id,
                "failed to build source file audit path: {error}"
            );
            return None;
        }
    };
    let target_path = match file_path_for_audit(state, target_file).await {
        Ok(path) => path,
        Err(error) => {
            tracing::warn!(
                file_id = target_file.id,
                "failed to build target file audit path: {error}"
            );
            return None;
        }
    };
    Some(json!({
        "source_folder_id": source_file.folder_id,
        "source_path": source_path,
        "target_folder_id": target_file.folder_id,
        "target_path": target_path,
        "previous_name": source_file.name,
        "next_name": target_file.name,
        "team_id": scope_team_id(scope),
    }))
}

async fn file_path_for_audit(
    state: &impl SharedRuntimeState,
    file: &file::Model,
) -> Result<String> {
    let parent_path = folder_path_for_audit(state, file.folder_id).await?;
    Ok(join_audit_path(&parent_path, &file.name))
}

async fn folder_path_for_audit(
    state: &impl SharedRuntimeState,
    folder_id: Option<i64>,
) -> Result<String> {
    let Some(folder_id) = folder_id else {
        return Ok("/".to_string());
    };
    let mut paths =
        crate::services::files::folder::build_folder_paths(state.reader_db(), &[folder_id]).await?;
    paths
        .remove(&folder_id)
        .ok_or_else(|| AsterError::record_not_found(format!("folder #{folder_id} audit path")))
}

fn join_audit_path(parent_path: &str, name: &str) -> String {
    if parent_path == "/" {
        format!("/{name}")
    } else {
        format!("{parent_path}/{name}")
    }
}

fn scope_team_id(scope: WorkspaceStorageScope) -> Option<i64> {
    match scope {
        WorkspaceStorageScope::Personal { .. } => None,
        WorkspaceStorageScope::Team { team_id, .. } => Some(team_id),
    }
}

pub fn record_download_metric(
    state: &impl SharedRuntimeState,
    source: &'static str,
    outcome: &DownloadOutcome,
) {
    state
        .metrics()
        .record_file_download(source, outcome.metrics_outcome(), outcome.has_range());
}

pub fn record_download_failure_metric(state: &impl SharedRuntimeState, source: &'static str) {
    state
        .metrics()
        .record_file_download(source, "failure", false);
}

pub fn record_download_failure_metric_with_reason(
    state: &impl SharedRuntimeState,
    source: &'static str,
    reason: &'static str,
    has_range: bool,
) {
    let outcome = format!("failure:{reason}");
    state
        .metrics()
        .record_file_download(source, outcome.as_str(), has_range);
}

pub async fn record_download_result<Fut>(
    state: &impl SharedRuntimeState,
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

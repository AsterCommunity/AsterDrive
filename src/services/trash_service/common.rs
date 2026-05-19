//! 回收站服务子模块：`common`。

use std::collections::{BTreeSet, HashMap};

use sea_orm::DatabaseConnection;

use crate::db::repository::{file_repo, folder_repo, property_repo, share_repo};
use crate::entities::{file, folder};
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::services::{
    file_service, folder_service, storage_change_service,
    workspace_storage_service::{self, WorkspaceResourceScope, WorkspaceStorageScope},
};
use crate::types::EntityType;

use super::DEFAULT_RETENTION_DAYS;
use super::models::{TrashFileItem, TrashFolderItem};

pub fn load_retention_days(state: &PrimaryAppState) -> i64 {
    state
        .runtime_config
        .get_i64("trash_retention_days")
        .unwrap_or_else(|| {
            if let Some(raw) = state.runtime_config.get("trash_retention_days") {
                tracing::warn!(
                    "invalid trash_retention_days value '{}', using default",
                    raw
                );
            }
            DEFAULT_RETENTION_DAYS
        })
}

pub(super) async fn build_trash_path_cache(
    db: &DatabaseConnection,
    folders: &[folder::Model],
    files: &[file::Model],
) -> Result<HashMap<i64, String>> {
    let folder_ids: Vec<i64> = folders
        .iter()
        .filter_map(|folder| folder.parent_id)
        .chain(files.iter().filter_map(|file| file.folder_id))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    folder_service::build_folder_paths(db, &folder_ids).await
}

pub(super) fn build_trash_file_item(
    file: file::Model,
    folder_paths: &HashMap<i64, String>,
    retention_days: i64,
) -> Result<TrashFileItem> {
    let deleted_at = file
        .deleted_at
        .ok_or_else(|| AsterError::validation_error("file is not in trash"))?;
    Ok(TrashFileItem {
        id: file.id,
        name: file.name,
        size: file.size,
        mime_type: file.mime_type,
        created_at: file.created_at,
        updated_at: file.updated_at,
        expires_at: deleted_at + chrono::Duration::days(retention_days),
        is_locked: file.is_locked,
        original_path: resolve_folder_path(folder_paths, file.folder_id)?,
    })
}

pub(super) fn build_trash_folder_item(
    folder: folder::Model,
    folder_paths: &HashMap<i64, String>,
    retention_days: i64,
) -> Result<TrashFolderItem> {
    let deleted_at = folder
        .deleted_at
        .ok_or_else(|| AsterError::validation_error("folder is not in trash"))?;
    Ok(TrashFolderItem {
        id: folder.id,
        name: folder.name,
        created_at: folder.created_at,
        updated_at: folder.updated_at,
        expires_at: deleted_at + chrono::Duration::days(retention_days),
        is_locked: folder.is_locked,
        original_path: resolve_folder_path(folder_paths, folder.parent_id)?,
    })
}

fn resolve_folder_path(
    folder_paths: &HashMap<i64, String>,
    folder_id: Option<i64>,
) -> Result<String> {
    match folder_id {
        Some(folder_id) => folder_paths
            .get(&folder_id)
            .cloned()
            .ok_or_else(|| AsterError::record_not_found(format!("folder #{folder_id}"))),
        None => Ok("/".to_string()),
    }
}

pub(super) fn parent_restore_target_unavailable(
    parent_result: &Result<folder::Model>,
    scope: WorkspaceStorageScope,
) -> Result<bool> {
    match parent_result {
        Ok(parent) => match workspace_storage_service::ensure_folder_scope(parent, scope) {
            Ok(()) => Ok(parent.deleted_at.is_some()),
            Err(AsterError::AuthForbidden(_))
            | Err(AsterError::RecordNotFound(_))
            | Err(AsterError::FileNotFound(_))
            | Err(AsterError::FolderNotFound(_)) => Ok(true),
            Err(error) => Err(error),
        },
        Err(AsterError::AuthForbidden(_))
        | Err(AsterError::RecordNotFound(_))
        | Err(AsterError::FileNotFound(_))
        | Err(AsterError::FolderNotFound(_)) => Ok(true),
        Err(error) => Err(error.clone()),
    }
}

pub(super) async fn verify_file_in_trash_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_id: i64,
) -> Result<file::Model> {
    workspace_storage_service::require_scope_access(state, scope).await?;
    let file = file_repo::find_by_id(&state.db, file_id).await?;
    workspace_storage_service::ensure_file_scope(&file, scope)?;
    if file.deleted_at.is_none() {
        return Err(AsterError::validation_error("file is not in trash"));
    }
    Ok(file)
}

pub(super) async fn verify_folder_in_trash_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder_id: i64,
) -> Result<folder::Model> {
    workspace_storage_service::require_scope_access(state, scope).await?;
    let folder = folder_repo::find_by_id(&state.db, folder_id).await?;
    workspace_storage_service::ensure_folder_scope(&folder, scope)?;
    if folder.deleted_at.is_none() {
        return Err(AsterError::validation_error("folder is not in trash"));
    }
    Ok(folder)
}

pub(super) async fn purge_folder_tree_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder_id: i64,
) -> Result<()> {
    purge_folder_forest_in_scope(state, scope, &[folder_id]).await
}

pub(super) async fn purge_folder_tree_in_resource_scope(
    state: &PrimaryAppState,
    scope: WorkspaceResourceScope,
    folder_id: i64,
) -> Result<()> {
    purge_folder_forest_in_resource_scope(state, scope, &[folder_id], true)
        .await
        .map(|_| ())
}

pub(super) async fn purge_folder_forest_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder_ids: &[i64],
) -> Result<()> {
    purge_folder_forest_in_resource_scope(state, scope.into(), folder_ids, true)
        .await
        .map(|_| ())
}

pub(super) async fn purge_folder_forest_in_scope_silent(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder_ids: &[i64],
) -> Result<FolderPurgeSummary> {
    purge_folder_forest_in_resource_scope(state, scope.into(), folder_ids, false).await
}

#[derive(Debug, Default)]
pub(super) struct FolderPurgeSummary {
    pub purged: u32,
    pub freed_bytes: i64,
}

pub(super) async fn purge_folder_forest_in_resource_scope(
    state: &PrimaryAppState,
    scope: WorkspaceResourceScope,
    folder_ids: &[i64],
    emit_storage_event: bool,
) -> Result<FolderPurgeSummary> {
    if folder_ids.is_empty() {
        return Ok(FolderPurgeSummary::default());
    }

    tracing::debug!(
        scope = ?scope,
        root_folder_count = folder_ids.len(),
        "purging folder forest permanently"
    );
    let root_folders = folder_repo::find_by_ids(&state.db, folder_ids).await?;
    let root_parent_ids: HashMap<i64, Option<i64>> = root_folders
        .iter()
        .map(|folder| (folder.id, folder.parent_id))
        .collect();
    let parent_ids: Vec<Option<i64>> = folder_ids
        .iter()
        .map(|folder_id| root_parent_ids.get(folder_id).copied().flatten())
        .collect();
    let (all_files, all_folder_ids) =
        folder_service::collect_folder_forest_in_resource_scope(&state.db, scope, folder_ids, true)
            .await?;
    let file_count = all_files.len();
    let folder_count = all_folder_ids.len();
    let file_summary =
        file_service::batch_purge_in_resource_scope_silent(state, scope, all_files).await?;
    property_repo::delete_all_for_entities(&state.db, EntityType::Folder, &all_folder_ids).await?;
    let deleted_shares = share_repo::delete_by_folder_ids(&state.db, &all_folder_ids).await?;
    if deleted_shares > 0 {
        crate::services::share_service::invalidate_active_share_target_cache_for_resource_scope(
            state, scope,
        )
        .await;
        crate::services::share_service::invalidate_all_share_token_record_cache(state).await;
    }
    crate::services::folder_service::invalidate_folder_path_cache(state).await;
    folder_repo::delete_many(&state.db, &all_folder_ids).await?;
    if emit_storage_event {
        storage_change_service::publish(
            state,
            storage_change_service::StorageChangeEvent::new_for_resource_scope(
                storage_change_service::StorageChangeKind::FolderPurged,
                scope,
                file_summary.file_ids.clone(),
                folder_ids.to_vec(),
                parent_ids.clone(),
            )
            .with_storage_delta(-file_summary.freed_bytes),
        );
    }
    tracing::debug!(
        scope = ?scope,
        root_folder_count = folder_ids.len(),
        file_count,
        folder_count,
        deleted_shares,
        "purged folder forest permanently"
    );
    Ok(FolderPurgeSummary {
        purged: crate::utils::numbers::usize_to_u32(folder_ids.len(), "purged folder count")?,
        freed_bytes: file_summary.freed_bytes,
    })
}

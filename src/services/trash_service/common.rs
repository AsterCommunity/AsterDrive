//! 回收站服务子模块：`common`。

use std::collections::{BTreeSet, HashMap};

use sea_orm::ConnectionTrait;

use crate::db::repository::{file_repo, folder_repo, property_repo, share_repo};
use crate::entities::{file, folder};
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::{
    files::{file as file_ops, folder as folder_ops},
    storage_change_service,
    workspace_storage_service::{self, WorkspaceResourceScope, WorkspaceStorageScope},
};
use crate::types::EntityType;

use super::DEFAULT_RETENTION_DAYS;
use super::models::{TrashFileItem, TrashFolderItem};

const FOLDER_PURGED_EVENT_FILE_IDS_LIMIT: usize = 1000;

pub fn load_retention_days(state: &impl SharedRuntimeState) -> i64 {
    state
        .runtime_config()
        .get_i64("trash_retention_days")
        .unwrap_or_else(|| {
            if let Some(raw) = state.runtime_config().get("trash_retention_days") {
                tracing::warn!(
                    "invalid trash_retention_days value '{}', using default",
                    raw
                );
            }
            DEFAULT_RETENTION_DAYS
        })
}

pub(super) async fn build_trash_path_cache<C: ConnectionTrait>(
    db: &C,
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

    folder_ops::build_folder_paths(db, &folder_ids).await
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
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    file_id: i64,
) -> Result<file::Model> {
    workspace_storage_service::require_scope_access_with_db(state, state.writer_db(), scope)
        .await?;
    let file = file_repo::find_by_id(state.writer_db(), file_id).await?;
    workspace_storage_service::ensure_file_scope(&file, scope)?;
    if file.deleted_at.is_none() {
        return Err(AsterError::validation_error("file is not in trash"));
    }
    Ok(file)
}

pub(super) async fn verify_folder_in_trash_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    folder_id: i64,
) -> Result<folder::Model> {
    workspace_storage_service::require_scope_access_with_db(state, state.writer_db(), scope)
        .await?;
    let folder = folder_repo::find_by_id(state.writer_db(), folder_id).await?;
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

fn build_folder_purged_storage_event(
    scope: WorkspaceResourceScope,
    file_ids: &[i64],
    folder_ids: Vec<i64>,
    parent_ids: Vec<Option<i64>>,
    freed_bytes: i64,
) -> storage_change_service::StorageChangeEvent {
    let (event_kind, event_file_ids) = if file_ids.len() > FOLDER_PURGED_EVENT_FILE_IDS_LIMIT {
        (
            storage_change_service::StorageChangeKind::SyncRequired,
            Vec::new(),
        )
    } else {
        (
            storage_change_service::StorageChangeKind::FolderPurged,
            file_ids.to_vec(),
        )
    };

    storage_change_service::StorageChangeEvent::new_for_resource_scope(
        event_kind,
        scope,
        event_file_ids,
        folder_ids,
        parent_ids,
    )
    .with_storage_delta(-freed_bytes)
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
    let root_folders = folder_repo::find_by_ids(state.writer_db(), folder_ids).await?;
    let root_parent_ids: HashMap<i64, Option<i64>> = root_folders
        .iter()
        .map(|folder| (folder.id, folder.parent_id))
        .collect();
    let parent_ids: Vec<Option<i64>> = folder_ids
        .iter()
        .map(|folder_id| root_parent_ids.get(folder_id).copied().flatten())
        .collect();
    let (all_files, all_folder_ids) = folder_ops::collect_folder_forest_in_resource_scope(
        state.writer_db(),
        scope,
        folder_ids,
        true,
    )
    .await?;
    let file_count = all_files.len();
    let folder_count = all_folder_ids.len();
    let file_summary =
        file_ops::batch_purge_in_resource_scope_silent(state, scope, all_files).await?;
    property_repo::delete_all_for_entities(state.writer_db(), EntityType::Folder, &all_folder_ids)
        .await?;
    let deleted_shares =
        share_repo::delete_by_folder_ids(state.writer_db(), &all_folder_ids).await?;
    if deleted_shares > 0 {
        crate::services::share_service::invalidate_active_share_target_cache_for_resource_scope(
            state, scope,
        )
        .await;
        crate::services::share_service::invalidate_all_share_token_record_cache(state).await;
    }
    crate::services::files::folder::invalidate_folder_path_cache_for_ids(state, &all_folder_ids)
        .await;
    folder_repo::delete_many(state.writer_db(), &all_folder_ids).await?;
    if emit_storage_event {
        storage_change_service::publish(
            state,
            build_folder_purged_storage_event(
                scope,
                &file_summary.file_ids,
                folder_ids.to_vec(),
                parent_ids.clone(),
                file_summary.freed_bytes,
            ),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::storage_change_service::StorageChangeKind;

    fn make_file_ids(count: usize) -> Vec<i64> {
        (0..count)
            .map(|id| crate::utils::numbers::usize_to_i64(id, "test file id").unwrap())
            .collect()
    }

    #[test]
    fn folder_purged_storage_event_keeps_file_ids_at_limit() {
        let file_ids = make_file_ids(FOLDER_PURGED_EVENT_FILE_IDS_LIMIT);

        let event = build_folder_purged_storage_event(
            WorkspaceResourceScope::Personal { user_id: 1 },
            &file_ids,
            vec![10],
            vec![Some(2)],
            512,
        );

        assert_eq!(event.kind, StorageChangeKind::FolderPurged);
        assert_eq!(event.file_ids, file_ids);
        assert_eq!(event.folder_ids, vec![10]);
        assert_eq!(event.affected_parent_ids, vec![2]);
        assert_eq!(event.storage_delta, Some(-512));
        assert!(event.affects_quota);
    }

    #[test]
    fn folder_purged_storage_event_degrades_above_file_id_limit() {
        let file_ids = make_file_ids(FOLDER_PURGED_EVENT_FILE_IDS_LIMIT + 1);

        let event = build_folder_purged_storage_event(
            WorkspaceResourceScope::Personal { user_id: 1 },
            &file_ids,
            vec![10],
            vec![Some(2)],
            512,
        );

        assert_eq!(event.kind, StorageChangeKind::SyncRequired);
        assert!(event.file_ids.is_empty());
        assert_eq!(event.folder_ids, vec![10]);
        assert_eq!(event.affected_parent_ids, vec![2]);
        assert_eq!(event.storage_delta, Some(-512));
        assert!(event.affects_quota);
    }
}

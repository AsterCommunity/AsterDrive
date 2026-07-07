//! 批量操作服务子模块：`delete`。

use std::collections::HashSet;

use chrono::Utc;

use crate::db::repository::{file_repo, folder_repo};
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::{
    files::folder,
    storage_change_service,
    workspace::storage::{self, WorkspaceStorageScope},
};

use super::{BatchResult, NormalizedSelection, load_normalized_selection_in_scope};

pub(crate) async fn batch_delete_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_ids: &[i64],
    folder_ids: &[i64],
) -> Result<BatchResult> {
    tracing::debug!(
        scope = ?scope,
        requested_file_count = file_ids.len(),
        requested_folder_count = folder_ids.len(),
        "batch soft deleting workspace entries"
    );
    let mut result = BatchResult::new();
    let NormalizedSelection {
        file_ids: normalized_file_ids,
        folder_ids: normalized_folder_ids,
        file_map,
        folder_map,
    } = load_normalized_selection_in_scope(state, scope, file_ids, folder_ids).await?;

    let mut file_ids_to_delete = HashSet::new();
    let mut root_folder_ids_to_delete = Vec::new();
    let mut queued_root_folders = HashSet::new();

    for &id in &normalized_file_ids {
        let Some(file) = file_map.get(&id) else {
            result.record_failure(
                "file",
                id,
                AsterError::file_not_found(format!("file #{id}")).to_string(),
            );
            continue;
        };
        if let Err(err) = storage::ensure_active_file_scope(file, scope) {
            result.record_failure("file", id, err.to_string());
            continue;
        }
        if file.is_locked {
            result.record_failure(
                "file",
                id,
                AsterError::resource_locked("file is locked").to_string(),
            );
            continue;
        }
        result.record_success();
        file_ids_to_delete.insert(id);
    }

    for &id in &normalized_folder_ids {
        let Some(folder) = folder_map.get(&id) else {
            result.record_failure(
                "folder",
                id,
                AsterError::record_not_found(format!("folder #{id}")).to_string(),
            );
            continue;
        };
        if let Err(err) = storage::ensure_active_folder_scope(folder, scope) {
            result.record_failure("folder", id, err.to_string());
            continue;
        }
        if folder.is_locked {
            result.record_failure(
                "folder",
                id,
                AsterError::resource_locked("folder is locked").to_string(),
            );
            continue;
        }
        result.record_success();
        if queued_root_folders.insert(id) {
            root_folder_ids_to_delete.push(id);
        }
    }

    let mut folder_ids_to_delete = Vec::new();
    let direct_file_ids_deleted: Vec<i64> = file_ids_to_delete.iter().copied().collect();
    let file_parent_ids: Vec<Option<i64>> = direct_file_ids_deleted
        .iter()
        .map(|id| file_map.get(id).map(|file| file.folder_id).unwrap_or(None))
        .collect();
    let folder_parent_ids: Vec<Option<i64>> = root_folder_ids_to_delete
        .iter()
        .map(|id| {
            folder_map
                .get(id)
                .map(|folder| folder.parent_id)
                .unwrap_or(None)
        })
        .collect();
    if !root_folder_ids_to_delete.is_empty() {
        let (tree_files, tree_folder_ids) = folder::collect_folder_forest_in_scope(
            state.writer_db(),
            scope,
            &root_folder_ids_to_delete,
            false,
        )
        .await?;
        file_ids_to_delete.extend(tree_files.into_iter().map(|file| file.id));
        folder_ids_to_delete = tree_folder_ids;
    }

    if !file_ids_to_delete.is_empty() || !folder_ids_to_delete.is_empty() {
        let now = Utc::now();
        let file_ids_to_delete: Vec<i64> = file_ids_to_delete.into_iter().collect();
        let direct_file_count = direct_file_ids_deleted.len();
        let root_folder_count = root_folder_ids_to_delete.len();
        let total_file_count = file_ids_to_delete.len();
        let total_folder_count = folder_ids_to_delete.len();

        let txn = crate::db::transaction::begin(state.writer_db()).await?;
        file_repo::soft_delete_many(&txn, &file_ids_to_delete, now).await?;
        folder_repo::soft_delete_many(&txn, &folder_ids_to_delete, now).await?;
        crate::db::transaction::commit(txn).await?;

        if !direct_file_ids_deleted.is_empty() {
            storage_change_service::publish(
                state,
                storage_change_service::StorageChangeEvent::new(
                    storage_change_service::StorageChangeKind::FileTrashed,
                    scope,
                    direct_file_ids_deleted,
                    vec![],
                    file_parent_ids,
                ),
            );
        }
        if !root_folder_ids_to_delete.is_empty() {
            storage_change_service::publish(
                state,
                storage_change_service::StorageChangeEvent::new(
                    storage_change_service::StorageChangeKind::FolderTrashed,
                    scope,
                    vec![],
                    root_folder_ids_to_delete,
                    folder_parent_ids,
                ),
            );
        }
        tracing::debug!(
            scope = ?scope,
            direct_file_count,
            root_folder_count,
            total_file_count,
            total_folder_count,
            succeeded = result.succeeded,
            failed = result.failed,
            "batch soft deleted workspace entries"
        );
    } else {
        tracing::debug!(
            scope = ?scope,
            succeeded = result.succeeded,
            failed = result.failed,
            "batch soft delete completed without database changes"
        );
    }

    Ok(result)
}

/// 批量删除（软删除 -> 回收站）
pub async fn batch_delete(
    state: &PrimaryAppState,
    user_id: i64,
    file_ids: &[i64],
    folder_ids: &[i64],
) -> Result<BatchResult> {
    batch_delete_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        file_ids,
        folder_ids,
    )
    .await
}

/// 团队空间批量删除（软删除 -> 回收站）
pub async fn batch_delete_team(
    state: &PrimaryAppState,
    team_id: i64,
    user_id: i64,
    file_ids: &[i64],
    folder_ids: &[i64],
) -> Result<BatchResult> {
    batch_delete_in_scope(
        state,
        WorkspaceStorageScope::Team {
            team_id,
            actor_user_id: user_id,
        },
        file_ids,
        folder_ids,
    )
    .await
}

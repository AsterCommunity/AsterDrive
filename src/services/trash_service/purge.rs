//! 回收站服务子模块：`purge`。

use super::PURGE_ALL_BATCH_SIZE;
use super::common::{
    FolderPurgeSummary, purge_folder_forest_in_scope_silent, purge_folder_tree_in_scope,
    verify_file_in_trash_in_scope, verify_folder_in_trash_in_scope,
};
use crate::db::repository::{file_repo, folder_repo};
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::{
    file_service, storage_change_service,
    workspace_storage_service::{self, WorkspaceStorageScope},
};

/// 永久删除单个文件
pub async fn purge_file(state: &PrimaryAppState, id: i64, user_id: i64) -> Result<()> {
    let scope = WorkspaceStorageScope::Personal { user_id };
    tracing::debug!(scope = ?scope, file_id = id, "purging file from trash");
    let file = verify_file_in_trash_in_scope(state, scope, id).await?;
    file_service::batch_purge_in_scope(state, scope, vec![file]).await?;
    tracing::debug!(scope = ?scope, file_id = id, "purged file from trash");
    Ok(())
}

pub async fn purge_team_file(
    state: &PrimaryAppState,
    team_id: i64,
    id: i64,
    user_id: i64,
) -> Result<()> {
    let scope = WorkspaceStorageScope::Team {
        team_id,
        actor_user_id: user_id,
    };
    tracing::debug!(scope = ?scope, file_id = id, "purging file from trash");
    let file = verify_file_in_trash_in_scope(state, scope, id).await?;
    file_service::batch_purge_in_scope(state, scope, vec![file]).await?;
    tracing::debug!(scope = ?scope, file_id = id, "purged file from trash");
    Ok(())
}

/// 永久删除单个文件夹（递归）
pub async fn purge_folder(state: &PrimaryAppState, id: i64, user_id: i64) -> Result<()> {
    let scope = WorkspaceStorageScope::Personal { user_id };
    tracing::debug!(scope = ?scope, folder_id = id, "purging folder from trash");
    verify_folder_in_trash_in_scope(state, scope, id).await?;
    purge_folder_tree_in_scope(state, scope, id).await?;
    tracing::debug!(scope = ?scope, folder_id = id, "purged folder from trash");
    Ok(())
}

pub async fn purge_team_folder(
    state: &PrimaryAppState,
    team_id: i64,
    id: i64,
    user_id: i64,
) -> Result<()> {
    let scope = WorkspaceStorageScope::Team {
        team_id,
        actor_user_id: user_id,
    };
    tracing::debug!(scope = ?scope, folder_id = id, "purging folder from trash");
    verify_folder_in_trash_in_scope(state, scope, id).await?;
    purge_folder_tree_in_scope(state, scope, id).await?;
    tracing::debug!(scope = ?scope, folder_id = id, "purged folder from trash");
    Ok(())
}

pub(crate) async fn purge_all_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
) -> Result<u32> {
    let summary = purge_all_in_scope_silent(state, scope).await?;
    publish_purge_all_storage_change(state, scope, &summary);
    Ok(summary.purged)
}

#[derive(Debug, Default)]
pub(crate) struct PurgeAllSummary {
    pub purged: u32,
    freed_bytes: i64,
}

impl PurgeAllSummary {
    fn add_folder_summary(&mut self, summary: FolderPurgeSummary) {
        self.purged += summary.purged;
        self.freed_bytes += summary.freed_bytes;
    }

    fn add_file_summary(&mut self, summary: file_service::BatchPurgeSummary) {
        self.purged += summary.purged;
        self.freed_bytes += summary.freed_bytes;
    }
}

pub(crate) async fn purge_all_in_scope_silent(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
) -> Result<PurgeAllSummary> {
    tracing::debug!(scope = ?scope, "purging all trash contents");
    workspace_storage_service::require_scope_access(state, scope).await?;
    let mut summary = PurgeAllSummary::default();

    let mut folder_cursor: Option<(chrono::DateTime<chrono::Utc>, i64)> = None;
    loop {
        let (top_folders, _) = match scope {
            WorkspaceStorageScope::Personal { user_id } => {
                folder_repo::find_top_level_deleted_cursor(
                    &state.db,
                    user_id,
                    PURGE_ALL_BATCH_SIZE,
                    folder_cursor,
                )
                .await?
            }
            WorkspaceStorageScope::Team { team_id, .. } => {
                folder_repo::find_top_level_deleted_by_team_cursor(
                    &state.db,
                    team_id,
                    PURGE_ALL_BATCH_SIZE,
                    folder_cursor,
                )
                .await?
            }
        };
        if top_folders.is_empty() {
            break;
        }

        folder_cursor = top_folders
            .last()
            .and_then(|folder| folder.deleted_at.map(|deleted_at| (deleted_at, folder.id)));
        let folder_ids: Vec<i64> = top_folders.into_iter().map(|folder| folder.id).collect();
        match purge_folder_forest_in_scope_silent(state, scope, &folder_ids).await {
            Ok(folder_summary) => summary.add_folder_summary(folder_summary),
            Err(error) => {
                tracing::warn!(
                    folder_ids = ?folder_ids,
                    "batch purge top-level folders failed, falling back to per-folder purge: {error}"
                );
                for folder_id in folder_ids {
                    match purge_folder_forest_in_scope_silent(state, scope, &[folder_id]).await {
                        Ok(folder_summary) => summary.add_folder_summary(folder_summary),
                        Err(error) => return Err(error),
                    }
                }
            }
        }
    }

    let mut file_cursor = None;
    loop {
        let (top_files, _) = match scope {
            WorkspaceStorageScope::Personal { user_id } => {
                file_repo::find_top_level_deleted_paginated(
                    &state.db,
                    user_id,
                    PURGE_ALL_BATCH_SIZE,
                    file_cursor,
                )
                .await?
            }
            WorkspaceStorageScope::Team { team_id, .. } => {
                file_repo::find_top_level_deleted_by_team_paginated(
                    &state.db,
                    team_id,
                    PURGE_ALL_BATCH_SIZE,
                    file_cursor,
                )
                .await?
            }
        };
        if top_files.is_empty() {
            break;
        }

        let next_file_cursor = top_files
            .last()
            .and_then(|file| file.deleted_at.map(|deleted_at| (deleted_at, file.id)));
        match file_service::batch_purge_in_resource_scope_silent(state, scope.into(), top_files)
            .await
        {
            Ok(file_summary) => {
                summary.add_file_summary(file_summary);
                file_cursor = next_file_cursor;
            }
            Err(error) => return Err(error),
        }
    }

    tracing::debug!(
        scope = ?scope,
        purged_count = summary.purged,
        "purged all trash contents"
    );
    Ok(summary)
}

pub(crate) fn publish_purge_all_storage_change(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    summary: &PurgeAllSummary,
) {
    if summary.purged == 0 && summary.freed_bytes == 0 {
        return;
    }

    storage_change_service::publish(
        state,
        storage_change_service::StorageChangeEvent::new(
            storage_change_service::StorageChangeKind::SyncRequired,
            scope,
            vec![],
            vec![],
            vec![],
        )
        .with_storage_delta(-summary.freed_bytes),
    );
}

/// 清空用户回收站（返回实际成功删除数量）
///
/// 只处理顶层已删除项（文件夹内子项由递归批量清理），
/// 避免同一文件被重复 purge。
pub async fn purge_all(state: &PrimaryAppState, user_id: i64) -> Result<u32> {
    purge_all_in_scope(state, WorkspaceStorageScope::Personal { user_id }).await
}

pub async fn purge_all_team(state: &PrimaryAppState, team_id: i64, user_id: i64) -> Result<u32> {
    purge_all_in_scope(
        state,
        WorkspaceStorageScope::Team {
            team_id,
            actor_user_id: user_id,
        },
    )
    .await
}

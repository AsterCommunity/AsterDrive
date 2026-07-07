//! 回收站服务子模块：`listing`。

use crate::db::repository::{file_repo, folder_repo};
use crate::errors::Result;
use crate::runtime::SharedRuntimeState;
use crate::services::workspace::storage::{self, WorkspaceStorageScope};
use crate::utils::numbers::usize_to_u64;

use super::common::{
    build_trash_file_item, build_trash_folder_item, build_trash_path_cache, load_retention_days,
};
use super::models::{TrashContents, TrashFileCursor};

pub fn expires_cursor_to_deleted_cursor(
    state: &impl SharedRuntimeState,
    expires_at: chrono::DateTime<chrono::Utc>,
    id: i64,
) -> (chrono::DateTime<chrono::Utc>, i64) {
    let retention_days = load_retention_days(state);
    (expires_at - chrono::Duration::days(retention_days), id)
}

async fn list_trash_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    folder_limit: u64,
    folder_offset: u64,
    file_limit: u64,
    file_cursor: Option<(chrono::DateTime<chrono::Utc>, i64)>,
) -> Result<TrashContents> {
    tracing::debug!(
        scope = ?scope,
        folder_limit,
        folder_offset,
        file_limit,
        has_file_cursor = file_cursor.is_some(),
        "listing trash contents"
    );
    storage::require_scope_access(state, scope).await?;
    let retention_days = load_retention_days(state);

    let (raw_folders, folders_total) = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            folder_repo::find_top_level_deleted_paginated(
                state.reader_db(),
                user_id,
                folder_limit,
                folder_offset,
            )
            .await?
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            folder_repo::find_top_level_deleted_by_team_paginated(
                state.reader_db(),
                team_id,
                folder_limit,
                folder_offset,
            )
            .await?
        }
    };

    let (raw_files, files_total) = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            file_repo::find_top_level_deleted_paginated(
                state.reader_db(),
                user_id,
                file_limit,
                file_cursor,
            )
            .await?
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            file_repo::find_top_level_deleted_by_team_paginated(
                state.reader_db(),
                team_id,
                file_limit,
                file_cursor,
            )
            .await?
        }
    };

    let folder_paths = build_trash_path_cache(state.reader_db(), &raw_folders, &raw_files).await?;

    let raw_file_count = usize_to_u64(raw_files.len(), "trash file count")?;
    let next_file_cursor = if file_limit > 0 && raw_file_count == file_limit {
        raw_files.last().and_then(|file| {
            file.deleted_at.map(|deleted_at| TrashFileCursor {
                expires_at: deleted_at + chrono::Duration::days(retention_days),
                id: file.id,
            })
        })
    } else {
        None
    };

    let folders = raw_folders
        .into_iter()
        .map(|folder| build_trash_folder_item(folder, &folder_paths, retention_days))
        .collect::<Result<Vec<_>>>()?;

    let files = raw_files
        .into_iter()
        .map(|file| build_trash_file_item(file, &folder_paths, retention_days))
        .collect::<Result<Vec<_>>>()?;

    let contents = TrashContents {
        folders,
        files,
        folders_total,
        files_total,
        next_file_cursor,
    };
    tracing::debug!(
        scope = ?scope,
        folders_total = contents.folders_total,
        files_total = contents.files_total,
        returned_folders = contents.folders.len(),
        returned_files = contents.files.len(),
        has_next_file_cursor = contents.next_file_cursor.is_some(),
        "listed trash contents"
    );
    Ok(contents)
}

/// 列出用户回收站内容（分页）
pub async fn list_trash(
    state: &impl SharedRuntimeState,
    user_id: i64,
    folder_limit: u64,
    folder_offset: u64,
    file_limit: u64,
    file_cursor: Option<(chrono::DateTime<chrono::Utc>, i64)>,
) -> Result<TrashContents> {
    list_trash_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        folder_limit,
        folder_offset,
        file_limit,
        file_cursor,
    )
    .await
}

pub async fn list_team_trash(
    state: &impl SharedRuntimeState,
    team_id: i64,
    user_id: i64,
    folder_limit: u64,
    folder_offset: u64,
    file_limit: u64,
    file_cursor: Option<(chrono::DateTime<chrono::Utc>, i64)>,
) -> Result<TrashContents> {
    list_trash_in_scope(
        state,
        WorkspaceStorageScope::Team {
            team_id,
            actor_user_id: user_id,
        },
        folder_limit,
        folder_offset,
        file_limit,
        file_cursor,
    )
    .await
}

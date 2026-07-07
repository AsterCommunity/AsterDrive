//! 回收站服务子模块：`cleanup`。

use std::collections::{HashMap, HashSet};

use chrono::{Duration, Utc};

use crate::db::repository::{file_repo, folder_repo};
use crate::entities::{file, folder};
use crate::errors::Result;
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::{files::file as file_ops, workspace_storage_service::WorkspaceResourceScope};

use super::common::{load_retention_days, purge_folder_tree_in_resource_scope};

/// 自动清理过期回收站条目（后台任务调用）
pub async fn cleanup_expired(state: &PrimaryAppState) -> Result<u32> {
    let retention_days = load_retention_days(state);
    let cutoff = Utc::now() - Duration::days(retention_days);
    let mut count: u32 = 0;

    // 清理过期文件（批量）
    let expired_files = file_repo::find_expired_deleted(state.writer_db(), cutoff).await?;
    let mut by_user: HashMap<i64, Vec<file::Model>> = HashMap::new();
    let mut by_team: HashMap<i64, Vec<file::Model>> = HashMap::new();
    for file in expired_files {
        if let Some(team_id) = file.team_id {
            by_team.entry(team_id).or_default().push(file);
        } else if let Some(owner_user_id) = file.owner_user_id {
            by_user.entry(owner_user_id).or_default().push(file);
        } else {
            tracing::warn!(
                file_id = file.id,
                "skipping expired personal file without owner_user_id"
            );
        }
    }

    for (user_id, files) in by_user {
        match file_ops::batch_purge_in_resource_scope(
            state,
            WorkspaceResourceScope::Personal { user_id },
            files,
        )
        .await
        {
            Ok(purged) => count += purged,
            Err(error) => {
                tracing::warn!("trash cleanup expired files for user #{user_id} failed: {error}")
            }
        }
    }

    for (team_id, files) in by_team {
        match file_ops::batch_purge_in_resource_scope(
            state,
            WorkspaceResourceScope::Team { team_id },
            files,
        )
        .await
        {
            Ok(purged) => count += purged,
            Err(error) => {
                tracing::warn!("trash cleanup expired files for team #{team_id} failed: {error}")
            }
        }
    }

    // 清理过期文件夹，只处理顶层，父文件夹会递归覆盖子项。
    let expired_folders = folder_repo::find_expired_deleted(state.writer_db(), cutoff).await?;
    let expired_folder_ids: HashSet<i64> = expired_folders.iter().map(|folder| folder.id).collect();
    let top_level_folders: Vec<&folder::Model> = expired_folders
        .iter()
        .filter(|folder| {
            folder
                .parent_id
                .is_none_or(|parent_id| !expired_folder_ids.contains(&parent_id))
        })
        .collect();

    for folder in top_level_folders {
        let result = if let Some(team_id) = folder.team_id {
            purge_folder_tree_in_resource_scope(
                state,
                WorkspaceResourceScope::Team { team_id },
                folder.id,
            )
            .await
        } else if let Some(owner_user_id) = folder.owner_user_id {
            purge_folder_tree_in_resource_scope(
                state,
                WorkspaceResourceScope::Personal {
                    user_id: owner_user_id,
                },
                folder.id,
            )
            .await
        } else {
            tracing::warn!(
                folder_id = folder.id,
                "skipping expired personal folder without owner_user_id"
            );
            continue;
        };

        match result {
            Ok(()) => count += 1,
            Err(error) => tracing::warn!("trash cleanup folder {} failed: {error}", folder.id),
        }
    }

    if count > 0 {
        tracing::info!("trash cleanup: purged {count} expired items (retention={retention_days}d)");
    }
    Ok(count)
}

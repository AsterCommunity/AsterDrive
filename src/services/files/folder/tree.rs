//! 递归收集目录树。
//!
//! 删除、复制、归档、分享范围校验等流程都会用到“从一组根目录向下收集全部子孙”。
//! 这里把这件事单独抽出来，避免每个业务流程都自己写一套 BFS / scope 过滤逻辑。

use std::collections::HashSet;

use sea_orm::ConnectionTrait;

use crate::db::repository::{file_repo, folder_repo};
use crate::entities::{file, folder};
use crate::errors::Result;
use crate::services::workspace_storage_service::{WorkspaceResourceScope, WorkspaceStorageScope};

fn file_matches_scope(file: &file::Model, scope: WorkspaceResourceScope) -> bool {
    match scope {
        WorkspaceResourceScope::Personal { user_id } => {
            file.team_id.is_none() && file.owner_user_id == Some(user_id)
        }
        WorkspaceResourceScope::Team { team_id } => file.team_id == Some(team_id),
    }
}

fn folder_matches_scope(folder: &folder::Model, scope: WorkspaceResourceScope) -> bool {
    match scope {
        WorkspaceResourceScope::Personal { user_id } => {
            folder.team_id.is_none() && folder.owner_user_id == Some(user_id)
        }
        WorkspaceResourceScope::Team { team_id } => folder.team_id == Some(team_id),
    }
}

pub(crate) async fn collect_folder_forest_in_resource_scope<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceResourceScope,
    root_folder_ids: &[i64],
    include_deleted: bool,
) -> Result<(Vec<file::Model>, Vec<i64>)> {
    if root_folder_ids.is_empty() {
        return Ok((vec![], vec![]));
    }

    let mut files = Vec::new();
    let mut folder_ids = Vec::new();
    let mut seen_folder_ids = HashSet::new();
    let mut frontier = root_folder_ids.to_vec();

    // 这里按“当前层 frontier -> 下一层 children”的方式做 BFS。
    // 相比递归 DFS，更容易批量查询当前层所有 children，减少数据库 round-trip。
    while !frontier.is_empty() {
        frontier.sort_unstable();
        frontier.dedup();
        frontier.retain(|id| seen_folder_ids.insert(*id));
        if frontier.is_empty() {
            break;
        }

        folder_ids.extend(frontier.iter().copied());

        if include_deleted {
            // 带 deleted 节点的场景通常是回收站恢复/清理，不适合走普通 repo 过滤器，
            // 所以先拉全量 children，再在内存里按 scope 过滤。
            files.extend(
                file_repo::find_all_in_folders(db, &frontier)
                    .await?
                    .into_iter()
                    .filter(|file| file_matches_scope(file, scope)),
            );
            frontier = folder_repo::find_all_children_in_parents(db, &frontier)
                .await?
                .into_iter()
                .filter(|folder| folder_matches_scope(folder, scope))
                .map(|folder| folder.id)
                .collect();
            continue;
        }

        frontier = match scope {
            WorkspaceResourceScope::Personal { user_id } => {
                files.extend(file_repo::find_by_folders(db, user_id, &frontier).await?);
                folder_repo::find_children_in_parents(db, user_id, &frontier)
                    .await?
                    .into_iter()
                    .map(|folder| folder.id)
                    .collect()
            }
            WorkspaceResourceScope::Team { team_id } => {
                files.extend(file_repo::find_by_team_folders(db, team_id, &frontier).await?);
                folder_repo::find_team_children_in_parents(db, team_id, &frontier)
                    .await?
                    .into_iter()
                    .map(|folder| folder.id)
                    .collect()
            }
        };
    }

    Ok((files, folder_ids))
}

pub(crate) async fn collect_folder_forest_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    root_folder_ids: &[i64],
    include_deleted: bool,
) -> Result<(Vec<file::Model>, Vec<i64>)> {
    collect_folder_forest_in_resource_scope(db, scope.into(), root_folder_ids, include_deleted)
        .await
}

pub(crate) async fn collect_folder_tree_in_resource_scope<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceResourceScope,
    folder_id: i64,
    include_deleted: bool,
) -> Result<(Vec<file::Model>, Vec<i64>)> {
    collect_folder_forest_in_resource_scope(db, scope, &[folder_id], include_deleted).await
}

pub(crate) async fn collect_folder_tree_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    folder_id: i64,
    include_deleted: bool,
) -> Result<(Vec<file::Model>, Vec<i64>)> {
    collect_folder_tree_in_resource_scope(db, scope.into(), folder_id, include_deleted).await
}

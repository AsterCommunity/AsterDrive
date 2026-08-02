//! 递归收集目录树。
//!
//! 删除、复制、归档、分享范围校验等流程都会用到“从一组根目录向下收集全部子孙”。
//! 这里把这件事单独抽出来，避免每个业务流程都自己写一套 BFS / scope 过滤逻辑。

use std::collections::HashSet;

use sea_orm::ConnectionTrait;

use crate::db::repository::{file_repo, folder_repo};
use crate::errors::{AsterError, Result};
use crate::services::workspace::storage::{WorkspaceResourceScope, WorkspaceStorageScope};
use aster_drive_model::entities::{file, folder};
use aster_forge_utils::numbers::usize_to_u64;

const FOLDER_TREE_FILE_PAGE_SIZE: usize = 512;

/// Optional resource bounds for a product-owned folder-tree traversal.
///
/// Callers that do not receive untrusted recursive work retain the historical unbounded helper.
/// WebDAV passes explicit limits at its actual mutation boundary in addition to protocol preflight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FolderTreeTraversalLimits {
    pub maximum_resources: usize,
    pub maximum_frontier: usize,
    pub maximum_depth: usize,
}

impl FolderTreeTraversalLimits {
    #[must_use]
    pub const fn new(
        maximum_resources: usize,
        maximum_frontier: usize,
        maximum_depth: usize,
    ) -> Self {
        Self {
            maximum_resources,
            maximum_frontier,
            maximum_depth,
        }
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
    limits: Option<FolderTreeTraversalLimits>,
) -> Result<(Vec<file::Model>, Vec<i64>)> {
    if root_folder_ids.is_empty() {
        return Ok((vec![], vec![]));
    }

    let mut files = Vec::new();
    let mut folder_ids = Vec::new();
    let mut seen_folder_ids = HashSet::new();
    let mut frontier = root_folder_ids.to_vec();
    let mut depth = 0usize;

    // 这里按“当前层 frontier -> 下一层 children”的方式做 BFS。
    // 相比递归 DFS，更容易批量查询当前层所有 children，减少数据库 round-trip。
    while !frontier.is_empty() {
        frontier.sort_unstable();
        frontier.dedup();
        frontier.retain(|id| seen_folder_ids.insert(*id));
        if frontier.is_empty() {
            break;
        }

        check_folder_tree_frontier_limits(limits, frontier.len(), depth)?;
        check_folder_tree_resource_limits(limits, folder_ids.len(), files.len(), frontier.len())?;

        folder_ids.extend(frontier.iter().copied());

        let level_files = load_level_files(
            db,
            scope,
            &frontier,
            include_deleted,
            limits,
            folder_ids.len(),
            files.len(),
        )
        .await?;
        extend_level_files(limits, &mut files, folder_ids.len(), level_files)?;

        if include_deleted {
            frontier = folder_repo::find_all_children_in_parents(db, &frontier)
                .await?
                .into_iter()
                .filter(|folder| folder_matches_scope(folder, scope))
                .map(|folder| folder.id)
                .collect();
            depth = depth.checked_add(1).ok_or_else(folder_tree_limit_error)?;
            continue;
        }

        frontier = match scope {
            WorkspaceResourceScope::Personal { user_id } => {
                folder_repo::find_children_in_parents(db, user_id, &frontier)
                    .await?
                    .into_iter()
                    .map(|folder| folder.id)
                    .collect()
            }
            WorkspaceResourceScope::Team { team_id } => {
                folder_repo::find_team_children_in_parents(db, team_id, &frontier)
                    .await?
                    .into_iter()
                    .map(|folder| folder.id)
                    .collect()
            }
        };
        depth = depth.checked_add(1).ok_or_else(folder_tree_limit_error)?;
    }

    Ok((files, folder_ids))
}

async fn load_level_files<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceResourceScope,
    frontier: &[i64],
    include_deleted: bool,
    limits: Option<FolderTreeTraversalLimits>,
    folder_count: usize,
    file_count: usize,
) -> Result<Vec<file::Model>> {
    let page_limit = if let Some(limits) = limits {
        let used = folder_count
            .checked_add(file_count)
            .ok_or_else(folder_tree_limit_error)?;
        let remaining = limits
            .maximum_resources
            .checked_sub(used)
            .ok_or_else(folder_tree_limit_error)?;
        usize_to_u64(remaining, "folder tree remaining resource limit")?
            .checked_add(1)
            .ok_or_else(folder_tree_limit_error)?
    } else {
        usize_to_u64(FOLDER_TREE_FILE_PAGE_SIZE, "folder tree file page size")?
    };

    let mut files = Vec::new();
    let mut after_id = None;
    loop {
        let page = if include_deleted {
            let file_scope = match scope {
                WorkspaceResourceScope::Personal { user_id } => {
                    file_repo::FileScope::Personal { user_id }
                }
                WorkspaceResourceScope::Team { team_id } => file_repo::FileScope::Team { team_id },
            };
            file_repo::find_all_by_folders_after_id_in_scope(
                db, file_scope, frontier, after_id, page_limit,
            )
            .await?
        } else {
            match scope {
                WorkspaceResourceScope::Personal { user_id } => {
                    file_repo::find_by_folders_after_id(db, user_id, frontier, after_id, page_limit)
                        .await?
                }
                WorkspaceResourceScope::Team { team_id } => {
                    file_repo::find_by_team_folders_after_id(
                        db, team_id, frontier, after_id, page_limit,
                    )
                    .await?
                }
            }
        };
        if let Some(limits) = limits {
            if page.len()
                > limits
                    .maximum_resources
                    .saturating_sub(folder_count.saturating_add(file_count))
            {
                return Err(folder_tree_limit_error());
            }
            files.extend(page);
            break;
        }
        let Some(last_id) = page.last().map(|file| file.id) else {
            break;
        };
        let page_len = page.len();
        files.extend(page);
        after_id = Some(last_id);
        if page_len < FOLDER_TREE_FILE_PAGE_SIZE {
            break;
        }
    }
    Ok(files)
}

fn extend_level_files(
    limits: Option<FolderTreeTraversalLimits>,
    files: &mut Vec<file::Model>,
    folder_count: usize,
    level_files: Vec<file::Model>,
) -> Result<()> {
    check_folder_tree_resource_limits(limits, folder_count, files.len(), level_files.len())?;
    files.extend(level_files);
    Ok(())
}

fn check_folder_tree_frontier_limits(
    limits: Option<FolderTreeTraversalLimits>,
    frontier: usize,
    depth: usize,
) -> Result<()> {
    let Some(limits) = limits else {
        return Ok(());
    };
    if depth > limits.maximum_depth || frontier > limits.maximum_frontier {
        return Err(folder_tree_limit_error());
    }
    Ok(())
}

fn check_folder_tree_resource_limits(
    limits: Option<FolderTreeTraversalLimits>,
    folders: usize,
    files: usize,
    additional: usize,
) -> Result<()> {
    let Some(limits) = limits else {
        return Ok(());
    };
    if folders
        .checked_add(files)
        .and_then(|count| count.checked_add(additional))
        .is_none_or(|count| count > limits.maximum_resources)
    {
        return Err(folder_tree_limit_error());
    }
    Ok(())
}

fn folder_tree_limit_error() -> AsterError {
    AsterError::operation_resource_limit_exceeded(
        "recursive folder tree exceeds the operation resource budget",
    )
}

pub(crate) async fn collect_folder_forest_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    root_folder_ids: &[i64],
    include_deleted: bool,
    limits: Option<FolderTreeTraversalLimits>,
) -> Result<(Vec<file::Model>, Vec<i64>)> {
    collect_folder_forest_in_resource_scope(
        db,
        scope.into(),
        root_folder_ids,
        include_deleted,
        limits,
    )
    .await
}

pub(crate) async fn collect_folder_tree_in_resource_scope<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceResourceScope,
    folder_id: i64,
    include_deleted: bool,
    limits: Option<FolderTreeTraversalLimits>,
) -> Result<(Vec<file::Model>, Vec<i64>)> {
    collect_folder_forest_in_resource_scope(db, scope, &[folder_id], include_deleted, limits).await
}

pub(crate) async fn collect_folder_tree_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    folder_id: i64,
    include_deleted: bool,
    limits: Option<FolderTreeTraversalLimits>,
) -> Result<(Vec<file::Model>, Vec<i64>)> {
    collect_folder_tree_in_resource_scope(db, scope.into(), folder_id, include_deleted, limits)
        .await
}

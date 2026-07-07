//! 目录列表查询。
//!
//! 这里的一个关键取舍是：文件夹和文件分别分页。
//! 文件夹用 offset，文件用 cursor，这样目录页可以稳定展示大量文件而不要求
//! 文件夹列表也跟着使用 cursor。

use crate::db::repository::{file_repo, folder_repo};
use crate::errors::Result;
use crate::runtime::SharedRuntimeState;
use crate::services::{
    share_service, tag_service,
    workspace_storage_service::{self, WorkspaceResourceScope, WorkspaceStorageScope},
};
use crate::utils::numbers::usize_to_u64;

use super::{
    FileCursor, FolderContents, build_file_list_items_with_tags, build_folder_list_items_with_tags,
    ensure_personal_folder_scope,
};

#[derive(Debug, Clone)]
pub struct FolderListParams {
    pub(crate) folder_limit: u64,
    pub(crate) folder_offset: u64,
    pub(crate) file_limit: u64,
    pub(crate) file_cursor: Option<(String, i64)>,
    pub(crate) sort_by: crate::api::pagination::SortBy,
    pub(crate) sort_order: crate::api::pagination::SortOrder,
}

impl From<&crate::api::pagination::FolderListQuery> for FolderListParams {
    fn from(query: &crate::api::pagination::FolderListQuery) -> Self {
        Self {
            folder_limit: query.folder_limit(),
            folder_offset: query.folder_offset(),
            file_limit: query.file_limit(),
            file_cursor: query.file_cursor(),
            sort_by: query.sort_by(),
            sort_order: query.sort_order(),
        }
    }
}

struct FolderListingResult {
    folders: Vec<crate::entities::folder::Model>,
    folders_total: u64,
    files: Vec<crate::entities::file::Model>,
    files_total: u64,
}

async fn build_folder_contents(
    state: &impl SharedRuntimeState,
    scope: WorkspaceResourceScope,
    listing: FolderListingResult,
    params: &FolderListParams,
) -> Result<FolderContents> {
    let FolderListingResult {
        folders,
        folders_total,
        files,
        files_total,
    } = listing;
    // 列表接口除了返回文件/目录本身，还要顺手标注“是否已有活跃分享”。
    // 这里一次性批量查 share 状态，避免前端列表页出现 N+1。
    let file_count = usize_to_u64(files.len(), "folder listing file count")?;
    let next_file_cursor = if file_count == params.file_limit && params.file_limit > 0 {
        files.last().map(|f| FileCursor {
            value: crate::api::pagination::SortBy::cursor_value(f, params.sort_by),
            id: f.id,
        })
    } else {
        None
    };

    let file_ids: Vec<i64> = files.iter().map(|file| file.id).collect();
    let folder_ids: Vec<i64> = folders.iter().map(|folder| folder.id).collect();
    let (shared_file_ids, shared_folder_ids) = tokio::try_join!(
        share_service::find_active_file_ids_in_resource_scope(state, scope, &file_ids),
        share_service::find_active_folder_ids_in_resource_scope(state, scope, &folder_ids),
    )?;
    let tags_by_entity =
        tag_service::load_entity_tag_map(state, scope, &file_ids, &folder_ids).await?;

    Ok(FolderContents {
        folders: build_folder_list_items_with_tags(folders, &shared_folder_ids, &tags_by_entity),
        files: build_file_list_items_with_tags(files, &shared_file_ids, &tags_by_entity),
        folders_total,
        files_total,
        next_file_cursor,
    })
}

pub(crate) async fn list_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    parent_id: Option<i64>,
    params: &FolderListParams,
) -> Result<FolderContents> {
    tracing::debug!(
        scope = ?scope,
        parent_id,
        folder_limit = params.folder_limit,
        folder_offset = params.folder_offset,
        file_limit = params.file_limit,
        has_file_cursor = params.file_cursor.is_some(),
        sort_by = ?params.sort_by,
        sort_order = ?params.sort_order,
        "listing folder contents"
    );
    if let WorkspaceStorageScope::Team {
        team_id,
        actor_user_id,
    } = scope
    {
        workspace_storage_service::require_team_access(state, team_id, actor_user_id).await?;
    }

    if let Some(parent_id) = parent_id {
        workspace_storage_service::verify_folder_access_for_read(state, scope, parent_id).await?;
    }

    // 目录和文件分开查询，是因为它们的分页策略不同：
    // folders 走 offset，files 走 cursor；最终再拼成同一个 FolderContents。
    let (folders, folders_total, files, files_total) = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            let folder_task = async {
                if params.folder_limit == 0 {
                    Ok((
                        vec![],
                        folder_repo::find_children_paginated(
                            state.reader_db(),
                            user_id,
                            parent_id,
                            0,
                            0,
                            params.sort_by,
                            params.sort_order,
                        )
                        .await?
                        .1,
                    ))
                } else {
                    folder_repo::find_children_paginated(
                        state.reader_db(),
                        user_id,
                        parent_id,
                        params.folder_limit,
                        params.folder_offset,
                        params.sort_by,
                        params.sort_order,
                    )
                    .await
                }
            };
            let file_task = async {
                if params.file_limit == 0 {
                    Ok((
                        vec![],
                        file_repo::find_by_folder_cursor(
                            state.reader_db(),
                            user_id,
                            parent_id,
                            0,
                            None,
                            params.sort_by,
                            params.sort_order,
                        )
                        .await?
                        .1,
                    ))
                } else {
                    file_repo::find_by_folder_cursor(
                        state.reader_db(),
                        user_id,
                        parent_id,
                        params.file_limit,
                        params.file_cursor.clone(),
                        params.sort_by,
                        params.sort_order,
                    )
                    .await
                }
            };
            let ((folders, folders_total), (files, files_total)) =
                tokio::try_join!(folder_task, file_task)?;

            (folders, folders_total, files, files_total)
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            let folder_task = async {
                if params.folder_limit == 0 {
                    Ok((
                        vec![],
                        folder_repo::find_team_children_paginated(
                            state.reader_db(),
                            team_id,
                            parent_id,
                            0,
                            0,
                            params.sort_by,
                            params.sort_order,
                        )
                        .await?
                        .1,
                    ))
                } else {
                    folder_repo::find_team_children_paginated(
                        state.reader_db(),
                        team_id,
                        parent_id,
                        params.folder_limit,
                        params.folder_offset,
                        params.sort_by,
                        params.sort_order,
                    )
                    .await
                }
            };
            let file_task = async {
                if params.file_limit == 0 {
                    Ok((
                        vec![],
                        file_repo::find_by_team_folder_cursor(
                            state.reader_db(),
                            team_id,
                            parent_id,
                            0,
                            None,
                            params.sort_by,
                            params.sort_order,
                        )
                        .await?
                        .1,
                    ))
                } else {
                    file_repo::find_by_team_folder_cursor(
                        state.reader_db(),
                        team_id,
                        parent_id,
                        params.file_limit,
                        params.file_cursor.clone(),
                        params.sort_by,
                        params.sort_order,
                    )
                    .await
                }
            };
            let ((folders, folders_total), (files, files_total)) =
                tokio::try_join!(folder_task, file_task)?;

            (folders, folders_total, files, files_total)
        }
    };

    let contents = build_folder_contents(
        state,
        scope.into(),
        FolderListingResult {
            folders,
            folders_total,
            files,
            files_total,
        },
        params,
    )
    .await?;
    tracing::debug!(
        scope = ?scope,
        parent_id,
        folders_total = contents.folders_total,
        files_total = contents.files_total,
        returned_folders = contents.folders.len(),
        returned_files = contents.files.len(),
        has_next_file_cursor = contents.next_file_cursor.is_some(),
        "listed folder contents"
    );
    Ok(contents)
}

pub async fn list(
    state: &impl SharedRuntimeState,
    user_id: i64,
    parent_id: Option<i64>,
    params: &FolderListParams,
) -> Result<FolderContents> {
    list_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        parent_id,
        params,
    )
    .await
}

/// 列出文件夹内容（无用户校验，用于分享链接）
pub async fn list_shared(
    state: &impl SharedRuntimeState,
    folder_id: i64,
    params: &FolderListParams,
) -> Result<FolderContents> {
    tracing::debug!(
        folder_id,
        folder_limit = params.folder_limit,
        folder_offset = params.folder_offset,
        file_limit = params.file_limit,
        has_file_cursor = params.file_cursor.is_some(),
        sort_by = ?params.sort_by,
        sort_order = ?params.sort_order,
        "listing shared folder contents"
    );
    let folder = folder_repo::find_by_id(state.reader_db(), folder_id).await?;
    let contents = if let Some(team_id) = folder.team_id {
        // 公开分享页不再校验“当前用户是不是团队成员”，但数据边界仍然是团队空间。
        let (folders, folders_total) = folder_repo::find_team_children_paginated(
            state.reader_db(),
            team_id,
            Some(folder_id),
            params.folder_limit,
            params.folder_offset,
            params.sort_by,
            params.sort_order,
        )
        .await?;
        let (files, files_total) = file_repo::find_by_team_folder_cursor(
            state.reader_db(),
            team_id,
            Some(folder_id),
            params.file_limit,
            params.file_cursor.clone(),
            params.sort_by,
            params.sort_order,
        )
        .await?;

        build_folder_contents(
            state,
            WorkspaceResourceScope::Team { team_id },
            FolderListingResult {
                folders,
                folders_total,
                files,
                files_total,
            },
            params,
        )
        .await?
    } else {
        ensure_personal_folder_scope(&folder)?;
        let owner_user_id = folder.owner_user_id.ok_or_else(|| {
            crate::errors::AsterError::auth_forbidden("folder has no personal owner")
        })?;
        let (folders, folders_total) = folder_repo::find_children_paginated(
            state.reader_db(),
            owner_user_id,
            Some(folder_id),
            params.folder_limit,
            params.folder_offset,
            params.sort_by,
            params.sort_order,
        )
        .await?;
        let (files, files_total) = file_repo::find_by_folder_cursor(
            state.reader_db(),
            owner_user_id,
            Some(folder_id),
            params.file_limit,
            params.file_cursor.clone(),
            params.sort_by,
            params.sort_order,
        )
        .await?;

        build_folder_contents(
            state,
            WorkspaceResourceScope::Personal {
                user_id: owner_user_id,
            },
            FolderListingResult {
                folders,
                folders_total,
                files,
                files_total,
            },
            params,
        )
        .await?
    };
    tracing::debug!(
        folder_id,
        folders_total = contents.folders_total,
        files_total = contents.files_total,
        returned_folders = contents.folders.len(),
        returned_files = contents.files.len(),
        has_next_file_cursor = contents.next_file_cursor.is_some(),
        "listed shared folder contents"
    );
    Ok(contents)
}

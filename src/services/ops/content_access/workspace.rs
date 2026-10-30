use super::{denied, load};
use crate::api::api_error_code::ApiErrorCode;
use crate::{
    db::repository::{file_repo, folder_repo},
    errors::Result,
    runtime::SharedRuntimeState,
    services::{
        auth::local::Claims,
        files::folder::{
            self, FolderAncestorItem, FolderContents, FolderListParams, FolderListingResult,
        },
        workspace::scope::{WorkspaceResourceScope, ensure_folder_resource_scope},
    },
};
use serde::Serialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

pub struct AdminWorkspaceQuery {
    pub access_id: String,
    pub folder_id: Option<i64>,
    pub deleted: bool,
    pub pagination: crate::api::pagination::FolderListQuery,
}
#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AdminWorkspaceContents {
    pub contents: FolderContents,
    pub ancestors: Vec<FolderAncestorItem>,
    pub deleted: bool,
}

pub async fn list_workspace(
    state: &impl SharedRuntimeState,
    claims: &Claims,
    query: &AdminWorkspaceQuery,
) -> Result<AdminWorkspaceContents> {
    if query.folder_id.is_some_and(|id| id <= 0)
        || query.pagination.file_after_id.is_some_and(|id| id <= 0)
        || query.pagination.file_after_id.is_some() != query.pagination.file_after_value.is_some()
    {
        return Err(crate::errors::AsterError::validation_error(
            "invalid workspace navigation",
        ));
    }
    let payload = load(state, claims, &query.access_id).await?;
    if payload.download.is_some() {
        return Err(denied(ApiErrorCode::AdminContentContextInvalid));
    }
    let scope = super::require_target(state, &payload).await?;
    let mut ancestors = Vec::new();
    let mut id = query.folder_id;
    let mut seen = std::collections::HashSet::new();
    while let Some(folder_id) = id {
        if !seen.insert(folder_id) || seen.len() > 1024 {
            return Err(crate::errors::AsterError::internal_error(
                "invalid folder ancestry",
            ));
        }
        let item = folder_repo::find_by_id(state.writer_db(), folder_id).await?;
        ensure_folder_resource_scope(&item, scope)?;
        // A deleted folder may be navigated only in the explicit deleted view.
        if item.deleted_at.is_some() && !query.deleted {
            return Err(crate::errors::AsterError::folder_not_found(
                "folder unavailable",
            ));
        }
        id = item.parent_id;
        ancestors.push(FolderAncestorItem {
            id: item.id,
            name: item.name.clone(),
            icon: folder::FolderIcon::from_model(&item),
        });
    }
    ancestors.reverse();
    let params = FolderListParams::from(&query.pagination);
    let (file_scope, folder_scope) = match scope {
        WorkspaceResourceScope::Personal { user_id } => (
            file_repo::FileScope::Personal { user_id },
            folder_repo::FolderScope::Personal { user_id },
        ),
        WorkspaceResourceScope::Team { team_id } => (
            file_repo::FileScope::Team { team_id },
            folder_repo::FolderScope::Team { team_id },
        ),
    };
    let ((folders, folders_total), (files, files_total)) = tokio::try_join!(
        folder_repo::find_children_paginated_in_scope(
            state.writer_db(),
            folder_scope,
            folder_repo::FolderChildrenQuery {
                parent_id: query.folder_id,
                limit: params.folder_limit,
                offset: params.folder_offset,
                sort_by: params.sort_by,
                sort_order: params.sort_order,
                deleted: query.deleted
            }
        ),
        file_repo::find_by_folder_cursor_in_scope(
            state.writer_db(),
            file_scope,
            file_repo::FileCursorQuery {
                folder_id: query.folder_id,
                limit: params.file_limit,
                after: params.file_cursor.clone(),
                sort_by: params.sort_by,
                sort_order: params.sort_order,
                deleted: query.deleted
            }
        ),
    )?;
    let contents = folder::build_folder_contents(
        state,
        scope,
        FolderListingResult {
            folders,
            folders_total,
            files,
            files_total,
        },
        &params,
        true,
    )
    .await?;
    Ok(AdminWorkspaceContents {
        contents,
        ancestors,
        deleted: query.deleted,
    })
}

/// Metadata and revisions remain within the same governed workspace context.
pub async fn get_file(
    state: &impl SharedRuntimeState,
    claims: &Claims,
    access_id: &str,
    file_id: i64,
) -> Result<crate::services::workspace::models::FileInfo> {
    let payload = load(state, claims, access_id).await?;
    if payload.download.is_some() {
        return Err(denied(ApiErrorCode::AdminContentContextInvalid));
    }
    let file = file_repo::find_by_id(state.writer_db(), file_id).await?;

    crate::services::files::file::build_authorized_file_info(
        state,
        super::require_target(state, &payload).await?,
        file,
    )
    .await
}

pub async fn list_versions(
    state: &impl SharedRuntimeState,
    claims: &Claims,
    access_id: &str,
    file_id: i64,
    query: crate::services::workspace::models::FileVersionListQuery,
) -> Result<Vec<crate::services::workspace::models::FileVersion>> {
    let payload = load(state, claims, access_id).await?;
    if payload.download.is_some() {
        return Err(denied(ApiErrorCode::AdminContentContextInvalid));
    }
    let file = file_repo::find_by_id(state.writer_db(), file_id).await?;
    crate::services::workspace::scope::ensure_file_resource_scope(
        &file,
        super::require_target(state, &payload).await?,
    )?;
    crate::services::content::version::list_authorized_versions(state, file_id, query).await
}

/// The target's entire recycle bin, independent of the currently browsed directory.
pub async fn list_trash(
    state: &impl SharedRuntimeState,
    claims: &Claims,
    access_id: &str,
    query: crate::api::pagination::TrashListQuery,
) -> Result<crate::services::files::trash::TrashContents> {
    let payload = load(state, claims, access_id).await?;
    if payload.download.is_some() {
        return Err(denied(ApiErrorCode::AdminContentContextInvalid));
    }
    let scope = super::require_target(state, &payload).await?;
    let has_cursor = query.file_after_expires_at.is_some() || query.file_after_id.is_some();
    if query.file_after_id.is_some_and(|id| id <= 0)
        || (has_cursor && query.file_cursor().is_none())
    {
        return Err(crate::errors::AsterError::validation_error(
            "invalid recycle bin cursor",
        ));
    }
    let cursor = query.file_cursor().map(|(expiry, id)| {
        crate::services::files::trash::expires_cursor_to_deleted_cursor(state, expiry, id)
    });
    crate::services::files::trash::list_authorized_trash(
        state,
        scope,
        query.folder_limit(),
        query.folder_offset(),
        query.file_limit(),
        cursor,
    )
    .await
}

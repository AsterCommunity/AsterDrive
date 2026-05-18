//! 服务模块：`search_service`。

use chrono::DateTime;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::{IntoParams, ToSchema};

use crate::db::repository::search_repo;
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::services::{
    folder_service::{FileListItem, FolderListItem, build_folder_list_items},
    share_service,
    workspace_storage_service::{self, WorkspaceStorageScope},
};
use crate::types::FileCategory;
use crate::utils::file_classification::{parse_extension_filters, parse_file_category};

#[derive(Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
pub struct SearchParams {
    /// Name search pattern (case-insensitive substring match)
    pub q: Option<String>,
    /// Result type filter: "file", "folder", or "all" (default)
    #[serde(rename = "type")]
    pub search_type: Option<String>,
    /// Filter by exact MIME type (e.g. "image/png")
    pub mime_type: Option<String>,
    /// Filter by file category (image, video, audio, document, spreadsheet, presentation, archive, code, other)
    pub category: Option<String>,
    /// Comma-separated file extensions (recommended format), e.g. "pdf,docx,xlsx"
    pub extensions: Option<String>,
    /// Minimum file size in bytes
    pub min_size: Option<i64>,
    /// Maximum file size in bytes
    pub max_size: Option<i64>,
    /// ISO 8601 datetime — only return items created after this time
    pub created_after: Option<String>,
    /// ISO 8601 datetime — only return items created before this time
    pub created_before: Option<String>,
    /// Scope search to a specific folder (folder_id for files, parent_id for folders)
    pub folder_id: Option<i64>,
    /// Max results per type (default 50, max 100)
    pub limit: Option<u64>,
    /// Offset for pagination
    pub offset: Option<u64>,
}

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct SearchResults {
    pub files: Vec<FileListItem>,
    pub folders: Vec<FolderListItem>,
    pub total_files: u64,
    pub total_folders: u64,
}

type SearchDateRange = (
    Option<chrono::DateTime<chrono::Utc>>,
    Option<chrono::DateTime<chrono::Utc>>,
);

#[derive(Debug, Clone)]
struct NormalizedFileFilters {
    category: Option<FileCategory>,
    extensions: Vec<String>,
}

fn build_search_file_list_items(
    files: Vec<search_repo::FileSearchItem>,
    shared_file_ids: &HashSet<i64>,
) -> Vec<FileListItem> {
    files
        .into_iter()
        .map(|file| FileListItem {
            id: file.id,
            name: file.name,
            size: file.size,
            mime_type: file.mime_type,
            extension: file.extension,
            compound_extension: file.compound_extension,
            file_category: file.file_category,
            updated_at: file.updated_at,
            is_locked: file.is_locked,
            is_shared: shared_file_ids.contains(&file.id),
        })
        .collect()
}

fn validate_search_params(params: &SearchParams) -> Result<()> {
    if params.q.is_some() && normalized_query(params).is_none() {
        return Err(AsterError::validation_error(
            "search query must not be empty",
        ));
    }

    if let Some(search_type) = params.search_type.as_deref()
        && !matches!(search_type, "file" | "folder" | "all")
    {
        return Err(AsterError::validation_error(
            "type must be one of: file, folder, all",
        ));
    }

    if let (Some(min), Some(max)) = (params.min_size, params.max_size)
        && min > max
    {
        return Err(AsterError::validation_error("min_size must be <= max_size"));
    }

    let has_file_only_filter = params.category.is_some() || params.extensions.is_some();
    if has_file_only_filter && matches!(params.search_type.as_deref(), Some("folder")) {
        return Err(AsterError::validation_error(
            "category and extensions require type=file or type=all",
        ));
    }

    if params.mime_type.is_some()
        && params
            .mime_type
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
    {
        return Err(AsterError::validation_error("mime_type must not be empty"));
    }

    if params.category.is_some()
        && params
            .category
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
    {
        return Err(AsterError::validation_error("category must not be empty"));
    }

    if params.extensions.is_some()
        && params
            .extensions
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
    {
        return Err(AsterError::validation_error("extensions must not be empty"));
    }

    Ok(())
}

fn normalize_file_filters(params: &SearchParams) -> Result<NormalizedFileFilters> {
    let category = params
        .category
        .as_deref()
        .map(parse_file_category)
        .transpose()?;
    let extensions = params
        .extensions
        .as_deref()
        .map(parse_extension_filters)
        .transpose()?
        .unwrap_or_default();

    Ok(NormalizedFileFilters {
        category,
        extensions,
    })
}

fn normalized_query(params: &SearchParams) -> Option<&str> {
    params.q.as_deref().map(str::trim).filter(|q| !q.is_empty())
}

fn parse_search_dates(params: &SearchParams) -> Result<SearchDateRange> {
    let parse_field =
        |field: &str, value: Option<&str>| -> Result<Option<chrono::DateTime<chrono::Utc>>> {
            value
                .map(|raw| {
                    DateTime::parse_from_rfc3339(raw)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .map_err(|_| {
                            AsterError::validation_error(format!(
                                "{field} must be a valid RFC3339 datetime"
                            ))
                        })
                })
                .transpose()
        };

    let created_after = parse_field("created_after", params.created_after.as_deref())?;
    let created_before = parse_field("created_before", params.created_before.as_deref())?;

    if let (Some(after), Some(before)) = (created_after, created_before)
        && after > before
    {
        return Err(AsterError::validation_error(
            "created_after must be <= created_before",
        ));
    }

    Ok((created_after, created_before))
}

pub(crate) async fn search_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    params: &SearchParams,
) -> Result<SearchResults> {
    validate_search_params(params)?;
    let file_filters = normalize_file_filters(params)?;
    workspace_storage_service::require_scope_access(state, scope).await?;

    let limit = params.limit.unwrap_or(50).clamp(1, 100);
    let offset = params.offset.unwrap_or(0);
    let query = normalized_query(params);
    let normalized_mime_type = params
        .mime_type
        .as_deref()
        .map(str::trim)
        .filter(|mime_type| !mime_type.is_empty())
        .map(str::to_string);
    tracing::debug!(
        scope = ?scope,
        search_type = params.search_type.as_deref().unwrap_or("all"),
        has_query = query.is_some(),
        query_len = query.map(str::len),
        mime_type = normalized_mime_type.as_deref().unwrap_or(""),
        category = params.category.as_deref().unwrap_or(""),
        has_extensions = params.extensions.is_some(),
        min_size = params.min_size,
        max_size = params.max_size,
        folder_id = params.folder_id,
        limit,
        offset,
        "running search"
    );

    let search_type = params.search_type.as_deref().unwrap_or("all");
    let (created_after, created_before) = parse_search_dates(params)?;
    let file_only_filters_present =
        file_filters.category.is_some() || !file_filters.extensions.is_empty();
    let include_folders = search_type != "file" && !file_only_filters_present;

    let (files, total_files, folders, total_folders, shared_file_ids, shared_folder_ids) =
        match scope {
            WorkspaceStorageScope::Personal { user_id } => {
                let file_search = async {
                    if search_type == "folder" {
                        Ok((vec![], 0))
                    } else {
                        search_repo::search_files(
                            &state.db,
                            user_id,
                            search_repo::FileSearchFilters {
                                query,
                                mime_type: normalized_mime_type.as_deref(),
                                category: file_filters.category,
                                extensions: &file_filters.extensions,
                                min_size: params.min_size,
                                max_size: params.max_size,
                                created_after,
                                created_before,
                                folder_id: params.folder_id,
                                limit,
                                offset,
                            },
                        )
                        .await
                    }
                };
                let folder_search = async {
                    if !include_folders {
                        Ok((vec![], 0))
                    } else {
                        search_repo::search_folders(
                            &state.db,
                            user_id,
                            search_repo::FolderSearchFilters {
                                query,
                                created_after,
                                created_before,
                                parent_id: params.folder_id,
                                limit,
                                offset,
                            },
                        )
                        .await
                    }
                };
                let ((files, total_files), (folders, total_folders)) =
                    tokio::try_join!(file_search, folder_search)?;

                let file_ids: Vec<i64> = files.iter().map(|file| file.id).collect();
                let folder_ids: Vec<i64> = folders.iter().map(|folder| folder.id).collect();
                let scope = WorkspaceStorageScope::Personal { user_id };
                let (shared_file_ids, shared_folder_ids) = tokio::try_join!(
                    share_service::find_active_file_ids_in_scope(state, scope, &file_ids),
                    share_service::find_active_folder_ids_in_scope(state, scope, &folder_ids),
                )?;

                (
                    files,
                    total_files,
                    folders,
                    total_folders,
                    shared_file_ids,
                    shared_folder_ids,
                )
            }
            WorkspaceStorageScope::Team {
                team_id,
                actor_user_id,
            } => {
                let file_search = async {
                    if search_type == "folder" {
                        Ok((vec![], 0))
                    } else {
                        search_repo::search_team_files(
                            &state.db,
                            team_id,
                            search_repo::FileSearchFilters {
                                query,
                                mime_type: normalized_mime_type.as_deref(),
                                category: file_filters.category,
                                extensions: &file_filters.extensions,
                                min_size: params.min_size,
                                max_size: params.max_size,
                                created_after,
                                created_before,
                                folder_id: params.folder_id,
                                limit,
                                offset,
                            },
                        )
                        .await
                    }
                };
                let folder_search = async {
                    if !include_folders {
                        Ok((vec![], 0))
                    } else {
                        search_repo::search_team_folders(
                            &state.db,
                            team_id,
                            search_repo::FolderSearchFilters {
                                query,
                                created_after,
                                created_before,
                                parent_id: params.folder_id,
                                limit,
                                offset,
                            },
                        )
                        .await
                    }
                };
                let ((files, total_files), (folders, total_folders)) =
                    tokio::try_join!(file_search, folder_search)?;

                let file_ids: Vec<i64> = files.iter().map(|file| file.id).collect();
                let folder_ids: Vec<i64> = folders.iter().map(|folder| folder.id).collect();
                let scope = WorkspaceStorageScope::Team {
                    team_id,
                    actor_user_id,
                };
                let (shared_file_ids, shared_folder_ids) = tokio::try_join!(
                    share_service::find_active_file_ids_in_scope(state, scope, &file_ids),
                    share_service::find_active_folder_ids_in_scope(state, scope, &folder_ids),
                )?;

                (
                    files,
                    total_files,
                    folders,
                    total_folders,
                    shared_file_ids,
                    shared_folder_ids,
                )
            }
        };

    let results = SearchResults {
        files: build_search_file_list_items(files, &shared_file_ids),
        folders: build_folder_list_items(folders, &shared_folder_ids),
        total_files,
        total_folders,
    };
    tracing::debug!(
        scope = ?scope,
        total_files = results.total_files,
        total_folders = results.total_folders,
        returned_files = results.files.len(),
        returned_folders = results.folders.len(),
        "completed search"
    );
    Ok(results)
}

pub async fn search(
    state: &PrimaryAppState,
    user_id: i64,
    params: &SearchParams,
) -> Result<SearchResults> {
    search_in_scope(state, WorkspaceStorageScope::Personal { user_id }, params).await
}

pub async fn search_in_team(
    state: &PrimaryAppState,
    team_id: i64,
    user_id: i64,
    params: &SearchParams,
) -> Result<SearchResults> {
    search_in_scope(
        state,
        WorkspaceStorageScope::Team {
            team_id,
            actor_user_id: user_id,
        },
        params,
    )
    .await
}

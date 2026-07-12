//! 服务模块：`content::search`。

use chrono::DateTime;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::{IntoParams, ToSchema};

use crate::api::api_error_code::ApiErrorCode;
use crate::api::pagination::SortBy;
use crate::db::repository::search_repo::{self, TagSearchFilter, TagSearchMatch};
use crate::errors::{AsterError, Result, validation_error_with_code};
use crate::runtime::SharedRuntimeState;
use crate::services::{
    content::tag,
    files::folder::{FileListItem, FolderListItem, build_folder_list_items_with_tags},
    share,
    workspace::storage::WorkspaceResourceScope,
    workspace::storage::{self, WorkspaceStorageScope},
};
use aster_forge_api::SortOrder;
use aster_forge_file_classification::FileCategory;
use aster_forge_file_classification::{parse_extension_filters, parse_file_category};

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
    /// Comma-separated tag ids, e.g. "1,2,3"
    pub tag_ids: Option<String>,
    /// Tag filter mode: "any" or "all" (default any)
    pub tag_match: Option<String>,
    /// Sort field (default name)
    pub sort_by: Option<SortBy>,
    /// Sort direction (default asc)
    pub sort_order: Option<SortOrder>,
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

fn search_validation_error(api_code: ApiErrorCode, message: impl Into<String>) -> AsterError {
    validation_error_with_code(api_code, message)
}

fn build_search_file_list_items(
    files: Vec<search_repo::FileSearchItem>,
    shared_file_ids: &HashSet<i64>,
    tags_by_entity: &std::collections::HashMap<
        (crate::types::EntityType, i64),
        Vec<tag::TagSummary>,
    >,
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
            tags: tags_by_entity
                .get(&(crate::types::EntityType::File, file.id))
                .cloned()
                .unwrap_or_default(),
        })
        .collect()
}

fn validate_search_params(params: &SearchParams) -> Result<()> {
    if params.q.is_some() && normalized_query(params).is_none() {
        return Err(search_validation_error(
            ApiErrorCode::SearchQueryEmpty,
            "search query must not be empty",
        ));
    }

    if let Some(search_type) = params.search_type.as_deref()
        && !matches!(search_type, "file" | "folder" | "all")
    {
        return Err(search_validation_error(
            ApiErrorCode::SearchTypeInvalid,
            "type must be one of: file, folder, all",
        ));
    }

    if let Some(tag_match) = params.tag_match.as_deref()
        && !matches!(tag_match, "any" | "all")
    {
        return Err(search_validation_error(
            ApiErrorCode::SearchTagMatchInvalid,
            "tag_match must be one of: any, all",
        ));
    }

    if let (Some(min), Some(max)) = (params.min_size, params.max_size)
        && min > max
    {
        return Err(search_validation_error(
            ApiErrorCode::SearchSizeRangeInvalid,
            "min_size must be <= max_size",
        ));
    }

    let has_file_only_filter = params.category.is_some() || params.extensions.is_some();
    if has_file_only_filter && matches!(params.search_type.as_deref(), Some("folder")) {
        return Err(search_validation_error(
            ApiErrorCode::SearchFileFilterTypeConflict,
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
        return Err(search_validation_error(
            ApiErrorCode::SearchMimeTypeEmpty,
            "mime_type must not be empty",
        ));
    }

    if params.category.is_some()
        && params
            .category
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
    {
        return Err(search_validation_error(
            ApiErrorCode::SearchCategoryInvalid,
            "category must not be empty",
        ));
    }

    if params.extensions.is_some()
        && params
            .extensions
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
    {
        return Err(search_validation_error(
            ApiErrorCode::SearchExtensionsInvalid,
            "extensions must not be empty",
        ));
    }

    Ok(())
}

fn parse_tag_ids(params: &SearchParams) -> Result<Vec<i64>> {
    let Some(raw) = params.tag_ids.as_deref() else {
        return Ok(vec![]);
    };
    let mut ids = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for part in raw.split(',') {
        let value = part.trim();
        if value.is_empty() {
            return Err(search_validation_error(
                ApiErrorCode::SearchTagIdsInvalid,
                "tag_ids must not contain empty values",
            ));
        }
        let id = value.parse::<i64>().map_err(|_| {
            search_validation_error(
                ApiErrorCode::SearchTagIdsInvalid,
                "tag_ids must contain integer ids",
            )
        })?;
        if id <= 0 {
            return Err(search_validation_error(
                ApiErrorCode::SearchTagIdsInvalid,
                "tag_ids must contain positive ids",
            ));
        }
        if seen.insert(id) {
            ids.push(id);
        }
    }
    if ids.len() > 64 {
        return Err(search_validation_error(
            ApiErrorCode::SearchTagIdsInvalid,
            "tag_ids cannot contain more than 64 items",
        ));
    }
    Ok(ids)
}

fn normalize_file_filters(params: &SearchParams) -> Result<NormalizedFileFilters> {
    let category = params
        .category
        .as_deref()
        .map(parse_file_category)
        .transpose()
        .map_err(AsterError::from)
        .map_err(|error| error.with_api_error_code(ApiErrorCode::SearchCategoryInvalid))?;
    let extensions = params
        .extensions
        .as_deref()
        .map(parse_extension_filters)
        .transpose()
        .map_err(AsterError::from)
        .map_err(|error| error.with_api_error_code(ApiErrorCode::SearchExtensionsInvalid))?
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
                            search_validation_error(
                                ApiErrorCode::SearchDateInvalid,
                                format!("{field} must be a valid RFC3339 datetime"),
                            )
                        })
                })
                .transpose()
        };

    let created_after = parse_field("created_after", params.created_after.as_deref())?;
    let created_before = parse_field("created_before", params.created_before.as_deref())?;

    if let (Some(after), Some(before)) = (created_after, created_before)
        && after > before
    {
        return Err(search_validation_error(
            ApiErrorCode::SearchDateRangeInvalid,
            "created_after must be <= created_before",
        ));
    }

    Ok((created_after, created_before))
}

pub(crate) async fn search_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    params: &SearchParams,
) -> Result<SearchResults> {
    validate_search_params(params)?;
    let file_filters = normalize_file_filters(params)?;
    storage::require_scope_access(state, scope).await?;

    let limit = params.limit.unwrap_or(50).clamp(1, 100);
    let offset = params.offset.unwrap_or(0);
    let sort_by = params.sort_by.unwrap_or_default();
    let sort_order = params.sort_order.unwrap_or_default();
    let query = normalized_query(params);
    let tag_ids = parse_tag_ids(params)?;
    let tag_match = params.tag_match.as_deref().unwrap_or("any");
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
        has_tag_filter = !tag_ids.is_empty(),
        tag_match,
        sort_by = ?sort_by,
        sort_order = ?sort_order,
        limit,
        offset,
        "running search"
    );

    let search_type = params.search_type.as_deref().unwrap_or("all");
    let (created_after, created_before) = parse_search_dates(params)?;
    let file_only_filters_present =
        file_filters.category.is_some() || !file_filters.extensions.is_empty();
    let include_folders = search_type != "file" && !file_only_filters_present;
    if !tag_ids.is_empty() {
        tag::ensure_tags_readable_in_scope(state, scope, &tag_ids).await?;
    }
    let tag_filter = (!tag_ids.is_empty()).then_some(TagSearchFilter {
        tag_ids: &tag_ids,
        match_mode: if tag_match == "all" {
            TagSearchMatch::All
        } else {
            TagSearchMatch::Any
        },
    });

    let (
        files,
        total_files,
        folders,
        total_folders,
        shared_file_ids,
        shared_folder_ids,
        tags_by_entity,
    ) = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            let file_search = async {
                if search_type == "folder" {
                    Ok((vec![], 0))
                } else {
                    search_repo::search_files(
                        state.reader_db(),
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
                            tag_filter,
                            sort_by,
                            sort_order,
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
                        state.reader_db(),
                        user_id,
                        search_repo::FolderSearchFilters {
                            query,
                            created_after,
                            created_before,
                            parent_id: params.folder_id,
                            tag_filter,
                            sort_by,
                            sort_order,
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
                share::find_active_file_ids_in_scope(state, scope, &file_ids),
                share::find_active_folder_ids_in_scope(state, scope, &folder_ids),
            )?;
            let tags_by_entity = tag::load_entity_tag_map(
                state,
                WorkspaceResourceScope::Personal { user_id },
                &file_ids,
                &folder_ids,
            )
            .await?;

            (
                files,
                total_files,
                folders,
                total_folders,
                shared_file_ids,
                shared_folder_ids,
                tags_by_entity,
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
                        state.reader_db(),
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
                            tag_filter,
                            sort_by,
                            sort_order,
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
                        state.reader_db(),
                        team_id,
                        search_repo::FolderSearchFilters {
                            query,
                            created_after,
                            created_before,
                            parent_id: params.folder_id,
                            tag_filter,
                            sort_by,
                            sort_order,
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
                share::find_active_file_ids_in_scope(state, scope, &file_ids),
                share::find_active_folder_ids_in_scope(state, scope, &folder_ids),
            )?;
            let tags_by_entity = tag::load_entity_tag_map(
                state,
                WorkspaceResourceScope::Team { team_id },
                &file_ids,
                &folder_ids,
            )
            .await?;

            (
                files,
                total_files,
                folders,
                total_folders,
                shared_file_ids,
                shared_folder_ids,
                tags_by_entity,
            )
        }
    };

    let results = SearchResults {
        files: build_search_file_list_items(files, &shared_file_ids, &tags_by_entity),
        folders: build_folder_list_items_with_tags(folders, &shared_folder_ids, &tags_by_entity),
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
    state: &impl SharedRuntimeState,
    user_id: i64,
    params: &SearchParams,
) -> Result<SearchResults> {
    search_in_scope(state, WorkspaceStorageScope::Personal { user_id }, params).await
}

pub async fn search_in_team(
    state: &impl SharedRuntimeState,
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

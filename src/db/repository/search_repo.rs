//! 仓储模块：`search_repo`。

use crate::api::pagination::SortBy;
use crate::errors::{AsterError, Result};
use crate::services::content::tag::TAG_PROPERTY_NAMESPACE;
use aster_drive_model::entities::{
    entity_property::{self, Entity as EntityProperty},
    file::{self, Entity as File},
    file_blob,
    folder::{self, Entity as Folder},
};
use aster_drive_model::types::EntityType;
use aster_forge_api::SortOrder;
use aster_forge_db::search_query::{
    escape_like_query, lower_like_condition, mysql_boolean_mode_query, sqlite_fts_match_condition,
    sqlite_match_query,
};
use aster_forge_db::sort::order_by_column_with_id;
use aster_forge_file_classification::FileCategory;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::Query;
use sea_orm::sea_query::extension::postgres::PgExpr;
use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, DbBackend, EntityTrait, ExprTrait, FromQueryResult,
    JoinType, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, RelationTrait, sea_query::Expr,
};
use serde::Serialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

type DateTimeUtc = DateTime<Utc>;

const SQLITE_FILES_FTS_TABLE: &str = "files_name_fts";
const SQLITE_FOLDERS_FTS_TABLE: &str = "folders_name_fts";

#[derive(Clone, Copy)]
enum SearchScope {
    Personal { user_id: i64 },
    Team { team_id: i64 },
}

/// How a tag filter combines the requested tags.
#[derive(Clone, Copy)]
pub enum TagSearchMatch {
    /// Entities carrying any of the requested tags match.
    Any,
    /// Entities carrying every requested tag match.
    All,
}

/// Tag-based search filter resolved against tag properties.
#[derive(Clone, Copy)]
pub struct TagSearchFilter<'a> {
    /// Tag ids to match; ids are stored as property names under the tag namespace.
    pub tag_ids: &'a [i64],
    /// Whether any or all of the tags must be present.
    pub match_mode: TagSearchMatch,
}

fn file_scope_condition(scope: SearchScope) -> Condition {
    match scope {
        SearchScope::Personal { user_id } => Condition::all()
            .add(file::Column::OwnerUserId.eq(user_id))
            .add(file::Column::TeamId.is_null()),
        SearchScope::Team { team_id } => Condition::all().add(file::Column::TeamId.eq(team_id)),
    }
}

fn folder_scope_condition(scope: SearchScope) -> Condition {
    match scope {
        SearchScope::Personal { user_id } => Condition::all()
            .add(folder::Column::OwnerUserId.eq(user_id))
            .add(folder::Column::TeamId.is_null()),
        SearchScope::Team { team_id } => Condition::all().add(folder::Column::TeamId.eq(team_id)),
    }
}

/// Filters, ordering, and paging for file searches.
#[derive(Clone, Copy)]
pub struct FileSearchFilters<'a> {
    /// Optional name query; matched via FTS or LIKE depending on the database backend.
    pub query: Option<&'a str>,
    /// Exact MIME type to match.
    pub mime_type: Option<&'a str>,
    /// File category to match.
    pub category: Option<FileCategory>,
    /// Extension filter; matches the simple or compound extension, empty means unfiltered.
    pub extensions: &'a [String],
    /// Inclusive lower bound on blob size in bytes.
    pub min_size: Option<i64>,
    /// Inclusive upper bound on blob size in bytes.
    pub max_size: Option<i64>,
    /// Inclusive lower bound on the creation time.
    pub created_after: Option<DateTime<Utc>>,
    /// Inclusive upper bound on the creation time.
    pub created_before: Option<DateTime<Utc>>,
    /// Restricts results to this folder.
    pub folder_id: Option<i64>,
    /// Optional tag-based filter.
    pub tag_filter: Option<TagSearchFilter<'a>>,
    /// Result ordering key.
    pub sort_by: SortBy,
    /// Result ordering direction.
    pub sort_order: SortOrder,
    /// Maximum number of items returned.
    pub limit: u64,
    /// Number of items skipped before the first returned item.
    pub offset: u64,
}

/// Filters, ordering, and paging for folder searches.
#[derive(Clone, Copy)]
pub struct FolderSearchFilters<'a> {
    /// Optional name query; matched via FTS or LIKE depending on the database backend.
    pub query: Option<&'a str>,
    /// Inclusive lower bound on the creation time.
    pub created_after: Option<DateTime<Utc>>,
    /// Inclusive upper bound on the creation time.
    pub created_before: Option<DateTime<Utc>>,
    /// Restricts results to this parent folder.
    pub parent_id: Option<i64>,
    /// Optional tag-based filter.
    pub tag_filter: Option<TagSearchFilter<'a>>,
    /// Result ordering key.
    pub sort_by: SortBy,
    /// Result ordering direction.
    pub sort_order: SortOrder,
    /// Maximum number of items returned.
    pub limit: u64,
    /// Number of items skipped before the first returned item.
    pub offset: u64,
}

/// Search result file item (includes blob size from JOIN)
#[derive(Debug, Serialize, FromQueryResult)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct FileSearchItem {
    /// File id.
    pub id: i64,
    /// File name including its extension.
    pub name: String,
    /// Id of the containing folder; `None` when the file is at the root.
    pub folder_id: Option<i64>,
    /// Id of the blob backing the file content.
    pub blob_id: i64,
    /// Owning user for personal files; `None` for team-owned files.
    pub owner_user_id: Option<i64>,
    /// Id of the user that created the file, if known.
    pub created_by_user_id: Option<i64>,
    /// Display name of the user that created the file.
    pub created_by_username: String,
    /// Id of the user that last modified the file, if tracked.
    pub modified_by_user_id: Option<i64>,
    /// Display name of the user that last modified the file.
    pub modified_by_username: Option<String>,
    /// Time of the last modification, if tracked.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub modified_at: Option<DateTimeUtc>,
    /// MIME type of the file content.
    pub mime_type: String,
    /// Simple extension of the file.
    pub extension: String,
    /// Compound extension such as `tar.gz`, when present.
    pub compound_extension: Option<String>,
    /// Classification category of the file.
    pub file_category: FileCategory,
    /// Blob size in bytes, projected from the joined blob row.
    pub size: i64,
    /// Creation time of the file.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    /// Time of the last update to the file.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTimeUtc,
}

fn name_search_condition(
    backend: DbBackend,
    column: impl sea_orm::sea_query::IntoColumnRef + Copy,
    query: &str,
) -> sea_orm::sea_query::SimpleExpr {
    match backend {
        DbBackend::Postgres => Expr::col(column).ilike(format!("%{}%", escape_like_query(query))),
        DbBackend::MySql => mysql_boolean_mode_query(query)
            .map(|boolean_query| {
                Expr::cust_with_exprs(
                    "MATCH(?) AGAINST (? IN BOOLEAN MODE)",
                    [Expr::col(column), Expr::val(boolean_query)],
                )
            })
            .unwrap_or_else(|| lower_like_condition(column, query)),
        _ => lower_like_condition(column, query),
    }
}

fn tag_search_condition(
    backend: DbBackend,
    entity_id_column: impl sea_orm::sea_query::IntoColumnRef + Copy,
    entity_type: EntityType,
    filter: TagSearchFilter<'_>,
) -> Option<sea_orm::sea_query::SimpleExpr> {
    if filter.tag_ids.is_empty() {
        return None;
    }

    let tag_names = filter
        .tag_ids
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>();
    let mut subquery = Query::select();
    subquery
        .expr(Expr::col(entity_property::Column::EntityId))
        .from(EntityProperty)
        .and_where(
            crate::db::repository::property_repo::namespace_eq_condition(
                backend,
                TAG_PROPERTY_NAMESPACE,
            ),
        )
        .and_where(entity_property::Column::EntityType.eq(entity_type))
        .and_where(entity_property::Column::Name.is_in(tag_names));

    if matches!(filter.match_mode, TagSearchMatch::All) {
        subquery
            .group_by_col(entity_property::Column::EntityId)
            .and_having(
                Expr::col(entity_property::Column::Name)
                    .count_distinct()
                    .eq(filter.tag_ids.len() as i64),
            );
    }

    Some(Expr::col(entity_id_column).in_subquery(subquery.to_owned()))
}

fn apply_file_search_order<E>(query: E, sort_by: SortBy, sort_order: SortOrder) -> E
where
    E: QueryOrder,
{
    match sort_by {
        SortBy::Name => {
            order_by_column_with_id(query, file::Column::Name, sort_order, file::Column::Id)
        }
        SortBy::Size => {
            order_by_column_with_id(query, file::Column::Size, sort_order, file::Column::Id)
        }
        SortBy::CreatedAt => {
            order_by_column_with_id(query, file::Column::CreatedAt, sort_order, file::Column::Id)
        }
        SortBy::UpdatedAt => {
            order_by_column_with_id(query, file::Column::UpdatedAt, sort_order, file::Column::Id)
        }
        SortBy::Type => {
            order_by_column_with_id(query, file::Column::MimeType, sort_order, file::Column::Id)
        }
    }
}

fn apply_folder_search_order<E>(query: E, sort_by: SortBy, sort_order: SortOrder) -> E
where
    E: QueryOrder,
{
    match sort_by {
        SortBy::CreatedAt => order_by_column_with_id(
            query,
            folder::Column::CreatedAt,
            sort_order,
            folder::Column::Id,
        ),
        SortBy::UpdatedAt => order_by_column_with_id(
            query,
            folder::Column::UpdatedAt,
            sort_order,
            folder::Column::Id,
        ),
        _ => order_by_column_with_id(query, folder::Column::Name, sort_order, folder::Column::Id),
    }
}

/// Search files with optional filters. JOINs file_blobs to include size.
///
/// Returns `(items, total_count)`.
async fn search_files_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: SearchScope,
    filters: FileSearchFilters<'_>,
) -> Result<(Vec<FileSearchItem>, u64)> {
    let backend = db.get_database_backend();
    let mut file_condition = file_scope_condition(scope).add(file::Column::DeletedAt.is_null());
    let mut blob_condition = Condition::all();

    if let Some(q) = filters.query {
        if backend == DbBackend::Sqlite {
            if let Some(match_query) = sqlite_match_query(q) {
                file_condition = file_condition.add(sqlite_fts_match_condition(
                    (File, file::Column::Id),
                    SQLITE_FILES_FTS_TABLE,
                    &match_query,
                ));
            } else {
                file_condition = file_condition.add(name_search_condition(
                    backend,
                    (File, file::Column::Name),
                    q,
                ));
            }
        } else {
            file_condition = file_condition.add(name_search_condition(
                backend,
                (File, file::Column::Name),
                q,
            ));
        }
    }

    if let Some(mt) = filters.mime_type {
        file_condition = file_condition.add(file::Column::MimeType.eq(mt));
    }

    if let Some(category) = filters.category {
        file_condition = file_condition.add(file::Column::FileCategory.eq(category));
    }

    if !filters.extensions.is_empty() {
        let extension_condition =
            filters
                .extensions
                .iter()
                .fold(Condition::any(), |condition, extension| {
                    condition
                        .add(file::Column::Extension.eq(extension.as_str()))
                        .add(file::Column::CompoundExtension.eq(extension.as_str()))
                });
        file_condition = file_condition.add(extension_condition);
    }

    if let Some(min) = filters.min_size {
        blob_condition = blob_condition.add(file_blob::Column::Size.gte(min));
    }

    if let Some(max) = filters.max_size {
        blob_condition = blob_condition.add(file_blob::Column::Size.lte(max));
    }

    if let Some(after) = filters.created_after {
        file_condition = file_condition.add(file::Column::CreatedAt.gte(after));
    }

    if let Some(before) = filters.created_before {
        file_condition = file_condition.add(file::Column::CreatedAt.lte(before));
    }

    if let Some(folder_id) = filters.folder_id {
        file_condition = file_condition.add(file::Column::FolderId.eq(folder_id));
    }

    if let Some(tag_filter) = filters.tag_filter
        && let Some(condition) = tag_search_condition(
            backend,
            (File, file::Column::Id),
            EntityType::File,
            tag_filter,
        )
    {
        file_condition = file_condition.add(condition);
    }

    let needs_blob_filters = filters.min_size.is_some() || filters.max_size.is_some();

    let mut count_query = File::find().filter(file_condition.clone());
    if needs_blob_filters {
        count_query = count_query
            .join(JoinType::InnerJoin, file::Relation::FileBlob.def())
            .filter(blob_condition.clone());
    }

    let total = count_query.count(db).await.map_err(AsterError::from)?;

    if total == 0 {
        return Ok((vec![], 0));
    }

    let query = File::find()
        .join(JoinType::InnerJoin, file::Relation::FileBlob.def())
        .filter(file_condition)
        .filter(blob_condition)
        .select_only()
        .column(file::Column::Id)
        .column(file::Column::Name)
        .column(file::Column::FolderId)
        .column(file::Column::BlobId)
        .column(file::Column::OwnerUserId)
        .column(file::Column::CreatedByUserId)
        .column(file::Column::CreatedByUsername)
        .column(file::Column::ModifiedByUserId)
        .column(file::Column::ModifiedByUsername)
        .column(file::Column::ModifiedAt)
        .column(file::Column::MimeType)
        .column(file::Column::Extension)
        .column(file::Column::CompoundExtension)
        .column(file::Column::FileCategory)
        .column_as(file_blob::Column::Size, "size")
        .column(file::Column::CreatedAt)
        .column(file::Column::UpdatedAt);

    let items = apply_file_search_order(query, filters.sort_by, filters.sort_order)
        .limit(filters.limit)
        .offset(filters.offset)
        .into_model::<FileSearchItem>()
        .all(db)
        .await
        .map_err(AsterError::from)?;

    Ok((items, total))
}

/// Searches files owned by the given user, returning `(items, total_count)`.
pub async fn search_files<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    filters: FileSearchFilters<'_>,
) -> Result<(Vec<FileSearchItem>, u64)> {
    search_files_in_scope(db, SearchScope::Personal { user_id }, filters).await
}

/// Searches files in the given team, returning `(items, total_count)`.
pub async fn search_team_files<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    filters: FileSearchFilters<'_>,
) -> Result<(Vec<FileSearchItem>, u64)> {
    search_files_in_scope(db, SearchScope::Team { team_id }, filters).await
}

/// Search folders with optional filters.
///
/// Returns `(items, total_count)`.
async fn search_folders_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: SearchScope,
    filters: FolderSearchFilters<'_>,
) -> Result<(Vec<folder::Model>, u64)> {
    let backend = db.get_database_backend();
    let mut condition = folder_scope_condition(scope).add(folder::Column::DeletedAt.is_null());

    if let Some(q) = filters.query {
        if backend == DbBackend::Sqlite {
            if let Some(match_query) = sqlite_match_query(q) {
                condition = condition.add(sqlite_fts_match_condition(
                    (Folder, folder::Column::Id),
                    SQLITE_FOLDERS_FTS_TABLE,
                    &match_query,
                ));
            } else {
                condition = condition.add(name_search_condition(
                    backend,
                    (Folder, folder::Column::Name),
                    q,
                ));
            }
        } else {
            condition = condition.add(name_search_condition(
                backend,
                (Folder, folder::Column::Name),
                q,
            ));
        }
    }

    if let Some(after) = filters.created_after {
        condition = condition.add(folder::Column::CreatedAt.gte(after));
    }

    if let Some(before) = filters.created_before {
        condition = condition.add(folder::Column::CreatedAt.lte(before));
    }

    if let Some(parent_id) = filters.parent_id {
        condition = condition.add(folder::Column::ParentId.eq(parent_id));
    }

    if let Some(tag_filter) = filters.tag_filter
        && let Some(tag_condition) = tag_search_condition(
            backend,
            (Folder, folder::Column::Id),
            EntityType::Folder,
            tag_filter,
        )
    {
        condition = condition.add(tag_condition);
    }

    let base = Folder::find().filter(condition);

    let total = base.clone().count(db).await.map_err(AsterError::from)?;

    if total == 0 {
        return Ok((vec![], 0));
    }

    let items = apply_folder_search_order(base, filters.sort_by, filters.sort_order)
        .limit(filters.limit)
        .offset(filters.offset)
        .all(db)
        .await
        .map_err(AsterError::from)?;

    Ok((items, total))
}

/// Searches folders owned by the given user, returning `(items, total_count)`.
pub async fn search_folders<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    filters: FolderSearchFilters<'_>,
) -> Result<(Vec<folder::Model>, u64)> {
    search_folders_in_scope(db, SearchScope::Personal { user_id }, filters).await
}

/// Searches folders in the given team, returning `(items, total_count)`.
pub async fn search_team_folders<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    filters: FolderSearchFilters<'_>,
) -> Result<(Vec<folder::Model>, u64)> {
    search_folders_in_scope(db, SearchScope::Team { team_id }, filters).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{
        DbBackend, JoinType, QueryFilter, QueryTrait, RelationTrait,
        sea_query::{MysqlQueryBuilder, Query},
    };

    #[test]
    fn mysql_match_against_sql_is_valid() {
        let sql: String = Query::select()
            .expr(super::name_search_condition(
                DbBackend::MySql,
                super::file::Column::Name,
                "report",
            ))
            .from(super::File)
            .to_string(MysqlQueryBuilder);

        assert!(
            sql.as_str()
                .contains(r#"MATCH(`name`) AGAINST ('\"report\"' IN BOOLEAN MODE)"#),
            "{sql}"
        );
        assert!(!sql.as_str().contains("$1"), "{sql}");
    }

    #[test]
    fn sqlite_file_search_condition_qualifies_file_id_for_join_queries() {
        let sql: String = format!(
            "{}",
            File::find()
                .join(JoinType::InnerJoin, file::Relation::FileBlob.def())
                .filter(sqlite_fts_match_condition(
                    (File, file::Column::Id),
                    SQLITE_FILES_FTS_TABLE,
                    "\"report\"",
                ))
                .build(DbBackend::Sqlite)
        );

        assert!(
            sql.as_str()
                .contains(r#""files"."id" IN (SELECT "rowid" FROM "files_name_fts""#),
            "{sql}"
        );
    }
}

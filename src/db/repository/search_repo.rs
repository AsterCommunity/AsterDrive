//! 仓储模块：`search_repo`。

use crate::db::repository::search_query::{
    escape_like_query, lower_like_condition, mysql_boolean_mode_query, sqlite_fts_match_condition,
    sqlite_match_query,
};
use crate::entities::{
    file::{self, Entity as File},
    file_blob,
    folder::{self, Entity as Folder},
};
use crate::errors::{AsterError, Result};
use chrono::{DateTime, Utc};
use sea_orm::sea_query::extension::postgres::PgExpr;
use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, DbBackend, EntityTrait, FromQueryResult, JoinType,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, RelationTrait, sea_query::Expr,
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

#[derive(Clone, Copy)]
pub struct FileSearchFilters<'a> {
    pub query: Option<&'a str>,
    pub mime_type: Option<&'a str>,
    pub min_size: Option<i64>,
    pub max_size: Option<i64>,
    pub created_after: Option<DateTime<Utc>>,
    pub created_before: Option<DateTime<Utc>>,
    pub folder_id: Option<i64>,
    pub limit: u64,
    pub offset: u64,
}

#[derive(Clone, Copy)]
pub struct FolderSearchFilters<'a> {
    pub query: Option<&'a str>,
    pub created_after: Option<DateTime<Utc>>,
    pub created_before: Option<DateTime<Utc>>,
    pub parent_id: Option<i64>,
    pub limit: u64,
    pub offset: u64,
}

/// Search result file item (includes blob size from JOIN)
#[derive(Debug, Serialize, FromQueryResult)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct FileSearchItem {
    pub id: i64,
    pub name: String,
    pub folder_id: Option<i64>,
    pub blob_id: i64,
    pub owner_user_id: Option<i64>,
    pub created_by_user_id: Option<i64>,
    pub created_by_username: String,
    pub mime_type: String,
    pub size: i64,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTimeUtc,
    pub is_locked: bool,
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

    let items = File::find()
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
        .column(file::Column::MimeType)
        .column_as(file_blob::Column::Size, "size")
        .column(file::Column::CreatedAt)
        .column(file::Column::UpdatedAt)
        .column(file::Column::IsLocked)
        .order_by_asc(file::Column::Name)
        .limit(filters.limit)
        .offset(filters.offset)
        .into_model::<FileSearchItem>()
        .all(db)
        .await
        .map_err(AsterError::from)?;

    Ok((items, total))
}

pub async fn search_files<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    filters: FileSearchFilters<'_>,
) -> Result<(Vec<FileSearchItem>, u64)> {
    search_files_in_scope(db, SearchScope::Personal { user_id }, filters).await
}

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

    let base = Folder::find().filter(condition);

    let total = base.clone().count(db).await.map_err(AsterError::from)?;

    if total == 0 {
        return Ok((vec![], 0));
    }

    let items = base
        .order_by_asc(folder::Column::Name)
        .limit(filters.limit)
        .offset(filters.offset)
        .all(db)
        .await
        .map_err(AsterError::from)?;

    Ok((items, total))
}

pub async fn search_folders<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    filters: FolderSearchFilters<'_>,
) -> Result<(Vec<folder::Model>, u64)> {
    search_folders_in_scope(db, SearchScope::Personal { user_id }, filters).await
}

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

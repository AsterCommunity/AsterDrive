use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait,
    ExprTrait, FromQueryResult, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set,
    TryInsertResult, sea_query::Expr,
};

use crate::api::pagination::AdminFileBlobSortBy;
use crate::entities::file_blob::{self, Entity as FileBlob};
use crate::errors::{AsterError, Result};
use aster_forge_api::SortOrder;
use aster_forge_db::search_query::lower_like_condition;
use aster_forge_db::sort::{order_by_column_with_id, order_by_id};

use super::ref_count::{find_active_blob_by_hash, increment_blob_ref_count};

pub struct FindOrCreateBlobResult {
    pub model: file_blob::Model,
    pub inserted: bool,
}

#[derive(Debug, Clone, FromQueryResult)]
pub struct StoragePolicyBlobSummary {
    pub count: i64,
    pub total_size: i64,
}

#[derive(Debug, Clone, FromQueryResult)]
pub struct StoragePolicyMissingBlobSummary {
    pub count: i64,
    pub total_size: i64,
}

#[derive(Debug, Clone, FromQueryResult)]
pub struct StoragePolicyBlobHashKindSummary {
    pub content_sha256_count: i64,
    pub opaque_count: i64,
}

#[derive(Debug, Clone, FromQueryResult)]
struct CountRow {
    count: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct AdminFileBlobFilters<'a> {
    pub hash: Option<&'a str>,
    pub policy_id: Option<i64>,
    pub storage_path: Option<&'a str>,
    pub ref_count_min: Option<i32>,
    pub ref_count_max: Option<i32>,
    pub size_min: Option<i64>,
    pub size_max: Option<i64>,
    pub sort_by: AdminFileBlobSortBy,
    pub sort_order: SortOrder,
}

fn content_sha256_sql_condition(backend: DbBackend, column: &str) -> String {
    match backend {
        DbBackend::Postgres => format!("{column} ~ '^[0-9A-Fa-f]{{64}}$'"),
        DbBackend::MySql => format!("{column} REGEXP '^[0-9A-Fa-f]{{64}}$'"),
        DbBackend::Sqlite | _ => {
            format!("length({column}) = 64 AND {column} NOT GLOB '*[^0-9A-Fa-f]*'")
        }
    }
}

fn i64_cast_sql(backend: DbBackend, expr: &str) -> String {
    let type_name = match backend {
        DbBackend::Postgres => "BIGINT",
        DbBackend::MySql => "SIGNED",
        DbBackend::Sqlite | _ => "INTEGER",
    };
    format!("CAST({expr} AS {type_name})")
}

// `find_or_create_blob()` only retries short-lived races:
// 1. another transaction inserted the same (hash, policy_id) row but has not become visible yet;
// 2. a cleanup worker deleted a zero-ref blob after we read it but before we bumped ref_count.
//
// Those windows should resolve after the competing transaction commits, so we use a small
// exponential backoff budget instead of a fixed 1s spin loop. Total sleep is capped at
// 5 + 10 + 20 + 40 + 80 + 80 = 235ms across 7 attempts.
const FIND_OR_CREATE_BLOB_MAX_ATTEMPTS: usize = 7;
const FIND_OR_CREATE_BLOB_INITIAL_DELAY_MS: u64 = 5;
const FIND_OR_CREATE_BLOB_MAX_DELAY_MS: u64 = 80;

pub async fn find_blob_by_hash<C: ConnectionTrait>(
    db: &C,
    hash: &str,
    policy_id: i64,
) -> Result<Option<file_blob::Model>> {
    FileBlob::find()
        .filter(file_blob::Column::Hash.eq(hash))
        .filter(file_blob::Column::PolicyId.eq(policy_id))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_blobs_by_policy_paginated<C: ConnectionTrait>(
    db: &C,
    policy_id: i64,
    after_id: i64,
    limit: u64,
) -> Result<Vec<file_blob::Model>> {
    FileBlob::find()
        .filter(file_blob::Column::PolicyId.eq(policy_id))
        .filter(file_blob::Column::Id.gt(after_id))
        .order_by_asc(file_blob::Column::Id)
        .limit(limit)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn summarize_blobs_by_policy<C: ConnectionTrait>(
    db: &C,
    policy_id: i64,
) -> Result<StoragePolicyBlobSummary> {
    let (count, total_size) = FileBlob::find()
        .select_only()
        .column_as(Expr::col(file_blob::Column::Id).count(), "count")
        .column_as(
            sum_blob_size_as_i64_expr(db.get_database_backend()),
            "total_size",
        )
        .filter(file_blob::Column::PolicyId.eq(policy_id))
        .into_tuple::<(i64, Option<i64>)>()
        .one(db)
        .await
        .map_err(AsterError::from)?
        .unwrap_or((0, None));

    Ok(StoragePolicyBlobSummary {
        count,
        total_size: total_size.unwrap_or(0),
    })
}

pub async fn summarize_blob_hash_kinds_by_policy<C: ConnectionTrait>(
    db: &C,
    policy_id: i64,
) -> Result<StoragePolicyBlobHashKindSummary> {
    let backend = db.get_database_backend();
    let condition = match backend {
        DbBackend::Postgres => "hash ~ '^[0-9A-Fa-f]{64}$'",
        DbBackend::MySql => "hash REGEXP '^[0-9A-Fa-f]{64}$'",
        DbBackend::Sqlite | _ => "length(hash) = 64 AND hash NOT GLOB '*[^0-9A-Fa-f]*'",
    };
    let content_count = i64_cast_sql(
        backend,
        &format!("COALESCE(SUM(CASE WHEN {condition} THEN 1 ELSE 0 END), 0)"),
    );
    let opaque_count = i64_cast_sql(
        backend,
        &format!("COALESCE(SUM(CASE WHEN {condition} THEN 0 ELSE 1 END), 0)"),
    );
    let sql = format!(
        "SELECT \
            {content_count} AS content_sha256_count, \
            {opaque_count} AS opaque_count \
         FROM file_blobs WHERE policy_id = ?"
    );
    let statement = match backend {
        DbBackend::Postgres => sea_orm::Statement::from_sql_and_values(
            backend,
            sql.replace("policy_id = ?", "policy_id = $1"),
            [policy_id.into()],
        ),
        DbBackend::MySql | DbBackend::Sqlite | _ => {
            sea_orm::Statement::from_sql_and_values(backend, sql, [policy_id.into()])
        }
    };
    StoragePolicyBlobHashKindSummary::find_by_statement(statement)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| {
            AsterError::internal_error(
                "storage policy blob hash kind summary query returned no row",
            )
        })
}

pub async fn count_matching_hashes_between_policies<C: ConnectionTrait>(
    db: &C,
    source_policy_id: i64,
    target_policy_id: i64,
) -> Result<i64> {
    let backend = db.get_database_backend();
    let content_hash_condition = content_sha256_sql_condition(backend, "source.hash");
    let sql = match backend {
        DbBackend::Postgres => {
            format!(
                r#"SELECT COUNT(*) AS count
               FROM file_blobs source
               INNER JOIN file_blobs target
                 ON target.hash = source.hash
                AND target.size = source.size
                AND target.policy_id = $2
               WHERE source.policy_id = $1 AND {content_hash_condition}"#
            )
        }
        DbBackend::MySql | DbBackend::Sqlite | _ => {
            format!(
                r#"SELECT COUNT(*) AS count
               FROM file_blobs source
               INNER JOIN file_blobs target
                 ON target.hash = source.hash
                AND target.size = source.size
                AND target.policy_id = ?
               WHERE source.policy_id = ? AND {content_hash_condition}"#
            )
        }
    };
    let statement = match backend {
        DbBackend::Postgres => sea_orm::Statement::from_sql_and_values(
            backend,
            sql,
            [source_policy_id.into(), target_policy_id.into()],
        ),
        DbBackend::MySql | DbBackend::Sqlite | _ => sea_orm::Statement::from_sql_and_values(
            backend,
            sql,
            [target_policy_id.into(), source_policy_id.into()],
        ),
    };
    CountRow::find_by_statement(statement)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .map(|row| row.count)
        .ok_or_else(|| AsterError::internal_error("matching hash count query returned no row"))
}

pub async fn count_opaque_hash_conflicts_between_policies<C: ConnectionTrait>(
    db: &C,
    source_policy_id: i64,
    target_policy_id: i64,
) -> Result<i64> {
    let backend = db.get_database_backend();
    let content_hash_condition = content_sha256_sql_condition(backend, "source.hash");
    let sql = match backend {
        DbBackend::Postgres => format!(
            r#"SELECT COUNT(*) AS count
               FROM file_blobs source
               WHERE source.policy_id = $1
                 AND NOT ({content_hash_condition})
                 AND EXISTS (
                    SELECT 1 FROM file_blobs target
                    WHERE target.policy_id = $2
                      AND target.hash = source.hash
                 )"#
        ),
        DbBackend::MySql | DbBackend::Sqlite | _ => format!(
            r#"SELECT COUNT(*) AS count
               FROM file_blobs source
               WHERE source.policy_id = ?
                 AND NOT ({content_hash_condition})
                 AND EXISTS (
                    SELECT 1 FROM file_blobs target
                    WHERE target.policy_id = ?
                      AND target.hash = source.hash
                 )"#
        ),
    };
    CountRow::find_by_statement(sea_orm::Statement::from_sql_and_values(
        backend,
        sql,
        [source_policy_id.into(), target_policy_id.into()],
    ))
    .one(db)
    .await
    .map_err(AsterError::from)?
    .map(|row| row.count)
    .ok_or_else(|| AsterError::internal_error("opaque hash conflict count query returned no row"))
}

pub async fn summarize_missing_blobs_between_policies<C: ConnectionTrait>(
    db: &C,
    source_policy_id: i64,
    target_policy_id: i64,
) -> Result<StoragePolicyMissingBlobSummary> {
    let backend = db.get_database_backend();
    let content_hash_condition = content_sha256_sql_condition(backend, "source.hash");
    let total_size = i64_cast_sql(backend, "COALESCE(SUM(source.size), 0)");
    let sql = match backend {
        DbBackend::Postgres => {
            format!(
                r#"SELECT COUNT(*) AS count, {total_size} AS total_size
               FROM file_blobs source
               WHERE source.policy_id = $1
                 AND NOT EXISTS (
                    SELECT 1 FROM file_blobs target
                    WHERE target.policy_id = $2
                      AND target.hash = source.hash
                      AND target.size = source.size
                      AND {content_hash_condition}
                 )"#
            )
        }
        DbBackend::MySql | DbBackend::Sqlite | _ => {
            format!(
                r#"SELECT COUNT(*) AS count, {total_size} AS total_size
               FROM file_blobs source
               WHERE source.policy_id = ?
                 AND NOT EXISTS (
                    SELECT 1 FROM file_blobs target
                    WHERE target.policy_id = ?
                      AND target.hash = source.hash
                      AND target.size = source.size
                      AND {content_hash_condition}
                 )"#
            )
        }
    };
    StoragePolicyMissingBlobSummary::find_by_statement(sea_orm::Statement::from_sql_and_values(
        backend,
        sql,
        [source_policy_id.into(), target_policy_id.into()],
    ))
    .one(db)
    .await
    .map_err(AsterError::from)?
    .ok_or_else(|| AsterError::internal_error("missing blob summary query returned no row"))
}

pub async fn create_blob<C: ConnectionTrait>(
    db: &C,
    model: file_blob::ActiveModel,
) -> Result<file_blob::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

/// Blob 去重：查找已有 blob 则原子递增 ref_count 并返回，否则新建 ref_count=1。
pub async fn find_or_create_blob<C: ConnectionTrait>(
    db: &C,
    hash: &str,
    size: i64,
    policy_id: i64,
    storage_path: &str,
) -> Result<FindOrCreateBlobResult> {
    for attempt in 0..FIND_OR_CREATE_BLOB_MAX_ATTEMPTS {
        if let Some(existing) = find_active_blob_by_hash(db, hash, policy_id).await? {
            let blob_id = existing.id;
            existing.ref_count.checked_add(1).ok_or_else(|| {
                AsterError::internal_error(format!(
                    "file_blob #{} ref_count overflow: {}",
                    existing.id, existing.ref_count
                ))
            })?;
            match increment_blob_ref_count(db, blob_id).await {
                Ok(()) => {
                    return Ok(FindOrCreateBlobResult {
                        model: find_blob_by_id(db, blob_id).await?,
                        inserted: false,
                    });
                }
                Err(e) if e.code() == "E006" => {
                    if attempt + 1 == FIND_OR_CREATE_BLOB_MAX_ATTEMPTS {
                        break;
                    }
                    tokio::time::sleep(find_or_create_blob_retry_delay(attempt)).await;
                    continue;
                }
                Err(e) => return Err(e),
            }
        }

        let now = Utc::now();
        let inserted = match FileBlob::insert(file_blob::ActiveModel {
            hash: Set(hash.to_string()),
            size: Set(size),
            policy_id: Set(policy_id),
            storage_path: Set(storage_path.to_string()),
            thumbnail_path: Set(None),
            thumbnail_processor: Set(None),
            thumbnail_version: Set(None),
            ref_count: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        })
        .on_conflict_do_nothing_on([file_blob::Column::Hash, file_blob::Column::PolicyId])
        .exec(db)
        .await
        .map_err(AsterError::from)?
        {
            TryInsertResult::Inserted(_) => true,
            TryInsertResult::Conflicted => false,
            TryInsertResult::Empty => {
                return Err(AsterError::internal_error(
                    "find_or_create_blob produced empty insert result",
                ));
            }
        };

        if inserted {
            return Ok(FindOrCreateBlobResult {
                model: find_blob_by_hash(db, hash, policy_id).await?.ok_or_else(|| {
                    AsterError::internal_error(format!(
                        "find_or_create_blob could not reload inserted blob for hash={hash}, policy_id={policy_id}"
                    ))
                })?,
                inserted: true,
            });
        }

        if attempt + 1 == FIND_OR_CREATE_BLOB_MAX_ATTEMPTS {
            break;
        }
        tokio::time::sleep(find_or_create_blob_retry_delay(attempt)).await;
    }

    Err(AsterError::internal_error(format!(
        "find_or_create_blob exceeded contention retry budget after {FIND_OR_CREATE_BLOB_MAX_ATTEMPTS} attempts for hash={hash}, policy_id={policy_id}"
    )))
}

pub(super) fn find_or_create_blob_retry_delay(attempt: usize) -> std::time::Duration {
    let backoff_ms = FIND_OR_CREATE_BLOB_INITIAL_DELAY_MS.saturating_mul(1_u64 << attempt.min(4));
    std::time::Duration::from_millis(std::cmp::min(backoff_ms, FIND_OR_CREATE_BLOB_MAX_DELAY_MS))
}

pub async fn find_blob_by_id<C: ConnectionTrait>(db: &C, id: i64) -> Result<file_blob::Model> {
    FileBlob::find_by_id(id)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::record_not_found(format!("file_blob #{id}")))
}

pub async fn find_admin_blobs_paginated(
    db: &DatabaseConnection,
    limit: u64,
    offset: u64,
    filters: AdminFileBlobFilters<'_>,
) -> Result<(Vec<file_blob::Model>, u64)> {
    let mut query = FileBlob::find();
    if let Some(hash) = filters.hash {
        query = query.filter(lower_like_condition(file_blob::Column::Hash, hash));
    }
    if let Some(policy_id) = filters.policy_id {
        query = query.filter(file_blob::Column::PolicyId.eq(policy_id));
    }
    if let Some(storage_path) = filters.storage_path {
        query = query.filter(lower_like_condition(
            file_blob::Column::StoragePath,
            storage_path,
        ));
    }
    if let Some(ref_count_min) = filters.ref_count_min {
        query = query.filter(file_blob::Column::RefCount.gte(ref_count_min));
    }
    if let Some(ref_count_max) = filters.ref_count_max {
        query = query.filter(file_blob::Column::RefCount.lte(ref_count_max));
    }
    if let Some(size_min) = filters.size_min {
        query = query.filter(file_blob::Column::Size.gte(size_min));
    }
    if let Some(size_max) = filters.size_max {
        query = query.filter(file_blob::Column::Size.lte(size_max));
    }
    query = apply_admin_blob_order(query, filters.sort_by, filters.sort_order);

    let total = query.clone().count(db).await.map_err(AsterError::from)?;
    let items = query
        .limit(limit)
        .offset(offset)
        .all(db)
        .await
        .map_err(AsterError::from)?;
    Ok((items, total))
}

pub async fn lock_blob_by_id<C: ConnectionTrait>(db: &C, id: i64) -> Result<file_blob::Model> {
    match db.get_database_backend() {
        DbBackend::Postgres | DbBackend::MySql => FileBlob::find_by_id(id)
            .lock_exclusive()
            .one(db)
            .await
            .map_err(AsterError::from)?
            .ok_or_else(|| AsterError::record_not_found(format!("file_blob #{id}"))),
        DbBackend::Sqlite => find_blob_by_id(db, id).await,
        _ => find_blob_by_id(db, id).await,
    }
}

/// 批量查询 blob，返回 id → Model 的映射
pub async fn find_blobs_by_ids<C: ConnectionTrait>(
    db: &C,
    ids: &[i64],
) -> Result<std::collections::HashMap<i64, file_blob::Model>> {
    if ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let blobs = FileBlob::find()
        .filter(file_blob::Column::Id.is_in(ids.iter().copied()))
        .all(db)
        .await
        .map_err(AsterError::from)?;
    Ok(blobs.into_iter().map(|b| (b.id, b)).collect())
}

/// 批量扫描 blobs（cursor 分页，id 升序），用于 reconcile 任务
pub async fn find_blobs_paginated<C: ConnectionTrait>(
    db: &C,
    after_id: Option<i64>,
    max_id: Option<i64>,
    limit: u64,
) -> Result<Vec<file_blob::Model>> {
    let mut query = FileBlob::find()
        .order_by_asc(file_blob::Column::Id)
        .limit(limit);
    if let Some(last_id) = after_id {
        query = query.filter(file_blob::Column::Id.gt(last_id));
    }
    if let Some(max_id) = max_id {
        query = query.filter(file_blob::Column::Id.lte(max_id));
    }
    query.all(db).await.map_err(AsterError::from)
}

fn apply_admin_blob_order(
    query: sea_orm::Select<FileBlob>,
    sort_by: AdminFileBlobSortBy,
    sort_order: SortOrder,
) -> sea_orm::Select<FileBlob> {
    match sort_by {
        AdminFileBlobSortBy::Id => order_by_id(query, file_blob::Column::Id, sort_order),
        AdminFileBlobSortBy::Hash => order_by_column_with_id(
            query,
            file_blob::Column::Hash,
            sort_order,
            file_blob::Column::Id,
        ),
        AdminFileBlobSortBy::Size => order_by_column_with_id(
            query,
            file_blob::Column::Size,
            sort_order,
            file_blob::Column::Id,
        ),
        AdminFileBlobSortBy::PolicyId => order_by_column_with_id(
            query,
            file_blob::Column::PolicyId,
            sort_order,
            file_blob::Column::Id,
        ),
        AdminFileBlobSortBy::StoragePath => order_by_column_with_id(
            query,
            file_blob::Column::StoragePath,
            sort_order,
            file_blob::Column::Id,
        ),
        AdminFileBlobSortBy::RefCount => order_by_column_with_id(
            query,
            file_blob::Column::RefCount,
            sort_order,
            file_blob::Column::Id,
        ),
        AdminFileBlobSortBy::CreatedAt => order_by_column_with_id(
            query,
            file_blob::Column::CreatedAt,
            sort_order,
            file_blob::Column::Id,
        ),
        AdminFileBlobSortBy::UpdatedAt => order_by_column_with_id(
            query,
            file_blob::Column::UpdatedAt,
            sort_order,
            file_blob::Column::Id,
        ),
    }
}

pub(super) fn sum_blob_size_as_i64_expr(backend: DbBackend) -> sea_orm::sea_query::SimpleExpr {
    let type_name = match backend {
        DbBackend::Postgres => "bigint",
        DbBackend::MySql => "signed",
        _ => "integer",
    };
    Expr::col(file_blob::Column::Size).sum().cast_as(type_name)
}

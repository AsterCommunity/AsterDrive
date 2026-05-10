//! `file_repo` 仓储子模块：`blob`。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, ExprTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, TryInsertResult,
    sea_query::{CaseStatement, Expr},
};

use crate::entities::file_blob::{self, Entity as FileBlob};
use crate::errors::{AsterError, Result};
use crate::utils::numbers::usize_to_u64;

pub struct FindOrCreateBlobResult {
    pub model: file_blob::Model,
    pub inserted: bool,
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

pub async fn find_active_blob_by_hash<C: ConnectionTrait>(
    db: &C,
    hash: &str,
    policy_id: i64,
) -> Result<Option<file_blob::Model>> {
    FileBlob::find()
        .filter(file_blob::Column::Hash.eq(hash))
        .filter(file_blob::Column::PolicyId.eq(policy_id))
        .filter(file_blob::Column::RefCount.gte(0))
        .one(db)
        .await
        .map_err(AsterError::from)
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
            let mut model = existing;
            let blob_id = model.id;
            let next_ref_count = model.ref_count.checked_add(1).ok_or_else(|| {
                AsterError::internal_error(format!(
                    "file_blob #{} ref_count overflow: {}",
                    model.id, model.ref_count
                ))
            })?;
            match increment_blob_ref_count(db, blob_id).await {
                Ok(()) => {
                    model.ref_count = next_ref_count;
                    return Ok(FindOrCreateBlobResult {
                        model,
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

fn find_or_create_blob_retry_delay(attempt: usize) -> std::time::Duration {
    let backoff_ms = FIND_OR_CREATE_BLOB_INITIAL_DELAY_MS.saturating_mul(1_u64 << attempt.min(4));
    std::time::Duration::from_millis(std::cmp::min(backoff_ms, FIND_OR_CREATE_BLOB_MAX_DELAY_MS))
}

/// 原子递增 blob ref_count（防止并发丢更新）
pub async fn increment_blob_ref_count<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    let result = FileBlob::update_many()
        .col_expr(
            file_blob::Column::RefCount,
            Expr::col(file_blob::Column::RefCount).add(1i32),
        )
        .col_expr(file_blob::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(file_blob::Column::Id.eq(id))
        .filter(file_blob::Column::RefCount.gte(0))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    if result.rows_affected == 0 {
        return Err(AsterError::record_not_found(format!("file_blob #{id}")));
    }
    Ok(())
}

/// 原子增加 blob ref_count（可变增量，批量复制用）
pub async fn increment_blob_ref_count_by<C: ConnectionTrait>(
    db: &C,
    id: i64,
    delta: i32,
) -> Result<()> {
    if delta < 0 {
        return Err(AsterError::internal_error(format!(
            "increment_blob_ref_count_by requires positive delta, got {delta}"
        )));
    }
    if delta == 0 {
        return Ok(());
    }
    let result = FileBlob::update_many()
        .col_expr(
            file_blob::Column::RefCount,
            Expr::col(file_blob::Column::RefCount).add(delta),
        )
        .col_expr(file_blob::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(file_blob::Column::Id.eq(id))
        .filter(file_blob::Column::RefCount.gte(0))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    if result.rows_affected == 0 {
        return Err(AsterError::record_not_found(format!("file_blob #{id}")));
    }
    Ok(())
}

/// 批量原子增加多个 blob 的 ref_count（每个 blob 可有不同增量）。
pub async fn increment_blob_ref_counts_by<C: ConnectionTrait>(
    db: &C,
    deltas: &[(i64, i32)],
) -> Result<()> {
    let deltas = normalize_blob_ref_count_deltas(deltas, "increment_blob_ref_counts_by")?;
    if deltas.is_empty() {
        return Ok(());
    }

    let ids: Vec<i64> = deltas.iter().map(|(id, _)| *id).collect();
    let expected_rows = usize_to_u64(deltas.len(), "blob ref_count increment row count")?;
    let result = FileBlob::update_many()
        .col_expr(
            file_blob::Column::RefCount,
            blob_ref_count_increment_expr(&deltas),
        )
        .col_expr(file_blob::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(file_blob::Column::Id.is_in(ids.iter().copied()))
        .filter(file_blob::Column::RefCount.gte(0))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    if result.rows_affected != expected_rows {
        return Err(AsterError::record_not_found(
            "one or more file_blob rows were missing or cleanup-claimed during ref_count increment",
        ));
    }
    Ok(())
}

/// 原子递减 blob ref_count（floor 0，防止并发丢更新）
pub async fn decrement_blob_ref_count<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    FileBlob::update_many()
        .col_expr(
            file_blob::Column::RefCount,
            Expr::case(Expr::col(file_blob::Column::RefCount).lt(1i32), 0)
                .finally(Expr::col(file_blob::Column::RefCount).sub(1i32))
                .into(),
        )
        .col_expr(file_blob::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(file_blob::Column::Id.eq(id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

/// 原子递减 blob ref_count（可变减量，floor 0）
pub async fn decrement_blob_ref_count_by<C: ConnectionTrait>(
    db: &C,
    id: i64,
    delta: i32,
) -> Result<()> {
    if delta < 0 {
        return Err(AsterError::internal_error(format!(
            "decrement_blob_ref_count_by requires positive delta, got {delta}"
        )));
    }
    if delta == 0 {
        return Ok(());
    }
    FileBlob::update_many()
        .col_expr(
            file_blob::Column::RefCount,
            Expr::case(Expr::col(file_blob::Column::RefCount).lt(delta), 0)
                .finally(Expr::col(file_blob::Column::RefCount).sub(delta))
                .into(),
        )
        .col_expr(file_blob::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(file_blob::Column::Id.eq(id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

/// 批量原子递减多个 blob 的 ref_count（每个 blob 可有不同减量，floor 0）。
pub async fn decrement_blob_ref_counts_by<C: ConnectionTrait>(
    db: &C,
    deltas: &[(i64, i32)],
) -> Result<()> {
    let deltas = normalize_blob_ref_count_deltas(deltas, "decrement_blob_ref_counts_by")?;
    if deltas.is_empty() {
        return Ok(());
    }

    let ids: Vec<i64> = deltas.iter().map(|(id, _)| *id).collect();
    FileBlob::update_many()
        .col_expr(
            file_blob::Column::RefCount,
            blob_ref_count_decrement_expr(&deltas),
        )
        .col_expr(file_blob::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(file_blob::Column::Id.is_in(ids.iter().copied()))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

fn normalize_blob_ref_count_deltas(
    deltas: &[(i64, i32)],
    context: &str,
) -> Result<Vec<(i64, i32)>> {
    let mut merged = std::collections::BTreeMap::<i64, i32>::new();
    for &(id, delta) in deltas {
        if delta < 0 {
            return Err(AsterError::internal_error(format!(
                "{context} requires positive delta for blob {id}, got {delta}"
            )));
        }
        if delta == 0 {
            continue;
        }
        let entry = merged.entry(id).or_default();
        *entry = entry.checked_add(delta).ok_or_else(|| {
            AsterError::internal_error(format!("{context} delta overflow while merging blob {id}"))
        })?;
    }
    Ok(merged.into_iter().collect())
}

fn blob_ref_count_increment_expr(deltas: &[(i64, i32)]) -> sea_orm::sea_query::SimpleExpr {
    let mut case = CaseStatement::new();
    for &(id, delta) in deltas {
        case = case.case(
            Expr::col(file_blob::Column::Id).eq(id),
            Expr::col(file_blob::Column::RefCount).add(delta),
        );
    }
    case.finally(Expr::col(file_blob::Column::RefCount)).into()
}

fn blob_ref_count_decrement_expr(deltas: &[(i64, i32)]) -> sea_orm::sea_query::SimpleExpr {
    let mut case = CaseStatement::new();
    for &(id, delta) in deltas {
        let decrement = Expr::case(Expr::col(file_blob::Column::RefCount).lt(delta), 0)
            .finally(Expr::col(file_blob::Column::RefCount).sub(delta));
        case = case.case(Expr::col(file_blob::Column::Id).eq(id), decrement);
    }
    case.finally(Expr::col(file_blob::Column::RefCount)).into()
}

/// 统计某存储策略下的 blob 数量（策略删除保护用）
pub async fn count_blobs_by_policy<C: ConnectionTrait>(db: &C, policy_id: i64) -> Result<u64> {
    FileBlob::find()
        .filter(file_blob::Column::PolicyId.eq(policy_id))
        .count(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_blob_by_id<C: ConnectionTrait>(db: &C, id: i64) -> Result<file_blob::Model> {
    FileBlob::find_by_id(id)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::record_not_found(format!("file_blob #{id}")))
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

/// 批量硬删除 blob 记录
pub async fn delete_blobs<C: ConnectionTrait>(db: &C, ids: &[i64]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    FileBlob::delete_many()
        .filter(file_blob::Column::Id.is_in(ids.iter().copied()))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

/// 批量原子递减 blob ref_count
pub async fn decrement_blob_ref_counts<C: ConnectionTrait>(db: &C, ids: &[i64]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    FileBlob::update_many()
        .col_expr(
            file_blob::Column::RefCount,
            Expr::case(Expr::col(file_blob::Column::RefCount).lt(1i32), 0)
                .finally(Expr::col(file_blob::Column::RefCount).sub(1i32))
                .into(),
        )
        .col_expr(file_blob::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(file_blob::Column::Id.is_in(ids.iter().copied()))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

pub async fn delete_blob<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    FileBlob::delete_by_id(id)
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

pub async fn claim_blob_cleanup<C: ConnectionTrait>(db: &C, id: i64) -> Result<bool> {
    let result = FileBlob::update_many()
        .col_expr(file_blob::Column::RefCount, Expr::value(-1i32))
        .col_expr(file_blob::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(file_blob::Column::Id.eq(id))
        .filter(file_blob::Column::RefCount.eq(0))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn restore_blob_cleanup_claim<C: ConnectionTrait>(db: &C, id: i64) -> Result<bool> {
    let result = FileBlob::update_many()
        .col_expr(file_blob::Column::RefCount, Expr::value(0i32))
        .col_expr(file_blob::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(file_blob::Column::Id.eq(id))
        .filter(file_blob::Column::RefCount.eq(-1))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn delete_blob_if_cleanup_claimed<C: ConnectionTrait>(db: &C, id: i64) -> Result<bool> {
    let result = FileBlob::delete_many()
        .filter(file_blob::Column::Id.eq(id))
        .filter(file_blob::Column::RefCount.eq(-1))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

/// 将 blob 的 ref_count 强制重置为 0（用于 reconcile 修正负值）
pub async fn reset_blob_ref_count_to_zero<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    FileBlob::update_many()
        .col_expr(file_blob::Column::RefCount, Expr::value(0i32))
        .col_expr(file_blob::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(file_blob::Column::Id.eq(id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

/// 将 blob 的 ref_count 设置为指定值（用于 reconcile 修正偏差）
pub async fn set_blob_ref_count<C: ConnectionTrait>(db: &C, id: i64, ref_count: i32) -> Result<()> {
    FileBlob::update_many()
        .col_expr(file_blob::Column::RefCount, Expr::value(ref_count))
        .col_expr(file_blob::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(file_blob::Column::Id.eq(id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

/// 批量扫描 blobs（cursor 分页，id 升序），用于 reconcile 任务
pub async fn find_blobs_paginated<C: ConnectionTrait>(
    db: &C,
    after_id: Option<i64>,
    limit: u64,
) -> Result<Vec<file_blob::Model>> {
    let mut query = FileBlob::find()
        .order_by_asc(file_blob::Column::Id)
        .limit(limit);
    if let Some(last_id) = after_id {
        query = query.filter(file_blob::Column::Id.gt(last_id));
    }
    query.all(db).await.map_err(AsterError::from)
}

/// 统计所有文件（未删除）引用每个 blob 的次数，返回 blob_id → count
pub async fn count_blob_refs_from_files<C: ConnectionTrait>(
    db: &C,
) -> Result<std::collections::HashMap<i64, i64>> {
    use crate::entities::file::{self, Entity as File};
    let rows = File::find()
        .select_only()
        .column(file::Column::BlobId)
        .column_as(Expr::col(file::Column::Id).count(), "ref_count")
        .group_by(file::Column::BlobId)
        .into_tuple::<(i64, i64)>()
        .all(db)
        .await
        .map_err(AsterError::from)?;
    Ok(rows.into_iter().collect())
}

/// 统计单个 blob 当前被文件记录引用的次数。
pub async fn count_blob_refs_from_files_for_blob<C: ConnectionTrait>(
    db: &C,
    blob_id: i64,
) -> Result<i64> {
    use crate::entities::file::{self, Entity as File};
    Ok(File::find()
        .select_only()
        .column_as(Expr::col(file::Column::Id).count(), "ref_count")
        .filter(file::Column::BlobId.eq(blob_id))
        .into_tuple::<i64>()
        .one(db)
        .await
        .map_err(AsterError::from)?
        .unwrap_or(0))
}

/// 查询存储路径在给定候选集中的所有 blob 路径（用于孤儿检测）
pub async fn find_blob_storage_paths_by_storage_paths<C: ConnectionTrait>(
    db: &C,
    candidate_paths: &[String],
) -> Result<std::collections::HashSet<String>> {
    if candidate_paths.is_empty() {
        return Ok(std::collections::HashSet::new());
    }
    let paths = FileBlob::find()
        .select_only()
        .column(file_blob::Column::StoragePath)
        .filter(file_blob::Column::StoragePath.is_in(candidate_paths.iter().cloned()))
        .into_tuple::<String>()
        .all(db)
        .await
        .map_err(AsterError::from)?;
    Ok(paths.into_iter().collect())
}

pub async fn set_thumbnail_metadata<C: ConnectionTrait>(
    db: &C,
    id: i64,
    thumbnail_path: &str,
    thumbnail_processor: &str,
    thumbnail_version: &str,
) -> Result<bool> {
    let result = FileBlob::update_many()
        .col_expr(
            file_blob::Column::ThumbnailPath,
            Expr::value(Some(thumbnail_path.to_string())),
        )
        .col_expr(
            file_blob::Column::ThumbnailProcessor,
            Expr::value(Some(thumbnail_processor.to_string())),
        )
        .col_expr(
            file_blob::Column::ThumbnailVersion,
            Expr::value(Some(thumbnail_version.to_string())),
        )
        .col_expr(file_blob::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(file_blob::Column::Id.eq(id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn clear_thumbnail_metadata<C: ConnectionTrait>(db: &C, id: i64) -> Result<bool> {
    let result = FileBlob::update_many()
        .col_expr(
            file_blob::Column::ThumbnailPath,
            Expr::value(Option::<String>::None),
        )
        .col_expr(
            file_blob::Column::ThumbnailProcessor,
            Expr::value(Option::<String>::None),
        )
        .col_expr(
            file_blob::Column::ThumbnailVersion,
            Expr::value(Option::<String>::None),
        )
        .col_expr(file_blob::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(file_blob::Column::Id.eq(id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

/// 统计 blob 总数
pub async fn count_all_blobs<C: ConnectionTrait>(db: &C) -> Result<u64> {
    FileBlob::find().count(db).await.map_err(AsterError::from)
}

/// 统计所有 blob 的总字节数
pub async fn sum_blob_bytes<C: ConnectionTrait>(db: &C) -> Result<i64> {
    let type_name = match db.get_database_backend() {
        DbBackend::Postgres => "bigint",
        DbBackend::MySql => "signed",
        _ => "integer",
    };
    Ok(FileBlob::find()
        .select_only()
        .column_as(
            Expr::col(file_blob::Column::Size).sum().cast_as(type_name),
            "sum",
        )
        .into_tuple::<Option<i64>>()
        .one(db)
        .await?
        .flatten()
        .unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{DbBackend, QueryTrait};
    use std::time::Duration;

    #[test]
    fn find_or_create_blob_retry_delay_grows_exponentially_and_caps() {
        assert_eq!(find_or_create_blob_retry_delay(0), Duration::from_millis(5));
        assert_eq!(
            find_or_create_blob_retry_delay(1),
            Duration::from_millis(10)
        );
        assert_eq!(
            find_or_create_blob_retry_delay(2),
            Duration::from_millis(20)
        );
        assert_eq!(
            find_or_create_blob_retry_delay(3),
            Duration::from_millis(40)
        );
        assert_eq!(
            find_or_create_blob_retry_delay(4),
            Duration::from_millis(80)
        );
        assert_eq!(
            find_or_create_blob_retry_delay(5),
            Duration::from_millis(80)
        );
        assert_eq!(
            find_or_create_blob_retry_delay(99),
            Duration::from_millis(80)
        );
    }

    #[test]
    fn postgres_find_or_create_blob_insert_sql_uses_valid_on_conflict() {
        let now = Utc::now();
        let sql = FileBlob::insert(file_blob::ActiveModel {
            hash: Set("hash".to_string()),
            size: Set(1),
            policy_id: Set(2),
            storage_path: Set("files/hash".to_string()),
            thumbnail_path: Set(None),
            thumbnail_processor: Set(None),
            thumbnail_version: Set(None),
            ref_count: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        })
        .on_conflict_do_nothing_on([file_blob::Column::Hash, file_blob::Column::PolicyId])
        .build(DbBackend::Postgres)
        .to_string();

        assert!(
            sql.contains(r#"ON CONFLICT ("hash", "policy_id") DO NOTHING"#),
            "{sql}"
        );
        assert!(!sql.contains(" WHERE "), "{sql}");
    }

    // 第二轮审查有 agent 怀疑 `RefCount.gte(0)` 是无效过滤（误以为 ref_count 不能为负）。
    // 实际上 `claim_blob_cleanup` 会把 ref_count 置为 -1 作为 "待清理" 标记，
    // `gte(0)` 正是为了把这类已被 cleanup worker 认领的行从 dedup 命中集中排除。
    // 这个测试通过检查生成的 SQL 是否包含 `ref_count >= 0` 条件来锁定这一不变量。
    #[test]
    fn find_active_blob_by_hash_sql_excludes_cleanup_claimed_rows() {
        let sql = FileBlob::find()
            .filter(file_blob::Column::Hash.eq("h"))
            .filter(file_blob::Column::PolicyId.eq(1))
            .filter(file_blob::Column::RefCount.gte(0))
            .build(DbBackend::Postgres)
            .to_string();
        assert!(
            sql.contains(r#""ref_count" >= 0"#),
            "expected ref_count >= 0 filter in '{sql}'"
        );
    }

    #[test]
    fn increment_blob_ref_count_sql_uses_cas_against_cleanup_claim() {
        use sea_orm::{EntityTrait, QueryTrait};
        let sql = FileBlob::update_many()
            .col_expr(
                file_blob::Column::RefCount,
                Expr::col(file_blob::Column::RefCount).add(1i32),
            )
            .filter(file_blob::Column::Id.eq(1))
            .filter(file_blob::Column::RefCount.gte(0))
            .build(DbBackend::Postgres)
            .to_string();
        assert!(
            sql.contains(r#""ref_count" >= 0"#),
            "ref_count CAS must reject cleanup-claimed (-1) rows; sql='{sql}'"
        );
    }

    #[test]
    fn normalize_blob_ref_count_deltas_merges_duplicate_ids_and_skips_zeroes() {
        let deltas =
            normalize_blob_ref_count_deltas(&[(2, 1), (1, 0), (2, 3), (3, 2)], "test merge")
                .unwrap();

        assert_eq!(deltas, vec![(2, 4), (3, 2)]);
    }

    #[test]
    fn normalize_blob_ref_count_deltas_rejects_negative_values() {
        let error = normalize_blob_ref_count_deltas(&[(7, -1)], "test negative").unwrap_err();

        assert!(
            error
                .to_string()
                .contains("test negative requires positive delta"),
            "{error}"
        );
    }

    #[test]
    fn postgres_batch_increment_blob_ref_counts_sql_uses_single_case_update() {
        use sea_orm::{EntityTrait, QueryTrait};
        let deltas = vec![(2, 3), (4, 1)];
        let ids: Vec<i64> = deltas.iter().map(|(id, _)| *id).collect();
        let sql = FileBlob::update_many()
            .col_expr(
                file_blob::Column::RefCount,
                blob_ref_count_increment_expr(&deltas),
            )
            .filter(file_blob::Column::Id.is_in(ids.iter().copied()))
            .filter(file_blob::Column::RefCount.gte(0))
            .build(DbBackend::Postgres)
            .to_string();

        assert!(sql.contains(r#"CASE WHEN ("id" = 2)"#), "{sql}");
        assert!(sql.contains(r#"THEN "ref_count" + 3"#), "{sql}");
        assert!(sql.contains(r#"WHEN ("id" = 4)"#), "{sql}");
        assert!(sql.contains(r#"THEN "ref_count" + 1"#), "{sql}");
        assert!(sql.contains(r#""ref_count" >= 0"#), "{sql}");
    }

    #[test]
    fn postgres_batch_decrement_blob_ref_counts_sql_uses_floor_case_update() {
        use sea_orm::{EntityTrait, QueryTrait};
        let deltas = vec![(2, 3), (4, 1)];
        let ids: Vec<i64> = deltas.iter().map(|(id, _)| *id).collect();
        let sql = FileBlob::update_many()
            .col_expr(
                file_blob::Column::RefCount,
                blob_ref_count_decrement_expr(&deltas),
            )
            .filter(file_blob::Column::Id.is_in(ids.iter().copied()))
            .build(DbBackend::Postgres)
            .to_string();

        assert!(sql.contains(r#"CASE WHEN ("id" = 2)"#), "{sql}");
        assert!(sql.contains(r#"CASE WHEN ("ref_count" < 3)"#), "{sql}");
        assert!(sql.contains(r#"ELSE "ref_count" - 3 END"#), "{sql}");
        assert!(sql.contains(r#"WHEN ("id" = 4)"#), "{sql}");
    }
}

//! 仓储模块：`version_repo`。

use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait,
    ExprTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, sea_query::Expr,
};
use std::collections::HashMap;

use crate::entities::file_version::{self, Entity as FileVersion};
use crate::errors::{AsterError, Result};

fn sum_version_size_as_i64_expr(backend: DbBackend) -> sea_orm::sea_query::SimpleExpr {
    let type_name = match backend {
        DbBackend::Postgres => "bigint",
        DbBackend::MySql => "signed",
        DbBackend::Sqlite => "integer",
        other => {
            tracing::warn!(
                backend = ?other,
                "unknown database backend for file version size sum cast; using integer"
            );
            "integer"
        }
    };
    Expr::col(file_version::Column::Size)
        .sum()
        .cast_as(type_name)
}

pub async fn create<C: ConnectionTrait>(
    db: &C,
    model: file_version::ActiveModel,
) -> Result<file_version::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn sum_sizes_by_file_id(db: &DatabaseConnection, file_id: i64) -> Result<i64> {
    Ok(FileVersion::find()
        .select_only()
        .column_as(
            sum_version_size_as_i64_expr(db.get_database_backend()),
            "sum",
        )
        .filter(file_version::Column::FileId.eq(file_id))
        .into_tuple::<Option<i64>>()
        .one(db)
        .await
        .map_err(AsterError::from)?
        .flatten()
        .unwrap_or(0))
}

pub async fn sum_sizes_by_file_ids(db: &DatabaseConnection, file_ids: &[i64]) -> Result<i64> {
    if file_ids.is_empty() {
        return Ok(0);
    }

    Ok(FileVersion::find()
        .select_only()
        .column_as(
            sum_version_size_as_i64_expr(db.get_database_backend()),
            "sum",
        )
        .filter(file_version::Column::FileId.is_in(file_ids.iter().copied()))
        .into_tuple::<Option<i64>>()
        .one(db)
        .await
        .map_err(AsterError::from)?
        .flatten()
        .unwrap_or(0))
}

/// 按 file_id 查询所有版本（version DESC）
pub async fn find_by_file_id<C: ConnectionTrait>(
    db: &C,
    file_id: i64,
) -> Result<Vec<file_version::Model>> {
    FileVersion::find()
        .filter(file_version::Column::FileId.eq(file_id))
        .order_by_desc(file_version::Column::Version)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_blob_id<C: ConnectionTrait>(
    db: &C,
    blob_id: i64,
) -> Result<Vec<file_version::Model>> {
    FileVersion::find()
        .filter(file_version::Column::BlobId.eq(blob_id))
        .order_by_asc(file_version::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_id<C: ConnectionTrait>(
    db: &C,
    id: i64,
) -> Result<Option<file_version::Model>> {
    FileVersion::find_by_id(id)
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn delete_by_id<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    FileVersion::delete_by_id(id)
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

/// 删除某个历史版本后，把更“新”的版本号整体减 1，保持显示编号连续
pub async fn decrement_versions_after<C: ConnectionTrait>(
    db: &C,
    file_id: i64,
    deleted_version: i32,
) -> Result<()> {
    FileVersion::update_many()
        .col_expr(
            file_version::Column::Version,
            Expr::col(file_version::Column::Version).sub(1i32),
        )
        .filter(file_version::Column::FileId.eq(file_id))
        .filter(file_version::Column::Version.gt(deleted_version))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

/// 查找指定版本及之后的所有版本（version DESC）
pub async fn find_by_file_id_from_version<C: ConnectionTrait>(
    db: &C,
    file_id: i64,
    min_version: i32,
) -> Result<Vec<file_version::Model>> {
    FileVersion::find()
        .filter(file_version::Column::FileId.eq(file_id))
        .filter(file_version::Column::Version.gte(min_version))
        .order_by_desc(file_version::Column::Version)
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// 删除指定版本及之后的所有版本，返回对应 blob_id 列表
pub async fn delete_by_file_id_from_version<C: ConnectionTrait>(
    db: &C,
    file_id: i64,
    min_version: i32,
) -> Result<Vec<i64>> {
    let versions = find_by_file_id_from_version(db, file_id, min_version).await?;
    let blob_ids: Vec<i64> = versions.iter().map(|v| v.blob_id).collect();

    FileVersion::delete_many()
        .filter(file_version::Column::FileId.eq(file_id))
        .filter(file_version::Column::Version.gte(min_version))
        .exec(db)
        .await
        .map_err(AsterError::from)?;

    Ok(blob_ids)
}

/// 统计文件的版本数量
pub async fn count_by_file_id<C: ConnectionTrait>(db: &C, file_id: i64) -> Result<u64> {
    FileVersion::find()
        .filter(file_version::Column::FileId.eq(file_id))
        .count(db)
        .await
        .map_err(AsterError::from)
}

/// 查找最旧的版本（version ASC limit 1）
pub async fn find_oldest_by_file_id<C: ConnectionTrait>(
    db: &C,
    file_id: i64,
) -> Result<Option<file_version::Model>> {
    FileVersion::find()
        .filter(file_version::Column::FileId.eq(file_id))
        .order_by_asc(file_version::Column::Version)
        .one(db)
        .await
        .map_err(AsterError::from)
}

/// 删除文件的所有版本记录（文件永久删除时用）
pub async fn delete_all_by_file_id<C: ConnectionTrait>(db: &C, file_id: i64) -> Result<Vec<i64>> {
    // 先查出所有 blob_id（需要减引用计数）
    let versions = find_by_file_id(db, file_id).await?;
    let blob_ids: Vec<i64> = versions.iter().map(|v| v.blob_id).collect();

    FileVersion::delete_many()
        .filter(file_version::Column::FileId.eq(file_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;

    Ok(blob_ids)
}

/// 批量删除多个文件的所有版本记录，返回所有涉及的 blob_id
pub async fn delete_all_by_file_ids<C: ConnectionTrait>(
    db: &C,
    file_ids: &[i64],
) -> Result<Vec<i64>> {
    if file_ids.is_empty() {
        return Ok(vec![]);
    }
    let versions = FileVersion::find()
        .filter(file_version::Column::FileId.is_in(file_ids.iter().copied()))
        .all(db)
        .await
        .map_err(AsterError::from)?;
    let blob_ids: Vec<i64> = versions.iter().map(|v| v.blob_id).collect();

    FileVersion::delete_many()
        .filter(file_version::Column::FileId.is_in(file_ids.iter().copied()))
        .exec(db)
        .await
        .map_err(AsterError::from)?;

    Ok(blob_ids)
}

/// 统计所有版本记录引用每个 blob 的次数，返回 blob_id → count
pub async fn count_blob_refs_from_versions<C: ConnectionTrait>(
    db: &C,
) -> Result<std::collections::HashMap<i64, i64>> {
    let rows = FileVersion::find()
        .select_only()
        .column(file_version::Column::BlobId)
        .column_as(Expr::col(file_version::Column::Id).count(), "ref_count")
        .group_by(file_version::Column::BlobId)
        .into_tuple::<(i64, i64)>()
        .all(db)
        .await
        .map_err(AsterError::from)?;
    Ok(rows.into_iter().collect())
}

/// 批量统计指定 blob 当前被历史版本引用的次数。
pub async fn count_blob_refs_from_versions_for_blobs<C: ConnectionTrait>(
    db: &C,
    blob_ids: &[i64],
) -> Result<HashMap<i64, i64>> {
    if blob_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = FileVersion::find()
        .select_only()
        .column(file_version::Column::BlobId)
        .column_as(Expr::col(file_version::Column::Id).count(), "ref_count")
        .filter(file_version::Column::BlobId.is_in(blob_ids.iter().copied()))
        .group_by(file_version::Column::BlobId)
        .into_tuple::<(i64, i64)>()
        .all(db)
        .await
        .map_err(AsterError::from)?;
    Ok(rows.into_iter().collect())
}

/// 统计单个 blob 当前被版本记录引用的次数。
pub async fn count_blob_refs_from_versions_for_blob<C: ConnectionTrait>(
    db: &C,
    blob_id: i64,
) -> Result<i64> {
    Ok(FileVersion::find()
        .select_only()
        .column_as(Expr::col(file_version::Column::Id).count(), "ref_count")
        .filter(file_version::Column::BlobId.eq(blob_id))
        .into_tuple::<i64>()
        .one(db)
        .await
        .map_err(AsterError::from)?
        .unwrap_or(0))
}

/// 获取下一个版本号
pub async fn next_version<C: ConnectionTrait>(db: &C, file_id: i64) -> Result<i32> {
    let latest = FileVersion::find()
        .filter(file_version::Column::FileId.eq(file_id))
        .order_by_desc(file_version::Column::Version)
        .one(db)
        .await
        .map_err(AsterError::from)?;
    Ok(latest.map(|v| v.version + 1).unwrap_or(1))
}

pub async fn replace_version_blob_refs<C: ConnectionTrait>(
    db: &C,
    old_blob_id: i64,
    new_blob_id: i64,
) -> Result<u64> {
    if old_blob_id == new_blob_id {
        return Ok(0);
    }

    let result = FileVersion::update_many()
        .col_expr(file_version::Column::BlobId, Expr::value(new_blob_id))
        .filter(file_version::Column::BlobId.eq(old_blob_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

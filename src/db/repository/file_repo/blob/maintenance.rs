use chrono::Utc;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, ExprTrait, PaginatorTrait, QueryFilter,
    QuerySelect, sea_query::Expr,
};
use std::collections::HashMap;

use crate::entities::file_blob::{self, Entity as FileBlob};
use crate::errors::{AsterError, Result};

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

/// 统计某个 blob 被所有文件引用的次数。
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

/// 批量统计指定 blob 当前被文件记录引用的次数。
pub async fn count_blob_refs_from_files_for_blobs<C: ConnectionTrait>(
    db: &C,
    blob_ids: &[i64],
) -> Result<HashMap<i64, i64>> {
    use crate::entities::file::{self, Entity as File};
    if blob_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = File::find()
        .select_only()
        .column(file::Column::BlobId)
        .column_as(Expr::col(file::Column::Id).count(), "ref_count")
        .filter(file::Column::BlobId.is_in(blob_ids.iter().copied()))
        .group_by(file::Column::BlobId)
        .into_tuple::<(i64, i64)>()
        .all(db)
        .await
        .map_err(AsterError::from)?;
    Ok(rows.into_iter().collect())
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

pub async fn blob_storage_path_exists_for_policy<C: ConnectionTrait>(
    db: &C,
    policy_id: i64,
    storage_path: &str,
) -> Result<bool> {
    let count = FileBlob::find()
        .filter(file_blob::Column::PolicyId.eq(policy_id))
        .filter(file_blob::Column::StoragePath.eq(storage_path))
        .count(db)
        .await
        .map_err(AsterError::from)?;
    Ok(count > 0)
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

pub async fn sum_blob_bytes_by_policy<C: ConnectionTrait>(db: &C, policy_id: i64) -> Result<i64> {
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
        .filter(file_blob::Column::PolicyId.eq(policy_id))
        .into_tuple::<Option<i64>>()
        .one(db)
        .await?
        .flatten()
        .unwrap_or(0))
}

pub async fn move_blob_policy_if_current<C: ConnectionTrait>(
    db: &C,
    blob_id: i64,
    source_policy_id: i64,
    target_policy_id: i64,
    target_path: &str,
) -> Result<bool> {
    let result = FileBlob::update_many()
        .col_expr(file_blob::Column::PolicyId, Expr::value(target_policy_id))
        .col_expr(
            file_blob::Column::StoragePath,
            Expr::value(target_path.to_string()),
        )
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
        .filter(file_blob::Column::Id.eq(blob_id))
        .filter(file_blob::Column::PolicyId.eq(source_policy_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn delete_blob_by_id<C: ConnectionTrait>(db: &C, blob_id: i64) -> Result<bool> {
    let result = FileBlob::delete_by_id(blob_id)
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

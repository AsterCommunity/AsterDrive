use chrono::Utc;
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter};

use crate::entities::file_blob::{self, Entity as FileBlob};
use crate::errors::{AsterError, Result};

pub const BLOB_CLEANUP_CLAIMED_REF_COUNT: i32 = -1;

/// 统计某存储策略下的 blob 数量（策略删除保护用）
pub async fn count_blobs_by_policy<C: ConnectionTrait>(db: &C, policy_id: i64) -> Result<u64> {
    FileBlob::find()
        .filter(file_blob::Column::PolicyId.eq(policy_id))
        .count(db)
        .await
        .map_err(AsterError::from)
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

pub async fn delete_blob<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    FileBlob::delete_by_id(id)
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

pub async fn claim_blob_cleanup<C: ConnectionTrait>(db: &C, id: i64) -> Result<bool> {
    let result = FileBlob::update_many()
        .col_expr(
            file_blob::Column::RefCount,
            sea_orm::sea_query::Expr::value(BLOB_CLEANUP_CLAIMED_REF_COUNT),
        )
        .col_expr(
            file_blob::Column::UpdatedAt,
            sea_orm::sea_query::Expr::value(Utc::now()),
        )
        .filter(file_blob::Column::Id.eq(id))
        .filter(file_blob::Column::RefCount.eq(0))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn restore_blob_cleanup_claim<C: ConnectionTrait>(db: &C, id: i64) -> Result<bool> {
    let result = FileBlob::update_many()
        .col_expr(
            file_blob::Column::RefCount,
            sea_orm::sea_query::Expr::value(0i32),
        )
        .col_expr(
            file_blob::Column::UpdatedAt,
            sea_orm::sea_query::Expr::value(Utc::now()),
        )
        .filter(file_blob::Column::Id.eq(id))
        .filter(file_blob::Column::RefCount.eq(BLOB_CLEANUP_CLAIMED_REF_COUNT))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn delete_blob_if_cleanup_claimed<C: ConnectionTrait>(db: &C, id: i64) -> Result<bool> {
    let result = FileBlob::delete_many()
        .filter(file_blob::Column::Id.eq(id))
        .filter(file_blob::Column::RefCount.eq(BLOB_CLEANUP_CLAIMED_REF_COUNT))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

/// 将 blob 的 ref_count 强制重置为 0（用于 reconcile 修正负值）
pub async fn reset_blob_ref_count_to_zero<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    FileBlob::update_many()
        .col_expr(
            file_blob::Column::RefCount,
            sea_orm::sea_query::Expr::value(0i32),
        )
        .col_expr(
            file_blob::Column::UpdatedAt,
            sea_orm::sea_query::Expr::value(Utc::now()),
        )
        .filter(file_blob::Column::Id.eq(id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

/// 将 blob 的 ref_count 设置为指定值（用于 reconcile 修正偏差）
pub async fn set_blob_ref_count<C: ConnectionTrait>(db: &C, id: i64, ref_count: i32) -> Result<()> {
    FileBlob::update_many()
        .col_expr(
            file_blob::Column::RefCount,
            sea_orm::sea_query::Expr::value(ref_count),
        )
        .col_expr(
            file_blob::Column::UpdatedAt,
            sea_orm::sea_query::Expr::value(Utc::now()),
        )
        .filter(file_blob::Column::Id.eq(id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

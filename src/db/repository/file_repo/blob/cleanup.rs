use chrono::Utc;
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, ExprTrait, PaginatorTrait, QueryFilter};

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::file_blob::{self, Entity as FileBlob};

use super::ref_count::cleanup_pending_since_expr;

/// Sentinel `ref_count` marking a blob whose physical deletion has been claimed by a
/// cleanup worker; normal live blobs always hold a non-negative count.
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

/// Hard-deletes a single blob record; succeeds even when the row is absent.
pub async fn delete_blob<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    FileBlob::delete_by_id(id)
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

/// Claims the physical deletion of a zero-reference blob in a single conditional
/// UPDATE. The row wins the claim only if its `ref_count` is still exactly 0, guarding
/// against a concurrent upload adopting it. On success the ref count moves to
/// [`BLOB_CLEANUP_CLAIMED_REF_COUNT`] and `cleanup_pending_since` is stamped with the
/// current time when still unset. Returns true when this call performed the claim.
pub async fn claim_blob_cleanup<C: ConnectionTrait>(db: &C, id: i64) -> Result<bool> {
    let result = FileBlob::update_many()
        .col_expr(
            file_blob::Column::CleanupPendingSince,
            sea_orm::sea_query::Expr::col(file_blob::Column::CleanupPendingSince)
                .if_null(sea_orm::sea_query::Expr::value(Utc::now())),
        )
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

/// Retire only the immutable physical generation whose deletion was claimed.
pub async fn delete_claimed_blob_generation<C: ConnectionTrait>(
    db: &C,
    blob: &file_blob::Model,
) -> Result<bool> {
    let query = FileBlob::delete_many()
        .filter(file_blob::Column::Id.eq(blob.id))
        .filter(file_blob::Column::RefCount.eq(BLOB_CLEANUP_CLAIMED_REF_COUNT));
    let query = match blob.storage_path.as_deref() {
        Some(path) => query.filter(file_blob::Column::StoragePath.eq(path)),
        None => query.filter(file_blob::Column::StoragePath.is_null()),
    };
    Ok(query
        .exec(db)
        .await
        .map_err(AsterError::from)?
        .rows_affected
        == 1)
}

/// Renew a retry on the same claimed generation without reopening it for adoption.
pub(crate) async fn renew_blob_cleanup_attempt<C: ConnectionTrait>(
    db: &C,
    blob: &file_blob::Model,
) -> Result<bool> {
    let query = FileBlob::update_many()
        .col_expr(
            file_blob::Column::UpdatedAt,
            sea_orm::sea_query::Expr::value(Utc::now()),
        )
        .filter(file_blob::Column::Id.eq(blob.id))
        .filter(file_blob::Column::RefCount.eq(BLOB_CLEANUP_CLAIMED_REF_COUNT))
        .filter(file_blob::Column::UpdatedAt.eq(blob.updated_at));
    let query = match blob.storage_path.as_deref() {
        Some(path) => query.filter(file_blob::Column::StoragePath.eq(path)),
        None => query.filter(file_blob::Column::StoragePath.is_null()),
    };
    Ok(query
        .exec(db)
        .await
        .map_err(AsterError::from)?
        .rows_affected
        == 1)
}

/// 将 blob 的 ref_count 设置为指定值（用于 reconcile 修正偏差）
pub async fn set_blob_ref_count<C: ConnectionTrait>(
    db: &C,
    id: i64,
    ref_count: i32,
) -> Result<bool> {
    let result = FileBlob::update_many()
        .col_expr(
            file_blob::Column::CleanupPendingSince,
            cleanup_pending_since_expr(sea_orm::sea_query::Expr::value(ref_count)),
        )
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
    Ok(result.rows_affected > 0)
}

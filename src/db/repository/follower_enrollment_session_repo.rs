//! 仓储模块：`follower_enrollment_session_repo`。

use crate::entities::follower_enrollment_session::{self, Entity as FollowerEnrollmentSession};
use crate::errors::{AsterError, Result};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, sea_query::Expr,
};

pub async fn create<C: ConnectionTrait>(
    db: &C,
    model: follower_enrollment_session::ActiveModel,
) -> Result<follower_enrollment_session::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn find_by_token_hash<C: ConnectionTrait>(
    db: &C,
    token_hash: &str,
) -> Result<Option<follower_enrollment_session::Model>> {
    FollowerEnrollmentSession::find()
        .filter(follower_enrollment_session::Column::TokenHash.eq(token_hash))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_ack_token_hash<C: ConnectionTrait>(
    db: &C,
    ack_token_hash: &str,
) -> Result<Option<follower_enrollment_session::Model>> {
    FollowerEnrollmentSession::find()
        .filter(follower_enrollment_session::Column::AckTokenHash.eq(ack_token_hash))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_latest_for_managed_follower(
    db: &DatabaseConnection,
    managed_follower_id: i64,
) -> Result<Option<follower_enrollment_session::Model>> {
    FollowerEnrollmentSession::find()
        .filter(follower_enrollment_session::Column::ManagedFollowerId.eq(managed_follower_id))
        .order_by_desc(follower_enrollment_session::Column::CreatedAt)
        .order_by_desc(follower_enrollment_session::Column::Id)
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_managed_follower_ids(
    db: &DatabaseConnection,
    managed_follower_ids: &[i64],
) -> Result<Vec<follower_enrollment_session::Model>> {
    if managed_follower_ids.is_empty() {
        return Ok(vec![]);
    }

    FollowerEnrollmentSession::find()
        .filter(
            follower_enrollment_session::Column::ManagedFollowerId
                .is_in(managed_follower_ids.iter().copied()),
        )
        .order_by_asc(follower_enrollment_session::Column::ManagedFollowerId)
        .order_by_desc(follower_enrollment_session::Column::CreatedAt)
        .order_by_desc(follower_enrollment_session::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn has_completed_for_managed_follower(
    db: &DatabaseConnection,
    managed_follower_id: i64,
) -> Result<bool> {
    let completed = FollowerEnrollmentSession::find()
        .filter(follower_enrollment_session::Column::ManagedFollowerId.eq(managed_follower_id))
        .filter(follower_enrollment_session::Column::AckedAt.is_not_null())
        .count(db)
        .await
        .map_err(AsterError::from)?;
    Ok(completed > 0)
}

pub async fn invalidate_pending_for_managed_follower<C: ConnectionTrait>(
    db: &C,
    managed_follower_id: i64,
) -> Result<u64> {
    let now = Utc::now();
    let result = FollowerEnrollmentSession::update_many()
        .col_expr(
            follower_enrollment_session::Column::InvalidatedAt,
            Expr::value(Some(now)),
        )
        .filter(follower_enrollment_session::Column::ManagedFollowerId.eq(managed_follower_id))
        .filter(follower_enrollment_session::Column::AckedAt.is_null())
        .filter(follower_enrollment_session::Column::InvalidatedAt.is_null())
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

pub async fn mark_redeemed_if_needed<C: ConnectionTrait>(db: &C, session_id: i64) -> Result<()> {
    FollowerEnrollmentSession::update_many()
        .col_expr(
            follower_enrollment_session::Column::RedeemedAt,
            Expr::value(Some(Utc::now())),
        )
        .filter(follower_enrollment_session::Column::Id.eq(session_id))
        .filter(follower_enrollment_session::Column::RedeemedAt.is_null())
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

pub async fn mark_acked<C: ConnectionTrait>(db: &C, session_id: i64) -> Result<bool> {
    let result = FollowerEnrollmentSession::update_many()
        .col_expr(
            follower_enrollment_session::Column::AckedAt,
            Expr::value(Some(Utc::now())),
        )
        .filter(follower_enrollment_session::Column::Id.eq(session_id))
        .filter(follower_enrollment_session::Column::AckedAt.is_null())
        .filter(follower_enrollment_session::Column::InvalidatedAt.is_null())
        .filter(follower_enrollment_session::Column::RedeemedAt.is_not_null())
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

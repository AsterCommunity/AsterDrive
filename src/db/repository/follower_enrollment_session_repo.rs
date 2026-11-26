//! 仓储模块：`follower_enrollment_session_repo`。

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::follower_enrollment_session::{
    self, Entity as FollowerEnrollmentSession,
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, sea_query::Expr,
};

/// Inserts a new enrollment session from the given active model.
pub async fn create<C: ConnectionTrait>(
    db: &C,
    model: follower_enrollment_session::ActiveModel,
) -> Result<follower_enrollment_session::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

/// Resolves an enrollment session by the hash of its enrollment token,
/// regardless of status. Returns `None` when no session carries the hash.
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

/// Resolves an enrollment session by the hash of its acknowledgement token.
/// Returns `None` when no session carries the hash.
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

/// Returns the most recent enrollment session started for a follower node,
/// regardless of status. Returns `None` when the node has no session.
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

/// Lists the enrollment sessions of the given follower nodes, grouped by node
/// and newest-first within each. An empty id list short-circuits to empty.
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

/// Returns `true` when the follower node has at least one acknowledged
/// (completed) enrollment session.
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

/// Stamps the node's not-yet-acknowledged, not-yet-invalidated sessions with
/// the current time, returning how many sessions were invalidated; used to
/// retire outstanding enrollment attempts before starting a new one.
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

/// One-shot redemption of an enrollment token: marks the session redeemed only
/// when it is unredeemed, unacked, not invalidated and not expired at `now`.
/// Returns `true` when this call won the redemption.
pub async fn claim_redeemable_by_token_hash<C: ConnectionTrait>(
    db: &C,
    token_hash: &str,
    now: chrono::DateTime<Utc>,
) -> Result<bool> {
    let result = FollowerEnrollmentSession::update_many()
        .col_expr(
            follower_enrollment_session::Column::RedeemedAt,
            Expr::value(Some(now)),
        )
        .filter(follower_enrollment_session::Column::TokenHash.eq(token_hash))
        .filter(follower_enrollment_session::Column::RedeemedAt.is_null())
        .filter(follower_enrollment_session::Column::AckedAt.is_null())
        .filter(follower_enrollment_session::Column::InvalidatedAt.is_null())
        .filter(follower_enrollment_session::Column::ExpiresAt.gt(now))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

/// Marks a redeemed, unacked, uninvalidated session as acknowledged at the
/// current time. Returns `false` when the session is not in that state.
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

//! 仓储模块：`mfa_email_code_repo`。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, sea_query::Expr,
};

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::mfa_email_code::{self, Entity as MfaEmailCode};

/// Inserts a new email verification code row from the given active model.
pub async fn create<C: ConnectionTrait>(
    db: &C,
    model: mfa_email_code::ActiveModel,
) -> Result<mfa_email_code::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

/// Returns the user's most recent unconsumed, still-valid code at `now`.
pub async fn find_latest_active_for_user<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    now: chrono::DateTime<Utc>,
) -> Result<Option<mfa_email_code::Model>> {
    MfaEmailCode::find()
        .filter(mfa_email_code::Column::UserId.eq(user_id))
        .filter(mfa_email_code::Column::ConsumedAt.is_null())
        .filter(mfa_email_code::Column::ExpiresAt.gt(now))
        .order_by_desc(mfa_email_code::Column::CreatedAt)
        .one(db)
        .await
        .map_err(AsterError::from)
}

/// Returns the user's most recent unconsumed code, even if already expired;
/// used for diagnostics and resend bookkeeping.
pub async fn find_latest_unconsumed_for_user<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
) -> Result<Option<mfa_email_code::Model>> {
    MfaEmailCode::find()
        .filter(mfa_email_code::Column::UserId.eq(user_id))
        .filter(mfa_email_code::Column::ConsumedAt.is_null())
        .order_by_desc(mfa_email_code::Column::CreatedAt)
        .one(db)
        .await
        .map_err(AsterError::from)
}

/// Returns the most recent unconsumed code issued within one login flow for
/// the user, regardless of expiry.
pub async fn find_latest_unconsumed_for_flow<C: ConnectionTrait>(
    db: &C,
    flow_id: i64,
    user_id: i64,
) -> Result<Option<mfa_email_code::Model>> {
    MfaEmailCode::find()
        .filter(mfa_email_code::Column::FlowId.eq(flow_id))
        .filter(mfa_email_code::Column::UserId.eq(user_id))
        .filter(mfa_email_code::Column::ConsumedAt.is_null())
        .order_by_desc(mfa_email_code::Column::CreatedAt)
        .one(db)
        .await
        .map_err(AsterError::from)
}

/// Marks one code consumed at `now`. Returns `false` when the code is absent
/// or already consumed, so concurrent verifications cannot double-spend it.
pub async fn consume<C: ConnectionTrait>(
    db: &C,
    id: i64,
    now: chrono::DateTime<Utc>,
) -> Result<bool> {
    let result = MfaEmailCode::update_many()
        .col_expr(mfa_email_code::Column::ConsumedAt, Expr::value(Some(now)))
        .filter(mfa_email_code::Column::Id.eq(id))
        .filter(mfa_email_code::Column::ConsumedAt.is_null())
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

/// Consumes every unconsumed, unexpired code of the user, returning how many
/// were invalidated; used when a fresh code supersedes the old ones.
pub async fn consume_active_for_user<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    now: chrono::DateTime<Utc>,
) -> Result<u64> {
    let result = MfaEmailCode::update_many()
        .col_expr(mfa_email_code::Column::ConsumedAt, Expr::value(Some(now)))
        .filter(mfa_email_code::Column::UserId.eq(user_id))
        .filter(mfa_email_code::Column::ConsumedAt.is_null())
        .filter(mfa_email_code::Column::ExpiresAt.gt(now))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

/// Removes every code row of a user, returning the number of deleted rows.
pub async fn delete_all_for_user<C: ConnectionTrait>(db: &C, user_id: i64) -> Result<u64> {
    let result = MfaEmailCode::delete_many()
        .filter(mfa_email_code::Column::UserId.eq(user_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

/// Removes codes that expired before `now`, returning the number of deleted rows.
pub async fn cleanup_expired(db: &DatabaseConnection, now: chrono::DateTime<Utc>) -> Result<u64> {
    let result = MfaEmailCode::delete_many()
        .filter(mfa_email_code::Column::ExpiresAt.lt(now))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

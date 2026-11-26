//! 仓储模块：`mfa_factor_repo`。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder,
    sea_query::Expr,
};

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::mfa_factor::{self, Entity as MfaFactor};
use aster_drive_model::types::MfaPersistentFactorMethod;

/// Inserts a new persistent MFA factor from the given active model.
pub async fn create<C: ConnectionTrait>(
    db: &C,
    model: mfa_factor::ActiveModel,
) -> Result<mfa_factor::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

/// Lists the user's persistent MFA factors ordered by id (registration order).
pub async fn list_for_user<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
) -> Result<Vec<mfa_factor::Model>> {
    MfaFactor::find()
        .filter(mfa_factor::Column::UserId.eq(user_id))
        .order_by_asc(mfa_factor::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// Returns the user's persistent TOTP factor, if bound. `mfa_factors` only
/// carries long-lived factors (currently TOTP only); email verification codes
/// live in `mfa_email_code_repo` and are never queried here.
pub async fn find_totp_for_user<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
) -> Result<Option<mfa_factor::Model>> {
    MfaFactor::find()
        .filter(mfa_factor::Column::UserId.eq(user_id))
        .filter(mfa_factor::Column::Method.eq(MfaPersistentFactorMethod::Totp))
        .one(db)
        .await
        .map_err(AsterError::from)
}

/// Loads a factor only when it belongs to the given user. Returns `None` for
/// other users' factors or unknown ids.
pub async fn find_by_id_for_user<C: ConnectionTrait>(
    db: &C,
    id: i64,
    user_id: i64,
) -> Result<Option<mfa_factor::Model>> {
    MfaFactor::find()
        .filter(mfa_factor::Column::Id.eq(id))
        .filter(mfa_factor::Column::UserId.eq(user_id))
        .one(db)
        .await
        .map_err(AsterError::from)
}

/// Advances a factor's `last_used_at` (and `updated_at`). Returns `false` when
/// the id does not exist.
pub async fn touch_last_used<C: ConnectionTrait>(
    db: &C,
    id: i64,
    now: chrono::DateTime<Utc>,
) -> Result<bool> {
    let result = MfaFactor::update_many()
        .col_expr(mfa_factor::Column::LastUsedAt, Expr::value(Some(now)))
        .col_expr(mfa_factor::Column::UpdatedAt, Expr::value(now))
        .filter(mfa_factor::Column::Id.eq(id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

/// Removes the user's factor. Returns `false` when the id does not belong to
/// the user or does not exist.
pub async fn delete_for_user<C: ConnectionTrait>(db: &C, id: i64, user_id: i64) -> Result<bool> {
    let result = MfaFactor::delete_many()
        .filter(mfa_factor::Column::Id.eq(id))
        .filter(mfa_factor::Column::UserId.eq(user_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

/// Removes every MFA factor of a user, returning the number of deleted rows.
pub async fn delete_all_for_user<C: ConnectionTrait>(db: &C, user_id: i64) -> Result<u64> {
    let result = MfaFactor::delete_many()
        .filter(mfa_factor::Column::UserId.eq(user_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

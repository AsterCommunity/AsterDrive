//! 仓储模块：`mfa_recovery_code_repo`。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, sea_query::Expr,
};

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::mfa_recovery_code::{self, Entity as MfaRecoveryCode};

/// Inserts a batch of recovery codes one by one, returning the persisted rows
/// in input order.
pub async fn create_many<C: ConnectionTrait>(
    db: &C,
    models: Vec<mfa_recovery_code::ActiveModel>,
) -> Result<Vec<mfa_recovery_code::Model>> {
    let mut created = Vec::with_capacity(models.len());
    for model in models {
        created.push(model.insert(db).await.map_err(AsterError::from)?);
    }
    Ok(created)
}

/// Lists the user's still-unused recovery codes in creation order.
pub async fn list_unused_for_user<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
) -> Result<Vec<mfa_recovery_code::Model>> {
    MfaRecoveryCode::find()
        .filter(mfa_recovery_code::Column::UserId.eq(user_id))
        .filter(mfa_recovery_code::Column::UsedAt.is_null())
        .order_by_asc(mfa_recovery_code::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// Counts the user's still-unused recovery codes; drives the low-balance warning.
pub async fn count_unused_for_user<C: ConnectionTrait>(db: &C, user_id: i64) -> Result<u64> {
    MfaRecoveryCode::find()
        .filter(mfa_recovery_code::Column::UserId.eq(user_id))
        .filter(mfa_recovery_code::Column::UsedAt.is_null())
        .count(db)
        .await
        .map_err(AsterError::from)
}

/// One-shot redemption: marks a code used only when its id, owner and code
/// hash all match and it is still unused, so a code can never be redeemed
/// twice. Returns `true` when this call spent the code.
pub async fn mark_used_if_current<C: ConnectionTrait>(
    db: &C,
    id: i64,
    user_id: i64,
    code_hash: &str,
    now: chrono::DateTime<Utc>,
) -> Result<bool> {
    let result = MfaRecoveryCode::update_many()
        .col_expr(mfa_recovery_code::Column::UsedAt, Expr::value(Some(now)))
        .filter(mfa_recovery_code::Column::Id.eq(id))
        .filter(mfa_recovery_code::Column::UserId.eq(user_id))
        .filter(mfa_recovery_code::Column::CodeHash.eq(code_hash))
        .filter(mfa_recovery_code::Column::UsedAt.is_null())
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

/// Removes every recovery code of a user (used and unused), returning the
/// number of deleted rows; called when codes are regenerated or MFA is reset.
pub async fn delete_all_for_user<C: ConnectionTrait>(db: &C, user_id: i64) -> Result<u64> {
    let result = MfaRecoveryCode::delete_many()
        .filter(mfa_recovery_code::Column::UserId.eq(user_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

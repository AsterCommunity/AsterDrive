//! 仓储模块：`mfa_recovery_code_repo`。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, sea_query::Expr,
};

use crate::entities::mfa_recovery_code::{self, Entity as MfaRecoveryCode};
use crate::errors::{AsterError, Result};

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

pub async fn count_unused_for_user<C: ConnectionTrait>(db: &C, user_id: i64) -> Result<u64> {
    MfaRecoveryCode::find()
        .filter(mfa_recovery_code::Column::UserId.eq(user_id))
        .filter(mfa_recovery_code::Column::UsedAt.is_null())
        .count(db)
        .await
        .map_err(AsterError::from)
}

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

pub async fn delete_all_for_user<C: ConnectionTrait>(db: &C, user_id: i64) -> Result<u64> {
    let result = MfaRecoveryCode::delete_many()
        .filter(mfa_recovery_code::Column::UserId.eq(user_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

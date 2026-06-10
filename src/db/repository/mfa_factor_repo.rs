//! 仓储模块：`mfa_factor_repo`。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, sea_query::Expr,
};

use crate::entities::mfa_factor::{self, Entity as MfaFactor};
use crate::errors::{AsterError, Result};
use crate::types::MfaPersistentFactorMethod;

pub async fn create<C: ConnectionTrait>(
    db: &C,
    model: mfa_factor::ActiveModel,
) -> Result<mfa_factor::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn list_for_user(
    db: &DatabaseConnection,
    user_id: i64,
) -> Result<Vec<mfa_factor::Model>> {
    MfaFactor::find()
        .filter(mfa_factor::Column::UserId.eq(user_id))
        .order_by_asc(mfa_factor::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_totp_for_user<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
) -> Result<Option<mfa_factor::Model>> {
    // `mfa_factors` 只承载长期绑定的 factor；目前只有 TOTP 会出现在这里。
    // 邮箱验证码走 `mfa_email_code_repo`，不要把它作为 factor method 查询。
    MfaFactor::find()
        .filter(mfa_factor::Column::UserId.eq(user_id))
        .filter(mfa_factor::Column::Method.eq(MfaPersistentFactorMethod::Totp))
        .one(db)
        .await
        .map_err(AsterError::from)
}

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

pub async fn delete_for_user<C: ConnectionTrait>(db: &C, id: i64, user_id: i64) -> Result<bool> {
    let result = MfaFactor::delete_many()
        .filter(mfa_factor::Column::Id.eq(id))
        .filter(mfa_factor::Column::UserId.eq(user_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn delete_all_for_user<C: ConnectionTrait>(db: &C, user_id: i64) -> Result<u64> {
    let result = MfaFactor::delete_many()
        .filter(mfa_factor::Column::UserId.eq(user_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

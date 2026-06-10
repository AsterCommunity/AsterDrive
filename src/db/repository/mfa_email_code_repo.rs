//! 仓储模块：`mfa_email_code_repo`。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, sea_query::Expr,
};

use crate::entities::mfa_email_code::{self, Entity as MfaEmailCode};
use crate::errors::{AsterError, Result};

pub async fn create<C: ConnectionTrait>(
    db: &C,
    model: mfa_email_code::ActiveModel,
) -> Result<mfa_email_code::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

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

pub async fn delete_all_for_user<C: ConnectionTrait>(db: &C, user_id: i64) -> Result<u64> {
    let result = MfaEmailCode::delete_many()
        .filter(mfa_email_code::Column::UserId.eq(user_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

pub async fn cleanup_expired(db: &DatabaseConnection, now: chrono::DateTime<Utc>) -> Result<u64> {
    let result = MfaEmailCode::delete_many()
        .filter(mfa_email_code::Column::ExpiresAt.lt(now))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

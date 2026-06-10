//! 仓储模块：`mfa_totp_setup_flow_repo`。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    sea_query::Expr,
};

use crate::entities::mfa_totp_setup_flow::{self, Entity as MfaTotpSetupFlow};
use crate::errors::{AsterError, Result};

pub async fn create(
    db: &DatabaseConnection,
    model: mfa_totp_setup_flow::ActiveModel,
) -> Result<mfa_totp_setup_flow::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn find_active_by_flow_token_hash<C: ConnectionTrait>(
    db: &C,
    flow_token_hash: &str,
    now: chrono::DateTime<Utc>,
) -> Result<Option<mfa_totp_setup_flow::Model>> {
    MfaTotpSetupFlow::find()
        .filter(mfa_totp_setup_flow::Column::FlowTokenHash.eq(flow_token_hash))
        .filter(mfa_totp_setup_flow::Column::ConsumedAt.is_null())
        .filter(mfa_totp_setup_flow::Column::ExpiresAt.gt(now))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn consume<C: ConnectionTrait>(
    db: &C,
    id: i64,
    now: chrono::DateTime<Utc>,
) -> Result<bool> {
    let result = MfaTotpSetupFlow::update_many()
        .col_expr(
            mfa_totp_setup_flow::Column::ConsumedAt,
            Expr::value(Some(now)),
        )
        .filter(mfa_totp_setup_flow::Column::Id.eq(id))
        .filter(mfa_totp_setup_flow::Column::ConsumedAt.is_null())
        .filter(mfa_totp_setup_flow::Column::ExpiresAt.gt(now))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn delete_all_for_user<C: ConnectionTrait>(db: &C, user_id: i64) -> Result<u64> {
    let result = MfaTotpSetupFlow::delete_many()
        .filter(mfa_totp_setup_flow::Column::UserId.eq(user_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

pub async fn cleanup_expired(db: &DatabaseConnection, now: chrono::DateTime<Utc>) -> Result<u64> {
    let result = MfaTotpSetupFlow::delete_many()
        .filter(mfa_totp_setup_flow::Column::ExpiresAt.lt(now))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

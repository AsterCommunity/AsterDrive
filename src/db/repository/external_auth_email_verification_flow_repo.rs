//! 仓储模块：`external_auth_email_verification_flow_repo`。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    sea_query::Expr,
};

use crate::entities::external_auth_email_verification_flow::{
    self, Entity as ExternalAuthEmailVerificationFlow,
};
use crate::errors::{AsterError, Result};

pub async fn create(
    db: &DatabaseConnection,
    model: external_auth_email_verification_flow::ActiveModel,
) -> Result<external_auth_email_verification_flow::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn find_active_by_flow_token_hash(
    db: &DatabaseConnection,
    flow_token_hash: &str,
    now: chrono::DateTime<Utc>,
) -> Result<Option<external_auth_email_verification_flow::Model>> {
    ExternalAuthEmailVerificationFlow::find()
        .filter(external_auth_email_verification_flow::Column::FlowTokenHash.eq(flow_token_hash))
        .filter(external_auth_email_verification_flow::Column::ConsumedAt.is_null())
        .filter(external_auth_email_verification_flow::Column::ExpiresAt.gt(now))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_active_by_verification_token_hash(
    db: &DatabaseConnection,
    verification_token_hash: &str,
    now: chrono::DateTime<Utc>,
) -> Result<Option<external_auth_email_verification_flow::Model>> {
    ExternalAuthEmailVerificationFlow::find()
        .filter(
            external_auth_email_verification_flow::Column::VerificationTokenHash
                .eq(verification_token_hash),
        )
        .filter(external_auth_email_verification_flow::Column::ConsumedAt.is_null())
        .filter(external_auth_email_verification_flow::Column::ExpiresAt.gt(now))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn update_email_request<C: ConnectionTrait>(
    db: &C,
    flow: external_auth_email_verification_flow::Model,
    target_email: &str,
    verification_token_hash: &str,
    now: chrono::DateTime<Utc>,
) -> Result<bool> {
    let result = ExternalAuthEmailVerificationFlow::update_many()
        .col_expr(
            external_auth_email_verification_flow::Column::TargetEmail,
            Expr::value(Some(target_email.to_string())),
        )
        .col_expr(
            external_auth_email_verification_flow::Column::VerificationTokenHash,
            Expr::value(Some(verification_token_hash.to_string())),
        )
        .col_expr(
            external_auth_email_verification_flow::Column::EmailRequestedAt,
            Expr::value(Some(now)),
        )
        .filter(external_auth_email_verification_flow::Column::Id.eq(flow.id))
        .filter(external_auth_email_verification_flow::Column::VerificationTokenHash.is_null())
        .filter(external_auth_email_verification_flow::Column::ConsumedAt.is_null())
        .filter(external_auth_email_verification_flow::Column::ExpiresAt.gt(now))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn mark_consumed_if_unused<C: ConnectionTrait>(
    db: &C,
    id: i64,
    now: chrono::DateTime<Utc>,
) -> Result<bool> {
    let result = ExternalAuthEmailVerificationFlow::update_many()
        .col_expr(
            external_auth_email_verification_flow::Column::ConsumedAt,
            Expr::value(Some(now)),
        )
        .filter(external_auth_email_verification_flow::Column::Id.eq(id))
        .filter(external_auth_email_verification_flow::Column::ConsumedAt.is_null())
        .filter(external_auth_email_verification_flow::Column::ExpiresAt.gt(now))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn cleanup_expired(db: &DatabaseConnection, now: chrono::DateTime<Utc>) -> Result<u64> {
    let result = ExternalAuthEmailVerificationFlow::delete_many()
        .filter(external_auth_email_verification_flow::Column::ExpiresAt.lt(now))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

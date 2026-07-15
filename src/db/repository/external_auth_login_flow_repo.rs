//! 仓储模块：`external_auth_login_flow_repo`。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, sea_query::Expr,
};

use crate::entities::external_auth_login_flow::{self, Entity as ExternalAuthLoginFlow};
use crate::errors::{AsterError, Result};

pub async fn create(
    db: &DatabaseConnection,
    model: external_auth_login_flow::ActiveModel,
) -> Result<external_auth_login_flow::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn consume_by_state_and_browser_binding_hash(
    db: &DatabaseConnection,
    state_hash: &str,
    browser_binding_hash: &str,
    now: chrono::DateTime<Utc>,
) -> Result<Option<external_auth_login_flow::Model>> {
    let existing = ExternalAuthLoginFlow::find()
        .filter(external_auth_login_flow::Column::StateHash.eq(state_hash))
        .filter(external_auth_login_flow::Column::BrowserBindingHash.eq(browser_binding_hash))
        .filter(external_auth_login_flow::Column::ConsumedAt.is_null())
        .filter(external_auth_login_flow::Column::ExpiresAt.gt(now))
        .one(db)
        .await
        .map_err(AsterError::from)?;

    let Some(flow) = existing else {
        return Ok(None);
    };

    let result = ExternalAuthLoginFlow::update_many()
        .col_expr(
            external_auth_login_flow::Column::ConsumedAt,
            Expr::value(Some(now)),
        )
        .filter(external_auth_login_flow::Column::Id.eq(flow.id))
        .filter(external_auth_login_flow::Column::BrowserBindingHash.eq(browser_binding_hash))
        .filter(external_auth_login_flow::Column::ConsumedAt.is_null())
        .filter(external_auth_login_flow::Column::ExpiresAt.gt(now))
        .exec(db)
        .await
        .map_err(AsterError::from)?;

    if result.rows_affected == 1 {
        Ok(Some(flow))
    } else {
        Ok(None)
    }
}

pub async fn cleanup_expired(db: &DatabaseConnection, now: chrono::DateTime<Utc>) -> Result<u64> {
    let result = ExternalAuthLoginFlow::delete_many()
        .filter(external_auth_login_flow::Column::ExpiresAt.lt(now))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

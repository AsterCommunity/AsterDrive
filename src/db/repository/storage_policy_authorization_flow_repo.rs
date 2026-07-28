//! Repository helpers for `storage_policy_authorization_flows`.

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set, sea_query::Expr,
};

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::storage_policy_authorization_flow::{
    self, Entity as StoragePolicyAuthorizationFlow,
};
use aster_drive_model::types::StorageAuthorizationFlowStatus;

pub async fn create<C: ConnectionTrait>(
    db: &C,
    model: storage_policy_authorization_flow::ActiveModel,
) -> Result<storage_policy_authorization_flow::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn consume_by_state_hash<C: ConnectionTrait>(
    db: &C,
    state_hash: &str,
    now: chrono::DateTime<Utc>,
) -> Result<Option<storage_policy_authorization_flow::Model>> {
    let existing = StoragePolicyAuthorizationFlow::find()
        .filter(storage_policy_authorization_flow::Column::StateHash.eq(state_hash))
        .filter(
            storage_policy_authorization_flow::Column::Status
                .eq(StorageAuthorizationFlowStatus::Pending),
        )
        .filter(storage_policy_authorization_flow::Column::ConsumedAt.is_null())
        .filter(storage_policy_authorization_flow::Column::ExpiresAt.gt(now))
        .one(db)
        .await
        .map_err(AsterError::from)?;

    let Some(flow) = existing else {
        return Ok(None);
    };

    let result = StoragePolicyAuthorizationFlow::update_many()
        .col_expr(
            storage_policy_authorization_flow::Column::Status,
            Expr::value(StorageAuthorizationFlowStatus::Consumed.as_str()),
        )
        .col_expr(
            storage_policy_authorization_flow::Column::ConsumedAt,
            Expr::value(Some(now)),
        )
        .filter(storage_policy_authorization_flow::Column::Id.eq(flow.id))
        .filter(
            storage_policy_authorization_flow::Column::Status
                .eq(StorageAuthorizationFlowStatus::Pending),
        )
        .filter(storage_policy_authorization_flow::Column::ConsumedAt.is_null())
        .filter(storage_policy_authorization_flow::Column::ExpiresAt.gt(now))
        .exec(db)
        .await
        .map_err(AsterError::from)?;

    if result.rows_affected == 1 {
        Ok(Some(flow))
    } else {
        Ok(None)
    }
}

pub async fn cancel_pending_for_policy<C: ConnectionTrait>(
    db: &C,
    policy_id: i64,
    now: chrono::DateTime<Utc>,
) -> Result<u64> {
    let result = StoragePolicyAuthorizationFlow::update_many()
        .col_expr(
            storage_policy_authorization_flow::Column::Status,
            Expr::value(StorageAuthorizationFlowStatus::Cancelled.as_str()),
        )
        .col_expr(
            storage_policy_authorization_flow::Column::ConsumedAt,
            Expr::value(Some(now)),
        )
        .filter(storage_policy_authorization_flow::Column::PolicyId.eq(policy_id))
        .filter(
            storage_policy_authorization_flow::Column::Status
                .eq(StorageAuthorizationFlowStatus::Pending),
        )
        .filter(storage_policy_authorization_flow::Column::ConsumedAt.is_null())
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

pub async fn mark_expired<C: ConnectionTrait>(db: &C, now: chrono::DateTime<Utc>) -> Result<u64> {
    let result = StoragePolicyAuthorizationFlow::update_many()
        .col_expr(
            storage_policy_authorization_flow::Column::Status,
            Expr::value(StorageAuthorizationFlowStatus::Expired.as_str()),
        )
        .filter(
            storage_policy_authorization_flow::Column::Status
                .eq(StorageAuthorizationFlowStatus::Pending),
        )
        .filter(storage_policy_authorization_flow::Column::ExpiresAt.lt(now))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

pub async fn update_context<C: ConnectionTrait>(
    db: &C,
    id: i64,
    context: String,
) -> Result<Option<storage_policy_authorization_flow::Model>> {
    let Some(existing) = StoragePolicyAuthorizationFlow::find_by_id(id)
        .one(db)
        .await
        .map_err(AsterError::from)?
    else {
        return Ok(None);
    };
    let mut active: storage_policy_authorization_flow::ActiveModel = existing.into();
    active.context = Set(context);
    active.update(db).await.map(Some).map_err(AsterError::from)
}

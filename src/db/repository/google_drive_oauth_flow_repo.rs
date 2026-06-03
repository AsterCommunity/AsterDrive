//! 仓储模块：`google_drive_oauth_flow_repo`。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, sea_query::Expr,
};

use crate::entities::google_drive_oauth_flow::{self, Entity as GoogleDriveOauthFlow};
use crate::errors::{AsterError, Result};

pub async fn create<C: ConnectionTrait>(
    db: &C,
    model: google_drive_oauth_flow::ActiveModel,
) -> Result<google_drive_oauth_flow::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn consume_by_state_hash<C: ConnectionTrait>(
    db: &C,
    state_hash: &str,
    now: chrono::DateTime<Utc>,
) -> Result<Option<google_drive_oauth_flow::Model>> {
    let existing = GoogleDriveOauthFlow::find()
        .filter(google_drive_oauth_flow::Column::StateHash.eq(state_hash))
        .filter(google_drive_oauth_flow::Column::ConsumedAt.is_null())
        .filter(google_drive_oauth_flow::Column::ExpiresAt.gt(now))
        .one(db)
        .await
        .map_err(AsterError::from)?;

    let Some(flow) = existing else {
        return Ok(None);
    };

    let result = GoogleDriveOauthFlow::update_many()
        .col_expr(
            google_drive_oauth_flow::Column::ConsumedAt,
            Expr::value(Some(now)),
        )
        .filter(google_drive_oauth_flow::Column::Id.eq(flow.id))
        .filter(google_drive_oauth_flow::Column::ConsumedAt.is_null())
        .filter(google_drive_oauth_flow::Column::ExpiresAt.gt(now))
        .exec(db)
        .await
        .map_err(AsterError::from)?;

    if result.rows_affected == 1 {
        Ok(Some(flow))
    } else {
        Ok(None)
    }
}

pub async fn cleanup_expired<C: ConnectionTrait>(
    db: &C,
    now: chrono::DateTime<Utc>,
) -> Result<u64> {
    let result = GoogleDriveOauthFlow::delete_many()
        .filter(google_drive_oauth_flow::Column::ExpiresAt.lt(now))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

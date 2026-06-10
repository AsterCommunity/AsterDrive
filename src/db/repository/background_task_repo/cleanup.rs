use chrono::{DateTime, Utc};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect};

use super::common::{TerminalTaskCleanupFilters, terminal_cleanup_condition};
use crate::entities::background_task::{self, Entity as BackgroundTask};
use crate::errors::{AsterError, Result};
use crate::types::BackgroundTaskStatus;

pub async fn list_expired_terminal(
    db: &DatabaseConnection,
    now: DateTime<Utc>,
    limit: u64,
) -> Result<Vec<background_task::Model>> {
    BackgroundTask::find()
        .filter(background_task::Column::ExpiresAt.lte(now))
        .filter(background_task::Column::Status.is_in([
            BackgroundTaskStatus::Succeeded,
            BackgroundTaskStatus::Failed,
            BackgroundTaskStatus::Canceled,
        ]))
        .order_by_asc(background_task::Column::ExpiresAt)
        .limit(limit)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn delete_many(db: &DatabaseConnection, ids: &[i64]) -> Result<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
    Ok(BackgroundTask::delete_many()
        .filter(background_task::Column::Id.is_in(ids.iter().copied()))
        .exec(db)
        .await
        .map_err(AsterError::from)?
        .rows_affected)
}

pub async fn delete_terminal_by_filters(
    db: &DatabaseConnection,
    filters: &TerminalTaskCleanupFilters,
) -> Result<u64> {
    Ok(BackgroundTask::delete_many()
        .filter(terminal_cleanup_condition(filters))
        .exec(db)
        .await
        .map_err(AsterError::from)?
        .rows_affected)
}

use chrono::{DateTime, Utc};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect, Select,
};

use super::common::{TerminalTaskCleanupFilters, terminal_cleanup_condition};
use crate::errors::{AsterError, Result};
use aster_drive_model::entities::background_task::{self, Entity as BackgroundTask};
use aster_drive_model::types::BackgroundTaskStatus;

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

pub async fn delete_many<C: ConnectionTrait>(db: &C, ids: &[i64]) -> Result<u64> {
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

pub async fn delete_terminal_by_filters<C: ConnectionTrait>(
    db: &C,
    filters: &TerminalTaskCleanupFilters,
) -> Result<u64> {
    Ok(BackgroundTask::delete_many()
        .filter(terminal_cleanup_condition(filters))
        .exec(db)
        .await
        .map_err(AsterError::from)?
        .rows_affected)
}

pub async fn list_terminal_by_filters_for_update<C: ConnectionTrait>(
    db: &C,
    filters: &TerminalTaskCleanupFilters,
) -> Result<Vec<background_task::Model>> {
    terminal_by_filters_query(filters, db.get_database_backend())
        .all(db)
        .await
        .map_err(AsterError::from)
}

fn terminal_by_filters_query(
    filters: &TerminalTaskCleanupFilters,
    backend: DbBackend,
) -> Select<BackgroundTask> {
    let query = BackgroundTask::find()
        .filter(terminal_cleanup_condition(filters))
        .order_by_asc(background_task::Column::Id);
    if backend == DbBackend::Sqlite {
        query
    } else {
        query.lock_exclusive()
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::QueryTrait;

    use super::*;

    #[test]
    fn terminal_cleanup_query_locks_rows_only_on_row_locking_backends() {
        let filters = TerminalTaskCleanupFilters {
            finished_before: Utc::now(),
            kind: None,
            status: None,
        };

        let sqlite = terminal_by_filters_query(&filters, DbBackend::Sqlite)
            .build(DbBackend::Sqlite)
            .to_string();
        let postgres = terminal_by_filters_query(&filters, DbBackend::Postgres)
            .build(DbBackend::Postgres)
            .to_string();
        let mysql = terminal_by_filters_query(&filters, DbBackend::MySql)
            .build(DbBackend::MySql)
            .to_string();

        assert!(!sqlite.contains("FOR UPDATE"));
        assert!(postgres.ends_with("FOR UPDATE"));
        assert!(mysql.ends_with("FOR UPDATE"));
    }
}

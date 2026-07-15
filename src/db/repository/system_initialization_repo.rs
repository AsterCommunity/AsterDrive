//! Database coordination for the one-time system initialization flow.

use crate::config::definitions::AUTH_ALLOW_USER_REGISTRATION_KEY;
use crate::db::repository::user_repo;
use crate::errors::{AsterError, Result};
use aster_forge_db::system_config::{self, Entity as SystemConfig};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, sea_query::Expr};

pub async fn is_initialized<C: ConnectionTrait>(db: &C) -> Result<bool> {
    user_repo::count_all(db).await.map(|count| count > 0)
}

/// Serializes setup attempts using an existing, non-deletable system configuration row.
///
/// The no-op update acquires a row lock on PostgreSQL/MySQL and a write lock on SQLite until the
/// surrounding transaction completes. After this returns, the caller can safely re-check the user
/// table and create the initial administrator in the same transaction.
pub async fn acquire_setup_lock<C: ConnectionTrait>(db: &C) -> Result<()> {
    SystemConfig::update_many()
        .col_expr(
            system_config::Column::Value,
            Expr::col(system_config::Column::Value),
        )
        .filter(system_config::Column::Key.eq(AUTH_ALLOW_USER_REGISTRATION_KEY))
        .exec(db)
        .await
        .map_err(AsterError::from)?;

    // Check after the write-locking statement. Reading first would create a SQLite read snapshot
    // that cannot always be upgraded after another connection commits, yielding SQLITE_BUSY.
    let guard_exists = SystemConfig::find()
        .filter(system_config::Column::Key.eq(AUTH_ALLOW_USER_REGISTRATION_KEY))
        .one(db)
        .await
        .map_err(AsterError::from)?
        .is_some();
    if !guard_exists {
        return Err(AsterError::internal_error(format!(
            "setup lock config '{AUTH_ALLOW_USER_REGISTRATION_KEY}' is missing"
        )));
    }
    Ok(())
}

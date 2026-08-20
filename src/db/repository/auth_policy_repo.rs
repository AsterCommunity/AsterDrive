//! Database coordination for login-method policy mutations.

use crate::config::definitions::AUTH_PASSWORD_LOGIN_ENABLED_KEY;
use crate::errors::{AsterError, Result};
use aster_forge_db::system_config::{self, Entity as SystemConfig};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, sea_query::Expr};

/// Serializes mutations that can remove the last usable login method.
pub async fn acquire_login_method_lock<C: ConnectionTrait>(db: &C) -> Result<()> {
    SystemConfig::update_many()
        .col_expr(
            system_config::Column::Value,
            Expr::col(system_config::Column::Value),
        )
        .filter(system_config::Column::Key.eq(AUTH_PASSWORD_LOGIN_ENABLED_KEY))
        .exec(db)
        .await
        .map_err(AsterError::from)?;

    let guard_exists = SystemConfig::find()
        .filter(system_config::Column::Key.eq(AUTH_PASSWORD_LOGIN_ENABLED_KEY))
        .one(db)
        .await
        .map_err(AsterError::from)?
        .is_some();
    if !guard_exists {
        return Err(AsterError::internal_error(format!(
            "login policy lock config '{AUTH_PASSWORD_LOGIN_ENABLED_KEY}' is missing"
        )));
    }
    Ok(())
}

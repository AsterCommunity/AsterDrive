//! Database coordination for login-method policy mutations.

use crate::config::auth_runtime::{
    DEFAULT_AUTH_PASSKEY_LOGIN_ENABLED, DEFAULT_AUTH_PASSWORD_LOGIN_ENABLED, parse_bool_str,
};
use crate::config::definitions::{AUTH_PASSKEY_LOGIN_ENABLED_KEY, AUTH_PASSWORD_LOGIN_ENABLED_KEY};
use crate::db::repository::config_repo;
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

pub async fn any_builtin_login_method_enabled<C: ConnectionTrait>(db: &C) -> Result<bool> {
    let password_enabled = read_bool(
        db,
        AUTH_PASSWORD_LOGIN_ENABLED_KEY,
        DEFAULT_AUTH_PASSWORD_LOGIN_ENABLED,
    )
    .await?;
    let passkey_enabled = read_bool(
        db,
        AUTH_PASSKEY_LOGIN_ENABLED_KEY,
        DEFAULT_AUTH_PASSKEY_LOGIN_ENABLED,
    )
    .await?;
    Ok(password_enabled || passkey_enabled)
}

async fn read_bool<C: ConnectionTrait>(db: &C, key: &str, default: bool) -> Result<bool> {
    Ok(config_repo::find_by_key(db, key)
        .await?
        .and_then(|model| parse_bool_str(&model.value))
        .unwrap_or(default))
}

//! 认证服务子模块：`validation`。

use crate::errors::{AsterError, Result};

pub(crate) fn validate_username(username: &str) -> Result<()> {
    let len = username.len();
    if len < 4 {
        return Err(AsterError::validation_error(
            "username must be at least 4 characters",
        ));
    }
    if len > 16 {
        return Err(AsterError::validation_error(
            "username must be at most 16 characters",
        ));
    }
    if !username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(AsterError::validation_error(
            "username may only contain letters, numbers, underscores and hyphens",
        ));
    }
    Ok(())
}

pub(crate) fn validate_email(email: &str) -> Result<()> {
    aster_forge_validation::email::validate_email(email).map_err(AsterError::from)
}

pub(crate) fn validate_password(password: &str) -> Result<()> {
    if password.len() < 8 {
        return Err(AsterError::validation_error(
            "password must be at least 8 characters",
        ));
    }
    if password.len() > 128 {
        return Err(AsterError::validation_error(
            "password must be at most 128 characters",
        ));
    }
    Ok(())
}

pub(super) fn normalize_username(username: &str) -> Result<String> {
    let normalized = username.trim();
    validate_username(normalized)?;
    Ok(normalized.to_string())
}

pub(super) fn normalize_email(email: &str) -> Result<String> {
    aster_forge_validation::email::normalize_email(email).map_err(AsterError::from)
}

use crate::errors::{AsterError, Result};

pub fn validate_email(email: &str) -> Result<()> {
    if email.len() > 254 {
        return Err(AsterError::validation_error("email is too long"));
    }
    let parts: Vec<&str> = email.splitn(2, '@').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(AsterError::validation_error("invalid email format"));
    }
    if !parts[1].contains('.') {
        return Err(AsterError::validation_error("invalid email format"));
    }
    Ok(())
}

pub fn normalize_email(email: &str) -> Result<String> {
    let normalized = email.trim();
    validate_email(normalized)?;
    Ok(normalized.to_string())
}

pub fn email_domain(email: &str) -> Result<String> {
    let normalized = normalize_email(email)?;
    normalized
        .rsplit_once('@')
        .map(|(_, domain)| domain.to_ascii_lowercase())
        .ok_or_else(|| AsterError::validation_error("invalid email format"))
}

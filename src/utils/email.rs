use crate::errors::{AsterError, Result};

pub fn validate_email(email: &str) -> Result<()> {
    if email.len() > 254 {
        return Err(AsterError::validation_error("email is too long"));
    }
    if email.matches('@').count() != 1 {
        return Err(AsterError::validation_error("invalid email format"));
    }
    let Some((local, domain)) = email.split_once('@') else {
        return Err(AsterError::validation_error("invalid email format"));
    };
    if local.is_empty() || domain.is_empty() {
        return Err(AsterError::validation_error("invalid email format"));
    }
    if !domain.contains('.') {
        return Err(AsterError::validation_error("invalid email format"));
    }
    Ok(())
}

pub fn normalize_email(email: &str) -> Result<String> {
    let normalized = email.trim().to_ascii_lowercase();
    validate_email(&normalized)?;
    Ok(normalized)
}

pub fn email_domain(email: &str) -> Result<String> {
    let normalized = normalize_email(email)?;
    normalized
        .rsplit_once('@')
        .map(|(_, domain)| domain.to_ascii_lowercase())
        .ok_or_else(|| AsterError::validation_error("invalid email format"))
}

#[cfg(test)]
mod tests {
    use super::{email_domain, normalize_email, validate_email};

    #[test]
    fn validate_email_requires_exactly_one_at_separator() {
        assert!(validate_email("alice@example.com").is_ok());
        assert!(validate_email("alice@@example.com").is_err());
        assert!(validate_email("alice@example@com").is_err());
        assert!(validate_email("alice.example.com").is_err());
        assert!(validate_email("@example.com").is_err());
        assert!(validate_email("alice@").is_err());
    }

    #[test]
    fn email_helpers_keep_existing_normalization_contract() {
        assert_eq!(
            normalize_email(" Alice@Example.COM ").unwrap(),
            "alice@example.com"
        );
        assert_eq!(email_domain("alice@Example.COM").unwrap(), "example.com");
    }
}

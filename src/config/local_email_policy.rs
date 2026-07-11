use std::collections::BTreeSet;

use crate::api::api_error_code::ApiErrorCode;
use crate::config::RuntimeConfig;
use crate::errors::{AsterError, Result, validation_error_with_code};
use crate::utils::email::{email_domain, normalize_email};

pub use crate::config::definitions::{
    AUTH_LOCAL_EMAIL_ALLOWLIST_KEY, AUTH_LOCAL_EMAIL_BLOCKLIST_KEY,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalEmailPolicy {
    allowlist: EmailPolicyList,
    blocklist: EmailPolicyList,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct EmailPolicyList {
    emails: BTreeSet<String>,
    domains: BTreeSet<String>,
}

impl LocalEmailPolicy {
    pub fn from_runtime_config(runtime_config: &RuntimeConfig) -> Self {
        let allowlist = read_policy_list(runtime_config, AUTH_LOCAL_EMAIL_ALLOWLIST_KEY);
        let blocklist = read_policy_list(runtime_config, AUTH_LOCAL_EMAIL_BLOCKLIST_KEY);
        Self {
            allowlist,
            blocklist,
        }
    }

    pub fn check(&self, email: &str) -> Result<()> {
        let normalized = normalize_policy_email(email)?;
        let domain = email_domain(&normalized)?;

        if self.blocklist.matches(&normalized, &domain) {
            return Err(email_blocked_error());
        }

        if !self.allowlist.is_empty() && !self.allowlist.matches(&normalized, &domain) {
            return Err(validation_error_with_code(
                ApiErrorCode::AuthEmailNotAllowlisted,
                "email address is not allowed by local account policy",
            ));
        }

        Ok(())
    }

    pub fn check_not_blocked(&self, email: &str) -> Result<()> {
        let normalized = normalize_policy_email(email)?;
        let domain = email_domain(&normalized)?;

        if self.blocklist.matches(&normalized, &domain) {
            return Err(email_blocked_error());
        }

        Ok(())
    }
}

fn email_blocked_error() -> AsterError {
    validation_error_with_code(
        ApiErrorCode::AuthEmailBlocked,
        "email address is blocked by local account policy",
    )
}

impl EmailPolicyList {
    fn is_empty(&self) -> bool {
        self.emails.is_empty() && self.domains.is_empty()
    }

    fn matches(&self, email: &str, domain: &str) -> bool {
        self.emails.contains(email) || self.domains.contains(domain)
    }

    fn insert(&mut self, item: PolicyEntry) {
        match item {
            PolicyEntry::Email(value) => {
                self.emails.insert(value);
            }
            PolicyEntry::Domain(value) => {
                self.domains.insert(value);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PolicyEntry {
    Email(String),
    Domain(String),
}

pub fn normalize_local_email_policy_config_value(key: &str, value: &str) -> Result<String> {
    let raw_items = serde_json::from_str::<Vec<String>>(value).map_err(|error| {
        AsterError::validation_error(format!("{key} must be a JSON array of strings: {error}"))
    })?;
    let normalized = normalize_policy_items(raw_items)?;
    serde_json::to_string(&normalized).map_err(|error| {
        AsterError::internal_error(format!("failed to serialize {key} config value: {error}"))
    })
}

fn read_policy_list(runtime_config: &RuntimeConfig, key: &str) -> EmailPolicyList {
    let Some(raw) = runtime_config.get(key) else {
        return EmailPolicyList::default();
    };
    match serde_json::from_str::<Vec<String>>(&raw) {
        Ok(items) => {
            let mut list = EmailPolicyList::default();
            for item in items {
                let item = item.trim();
                if item.is_empty() {
                    continue;
                }
                match parse_policy_item(item) {
                    Ok(entry) => list.insert(entry),
                    Err(error) => {
                        tracing::warn!(
                            key,
                            item,
                            error = %error,
                            "invalid local email policy item; ignoring"
                        );
                    }
                }
            }
            list
        }
        Err(error) => {
            tracing::warn!(
                key,
                value = %raw,
                error = %error,
                "invalid local email policy config JSON; using an empty list"
            );
            EmailPolicyList::default()
        }
    }
}

fn normalize_policy_items(items: Vec<String>) -> Result<Vec<String>> {
    let mut normalized = BTreeSet::new();
    for item in items {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        let entry = parse_policy_item(item)?;
        match entry {
            PolicyEntry::Email(value) | PolicyEntry::Domain(value) => {
                normalized.insert(value);
            }
        }
    }
    Ok(normalized.into_iter().collect())
}

fn parse_policy_item(item: &str) -> Result<PolicyEntry> {
    if let Some(domain) = item.strip_prefix('@')
        && !domain.contains('@')
    {
        return normalize_policy_domain(domain).map(PolicyEntry::Domain);
    }

    if item.contains('@') {
        return normalize_policy_email(item).map(PolicyEntry::Email);
    }

    normalize_policy_domain(item).map(PolicyEntry::Domain)
}

fn normalize_policy_email(email: &str) -> Result<String> {
    let normalized = normalize_email(email)?;
    Ok(normalized.to_ascii_lowercase())
}

fn normalize_policy_domain(domain: &str) -> Result<String> {
    let normalized = domain.trim().trim_start_matches('@').to_ascii_lowercase();
    if normalized.is_empty()
        || normalized.len() > 253
        || normalized.contains('@')
        || !normalized.contains('.')
        || normalized.starts_with('.')
        || normalized.ends_with('.')
        || normalized.contains("..")
    {
        return Err(AsterError::validation_error(format!(
            "invalid email policy domain '{domain}'"
        )));
    }

    if !normalized.split('.').all(|label| {
        !label.is_empty() && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    }) {
        return Err(AsterError::validation_error(format!(
            "invalid email policy domain '{domain}'"
        )));
    }

    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RuntimeConfig;
    use crate::types::{ConfigSource, ConfigValueType, ConfigVisibility};
    use aster_forge_db::system_config;
    use chrono::Utc;

    fn config_model(key: &str, value: &str) -> system_config::Model {
        system_config::Model {
            id: 1,
            key: key.to_string(),
            value: value.to_string(),
            value_type: ConfigValueType::StringArray,
            requires_restart: false,
            is_sensitive: false,
            source: ConfigSource::System,
            visibility: ConfigVisibility::Private,
            namespace: String::new(),
            category: crate::config::definitions::CONFIG_CATEGORY_USER_REGISTRATION.to_string(),
            description: "test".to_string(),
            updated_at: Utc::now(),
            updated_by: None,
        }
    }

    #[test]
    fn normalizes_policy_items_with_trimming_lowercase_dedupe_and_sort() {
        let normalized = normalize_local_email_policy_config_value(
            AUTH_LOCAL_EMAIL_ALLOWLIST_KEY,
            r#"[" Example.COM ","alice@Example.com","example.com"," ALICE@example.COM ","@Team.Example"]"#,
        )
        .unwrap();

        assert_eq!(
            normalized,
            r#"["alice@example.com","example.com","team.example"]"#
        );
    }

    #[test]
    fn rejects_invalid_policy_items() {
        assert!(
            normalize_local_email_policy_config_value(
                AUTH_LOCAL_EMAIL_ALLOWLIST_KEY,
                r#"["localhost"]"#
            )
            .is_err()
        );
        assert!(
            normalize_local_email_policy_config_value(
                AUTH_LOCAL_EMAIL_ALLOWLIST_KEY,
                r#"["alice@example"]"#
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_unicode_domains_and_accepts_punycode_domains() {
        assert!(
            normalize_local_email_policy_config_value(
                AUTH_LOCAL_EMAIL_ALLOWLIST_KEY,
                r#"["用户.中国"]"#
            )
            .is_err()
        );
        assert_eq!(
            normalize_local_email_policy_config_value(
                AUTH_LOCAL_EMAIL_ALLOWLIST_KEY,
                r#"["xn--fiq228c.xn--fiqs8s"]"#,
            )
            .unwrap(),
            r#"["xn--fiq228c.xn--fiqs8s"]"#
        );
    }

    #[test]
    fn local_email_policy_allows_when_lists_are_empty() {
        let policy = LocalEmailPolicy::from_runtime_config(&RuntimeConfig::new());

        assert!(policy.check("user@example.com").is_ok());
    }

    #[test]
    fn local_email_policy_enforces_allowlist_blocklist_and_exact_domain_matching() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(config_model(
            AUTH_LOCAL_EMAIL_ALLOWLIST_KEY,
            r#"["example.com","alice@other.test"]"#,
        ));
        runtime_config.apply(config_model(
            AUTH_LOCAL_EMAIL_BLOCKLIST_KEY,
            r#"["blocked@example.com","tempmail.test"]"#,
        ));
        let policy = LocalEmailPolicy::from_runtime_config(&runtime_config);

        assert!(policy.check("ALICE@example.com").is_ok());
        assert!(policy.check("alice@other.test").is_ok());
        assert!(policy.check("blocked@example.com").is_err());
        assert!(policy.check("user@tempmail.test").is_err());
        assert!(policy.check("user@sub.example.com").is_err());
        assert!(policy.check("bob@other.test").is_err());
    }

    #[test]
    fn local_email_policy_check_not_blocked_ignores_allowlist() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(config_model(
            AUTH_LOCAL_EMAIL_ALLOWLIST_KEY,
            r#"["example.com"]"#,
        ));
        runtime_config.apply(config_model(
            AUTH_LOCAL_EMAIL_BLOCKLIST_KEY,
            r#"["blocked@example.com"]"#,
        ));
        let policy = LocalEmailPolicy::from_runtime_config(&runtime_config);

        assert!(policy.check_not_blocked("bob@other.test").is_ok());
        assert!(policy.check_not_blocked("blocked@example.com").is_err());
    }

    #[test]
    fn runtime_policy_list_keeps_valid_items_when_one_item_is_invalid() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(config_model(
            AUTH_LOCAL_EMAIL_BLOCKLIST_KEY,
            r#"["blocked@example.com","invalid-domain","tempmail.test"]"#,
        ));
        let policy = LocalEmailPolicy::from_runtime_config(&runtime_config);

        assert!(policy.check("blocked@example.com").is_err());
        assert!(policy.check("user@tempmail.test").is_err());
        assert!(policy.check("user@example.com").is_ok());
    }
}

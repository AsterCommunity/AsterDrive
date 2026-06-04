//! 配置子模块：`auth_runtime`。

use crate::config::RuntimeConfig;
use crate::errors::{AsterError, Result};

pub use crate::config::definitions::{
    AUTH_ACCESS_TOKEN_TTL_SECS_KEY, AUTH_ALLOW_USER_REGISTRATION_KEY,
    AUTH_CONTACT_CHANGE_TTL_SECS_KEY, AUTH_CONTACT_VERIFICATION_RESEND_COOLDOWN_SECS_KEY,
    AUTH_COOKIE_SECURE_KEY, AUTH_EMAIL_CODE_LOGIN_ALLOW_TOTP_FALLBACK_KEY,
    AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY, AUTH_EMAIL_CODE_LOGIN_RESEND_COOLDOWN_SECS_KEY,
    AUTH_EMAIL_CODE_LOGIN_TTL_SECS_KEY, AUTH_PASSKEY_LOGIN_ENABLED_KEY,
    AUTH_PASSWORD_RESET_REQUEST_COOLDOWN_SECS_KEY, AUTH_PASSWORD_RESET_TTL_SECS_KEY,
    AUTH_REFRESH_TOKEN_TTL_SECS_KEY, AUTH_REGISTER_ACTIVATION_ENABLED_KEY,
    AUTH_REGISTER_ACTIVATION_TTL_SECS_KEY,
};

pub const DEFAULT_AUTH_COOKIE_SECURE: bool = true;
pub const DEFAULT_AUTH_ALLOW_USER_REGISTRATION: bool = true;
pub const DEFAULT_AUTH_REGISTER_ACTIVATION_ENABLED: bool = true;
pub const DEFAULT_AUTH_ACCESS_TOKEN_TTL_SECS: u64 = 900;
pub const DEFAULT_AUTH_REFRESH_TOKEN_TTL_SECS: u64 = 604800;
pub const DEFAULT_AUTH_REGISTER_ACTIVATION_TTL_SECS: u64 = 86_400;
pub const DEFAULT_AUTH_CONTACT_CHANGE_TTL_SECS: u64 = 86_400;
pub const DEFAULT_AUTH_PASSWORD_RESET_TTL_SECS: u64 = 3_600;
pub const DEFAULT_AUTH_CONTACT_VERIFICATION_RESEND_COOLDOWN_SECS: u64 = 60;
pub const DEFAULT_AUTH_PASSWORD_RESET_REQUEST_COOLDOWN_SECS: u64 = 60;
pub const DEFAULT_AUTH_EMAIL_CODE_LOGIN_ENABLED: bool = false;
pub const DEFAULT_AUTH_PASSKEY_LOGIN_ENABLED: bool = true;
pub const DEFAULT_AUTH_EMAIL_CODE_LOGIN_ALLOW_TOTP_FALLBACK: bool = false;
pub const DEFAULT_AUTH_EMAIL_CODE_LOGIN_TTL_SECS: u64 = 600;
pub const DEFAULT_AUTH_EMAIL_CODE_LOGIN_RESEND_COOLDOWN_SECS: u64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeAuthPolicy {
    pub cookie_secure: bool,
    pub allow_user_registration: bool,
    pub passkey_login_enabled: bool,
    pub register_activation_enabled: bool,
    pub access_token_ttl_secs: u64,
    pub refresh_token_ttl_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeContactVerificationPolicy {
    pub register_activation_ttl_secs: u64,
    pub contact_change_ttl_secs: u64,
    pub resend_cooldown_secs: u64,
    pub password_reset_ttl_secs: u64,
    pub password_reset_request_cooldown_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeEmailCodeLoginPolicy {
    pub enabled: bool,
    pub allow_totp_fallback: bool,
    pub ttl_secs: u64,
    pub resend_cooldown_secs: u64,
}

impl RuntimeAuthPolicy {
    pub fn from_runtime_config(runtime_config: &RuntimeConfig) -> Self {
        let cookie_secure = match runtime_config.get(AUTH_COOKIE_SECURE_KEY) {
            Some(raw) => match parse_bool_str(&raw) {
                Some(value) => value,
                None => {
                    tracing::warn!(
                        key = AUTH_COOKIE_SECURE_KEY,
                        value = %raw,
                        "invalid runtime auth cookie secure config; using safe default"
                    );
                    DEFAULT_AUTH_COOKIE_SECURE
                }
            },
            None => DEFAULT_AUTH_COOKIE_SECURE,
        };

        let allow_user_registration = match runtime_config.get(AUTH_ALLOW_USER_REGISTRATION_KEY) {
            Some(raw) => match parse_bool_str(&raw) {
                Some(value) => value,
                None => {
                    tracing::warn!(
                        key = AUTH_ALLOW_USER_REGISTRATION_KEY,
                        value = %raw,
                        "invalid runtime auth registration config; using default"
                    );
                    DEFAULT_AUTH_ALLOW_USER_REGISTRATION
                }
            },
            None => DEFAULT_AUTH_ALLOW_USER_REGISTRATION,
        };

        let register_activation_enabled =
            match runtime_config.get(AUTH_REGISTER_ACTIVATION_ENABLED_KEY) {
                Some(raw) => match parse_bool_str(&raw) {
                    Some(value) => value,
                    None => {
                        tracing::warn!(
                            key = AUTH_REGISTER_ACTIVATION_ENABLED_KEY,
                            value = %raw,
                            "invalid runtime auth register activation config; using default"
                        );
                        DEFAULT_AUTH_REGISTER_ACTIVATION_ENABLED
                    }
                },
                None => DEFAULT_AUTH_REGISTER_ACTIVATION_ENABLED,
            };

        let passkey_login_enabled = match runtime_config.get(AUTH_PASSKEY_LOGIN_ENABLED_KEY) {
            Some(raw) => match parse_bool_str(&raw) {
                Some(value) => value,
                None => {
                    tracing::warn!(
                        key = AUTH_PASSKEY_LOGIN_ENABLED_KEY,
                        value = %raw,
                        "invalid runtime auth passkey login config; using default"
                    );
                    DEFAULT_AUTH_PASSKEY_LOGIN_ENABLED
                }
            },
            None => DEFAULT_AUTH_PASSKEY_LOGIN_ENABLED,
        };

        let access_token_ttl_secs = match runtime_config.get(AUTH_ACCESS_TOKEN_TTL_SECS_KEY) {
            Some(raw) => match parse_positive_u64(&raw) {
                Some(value) => value,
                None => {
                    tracing::warn!(
                        key = AUTH_ACCESS_TOKEN_TTL_SECS_KEY,
                        value = %raw,
                        "invalid runtime auth access token ttl config; using default"
                    );
                    DEFAULT_AUTH_ACCESS_TOKEN_TTL_SECS
                }
            },
            None => DEFAULT_AUTH_ACCESS_TOKEN_TTL_SECS,
        };

        let refresh_token_ttl_secs = match runtime_config.get(AUTH_REFRESH_TOKEN_TTL_SECS_KEY) {
            Some(raw) => match parse_positive_u64(&raw) {
                Some(value) => value,
                None => {
                    tracing::warn!(
                        key = AUTH_REFRESH_TOKEN_TTL_SECS_KEY,
                        value = %raw,
                        "invalid runtime auth refresh token ttl config; using default"
                    );
                    DEFAULT_AUTH_REFRESH_TOKEN_TTL_SECS
                }
            },
            None => DEFAULT_AUTH_REFRESH_TOKEN_TTL_SECS,
        };

        Self {
            cookie_secure,
            allow_user_registration,
            passkey_login_enabled,
            register_activation_enabled,
            access_token_ttl_secs,
            refresh_token_ttl_secs,
        }
    }
}

impl RuntimeContactVerificationPolicy {
    pub fn from_runtime_config(runtime_config: &RuntimeConfig) -> Self {
        let register_activation_ttl_secs = read_positive_u64(
            runtime_config,
            AUTH_REGISTER_ACTIVATION_TTL_SECS_KEY,
            DEFAULT_AUTH_REGISTER_ACTIVATION_TTL_SECS,
        );
        let contact_change_ttl_secs = read_positive_u64(
            runtime_config,
            AUTH_CONTACT_CHANGE_TTL_SECS_KEY,
            DEFAULT_AUTH_CONTACT_CHANGE_TTL_SECS,
        );
        let password_reset_ttl_secs = read_positive_u64(
            runtime_config,
            AUTH_PASSWORD_RESET_TTL_SECS_KEY,
            DEFAULT_AUTH_PASSWORD_RESET_TTL_SECS,
        );
        let resend_cooldown_secs = read_positive_u64(
            runtime_config,
            AUTH_CONTACT_VERIFICATION_RESEND_COOLDOWN_SECS_KEY,
            DEFAULT_AUTH_CONTACT_VERIFICATION_RESEND_COOLDOWN_SECS,
        );
        let password_reset_request_cooldown_secs = read_positive_u64(
            runtime_config,
            AUTH_PASSWORD_RESET_REQUEST_COOLDOWN_SECS_KEY,
            DEFAULT_AUTH_PASSWORD_RESET_REQUEST_COOLDOWN_SECS,
        );

        Self {
            register_activation_ttl_secs,
            contact_change_ttl_secs,
            resend_cooldown_secs,
            password_reset_ttl_secs,
            password_reset_request_cooldown_secs,
        }
    }
}

impl RuntimeEmailCodeLoginPolicy {
    pub fn from_runtime_config(runtime_config: &RuntimeConfig) -> Self {
        let enabled = match runtime_config.get(AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY) {
            Some(raw) => match parse_bool_str(&raw) {
                Some(value) => value,
                None => {
                    tracing::warn!(
                        key = AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY,
                        value = %raw,
                        "invalid runtime email code login enabled config; using default"
                    );
                    DEFAULT_AUTH_EMAIL_CODE_LOGIN_ENABLED
                }
            },
            None => DEFAULT_AUTH_EMAIL_CODE_LOGIN_ENABLED,
        };
        let allow_totp_fallback =
            match runtime_config.get(AUTH_EMAIL_CODE_LOGIN_ALLOW_TOTP_FALLBACK_KEY) {
                Some(raw) => match parse_bool_str(&raw) {
                    Some(value) => value,
                    None => {
                        tracing::warn!(
                            key = AUTH_EMAIL_CODE_LOGIN_ALLOW_TOTP_FALLBACK_KEY,
                            value = %raw,
                            "invalid runtime email code TOTP fallback config; using default"
                        );
                        DEFAULT_AUTH_EMAIL_CODE_LOGIN_ALLOW_TOTP_FALLBACK
                    }
                },
                None => DEFAULT_AUTH_EMAIL_CODE_LOGIN_ALLOW_TOTP_FALLBACK,
            };
        let ttl_secs = read_positive_u64(
            runtime_config,
            AUTH_EMAIL_CODE_LOGIN_TTL_SECS_KEY,
            DEFAULT_AUTH_EMAIL_CODE_LOGIN_TTL_SECS,
        );
        let resend_cooldown_secs = read_positive_u64(
            runtime_config,
            AUTH_EMAIL_CODE_LOGIN_RESEND_COOLDOWN_SECS_KEY,
            DEFAULT_AUTH_EMAIL_CODE_LOGIN_RESEND_COOLDOWN_SECS,
        );

        Self {
            enabled,
            allow_totp_fallback,
            ttl_secs,
            resend_cooldown_secs,
        }
    }
}

pub fn normalize_cookie_secure_config_value(value: &str) -> Result<String> {
    match parse_bool_str(value) {
        Some(value) => Ok(if value { "true" } else { "false" }.to_string()),
        None => Err(AsterError::validation_error(
            "auth_cookie_secure must be 'true' or 'false'",
        )),
    }
}

pub fn normalize_allow_user_registration_config_value(value: &str) -> Result<String> {
    match parse_bool_str(value) {
        Some(value) => Ok(if value { "true" } else { "false" }.to_string()),
        None => Err(AsterError::validation_error(
            "auth_allow_user_registration must be 'true' or 'false'",
        )),
    }
}

pub fn normalize_register_activation_enabled_config_value(value: &str) -> Result<String> {
    match parse_bool_str(value) {
        Some(value) => Ok(if value { "true" } else { "false" }.to_string()),
        None => Err(AsterError::validation_error(
            "auth_register_activation_enabled must be 'true' or 'false'",
        )),
    }
}

pub fn normalize_email_code_login_bool_config_value(key: &str, value: &str) -> Result<String> {
    match parse_bool_str(value) {
        Some(value) => Ok(if value { "true" } else { "false" }.to_string()),
        None => Err(AsterError::validation_error(format!(
            "{key} must be 'true' or 'false'",
        ))),
    }
}

pub fn normalize_token_ttl_config_value(key: &str, value: &str) -> Result<String> {
    let Some(ttl) = parse_positive_u64(value) else {
        return Err(AsterError::validation_error(format!(
            "{key} must be a positive integer",
        )));
    };
    Ok(ttl.to_string())
}

fn parse_bool_str(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn parse_positive_u64(value: &str) -> Option<u64> {
    let parsed = value.trim().parse::<u64>().ok()?;
    (parsed > 0).then_some(parsed)
}

fn read_positive_u64(runtime_config: &RuntimeConfig, key: &str, default: u64) -> u64 {
    match runtime_config.get(key) {
        Some(raw) => match parse_positive_u64(&raw) {
            Some(value) => value,
            None => {
                tracing::warn!(key, value = %raw, "invalid runtime auth contact config; using default");
                default
            }
        },
        None => default,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AUTH_ACCESS_TOKEN_TTL_SECS_KEY, AUTH_ALLOW_USER_REGISTRATION_KEY, AUTH_COOKIE_SECURE_KEY,
        AUTH_EMAIL_CODE_LOGIN_ALLOW_TOTP_FALLBACK_KEY, AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY,
        AUTH_EMAIL_CODE_LOGIN_RESEND_COOLDOWN_SECS_KEY, AUTH_EMAIL_CODE_LOGIN_TTL_SECS_KEY,
        AUTH_PASSKEY_LOGIN_ENABLED_KEY, AUTH_REFRESH_TOKEN_TTL_SECS_KEY,
        AUTH_REGISTER_ACTIVATION_ENABLED_KEY, DEFAULT_AUTH_ACCESS_TOKEN_TTL_SECS,
        DEFAULT_AUTH_ALLOW_USER_REGISTRATION, DEFAULT_AUTH_COOKIE_SECURE,
        DEFAULT_AUTH_EMAIL_CODE_LOGIN_ALLOW_TOTP_FALLBACK, DEFAULT_AUTH_EMAIL_CODE_LOGIN_ENABLED,
        DEFAULT_AUTH_EMAIL_CODE_LOGIN_RESEND_COOLDOWN_SECS, DEFAULT_AUTH_EMAIL_CODE_LOGIN_TTL_SECS,
        DEFAULT_AUTH_PASSKEY_LOGIN_ENABLED, DEFAULT_AUTH_REFRESH_TOKEN_TTL_SECS,
        DEFAULT_AUTH_REGISTER_ACTIVATION_ENABLED, RuntimeAuthPolicy, RuntimeEmailCodeLoginPolicy,
        normalize_email_code_login_bool_config_value, normalize_token_ttl_config_value,
    };
    use crate::config::RuntimeConfig;
    use crate::config::definitions::CONFIG_CATEGORY_AUTH;
    use crate::entities::system_config;
    use chrono::Utc;

    fn config_model(key: &str, value: &str) -> system_config::Model {
        system_config::Model {
            id: 1,
            key: key.to_string(),
            value: value.to_string(),
            value_type: crate::types::SystemConfigValueType::String,
            requires_restart: false,
            is_sensitive: false,
            source: crate::types::SystemConfigSource::System,
            visibility: crate::types::SystemConfigVisibility::Private,
            namespace: String::new(),
            category: CONFIG_CATEGORY_AUTH.to_string(),
            description: "test".to_string(),
            updated_at: Utc::now(),
            updated_by: None,
        }
    }

    #[test]
    fn runtime_auth_policy_uses_defaults_when_config_missing() {
        let runtime_config = RuntimeConfig::new();
        let policy = RuntimeAuthPolicy::from_runtime_config(&runtime_config);

        assert_eq!(policy.cookie_secure, DEFAULT_AUTH_COOKIE_SECURE);
        assert_eq!(
            policy.allow_user_registration,
            DEFAULT_AUTH_ALLOW_USER_REGISTRATION
        );
        assert_eq!(
            policy.register_activation_enabled,
            DEFAULT_AUTH_REGISTER_ACTIVATION_ENABLED
        );
        assert_eq!(
            policy.passkey_login_enabled,
            DEFAULT_AUTH_PASSKEY_LOGIN_ENABLED
        );
        assert_eq!(
            policy.access_token_ttl_secs,
            DEFAULT_AUTH_ACCESS_TOKEN_TTL_SECS
        );
        assert_eq!(
            policy.refresh_token_ttl_secs,
            DEFAULT_AUTH_REFRESH_TOKEN_TTL_SECS
        );
    }

    #[test]
    fn runtime_auth_policy_reads_runtime_values() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(config_model(AUTH_COOKIE_SECURE_KEY, "false"));
        runtime_config.apply(config_model(AUTH_ALLOW_USER_REGISTRATION_KEY, "false"));
        runtime_config.apply(config_model(AUTH_REGISTER_ACTIVATION_ENABLED_KEY, "false"));
        runtime_config.apply(config_model(AUTH_PASSKEY_LOGIN_ENABLED_KEY, "false"));
        runtime_config.apply(config_model(AUTH_ACCESS_TOKEN_TTL_SECS_KEY, "120"));
        runtime_config.apply(config_model(AUTH_REFRESH_TOKEN_TTL_SECS_KEY, "3600"));

        let policy = RuntimeAuthPolicy::from_runtime_config(&runtime_config);

        assert!(!policy.cookie_secure);
        assert!(!policy.allow_user_registration);
        assert!(!policy.register_activation_enabled);
        assert!(!policy.passkey_login_enabled);
        assert_eq!(policy.access_token_ttl_secs, 120);
        assert_eq!(policy.refresh_token_ttl_secs, 3600);
    }

    #[test]
    fn runtime_auth_policy_rejects_invalid_passkey_login_value_to_default() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(config_model(AUTH_PASSKEY_LOGIN_ENABLED_KEY, "maybe"));

        let policy = RuntimeAuthPolicy::from_runtime_config(&runtime_config);

        assert_eq!(
            policy.passkey_login_enabled,
            DEFAULT_AUTH_PASSKEY_LOGIN_ENABLED
        );
    }

    #[test]
    fn runtime_email_code_login_policy_uses_safe_defaults() {
        let runtime_config = RuntimeConfig::new();
        let policy = RuntimeEmailCodeLoginPolicy::from_runtime_config(&runtime_config);

        assert_eq!(policy.enabled, DEFAULT_AUTH_EMAIL_CODE_LOGIN_ENABLED);
        assert_eq!(
            policy.allow_totp_fallback,
            DEFAULT_AUTH_EMAIL_CODE_LOGIN_ALLOW_TOTP_FALLBACK
        );
        assert_eq!(policy.ttl_secs, DEFAULT_AUTH_EMAIL_CODE_LOGIN_TTL_SECS);
        assert_eq!(
            policy.resend_cooldown_secs,
            DEFAULT_AUTH_EMAIL_CODE_LOGIN_RESEND_COOLDOWN_SECS
        );
    }

    #[test]
    fn runtime_email_code_login_policy_reads_valid_values() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(config_model(AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY, "on"));
        runtime_config.apply(config_model(
            AUTH_EMAIL_CODE_LOGIN_ALLOW_TOTP_FALLBACK_KEY,
            "1",
        ));
        runtime_config.apply(config_model(AUTH_EMAIL_CODE_LOGIN_TTL_SECS_KEY, "1"));
        runtime_config.apply(config_model(
            AUTH_EMAIL_CODE_LOGIN_RESEND_COOLDOWN_SECS_KEY,
            "60",
        ));

        let policy = RuntimeEmailCodeLoginPolicy::from_runtime_config(&runtime_config);

        assert!(policy.enabled);
        assert!(policy.allow_totp_fallback);
        assert_eq!(policy.ttl_secs, 1);
        assert_eq!(policy.resend_cooldown_secs, 60);
    }

    #[test]
    fn runtime_email_code_login_policy_rejects_invalid_values_to_defaults() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(config_model(AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY, "maybe"));
        runtime_config.apply(config_model(
            AUTH_EMAIL_CODE_LOGIN_ALLOW_TOTP_FALLBACK_KEY,
            "perhaps",
        ));
        runtime_config.apply(config_model(AUTH_EMAIL_CODE_LOGIN_TTL_SECS_KEY, "0"));
        runtime_config.apply(config_model(
            AUTH_EMAIL_CODE_LOGIN_RESEND_COOLDOWN_SECS_KEY,
            "not-a-number",
        ));

        let policy = RuntimeEmailCodeLoginPolicy::from_runtime_config(&runtime_config);

        assert!(!policy.enabled);
        assert!(!policy.allow_totp_fallback);
        assert_eq!(policy.ttl_secs, DEFAULT_AUTH_EMAIL_CODE_LOGIN_TTL_SECS);
        assert_eq!(
            policy.resend_cooldown_secs,
            DEFAULT_AUTH_EMAIL_CODE_LOGIN_RESEND_COOLDOWN_SECS
        );
    }

    #[test]
    fn runtime_email_code_login_normalizers_enforce_boolean_and_positive_ttl() {
        assert_eq!(
            normalize_email_code_login_bool_config_value(AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY, "yes")
                .unwrap(),
            "true"
        );
        assert_eq!(
            normalize_email_code_login_bool_config_value(
                AUTH_EMAIL_CODE_LOGIN_ALLOW_TOTP_FALLBACK_KEY,
                "off"
            )
            .unwrap(),
            "false"
        );
        assert!(
            normalize_email_code_login_bool_config_value(
                AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY,
                "sometimes"
            )
            .is_err()
        );
        assert_eq!(
            normalize_token_ttl_config_value(AUTH_EMAIL_CODE_LOGIN_TTL_SECS_KEY, "60").unwrap(),
            "60"
        );
        assert!(normalize_token_ttl_config_value(AUTH_EMAIL_CODE_LOGIN_TTL_SECS_KEY, "0").is_err());
    }
}

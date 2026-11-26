//! 配置子模块：`auth_runtime`。

use crate::config::RuntimeConfig;
use crate::errors::{AsterError, Result};

pub use crate::config::definitions::{
    AUTH_ACCESS_TOKEN_TTL_SECS_KEY, AUTH_ALLOW_USER_REGISTRATION_KEY,
    AUTH_CONTACT_CHANGE_TTL_SECS_KEY, AUTH_CONTACT_VERIFICATION_RESEND_COOLDOWN_SECS_KEY,
    AUTH_COOKIE_SECURE_KEY, AUTH_EMAIL_CODE_LOGIN_ALLOW_TOTP_FALLBACK_KEY,
    AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY, AUTH_EMAIL_CODE_LOGIN_RESEND_COOLDOWN_SECS_KEY,
    AUTH_EMAIL_CODE_LOGIN_TTL_SECS_KEY, AUTH_PASSKEY_LOGIN_ENABLED_KEY,
    AUTH_PASSWORD_LOGIN_ENABLED_KEY, AUTH_PASSWORD_RESET_REQUEST_COOLDOWN_SECS_KEY,
    AUTH_PASSWORD_RESET_TTL_SECS_KEY, AUTH_REFRESH_TOKEN_TTL_SECS_KEY,
    AUTH_REGISTER_ACTIVATION_ENABLED_KEY, AUTH_REGISTER_ACTIVATION_TTL_SECS_KEY,
    AUTH_USER_INVITATION_TTL_SECS_KEY,
};

/// Default for `auth_cookie_secure` in the system_config registry: plain HTTP
/// first boot is allowed.
pub const DEFAULT_AUTH_COOKIE_SECURE: bool = false;
/// Fail-safe read default when the `auth_cookie_secure` runtime value is
/// missing or invalid: cookies stay `Secure`.
const FAILSAFE_AUTH_COOKIE_SECURE: bool = true;
/// Default for `auth_allow_user_registration`: public registration is open.
pub const DEFAULT_AUTH_ALLOW_USER_REGISTRATION: bool = true;
/// Default for `auth_register_activation_enabled`: email activation is required.
pub const DEFAULT_AUTH_REGISTER_ACTIVATION_ENABLED: bool = true;
/// Default access token TTL in seconds (15 minutes).
pub const DEFAULT_AUTH_ACCESS_TOKEN_TTL_SECS: u64 = 900;
/// Default refresh token TTL in seconds (7 days).
pub const DEFAULT_AUTH_REFRESH_TOKEN_TTL_SECS: u64 = 604800;
/// Default register-activation link TTL in seconds (24 hours).
pub const DEFAULT_AUTH_REGISTER_ACTIVATION_TTL_SECS: u64 = 86_400;
/// Default contact (email) change confirmation link TTL in seconds (24 hours).
pub const DEFAULT_AUTH_CONTACT_CHANGE_TTL_SECS: u64 = 86_400;
/// Default password reset link TTL in seconds (1 hour).
pub const DEFAULT_AUTH_PASSWORD_RESET_TTL_SECS: u64 = 3_600;
/// Default user invitation link TTL in seconds (72 hours).
pub const DEFAULT_AUTH_USER_INVITATION_TTL_SECS: u64 = 72 * 60 * 60;
/// Default resend cooldown for contact-verification codes, in seconds.
pub const DEFAULT_AUTH_CONTACT_VERIFICATION_RESEND_COOLDOWN_SECS: u64 = 60;
/// Default cooldown between password reset request attempts, in seconds.
pub const DEFAULT_AUTH_PASSWORD_RESET_REQUEST_COOLDOWN_SECS: u64 = 60;
/// Default for `auth_email_code_login_enabled`: email-code login is off.
pub const DEFAULT_AUTH_EMAIL_CODE_LOGIN_ENABLED: bool = false;
/// Default for `auth_passkey_login_enabled`: passkey login is on.
pub const DEFAULT_AUTH_PASSKEY_LOGIN_ENABLED: bool = true;
/// Default for `auth_password_login_enabled`: password login is on.
pub const DEFAULT_AUTH_PASSWORD_LOGIN_ENABLED: bool = true;
/// Default for allowing TOTP as a fallback during email-code login: disabled.
pub const DEFAULT_AUTH_EMAIL_CODE_LOGIN_ALLOW_TOTP_FALLBACK: bool = false;
/// Default email login code TTL in seconds (10 minutes).
pub const DEFAULT_AUTH_EMAIL_CODE_LOGIN_TTL_SECS: u64 = 600;
/// Default resend cooldown for email login codes, in seconds.
pub const DEFAULT_AUTH_EMAIL_CODE_LOGIN_RESEND_COOLDOWN_SECS: u64 = 60;

/// Snapshot of the runtime auth policy read once per request scope.
///
/// Invalid or missing runtime values fall back to the safe defaults declared
/// in this module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeAuthPolicy {
    /// Whether auth cookies are marked `Secure` (HTTPS-only).
    pub cookie_secure: bool,
    /// Whether self-service user registration is allowed at all.
    pub allow_user_registration: bool,
    /// Whether passkey (WebAuthn) login is enabled.
    pub passkey_login_enabled: bool,
    /// Whether password login is enabled.
    pub password_login_enabled: bool,
    /// Whether new accounts must confirm their email before activation.
    pub register_activation_enabled: bool,
    /// Access token TTL in seconds; strictly positive.
    pub access_token_ttl_secs: u64,
    /// Refresh token TTL in seconds; strictly positive.
    pub refresh_token_ttl_secs: u64,
}

/// TTLs and cooldowns for email verification flows, in seconds.
///
/// All values are strictly positive; invalid runtime values fall back to the
/// defaults declared in this module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeContactVerificationPolicy {
    /// Validity window of register-activation links.
    pub register_activation_ttl_secs: u64,
    /// Validity window of email-change confirmation links.
    pub contact_change_ttl_secs: u64,
    /// Minimum wait before resending a contact-verification code.
    pub resend_cooldown_secs: u64,
    /// Validity window of password reset links.
    pub password_reset_ttl_secs: u64,
    /// Minimum wait between password reset request attempts.
    pub password_reset_request_cooldown_secs: u64,
}

/// Runtime policy for email-code login.
///
/// Invalid or missing runtime values fall back to the defaults declared in
/// this module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeEmailCodeLoginPolicy {
    /// Whether email-code login is enabled at all.
    pub enabled: bool,
    /// Whether a user with TOTP configured may fall back to it instead.
    pub allow_totp_fallback: bool,
    /// Login code validity window in seconds; strictly positive.
    pub ttl_secs: u64,
    /// Minimum wait before resending a login code, in seconds.
    pub resend_cooldown_secs: u64,
}

impl RuntimeAuthPolicy {
    /// Reads the auth policy from the runtime config, falling back to the
    /// defaults declared in this module for missing or invalid values.
    pub fn from_runtime_config(runtime_config: &RuntimeConfig) -> Self {
        let cookie_secure = read_bool(
            runtime_config,
            AUTH_COOKIE_SECURE_KEY,
            FAILSAFE_AUTH_COOKIE_SECURE,
        );
        let allow_user_registration = read_bool(
            runtime_config,
            AUTH_ALLOW_USER_REGISTRATION_KEY,
            DEFAULT_AUTH_ALLOW_USER_REGISTRATION,
        );
        let register_activation_enabled = read_bool(
            runtime_config,
            AUTH_REGISTER_ACTIVATION_ENABLED_KEY,
            DEFAULT_AUTH_REGISTER_ACTIVATION_ENABLED,
        );
        let passkey_login_enabled = read_bool(
            runtime_config,
            AUTH_PASSKEY_LOGIN_ENABLED_KEY,
            DEFAULT_AUTH_PASSKEY_LOGIN_ENABLED,
        );
        let password_login_enabled = read_bool(
            runtime_config,
            AUTH_PASSWORD_LOGIN_ENABLED_KEY,
            DEFAULT_AUTH_PASSWORD_LOGIN_ENABLED,
        );

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
            password_login_enabled,
            register_activation_enabled,
            access_token_ttl_secs,
            refresh_token_ttl_secs,
        }
    }

    /// Whether anonymous visitors can self-register: registration must be
    /// allowed and password login must be available to complete it.
    pub fn public_registration_enabled(self) -> bool {
        self.allow_user_registration && self.password_login_enabled
    }
}

impl RuntimeContactVerificationPolicy {
    /// Reads the verification TTLs from the runtime config, falling back to
    /// the defaults declared in this module for missing or invalid values.
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
    /// Reads the email-code login policy from the runtime config, falling back
    /// to the defaults declared in this module for missing or invalid values.
    pub fn from_runtime_config(runtime_config: &RuntimeConfig) -> Self {
        let enabled = read_bool(
            runtime_config,
            AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY,
            DEFAULT_AUTH_EMAIL_CODE_LOGIN_ENABLED,
        );
        let allow_totp_fallback = read_bool(
            runtime_config,
            AUTH_EMAIL_CODE_LOGIN_ALLOW_TOTP_FALLBACK_KEY,
            DEFAULT_AUTH_EMAIL_CODE_LOGIN_ALLOW_TOTP_FALLBACK,
        );
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

/// Normalizes the `auth_cookie_secure` value to a canonical `"true"`/`"false"`.
pub fn normalize_cookie_secure_config_value(value: &str) -> Result<String> {
    match parse_bool_str(value) {
        Some(value) => Ok(if value { "true" } else { "false" }.to_string()),
        None => Err(AsterError::validation_error(
            "auth_cookie_secure must be 'true' or 'false'",
        )),
    }
}

/// Normalizes the `auth_allow_user_registration` value to `"true"`/`"false"`.
pub fn normalize_allow_user_registration_config_value(value: &str) -> Result<String> {
    match parse_bool_str(value) {
        Some(value) => Ok(if value { "true" } else { "false" }.to_string()),
        None => Err(AsterError::validation_error(
            "auth_allow_user_registration must be 'true' or 'false'",
        )),
    }
}

/// Normalizes the `auth_register_activation_enabled` value to `"true"`/`"false"`.
pub fn normalize_register_activation_enabled_config_value(value: &str) -> Result<String> {
    match parse_bool_str(value) {
        Some(value) => Ok(if value { "true" } else { "false" }.to_string()),
        None => Err(AsterError::validation_error(
            "auth_register_activation_enabled must be 'true' or 'false'",
        )),
    }
}

/// Normalizes a boolean email-code login value (identified by `key`) to
/// `"true"`/`"false"`.
pub fn normalize_email_code_login_bool_config_value(key: &str, value: &str) -> Result<String> {
    match parse_bool_str(value) {
        Some(value) => Ok(if value { "true" } else { "false" }.to_string()),
        None => Err(AsterError::validation_error(format!(
            "{key} must be 'true' or 'false'",
        ))),
    }
}

/// Validates a token/config TTL value for `key` as a strictly positive integer
/// and returns its canonical decimal form.
pub fn normalize_token_ttl_config_value(key: &str, value: &str) -> Result<String> {
    let Some(ttl) = parse_positive_u64(value) else {
        return Err(AsterError::validation_error(format!(
            "{key} must be a positive integer",
        )));
    };
    Ok(ttl.to_string())
}

/// User invitation link TTL in seconds, read from the runtime config with the
/// module default applied for missing or invalid values.
pub fn user_invitation_ttl_secs(runtime_config: &RuntimeConfig) -> u64 {
    read_positive_u64(
        runtime_config,
        AUTH_USER_INVITATION_TTL_SECS_KEY,
        DEFAULT_AUTH_USER_INVITATION_TTL_SECS,
    )
}

pub(crate) fn parse_bool_str(value: &str) -> Option<bool> {
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

fn read_bool(runtime_config: &RuntimeConfig, key: &str, default: bool) -> bool {
    match runtime_config.get(key) {
        Some(raw) => match parse_bool_str(&raw) {
            Some(value) => value,
            None => {
                tracing::warn!(key, value = %raw, "invalid runtime auth bool config; using default");
                default
            }
        },
        None => default,
    }
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
        AUTH_PASSKEY_LOGIN_ENABLED_KEY, AUTH_PASSWORD_LOGIN_ENABLED_KEY,
        AUTH_REFRESH_TOKEN_TTL_SECS_KEY, AUTH_REGISTER_ACTIVATION_ENABLED_KEY,
        DEFAULT_AUTH_ACCESS_TOKEN_TTL_SECS, DEFAULT_AUTH_ALLOW_USER_REGISTRATION,
        DEFAULT_AUTH_EMAIL_CODE_LOGIN_ALLOW_TOTP_FALLBACK, DEFAULT_AUTH_EMAIL_CODE_LOGIN_ENABLED,
        DEFAULT_AUTH_EMAIL_CODE_LOGIN_RESEND_COOLDOWN_SECS, DEFAULT_AUTH_EMAIL_CODE_LOGIN_TTL_SECS,
        DEFAULT_AUTH_PASSKEY_LOGIN_ENABLED, DEFAULT_AUTH_PASSWORD_LOGIN_ENABLED,
        DEFAULT_AUTH_REFRESH_TOKEN_TTL_SECS, DEFAULT_AUTH_REGISTER_ACTIVATION_ENABLED,
        DEFAULT_AUTH_USER_INVITATION_TTL_SECS, FAILSAFE_AUTH_COOKIE_SECURE, RuntimeAuthPolicy,
        RuntimeEmailCodeLoginPolicy, normalize_email_code_login_bool_config_value,
        normalize_token_ttl_config_value, user_invitation_ttl_secs,
    };
    use crate::config::RuntimeConfig;
    use crate::config::definitions::AUTH_USER_INVITATION_TTL_SECS_KEY;
    use crate::config::definitions::CONFIG_CATEGORY_AUTH;
    use aster_forge_db::system_config;
    use chrono::Utc;

    fn config_model(key: &str, value: &str) -> system_config::Model {
        system_config::Model {
            id: 1,
            key: key.to_string(),
            value: value.to_string(),
            value_type: aster_forge_config::ConfigValueType::String,
            requires_restart: false,
            is_sensitive: false,
            source: aster_forge_config::ConfigSource::System,
            visibility: aster_forge_config::ConfigVisibility::Private,
            namespace: String::new(),
            category: CONFIG_CATEGORY_AUTH.to_string(),
            description: "test".to_string(),
            updated_at: Utc::now(),
            updated_by: None,
        }
    }

    #[test]
    fn runtime_auth_policy_fails_secure_when_cookie_config_is_missing() {
        let runtime_config = RuntimeConfig::new();
        let policy = RuntimeAuthPolicy::from_runtime_config(&runtime_config);

        assert_eq!(policy.cookie_secure, FAILSAFE_AUTH_COOKIE_SECURE);
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
            policy.password_login_enabled,
            DEFAULT_AUTH_PASSWORD_LOGIN_ENABLED
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
        runtime_config.apply(config_model(AUTH_PASSWORD_LOGIN_ENABLED_KEY, "false"));
        runtime_config.apply(config_model(AUTH_ACCESS_TOKEN_TTL_SECS_KEY, "120"));
        runtime_config.apply(config_model(AUTH_REFRESH_TOKEN_TTL_SECS_KEY, "3600"));

        let policy = RuntimeAuthPolicy::from_runtime_config(&runtime_config);

        assert!(!policy.cookie_secure);
        assert!(!policy.allow_user_registration);
        assert!(!policy.register_activation_enabled);
        assert!(!policy.passkey_login_enabled);
        assert!(!policy.password_login_enabled);
        assert_eq!(policy.access_token_ttl_secs, 120);
        assert_eq!(policy.refresh_token_ttl_secs, 3600);
    }

    #[test]
    fn runtime_auth_policy_rejects_invalid_bool_values_to_defaults() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(config_model(AUTH_COOKIE_SECURE_KEY, "maybe"));
        runtime_config.apply(config_model(AUTH_ALLOW_USER_REGISTRATION_KEY, "unknown"));
        runtime_config.apply(config_model(
            AUTH_REGISTER_ACTIVATION_ENABLED_KEY,
            "sometimes",
        ));
        runtime_config.apply(config_model(AUTH_PASSKEY_LOGIN_ENABLED_KEY, "maybe"));
        runtime_config.apply(config_model(AUTH_PASSWORD_LOGIN_ENABLED_KEY, "maybe"));

        let policy = RuntimeAuthPolicy::from_runtime_config(&runtime_config);

        assert_eq!(policy.cookie_secure, FAILSAFE_AUTH_COOKIE_SECURE);
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
            policy.password_login_enabled,
            DEFAULT_AUTH_PASSWORD_LOGIN_ENABLED
        );
    }

    #[test]
    fn public_registration_requires_password_login() {
        let mut policy = RuntimeAuthPolicy::from_runtime_config(&RuntimeConfig::new());
        assert!(policy.public_registration_enabled());
        policy.password_login_enabled = false;
        assert!(!policy.public_registration_enabled());
        policy.password_login_enabled = true;
        policy.allow_user_registration = false;
        assert!(!policy.public_registration_enabled());
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

    #[test]
    fn user_invitation_ttl_reader_uses_runtime_value_and_default() {
        let runtime_config = RuntimeConfig::new();
        assert_eq!(
            user_invitation_ttl_secs(&runtime_config),
            DEFAULT_AUTH_USER_INVITATION_TTL_SECS
        );

        runtime_config.apply(config_model(AUTH_USER_INVITATION_TTL_SECS_KEY, "3600"));
        assert_eq!(user_invitation_ttl_secs(&runtime_config), 3600);

        runtime_config.apply(config_model(AUTH_USER_INVITATION_TTL_SECS_KEY, "0"));
        assert_eq!(
            user_invitation_ttl_secs(&runtime_config),
            DEFAULT_AUTH_USER_INVITATION_TTL_SECS
        );
    }
}

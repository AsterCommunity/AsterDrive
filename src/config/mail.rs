//! 配置子模块：`mail`。

use crate::config::RuntimeConfig;
use crate::errors::{AsterError, Result};
use crate::types::MailTemplateCode;

pub use crate::config::definitions::{
    MAIL_FROM_ADDRESS_KEY, MAIL_FROM_NAME_KEY, MAIL_SECURITY_KEY, MAIL_SMTP_HOST_KEY,
    MAIL_SMTP_PASSWORD_KEY, MAIL_SMTP_PORT_KEY, MAIL_SMTP_USERNAME_KEY,
    MAIL_TEMPLATE_CONTACT_CHANGE_CONFIRMATION_HTML_KEY,
    MAIL_TEMPLATE_CONTACT_CHANGE_CONFIRMATION_SUBJECT_KEY,
    MAIL_TEMPLATE_CONTACT_CHANGE_NOTICE_HTML_KEY, MAIL_TEMPLATE_CONTACT_CHANGE_NOTICE_SUBJECT_KEY,
    MAIL_TEMPLATE_EXTERNAL_AUTH_EMAIL_VERIFICATION_HTML_KEY,
    MAIL_TEMPLATE_EXTERNAL_AUTH_EMAIL_VERIFICATION_SUBJECT_KEY,
    MAIL_TEMPLATE_LOGIN_EMAIL_CODE_HTML_KEY, MAIL_TEMPLATE_LOGIN_EMAIL_CODE_SUBJECT_KEY,
    MAIL_TEMPLATE_PASSWORD_RESET_HTML_KEY, MAIL_TEMPLATE_PASSWORD_RESET_NOTICE_HTML_KEY,
    MAIL_TEMPLATE_PASSWORD_RESET_NOTICE_SUBJECT_KEY, MAIL_TEMPLATE_PASSWORD_RESET_SUBJECT_KEY,
    MAIL_TEMPLATE_REGISTER_ACTIVATION_HTML_KEY, MAIL_TEMPLATE_REGISTER_ACTIVATION_SUBJECT_KEY,
    MAIL_TEMPLATE_USER_INVITATION_HTML_KEY, MAIL_TEMPLATE_USER_INVITATION_SUBJECT_KEY,
};

pub const DEFAULT_MAIL_SMTP_PORT: u16 = 587;
pub const DEFAULT_MAIL_SECURITY: bool = true;
const MAIL_TEMPLATE_MAX_SUBJECT_LEN: usize = 255;
const MAIL_TEMPLATE_MAX_BODY_LEN: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeMailSettings {
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_username: String,
    pub smtp_password: String,
    pub from_address: String,
    pub from_name: String,
    pub encryption_enabled: bool,
}

impl RuntimeMailSettings {
    pub fn from_runtime_config(runtime_config: &RuntimeConfig) -> Self {
        let smtp_port = runtime_config
            .get(MAIL_SMTP_PORT_KEY)
            .and_then(|raw| parse_port(&raw))
            .unwrap_or(DEFAULT_MAIL_SMTP_PORT);
        let encryption_enabled =
            runtime_config.get_bool_or(MAIL_SECURITY_KEY, DEFAULT_MAIL_SECURITY);

        Self {
            smtp_host: runtime_config.get(MAIL_SMTP_HOST_KEY).unwrap_or_default(),
            smtp_port,
            smtp_username: runtime_config
                .get(MAIL_SMTP_USERNAME_KEY)
                .unwrap_or_default(),
            smtp_password: runtime_config
                .get(MAIL_SMTP_PASSWORD_KEY)
                .unwrap_or_default(),
            from_address: runtime_config
                .get(MAIL_FROM_ADDRESS_KEY)
                .unwrap_or_default(),
            from_name: runtime_config.get(MAIL_FROM_NAME_KEY).unwrap_or_default(),
            encryption_enabled,
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.smtp_host.trim().is_empty() && !self.from_address.trim().is_empty()
    }

    pub fn is_ready_for_delivery(&self) -> bool {
        self.is_configured()
            && self.smtp_username.trim().is_empty() == self.smtp_password.trim().is_empty()
    }
}

pub fn template_subject_key(code: MailTemplateCode) -> &'static str {
    match code {
        MailTemplateCode::RegisterActivation => MAIL_TEMPLATE_REGISTER_ACTIVATION_SUBJECT_KEY,
        MailTemplateCode::ContactChangeConfirmation => {
            MAIL_TEMPLATE_CONTACT_CHANGE_CONFIRMATION_SUBJECT_KEY
        }
        MailTemplateCode::PasswordReset => MAIL_TEMPLATE_PASSWORD_RESET_SUBJECT_KEY,
        MailTemplateCode::PasswordResetNotice => MAIL_TEMPLATE_PASSWORD_RESET_NOTICE_SUBJECT_KEY,
        MailTemplateCode::ContactChangeNotice => MAIL_TEMPLATE_CONTACT_CHANGE_NOTICE_SUBJECT_KEY,
        MailTemplateCode::ExternalAuthEmailVerification => {
            MAIL_TEMPLATE_EXTERNAL_AUTH_EMAIL_VERIFICATION_SUBJECT_KEY
        }
        MailTemplateCode::LoginEmailCode => MAIL_TEMPLATE_LOGIN_EMAIL_CODE_SUBJECT_KEY,
        MailTemplateCode::UserInvitation => MAIL_TEMPLATE_USER_INVITATION_SUBJECT_KEY,
    }
}

pub fn template_html_key(code: MailTemplateCode) -> &'static str {
    match code {
        MailTemplateCode::RegisterActivation => MAIL_TEMPLATE_REGISTER_ACTIVATION_HTML_KEY,
        MailTemplateCode::ContactChangeConfirmation => {
            MAIL_TEMPLATE_CONTACT_CHANGE_CONFIRMATION_HTML_KEY
        }
        MailTemplateCode::PasswordReset => MAIL_TEMPLATE_PASSWORD_RESET_HTML_KEY,
        MailTemplateCode::PasswordResetNotice => MAIL_TEMPLATE_PASSWORD_RESET_NOTICE_HTML_KEY,
        MailTemplateCode::ContactChangeNotice => MAIL_TEMPLATE_CONTACT_CHANGE_NOTICE_HTML_KEY,
        MailTemplateCode::ExternalAuthEmailVerification => {
            MAIL_TEMPLATE_EXTERNAL_AUTH_EMAIL_VERIFICATION_HTML_KEY
        }
        MailTemplateCode::LoginEmailCode => MAIL_TEMPLATE_LOGIN_EMAIL_CODE_HTML_KEY,
        MailTemplateCode::UserInvitation => MAIL_TEMPLATE_USER_INVITATION_HTML_KEY,
    }
}

pub fn default_template_subject(code: MailTemplateCode) -> &'static str {
    match code {
        MailTemplateCode::RegisterActivation => {
            include_str!("mail_templates/register_activation.subject.txt")
                .trim_end_matches(['\r', '\n'])
        }
        MailTemplateCode::ContactChangeConfirmation => {
            include_str!("mail_templates/contact_change_confirmation.subject.txt")
                .trim_end_matches(['\r', '\n'])
        }
        MailTemplateCode::PasswordReset => {
            include_str!("mail_templates/password_reset.subject.txt").trim_end_matches(['\r', '\n'])
        }
        MailTemplateCode::PasswordResetNotice => {
            include_str!("mail_templates/password_reset_notice.subject.txt")
                .trim_end_matches(['\r', '\n'])
        }
        MailTemplateCode::ContactChangeNotice => {
            include_str!("mail_templates/contact_change_notice.subject.txt")
                .trim_end_matches(['\r', '\n'])
        }
        MailTemplateCode::ExternalAuthEmailVerification => {
            include_str!("mail_templates/external_auth_email_verification.subject.txt")
                .trim_end_matches(['\r', '\n'])
        }
        MailTemplateCode::LoginEmailCode => {
            include_str!("mail_templates/login_email_code.subject.txt")
                .trim_end_matches(['\r', '\n'])
        }
        MailTemplateCode::UserInvitation => {
            include_str!("mail_templates/user_invitation.subject.txt")
                .trim_end_matches(['\r', '\n'])
        }
    }
}

pub fn default_template_html(code: MailTemplateCode) -> &'static str {
    match code {
        MailTemplateCode::RegisterActivation => {
            include_str!("mail_templates/register_activation.html").trim_end_matches(['\r', '\n'])
        }
        MailTemplateCode::ContactChangeConfirmation => {
            include_str!("mail_templates/contact_change_confirmation.html")
                .trim_end_matches(['\r', '\n'])
        }
        MailTemplateCode::PasswordReset => {
            include_str!("mail_templates/password_reset.html").trim_end_matches(['\r', '\n'])
        }
        MailTemplateCode::PasswordResetNotice => {
            include_str!("mail_templates/password_reset_notice.html").trim_end_matches(['\r', '\n'])
        }
        MailTemplateCode::ContactChangeNotice => {
            include_str!("mail_templates/contact_change_notice.html").trim_end_matches(['\r', '\n'])
        }
        MailTemplateCode::ExternalAuthEmailVerification => {
            include_str!("mail_templates/external_auth_email_verification.html")
                .trim_end_matches(['\r', '\n'])
        }
        MailTemplateCode::LoginEmailCode => {
            include_str!("mail_templates/login_email_code.html").trim_end_matches(['\r', '\n'])
        }
        MailTemplateCode::UserInvitation => {
            include_str!("mail_templates/user_invitation.html").trim_end_matches(['\r', '\n'])
        }
    }
}

pub fn template_subject(runtime_config: &RuntimeConfig, code: MailTemplateCode) -> String {
    runtime_config
        .get(template_subject_key(code))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default_template_subject(code).to_string())
}

pub fn template_html(runtime_config: &RuntimeConfig, code: MailTemplateCode) -> String {
    runtime_config
        .get(template_html_key(code))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default_template_html(code).to_string())
}

pub fn normalize_smtp_host_config_value(value: &str) -> Result<String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Ok(String::new());
    }
    if normalized.contains(char::is_whitespace) {
        return Err(AsterError::validation_error(
            "mail_smtp_host cannot contain spaces",
        ));
    }
    Ok(normalized)
}

pub fn normalize_smtp_port_config_value(value: &str) -> Result<String> {
    let Some(port) = parse_port(value) else {
        return Err(AsterError::validation_error(
            "mail_smtp_port must be an integer between 1 and 65535",
        ));
    };
    Ok(port.to_string())
}

pub fn normalize_mail_address_config_value(value: &str) -> Result<String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Ok(String::new());
    }
    validate_contact_email(&normalized)?;
    Ok(normalized)
}

pub fn normalize_mail_name_config_value(value: &str) -> Result<String> {
    let normalized = value.trim();
    if normalized.len() > 128 {
        return Err(AsterError::validation_error(
            "mail_from_name must be at most 128 characters",
        ));
    }
    Ok(normalized.to_string())
}

pub fn normalize_mail_security_config_value(value: &str) -> Result<String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok("true".to_string()),
        "false" | "0" | "no" | "off" => Ok("false".to_string()),
        _ => Err(AsterError::validation_error(
            "mail_security must be 'true' or 'false'",
        )),
    }
}

pub fn normalize_mail_template_subject_config_value(key: &str, value: &str) -> Result<String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(AsterError::validation_error(format!(
            "{key} cannot be empty"
        )));
    }
    if normalized.contains(['\r', '\n']) {
        return Err(AsterError::validation_error(format!(
            "{key} must be a single line",
        )));
    }
    if normalized.len() > MAIL_TEMPLATE_MAX_SUBJECT_LEN {
        return Err(AsterError::validation_error(format!(
            "{key} must be at most {MAIL_TEMPLATE_MAX_SUBJECT_LEN} characters",
        )));
    }
    Ok(normalized.to_string())
}

pub fn normalize_mail_template_body_config_value(key: &str, value: &str) -> Result<String> {
    let normalized = normalize_multiline(value);
    if normalized.trim().is_empty() {
        return Err(AsterError::validation_error(format!(
            "{key} cannot be empty"
        )));
    }
    if normalized.len() > MAIL_TEMPLATE_MAX_BODY_LEN {
        return Err(AsterError::validation_error(format!(
            "{key} must be at most {MAIL_TEMPLATE_MAX_BODY_LEN} characters",
        )));
    }
    Ok(normalized)
}

fn parse_port(value: &str) -> Option<u16> {
    value.trim().parse::<u16>().ok().filter(|port| *port > 0)
}

fn validate_contact_email(email: &str) -> Result<()> {
    if email.len() > 254 {
        return Err(AsterError::validation_error("email is too long"));
    }
    let parts: Vec<&str> = email.splitn(2, '@').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() || !parts[1].contains('.') {
        return Err(AsterError::validation_error("invalid email format"));
    }
    Ok(())
}

fn normalize_multiline(value: &str) -> String {
    value.replace("\r\n", "\n").replace('\r', "\n")
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_MAIL_SECURITY, DEFAULT_MAIL_SMTP_PORT, MAIL_SECURITY_KEY, MAIL_SMTP_PORT_KEY,
        RuntimeMailSettings, default_template_subject, normalize_mail_security_config_value,
        normalize_mail_template_body_config_value, normalize_mail_template_subject_config_value,
        template_html, template_subject,
    };
    use crate::config::RuntimeConfig;
    use crate::config::definitions::CONFIG_CATEGORY_MAIL_CONFIG;
    use crate::entities::system_config;
    use crate::types::MailTemplateCode;
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
            category: CONFIG_CATEGORY_MAIL_CONFIG.to_string(),
            description: "test".to_string(),
            updated_at: Utc::now(),
            updated_by: None,
        }
    }

    #[test]
    fn runtime_mail_settings_use_secure_defaults_when_config_missing() {
        let runtime_config = RuntimeConfig::new();
        let settings = RuntimeMailSettings::from_runtime_config(&runtime_config);

        assert_eq!(settings.smtp_port, DEFAULT_MAIL_SMTP_PORT);
        assert_eq!(settings.encryption_enabled, DEFAULT_MAIL_SECURITY);
    }

    #[test]
    fn runtime_mail_settings_read_boolean_security_values() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(config_model(MAIL_SMTP_PORT_KEY, "465"));
        runtime_config.apply(config_model(MAIL_SECURITY_KEY, "false"));

        let settings = RuntimeMailSettings::from_runtime_config(&runtime_config);

        assert_eq!(settings.smtp_port, 465);
        assert!(!settings.encryption_enabled);
    }

    #[test]
    fn normalize_mail_security_config_value_normalizes_boolean_values() {
        assert_eq!(
            normalize_mail_security_config_value(" true ").unwrap(),
            "true"
        );
        assert_eq!(
            normalize_mail_security_config_value("OFF").unwrap(),
            "false"
        );
    }

    #[test]
    fn template_defaults_are_used_when_runtime_config_missing() {
        let runtime_config = RuntimeConfig::new();

        assert_eq!(
            template_subject(&runtime_config, MailTemplateCode::RegisterActivation),
            default_template_subject(MailTemplateCode::RegisterActivation)
        );
        assert!(
            template_html(&runtime_config, MailTemplateCode::RegisterActivation)
                .starts_with("<!doctype html>")
        );
        assert!(
            template_html(&runtime_config, MailTemplateCode::RegisterActivation)
                .contains("{{verification_url}}")
        );
    }

    #[test]
    fn normalize_mail_template_subject_rejects_newlines() {
        assert!(normalize_mail_template_subject_config_value("subject", "hello\nworld").is_err());
    }

    #[test]
    fn normalize_mail_template_body_normalizes_crlf() {
        assert_eq!(
            normalize_mail_template_body_config_value("body", "line1\r\nline2").unwrap(),
            "line1\nline2"
        );
    }
}

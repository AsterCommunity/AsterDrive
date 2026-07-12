//! 配置子模块：`mail`。

use crate::config::RuntimeConfig;
use aster_forge_mail::{MailRuntimeSettings, MailTemplateCode};

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

pub fn runtime_mail_settings(runtime_config: &RuntimeConfig) -> MailRuntimeSettings {
    MailRuntimeSettings {
        smtp_host: runtime_config.get(MAIL_SMTP_HOST_KEY).unwrap_or_default(),
        smtp_port: runtime_config
            .get(MAIL_SMTP_PORT_KEY)
            .and_then(|raw| aster_forge_mail::parse_smtp_port(&raw))
            .unwrap_or(aster_forge_mail::DEFAULT_MAIL_SMTP_PORT),
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
        encryption_enabled: runtime_config
            .get_bool_or(MAIL_SECURITY_KEY, aster_forge_mail::DEFAULT_MAIL_SECURITY),
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

#[cfg(test)]
mod tests {
    use super::{
        MAIL_SECURITY_KEY, MAIL_SMTP_PORT_KEY, default_template_subject, runtime_mail_settings,
        template_html, template_subject,
    };
    use crate::config::RuntimeConfig;
    use crate::config::definitions::CONFIG_CATEGORY_MAIL_CONFIG;
    use aster_forge_db::system_config;
    use aster_forge_mail::MailTemplateCode;
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
            category: CONFIG_CATEGORY_MAIL_CONFIG.to_string(),
            description: "test".to_string(),
            updated_at: Utc::now(),
            updated_by: None,
        }
    }

    #[test]
    fn runtime_mail_settings_use_secure_defaults_when_config_missing() {
        let runtime_config = RuntimeConfig::new();
        let settings = runtime_mail_settings(&runtime_config);

        assert_eq!(settings.smtp_port, aster_forge_mail::DEFAULT_MAIL_SMTP_PORT);
        assert_eq!(
            settings.encryption_enabled,
            aster_forge_mail::DEFAULT_MAIL_SECURITY
        );
    }

    #[test]
    fn runtime_mail_settings_read_boolean_security_values() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(config_model(MAIL_SMTP_PORT_KEY, "465"));
        runtime_config.apply(config_model(MAIL_SECURITY_KEY, "false"));

        let settings = runtime_mail_settings(&runtime_config);

        assert_eq!(settings.smtp_port, 465);
        assert!(!settings.encryption_enabled);
    }

    #[test]
    fn normalize_mail_security_config_value_normalizes_boolean_values() {
        assert_eq!(
            aster_forge_mail::normalize_mail_security_config_value(" true ").unwrap(),
            "true"
        );
        assert_eq!(
            aster_forge_mail::normalize_mail_security_config_value("OFF").unwrap(),
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
        assert!(
            aster_forge_mail::normalize_mail_template_subject_config_value(
                "subject",
                "hello\nworld"
            )
            .is_err()
        );
    }

    #[test]
    fn normalize_mail_template_body_normalizes_crlf() {
        assert_eq!(
            aster_forge_mail::normalize_mail_template_body_config_value("body", "line1\r\nline2")
                .unwrap(),
            "line1\nline2"
        );
    }
}

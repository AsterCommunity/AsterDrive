use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// 邮件模板代码
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
#[serde(rename_all = "snake_case")]
pub enum MailTemplateCode {
    #[sea_orm(string_value = "register_activation")]
    RegisterActivation,
    #[sea_orm(string_value = "contact_change_confirmation")]
    ContactChangeConfirmation,
    #[sea_orm(string_value = "password_reset")]
    PasswordReset,
    #[sea_orm(string_value = "password_reset_notice")]
    PasswordResetNotice,
    #[sea_orm(string_value = "contact_change_notice")]
    ContactChangeNotice,
    #[sea_orm(string_value = "external_auth_email_verification")]
    ExternalAuthEmailVerification,
    #[sea_orm(string_value = "login_email_code")]
    LoginEmailCode,
    #[sea_orm(string_value = "user_invitation")]
    UserInvitation,
}

impl MailTemplateCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RegisterActivation => "register_activation",
            Self::ContactChangeConfirmation => "contact_change_confirmation",
            Self::PasswordReset => "password_reset",
            Self::PasswordResetNotice => "password_reset_notice",
            Self::ContactChangeNotice => "contact_change_notice",
            Self::ExternalAuthEmailVerification => "external_auth_email_verification",
            Self::LoginEmailCode => "login_email_code",
            Self::UserInvitation => "user_invitation",
        }
    }
}

/// Raw JSON payload stored in `mail_outbox.payload_json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, DeriveValueType)]
pub struct StoredMailPayload(pub String);

impl StoredMailPayload {
    pub const CLEARED_JSON: &str = "{}";

    pub fn cleared() -> Self {
        Self(Self::CLEARED_JSON.to_string())
    }
}

impl AsRef<str> for StoredMailPayload {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for StoredMailPayload {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<StoredMailPayload> for String {
    fn from(value: StoredMailPayload) -> Self {
        value.0
    }
}

/// 邮件 outbox 状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "snake_case")]
pub enum MailOutboxStatus {
    #[sea_orm(string_value = "pending")]
    Pending,
    #[sea_orm(string_value = "processing")]
    Processing,
    #[sea_orm(string_value = "retry")]
    Retry,
    #[sea_orm(string_value = "sent")]
    Sent,
    #[sea_orm(string_value = "failed")]
    Failed,
}

impl MailOutboxStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Sent | Self::Failed)
    }
}

#[cfg(test)]
mod tests {
    use super::{MailOutboxStatus, MailTemplateCode, StoredMailPayload};

    #[test]
    fn mail_template_code_exposes_stable_storage_names() {
        assert_eq!(
            MailTemplateCode::RegisterActivation.as_str(),
            "register_activation"
        );
        assert_eq!(
            MailTemplateCode::ContactChangeConfirmation.as_str(),
            "contact_change_confirmation"
        );
        assert_eq!(MailTemplateCode::PasswordReset.as_str(), "password_reset");
        assert_eq!(
            MailTemplateCode::PasswordResetNotice.as_str(),
            "password_reset_notice"
        );
        assert_eq!(
            MailTemplateCode::ContactChangeNotice.as_str(),
            "contact_change_notice"
        );
        assert_eq!(
            MailTemplateCode::ExternalAuthEmailVerification.as_str(),
            "external_auth_email_verification"
        );
        assert_eq!(
            MailTemplateCode::LoginEmailCode.as_str(),
            "login_email_code"
        );
        assert_eq!(MailTemplateCode::UserInvitation.as_str(), "user_invitation");
    }

    #[test]
    fn stored_mail_payload_helpers_preserve_raw_json() {
        let payload = StoredMailPayload::from("{\"token\":\"abc\"}".to_string());
        assert_eq!(payload.as_ref(), "{\"token\":\"abc\"}");

        let raw: String = payload.into();
        assert_eq!(raw, "{\"token\":\"abc\"}");
        assert_eq!(StoredMailPayload::cleared().as_ref(), "{}");
    }

    #[test]
    fn mail_outbox_status_terminal_states_are_explicit() {
        assert!(!MailOutboxStatus::Pending.is_terminal());
        assert!(!MailOutboxStatus::Processing.is_terminal());
        assert!(!MailOutboxStatus::Retry.is_terminal());
        assert!(MailOutboxStatus::Sent.is_terminal());
        assert!(MailOutboxStatus::Failed.is_terminal());
    }
}

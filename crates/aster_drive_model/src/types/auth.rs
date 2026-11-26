//! 联系方式验证与 MFA 认证类型。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// 联系方式验证渠道
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "snake_case")]
pub enum VerificationChannel {
    /// Verification code delivered by email.
    #[sea_orm(string_value = "email")]
    Email,
    /// Verification code delivered by SMS.
    #[sea_orm(string_value = "phone")]
    Phone,
}

/// 联系方式验证用途
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
#[serde(rename_all = "snake_case")]
pub enum VerificationPurpose {
    /// Activate a newly registered account.
    #[sea_orm(string_value = "register_activation")]
    RegisterActivation,
    /// Confirm a pending email / phone contact change.
    #[sea_orm(string_value = "contact_change")]
    ContactChange,
    /// Confirm identity before resetting the password.
    #[sea_orm(string_value = "password_reset")]
    PasswordReset,
}

/// 持久化 MFA factor 类型。
///
/// 这个枚举只描述会长期绑定到用户账号、并保存进 `mfa_factors.method` 的认证因子。
/// 目前只有 TOTP，因为它需要保存加密后的共享密钥并支持启用/删除等管理操作。
///
/// 注意不要把登录挑战里的临时验证方式加到这里：
/// - 恢复码独立保存在 `mfa_recovery_codes`，不是 factor 行；
/// - 邮箱验证码独立保存在 `mfa_email_codes`，是某次登录 flow 的短期 challenge code；
/// - 如果只是“本次 MFA challenge 可以用什么验证”，应使用下面的 `MfaMethod`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    schema(as = MfaPersistentFactorType)
)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "snake_case")]
pub enum MfaPersistentFactorMethod {
    /// Time-based one-time password factor backed by an encrypted shared secret.
    #[sea_orm(string_value = "totp")]
    Totp,
}

impl MfaPersistentFactorMethod {
    /// Returns the snake_case method name shared by API and storage values.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Totp => "totp",
        }
    }
}

/// MFA challenge 可用验证方法。
///
/// 这个枚举描述“某一次登录 flow 允许用户拿什么来完成第二步验证”。
/// 它可以包含持久化 factor 之外的短期方法，所以范围比 `MfaPersistentFactorMethod` 更宽。
/// 例如 `EmailCode` 只代表当前登录 flow 中发送到已验证邮箱的一次性验证码，
/// 不代表用户有一个持久化的 email MFA factor。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    schema(as = MfaChallengeMethodType)
)]
#[serde(rename_all = "snake_case")]
pub enum MfaMethod {
    /// Verification via the user's persisted TOTP factor.
    Totp,
    /// Verification via a one-time code from `mfa_recovery_codes`.
    RecoveryCode,
    /// Verification via a short-lived code mailed to a verified address.
    EmailCode,
}

impl MfaMethod {
    /// Returns the snake_case method name used by login flow responses.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Totp => "totp",
            Self::RecoveryCode => "recovery_code",
            Self::EmailCode => "email_code",
        }
    }
}

/// MFA flow 的第一因子来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
#[serde(rename_all = "snake_case")]
pub enum MfaFirstFactor {
    /// First factor completed with the account password.
    #[sea_orm(string_value = "password")]
    Password,
    /// First factor completed through an external identity provider.
    #[sea_orm(string_value = "external_auth")]
    ExternalAuth,
}

impl MfaFirstFactor {
    /// Returns the snake_case factor name used by storage and audit values.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::ExternalAuth => "external_auth",
        }
    }
}

/// JWT Token 类型（不存 DB）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum TokenType {
    /// Short-lived token used to authorize API requests.
    Access,
    /// Long-lived token used to obtain new access tokens.
    Refresh,
}

impl TokenType {
    /// Returns the lowercase token type name used in JWT claims.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Access => "access",
            Self::Refresh => "refresh",
        }
    }
}

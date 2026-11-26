//! 用户角色、状态与头像来源类型。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// 用户角色
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    /// Instance-level administrator with global management rights.
    #[sea_orm(string_value = "admin")]
    Admin,
    /// Regular user account.
    #[sea_orm(string_value = "user")]
    User,
}

impl UserRole {
    /// Returns whether this role carries instance administration rights.
    pub fn is_admin(&self) -> bool {
        matches!(self, Self::Admin)
    }
}

/// 用户状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "lowercase")]
pub enum UserStatus {
    /// Account can sign in and operate normally.
    #[sea_orm(string_value = "active")]
    Active,
    /// Account was disabled by an administrator and cannot sign in.
    #[sea_orm(string_value = "disabled")]
    Disabled,
}

impl UserStatus {
    /// Returns whether the account is enabled.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }
}

/// 用户头像来源
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "snake_case")]
pub enum AvatarSource {
    /// No avatar; clients render a generated fallback.
    #[sea_orm(string_value = "none")]
    None,
    /// Avatar resolved from the user's email through Gravatar.
    #[sea_orm(string_value = "gravatar")]
    Gravatar,
    /// Avatar image uploaded by the user.
    #[sea_orm(string_value = "upload")]
    Upload,
}

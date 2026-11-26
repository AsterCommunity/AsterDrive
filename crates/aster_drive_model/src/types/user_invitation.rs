//! 用户邀请状态。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// 用户邀请状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "snake_case")]
pub enum UserInvitationStatus {
    /// Invitation was sent and still awaits acceptance.
    #[sea_orm(string_value = "pending")]
    Pending,
    /// Invitee completed registration with this invitation.
    #[sea_orm(string_value = "accepted")]
    Accepted,
    /// Invitation was not accepted before its expiry.
    #[sea_orm(string_value = "expired")]
    Expired,
    /// Invitation was revoked before acceptance.
    #[sea_orm(string_value = "revoked")]
    Revoked,
}

impl UserInvitationStatus {
    /// Returns whether the invitation still awaits acceptance.
    pub fn is_pending(self) -> bool {
        matches!(self, Self::Pending)
    }
}

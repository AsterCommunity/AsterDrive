//! SeaORM 实体定义：`user`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::{StoredUserConfig, UserRole, UserStatus};

/// User account row: identity, authentication state, storage accounting and
/// routing to a storage policy profile.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), schema(as = UserInfo))]
#[sea_orm(table_name = "users")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Login name, unique across the instance.
    #[sea_orm(unique)]
    pub username: String,
    /// Email address, unique across the instance; used for contact
    /// verification and notifications.
    #[sea_orm(unique)]
    pub email: String,
    /// Local password hash (algorithm-tagged string); never serialized to
    /// API responses.
    #[serde(skip_serializing)]
    pub password_hash: String,
    /// Instance-wide permission level.
    pub role: UserRole,
    /// Account lifecycle state; disabled users fail authentication.
    pub status: UserStatus,
    /// Whether the user must change their password before doing anything
    /// else (e.g. after admin-issued reset).
    pub must_change_password: bool,
    /// Monotonic counter bumping invalidates all previously issued tokens
    /// and sessions for this user; never serialized.
    #[serde(skip_serializing)]
    pub session_version: i64,
    /// When the email address was verified; `None` while unverified.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub email_verified_at: Option<DateTimeUtc>,
    /// Email address awaiting verification before replacing the current one;
    /// `None` when no change is in flight.
    pub pending_email: Option<String>,
    /// Total storage bytes currently attributed to the user; kept consistent
    /// with upload/finalization and quota checks.
    pub storage_used: i64,
    /// Personal storage quota in bytes; `0` = unlimited.
    pub storage_quota: i64,
    /// Storage policy profile routing this user's uploads; `None` falls back
    /// to the system default profile.
    pub policy_group_id: Option<i64>,
    /// Row creation timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    /// Row last-update timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTimeUtc,
    /// Versioned user preference blob (JSON); `None` before first write.
    /// Never serialized to API responses.
    #[serde(skip_serializing)]
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub config: Option<StoredUserConfig>,
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// Personal files owned by this user.
    #[sea_orm(has_many = "super::file::Entity")]
    Files,
    /// Personal folders owned by this user.
    #[sea_orm(has_many = "super::folder::Entity")]
    Folders,
    /// Contact (email) verification tokens issued to this user.
    #[sea_orm(has_many = "super::contact_verification_token::Entity")]
    ContactVerificationTokens,
    /// Storage policy profile routing this user (FK `policy_group_id`).
    #[sea_orm(
        belongs_to = "super::storage_policy_group::Entity",
        from = "Column::PolicyGroupId",
        to = "super::storage_policy_group::Column::Id"
    )]
    StoragePolicyGroup,
}

impl Related<super::file::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Files.def()
    }
}

impl Related<super::folder::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Folders.def()
    }
}

impl Related<super::contact_verification_token::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ContactVerificationTokens.def()
    }
}

impl Related<super::storage_policy_group::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::StoragePolicyGroup.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

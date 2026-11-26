//! SeaORM 实体定义：`user_profile`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::AvatarSource;

/// Per-user profile extension row (1:1 with `users`): display name, WOPI
/// identity info and avatar state.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), schema(as = UserProfileModel))]
#[sea_orm(table_name = "user_profiles")]
pub struct Model {
    /// Owning user; part of a one-to-one mapping with `users.id`.
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: i64,
    /// Name shown instead of the login name; `None` means "use username".
    pub display_name: Option<String>,
    /// Extra identity fields WOPI client apps request for this user
    /// (connector-supplied, app-specific JSON); `None` when not configured.
    pub wopi_user_info: Option<String>,
    /// Which avatar representation is active.
    pub avatar_source: AvatarSource,
    /// Storage key of the uploaded avatar object; `None` unless
    /// `avatar_source` is upload (or the upload was cleared).
    pub avatar_key: Option<String>,
    /// Monotonic avatar revision starting at 1 and incremented on every
    /// avatar change; doubles as the compare-and-swap guard for concurrent
    /// uploads and busts cached avatar URLs.
    pub avatar_version: i32,
    /// Row creation timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    /// Row last-update timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTimeUtc,
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// The owning user account (FK `user_id`).
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id"
    )]
    User,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

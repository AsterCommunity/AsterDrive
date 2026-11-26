//! SeaORM 实体定义：`webdav_account`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Dedicated WebDAV credential row: a separate username/password pair bound
/// to an owner (and optional team/workspace scope), independent of the
/// user's login password.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), schema(as = WebdavAccount))]
#[sea_orm(table_name = "webdav_accounts")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Owning user; authenticating this account operates with that user's
    /// identity.
    pub user_id: i64,
    /// WebDAV Basic-auth username, unique across the instance.
    #[sea_orm(unique)]
    pub username: String,
    /// WebDAV Basic-auth password hash; never serialized to API responses.
    #[serde(skip_serializing)]
    pub password_hash: String,
    /// Restricts the account to this folder subtree; `None` exposes the
    /// user's/team's whole file tree.
    pub root_folder_id: Option<i64>,
    /// Team workspace this account is scoped to; `None` for personal-scope
    /// accounts.
    pub team_id: Option<i64>,
    /// Whether the account accepts WebDAV logins; disabled accounts are
    /// kept for audit but fail authentication.
    pub is_active: bool,
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
    /// Owning user whose identity this account operates with (FK `user_id`).
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id"
    )]
    User,
    /// Team workspace this account is scoped to (FK `team_id`).
    #[sea_orm(
        belongs_to = "super::team::Entity",
        from = "Column::TeamId",
        to = "super::team::Column::Id"
    )]
    Team,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl Related<super::team::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Team.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

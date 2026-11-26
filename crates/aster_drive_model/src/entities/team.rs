//! SeaORM 实体定义：`team`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Team workspace row: membership scope, storage accounting and routing to a
/// storage policy profile.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), schema(as = TeamInfo))]
#[sea_orm(table_name = "teams")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Team display name.
    pub name: String,
    /// Free-form team description (`""` when unset).
    pub description: String,
    /// User who created the team.
    pub created_by: i64,
    /// Total storage bytes currently attributed to the team; kept consistent
    /// with upload/finalization and quota checks.
    pub storage_used: i64,
    /// Team storage quota in bytes; `0` = unlimited.
    pub storage_quota: i64,
    /// Storage policy profile routing this team's uploads; `None` falls back
    /// to the system default profile.
    pub policy_group_id: Option<i64>,
    /// Row creation timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    /// Row last-update timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTimeUtc,
    /// When the team was archived; `None` while active. Archived teams are
    /// read-only for members.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub archived_at: Option<DateTimeUtc>,
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// The user who created the team (FK `created_by`).
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::CreatedBy",
        to = "super::user::Column::Id"
    )]
    Creator,
    /// Storage policy profile routing this team (FK `policy_group_id`).
    #[sea_orm(
        belongs_to = "super::storage_policy_group::Entity",
        from = "Column::PolicyGroupId",
        to = "super::storage_policy_group::Column::Id"
    )]
    StoragePolicyGroup,
    /// Membership rows of this team.
    #[sea_orm(has_many = "super::team_member::Entity")]
    TeamMembers,
    /// Team-scoped files.
    #[sea_orm(has_many = "super::file::Entity")]
    Files,
    /// Team-scoped folders.
    #[sea_orm(has_many = "super::folder::Entity")]
    Folders,
    /// Team-scoped shares.
    #[sea_orm(has_many = "super::share::Entity")]
    Shares,
    /// Upload sessions opened within the team scope.
    #[sea_orm(has_many = "super::upload_session::Entity")]
    UploadSessions,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Creator.def()
    }
}

impl Related<super::storage_policy_group::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::StoragePolicyGroup.def()
    }
}

impl Related<super::team_member::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::TeamMembers.def()
    }
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

impl Related<super::share::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Shares.def()
    }
}

impl Related<super::upload_session::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::UploadSessions.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

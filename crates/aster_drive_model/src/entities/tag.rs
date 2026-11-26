//! SeaORM 实体定义：`tag`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::TagScopeType;

/// User-facing label that can be attached to files/folders, scoped to a
/// single user (personal) or a team.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), schema(as = TagEntity))]
#[sea_orm(table_name = "tags")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Which scope the tag lives in; determines which of the two owner
    /// columns below is populated.
    pub scope_type: TagScopeType,
    /// Owning user for personal tags; `None` for team tags.
    pub owner_user_id: Option<i64>,
    /// Owning team for team tags; `None` for personal tags.
    pub team_id: Option<i64>,
    /// Tag name as entered by the user.
    pub name: String,
    /// `name` trimmed and lowercased; uniqueness key within the tag's scope.
    pub normalized_name: String,
    /// Display color as a lowercase `#rrggbb` hex string.
    pub color: String,
    /// Manual ordering position within the tag list; lower value sorts first.
    pub sort_order: i32,
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
    /// Owning user for personal tags (FK `owner_user_id`).
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::OwnerUserId",
        to = "super::user::Column::Id"
    )]
    OwnerUser,
    /// Owning team for team tags (FK `team_id`).
    #[sea_orm(
        belongs_to = "super::team::Entity",
        from = "Column::TeamId",
        to = "super::team::Column::Id"
    )]
    Team,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::OwnerUser.def()
    }
}

impl Related<super::team::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Team.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

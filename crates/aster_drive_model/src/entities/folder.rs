//! SeaORM 实体定义：`folder`。

use sea_orm::entity::prelude::*;
use sea_orm::{ConnectionTrait, Set};
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Folder row in either a personal or a team workspace. Deleting a folder is
/// soft: it moves the subtree to trash via `deleted_at` until purge or restore.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), schema(as = FolderEntity))]
#[sea_orm(table_name = "folders")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Folder display name; unique among non-deleted siblings of `parent_id`.
    pub name: String,
    /// Parent folder; `None` for a workspace root folder directly under the
    /// user or team root.
    pub parent_id: Option<i64>,
    /// Owning team for team workspaces; `None` for personal folders.
    pub team_id: Option<i64>,
    /// Owning user for personal workspaces; `None` for team folders.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_user_id: Option<i64>,
    /// Creator; `None` for migrated or system-provisioned rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_user_id: Option<i64>,
    /// Creator display name captured at creation; kept as text even if the
    /// creator account is later removed.
    pub created_by_username: String,
    /// Last editor; `None` until the folder is modified after creation.
    pub modified_by_user_id: Option<i64>,
    /// Last editor display name; `None` until the folder is modified after
    /// creation.
    pub modified_by_username: Option<String>,
    /// Last modification timestamp; `None` until the folder is modified after
    /// creation.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub modified_at: Option<DateTimeUtc>,
    /// Storage policy override for this subtree; `None` inherits the parent
    /// folder's or the workspace's policy.
    pub policy_id: Option<i64>,
    /// Presentation icon type; `icon_value` interprets according to this kind.
    pub icon_kind: crate::types::FolderIconKind,
    /// Icon payload (built-in icon name or emoji); `None` for the default icon.
    pub icon_value: Option<String>,
    /// Row creation timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    /// Row last-update timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTimeUtc,
    /// Soft-delete timestamp; `NULL` while the folder is live.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub deleted_at: Option<DateTimeUtc>,
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// Owning user of a personal folder (FK `owner_user_id`).
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::OwnerUserId",
        to = "super::user::Column::Id"
    )]
    OwnerUser,
    /// The user who created this folder (FK `created_by_user_id`).
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::CreatedByUserId",
        to = "super::user::Column::Id"
    )]
    CreatedByUser,
    /// Storage policy override applied to this subtree (FK `policy_id`).
    #[sea_orm(
        belongs_to = "super::storage_policy::Entity",
        from = "Column::PolicyId",
        to = "super::storage_policy::Column::Id"
    )]
    StoragePolicy,
    /// Owning team of a team folder (FK `team_id`).
    #[sea_orm(
        belongs_to = "super::team::Entity",
        from = "Column::TeamId",
        to = "super::team::Column::Id"
    )]
    Team,
    /// Files directly contained in this folder (not recursive).
    #[sea_orm(has_many = "super::file::Entity")]
    Files,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::OwnerUser.def()
    }
}

impl Related<super::storage_policy::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::StoragePolicy.def()
    }
}

impl Related<super::team::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Team.def()
    }
}

impl Related<super::file::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Files.def()
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        if insert {
            self.initialize_attribution();
        }
        Ok(self)
    }
}

impl ActiveModel {
    /// Initialize attribution only for genuinely new resources, never for
    /// migrated rows: copies the creator identity and creation time into the
    /// `modified_by_*` / `modified_at` columns.
    pub fn initialize_attribution(&mut self) {
        self.modified_by_user_id = self.created_by_user_id.clone();
        if let Some(name) = self.created_by_username.clone().take() {
            self.modified_by_username = Set(Some(name));
        }
        if let Some(at) = self.created_at.clone().take() {
            self.modified_at = Set(Some(at));
        }
    }
}

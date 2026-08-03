//! SeaORM 实体定义：`resource_lock`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::{
    EntityType, LockDepth, LockMode, LockOrigin, LockRootKind, StoredLockOwnerInfo,
};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), schema(as = ResourceLock))]
#[sea_orm(table_name = "resource_locks")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique)]
    pub token: String,
    pub namespace_id: i64,
    pub root_kind: LockRootKind,
    pub root_folder_id: Option<i64>,
    pub root_file_id: Option<i64>,
    pub depth: LockDepth,
    pub mode: LockMode,
    pub origin: LockOrigin,
    pub holder_user_id: Option<i64>,
    #[sea_orm(column_type = "Text", nullable)]
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub owner_info: Option<StoredLockOwnerInfo>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub timeout_at: Option<DateTimeUtc>,
    pub lockroot_path: Option<String>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::resource_lock_namespace::Entity",
        from = "Column::NamespaceId",
        to = "super::resource_lock_namespace::Column::Id",
        on_delete = "Cascade",
        on_update = "Cascade"
    )]
    Namespace,
    #[sea_orm(
        belongs_to = "super::folder::Entity",
        from = "Column::RootFolderId",
        to = "super::folder::Column::Id",
        on_delete = "Cascade",
        on_update = "Cascade"
    )]
    RootFolder,
    #[sea_orm(
        belongs_to = "super::file::Entity",
        from = "Column::RootFileId",
        to = "super::file::Column::Id",
        on_delete = "Cascade",
        on_update = "Cascade"
    )]
    RootFile,
}

impl Related<super::resource_lock_namespace::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Namespace.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

impl Model {
    #[must_use]
    pub const fn entity_type(&self) -> Option<EntityType> {
        match self.root_kind {
            LockRootKind::File => Some(EntityType::File),
            LockRootKind::Folder => Some(EntityType::Folder),
            LockRootKind::WorkspaceRoot => None,
        }
    }

    #[must_use]
    pub const fn entity_id(&self) -> Option<i64> {
        match self.root_kind {
            LockRootKind::File => self.root_file_id,
            LockRootKind::Folder => self.root_folder_id,
            LockRootKind::WorkspaceRoot => None,
        }
    }

    #[must_use]
    pub fn path(&self) -> &str {
        self.lockroot_path.as_deref().unwrap_or("/")
    }

    #[must_use]
    pub const fn shared(&self) -> bool {
        matches!(self.mode, LockMode::Shared)
    }

    #[must_use]
    pub const fn deep(&self) -> bool {
        matches!(self.depth, LockDepth::Infinity)
    }

    #[must_use]
    pub const fn owner_id(&self) -> Option<i64> {
        self.holder_user_id
    }
}

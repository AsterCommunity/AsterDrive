//! SeaORM 实体定义：`resource_lock`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::{
    EntityType, LockDepth, LockMode, LockOrigin, LockRootKind, StoredLockOwnerInfo,
};

/// One resource lock (WebDAV/WOPI/product) on a file, folder, or whole
/// workspace. All locks live in the namespace of the workspace they protect;
/// conflict checks and lock discovery run against that namespace generation.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), schema(as = ResourceLock))]
#[sea_orm(table_name = "resource_locks")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Opaque lock token handed to protocol clients and used in
    /// `If`-header style submissions; unique.
    #[sea_orm(unique)]
    pub token: String,
    /// Lock namespace of the workspace owning this lock; all conflict checks
    /// are scoped to it.
    pub namespace_id: i64,
    /// Kind of entity the lock is rooted on; `workspace_root` locks the whole
    /// workspace and leaves both root id columns empty.
    pub root_kind: LockRootKind,
    /// Locked folder id when `root_kind` is `folder`; `None` otherwise.
    pub root_folder_id: Option<i64>,
    /// Locked file id when `root_kind` is `file`; `None` otherwise.
    pub root_file_id: Option<i64>,
    /// Whether the lock covers only the root resource or its whole subtree.
    pub depth: LockDepth,
    /// Compatibility mode against other locks on the same hierarchy.
    pub mode: LockMode,
    /// Product integration that created this lock (WebDAV, WOPI, product UI).
    pub origin: LockOrigin,
    /// Account holding the lock; `None` for application-owned locks.
    pub holder_user_id: Option<i64>,
    /// Protocol-specific owner info (JSON) reported by the lock client, e.g.
    /// WebDAV `<owner>`; `None` when the client sent none.
    #[sea_orm(column_type = "Text", nullable)]
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub owner_info: Option<StoredLockOwnerInfo>,
    /// Expiry timestamp; `None` for locks without a timeout. Expired locks are
    /// pruned lazily and must not conflict with new operations.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub timeout_at: Option<DateTimeUtc>,
    /// Lock root path as last presented to the holder; display/compat only and
    /// never authoritative for authorization or discovery (ancestor renames
    /// make it stale).
    pub lockroot_path: Option<String>,
    /// Lock creation timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// Workspace namespace this lock is scoped to (FK `namespace_id`,
    /// cascading).
    #[sea_orm(
        belongs_to = "super::resource_lock_namespace::Entity",
        from = "Column::NamespaceId",
        to = "super::resource_lock_namespace::Column::Id",
        on_delete = "Cascade",
        on_update = "Cascade"
    )]
    Namespace,
    /// Locked folder when `root_kind` is `folder` (FK `root_folder_id`,
    /// cascading).
    #[sea_orm(
        belongs_to = "super::folder::Entity",
        from = "Column::RootFolderId",
        to = "super::folder::Column::Id",
        on_delete = "Cascade",
        on_update = "Cascade"
    )]
    RootFolder,
    /// Locked file when `root_kind` is `file` (FK `root_file_id`, cascading).
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
    /// Entity type of the lock root; `None` for workspace-root locks.
    #[must_use]
    pub const fn entity_type(&self) -> Option<EntityType> {
        match self.root_kind {
            LockRootKind::File => Some(EntityType::File),
            LockRootKind::Folder => Some(EntityType::Folder),
            LockRootKind::WorkspaceRoot => None,
        }
    }

    /// Entity id of the lock root; `None` for workspace-root locks.
    #[must_use]
    pub const fn entity_id(&self) -> Option<i64> {
        match self.root_kind {
            LockRootKind::File => self.root_file_id,
            LockRootKind::Folder => self.root_folder_id,
            LockRootKind::WorkspaceRoot => None,
        }
    }

    /// Lock root path as last presented to the holder; `/` when unset.
    #[must_use]
    pub fn path(&self) -> &str {
        self.lockroot_path.as_deref().unwrap_or("/")
    }

    /// Whether the lock is compatible with other shared locks on the same
    /// hierarchy.
    #[must_use]
    pub const fn shared(&self) -> bool {
        matches!(self.mode, LockMode::Shared)
    }

    /// Whether the lock covers the whole subtree below its root.
    #[must_use]
    pub const fn deep(&self) -> bool {
        matches!(self.depth, LockDepth::Infinity)
    }

    /// Account holding the lock; `None` for application-owned locks.
    #[must_use]
    pub const fn owner_id(&self) -> Option<i64> {
        self.holder_user_id
    }
}

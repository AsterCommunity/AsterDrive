//! SeaORM entity for workspace lock serialization namespaces.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use crate::types::LockWorkspaceType;

/// Lock serialization domain for one workspace. Lock rows reference the
/// namespace of their workspace; the monotonic `generation` counter versions
/// the workspace's lock state so projections and caches keyed on it stay
/// consistent across lock mutations.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "resource_lock_namespaces")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Workspace kind (personal or team) this namespace serializes locks for;
    /// unique together with `workspace_id`.
    pub workspace_type: LockWorkspaceType,
    /// Id of the user (personal) or team (team) workspace; unique together
    /// with `workspace_type`.
    pub workspace_id: i64,
    /// Monotonic lock-state version, incremented on every lock mutation in
    /// this namespace and used as the cache key for lock projections;
    /// overflow aborts the mutation.
    pub generation: i64,
    /// Row creation timestamp (UTC).
    pub created_at: DateTimeUtc,
    /// Last generation bump timestamp (UTC).
    pub updated_at: DateTimeUtc,
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// Locks scoped to this namespace.
    #[sea_orm(has_many = "super::resource_lock::Entity")]
    ResourceLock,
}

impl Related<super::resource_lock::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ResourceLock.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

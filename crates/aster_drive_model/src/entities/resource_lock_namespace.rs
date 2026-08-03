//! SeaORM entity for workspace lock serialization namespaces.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use crate::types::LockWorkspaceType;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "resource_lock_namespaces")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub workspace_type: LockWorkspaceType,
    pub workspace_id: i64,
    pub generation: i64,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::resource_lock::Entity")]
    ResourceLock,
}

impl Related<super::resource_lock::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ResourceLock.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

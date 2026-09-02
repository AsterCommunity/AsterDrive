//! Encrypted connector credentials for follower remote storage targets.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "remote_storage_target_credentials")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub target_id: i64,
    pub connector_id: String,
    pub schema_version: i32,
    pub revision: i64,
    #[serde(skip_serializing)]
    pub ciphertext: String,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::remote_storage_target::Entity",
        from = "Column::TargetId",
        to = "super::remote_storage_target::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    RemoteStorageTarget,
}

impl Related<super::remote_storage_target::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::RemoteStorageTarget.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

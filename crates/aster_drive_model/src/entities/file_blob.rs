//! SeaORM 实体定义：`file_blob`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "file_blobs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub hash: String, // sha256 or synthetic blob key
    pub size: i64,
    pub policy_id: i64,
    pub storage_path: String,
    pub thumbnail_path: Option<String>,
    pub thumbnail_processor: Option<String>,
    pub thumbnail_version: Option<String>,
    pub ref_count: i32,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::storage_policy::Entity",
        from = "Column::PolicyId",
        to = "super::storage_policy::Column::Id"
    )]
    StoragePolicy,
    #[sea_orm(has_many = "super::file::Entity")]
    Files,
}

impl Related<super::storage_policy::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::StoragePolicy.def()
    }
}

impl Related<super::file::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Files.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

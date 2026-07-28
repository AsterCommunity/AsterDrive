//! SeaORM 实体定义：`storage_migration_checkpoints`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "storage_migration_checkpoints")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub task_id: i64,
    pub source_policy_id: i64,
    pub target_policy_id: i64,
    pub plan_hash: String,
    pub stage: String,
    pub last_processed_blob_id: i64,
    pub scanned_blobs: i64,
    pub migrated_blobs: i64,
    pub merged_blobs: i64,
    pub skipped_blobs: i64,
    pub failed_blobs: i64,
    pub migrated_bytes: i64,
    pub renamed_opaque_blobs: i64,
    pub last_error: Option<String>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::background_task::Entity",
        from = "Column::TaskId",
        to = "super::background_task::Column::Id"
    )]
    BackgroundTask,
}

impl Related<super::background_task::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::BackgroundTask.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

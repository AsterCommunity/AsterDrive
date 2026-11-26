//! SeaORM 实体定义：`storage_migration_checkpoints`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Resumable checkpoint for one storage-policy migration background task:
/// records the immutable plan identity, the current stage, the resume cursor,
/// and running blob counters. Rows are created by the migration task and
/// deleted with it.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "storage_migration_checkpoints")]
pub struct Model {
    /// Migration background task this checkpoint belongs to; one row per task.
    #[sea_orm(primary_key, auto_increment = false)]
    pub task_id: i64,
    /// Storage policy blobs are migrated from.
    pub source_policy_id: i64,
    /// Storage policy blobs are migrated to.
    pub target_policy_id: i64,
    /// Hash of the migration plan; recovery must present the same hash or the
    /// checkpoint is rejected as stale.
    pub plan_hash: String,
    /// Migration pipeline stage the task is currently in, e.g.
    /// `prepare_policies`, `migrate_virtual_empty`, `migrate_blobs`, `complete`.
    pub stage: String,
    /// Resume cursor: id of the last blob processed in the current scan; the
    /// task continues after it.
    pub last_processed_blob_id: i64,
    /// Blobs examined so far in the migration scan.
    pub scanned_blobs: i64,
    /// Blobs copied to the target policy so far.
    pub migrated_blobs: i64,
    /// Blobs merged into an identical blob already on the target (refs
    /// redirected, source row deleted) so far.
    pub merged_blobs: i64,
    /// Blobs skipped because they already live on the target policy and only
    /// needed a cursor advance.
    pub skipped_blobs: i64,
    /// Blobs that failed to migrate; in recover-available mode missing source
    /// blobs are counted here and the run continues, other errors abort.
    pub failed_blobs: i64,
    /// Logical bytes migrated so far.
    pub migrated_bytes: i64,
    /// Opaque-named blobs renamed to canonical keys on the target so far.
    pub renamed_opaque_blobs: i64,
    /// Error text from the latest failed stage or blob; `None` when healthy.
    pub last_error: Option<String>,
    /// Row creation timestamp (UTC).
    pub created_at: DateTimeUtc,
    /// Last checkpoint update timestamp (UTC).
    pub updated_at: DateTimeUtc,
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// Migration background task owning this checkpoint (FK `task_id`).
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

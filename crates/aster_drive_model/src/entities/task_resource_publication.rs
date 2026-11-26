//! Durable product-file receipts for at-least-once task execution.
use sea_orm::entity::prelude::*;

/// Durable receipt that a logical task entry published a product file/folder,
/// keyed by (task, entry) so re-execution stays idempotent.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "task_resource_publications")]
pub struct Model {
    /// Producing background task; part of the composite primary key and
    /// cascade-deleted with the task row.
    #[sea_orm(primary_key, auto_increment = false)]
    pub task_id: i64,
    /// SHA-256 hex digest of the logical entry key identifying one output
    /// resource of the task; part of the composite primary key.
    #[sea_orm(primary_key, auto_increment = false)]
    pub entry_key: String,
    /// Serialized JSON snapshot of the published resource (file or folder
    /// model) captured at publication time for idempotent re-execution.
    pub resource_snapshot: String,
}

/// SeaORM relation definitions for this entity: intentionally empty, receipts
/// are keyed by task id and have no relation entries.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

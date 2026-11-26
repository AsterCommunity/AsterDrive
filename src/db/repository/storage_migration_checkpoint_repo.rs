//! `storage_migration_checkpoint` 仓储模块。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, EntityTrait,
    ExprTrait, QueryFilter, RelationTrait, Set, sea_query::Expr,
};

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::storage_migration_checkpoint::{
    self, Entity as StorageMigrationCheckpoint,
};

/// Values for creating a new storage migration checkpoint row.
#[derive(Debug, Clone)]
pub struct CreateCheckpointInput<'a> {
    /// Background task that owns the migration.
    pub task_id: i64,
    /// Id of the storage policy blobs migrate from.
    pub source_policy_id: i64,
    /// Id of the storage policy blobs migrate to.
    pub target_policy_id: i64,
    /// Hash of the migration plan; checkpoints with a different plan are incompatible.
    pub plan_hash: &'a str,
    /// Current migration stage name.
    pub stage: &'a str,
}

/// Counters to add to a checkpoint's progress totals.
#[derive(Debug, Clone, Copy, Default)]
pub struct CheckpointDelta {
    /// Additional blobs scanned since the last checkpoint update.
    pub scanned_blobs: i64,
    /// Additional blobs migrated to the target policy.
    pub migrated_blobs: i64,
    /// Additional blobs merged into existing target content.
    pub merged_blobs: i64,
    /// Additional blobs skipped by the migration plan.
    pub skipped_blobs: i64,
    /// Additional blobs whose migration failed.
    pub failed_blobs: i64,
    /// Additional bytes migrated since the last checkpoint update.
    pub migrated_bytes: i64,
    /// Additional opaque-named blobs renamed during migration.
    pub renamed_opaque_blobs: i64,
}

/// Creates a fresh checkpoint row for a migration task with zeroed progress counters.
pub async fn create<C: ConnectionTrait>(
    db: &C,
    input: CreateCheckpointInput<'_>,
) -> Result<storage_migration_checkpoint::Model> {
    let now = Utc::now();
    storage_migration_checkpoint::ActiveModel {
        task_id: Set(input.task_id),
        source_policy_id: Set(input.source_policy_id),
        target_policy_id: Set(input.target_policy_id),
        plan_hash: Set(input.plan_hash.to_string()),
        stage: Set(input.stage.to_string()),
        last_processed_blob_id: Set(0),
        scanned_blobs: Set(0),
        migrated_blobs: Set(0),
        merged_blobs: Set(0),
        skipped_blobs: Set(0),
        failed_blobs: Set(0),
        migrated_bytes: Set(0),
        renamed_opaque_blobs: Set(0),
        last_error: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await
    .map_err(AsterError::from)
}

/// Finds a migration checkpoint by its owning task id.
pub async fn find_by_task_id(
    db: &DatabaseConnection,
    task_id: i64,
) -> Result<Option<storage_migration_checkpoint::Model>> {
    StorageMigrationCheckpoint::find_by_id(task_id)
        .one(db)
        .await
        .map_err(AsterError::from)
}

/// Loads a migration checkpoint by task id.
///
/// Errors with `RecordNotFound` when no checkpoint exists for the task.
pub async fn get_by_task_id(
    db: &DatabaseConnection,
    task_id: i64,
) -> Result<storage_migration_checkpoint::Model> {
    find_by_task_id(db, task_id).await?.ok_or_else(|| {
        AsterError::record_not_found(format!("storage migration checkpoint #{task_id}"))
    })
}

/// Atomically advances a checkpoint: sets the stage, cursor, and last error, and adds
/// the delta to every progress counter.
///
/// Errors with `RecordNotFound` when the checkpoint does not exist; the updated row is
/// reloaded before returning.
pub async fn advance<C: ConnectionTrait>(
    db: &C,
    task_id: i64,
    stage: &str,
    last_processed_blob_id: i64,
    delta: CheckpointDelta,
    last_error: Option<&str>,
) -> Result<storage_migration_checkpoint::Model> {
    let updated = Utc::now();
    let result = StorageMigrationCheckpoint::update_many()
        .col_expr(
            storage_migration_checkpoint::Column::Stage,
            Expr::value(stage.to_string()),
        )
        .col_expr(
            storage_migration_checkpoint::Column::LastProcessedBlobId,
            Expr::value(last_processed_blob_id),
        )
        .col_expr(
            storage_migration_checkpoint::Column::ScannedBlobs,
            Expr::col(storage_migration_checkpoint::Column::ScannedBlobs).add(delta.scanned_blobs),
        )
        .col_expr(
            storage_migration_checkpoint::Column::MigratedBlobs,
            Expr::col(storage_migration_checkpoint::Column::MigratedBlobs)
                .add(delta.migrated_blobs),
        )
        .col_expr(
            storage_migration_checkpoint::Column::MergedBlobs,
            Expr::col(storage_migration_checkpoint::Column::MergedBlobs).add(delta.merged_blobs),
        )
        .col_expr(
            storage_migration_checkpoint::Column::SkippedBlobs,
            Expr::col(storage_migration_checkpoint::Column::SkippedBlobs).add(delta.skipped_blobs),
        )
        .col_expr(
            storage_migration_checkpoint::Column::FailedBlobs,
            Expr::col(storage_migration_checkpoint::Column::FailedBlobs).add(delta.failed_blobs),
        )
        .col_expr(
            storage_migration_checkpoint::Column::MigratedBytes,
            Expr::col(storage_migration_checkpoint::Column::MigratedBytes)
                .add(delta.migrated_bytes),
        )
        .col_expr(
            storage_migration_checkpoint::Column::RenamedOpaqueBlobs,
            Expr::col(storage_migration_checkpoint::Column::RenamedOpaqueBlobs)
                .add(delta.renamed_opaque_blobs),
        )
        .col_expr(
            storage_migration_checkpoint::Column::LastError,
            Expr::value(last_error.map(str::to_string)),
        )
        .col_expr(
            storage_migration_checkpoint::Column::UpdatedAt,
            Expr::value(updated),
        )
        .filter(storage_migration_checkpoint::Column::TaskId.eq(task_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    if result.rows_affected == 0 {
        return Err(AsterError::record_not_found(format!(
            "storage migration checkpoint #{task_id}"
        )));
    }
    StorageMigrationCheckpoint::find_by_id(task_id)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| {
            AsterError::record_not_found(format!("storage migration checkpoint #{task_id}"))
        })
}

/// Replaces a checkpoint's stage and last error without touching progress counters.
///
/// Errors with `RecordNotFound` when the checkpoint does not exist.
pub async fn set_stage(
    db: &DatabaseConnection,
    task_id: i64,
    stage: &str,
    last_error: Option<&str>,
) -> Result<storage_migration_checkpoint::Model> {
    let result = StorageMigrationCheckpoint::update_many()
        .col_expr(
            storage_migration_checkpoint::Column::Stage,
            Expr::value(stage.to_string()),
        )
        .col_expr(
            storage_migration_checkpoint::Column::LastError,
            Expr::value(last_error.map(str::to_string)),
        )
        .col_expr(
            storage_migration_checkpoint::Column::UpdatedAt,
            Expr::value(Utc::now()),
        )
        .filter(storage_migration_checkpoint::Column::TaskId.eq(task_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    if result.rows_affected == 0 {
        return Err(AsterError::record_not_found(format!(
            "storage migration checkpoint #{task_id}"
        )));
    }
    get_by_task_id(db, task_id).await
}

/// Reports whether an active (pending, processing, or retrying) migration conflicts with
/// the given source and target policies.
///
/// A policy that is the source of a new migration must not participate in any active
/// migration in any role; a target policy may accept multiple inbound migrations but
/// must not already be migrating out.
pub async fn has_active_conflict<C: ConnectionTrait>(
    db: &C,
    source_policy_id: i64,
    target_policy_id: i64,
) -> Result<bool> {
    use aster_drive_model::entities::background_task;
    use aster_drive_model::types::BackgroundTaskStatus;
    use sea_orm::QuerySelect;

    let existing = StorageMigrationCheckpoint::find()
        .join(
            sea_orm::JoinType::InnerJoin,
            storage_migration_checkpoint::Relation::BackgroundTask.def(),
        )
        .filter(
            Condition::any()
                .add(storage_migration_checkpoint::Column::SourcePolicyId.eq(source_policy_id))
                .add(storage_migration_checkpoint::Column::TargetPolicyId.eq(source_policy_id))
                .add(storage_migration_checkpoint::Column::SourcePolicyId.eq(target_policy_id)),
        )
        .filter(background_task::Column::Status.is_in([
            BackgroundTaskStatus::Pending,
            BackgroundTaskStatus::Processing,
            BackgroundTaskStatus::Retry,
        ]))
        .select_only()
        .column(storage_migration_checkpoint::Column::TaskId)
        .into_tuple::<i64>()
        .one(db)
        .await
        .map_err(AsterError::from)?;

    Ok(existing.is_some())
}

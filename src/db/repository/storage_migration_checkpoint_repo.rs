//! `storage_migration_checkpoint` 仓储模块。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, EntityTrait, ExprTrait, QueryFilter,
    RelationTrait, Set, sea_query::Expr,
};

use crate::entities::storage_migration_checkpoint::{self, Entity as StorageMigrationCheckpoint};
use crate::errors::{AsterError, Result};

#[derive(Debug, Clone)]
pub struct CreateCheckpointInput<'a> {
    pub task_id: i64,
    pub source_policy_id: i64,
    pub target_policy_id: i64,
    pub plan_hash: &'a str,
    pub stage: &'a str,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CheckpointDelta {
    pub scanned_blobs: i64,
    pub migrated_blobs: i64,
    pub merged_blobs: i64,
    pub skipped_blobs: i64,
    pub failed_blobs: i64,
    pub migrated_bytes: i64,
    pub renamed_opaque_blobs: i64,
}

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

pub async fn find_by_task_id<C: ConnectionTrait>(
    db: &C,
    task_id: i64,
) -> Result<Option<storage_migration_checkpoint::Model>> {
    StorageMigrationCheckpoint::find_by_id(task_id)
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn get_by_task_id<C: ConnectionTrait>(
    db: &C,
    task_id: i64,
) -> Result<storage_migration_checkpoint::Model> {
    find_by_task_id(db, task_id).await?.ok_or_else(|| {
        AsterError::record_not_found(format!("storage migration checkpoint #{task_id}"))
    })
}

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
    get_by_task_id(db, task_id).await
}

pub async fn set_stage<C: ConnectionTrait>(
    db: &C,
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

pub async fn has_active_conflict<C: ConnectionTrait>(
    db: &C,
    source_policy_id: i64,
    target_policy_id: i64,
) -> Result<bool> {
    use crate::entities::background_task;
    use crate::types::BackgroundTaskStatus;
    use sea_orm::QuerySelect;

    let existing = StorageMigrationCheckpoint::find()
        .join(
            sea_orm::JoinType::InnerJoin,
            storage_migration_checkpoint::Relation::BackgroundTask.def(),
        )
        .filter(
            Condition::any()
                // A policy that is the source of a new migration must not be
                // participating in any active migration.
                .add(storage_migration_checkpoint::Column::SourcePolicyId.eq(source_policy_id))
                .add(storage_migration_checkpoint::Column::TargetPolicyId.eq(source_policy_id))
                // A policy that is the target of a new migration may accept
                // multiple inbound migrations, but cannot already be migrating out.
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

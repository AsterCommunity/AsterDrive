//! 存储策略间 blob 迁移任务。

use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};

use crate::db::repository::{
    background_task_repo, file_repo, policy_repo, storage_migration_checkpoint_repo, version_repo,
};
use crate::db::transaction;
use crate::entities::{background_task, file_blob, storage_policy};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::PrimaryAppState;
use crate::storage::{MultipartStorageDriver, StorageDriver, StorageErrorKind};
use crate::types::BackgroundTaskKind;
use crate::utils::hash::{new_sha256, sha256_digest_to_hex, sha256_hex};
use crate::utils::numbers::{bytes_to_usize, u64_to_i64};

use super::spec::{self, StoragePolicyMigrationTask, decode_payload_as};
use super::steps::{
    TASK_STEP_FINISH, TASK_STEP_MIGRATE_BLOBS, TASK_STEP_PREPARE_SOURCES, TASK_STEP_SCAN_BLOBS,
    TASK_STEP_WAITING, parse_task_steps_json, set_task_step_active, set_task_step_succeeded,
};
use super::types::{
    StoragePolicyMigrationCapacityCheck, StoragePolicyMigrationDryRun,
    StoragePolicyMigrationDryRunWarning, StoragePolicyMigrationTaskPayload,
    StoragePolicyMigrationTaskResult, TaskInfo,
};
use super::{
    TaskExecutionContext, TypedTaskCreate, insert_typed_task_record, mark_task_progress,
    mark_task_succeeded, task_scope,
};

const MIGRATION_BATCH_SIZE: u64 = 100;
const MIGRATION_MULTIPART_MIN_PART_SIZE: i64 = 5 * 1024 * 1024;
const MIGRATION_MULTIPART_PREFERRED_MAX_PART_SIZE: i64 = 64 * 1024 * 1024;
const MIGRATION_MULTIPART_MAX_PARTS: i64 = 10_000;
const MIGRATION_MULTIPART_PART_UPLOAD_MAX_ATTEMPTS: usize = 3;
const CHECKPOINT_STAGE_PREPARE_POLICIES: &str = "prepare_policies";
const CHECKPOINT_STAGE_MIGRATE_BLOBS: &str = "migrate_blobs";
const CHECKPOINT_STAGE_COMPLETE: &str = "complete";

#[derive(Debug, Clone, Copy)]
pub(crate) struct CreateStoragePolicyMigrationInput {
    pub source_policy_id: i64,
    pub target_policy_id: i64,
    pub delete_source_after_success: bool,
    pub creator_user_id: i64,
}

#[derive(Debug, Clone, Copy, Default)]
struct BlobMigrationOutcome {
    scanned: i64,
    migrated: i64,
    merged: i64,
    skipped: i64,
    failed: i64,
    migrated_bytes: i64,
    renamed_opaque_blobs: i64,
}

struct BlobMigrationContext<'a> {
    state: &'a PrimaryAppState,
    execution: &'a TaskExecutionContext,
    task_id: i64,
    source_policy_id: i64,
    target_policy_id: i64,
    target_multipart_part_size: i64,
    source_driver: &'a dyn StorageDriver,
    target_driver: &'a dyn StorageDriver,
}

struct StoragePolicyMigrationPreflight {
    source_policy: storage_policy::Model,
    target_policy: storage_policy::Model,
    dry_run: StoragePolicyMigrationDryRun,
}

pub(crate) async fn create_storage_policy_migration_task(
    state: &PrimaryAppState,
    input: CreateStoragePolicyMigrationInput,
) -> Result<TaskInfo> {
    let preflight = build_storage_policy_migration_preflight(state, input).await?;
    if !preflight.dry_run.can_start {
        return Err(AsterError::validation_error(
            "target storage capacity is insufficient for this migration",
        ));
    }
    let source_policy = preflight.source_policy;
    let target_policy = preflight.target_policy;
    let plan_hash = migration_plan_hash(
        input.source_policy_id,
        input.target_policy_id,
        input.delete_source_after_success,
        &source_policy,
        &target_policy,
    );
    let payload = StoragePolicyMigrationTaskPayload {
        source_policy_id: input.source_policy_id,
        target_policy_id: input.target_policy_id,
        delete_source_after_success: input.delete_source_after_success,
        plan_hash: plan_hash.clone(),
        source_policy_updated_at: source_policy.updated_at,
        target_policy_updated_at: target_policy.updated_at,
    };

    let task = transaction::with_transaction(state.writer_db(), async |txn| {
        let first_policy_id = input.source_policy_id.min(input.target_policy_id);
        let second_policy_id = input.source_policy_id.max(input.target_policy_id);
        policy_repo::lock_by_id(txn, first_policy_id).await?;
        policy_repo::lock_by_id(txn, second_policy_id).await?;
        if storage_migration_checkpoint_repo::has_active_conflict(
            txn,
            input.source_policy_id,
            input.target_policy_id,
        )
        .await?
        {
            return Err(AsterError::validation_error(
                "a conflicting active storage policy migration already exists",
            ));
        }

        let task = insert_typed_task_record(
            state,
            txn,
            TypedTaskCreate::<StoragePolicyMigrationTask>::new(
                format!(
                    "Migrate storage policy #{} to #{}",
                    input.source_policy_id, input.target_policy_id
                ),
                payload.clone(),
            )
            .creator_user_id(Some(input.creator_user_id)),
        )
        .await?;
        storage_migration_checkpoint_repo::create(
            txn,
            storage_migration_checkpoint_repo::CreateCheckpointInput {
                task_id: task.id,
                source_policy_id: input.source_policy_id,
                target_policy_id: input.target_policy_id,
                plan_hash: &plan_hash,
                stage: CHECKPOINT_STAGE_PREPARE_POLICIES,
            },
        )
        .await?;
        Ok(task)
    })
    .await?;

    state.wake_background_task_dispatcher();
    super::get_task_in_scope(state, task_scope(&task)?, task.id).await
}

pub(crate) async fn dry_run_storage_policy_migration(
    state: &PrimaryAppState,
    input: CreateStoragePolicyMigrationInput,
) -> Result<StoragePolicyMigrationDryRun> {
    Ok(build_storage_policy_migration_preflight(state, input)
        .await?
        .dry_run)
}

async fn build_storage_policy_migration_preflight(
    state: &PrimaryAppState,
    input: CreateStoragePolicyMigrationInput,
) -> Result<StoragePolicyMigrationPreflight> {
    validate_storage_policy_migration_input(input)?;
    ensure_no_active_storage_policy_migration(state.writer_db(), input).await?;
    let source_policy = policy_repo::find_by_id(state.writer_db(), input.source_policy_id).await?;
    let target_policy = policy_repo::find_by_id(state.writer_db(), input.target_policy_id).await?;
    let _plan_hash = migration_plan_hash(
        input.source_policy_id,
        input.target_policy_id,
        input.delete_source_after_success,
        &source_policy,
        &target_policy,
    );
    let target_driver = state.driver_registry.get_driver(&target_policy)?;
    let target_supports_stream_upload = target_driver.as_stream_upload().is_some();
    if !target_supports_stream_upload {
        return Err(AsterError::storage_driver_error(
            "target storage policy does not support stream upload",
        ));
    }
    probe_storage_migration_target(target_driver.as_ref()).await?;

    let summary =
        file_repo::summarize_blobs_by_policy(state.writer_db(), input.source_policy_id).await?;
    let hash_kinds =
        file_repo::summarize_blob_hash_kinds_by_policy(state.writer_db(), input.source_policy_id)
            .await?;
    let missing_summary = file_repo::summarize_missing_blobs_between_policies(
        state.writer_db(),
        input.source_policy_id,
        input.target_policy_id,
    )
    .await?;
    let target_matching_blob_count = summary.count.saturating_sub(missing_summary.count);
    let opaque_key_conflict_count = file_repo::count_opaque_hash_conflicts_between_policies(
        state.writer_db(),
        input.source_policy_id,
        input.target_policy_id,
    )
    .await?;
    let target_capacity = crate::services::policy_service::capacity_info_or_status(
        target_driver.as_ref(),
        target_policy.driver_type,
    )
    .await;
    let target_capacity_check =
        migration_capacity_check(&target_capacity, missing_summary.total_size);
    let warnings = match target_capacity_check {
        StoragePolicyMigrationCapacityCheck::Unsupported
        | StoragePolicyMigrationCapacityCheck::Unavailable => {
            vec![StoragePolicyMigrationDryRunWarning::TargetCapacityUnavailable]
        }
        StoragePolicyMigrationCapacityCheck::Sufficient
        | StoragePolicyMigrationCapacityCheck::Insufficient => Vec::new(),
    };
    let can_start = storage_policy_migration_can_start(&target_capacity_check);

    Ok(StoragePolicyMigrationPreflight {
        source_policy,
        target_policy,
        dry_run: StoragePolicyMigrationDryRun {
            source_policy_id: input.source_policy_id,
            target_policy_id: input.target_policy_id,
            source_blob_count: summary.count,
            source_total_bytes: summary.total_size,
            content_sha256_blob_count: hash_kinds.content_sha256_count,
            opaque_blob_count: hash_kinds.opaque_count,
            target_matching_blob_count,
            estimated_copy_blob_count: missing_summary.count,
            opaque_key_conflict_count,
            target_supports_stream_upload,
            target_connection_ok: true,
            target_capacity_check,
            target_capacity,
            delete_source_after_success_supported: false,
            can_start,
            warnings,
        },
    })
}

fn migration_capacity_check(
    capacity: &crate::storage::StorageCapacityInfo,
    required_bytes: i64,
) -> StoragePolicyMigrationCapacityCheck {
    match capacity.status {
        crate::storage::StorageCapacityStatus::Supported => match capacity.available_bytes {
            Some(available) if available >= required_bytes => {
                StoragePolicyMigrationCapacityCheck::Sufficient
            }
            Some(_) => StoragePolicyMigrationCapacityCheck::Insufficient,
            None => StoragePolicyMigrationCapacityCheck::Unavailable,
        },
        crate::storage::StorageCapacityStatus::Unsupported => {
            StoragePolicyMigrationCapacityCheck::Unsupported
        }
        crate::storage::StorageCapacityStatus::Unavailable => {
            StoragePolicyMigrationCapacityCheck::Unavailable
        }
    }
}

fn storage_policy_migration_can_start(
    capacity_check: &StoragePolicyMigrationCapacityCheck,
) -> bool {
    !matches!(
        capacity_check,
        StoragePolicyMigrationCapacityCheck::Insufficient
    )
}

fn validate_storage_policy_migration_input(input: CreateStoragePolicyMigrationInput) -> Result<()> {
    if input.source_policy_id <= 0 || input.target_policy_id <= 0 {
        return Err(AsterError::validation_error(
            "source_policy_id and target_policy_id must be greater than 0",
        ));
    }
    if input.source_policy_id == input.target_policy_id {
        return Err(AsterError::validation_error(
            "source_policy_id and target_policy_id must be different",
        ));
    }
    if input.delete_source_after_success {
        return Err(AsterError::validation_error(
            "delete_source_after_success is not supported in the first storage migration version",
        ));
    }
    Ok(())
}

async fn ensure_no_active_storage_policy_migration<C: sea_orm::ConnectionTrait>(
    db: &C,
    input: CreateStoragePolicyMigrationInput,
) -> Result<()> {
    if storage_migration_checkpoint_repo::has_active_conflict(
        db,
        input.source_policy_id,
        input.target_policy_id,
    )
    .await?
    {
        return Err(AsterError::validation_error(
            "a conflicting active storage policy migration already exists",
        ));
    }
    Ok(())
}

pub(crate) async fn resume_storage_policy_migration_for_admin(
    state: &PrimaryAppState,
    task_id: i64,
    audit_ctx: &crate::services::audit_service::AuditContext,
) -> Result<TaskInfo> {
    let task = background_task_repo::find_by_id(state.writer_db(), task_id).await?;
    if task.kind != BackgroundTaskKind::StoragePolicyMigration {
        return Err(AsterError::validation_error(
            "only storage policy migration tasks can be resumed from this endpoint",
        ));
    }
    let scope = task_scope(&task)?;
    super::retry_task_in_scope_with_audit(state, scope, task_id, audit_ctx).await
}

async fn probe_storage_migration_target(driver: &dyn StorageDriver) -> Result<()> {
    let test_path = format!("_aster_migration_preflight-{}", uuid::Uuid::new_v4());
    driver.put(&test_path, b"ok").await.map_aster_err_ctx(
        "storage migration target write test",
        AsterError::storage_driver_error,
    )?;
    driver
        .delete(&test_path)
        .await
        .inspect_err(|error| {
            tracing::warn!(path = %test_path, "failed to clean up storage migration preflight object: {error}");
        })
        .map_aster_err_ctx(
            "storage migration target cleanup test",
            AsterError::storage_driver_error,
        )?;
    Ok(())
}

pub(super) async fn process_storage_policy_migration_task(
    state: &PrimaryAppState,
    task: &background_task::Model,
    context: TaskExecutionContext,
) -> Result<()> {
    let lease_guard = context.lease_guard().clone();
    let payload = decode_payload_as::<StoragePolicyMigrationTask>(task)?;
    let mut steps =
        parse_task_steps_json(task.steps_json.as_ref().map(|raw| raw.as_ref()), task.kind)?;
    set_task_step_succeeded(
        &mut steps,
        TASK_STEP_WAITING,
        Some("Worker claimed task"),
        None,
    )?;
    set_task_step_active(
        &mut steps,
        TASK_STEP_PREPARE_SOURCES,
        Some("Loading storage policies"),
        None,
    )?;
    mark_task_progress(
        state,
        &lease_guard,
        task.progress_current,
        task.progress_total,
        Some("Preparing storage migration"),
        &steps,
    )
    .await?;

    let source_policy =
        policy_repo::find_by_id(state.writer_db(), payload.source_policy_id).await?;
    let target_policy =
        policy_repo::find_by_id(state.writer_db(), payload.target_policy_id).await?;
    validate_migration_plan(&payload, &source_policy, &target_policy)?;
    let source_driver = state.driver_registry.get_driver(&source_policy)?;
    let target_driver = state.driver_registry.get_driver(&target_policy)?;
    if target_driver.as_stream_upload().is_none() {
        return Err(AsterError::storage_driver_error(
            "target storage policy does not support stream upload",
        ));
    }

    context.ensure_active()?;
    storage_migration_checkpoint_repo::set_stage(
        state.writer_db(),
        task.id,
        CHECKPOINT_STAGE_MIGRATE_BLOBS,
        None,
    )
    .await?;
    let mut checkpoint =
        storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task.id).await?;

    set_task_step_succeeded(
        &mut steps,
        TASK_STEP_PREPARE_SOURCES,
        Some("Storage policies are ready"),
        None,
    )?;
    set_task_step_active(
        &mut steps,
        TASK_STEP_SCAN_BLOBS,
        Some("Scanning source blobs"),
        None,
    )?;
    mark_task_progress(
        state,
        &lease_guard,
        checkpoint.scanned_blobs,
        checkpoint.scanned_blobs,
        Some("Scanning source blobs"),
        &steps,
    )
    .await?;

    let migration_context = BlobMigrationContext {
        state,
        execution: &context,
        task_id: task.id,
        source_policy_id: payload.source_policy_id,
        target_policy_id: payload.target_policy_id,
        target_multipart_part_size: target_policy.chunk_size,
        source_driver: source_driver.as_ref(),
        target_driver: target_driver.as_ref(),
    };

    loop {
        context.ensure_active()?;
        let blobs = file_repo::find_blobs_by_policy_paginated(
            state.writer_db(),
            payload.source_policy_id,
            checkpoint.last_processed_blob_id,
            MIGRATION_BATCH_SIZE,
        )
        .await?;
        if blobs.is_empty() {
            break;
        }

        set_task_step_succeeded(
            &mut steps,
            TASK_STEP_SCAN_BLOBS,
            Some("Source blob batch loaded"),
            None,
        )?;
        set_task_step_active(
            &mut steps,
            TASK_STEP_MIGRATE_BLOBS,
            Some("Migrating blobs"),
            None,
        )?;

        for blob in blobs {
            context.ensure_active()?;
            let outcome = migrate_one_blob(&migration_context, blob).await?;

            checkpoint =
                storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task.id)
                    .await?;
            let current = checkpoint
                .migrated_blobs
                .saturating_add(checkpoint.merged_blobs)
                .saturating_add(checkpoint.skipped_blobs)
                .saturating_add(checkpoint.failed_blobs);
            let total = checkpoint.scanned_blobs.max(current);
            mark_task_progress(
                state,
                &lease_guard,
                current,
                total,
                Some(&format!(
                    "Migrated {}, merged {}, skipped {} blob(s)",
                    checkpoint.migrated_blobs, checkpoint.merged_blobs, checkpoint.skipped_blobs
                )),
                &steps,
            )
            .await?;

            if outcome.failed > 0 {
                break;
            }
        }
    }

    context.ensure_active()?;
    checkpoint = storage_migration_checkpoint_repo::set_stage(
        state.writer_db(),
        task.id,
        CHECKPOINT_STAGE_COMPLETE,
        None,
    )
    .await?;
    set_task_step_succeeded(
        &mut steps,
        TASK_STEP_MIGRATE_BLOBS,
        Some("Blob migration finished"),
        None,
    )?;
    set_task_step_active(
        &mut steps,
        TASK_STEP_FINISH,
        Some("Finalizing migration"),
        None,
    )?;
    let result = StoragePolicyMigrationTaskResult {
        source_policy_id: payload.source_policy_id,
        target_policy_id: payload.target_policy_id,
        scanned_blobs: checkpoint.scanned_blobs,
        migrated_blobs: checkpoint.migrated_blobs,
        merged_blobs: checkpoint.merged_blobs,
        skipped_blobs: checkpoint.skipped_blobs,
        failed_blobs: checkpoint.failed_blobs,
        migrated_bytes: checkpoint.migrated_bytes,
        renamed_opaque_blobs: checkpoint.renamed_opaque_blobs,
    };
    let result_json = spec::serialize_result::<StoragePolicyMigrationTask>(&result)?;
    set_task_step_succeeded(
        &mut steps,
        TASK_STEP_FINISH,
        Some("Storage migration completed"),
        None,
    )?;
    let current = checkpoint
        .migrated_blobs
        .saturating_add(checkpoint.merged_blobs)
        .saturating_add(checkpoint.skipped_blobs)
        .saturating_add(checkpoint.failed_blobs);
    mark_task_succeeded(
        state,
        &lease_guard,
        Some(&result_json),
        current,
        checkpoint.scanned_blobs.max(current),
        Some("Storage migration completed"),
        &steps,
    )
    .await
}

async fn migrate_one_blob(
    migration: &BlobMigrationContext<'_>,
    blob: file_blob::Model,
) -> Result<BlobMigrationOutcome> {
    let BlobMigrationContext {
        state,
        execution: context,
        task_id,
        source_policy_id,
        target_policy_id,
        target_driver,
        ..
    } = *migration;
    let latest = match file_repo::find_blob_by_id(state.writer_db(), blob.id).await {
        Ok(blob) => blob,
        Err(error) if error.code() == "E006" => {
            return advance_checkpoint(
                state,
                task_id,
                blob.id,
                BlobMigrationOutcome {
                    scanned: 1,
                    skipped: 1,
                    ..Default::default()
                },
                None,
            )
            .await;
        }
        Err(error) => return Err(error),
    };
    if latest.policy_id != source_policy_id {
        return advance_checkpoint(
            state,
            task_id,
            latest.id,
            BlobMigrationOutcome {
                scanned: 1,
                skipped: 1,
                ..Default::default()
            },
            None,
        )
        .await;
    }

    let content_hash = is_content_sha256_blob_key(&latest.hash);
    let existing_target_blob =
        file_repo::find_blob_by_hash(state.writer_db(), &latest.hash, target_policy_id).await?;
    if let Some(target_blob) = existing_target_blob.as_ref()
        && content_hash
    {
        verify_existing_target(
            context,
            target_driver,
            target_blob,
            &latest.hash,
            latest.size,
        )
        .await?;
        return merge_blob_records(state, task_id, latest, target_blob.clone()).await;
    }
    let renamed_opaque_blob = !content_hash && existing_target_blob.is_some();
    let target_hash = if content_hash || !renamed_opaque_blob {
        latest.hash.clone()
    } else {
        format!("migration-{}", uuid::Uuid::new_v4())
    };
    let target_path = crate::utils::storage_path_from_blob_key(&target_hash);

    copy_blob_streaming(migration, &latest, &target_path).await?;
    let moved = transaction::with_transaction(state.writer_db(), async |txn| {
        let moved = file_repo::move_blob_policy_if_current(
            txn,
            latest.id,
            source_policy_id,
            target_policy_id,
            &target_hash,
            &target_path,
        )
        .await?;
        let outcome = if moved {
            BlobMigrationOutcome {
                scanned: 1,
                migrated: 1,
                migrated_bytes: latest.size,
                renamed_opaque_blobs: if renamed_opaque_blob { 1 } else { 0 },
                ..Default::default()
            }
        } else {
            BlobMigrationOutcome {
                scanned: 1,
                skipped: 1,
                ..Default::default()
            }
        };
        storage_migration_checkpoint_repo::advance(
            txn,
            task_id,
            CHECKPOINT_STAGE_MIGRATE_BLOBS,
            latest.id,
            checkpoint_delta(outcome),
            None,
        )
        .await?;
        Ok(outcome)
    })
    .await?;
    if moved.skipped > 0 {
        cleanup_unmoved_target_object(state, target_driver, target_policy_id, &target_path).await;
    }
    Ok(moved)
}

async fn cleanup_unmoved_target_object(
    state: &PrimaryAppState,
    target_driver: &dyn StorageDriver,
    target_policy_id: i64,
    target_path: &str,
) {
    if target_object_is_referenced(state, target_policy_id, target_path).await {
        return;
    }
    if let Err(error) = target_driver.delete(target_path).await {
        tracing::warn!(
            target_path,
            "failed to cleanup migrated target object after blob policy CAS miss: {error}"
        );
    }
}

async fn merge_blob_records(
    state: &PrimaryAppState,
    task_id: i64,
    old_blob: file_blob::Model,
    target_blob: file_blob::Model,
) -> Result<BlobMigrationOutcome> {
    transaction::with_transaction(state.writer_db(), async |txn| {
        let old_locked = file_repo::lock_blob_by_id(txn, old_blob.id).await?;
        if old_locked.policy_id != old_blob.policy_id {
            let outcome = BlobMigrationOutcome {
                scanned: 1,
                skipped: 1,
                ..Default::default()
            };
            storage_migration_checkpoint_repo::advance(
                txn,
                task_id,
                CHECKPOINT_STAGE_MIGRATE_BLOBS,
                old_blob.id,
                checkpoint_delta(outcome),
                None,
            )
            .await?;
            return Ok(outcome);
        }
        let target_locked = file_repo::lock_blob_by_id(txn, target_blob.id).await?;
        if target_locked.hash != old_locked.hash || target_locked.size != old_locked.size {
            return Err(AsterError::validation_error(
                "target blob no longer matches source blob",
            ));
        }
        file_repo::replace_file_blob_refs(txn, old_locked.id, target_locked.id).await?;
        version_repo::replace_version_blob_refs(txn, old_locked.id, target_locked.id).await?;
        file_repo::increment_blob_ref_count_by(txn, target_locked.id, old_locked.ref_count).await?;
        file_repo::delete_blob_by_id(txn, old_locked.id).await?;
        let outcome = BlobMigrationOutcome {
            scanned: 1,
            merged: 1,
            migrated_bytes: old_locked.size,
            ..Default::default()
        };
        storage_migration_checkpoint_repo::advance(
            txn,
            task_id,
            CHECKPOINT_STAGE_MIGRATE_BLOBS,
            old_locked.id,
            checkpoint_delta(outcome),
            None,
        )
        .await?;
        Ok(outcome)
    })
    .await
}

async fn advance_checkpoint(
    state: &PrimaryAppState,
    task_id: i64,
    last_processed_blob_id: i64,
    outcome: BlobMigrationOutcome,
    last_error: Option<&str>,
) -> Result<BlobMigrationOutcome> {
    storage_migration_checkpoint_repo::advance(
        state.writer_db(),
        task_id,
        CHECKPOINT_STAGE_MIGRATE_BLOBS,
        last_processed_blob_id,
        checkpoint_delta(outcome),
        last_error,
    )
    .await?;
    Ok(outcome)
}

fn checkpoint_delta(
    outcome: BlobMigrationOutcome,
) -> storage_migration_checkpoint_repo::CheckpointDelta {
    storage_migration_checkpoint_repo::CheckpointDelta {
        scanned_blobs: outcome.scanned,
        migrated_blobs: outcome.migrated,
        merged_blobs: outcome.merged,
        skipped_blobs: outcome.skipped,
        failed_blobs: outcome.failed,
        migrated_bytes: outcome.migrated_bytes,
        renamed_opaque_blobs: outcome.renamed_opaque_blobs,
    }
}

async fn copy_blob_streaming(
    migration: &BlobMigrationContext<'_>,
    blob: &file_blob::Model,
    target_path: &str,
) -> Result<()> {
    let BlobMigrationContext {
        state,
        execution: context,
        target_policy_id,
        target_multipart_part_size,
        source_driver,
        target_driver,
        ..
    } = *migration;
    context.ensure_active()?;
    if let Some(multipart) = target_driver.as_multipart()
        && should_use_multipart_migration(blob.size, target_multipart_part_size)?
    {
        // Large single PUT streams are not safely retryable: the S3 SDK cannot
        // clone an in-flight reader after a timeout. Multipart migration keeps
        // each retryable unit bounded and aborts the upload before the blob row
        // is moved if any part fails.
        return copy_blob_multipart(migration, multipart, blob, target_path).await;
    }

    let source_stream = source_driver.get_stream(&blob.storage_path).await?;
    context.ensure_active()?;
    let hashing_reader = HashingReader::new(source_stream, context.clone());
    let digest = hashing_reader.digest_handle();
    let stream_upload = target_driver.as_stream_upload().ok_or_else(|| {
        AsterError::storage_driver_error("target storage policy does not support stream upload")
    })?;
    let upload_result = stream_upload
        .put_reader(target_path, Box::new(hashing_reader), blob.size)
        .await;
    if let Err(error) = upload_result {
        // A streaming PUT can fail after the remote side has already accepted
        // bytes, especially when the client times out waiting for the response.
        // The blob row has not moved yet, so best-effort cleanup prevents
        // repeated migration retries from leaving orphan target objects behind.
        context.ensure_active()?;
        cleanup_failed_target_object(state, context, target_driver, target_policy_id, target_path)
            .await;
        return Err(error);
    }
    context.ensure_active()?;
    let verify_result = async {
        let copied_hash = digest.finish_hex()?;
        if is_content_sha256_blob_key(&blob.hash) && copied_hash != blob.hash {
            return Err(AsterError::storage_driver_error(format!(
                "copied blob hash mismatch for blob #{}",
                blob.id
            )));
        }
        verify_target_object(context, target_driver, target_path, &copied_hash, blob.size).await
    }
    .await;

    if let Err(error) = verify_result {
        cleanup_failed_target_object(state, context, target_driver, target_policy_id, target_path)
            .await;
        return Err(error);
    }

    Ok(())
}

async fn copy_blob_multipart(
    migration: &BlobMigrationContext<'_>,
    multipart: &dyn MultipartStorageDriver,
    blob: &file_blob::Model,
    target_path: &str,
) -> Result<()> {
    let BlobMigrationContext {
        state,
        execution: context,
        target_policy_id,
        target_multipart_part_size,
        source_driver,
        target_driver,
        ..
    } = *migration;
    context.ensure_active()?;
    let mut source_stream = source_driver.get_stream(&blob.storage_path).await?;
    let part_size = migration_multipart_part_size(blob.size, target_multipart_part_size)?;
    let upload_id = multipart.create_multipart_upload(target_path).await?;
    let mut completed_parts = Vec::new();
    let mut hasher = new_sha256();
    let mut remaining = blob.size;
    let mut part_number = 1_i32;
    let mut completed = false;

    let result = async {
        while remaining > 0 {
            context.ensure_active()?;
            let current_part_size = remaining.min(part_size);
            let part_bytes =
                read_multipart_part(&mut source_stream, current_part_size, &mut hasher).await?;
            let etag = upload_multipart_part_with_retry(
                multipart,
                target_path,
                &upload_id,
                part_number,
                part_bytes,
            )
            .await?;
            completed_parts.push((part_number, etag));
            remaining -= current_part_size;
            part_number = part_number.checked_add(1).ok_or_else(|| {
                AsterError::internal_error("storage migration multipart part number overflow")
            })?;
        }

        ensure_source_stream_finished(&mut source_stream).await?;
        context.ensure_active()?;
        complete_migration_multipart_upload(
            context,
            target_driver,
            multipart,
            target_path,
            &upload_id,
            completed_parts,
            blob.size,
        )
        .await?;
        completed = true;

        let copied_hash = sha256_digest_to_hex(&sha2::Digest::finalize(hasher));
        if is_content_sha256_blob_key(&blob.hash) && copied_hash != blob.hash {
            return Err(AsterError::storage_driver_error(format!(
                "copied blob hash mismatch for blob #{}",
                blob.id
            )));
        }
        verify_target_object(context, target_driver, target_path, &copied_hash, blob.size).await
    }
    .await;

    if let Err(error) = result {
        if !completed {
            abort_migration_multipart_upload(multipart, target_path, &upload_id).await;
        }
        cleanup_failed_target_object(state, context, target_driver, target_policy_id, target_path)
            .await;
        return Err(error);
    }

    Ok(())
}

fn should_use_multipart_migration(blob_size: i64, configured_part_size: i64) -> Result<bool> {
    if blob_size < 0 {
        return Err(AsterError::internal_error(format!(
            "storage migration blob size cannot be negative: {blob_size}"
        )));
    }
    Ok(blob_size > migration_multipart_part_size(blob_size, configured_part_size)?)
}

fn migration_multipart_part_size(blob_size: i64, configured_part_size: i64) -> Result<i64> {
    if blob_size < 0 {
        return Err(AsterError::internal_error(format!(
            "storage migration blob size cannot be negative: {blob_size}"
        )));
    }
    let configured_part_size = configured_part_size.clamp(
        MIGRATION_MULTIPART_MIN_PART_SIZE,
        MIGRATION_MULTIPART_PREFERRED_MAX_PART_SIZE,
    );
    let count_limited_part_size = if blob_size == 0 {
        MIGRATION_MULTIPART_MIN_PART_SIZE
    } else {
        blob_size
            .checked_add(MIGRATION_MULTIPART_MAX_PARTS - 1)
            .ok_or_else(|| {
                AsterError::internal_error("storage migration multipart size overflow")
            })?
            / MIGRATION_MULTIPART_MAX_PARTS
    };
    // Prefer bounded memory during migration, but S3-compatible providers cap
    // multipart uploads at 10,000 parts, so extremely large blobs may need a
    // larger part size to stay within the protocol limit.
    Ok(configured_part_size.max(count_limited_part_size))
}

async fn read_multipart_part(
    stream: &mut Box<dyn AsyncRead + Unpin + Send>,
    expected_size: i64,
    hasher: &mut sha2::Sha256,
) -> Result<Bytes> {
    let expected_size = bytes_to_usize(expected_size, "storage migration multipart part size")?;
    let mut data = vec![0_u8; expected_size];
    stream.read_exact(&mut data).await.map_aster_err_ctx(
        "read source object multipart part",
        AsterError::storage_driver_error,
    )?;
    sha2::Digest::update(hasher, &data);
    Ok(Bytes::from(data))
}

async fn ensure_source_stream_finished(
    stream: &mut Box<dyn AsyncRead + Unpin + Send>,
) -> Result<()> {
    let mut extra = [0_u8; 1];
    let read = stream.read(&mut extra).await.map_aster_err_ctx(
        "read source object after expected multipart size",
        AsterError::storage_driver_error,
    )?;
    if read > 0 {
        return Err(AsterError::storage_driver_error(
            "source object exceeds expected blob size",
        ));
    }
    Ok(())
}

async fn upload_multipart_part_with_retry(
    multipart: &dyn MultipartStorageDriver,
    target_path: &str,
    upload_id: &str,
    part_number: i32,
    part_bytes: Bytes,
) -> Result<String> {
    for attempt in 1..=MIGRATION_MULTIPART_PART_UPLOAD_MAX_ATTEMPTS {
        match multipart
            .upload_multipart_part_bytes(target_path, upload_id, part_number, part_bytes.clone())
            .await
        {
            Ok(etag) => return Ok(etag),
            Err(error)
                if storage_error_is_retryable(&error)
                    && attempt < MIGRATION_MULTIPART_PART_UPLOAD_MAX_ATTEMPTS =>
            {
                tracing::warn!(
                    target_path,
                    part_number,
                    attempt,
                    error = %error,
                    "storage migration multipart part upload failed; retrying"
                );
                tokio::time::sleep(std::time::Duration::from_millis(
                    200_u64.saturating_mul(u64::try_from(attempt).unwrap_or(u64::MAX)),
                ))
                .await;
            }
            Err(error) => return Err(error),
        }
    }
    Err(AsterError::internal_error(
        "storage migration multipart retry loop exhausted unexpectedly",
    ))
}

async fn complete_migration_multipart_upload(
    context: &TaskExecutionContext,
    target_driver: &dyn StorageDriver,
    multipart: &dyn MultipartStorageDriver,
    target_path: &str,
    upload_id: &str,
    completed_parts: Vec<(i32, String)>,
    expected_size: i64,
) -> Result<()> {
    if let Err(error) = multipart
        .complete_multipart_upload(target_path, upload_id, completed_parts)
        .await
    {
        if storage_error_is_retryable(&error)
            && let Ok(metadata) = target_driver.metadata(target_path).await
            && u64_to_i64(metadata.size, "completed multipart object size")? == expected_size
        {
            context.ensure_active()?;
            return Ok(());
        }
        return Err(error);
    }
    Ok(())
}

async fn abort_migration_multipart_upload(
    multipart: &dyn MultipartStorageDriver,
    target_path: &str,
    upload_id: &str,
) {
    if let Err(error) = multipart
        .abort_multipart_upload(target_path, upload_id)
        .await
    {
        tracing::warn!(
            target_path,
            upload_id,
            "failed to abort storage migration multipart upload: {error}"
        );
    }
}

fn storage_error_is_retryable(error: &AsterError) -> bool {
    matches!(
        error.storage_error_kind(),
        Some(StorageErrorKind::Transient | StorageErrorKind::RateLimited)
    )
}

async fn cleanup_failed_target_object(
    state: &PrimaryAppState,
    context: &TaskExecutionContext,
    target_driver: &dyn StorageDriver,
    target_policy_id: i64,
    target_path: &str,
) {
    if let Err(error) = context.ensure_active() {
        tracing::warn!(
            target_path,
            target_policy_id,
            "skip target object cleanup because task lease is no longer active: {error}"
        );
        return;
    }
    if target_object_is_referenced(state, target_policy_id, target_path).await {
        return;
    }
    if let Err(error) = context.ensure_active() {
        tracing::warn!(
            target_path,
            target_policy_id,
            "skip target object cleanup after reference check because task lease is no longer active: {error}"
        );
        return;
    }
    if let Err(cleanup_error) = target_driver.delete(target_path).await {
        tracing::warn!(
            target_path,
            "failed to cleanup migrated target object after verification error: {cleanup_error}"
        );
    }
}

async fn target_object_is_referenced(
    state: &PrimaryAppState,
    target_policy_id: i64,
    target_path: &str,
) -> bool {
    match file_repo::blob_storage_path_exists_for_policy(
        state.reader_db(),
        target_policy_id,
        target_path,
    )
    .await
    {
        Ok(true) => {
            tracing::debug!(
                target_path,
                target_policy_id,
                "skip target object cleanup because the path is already referenced"
            );
            true
        }
        Ok(false) => false,
        Err(error) => {
            tracing::warn!(
                target_path,
                target_policy_id,
                "failed to verify target object references before cleanup: {error}"
            );
            true
        }
    }
}

fn is_content_sha256_blob_key(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

async fn verify_existing_target(
    context: &TaskExecutionContext,
    target_driver: &dyn StorageDriver,
    target_blob: &file_blob::Model,
    source_hash: &str,
    source_size: i64,
) -> Result<()> {
    if target_blob.hash != source_hash || target_blob.size != source_size {
        return Err(AsterError::validation_error(
            "target blob record does not match source blob",
        ));
    }
    verify_target_object(
        context,
        target_driver,
        &target_blob.storage_path,
        source_hash,
        source_size,
    )
    .await
}

async fn verify_target_object(
    context: &TaskExecutionContext,
    target_driver: &dyn StorageDriver,
    target_path: &str,
    expected_hash: &str,
    expected_size: i64,
) -> Result<()> {
    context.ensure_active()?;
    let metadata = target_driver.metadata(target_path).await?;
    context.ensure_active()?;
    let actual_size = u64_to_i64(metadata.size, "target blob metadata size")?;
    if actual_size != expected_size {
        return Err(AsterError::storage_driver_error(format!(
            "target object size mismatch for {target_path}: expected {expected_size}, got {actual_size}"
        )));
    }
    let mut stream = target_driver.get_stream(target_path).await?;
    let mut hasher = new_sha256();
    let mut buf = vec![0_u8; 64 * 1024];
    loop {
        context.ensure_active()?;
        let read = stream.read(&mut buf).await.map_aster_err_ctx(
            "read target object for hash verification",
            AsterError::storage_driver_error,
        )?;
        if read == 0 {
            break;
        }
        sha2::Digest::update(&mut hasher, &buf[..read]);
    }
    let actual_hash = sha256_digest_to_hex(&sha2::Digest::finalize(hasher));
    if actual_hash != expected_hash {
        return Err(AsterError::storage_driver_error(format!(
            "target object hash mismatch for {target_path}"
        )));
    }
    Ok(())
}

fn validate_migration_plan(
    payload: &StoragePolicyMigrationTaskPayload,
    source_policy: &storage_policy::Model,
    target_policy: &storage_policy::Model,
) -> Result<()> {
    if payload.delete_source_after_success {
        return Err(AsterError::validation_error(
            "delete_source_after_success is not supported",
        ));
    }
    if source_policy.updated_at != payload.source_policy_updated_at
        || target_policy.updated_at != payload.target_policy_updated_at
    {
        return Err(AsterError::validation_error(
            "storage policy changed after migration task was created; create a new migration task",
        ));
    }
    let current_hash = migration_plan_hash(
        payload.source_policy_id,
        payload.target_policy_id,
        payload.delete_source_after_success,
        source_policy,
        target_policy,
    );
    if current_hash != payload.plan_hash {
        return Err(AsterError::validation_error(
            "storage migration plan no longer matches current policies",
        ));
    }
    Ok(())
}

fn migration_plan_hash(
    source_policy_id: i64,
    target_policy_id: i64,
    delete_source_after_success: bool,
    source_policy: &storage_policy::Model,
    target_policy: &storage_policy::Model,
) -> String {
    let plan = serde_json::json!({
        "source_policy_id": source_policy_id,
        "target_policy_id": target_policy_id,
        "delete_source_after_success": delete_source_after_success,
        "source": policy_identity(source_policy),
        "target": policy_identity(target_policy),
    });
    sha256_hex(plan.to_string().as_bytes())
}

fn policy_identity(policy: &storage_policy::Model) -> serde_json::Value {
    serde_json::json!({
        "id": policy.id,
        "driver_type": policy.driver_type,
        "endpoint": policy.endpoint,
        "bucket": policy.bucket,
        "base_path": policy.base_path,
        "remote_node_id": policy.remote_node_id,
        "options": policy.options.as_ref(),
        "chunk_size": policy.chunk_size,
        "updated_at": policy.updated_at,
    })
}

struct HashingReader {
    inner: Box<dyn AsyncRead + Unpin + Send + Sync>,
    digest: HashDigestHandle,
    context: TaskExecutionContext,
}

#[derive(Clone)]
struct HashDigestHandle(std::sync::Arc<std::sync::Mutex<Option<sha2::Sha256>>>);

impl HashingReader {
    fn new(inner: Box<dyn AsyncRead + Unpin + Send>, context: TaskExecutionContext) -> Self {
        Self {
            inner: Self::wrap_inner(inner),
            digest: HashDigestHandle(std::sync::Arc::new(std::sync::Mutex::new(Some(
                new_sha256(),
            )))),
            context,
        }
    }

    fn digest_handle(&self) -> HashDigestHandle {
        self.digest.clone()
    }
}

struct SyncRead {
    inner: std::sync::Mutex<Box<dyn AsyncRead + Unpin + Send>>,
}

impl SyncRead {
    fn new(inner: Box<dyn AsyncRead + Unpin + Send>) -> Self {
        Self {
            inner: std::sync::Mutex::new(inner),
        }
    }
}

impl AsyncRead for SyncRead {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.inner.lock() {
            Ok(mut inner) => Pin::new(&mut *inner).poll_read(cx, buf),
            Err(_) => Poll::Ready(Err(std::io::Error::other("sync read mutex poisoned"))),
        }
    }
}

impl Unpin for SyncRead {}

impl HashingReader {
    fn wrap_inner(
        inner: Box<dyn AsyncRead + Unpin + Send>,
    ) -> Box<dyn AsyncRead + Unpin + Send + Sync> {
        Box::new(SyncRead::new(inner))
    }
}

impl AsyncRead for HashingReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if let Err(error) = self.context.ensure_active() {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                error.to_string(),
            )));
        }

        let before = buf.filled().len();
        let poll = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &poll {
            let filled = buf.filled();
            let after = filled.len();
            if after > before
                && let Ok(mut guard) = self.digest.0.lock()
                && let Some(hasher) = guard.as_mut()
            {
                sha2::Digest::update(hasher, &filled[before..after]);
            }
        }
        poll
    }
}

impl HashDigestHandle {
    fn finish_hex(&self) -> Result<String> {
        let mut guard = self
            .0
            .lock()
            .map_err(|_| AsterError::internal_error("hashing reader digest lock poisoned"))?;
        let hasher = guard
            .take()
            .ok_or_else(|| AsterError::internal_error("hashing reader digest already finalized"))?;
        Ok(sha256_digest_to_hex(&sha2::Digest::finalize(hasher)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::task_service::{TaskLease, is_task_worker_shutdown_requested};
    use crate::storage::{StorageCapacityInfo, StorageCapacityStatus};
    use tokio_util::sync::CancellationToken;

    fn capacity(
        status: StorageCapacityStatus,
        available_bytes: Option<i64>,
    ) -> StorageCapacityInfo {
        StorageCapacityInfo {
            status,
            total_bytes: available_bytes,
            available_bytes,
            used_bytes: None,
            source: "test".to_string(),
            observed_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn migration_capacity_check_covers_supported_boundaries() {
        assert_eq!(
            migration_capacity_check(&capacity(StorageCapacityStatus::Supported, Some(100)), 100),
            StoragePolicyMigrationCapacityCheck::Sufficient
        );
        assert_eq!(
            migration_capacity_check(&capacity(StorageCapacityStatus::Supported, Some(101)), 100),
            StoragePolicyMigrationCapacityCheck::Sufficient
        );
        assert_eq!(
            migration_capacity_check(&capacity(StorageCapacityStatus::Supported, Some(99)), 100),
            StoragePolicyMigrationCapacityCheck::Insufficient
        );
        assert_eq!(
            migration_capacity_check(&capacity(StorageCapacityStatus::Supported, None), 100),
            StoragePolicyMigrationCapacityCheck::Unavailable
        );
    }

    #[test]
    fn migration_capacity_check_preserves_unsupported_and_unavailable() {
        assert_eq!(
            migration_capacity_check(&capacity(StorageCapacityStatus::Unsupported, None), 100),
            StoragePolicyMigrationCapacityCheck::Unsupported
        );
        assert_eq!(
            migration_capacity_check(&capacity(StorageCapacityStatus::Unavailable, None), 100),
            StoragePolicyMigrationCapacityCheck::Unavailable
        );
    }

    #[test]
    fn storage_policy_migration_can_start_only_blocks_confirmed_insufficient_capacity() {
        assert!(storage_policy_migration_can_start(
            &StoragePolicyMigrationCapacityCheck::Sufficient
        ));
        assert!(storage_policy_migration_can_start(
            &StoragePolicyMigrationCapacityCheck::Unsupported
        ));
        assert!(storage_policy_migration_can_start(
            &StoragePolicyMigrationCapacityCheck::Unavailable
        ));
        assert!(!storage_policy_migration_can_start(
            &StoragePolicyMigrationCapacityCheck::Insufficient
        ));
    }

    #[tokio::test]
    async fn hashing_reader_stops_when_shutdown_is_requested() {
        let shutdown_token = CancellationToken::new();
        let context = TaskExecutionContext::new(TaskLease::new(42, 7), shutdown_token.clone());
        shutdown_token.cancel();

        let mut reader =
            HashingReader::new(Box::new(tokio::io::repeat(1).take(1)), context.clone());
        let mut buffer = [0_u8; 1];

        let error = reader
            .read(&mut buffer)
            .await
            .expect_err("cancelled context should stop migration stream reads");
        assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);

        let shutdown_error = context
            .ensure_active()
            .expect_err("cancelled context should remain visible as a task shutdown");
        assert!(is_task_worker_shutdown_requested(&shutdown_error));
    }
}

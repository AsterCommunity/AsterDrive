//! 存储策略间 blob 迁移任务。

use aster_forge_tasks::TaskExecutionContext;
use std::pin::Pin;
use std::task::{Context, Poll};

use serde::Serialize;
use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};

use crate::db::repository::{
    background_task_repo, file_repo, policy_repo, revision_repo, storage_migration_checkpoint_repo,
};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState, TaskRuntimeState};
use aster_drive_model::entities::{background_task, file_blob, storage_policy};
use aster_drive_model::types::BackgroundTaskKind;
use aster_drive_storage::{
    MultipartStorageCapabilities, MultipartStorageDriver, MultipartUploadMode, StorageDriver,
    StorageErrorKind,
};
use aster_forge_crypto::{new_sha256, sha256_digest_to_hex, sha256_hex};
use aster_forge_db::transaction;
use aster_forge_tasks::{set_task_step_active, set_task_step_succeeded};
use aster_forge_utils::numbers::u64_to_i64;

use super::spec::{self, StoragePolicyMigrationTask, decode_payload_as};
use super::steps::{
    TASK_STEP_FINISH, TASK_STEP_MIGRATE_BLOBS, TASK_STEP_PREPARE_SOURCES, TASK_STEP_SCAN_BLOBS,
    TASK_STEP_WAITING, parse_task_steps_json,
};
use super::types::{
    StoragePolicyMigrationCapacityCheck, StoragePolicyMigrationDryRun,
    StoragePolicyMigrationDryRunWarning, StoragePolicyMigrationMode,
    StoragePolicyMigrationMultipartPlan, StoragePolicyMigrationMultipartUploadMode,
    StoragePolicyMigrationTaskPayload, StoragePolicyMigrationTaskResult, TaskInfo,
};
use super::{
    TypedTaskCreate, insert_typed_task_record, mark_task_progress, mark_task_succeeded, task_scope,
};

const MIGRATION_BATCH_SIZE: u64 = 100;
const MIGRATION_MULTIPART_MIN_PART_SIZE: i64 = 5 * 1024 * 1024;
const MIGRATION_MULTIPART_PREFERRED_MAX_PART_SIZE: i64 = 64 * 1024 * 1024;
const MIGRATION_MULTIPART_HEAP_BUDGET: i64 = 64 * 1024 * 1024;
const MIGRATION_MULTIPART_PART_UPLOAD_MAX_ATTEMPTS: usize = 3;
const CHECKPOINT_STAGE_PREPARE_POLICIES: &str = "prepare_policies";
const CHECKPOINT_STAGE_MIGRATE_VIRTUAL_EMPTY: &str = "migrate_virtual_empty";
const CHECKPOINT_STAGE_MIGRATE_BLOBS: &str = "migrate_blobs";
const CHECKPOINT_STAGE_COMPLETE: &str = "complete";

#[derive(Debug, Clone)]
pub(crate) struct CreateStoragePolicyMigrationInput {
    pub source_policy_id: i64,
    pub target_policy_id: i64,
    pub mode: StoragePolicyMigrationMode,
    pub recovery_plan_hash: Option<String>,
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
    source_driver: Option<&'a dyn StorageDriver>,
    target_driver: &'a dyn StorageDriver,
}

struct StoragePolicyMigrationPreflight {
    source_policy: storage_policy::Model,
    target_policy: storage_policy::Model,
    source_recovery_probe:
        Option<crate::services::storage_policy::recoverability::StoragePolicyRecoveryProbe>,
    dry_run: StoragePolicyMigrationDryRun,
}

#[derive(Debug, Clone, Copy)]
struct MigrationMultipartPartPlan {
    part_size: i64,
    part_count: i64,
    provider_max_parts: i64,
    provider_max_part_size: Option<i64>,
    upload_mode: MultipartUploadMode,
    can_start: bool,
}

struct MultipartPartRetry<'a> {
    multipart: &'a dyn MultipartStorageDriver,
    source_driver: &'a dyn StorageDriver,
    source_path: &'a str,
    target_path: &'a str,
    upload_id: &'a str,
    part_number: i32,
    offset: i64,
    part_size: i64,
    context: &'a TaskExecutionContext,
}

fn migration_multipart_part_plan(
    blob_size: i64,
    configured_part_size: i64,
    capabilities: MultipartStorageCapabilities,
) -> Result<MigrationMultipartPartPlan> {
    if blob_size < 0 {
        return Err(AsterError::internal_error(format!(
            "storage migration blob size cannot be negative: {blob_size}"
        )));
    }
    let max_parts = i64::try_from(capabilities.max_parts).map_err(|_| {
        AsterError::internal_error("storage migration multipart max parts exceeds i64 range")
    })?;
    if max_parts <= 0 {
        return Err(AsterError::validation_error(
            "storage migration multipart provider max parts must be positive",
        ));
    }
    let min_part_size = i64::try_from(capabilities.min_part_size).map_err(|_| {
        AsterError::internal_error(
            "storage migration multipart minimum part size exceeds i64 range",
        )
    })?;
    let configured_part_size = configured_part_size.max(min_part_size).clamp(
        MIGRATION_MULTIPART_MIN_PART_SIZE,
        MIGRATION_MULTIPART_PREFERRED_MAX_PART_SIZE,
    );
    let count_limited_part_size = if blob_size == 0 {
        configured_part_size
    } else {
        blob_size.checked_add(max_parts - 1).ok_or_else(|| {
            AsterError::internal_error("storage migration multipart size overflow")
        })? / max_parts
    };
    let part_size = configured_part_size.max(count_limited_part_size);
    let part_count = if blob_size == 0 {
        0
    } else {
        blob_size.checked_add(part_size - 1).ok_or_else(|| {
            AsterError::internal_error("storage migration multipart part count overflow")
        })? / part_size
    };
    let provider_max_part_size = capabilities
        .max_part_size
        .map(|size| {
            i64::try_from(size).map_err(|_| {
                AsterError::internal_error(
                    "storage migration provider max part size exceeds i64 range",
                )
            })
        })
        .transpose()?;
    let provider_size_ok = provider_max_part_size.is_none_or(|max| part_size <= max);
    let reader_size_ok = match capabilities.upload_mode {
        MultipartUploadMode::NativeStreaming => true,
        MultipartUploadMode::Buffered { max_size } => {
            i64::try_from(max_size).is_ok_and(|max| part_size <= max)
        }
    };
    Ok(MigrationMultipartPartPlan {
        part_size,
        part_count,
        provider_max_parts: max_parts,
        provider_max_part_size,
        upload_mode: capabilities.upload_mode,
        can_start: part_count <= max_parts && provider_size_ok && reader_size_ok,
    })
}

fn multipart_plan_for_dry_run(
    blob_size: i64,
    configured_part_size: i64,
    capabilities: MultipartStorageCapabilities,
) -> Result<Option<StoragePolicyMigrationMultipartPlan>> {
    if blob_size <= 0 {
        return Ok(None);
    }
    let plan = migration_multipart_part_plan(blob_size, configured_part_size, capabilities)?;
    let multipart_required = blob_size > plan.part_size;
    let reason = if plan.can_start || !multipart_required {
        None
    } else {
        Some(match plan.upload_mode {
            MultipartUploadMode::NativeStreaming => {
                "provider multipart limits cannot represent this blob size".to_string()
            }
            MultipartUploadMode::Buffered { .. } => {
                "provider multipart reader requires a part larger than its heap budget".to_string()
            }
        })
    };
    Ok(Some(StoragePolicyMigrationMultipartPlan {
        blob_size,
        part_size: plan.part_size,
        part_count: plan.part_count,
        provider_max_parts: plan.provider_max_parts,
        provider_max_part_size: plan.provider_max_part_size,
        heap_budget: MIGRATION_MULTIPART_HEAP_BUDGET,
        upload_mode: match plan.upload_mode {
            MultipartUploadMode::NativeStreaming => {
                StoragePolicyMigrationMultipartUploadMode::NativeStreaming
            }
            MultipartUploadMode::Buffered { .. } => {
                StoragePolicyMigrationMultipartUploadMode::Buffered
            }
        },
        can_start: plan.can_start || !multipart_required,
        reason,
    }))
}

pub(crate) async fn create_storage_policy_migration_task(
    state: &PrimaryAppState,
    input: CreateStoragePolicyMigrationInput,
) -> Result<TaskInfo> {
    let preflight = build_storage_policy_migration_preflight(state, input.clone()).await?;
    if !preflight.dry_run.can_start {
        if let Some(plan) = preflight.dry_run.multipart_plan.as_ref()
            && !plan.can_start
        {
            return Err(AsterError::validation_error(
                plan.reason
                    .as_deref()
                    .unwrap_or("target multipart capability cannot represent this migration"),
            ));
        }
        return Err(AsterError::validation_error(
            "target storage capacity is insufficient for this migration",
        ));
    }
    let source_policy = preflight.source_policy;
    let target_policy = preflight.target_policy;
    let plan_hash = migration_plan_hash(
        input.source_policy_id,
        input.target_policy_id,
        input.mode,
        preflight
            .source_recovery_probe
            .as_ref()
            .map(|probe| probe.plan_hash.as_str()),
        &source_policy,
        &target_policy,
    )?;
    let payload = StoragePolicyMigrationTaskPayload {
        source_policy_id: input.source_policy_id,
        target_policy_id: input.target_policy_id,
        mode: input.mode,
        plan_hash: plan_hash.clone(),
        source_policy_updated_at: source_policy.updated_at,
        target_policy_updated_at: target_policy.updated_at,
        source_recovery_probe: preflight.source_recovery_probe,
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
    validate_storage_policy_migration_input(&input)?;
    ensure_no_active_storage_policy_migration(state.writer_db(), &input).await?;
    let source_policy = policy_repo::find_by_id(state.writer_db(), input.source_policy_id).await?;
    let target_policy = policy_repo::find_by_id(state.writer_db(), input.target_policy_id).await?;
    let source_recovery_probe = match input.mode {
        StoragePolicyMigrationMode::Normal => None,
        StoragePolicyMigrationMode::RecoverAvailable => {
            let expected_hash = input.recovery_plan_hash.as_deref().ok_or_else(|| {
                AsterError::validation_error(
                    "recovery_plan_hash is required for recover_available migration",
                )
            })?;
            let probe =
                crate::services::storage_policy::recoverability::probe_policy_recoverability(
                    state,
                    input.source_policy_id,
                )
                .await?;
            if probe.plan_hash != expected_hash {
                return Err(AsterError::validation_error(
                    "storage recovery probe no longer matches current source policy",
                ));
            }
            if !probe.can_start_recovery {
                return Err(AsterError::validation_error(
                    "storage recovery probe did not find any recoverable source content",
                ));
            }
            Some(probe)
        }
    };
    let target_driver = state.driver_registry().get_driver(&target_policy)?;
    let target_supports_stream_upload = target_driver.extensions().stream_upload.is_some();
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
    let (target_capacity, _target_capacity_diagnostic) =
        crate::services::storage_policy::policy::capacity_info_or_status(
            target_driver.as_ref(),
            &target_policy.connector_id,
        )
        .await;
    let target_capacity_check =
        migration_capacity_check(&target_capacity, missing_summary.total_size);
    let multipart_plan = target_driver
        .extensions()
        .multipart
        .filter(|_| missing_summary.count > 0)
        .map(|multipart| {
            multipart_plan_for_dry_run(
                missing_summary.max_size,
                target_policy.chunk_size,
                multipart.capabilities(),
            )
        })
        .transpose()?
        .flatten();
    let multipart_can_start = multipart_plan.as_ref().is_none_or(|plan| plan.can_start);
    let mut warnings = match target_capacity_check {
        StoragePolicyMigrationCapacityCheck::Unsupported
        | StoragePolicyMigrationCapacityCheck::Unavailable => {
            vec![StoragePolicyMigrationDryRunWarning::TargetCapacityUnavailable]
        }
        StoragePolicyMigrationCapacityCheck::Sufficient
        | StoragePolicyMigrationCapacityCheck::Insufficient => Vec::new(),
    };
    if multipart_plan.as_ref().is_some_and(|plan| !plan.can_start) {
        warnings.push(StoragePolicyMigrationDryRunWarning::MultipartCapabilityUnavailable);
    }
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
            multipart_plan,
            source_recovery_probe: source_recovery_probe.clone(),
            can_start: can_start
                && multipart_can_start
                && source_recovery_probe
                    .as_ref()
                    .is_none_or(|probe| probe.can_start_recovery),
            warnings,
        },
        source_recovery_probe,
    })
}

fn migration_capacity_check(
    capacity: &aster_drive_storage::StorageCapacityInfo,
    required_bytes: i64,
) -> StoragePolicyMigrationCapacityCheck {
    match capacity.status {
        aster_drive_storage::StorageCapacityStatus::Supported => match capacity.available_bytes {
            Some(available) if available >= required_bytes => {
                StoragePolicyMigrationCapacityCheck::Sufficient
            }
            Some(_) => StoragePolicyMigrationCapacityCheck::Insufficient,
            None => StoragePolicyMigrationCapacityCheck::Unavailable,
        },
        aster_drive_storage::StorageCapacityStatus::Unsupported => {
            StoragePolicyMigrationCapacityCheck::Unsupported
        }
        aster_drive_storage::StorageCapacityStatus::Unavailable => {
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

/// Validates source and target policy identities before migration preflight.
fn validate_storage_policy_migration_input(
    input: &CreateStoragePolicyMigrationInput,
) -> Result<()> {
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
    Ok(())
}

async fn ensure_no_active_storage_policy_migration<C: sea_orm::ConnectionTrait>(
    db: &C,
    input: &CreateStoragePolicyMigrationInput,
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
    audit_ctx: &crate::services::ops::audit::AuditContext,
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
    let mut steps = parse_task_steps_json(task.steps_json.as_ref().map(|raw| raw.as_ref()))?;
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
    let target_driver = state.driver_registry().get_driver(&target_policy)?;
    if target_driver.extensions().stream_upload.is_none() {
        return Err(AsterError::storage_driver_error(
            "target storage policy does not support stream upload",
        ));
    }
    let source_has_stored_blobs = match payload.mode {
        StoragePolicyMigrationMode::Normal => true,
        StoragePolicyMigrationMode::RecoverAvailable => {
            file_repo::summarize_blobs_by_policy_and_backing(
                state.writer_db(),
                payload.source_policy_id,
                aster_drive_model::types::file_blob::FileBlobBacking::Stored,
            )
            .await?
            .count
                > 0
        }
    };
    let source_driver = source_has_stored_blobs
        .then(|| state.driver_registry().get_driver(&source_policy))
        .transpose()?;

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
        source_driver: source_driver.as_deref(),
        target_driver: target_driver.as_ref(),
    };

    if payload.mode == StoragePolicyMigrationMode::RecoverAvailable {
        storage_migration_checkpoint_repo::set_stage(
            state.writer_db(),
            task.id,
            CHECKPOINT_STAGE_MIGRATE_VIRTUAL_EMPTY,
            None,
        )
        .await?;
        migrate_recovery_virtual_empty(&migration_context, &mut checkpoint).await?;
    }

    if source_has_stored_blobs || payload.mode == StoragePolicyMigrationMode::Normal {
        loop {
            context.ensure_active()?;
            let blobs = match payload.mode {
                StoragePolicyMigrationMode::Normal => {
                    file_repo::find_blobs_by_policy_paginated(
                        state.writer_db(),
                        payload.source_policy_id,
                        checkpoint.last_processed_blob_id,
                        MIGRATION_BATCH_SIZE,
                    )
                    .await?
                }
                StoragePolicyMigrationMode::RecoverAvailable => {
                    file_repo::find_stored_blobs_by_policy_paginated(
                        state.writer_db(),
                        payload.source_policy_id,
                        checkpoint.last_processed_blob_id,
                        MIGRATION_BATCH_SIZE,
                    )
                    .await?
                }
            };
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
                let blob_id = blob.id;
                let outcome = match migrate_one_blob(&migration_context, blob).await {
                    Ok(outcome) => outcome,
                    Err(error)
                        if payload.mode == StoragePolicyMigrationMode::RecoverAvailable
                            && error.storage_error_kind()
                                == Some(aster_drive_storage::StorageErrorKind::NotFound) =>
                    {
                        advance_checkpoint(
                            state,
                            task.id,
                            blob_id,
                            BlobMigrationOutcome {
                                scanned: 1,
                                failed: 1,
                                ..Default::default()
                            },
                            Some(&error.to_string()),
                        )
                        .await?
                    }
                    Err(error) => return Err(error),
                };

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
                        checkpoint.migrated_blobs,
                        checkpoint.merged_blobs,
                        checkpoint.skipped_blobs
                    )),
                    &steps,
                )
                .await?;

                if outcome.failed > 0 && payload.mode == StoragePolicyMigrationMode::Normal {
                    break;
                }
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
    let remaining_blobs = i64::try_from(
        file_repo::count_blobs_by_policy(state.writer_db(), payload.source_policy_id).await?,
    )
    .map_err(|_| AsterError::internal_error("remaining storage migration blob count overflow"))?;
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
        remaining_blobs,
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

/// Migrates the source policy's shared virtual-empty blob before any stored-object read.
///
/// A policy can contain at most one virtual-empty row under the backing uniqueness
/// constraint. The stored-object cursor is deliberately preserved because this phase
/// may run again after a worker lease handoff. No source driver method is called.
async fn migrate_recovery_virtual_empty(
    migration: &BlobMigrationContext<'_>,
    checkpoint: &mut aster_drive_model::entities::storage_migration_checkpoint::Model,
) -> Result<()> {
    let Some(source_blob) = file_repo::find_virtual_empty_blob_by_policy(
        migration.state.writer_db(),
        migration.source_policy_id,
    )
    .await?
    else {
        return Ok(());
    };
    let cursor = checkpoint.last_processed_blob_id;
    let target_blob = file_repo::find_virtual_empty_blob_by_policy(
        migration.state.writer_db(),
        migration.target_policy_id,
    )
    .await?;

    let updated = transaction::with_transaction(migration.state.writer_db(), async |txn| {
        let outcome = if let Some(target_blob) = target_blob {
            let source_locked = file_repo::lock_blob_by_id(txn, source_blob.id).await?;
            if source_locked.policy_id != migration.source_policy_id {
                BlobMigrationOutcome {
                    scanned: 1,
                    skipped: 1,
                    ..Default::default()
                }
            } else {
                let target_locked = file_repo::lock_blob_by_id(txn, target_blob.id).await?;
                if !target_locked.is_virtual_empty() {
                    return Err(AsterError::internal_error(format!(
                        "recovery target blob #{} is not virtual-empty",
                        target_locked.id
                    )));
                }
                file_repo::replace_file_blob_refs(txn, source_locked.id, target_locked.id).await?;
                revision_repo::replace_blob_refs(txn, source_locked.id, target_locked.id).await?;
                file_repo::increment_blob_ref_count_by(
                    txn,
                    target_locked.id,
                    source_locked.ref_count,
                )
                .await?;
                file_repo::delete_blob_by_id(txn, source_locked.id).await?;
                BlobMigrationOutcome {
                    scanned: 1,
                    merged: 1,
                    ..Default::default()
                }
            }
        } else {
            let moved = file_repo::move_virtual_empty_blob_policy_if_current(
                txn,
                source_blob.id,
                migration.source_policy_id,
                migration.target_policy_id,
            )
            .await?;
            if moved {
                BlobMigrationOutcome {
                    scanned: 1,
                    migrated: 1,
                    ..Default::default()
                }
            } else {
                BlobMigrationOutcome {
                    scanned: 1,
                    skipped: 1,
                    ..Default::default()
                }
            }
        };
        storage_migration_checkpoint_repo::advance(
            txn,
            migration.task_id,
            CHECKPOINT_STAGE_MIGRATE_BLOBS,
            cursor,
            checkpoint_delta(outcome),
            None,
        )
        .await
    })
    .await?;
    *checkpoint = updated;
    Ok(())
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

    if latest.is_virtual_empty() {
        if let Some(target) =
            file_repo::find_virtual_empty_blob_by_policy(state.writer_db(), target_policy_id)
                .await?
        {
            return merge_blob_records(state, task_id, latest, target).await;
        }

        let move_result = transaction::with_transaction(state.writer_db(), async |txn| {
            let moved = file_repo::move_virtual_empty_blob_policy_if_current(
                txn,
                latest.id,
                source_policy_id,
                target_policy_id,
            )
            .await?;
            let outcome = if moved {
                BlobMigrationOutcome {
                    scanned: 1,
                    migrated: 1,
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
        .await;
        return match move_result {
            Ok(outcome) => Ok(outcome),
            Err(move_error) => {
                if let Some(target) = file_repo::find_virtual_empty_blob_by_policy(
                    state.writer_db(),
                    target_policy_id,
                )
                .await?
                {
                    merge_blob_records(state, task_id, latest, target).await
                } else {
                    Err(move_error)
                }
            }
        };
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
    let target_path = aster_forge_validation::filename::storage_path_from_blob_key(&target_hash)?;

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
        Ok::<_, AsterError>(outcome)
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
        revision_repo::replace_blob_refs(txn, old_locked.id, target_locked.id).await?;
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
    let source_path = blob.storage_path_for_connector().ok_or_else(|| {
        AsterError::validation_error("virtual-empty blobs must migrate as metadata-only records")
    })?;
    if let Some(multipart) = target_driver.extensions().multipart
        && should_use_multipart_migration(
            blob.size,
            target_multipart_part_size,
            multipart.capabilities(),
        )?
    {
        // Large single PUT streams are not safely retryable: the S3 SDK cannot
        // clone an in-flight reader after a timeout. Multipart migration keeps
        // each retryable unit bounded and aborts the upload before the blob row
        // is moved if any part fails.
        return copy_blob_multipart(migration, multipart, blob, target_path).await;
    }

    let source_driver = source_driver.ok_or_else(|| {
        AsterError::storage_driver_error(
            "source storage driver is unavailable for a stored blob migration",
        )
    })?;
    let source_stream = source_driver.get_stream(source_path).await?;
    context.ensure_active()?;
    let hashing_reader = HashingReader::new(source_stream, context.clone());
    let digest = hashing_reader.digest_handle();
    let stream_upload = target_driver.extensions().stream_upload.ok_or_else(|| {
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
        return Err(error.into());
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
    let source_path = blob.storage_path_for_connector().ok_or_else(|| {
        AsterError::validation_error("virtual-empty blobs must migrate as metadata-only records")
    })?;
    let source_driver = source_driver.ok_or_else(|| {
        AsterError::storage_driver_error(
            "source storage driver is unavailable for multipart blob migration",
        )
    })?;
    let part_plan = migration_multipart_part_plan(
        blob.size,
        target_multipart_part_size,
        multipart.capabilities(),
    )?;
    if !part_plan.can_start {
        return Err(AsterError::validation_error(
            "target multipart capability cannot represent this migration",
        ));
    }
    let part_size = part_plan.part_size;
    let upload_id = multipart.create_multipart_upload(target_path).await?;
    let mut completed_parts = Vec::new();
    let mut hasher = new_sha256();
    let mut remaining = blob.size;
    let mut offset = 0_i64;
    let mut part_number = 1_i32;
    let mut completed = false;

    let result = async {
        while remaining > 0 {
            context.ensure_active()?;
            let current_part_size = remaining.min(part_size);
            let etag = upload_multipart_part_with_retry(
                MultipartPartRetry {
                    multipart,
                    source_driver,
                    source_path,
                    target_path,
                    upload_id: &upload_id,
                    part_number,
                    offset,
                    part_size: current_part_size,
                    context,
                },
                &mut hasher,
            )
            .await?;
            completed_parts.push((part_number, etag));
            remaining -= current_part_size;
            offset = offset.checked_add(current_part_size).ok_or_else(|| {
                AsterError::internal_error("storage migration source range offset overflow")
            })?;
            part_number = part_number.checked_add(1).ok_or_else(|| {
                AsterError::internal_error("storage migration multipart part number overflow")
            })?;
        }

        ensure_source_range_finished(source_driver, source_path, offset).await?;
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

fn should_use_multipart_migration(
    blob_size: i64,
    configured_part_size: i64,
    capabilities: MultipartStorageCapabilities,
) -> Result<bool> {
    let plan = migration_multipart_part_plan(blob_size, configured_part_size, capabilities)?;
    Ok(blob_size > plan.part_size)
}

async fn ensure_source_range_finished(
    source_driver: &dyn StorageDriver,
    source_path: &str,
    offset: i64,
) -> Result<()> {
    let mut stream = source_driver
        .get_range(
            source_path,
            u64::try_from(offset).map_err(|_| {
                AsterError::internal_error("storage migration source range offset overflow")
            })?,
            None,
        )
        .await?;
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
    part: MultipartPartRetry<'_>,
    hasher: &mut sha2::Sha256,
) -> Result<String> {
    for attempt in 1..=MIGRATION_MULTIPART_PART_UPLOAD_MAX_ATTEMPTS {
        part.context.ensure_active()?;
        let source_reader = part
            .source_driver
            .get_range(
                part.source_path,
                u64::try_from(part.offset).map_err(|_| {
                    AsterError::internal_error("storage migration source range offset overflow")
                })?,
                Some(u64::try_from(part.part_size).map_err(|_| {
                    AsterError::internal_error("storage migration source range size overflow")
                })?),
            )
            .await?;
        let hashing_reader =
            HashingReader::with_digest(source_reader, part.context.clone(), hasher.clone());
        let digest = hashing_reader.digest_handle();
        match part
            .multipart
            .upload_multipart_part_reader(
                part.target_path,
                part.upload_id,
                part.part_number,
                Box::new(hashing_reader),
                part.part_size,
            )
            .await
        {
            Ok(etag) => {
                *hasher = digest.finish_hasher()?;
                return Ok(etag);
            }
            Err(error)
                if matches!(
                    error.kind(),
                    StorageErrorKind::Transient | StorageErrorKind::RateLimited
                ) && attempt < MIGRATION_MULTIPART_PART_UPLOAD_MAX_ATTEMPTS =>
            {
                tracing::warn!(
                    target_path = part.target_path,
                    part_number = part.part_number,
                    attempt,
                    error = %error,
                    "storage migration multipart part upload failed; retrying"
                );
                tokio::time::sleep(std::time::Duration::from_millis(
                    200_u64.saturating_mul(u64::try_from(attempt).unwrap_or(u64::MAX)),
                ))
                .await;
            }
            Err(error) => return Err(error.into()),
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
        if matches!(
            error.kind(),
            StorageErrorKind::Transient | StorageErrorKind::RateLimited
        ) && let Ok(metadata) = target_driver.metadata(target_path).await
            && u64_to_i64(metadata.size, "completed multipart object size")? == expected_size
        {
            context.ensure_active()?;
            return Ok(());
        }
        return Err(error.into());
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
        target_blob.storage_path_for_connector().ok_or_else(|| {
            AsterError::validation_error("virtual-empty blobs have no target object to verify")
        })?,
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
        payload.mode,
        payload
            .source_recovery_probe
            .as_ref()
            .map(|probe| probe.plan_hash.as_str()),
        source_policy,
        target_policy,
    )?;
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
    mode: StoragePolicyMigrationMode,
    source_recovery_plan_hash: Option<&str>,
    source_policy: &storage_policy::Model,
    target_policy: &storage_policy::Model,
) -> Result<String> {
    let plan = StorageMigrationPlanIdentity {
        source_policy_id,
        target_policy_id,
        mode,
        source_recovery_plan_hash,
        source: policy_identity(source_policy),
        target: policy_identity(target_policy),
    };
    let encoded = serde_json::to_vec(&plan).map_err(|error| {
        AsterError::internal_error(format!(
            "serialize storage migration plan identity: {error}"
        ))
    })?;
    Ok(sha256_hex(&encoded))
}

#[derive(Serialize)]
struct StorageMigrationPlanIdentity<'a> {
    source_policy_id: i64,
    target_policy_id: i64,
    mode: StoragePolicyMigrationMode,
    source_recovery_plan_hash: Option<&'a str>,
    source: StorageMigrationPolicyIdentity<'a>,
    target: StorageMigrationPolicyIdentity<'a>,
}

#[derive(Serialize)]
struct StorageMigrationPolicyIdentity<'a> {
    id: i64,
    connector_id: &'a str,
    storage_config: &'a str,
    chunk_size: i64,
    updated_at: chrono::DateTime<chrono::Utc>,
}

fn policy_identity(policy: &storage_policy::Model) -> StorageMigrationPolicyIdentity<'_> {
    StorageMigrationPolicyIdentity {
        id: policy.id,
        connector_id: &policy.connector_id,
        storage_config: policy.storage_config.as_ref(),
        chunk_size: policy.chunk_size,
        updated_at: policy.updated_at,
    }
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
        Self::with_digest(inner, context, new_sha256())
    }

    fn with_digest(
        inner: Box<dyn AsyncRead + Unpin + Send>,
        context: TaskExecutionContext,
        digest: sha2::Sha256,
    ) -> Self {
        Self {
            inner: Self::wrap_inner(inner),
            digest: HashDigestHandle(std::sync::Arc::new(std::sync::Mutex::new(Some(digest)))),
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
    fn finish_hasher(&self) -> Result<sha2::Sha256> {
        let mut guard = self
            .0
            .lock()
            .map_err(|_| AsterError::internal_error("hashing reader digest lock poisoned"))?;
        guard
            .take()
            .ok_or_else(|| AsterError::internal_error("hashing reader digest already finalized"))
    }

    fn finish_hex(&self) -> Result<String> {
        let hasher = self.finish_hasher()?;
        Ok(sha256_digest_to_hex(&sha2::Digest::finalize(hasher)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::task::is_task_worker_shutdown_requested;
    use aster_drive_storage::{StorageCapacityInfo, StorageCapacityStatus};
    use aster_forge_tasks::TaskLease;
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

    fn native_multipart_capabilities() -> MultipartStorageCapabilities {
        MultipartStorageCapabilities {
            min_part_size: 5 * 1024 * 1024,
            max_part_size: Some(5 * 1024 * 1024 * 1024),
            max_parts: 10_000,
            max_object_size: None,
            upload_mode: MultipartUploadMode::NativeStreaming,
        }
    }

    #[test]
    fn multipart_part_plan_keeps_heap_budget_separate_from_provider_part_size() {
        let plan = migration_multipart_part_plan(
            10_i64 * 1024 * 1024 * 1024 * 1024,
            64 * 1024 * 1024,
            native_multipart_capabilities(),
        )
        .expect("large blob plan should succeed");
        assert!(plan.part_size > MIGRATION_MULTIPART_HEAP_BUDGET);
        assert_eq!(plan.part_count, 10_000);
        assert!(plan.can_start);
    }

    #[test]
    fn multipart_part_plan_rejects_buffered_reader_above_heap_budget() {
        let mut capabilities = native_multipart_capabilities();
        capabilities.upload_mode = MultipartUploadMode::Buffered {
            max_size: MIGRATION_MULTIPART_HEAP_BUDGET as u64,
        };
        let plan = migration_multipart_part_plan(
            10_i64 * 1024 * 1024 * 1024 * 1024,
            64 * 1024 * 1024,
            capabilities,
        )
        .expect("buffered plan should be computed");
        assert!(!plan.can_start);
    }

    #[test]
    fn multipart_part_plan_handles_provider_part_limit_and_overflow() {
        let mut capabilities = native_multipart_capabilities();
        capabilities.max_parts = 50_000;
        let plan = migration_multipart_part_plan(1_i64 << 40, 5 * 1024 * 1024, capabilities)
            .expect("provider limit plan should succeed");
        assert_eq!(plan.part_count, 50_000);

        let error = migration_multipart_part_plan(i64::MAX, i64::MAX, capabilities)
            .expect_err("part size arithmetic should reject overflow");
        assert!(error.to_string().contains("overflow"));
    }

    #[tokio::test]
    async fn hashing_reader_stops_when_shutdown_is_requested() {
        let shutdown_token = CancellationToken::new();
        let context = TaskExecutionContext::new(
            TaskLease::new(42, 7),
            std::time::Duration::from_secs(60),
            shutdown_token.clone(),
        );
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
            .map_err(AsterError::from)
            .expect_err("cancelled context should remain visible as a task shutdown");
        assert!(is_task_worker_shutdown_requested(&shutdown_error));
    }
}

//! Admin-triggered file blob maintenance tasks.

use std::collections::{HashMap, HashSet};

use sea_orm::{EntityTrait, PaginatorTrait};

use crate::db::repository::{file_repo, version_repo};
use crate::db::transaction;
use crate::entities::{background_task, file_blob};
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::services::workspace_storage_service::WorkspaceStorageScope;
use crate::types::BackgroundTaskKind;

const BLOB_MAINTENANCE_BATCH_SIZE: u64 = 1_000;
const BLOB_MAINTENANCE_PROGRESS_INTERVAL: i64 = 1_000;

use super::steps::{
    TASK_STEP_CHECK_BLOBS, TASK_STEP_CLEANUP_OBJECTS, TASK_STEP_FINISH, TASK_STEP_RECONCILE_REFS,
    TASK_STEP_SCAN_BLOBS, TASK_STEP_WAITING, parse_task_steps_json, set_task_step_active,
    set_task_step_skipped, set_task_step_succeeded,
};
use super::types::{
    BlobMaintenanceAction, BlobMaintenanceTaskPayload, BlobMaintenanceTaskResult, TaskInfo,
    parse_task_payload, serialize_task_result,
};
use super::{
    TaskLeaseGuard, create_task_record, mark_task_progress, mark_task_succeeded, task_scope,
};

pub(crate) async fn create_blob_maintenance_task_for_admin(
    state: &PrimaryAppState,
    creator_user_id: i64,
    action: BlobMaintenanceAction,
    blob_ids: Option<Vec<i64>>,
) -> Result<TaskInfo> {
    let blob_ids = match blob_ids {
        Some(blob_ids) => {
            let blob_ids = normalize_blob_ids(blob_ids)?;
            ensure_blob_targets_exist(state, &blob_ids).await?;
            Some(blob_ids)
        }
        None => None,
    };
    let payload = BlobMaintenanceTaskPayload { action, blob_ids };
    let task = create_task_record(
        state,
        WorkspaceStorageScope::Personal {
            user_id: creator_user_id,
        },
        BackgroundTaskKind::BlobMaintenance,
        &blob_maintenance_display_name(action, payload.blob_ids.as_ref().map(Vec::len)),
        &payload,
    )
    .await?;
    super::get_task_in_scope(
        state,
        WorkspaceStorageScope::Personal {
            user_id: creator_user_id,
        },
        task.id,
    )
    .await
}

pub(super) async fn process_blob_maintenance_task(
    state: &PrimaryAppState,
    task: &background_task::Model,
    lease_guard: TaskLeaseGuard,
) -> Result<()> {
    let _scope = task_scope(task)?;
    let payload: BlobMaintenanceTaskPayload = parse_task_payload(task)?;
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
        TASK_STEP_SCAN_BLOBS,
        Some("Loading blob records"),
        None,
    )?;
    mark_task_progress(
        state,
        &lease_guard,
        0,
        0,
        Some("Loading blob records"),
        &steps,
    )
    .await?;

    let total = count_target_blobs(state, payload.blob_ids.as_deref()).await?;
    set_task_step_succeeded(
        &mut steps,
        TASK_STEP_SCAN_BLOBS,
        Some("Blob targets counted"),
        Some((total, total)),
    )?;

    let mut result = BlobMaintenanceTaskResult {
        action: payload.action,
        scanned_blobs: total,
        checked_objects: 0,
        missing_objects: 0,
        size_mismatches: 0,
        ref_counts_fixed: 0,
        orphan_blobs_deleted: 0,
        skipped_blobs: 0,
    };

    match payload.action {
        BlobMaintenanceAction::IntegrityCheck => {
            run_integrity_check(
                state,
                &lease_guard,
                &mut steps,
                payload.blob_ids.as_deref(),
                total,
                &mut result,
            )
            .await?;
            skip_reconcile_and_cleanup_steps(&mut steps)?;
        }
        BlobMaintenanceAction::RefCountReconcile => {
            set_task_step_skipped(
                &mut steps,
                TASK_STEP_CHECK_BLOBS,
                Some("Storage object check not requested"),
            )?;
            run_ref_count_reconcile(
                state,
                &lease_guard,
                &mut steps,
                payload.blob_ids.as_deref(),
                total,
                &mut result,
            )
            .await?;
            set_task_step_skipped(
                &mut steps,
                TASK_STEP_CLEANUP_OBJECTS,
                Some("Orphan cleanup not requested"),
            )?;
        }
        BlobMaintenanceAction::OrphanCleanup => {
            set_task_step_skipped(
                &mut steps,
                TASK_STEP_CHECK_BLOBS,
                Some("Storage object check not requested"),
            )?;
            run_ref_count_reconcile(
                state,
                &lease_guard,
                &mut steps,
                payload.blob_ids.as_deref(),
                total,
                &mut result,
            )
            .await?;
            run_orphan_cleanup(
                state,
                &lease_guard,
                &mut steps,
                payload.blob_ids.as_deref(),
                total,
                &mut result,
            )
            .await?;
        }
    }

    set_task_step_succeeded(
        &mut steps,
        TASK_STEP_FINISH,
        Some("Blob maintenance finished"),
        Some((total, total)),
    )?;
    let result_json = serialize_task_result(&result)?;
    mark_task_succeeded(
        state,
        &lease_guard,
        Some(&result_json),
        total,
        total,
        Some("Blob maintenance finished"),
        &steps,
    )
    .await
}

async fn run_integrity_check(
    state: &PrimaryAppState,
    lease_guard: &TaskLeaseGuard,
    steps: &mut [super::TaskStepInfo],
    blob_ids: Option<&[i64]>,
    total: i64,
    result: &mut BlobMaintenanceTaskResult,
) -> Result<()> {
    set_task_step_active(
        steps,
        TASK_STEP_CHECK_BLOBS,
        Some("Checking storage objects"),
        Some((0, total)),
    )?;
    mark_task_progress(state, lease_guard, 0, total, Some("Checking blobs"), steps).await?;

    let mut cursor = BlobTargetCursor::new(blob_ids);
    let mut progress = 0;

    loop {
        let blobs = load_target_blob_batch(state, blob_ids, &mut cursor).await?;
        if blobs.is_empty() {
            break;
        }

        for blob in blobs {
            lease_guard.ensure_active()?;
            progress += 1;
            match check_blob_object(state, &blob).await {
                Ok(BlobObjectCheck::Present) => {
                    result.checked_objects += 1;
                }
                Ok(BlobObjectCheck::Missing) => {
                    result.checked_objects += 1;
                    result.missing_objects += 1;
                }
                Ok(BlobObjectCheck::SizeMismatch) => {
                    result.checked_objects += 1;
                    result.size_mismatches += 1;
                }
                Err(error) => {
                    tracing::warn!(
                        blob_id = blob.id,
                        policy_id = blob.policy_id,
                        path = %blob.storage_path,
                        "blob integrity check failed: {error}"
                    );
                    result.skipped_blobs += 1;
                }
            }
            if should_mark_blob_maintenance_progress(progress, total) {
                mark_task_progress(
                    state,
                    lease_guard,
                    progress,
                    total,
                    Some("Checking blobs"),
                    steps,
                )
                .await?;
            }
        }
    }

    set_task_step_succeeded(
        steps,
        TASK_STEP_CHECK_BLOBS,
        Some("Storage object check finished"),
        Some((total, total)),
    )
}

async fn run_ref_count_reconcile(
    state: &PrimaryAppState,
    lease_guard: &TaskLeaseGuard,
    steps: &mut [super::TaskStepInfo],
    blob_ids: Option<&[i64]>,
    total: i64,
    result: &mut BlobMaintenanceTaskResult,
) -> Result<()> {
    set_task_step_active(
        steps,
        TASK_STEP_RECONCILE_REFS,
        Some("Reconciling reference counts"),
        Some((0, total)),
    )?;
    mark_task_progress(
        state,
        lease_guard,
        0,
        total,
        Some("Reconciling references"),
        steps,
    )
    .await?;

    let mut cursor = BlobTargetCursor::new(blob_ids);
    let mut progress = 0;

    loop {
        let blobs = load_target_blob_batch(state, blob_ids, &mut cursor).await?;
        if blobs.is_empty() {
            break;
        }
        let batch_blob_ids: Vec<i64> = blobs.iter().map(|blob| blob.id).collect();
        let actual_ref_counts = current_blob_ref_counts(state, &batch_blob_ids).await?;

        for blob in blobs {
            lease_guard.ensure_active()?;
            progress += 1;
            let actual_refs = actual_ref_counts.get(&blob.id).copied().unwrap_or(0);
            if blob.ref_count == actual_refs
                || blob.ref_count == file_repo::BLOB_CLEANUP_CLAIMED_REF_COUNT
            {
                if should_mark_blob_maintenance_progress(progress, total) {
                    mark_task_progress(
                        state,
                        lease_guard,
                        progress,
                        total,
                        Some("Reconciling references"),
                        steps,
                    )
                    .await?;
                }
                continue;
            }

            let Some(reconciled) = reconcile_single_blob_ref_count(state, blob.id).await? else {
                result.skipped_blobs += 1;
                if should_mark_blob_maintenance_progress(progress, total) {
                    mark_task_progress(
                        state,
                        lease_guard,
                        progress,
                        total,
                        Some("Reconciling references"),
                        steps,
                    )
                    .await?;
                }
                continue;
            };
            if reconciled.ref_count_fixed {
                result.ref_counts_fixed += 1;
            }
            if should_mark_blob_maintenance_progress(progress, total) {
                mark_task_progress(
                    state,
                    lease_guard,
                    progress,
                    total,
                    Some("Reconciling references"),
                    steps,
                )
                .await?;
            }
        }
    }

    set_task_step_succeeded(
        steps,
        TASK_STEP_RECONCILE_REFS,
        Some("Reference counts reconciled"),
        Some((total, total)),
    )
}

async fn run_orphan_cleanup(
    state: &PrimaryAppState,
    lease_guard: &TaskLeaseGuard,
    steps: &mut [super::TaskStepInfo],
    blob_ids: Option<&[i64]>,
    total: i64,
    result: &mut BlobMaintenanceTaskResult,
) -> Result<()> {
    set_task_step_active(
        steps,
        TASK_STEP_CLEANUP_OBJECTS,
        Some("Cleaning orphan blobs"),
        Some((0, total)),
    )?;
    mark_task_progress(
        state,
        lease_guard,
        0,
        total,
        Some("Cleaning orphans"),
        steps,
    )
    .await?;

    let mut cursor = BlobTargetCursor::new(blob_ids);
    let mut progress = 0;

    loop {
        let blobs = load_target_blob_batch(state, blob_ids, &mut cursor).await?;
        if blobs.is_empty() {
            break;
        }
        let batch_blob_ids: Vec<i64> = blobs.iter().map(|blob| blob.id).collect();
        let actual_ref_counts = current_blob_ref_counts(state, &batch_blob_ids).await?;

        for blob in blobs {
            lease_guard.ensure_active()?;
            progress += 1;
            let actual_refs = actual_ref_counts.get(&blob.id).copied().unwrap_or(0);
            if actual_refs != 0 {
                result.skipped_blobs += 1;
                if should_mark_blob_maintenance_progress(progress, total) {
                    mark_task_progress(
                        state,
                        lease_guard,
                        progress,
                        total,
                        Some("Cleaning orphans"),
                        steps,
                    )
                    .await?;
                }
                continue;
            }

            match reconcile_single_blob_ref_count(state, blob.id).await? {
                Some(reconciled)
                    if reconciled.actual_refs == 0
                        && reconciled.blob.ref_count == 0
                        && crate::services::file_service::cleanup_unreferenced_blob(
                            state,
                            &reconciled.blob,
                        )
                        .await =>
                {
                    result.orphan_blobs_deleted += 1;
                    if reconciled.ref_count_fixed {
                        result.ref_counts_fixed += 1;
                    }
                }
                Some(reconciled) => {
                    result.skipped_blobs += 1;
                    if reconciled.ref_count_fixed {
                        result.ref_counts_fixed += 1;
                    }
                }
                None => {
                    result.skipped_blobs += 1;
                }
            }

            if should_mark_blob_maintenance_progress(progress, total) {
                mark_task_progress(
                    state,
                    lease_guard,
                    progress,
                    total,
                    Some("Cleaning orphans"),
                    steps,
                )
                .await?;
            }
        }
    }

    set_task_step_succeeded(
        steps,
        TASK_STEP_CLEANUP_OBJECTS,
        Some("Orphan cleanup finished"),
        Some((total, total)),
    )
}

enum BlobObjectCheck {
    Present,
    Missing,
    SizeMismatch,
}

async fn check_blob_object(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
) -> Result<BlobObjectCheck> {
    let policy = state.policy_snapshot.get_policy_or_err(blob.policy_id)?;
    let driver = state.driver_registry.get_driver(&policy)?;
    let metadata = match driver.metadata(&blob.storage_path).await {
        Ok(metadata) => metadata,
        Err(error) => {
            return match driver.exists(&blob.storage_path).await {
                Ok(false) => Ok(BlobObjectCheck::Missing),
                Ok(true) => Err(error),
                Err(exists_error) => Err(AsterError::storage_driver_error(format!(
                    "metadata failed and existence probe failed for blob #{}: metadata_error={error}; exists_error={exists_error}",
                    blob.id
                ))),
            };
        }
    };
    let expected_size = crate::utils::numbers::i64_to_u64(blob.size, "blob size")?;
    if metadata.size == expected_size {
        Ok(BlobObjectCheck::Present)
    } else {
        Ok(BlobObjectCheck::SizeMismatch)
    }
}

struct ReconciledBlob {
    blob: file_blob::Model,
    actual_refs: i32,
    ref_count_fixed: bool,
}

async fn reconcile_single_blob_ref_count(
    state: &PrimaryAppState,
    blob_id: i64,
) -> Result<Option<ReconciledBlob>> {
    transaction::with_transaction(state.writer_db(), async |txn| {
        let mut blob = match file_repo::lock_blob_by_id(txn, blob_id).await {
            Ok(blob) => blob,
            Err(AsterError::RecordNotFound(_)) => return Ok(None),
            Err(error) => return Err(error),
        };
        if blob.ref_count == file_repo::BLOB_CLEANUP_CLAIMED_REF_COUNT {
            return Ok(None);
        }

        let actual_refs = current_blob_ref_count(txn, blob_id).await?;
        let ref_count_fixed = blob.ref_count != actual_refs;
        if ref_count_fixed {
            file_repo::set_blob_ref_count(txn, blob_id, actual_refs).await?;
            blob = file_repo::find_blob_by_id(txn, blob_id).await?;
        }

        Ok(Some(ReconciledBlob {
            blob,
            actual_refs,
            ref_count_fixed,
        }))
    })
    .await
}

async fn current_blob_ref_count<C: sea_orm::ConnectionTrait>(db: &C, blob_id: i64) -> Result<i32> {
    let file_refs = file_repo::count_blob_refs_from_files_for_blob(db, blob_id).await?;
    let version_refs = version_repo::count_blob_refs_from_versions_for_blob(db, blob_id).await?;
    let total_refs = file_refs
        .checked_add(version_refs)
        .ok_or_else(|| AsterError::internal_error("blob ref count overflow during reconcile"))?;
    crate::utils::numbers::i64_to_i32(total_refs, "blob actual reference count")
}

async fn current_blob_ref_counts(
    state: &PrimaryAppState,
    blob_ids: &[i64],
) -> Result<HashMap<i64, i32>> {
    let mut counts = HashMap::new();
    let file_refs =
        file_repo::count_blob_refs_from_files_for_blobs(state.writer_db(), blob_ids).await?;
    let version_refs =
        version_repo::count_blob_refs_from_versions_for_blobs(state.writer_db(), blob_ids).await?;

    for blob_id in blob_ids {
        let file_count = file_refs.get(blob_id).copied().unwrap_or(0);
        let version_count = version_refs.get(blob_id).copied().unwrap_or(0);
        let total_refs = file_count.checked_add(version_count).ok_or_else(|| {
            AsterError::internal_error("blob ref count overflow during batch reconcile")
        })?;
        counts.insert(
            *blob_id,
            crate::utils::numbers::i64_to_i32(total_refs, "blob actual reference count")?,
        );
    }

    Ok(counts)
}

enum BlobTargetCursor {
    All { last_blob_id: Option<i64> },
    Targeted { next_index: usize },
}

impl BlobTargetCursor {
    fn new(blob_ids: Option<&[i64]>) -> Self {
        if blob_ids.is_some() {
            Self::Targeted { next_index: 0 }
        } else {
            Self::All { last_blob_id: None }
        }
    }
}

async fn count_target_blobs(state: &PrimaryAppState, blob_ids: Option<&[i64]>) -> Result<i64> {
    let Some(blob_ids) = blob_ids else {
        let count = file_blob::Entity::find()
            .count(state.writer_db())
            .await
            .map_err(AsterError::from)?;
        return crate::utils::numbers::u64_to_i64(count, "blob maintenance target count");
    };
    crate::utils::numbers::usize_to_i64(blob_ids.len(), "blob maintenance target count")
}

async fn load_target_blob_batch(
    state: &PrimaryAppState,
    blob_ids: Option<&[i64]>,
    cursor: &mut BlobTargetCursor,
) -> Result<Vec<file_blob::Model>> {
    match cursor {
        BlobTargetCursor::All { last_blob_id } => {
            let blobs = file_repo::find_blobs_paginated(
                state.writer_db(),
                *last_blob_id,
                BLOB_MAINTENANCE_BATCH_SIZE,
            )
            .await?;
            *last_blob_id = blobs.last().map(|blob| blob.id);
            Ok(blobs)
        }
        BlobTargetCursor::Targeted { next_index } => {
            let blob_ids = blob_ids.ok_or_else(|| {
                AsterError::internal_error("targeted blob cursor without target blob ids")
            })?;
            if *next_index >= blob_ids.len() {
                return Ok(Vec::new());
            }
            let batch_size = crate::utils::numbers::u64_to_usize(
                BLOB_MAINTENANCE_BATCH_SIZE,
                "blob maintenance batch size",
            )?;
            let end = next_index.saturating_add(batch_size).min(blob_ids.len());
            let blobs = load_target_blobs(state, &blob_ids[*next_index..end]).await?;
            *next_index = end;
            Ok(blobs)
        }
    }
}

async fn load_target_blobs(
    state: &PrimaryAppState,
    blob_ids: &[i64],
) -> Result<Vec<file_blob::Model>> {
    let blob_map = file_repo::find_blobs_by_ids(state.writer_db(), blob_ids).await?;
    let mut blobs = Vec::with_capacity(blob_ids.len());
    for blob_id in blob_ids {
        let blob = blob_map
            .get(blob_id)
            .cloned()
            .ok_or_else(|| AsterError::record_not_found(format!("file_blob #{blob_id}")))?;
        blobs.push(blob);
    }
    Ok(blobs)
}

fn should_mark_blob_maintenance_progress(progress: i64, total: i64) -> bool {
    progress == total || progress % BLOB_MAINTENANCE_PROGRESS_INTERVAL == 0
}

async fn ensure_blob_targets_exist(state: &PrimaryAppState, blob_ids: &[i64]) -> Result<()> {
    let found = file_repo::find_blobs_by_ids(state.reader_db(), blob_ids).await?;
    for blob_id in blob_ids {
        if !found.contains_key(blob_id) {
            return Err(AsterError::record_not_found(format!(
                "file_blob #{blob_id}"
            )));
        }
    }
    Ok(())
}

fn normalize_blob_ids(blob_ids: Vec<i64>) -> Result<Vec<i64>> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for blob_id in blob_ids {
        if blob_id <= 0 {
            return Err(AsterError::validation_error(
                "blob_id must be greater than 0",
            ));
        }
        if seen.insert(blob_id) {
            normalized.push(blob_id);
        }
    }
    if normalized.is_empty() {
        return Err(AsterError::validation_error(
            "at least one blob_id is required",
        ));
    }
    Ok(normalized)
}

fn skip_reconcile_and_cleanup_steps(steps: &mut [super::TaskStepInfo]) -> Result<()> {
    set_task_step_skipped(
        steps,
        TASK_STEP_RECONCILE_REFS,
        Some("Reference reconcile not requested"),
    )?;
    set_task_step_skipped(
        steps,
        TASK_STEP_CLEANUP_OBJECTS,
        Some("Orphan cleanup not requested"),
    )
}

fn blob_maintenance_display_name(
    action: BlobMaintenanceAction,
    target_count: Option<usize>,
) -> String {
    let scope = target_count
        .map(|count| format!("{count} blob(s)"))
        .unwrap_or_else(|| "all blobs".to_string());
    match action {
        BlobMaintenanceAction::IntegrityCheck => {
            format!("Check integrity for {scope}")
        }
        BlobMaintenanceAction::RefCountReconcile => {
            format!("Reconcile references for {scope}")
        }
        BlobMaintenanceAction::OrphanCleanup => {
            format!("Clean orphan blobs for {scope}")
        }
    }
}

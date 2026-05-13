//! 存储策略删除后的临时对象兜底清理任务。

use chrono::{Duration, Utc};
use sea_orm::Set;

use crate::api::constants::HOUR_SECS;
use crate::db::repository::{background_task_repo, managed_follower_repo};
use crate::entities::{background_task, managed_follower, storage_policy};
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::storage::StorageErrorKind;
use crate::storage::driver::StorageDriver;
use crate::storage::drivers::{local::LocalDriver, remote::RemoteDriver, s3::S3Driver};
use crate::types::{
    BackgroundTaskKind, BackgroundTaskStatus, DriverType, StoredStoragePolicyAllowedTypes,
    StoredStoragePolicyOptions,
};
use crate::utils::numbers::u64_to_i64;

use super::steps::{
    TASK_STEP_CLEANUP_OBJECTS, TASK_STEP_PREPARE_SOURCES, parse_task_steps_json,
    set_task_step_active, set_task_step_succeeded,
};
use super::types::{
    StoragePolicyCleanupPolicySnapshot, StoragePolicyCleanupRemoteNodeSnapshot,
    StoragePolicyTempCleanupTarget, StoragePolicyTempCleanupTaskPayload,
    StoragePolicyTempCleanupTaskResult, parse_task_payload, serialize_task_payload,
    serialize_task_result,
};
use super::{
    TaskLeaseGuard, configured_task_max_attempts, initial_task_steps, mark_task_progress,
    mark_task_succeeded, serialize_task_steps, task_expiration_from,
};

const TEMP_CLEANUP_GRACE_SECS: u64 = HOUR_SECS + 60;

#[derive(Debug, Default)]
struct CleanupRunStats {
    deleted_objects: u64,
    missing_objects: u64,
    failed_objects: u64,
    errors: Vec<String>,
}

pub(crate) async fn create_storage_policy_temp_cleanup_task(
    state: &PrimaryAppState,
    policy: &storage_policy::Model,
    temp_keys: &[String],
    multipart_uploads: &[(String, String)],
) -> Result<Option<background_task::Model>> {
    if temp_keys.is_empty() && multipart_uploads.is_empty() {
        return Ok(None);
    }

    let payload = StoragePolicyTempCleanupTaskPayload {
        policy: policy_snapshot(policy),
        remote_node: remote_node_snapshot_for_policy(state, policy).await?,
        temp_keys: dedup_strings(temp_keys.iter().cloned()),
        multipart_uploads: dedup_multipart_targets(multipart_uploads.iter().cloned()),
    };

    let now = Utc::now();
    let cleanup_after = now
        + Duration::seconds(u64_to_i64(
            TEMP_CLEANUP_GRACE_SECS,
            "storage policy temp cleanup grace",
        )?);
    let payload_json = serialize_task_payload(&payload)?;
    let steps_json = serialize_task_steps(&initial_task_steps(
        BackgroundTaskKind::StoragePolicyTempCleanup,
    ))?;

    background_task_repo::create(
        &state.db,
        background_task::ActiveModel {
            kind: Set(BackgroundTaskKind::StoragePolicyTempCleanup),
            status: Set(BackgroundTaskStatus::Pending),
            creator_user_id: Set(None),
            team_id: Set(None),
            share_id: Set(None),
            display_name: Set(format!(
                "Clean deleted storage policy #{} temporary uploads",
                policy.id
            )),
            payload_json: Set(payload_json),
            result_json: Set(None),
            steps_json: Set(Some(steps_json)),
            progress_current: Set(0),
            progress_total: Set(0),
            status_text: Set(Some("Waiting for presigned URLs to expire".to_string())),
            attempt_count: Set(0),
            max_attempts: Set(configured_task_max_attempts(
                state,
                BackgroundTaskKind::StoragePolicyTempCleanup,
            )),
            next_run_at: Set(cleanup_after),
            processing_token: Set(0),
            processing_started_at: Set(None),
            last_heartbeat_at: Set(None),
            lease_expires_at: Set(None),
            started_at: Set(None),
            finished_at: Set(None),
            last_error: Set(None),
            failure_can_retry: Set(None),
            expires_at: Set(task_expiration_from(state, cleanup_after)),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .map(Some)
}

pub(super) async fn process_storage_policy_temp_cleanup_task(
    state: &PrimaryAppState,
    task: &background_task::Model,
    lease_guard: TaskLeaseGuard,
) -> Result<()> {
    let payload: StoragePolicyTempCleanupTaskPayload = parse_task_payload(task)?;
    let mut steps =
        parse_task_steps_json(task.steps_json.as_ref().map(|raw| raw.as_ref()), task.kind)?;
    let total_targets = cleanup_target_count(&payload)?;

    set_task_step_active(
        &mut steps,
        TASK_STEP_PREPARE_SOURCES,
        Some("Preparing deleted policy driver snapshot"),
        None,
    )?;
    mark_task_progress(
        state,
        &lease_guard,
        0,
        total_targets,
        Some("Preparing cleanup"),
        &steps,
    )
    .await?;

    let driver = driver_from_payload(&payload)?;
    set_task_step_succeeded(
        &mut steps,
        TASK_STEP_PREPARE_SOURCES,
        Some("Policy driver snapshot is ready"),
        None,
    )?;
    set_task_step_active(
        &mut steps,
        TASK_STEP_CLEANUP_OBJECTS,
        Some("Deleting temporary upload objects"),
        Some((0, total_targets)),
    )?;
    mark_task_progress(
        state,
        &lease_guard,
        0,
        total_targets,
        Some("Deleting temporary upload objects"),
        &steps,
    )
    .await?;

    let mut stats = CleanupRunStats::default();
    let mut current = 0_i64;

    for temp_key in &payload.temp_keys {
        delete_object_if_present(driver.as_ref(), temp_key, &mut stats).await;
        current += 1;
        mark_task_progress(
            state,
            &lease_guard,
            current,
            total_targets,
            Some("Deleting temporary upload objects"),
            &steps,
        )
        .await?;
    }

    if let Some(multipart) = driver.as_multipart() {
        for target in &payload.multipart_uploads {
            match multipart
                .abort_multipart_upload(&target.temp_key, &target.multipart_id)
                .await
            {
                Ok(()) => stats.deleted_objects += 1,
                Err(error) if error.storage_error_kind() == Some(StorageErrorKind::NotFound) => {
                    stats.missing_objects += 1;
                }
                Err(error) => {
                    stats.failed_objects += 1;
                    stats.errors.push(format!(
                        "abort multipart {} for {}: {error}",
                        target.multipart_id, target.temp_key
                    ));
                }
            }
            current += 1;
            mark_task_progress(
                state,
                &lease_guard,
                current,
                total_targets,
                Some("Deleting temporary upload objects"),
                &steps,
            )
            .await?;
        }
    } else {
        for target in &payload.multipart_uploads {
            stats.failed_objects += 1;
            stats.errors.push(format!(
                "driver does not support multipart cleanup for {} ({})",
                target.temp_key, target.multipart_id
            ));
            current += 1;
            mark_task_progress(
                state,
                &lease_guard,
                current,
                total_targets,
                Some("Deleting temporary upload objects"),
                &steps,
            )
            .await?;
        }
    }

    if !stats.errors.is_empty() {
        return Err(AsterError::storage_driver_error(format!(
            "storage policy temp cleanup failed for {} object(s): {}",
            stats.failed_objects,
            stats.errors.join("; ")
        )));
    }

    set_task_step_succeeded(
        &mut steps,
        TASK_STEP_CLEANUP_OBJECTS,
        Some("Temporary upload cleanup finished"),
        Some((total_targets, total_targets)),
    )?;
    let result = serialize_task_result(&StoragePolicyTempCleanupTaskResult {
        deleted_objects: stats.deleted_objects,
        missing_objects: stats.missing_objects,
        failed_objects: stats.failed_objects,
    })?;
    mark_task_succeeded(
        state,
        &lease_guard,
        Some(&result),
        total_targets,
        total_targets,
        Some("Temporary upload cleanup finished"),
        &steps,
    )
    .await
}

fn policy_snapshot(policy: &storage_policy::Model) -> StoragePolicyCleanupPolicySnapshot {
    StoragePolicyCleanupPolicySnapshot {
        id: policy.id,
        name: policy.name.clone(),
        driver_type: policy.driver_type,
        endpoint: policy.endpoint.clone(),
        bucket: policy.bucket.clone(),
        access_key: policy.access_key.clone(),
        secret_key: policy.secret_key.clone(),
        base_path: policy.base_path.clone(),
        remote_node_id: policy.remote_node_id,
        max_file_size: policy.max_file_size,
        allowed_types: policy.allowed_types.as_ref().to_string(),
        options: policy.options.as_ref().to_string(),
        is_default: policy.is_default,
        chunk_size: policy.chunk_size,
    }
}

async fn remote_node_snapshot_for_policy(
    state: &PrimaryAppState,
    policy: &storage_policy::Model,
) -> Result<Option<StoragePolicyCleanupRemoteNodeSnapshot>> {
    if policy.driver_type != DriverType::Remote {
        return Ok(None);
    }
    let remote_node_id = policy.remote_node_id.ok_or_else(|| {
        AsterError::validation_error("remote storage policy requires remote_node_id")
    })?;
    let remote = managed_follower_repo::find_by_id(&state.db, remote_node_id).await?;
    Ok(Some(StoragePolicyCleanupRemoteNodeSnapshot {
        id: remote.id,
        name: remote.name,
        base_url: remote.base_url,
        access_key: remote.access_key,
        secret_key: remote.secret_key,
    }))
}

fn driver_from_payload(
    payload: &StoragePolicyTempCleanupTaskPayload,
) -> Result<Box<dyn StorageDriver>> {
    let policy = storage_policy::Model {
        id: payload.policy.id,
        name: payload.policy.name.clone(),
        driver_type: payload.policy.driver_type,
        endpoint: payload.policy.endpoint.clone(),
        bucket: payload.policy.bucket.clone(),
        access_key: payload.policy.access_key.clone(),
        secret_key: payload.policy.secret_key.clone(),
        base_path: payload.policy.base_path.clone(),
        remote_node_id: payload.policy.remote_node_id,
        max_file_size: payload.policy.max_file_size,
        allowed_types: StoredStoragePolicyAllowedTypes(payload.policy.allowed_types.clone()),
        options: StoredStoragePolicyOptions(payload.policy.options.clone()),
        is_default: payload.policy.is_default,
        chunk_size: payload.policy.chunk_size,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    match policy.driver_type {
        DriverType::Local => Ok(Box::new(LocalDriver::new(&policy)?)),
        DriverType::S3 => Ok(Box::new(S3Driver::new(&policy)?)),
        DriverType::Remote => {
            let remote = payload.remote_node.as_ref().ok_or_else(|| {
                AsterError::validation_error(
                    "remote storage policy cleanup missing remote snapshot",
                )
            })?;
            let follower = managed_follower::Model {
                id: remote.id,
                name: remote.name.clone(),
                base_url: remote.base_url.clone(),
                access_key: remote.access_key.clone(),
                secret_key: remote.secret_key.clone(),
                is_enabled: true,
                last_capabilities: String::new(),
                last_error: String::new(),
                last_checked_at: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            Ok(Box::new(RemoteDriver::new(&policy, &follower)?))
        }
    }
}

async fn delete_object_if_present(
    driver: &dyn StorageDriver,
    path: &str,
    stats: &mut CleanupRunStats,
) {
    match driver.delete(path).await {
        Ok(()) => stats.deleted_objects += 1,
        Err(error) => match driver.exists(path).await {
            Ok(false) => stats.missing_objects += 1,
            Ok(true) => {
                stats.failed_objects += 1;
                stats.errors.push(format!("delete {path}: {error}"));
            }
            Err(exists_error) => {
                stats.failed_objects += 1;
                stats.errors.push(format!(
                    "delete {path}: {error}; existence check failed: {exists_error}"
                ));
            }
        },
    }
}

fn cleanup_target_count(payload: &StoragePolicyTempCleanupTaskPayload) -> Result<i64> {
    let total = payload
        .temp_keys
        .len()
        .checked_add(payload.multipart_uploads.len())
        .ok_or_else(|| {
            AsterError::internal_error("storage policy cleanup target count overflow")
        })?;
    crate::utils::numbers::usize_to_i64(total, "storage policy cleanup target count")
}

fn dedup_strings(values: impl Iterator<Item = String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for value in values {
        if seen.insert(value.clone()) {
            out.push(value);
        }
    }
    out
}

fn dedup_multipart_targets(
    values: impl Iterator<Item = (String, String)>,
) -> Vec<StoragePolicyTempCleanupTarget> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (temp_key, multipart_id) in values {
        if seen.insert((temp_key.clone(), multipart_id.clone())) {
            out.push(StoragePolicyTempCleanupTarget {
                temp_key,
                multipart_id,
            });
        }
    }
    out
}

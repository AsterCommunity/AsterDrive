//! 存储策略删除后的临时对象兜底清理任务。

use chrono::{Duration, Utc};

use crate::api::constants::HOUR_SECS;
use crate::entities::{background_task, storage_policy};
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, TaskRuntimeState};
use crate::storage::StorageDriver;
use crate::storage::StorageErrorKind;
use crate::storage::connectors::{
    StoragePolicyCleanupSnapshots, build_cleanup_driver, can_create_cleanup_task_with_snapshot,
    cleanup_snapshot_for_policy,
};
use crate::types::{StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions};
use crate::utils::numbers::u64_to_i64;

use super::spec::{self, StoragePolicyTempCleanupTask, decode_payload_as};
use super::steps::{
    TASK_STEP_CLEANUP_OBJECTS, TASK_STEP_PREPARE_SOURCES, parse_task_steps_json,
    set_task_step_active, set_task_step_succeeded,
};
use super::types::{
    StoragePolicyCleanupPolicySnapshot, StoragePolicyTempCleanupTarget,
    StoragePolicyTempCleanupTaskPayload, StoragePolicyTempCleanupTaskResult,
};
use super::{
    TaskExecutionContext, TypedTaskCreate, insert_typed_task_record, mark_task_progress,
    mark_task_succeeded,
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
    state: &(impl TaskRuntimeState + Sync),
    policy: &storage_policy::Model,
    temp_keys: &[String],
    multipart_uploads: &[(String, String)],
) -> Result<Option<background_task::Model>> {
    if temp_keys.is_empty() && multipart_uploads.is_empty() {
        return Ok(None);
    }

    let driver_snapshot = cleanup_snapshot_for_policy(state, policy).await?;
    if !can_create_cleanup_task_with_snapshot(policy.driver_type, &driver_snapshot) {
        return Ok(None);
    }

    let payload = StoragePolicyTempCleanupTaskPayload {
        policy: policy_snapshot(policy),
        driver_snapshot,
        onedrive_credential: None,
        remote_node: None,
        temp_keys: dedup_strings(temp_keys.iter().cloned()),
        multipart_uploads: dedup_multipart_targets(multipart_uploads.iter().cloned()),
    };

    let cleanup_after = chrono::Utc::now()
        + Duration::seconds(u64_to_i64(
            TEMP_CLEANUP_GRACE_SECS,
            "storage policy temp cleanup grace",
        )?);
    let task = insert_typed_task_record(
        state,
        state.writer_db(),
        TypedTaskCreate::<StoragePolicyTempCleanupTask>::new(
            format!(
                "Clean deleted storage policy #{} temporary uploads",
                policy.id
            ),
            payload,
        )
        .next_run_at(cleanup_after)
        .status_text("Waiting for presigned URLs to expire".to_string()),
    )
    .await?;

    state.wake_background_task_dispatcher();
    Ok(Some(task))
}

pub(super) async fn process_storage_policy_temp_cleanup_task(
    state: &PrimaryAppState,
    task: &background_task::Model,
    context: TaskExecutionContext,
) -> Result<()> {
    let lease_guard = context.lease_guard().clone();
    let payload = decode_payload_as::<StoragePolicyTempCleanupTask>(task)?;
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

    let policy = policy_model_from_snapshot(&payload.policy);
    let driver = build_cleanup_driver(
        state,
        &policy,
        StoragePolicyCleanupSnapshots {
            driver_snapshot: payload.driver_snapshot.as_ref(),
            legacy_onedrive_credential: payload.onedrive_credential.as_ref(),
            legacy_remote_node: payload.remote_node.as_ref(),
        },
    )
    .await?;
    set_task_step_succeeded(
        &mut steps,
        TASK_STEP_PREPARE_SOURCES,
        Some("Policy driver snapshot is ready"),
        None,
    )?;
    context.ensure_active()?;
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
        context.ensure_active()?;
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
            context.ensure_active()?;
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
            context.ensure_active()?;
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

    context.ensure_active()?;
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
    let result = spec::serialize_result::<StoragePolicyTempCleanupTask>(
        &StoragePolicyTempCleanupTaskResult {
            deleted_objects: stats.deleted_objects,
            missing_objects: stats.missing_objects,
            failed_objects: stats.failed_objects,
        },
    )?;
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

fn policy_model_from_snapshot(
    policy: &StoragePolicyCleanupPolicySnapshot,
) -> storage_policy::Model {
    storage_policy::Model {
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
        allowed_types: StoredStoragePolicyAllowedTypes(policy.allowed_types.clone()),
        options: StoredStoragePolicyOptions(policy.options.clone()),
        is_default: policy.is_default,
        chunk_size: policy.chunk_size,
        created_at: Utc::now(),
        updated_at: Utc::now(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::connectors::{
        StoragePolicyCleanupDriverSnapshot, StoragePolicyCleanupOneDriveCredentialSnapshot,
    };

    #[test]
    fn onedrive_cleanup_task_requires_driver_snapshot() {
        assert!(!can_create_cleanup_task_with_snapshot(
            crate::types::DriverType::OneDrive,
            &None
        ));
        assert!(can_create_cleanup_task_with_snapshot(
            crate::types::DriverType::Local,
            &None
        ));

        let snapshot = StoragePolicyCleanupOneDriveCredentialSnapshot {
            cloud: crate::types::MicrosoftGraphCloud::Global,
            tenant_id: None,
            client_id: None,
            client_secret_ciphertext: None,
            drive_id: "drive".to_string(),
            root_item_id: "root".to_string(),
            access_token_ciphertext: "access".to_string(),
            refresh_token_ciphertext: Some("refresh".to_string()),
            expires_at: None,
        };
        assert!(can_create_cleanup_task_with_snapshot(
            crate::types::DriverType::OneDrive,
            &Some(StoragePolicyCleanupDriverSnapshot::MicrosoftGraph(snapshot))
        ));
    }
}

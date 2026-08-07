//! 存储策略删除后的临时对象兜底清理任务。

use aster_forge_tasks::TaskExecutionContext;
use chrono::{Duration, Utc};

use crate::api::constants::HOUR_SECS;
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, TaskRuntimeState};
use crate::storage::connectors::{
    StoragePolicyCleanupSnapshots, build_cleanup_driver, can_create_cleanup_task_with_snapshot,
    cleanup_snapshot_for_policy,
};
use aster_drive_model::entities::{background_task, storage_policy};
use aster_drive_model::types::{StoredStoragePolicyAllowedTypes, StoredStoragePolicyConfig};
use aster_drive_storage::StorageDriver;
use aster_drive_storage::StorageErrorKind;
use aster_forge_tasks::{set_task_step_active, set_task_step_succeeded};
use aster_forge_utils::numbers::u64_to_i64;

use super::spec::{self, StoragePolicyTempCleanupTask, decode_payload_as};
use super::steps::{TASK_STEP_CLEANUP_OBJECTS, TASK_STEP_PREPARE_SOURCES, parse_task_steps_json};
use super::types::{
    StoragePolicyCleanupPolicySnapshot, StoragePolicyTempCleanupTarget,
    StoragePolicyTempCleanupTaskPayload, StoragePolicyTempCleanupTaskResult,
};
use super::{TypedTaskCreate, insert_typed_task_record, mark_task_progress, mark_task_succeeded};

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

    let connectors = state.driver_registry().connectors();
    let driver_snapshot = cleanup_snapshot_for_policy(connectors, state, policy).await?;
    if !can_create_cleanup_task_with_snapshot(connectors, policy, &driver_snapshot)? {
        return Err(AsterError::validation_error(format!(
            "storage policy #{} requires a cleanup driver snapshot, but none was available",
            policy.id
        )));
    }

    let payload = StoragePolicyTempCleanupTaskPayload {
        policy: policy_snapshot(policy),
        driver_snapshot,
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
    let mut steps = parse_task_steps_json(task.steps_json.as_ref().map(|raw| raw.as_ref()))?;
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
        state.driver_registry().connectors(),
        state,
        &policy,
        StoragePolicyCleanupSnapshots {
            driver_snapshot: payload.driver_snapshot.as_ref(),
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

    if let Some(multipart) = driver.extensions().multipart {
        for target in &payload.multipart_uploads {
            context.ensure_active()?;
            match multipart
                .abort_multipart_upload(&target.temp_key, &target.multipart_id)
                .await
            {
                Ok(()) => stats.deleted_objects += 1,
                Err(error) if error.kind() == StorageErrorKind::NotFound => {
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
        connector_id: policy.connector_id.clone(),
        storage_config: policy.storage_config.as_ref().to_string(),
        max_file_size: policy.max_file_size,
        allowed_types: policy.allowed_types.as_ref().to_string(),
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
        connector_id: policy.connector_id.clone(),
        storage_config: StoredStoragePolicyConfig(policy.storage_config.clone()),
        max_file_size: policy.max_file_size,
        allowed_types: StoredStoragePolicyAllowedTypes(policy.allowed_types.clone()),
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
    Ok(aster_forge_utils::numbers::usize_to_i64(
        total,
        "storage policy cleanup target count",
    )?)
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
    use crate::storage::connectors::test_support::{local_policy, onedrive_policy, remote_policy};
    use crate::storage::connectors::{
        OneDriveAccountMode, StoragePolicyCleanupDriverSnapshot, builtin_storage_connector_registry,
    };
    use aster_drive_model::types::{RemoteDownloadStrategy, RemoteUploadStrategy};
    use aster_drive_storage::{ConnectorId, StoragePolicyBehaviorConfig};

    fn registry() -> crate::storage::connectors::StorageConnectorRegistry {
        builtin_storage_connector_registry().expect("built-in connector registry")
    }

    #[test]
    fn credential_backed_cleanup_tasks_require_driver_snapshots() {
        let registry = registry();
        let onedrive = onedrive_policy(
            OneDriveAccountMode::Personal,
            Some("drive".to_string()),
            None,
            None,
            StoragePolicyBehaviorConfig::default(),
        );
        let local = local_policy("");
        assert!(!can_create_cleanup_task_with_snapshot(&registry, &onedrive, &None).unwrap());
        assert!(can_create_cleanup_task_with_snapshot(&registry, &local, &None).unwrap());

        for connector_id in [
            "asterdrive.storage.s3",
            "asterdrive.storage.alibaba_oss",
            "asterdrive.storage.sftp",
            "asterdrive.storage.azure_blob",
            "asterdrive.storage.tencent_cos",
        ] {
            let mut policy = local.clone();
            policy.connector_id = connector_id.to_string();
            assert!(
                !can_create_cleanup_task_with_snapshot(&registry, &policy, &None).unwrap(),
                "{connector_id} cleanup must not depend on process-local credentials"
            );
        }

        let snapshot = StoragePolicyCleanupDriverSnapshot::encode(
            ConnectorId::declared("asterdrive.storage.onedrive"),
            1,
            &serde_json::json!({ "credential": "snapshot" }),
        )
        .unwrap();
        assert!(
            can_create_cleanup_task_with_snapshot(&registry, &onedrive, &Some(snapshot)).unwrap()
        );
    }

    #[test]
    fn remote_cleanup_task_requires_driver_snapshot() {
        let registry = registry();
        let remote = remote_policy(
            "",
            Some(7),
            RemoteDownloadStrategy::RelayStream,
            RemoteUploadStrategy::RelayStream,
        );
        assert!(!can_create_cleanup_task_with_snapshot(&registry, &remote, &None).unwrap());

        let snapshot = StoragePolicyCleanupDriverSnapshot::encode(
            ConnectorId::declared("asterdrive.storage.remote"),
            1,
            &serde_json::json!({ "remote_node": 7 }),
        )
        .unwrap();
        assert!(
            can_create_cleanup_task_with_snapshot(&registry, &remote, &Some(snapshot)).unwrap()
        );
    }
}

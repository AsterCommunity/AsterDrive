//! Confirmed destruction of file histories that still reference a lost storage policy.

use aster_drive_model::entities::{background_task, file, storage_policy};
use aster_forge_crypto::sha256_hex;
use aster_forge_db::transaction;
use aster_forge_tasks::{
    TaskDedupeKey, TaskExecutionContext, set_task_step_active, set_task_step_succeeded,
};
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::db::repository::{
    file_repo, policy_placement_repo, policy_repo, revision_repo, upload_session_repo,
};
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState, TaskRuntimeState};
use crate::services::workspace::storage::WorkspaceResourceScope;

use super::spec::{StoragePolicyForcedPurgeTask, decode_payload_as};
use super::steps::{
    TASK_STEP_CLEANUP_OBJECTS, TASK_STEP_FINISH, TASK_STEP_PREPARE_SOURCES, TASK_STEP_PURGE_FILES,
    TASK_STEP_WAITING, parse_task_steps_json,
};
use super::types::{
    StoragePolicyForcedPurgeTaskPayload, StoragePolicyForcedPurgeTaskResult, TaskInfo,
};
use super::{
    TypedTaskCreate, insert_typed_task_record, mark_task_progress, mark_task_succeeded,
    set_task_runtime_json, task_scope,
};

const FORCED_PURGE_FILE_BATCH_SIZE: u64 = 100;

/// Complete impact preview that must be confirmed before destructive task creation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyForcedPurgePreview {
    /// Policy selected for permanent removal.
    pub policy_id: i64,
    /// Current policy name used by the confirmation phrase.
    pub policy_name: String,
    /// Current policy revision bound into the impact digest.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub policy_updated_at: chrono::DateTime<chrono::Utc>,
    /// Blob rows whose metadata will be removed after all references are purged.
    pub blob_count: i64,
    /// Bytes represented by source-policy blob rows.
    pub blob_total_bytes: i64,
    /// Distinct file histories permanently removed.
    pub file_count: i64,
    /// Affected files that are currently in trash.
    pub trash_file_count: i64,
    /// Canonical revisions backed by source-policy blobs.
    pub affected_revision_count: i64,
    /// Logical quota bytes represented by affected revisions.
    pub affected_logical_bytes: i64,
    /// Direct shares removed with the affected files.
    pub direct_share_count: i64,
    /// Placement targets that must be detached before purge can start.
    pub placement_target_count: u64,
    /// Active upload sessions that the confirmed task will abandon locally.
    pub upload_session_count: u64,
    /// Whether placement topology blockers have been cleared.
    pub can_start: bool,
    /// Exact phrase required by the destructive confirmation endpoint.
    pub confirmation_phrase: String,
    /// Digest binding task creation to this exact policy and impact snapshot.
    pub impact_digest: String,
}

/// Administrator input required to create one forced-purge task.
#[derive(Debug, Clone)]
pub struct CreateStoragePolicyForcedPurgeInput {
    /// Source policy selected in the impact preview.
    pub policy_id: i64,
    /// Exact digest returned by the latest preview.
    pub impact_digest: String,
    /// Exact high-risk confirmation phrase returned by the latest preview.
    pub confirmation: String,
    /// Human explanation retained with the destructive task.
    pub reason: String,
    /// Administrator creating the task.
    pub creator_user_id: i64,
    /// Request metadata used to write the audit row in the same transaction as task creation.
    pub audit_context: crate::services::ops::audit::AuditContext,
}

/// Durable cursor and counters stored in `background_tasks.runtime_json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct StoragePolicyForcedPurgeRuntime {
    /// Runtime schema version for future compatible decoding.
    schema_version: u32,
    /// Last fully purged file ID; processing resumes strictly after this value.
    last_processed_file_id: i64,
    /// File histories permanently removed so far.
    purged_files: i64,
    /// Logical quota bytes released so far.
    freed_logical_bytes: i64,
    /// Source blob metadata rows removed after all file references were gone.
    deleted_blob_records: i64,
}

/// Builds an authoritative writer-backed purge preview without changing any data.
pub async fn preview_storage_policy_forced_purge(
    state: &(impl SharedRuntimeState + Sync),
    policy_id: i64,
) -> Result<StoragePolicyForcedPurgePreview> {
    let policy = policy_repo::find_by_id(state.writer_db(), policy_id).await?;
    let blobs = file_repo::summarize_blobs_by_policy(state.writer_db(), policy_id).await?;
    let impact =
        file_repo::summarize_storage_policy_purge_impact(state.writer_db(), policy_id).await?;
    let placement_target_count =
        policy_placement_repo::count_targets_by_policy(state.writer_db(), policy_id).await?;
    let upload_session_count =
        upload_session_repo::count_active_by_policy(state.writer_db(), policy_id).await?;
    build_forced_purge_preview(
        &policy,
        blobs,
        impact,
        placement_target_count,
        upload_session_count,
    )
}

/// Creates a typed forced-purge task after exact digest and confirmation validation.
pub async fn create_storage_policy_forced_purge_task(
    state: &(impl TaskRuntimeState + Sync),
    input: CreateStoragePolicyForcedPurgeInput,
) -> Result<TaskInfo> {
    let reason = input.reason.trim();
    let preview = preview_storage_policy_forced_purge(state, input.policy_id).await?;
    if !preview.can_start {
        return Err(AsterError::validation_error(
            "storage policy still has placement targets",
        ));
    }
    if input.impact_digest != preview.impact_digest {
        return Err(AsterError::validation_error(
            "forced purge impact changed; request a new preview",
        ));
    }
    if input.confirmation != preview.confirmation_phrase {
        return Err(AsterError::validation_error(
            "forced purge confirmation phrase does not match",
        ));
    }
    let payload = StoragePolicyForcedPurgeTaskPayload {
        policy_id: preview.policy_id,
        policy_name: preview.policy_name.clone(),
        policy_updated_at: preview.policy_updated_at,
        impact_digest: preview.impact_digest.clone(),
        reason: reason.chars().take(1000).collect(),
    };
    let audit_context = input.audit_context.clone();
    let impact_digest = preview.impact_digest.clone();
    let task = transaction::with_transaction(state.writer_db(), async |txn| {
        let locked = policy_repo::lock_by_id(txn, input.policy_id).await?;
        if locked.updated_at != preview.policy_updated_at {
            return Err(AsterError::validation_error(
                "storage policy changed after forced purge preview",
            ));
        }
        let task = insert_typed_task_record(
            state,
            txn,
            TypedTaskCreate::<StoragePolicyForcedPurgeTask>::new(
                format!("Permanently purge storage policy #{}", input.policy_id),
                payload,
            )
            .dedupe_key(TaskDedupeKey::new(format!(
                "storage-policy-forced-purge:{}:{}",
                input.policy_id, preview.impact_digest
            ))?)
            .creator_user_id(Some(input.creator_user_id)),
        )
        .await?;
        crate::services::ops::audit::log_with_transaction(
            txn,
            state.runtime_config(),
            crate::services::ops::audit::AuditLogInput {
                ctx: &audit_context,
                action: crate::services::ops::audit::AuditAction::AdminCreateStoragePolicyForcedPurgeTask,
                entity_type: crate::services::ops::audit::AuditEntityType::StoragePolicy,
                entity_id: Some(input.policy_id),
                entity_name: Some(&locked.name),
            },
            || {
                crate::services::ops::audit::details(serde_json::json!({
                    "task_id": task.id,
                    "impact_digest": impact_digest,
                }))
            },
        )
        .await
        .map_err(|error| AsterError::database_operation(format!("write forced purge audit: {error}")))?;
        Ok(task)
    })
    .await?;
    state.wake_background_task_dispatcher();
    super::get_task_in_scope(state, task_scope(&task)?, task.id).await
}

/// Executes a confirmed forced purge with a durable per-file cursor and lease fencing.
pub(super) async fn process_storage_policy_forced_purge_task(
    state: &PrimaryAppState,
    task: &background_task::Model,
    context: TaskExecutionContext,
) -> Result<()> {
    let lease_guard = context.lease_guard().clone();
    let payload = decode_payload_as::<StoragePolicyForcedPurgeTask>(task)?;
    let mut runtime = decode_runtime(task.runtime_json.as_ref().map(AsRef::as_ref))?;
    let mut steps = parse_task_steps_json(task.steps_json.as_ref().map(AsRef::as_ref))?;
    set_task_step_succeeded(
        &mut steps,
        TASK_STEP_WAITING,
        Some("Worker claimed task"),
        None,
    )?;
    set_task_step_active(
        &mut steps,
        TASK_STEP_PREPARE_SOURCES,
        Some("Validating forced purge plan"),
        None,
    )?;
    mark_task_progress(
        state,
        &lease_guard,
        runtime.purged_files,
        runtime.purged_files,
        Some("Validating forced purge plan"),
        &steps,
    )
    .await?;

    let policy = policy_repo::find_by_id(state.writer_db(), payload.policy_id).await?;
    if policy.updated_at != payload.policy_updated_at {
        return Err(AsterError::validation_error(
            "storage policy changed after forced purge task was created",
        ));
    }
    if runtime.last_processed_file_id == 0 {
        let preview = preview_storage_policy_forced_purge(state, payload.policy_id).await?;
        if preview.impact_digest != payload.impact_digest {
            return Err(AsterError::validation_error(
                "forced purge impact changed before execution; create a new task",
            ));
        }
        if !preview.can_start {
            return Err(AsterError::validation_error(
                "storage policy gained placement targets before purge",
            ));
        }
        crate::services::files::upload::abandon_for_disaster_policy_purge(state, payload.policy_id)
            .await?;
    }
    set_task_step_succeeded(
        &mut steps,
        TASK_STEP_PREPARE_SOURCES,
        Some("Forced purge plan validated"),
        None,
    )?;
    set_task_step_active(
        &mut steps,
        TASK_STEP_PURGE_FILES,
        Some("Purging affected file histories"),
        None,
    )?;

    loop {
        context.ensure_active()?;
        let files = file_repo::find_files_referencing_policy_blobs_paginated(
            state.writer_db(),
            payload.policy_id,
            runtime.last_processed_file_id,
            FORCED_PURGE_FILE_BATCH_SIZE,
        )
        .await?;
        if files.is_empty() {
            break;
        }
        for batch in contiguous_scope_batches(files)? {
            context.ensure_active()?;
            let last_file_id = batch.files.last().map(|file| file.id).unwrap_or_default();
            let summary = crate::services::files::file::batch_purge_in_resource_scope_silent(
                state,
                batch.scope,
                batch.files,
            )
            .await?;
            runtime.last_processed_file_id = last_file_id;
            runtime.purged_files = runtime
                .purged_files
                .saturating_add(i64::from(summary.purged));
            runtime.freed_logical_bytes = runtime
                .freed_logical_bytes
                .saturating_add(summary.freed_bytes);
            persist_runtime(state, &lease_guard, &runtime).await?;
            mark_task_progress(
                state,
                &lease_guard,
                runtime.purged_files,
                runtime.purged_files,
                Some("Purging affected file histories"),
                &steps,
            )
            .await?;
        }
    }

    set_task_step_succeeded(
        &mut steps,
        TASK_STEP_PURGE_FILES,
        Some("Affected file histories purged"),
        None,
    )?;
    set_task_step_active(
        &mut steps,
        TASK_STEP_CLEANUP_OBJECTS,
        Some("Removing unreferenced blob records"),
        None,
    )?;
    loop {
        let blobs = file_repo::find_blobs_by_policy_paginated(
            state.writer_db(),
            payload.policy_id,
            0,
            FORCED_PURGE_FILE_BATCH_SIZE,
        )
        .await?;
        if blobs.is_empty() {
            break;
        }
        for blob in blobs {
            context.ensure_active()?;
            let current_refs =
                file_repo::count_blob_refs_from_files_for_blob(state.writer_db(), blob.id).await?;
            let historical_refs =
                revision_repo::count_non_current_blob_refs_for_blob(state.writer_db(), blob.id)
                    .await?;
            if current_refs > 0 || historical_refs > 0 {
                return Err(AsterError::internal_error(format!(
                    "forced purge blob #{} still has {current_refs} current and {historical_refs} historical reference(s)",
                    blob.id
                )));
            }
            if file_repo::delete_blob_by_id(state.writer_db(), blob.id).await? {
                runtime.deleted_blob_records = runtime.deleted_blob_records.saturating_add(1);
            }
        }
        persist_runtime(state, &lease_guard, &runtime).await?;
    }
    set_task_step_succeeded(
        &mut steps,
        TASK_STEP_CLEANUP_OBJECTS,
        Some("Unreferenced blob records removed"),
        None,
    )?;
    set_task_step_active(
        &mut steps,
        TASK_STEP_FINISH,
        Some("Deleting storage policy"),
        None,
    )?;
    // Upload sessions were abandoned locally above, but retain ordinary policy-delete
    // guards so a newly-created reference can never be silently bypassed.
    let force_delete = false;
    crate::services::storage_policy::policy::delete(state, payload.policy_id, force_delete).await?;
    set_task_step_succeeded(
        &mut steps,
        TASK_STEP_FINISH,
        Some("Storage policy permanently deleted"),
        None,
    )?;
    let result = StoragePolicyForcedPurgeTaskResult {
        policy_id: payload.policy_id,
        purged_files: runtime.purged_files,
        deleted_blob_records: runtime.deleted_blob_records,
        freed_logical_bytes: runtime.freed_logical_bytes,
        policy_deleted: true,
    };
    let result_json = super::spec::serialize_result::<StoragePolicyForcedPurgeTask>(&result)?;
    mark_task_succeeded(
        state,
        &lease_guard,
        Some(&result_json),
        runtime.purged_files,
        runtime.purged_files,
        Some("Storage policy forced purge completed"),
        &steps,
    )
    .await
}

struct ScopedFileBatch {
    scope: WorkspaceResourceScope,
    files: Vec<file::Model>,
}

/// Groups adjacent files by resource owner without reordering the global file-ID cursor.
///
/// Reordering an ID-sorted page by owner would make a persisted cursor unsafe: a
/// crash after one owner group could advance past lower-ID files in another group.
/// Keeping only adjacent equal scopes together guarantees every persisted cursor
/// covers one contiguous prefix of the original page.
fn contiguous_scope_batches(files: Vec<file::Model>) -> Result<Vec<ScopedFileBatch>> {
    let mut batches = Vec::<ScopedFileBatch>::new();
    let mut current_key = None::<(u8, i64)>;
    for file in files {
        let (key, scope) = resource_scope_for_file(&file)?;
        if current_key == Some(key) {
            if let Some(batch) = batches.last_mut() {
                batch.files.push(file);
            } else {
                return Err(AsterError::internal_error(
                    "forced purge scope cursor lost its current batch",
                ));
            }
        } else {
            current_key = Some(key);
            batches.push(ScopedFileBatch {
                scope,
                files: vec![file],
            });
        }
    }
    Ok(batches)
}

/// Resolves the quota and ownership scope for one persisted file row.
fn resource_scope_for_file(file: &file::Model) -> Result<((u8, i64), WorkspaceResourceScope)> {
    match (file.team_id, file.owner_user_id) {
        (Some(team_id), _) => Ok(((1, team_id), WorkspaceResourceScope::Team { team_id })),
        (None, Some(user_id)) => Ok(((0, user_id), WorkspaceResourceScope::Personal { user_id })),
        (None, None) => Err(AsterError::internal_error(format!(
            "forced purge file #{} has no resource owner",
            file.id
        ))),
    }
}

/// Serializes and persists the forced-purge cursor under the current fencing token.
async fn persist_runtime(
    state: &PrimaryAppState,
    lease_guard: &aster_forge_tasks::TaskLeaseGuard,
    runtime: &StoragePolicyForcedPurgeRuntime,
) -> Result<()> {
    let encoded = serde_json::to_string(runtime).map_err(|error| {
        AsterError::internal_error(format!("serialize forced purge runtime: {error}"))
    })?;
    set_task_runtime_json(state, lease_guard, Some(&encoded)).await
}

/// Decodes persisted runtime state and supplies the current schema version for a new task.
fn decode_runtime(raw: Option<&str>) -> Result<StoragePolicyForcedPurgeRuntime> {
    match raw {
        Some(raw) if !raw.trim().is_empty() => serde_json::from_str(raw).map_err(|error| {
            AsterError::internal_error(format!("decode forced purge runtime: {error}"))
        }),
        _ => Ok(StoragePolicyForcedPurgeRuntime {
            schema_version: 1,
            ..Default::default()
        }),
    }
}

/// Binds destructive confirmation to the current policy, references, and blockers.
fn build_forced_purge_preview(
    policy: &storage_policy::Model,
    blobs: file_repo::StoragePolicyBlobSummary,
    impact: file_repo::StoragePolicyPurgeImpact,
    placement_target_count: u64,
    upload_session_count: u64,
) -> Result<StoragePolicyForcedPurgePreview> {
    #[derive(Serialize)]
    struct ImpactIdentity<'a> {
        policy_id: i64,
        policy_name: &'a str,
        policy_updated_at: chrono::DateTime<chrono::Utc>,
        blob_count: i64,
        blob_total_bytes: i64,
        file_count: i64,
        trash_file_count: i64,
        affected_revision_count: i64,
        affected_logical_bytes: i64,
        direct_share_count: i64,
        placement_target_count: u64,
    }
    let identity = ImpactIdentity {
        policy_id: policy.id,
        policy_name: &policy.name,
        policy_updated_at: policy.updated_at,
        blob_count: blobs.count,
        blob_total_bytes: blobs.total_size,
        file_count: impact.file_count,
        trash_file_count: impact.trash_file_count,
        affected_revision_count: impact.affected_revision_count,
        affected_logical_bytes: impact.affected_logical_bytes,
        direct_share_count: impact.direct_share_count,
        placement_target_count,
    };
    let encoded = serde_json::to_vec(&identity).map_err(|error| {
        AsterError::internal_error(format!("serialize forced purge impact: {error}"))
    })?;
    Ok(StoragePolicyForcedPurgePreview {
        policy_id: policy.id,
        policy_name: policy.name.clone(),
        policy_updated_at: policy.updated_at,
        blob_count: blobs.count,
        blob_total_bytes: blobs.total_size,
        file_count: impact.file_count,
        trash_file_count: impact.trash_file_count,
        affected_revision_count: impact.affected_revision_count,
        affected_logical_bytes: impact.affected_logical_bytes,
        direct_share_count: impact.direct_share_count,
        placement_target_count,
        upload_session_count,
        can_start: placement_target_count == 0,
        confirmation_phrase: format!("DELETE {}", policy.name),
        impact_digest: sha256_hex(&encoded),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aster_drive_model::types::{StoredStoragePolicyAllowedTypes, StoredStoragePolicyConfig};

    /// Builds a stable policy model for digest and confirmation boundary tests.
    fn policy(name: &str) -> storage_policy::Model {
        storage_policy::Model {
            id: 7,
            name: name.to_string(),
            connector_id: "test".to_string(),
            storage_config: StoredStoragePolicyConfig::from("{}".to_string()),
            max_file_size: 0,
            allowed_types: StoredStoragePolicyAllowedTypes::from("[]".to_string()),
            is_default: false,
            chunk_size: 1,
            created_at: chrono::Utc::now(),
            updated_at: chrono::DateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn preview_digest_changes_with_impact_and_blockers() {
        let base = build_forced_purge_preview(
            &policy("lost"),
            file_repo::StoragePolicyBlobSummary {
                count: 2,
                total_size: 8,
            },
            file_repo::StoragePolicyPurgeImpact {
                file_count: 1,
                affected_revision_count: 2,
                affected_logical_bytes: 8,
                ..Default::default()
            },
            0,
            0,
        )
        .unwrap();
        let blocked = build_forced_purge_preview(
            &policy("lost"),
            file_repo::StoragePolicyBlobSummary {
                count: 2,
                total_size: 8,
            },
            file_repo::StoragePolicyPurgeImpact {
                file_count: 1,
                affected_revision_count: 2,
                affected_logical_bytes: 8,
                ..Default::default()
            },
            1,
            0,
        )
        .unwrap();
        assert_eq!(base.confirmation_phrase, "DELETE lost");
        assert!(base.can_start);
        assert!(!blocked.can_start);
        assert_ne!(base.impact_digest, blocked.impact_digest);
    }

    #[test]
    fn runtime_decoder_rejects_corrupt_json_and_defaults_new_tasks() {
        assert_eq!(decode_runtime(None).unwrap().schema_version, 1);
        assert!(decode_runtime(Some("not-json")).is_err());
    }

    /// Builds a minimal owned file row for stable batching tests.
    fn owned_file(id: i64, owner_user_id: Option<i64>, team_id: Option<i64>) -> file::Model {
        file::Model {
            id,
            name: format!("file-{id}"),
            folder_id: None,
            team_id,
            blob_id: id,
            size: 1,
            owner_user_id,
            created_by_user_id: owner_user_id,
            created_by_username: "tester".to_string(),
            mime_type: "text/plain".to_string(),
            extension: "txt".to_string(),
            compound_extension: None,
            file_category: aster_forge_file_classification::FileCategory::Document,
            created_at: chrono::DateTime::UNIX_EPOCH,
            updated_at: chrono::DateTime::UNIX_EPOCH,
            deleted_at: None,
        }
    }

    #[test]
    fn scope_batches_preserve_id_order_when_owners_are_interleaved() {
        let batches = contiguous_scope_batches(vec![
            owned_file(1, Some(10), None),
            owned_file(2, Some(20), None),
            owned_file(3, Some(10), None),
            owned_file(4, None, Some(30)),
            owned_file(5, None, Some(30)),
        ])
        .unwrap();
        let batch_ids = batches
            .iter()
            .map(|batch| batch.files.iter().map(|file| file.id).collect::<Vec<_>>())
            .collect::<Vec<_>>();
        assert_eq!(batch_ids, vec![vec![1], vec![2], vec![3], vec![4, 5]]);
    }

    #[test]
    fn scope_batching_rejects_files_without_user_or_team_ownership() {
        let error = contiguous_scope_batches(vec![owned_file(1, None, None)])
            .err()
            .expect("ownerless file must be rejected");
        assert!(error.to_string().contains("no resource owner"));
    }
}

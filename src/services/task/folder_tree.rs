//! Bounded, resumable folder-tree delete/restore tasks.

use aster_forge_db::transaction;
use aster_forge_tasks::{
    TaskExecutionContext, TaskLeaseGuard, TaskStepInfo, set_task_step_active,
    set_task_step_succeeded,
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ConnectionTrait, Set};

use crate::db::repository::{
    background_task_repo, file_repo, folder_repo, folder_tree_operation_repo, lock_namespace_repo,
    lock_repo,
};
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, TaskRuntimeState};
use crate::services::events::storage_change;
use crate::services::files::folder;
use crate::services::files::lock::{self, SubmittedLockCredentials};
use crate::services::task::spec::{self, FolderTreeMutationTask, decode_payload_as};
use crate::services::task::steps::{
    TASK_STEP_FOLDER_TREE, TASK_STEP_WAITING, parse_task_steps_json, serialize_task_steps,
};
use crate::services::task::types::{
    FolderTreeMutationOperation, FolderTreeMutationTaskPayload, FolderTreeMutationTaskResult,
    TaskInfo,
};
use crate::services::workspace::storage::{self, WorkspaceStorageScope};
use aster_drive_model::entities::{background_task, folder as folder_entity, resource_lock};
use aster_drive_model::types::{EntityType, LockDepth, LockMode, LockOrigin};

use super::{create_typed_task_record, mark_task_progress, task_scope};

const PAGE_SIZE: u64 = 512;
const MAX_BACKGROUND_DEPTH: usize = 4096;
const LOCK_MARKER_PREFIX: &str = "folder-tree-task:";

#[derive(Debug, Clone, Copy)]
struct Frame {
    folder_id: i64,
    next_child_after: Option<i64>,
    next_file_after: Option<i64>,
    entered: bool,
    files_done: bool,
    depth: usize,
}

pub(crate) async fn create_folder_tree_mutation_task_in_scope<S: TaskRuntimeState + Sync>(
    state: &S,
    scope: WorkspaceStorageScope,
    folder_id: i64,
    operation: FolderTreeMutationOperation,
) -> Result<TaskInfo> {
    storage::require_scope_access_with_db(state, state.writer_db(), scope).await?;
    let root = folder_repo::find_by_id(state.writer_db(), folder_id).await?;
    ensure_root_scope(&root, scope)?;
    ensure_operation_state(&root, operation)?;
    let payload = FolderTreeMutationTaskPayload {
        folder_id,
        operation,
    };
    let display_name = match operation {
        FolderTreeMutationOperation::Delete => "Delete folder tree",
        FolderTreeMutationOperation::Restore => "Restore folder tree",
    };
    let task =
        create_typed_task_record::<FolderTreeMutationTask>(state, scope, display_name, &payload)
            .await?;
    super::get_task_in_scope(state, scope, task.id).await
}

pub(crate) async fn process_folder_tree_mutation_task(
    state: &PrimaryAppState,
    task: &background_task::Model,
    context: TaskExecutionContext,
) -> Result<()> {
    let lease_guard = context.lease_guard().clone();
    let payload = decode_payload_as::<FolderTreeMutationTask>(task)?;
    let scope = task_scope(task)?;
    let root = folder_repo::find_by_id(state.writer_db(), payload.folder_id).await?;
    ensure_root_scope(&root, scope)?;
    ensure_operation_state(&root, payload.operation)?;
    let token = ensure_operation_lock(state, task, &lease_guard, scope, &root).await?;
    let result = async {
        let mut steps = parse_task_steps_json(task.steps_json.as_ref().map(|raw| raw.as_ref()))?;
        set_task_step_succeeded(
            &mut steps,
            TASK_STEP_WAITING,
            Some("Worker claimed task"),
            None,
        )?;
        set_task_step_active(
            &mut steps,
            TASK_STEP_FOLDER_TREE,
            Some("Scanning folder tree"),
            None,
        )?;
        mark_task_progress(
            state,
            &lease_guard,
            0,
            0,
            Some("Scanning folder tree"),
            &steps,
        )
        .await?;

        process_staging(
            state,
            task,
            &context,
            &lease_guard,
            scope,
            &payload,
            &token,
            root,
            &mut steps,
        )
        .await?;

        finalize_operation(
            state,
            task,
            &context,
            &lease_guard,
            scope,
            &payload,
            &token,
            &mut steps,
        )
        .await
    }
    .await;
    match result {
        Ok(()) => Ok(()),
        Err(error) => {
            if !super::is_task_lease_lost(&error) && !super::is_task_lease_renewal_timed_out(&error)
            {
                cleanup_operation(state, task.id, &token).await;
            }
            Err(error)
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the traversal keeps task, lease, scope, payload, lock, and progress state explicit"
)]
async fn process_staging(
    state: &PrimaryAppState,
    task: &background_task::Model,
    context: &TaskExecutionContext,
    lease_guard: &TaskLeaseGuard,
    scope: WorkspaceStorageScope,
    payload: &FolderTreeMutationTaskPayload,
    token: &str,
    root: folder_entity::Model,
    steps: &mut [TaskStepInfo],
) -> Result<()> {
    let include_deleted = payload.operation == FolderTreeMutationOperation::Restore;
    let folder_scope = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            folder_repo::FolderScope::Personal { user_id }
        }
        WorkspaceStorageScope::Team { team_id, .. } => folder_repo::FolderScope::Team { team_id },
    };
    let file_scope = match scope {
        WorkspaceStorageScope::Personal { user_id } => file_repo::FileScope::Personal { user_id },
        WorkspaceStorageScope::Team { team_id, .. } => file_repo::FileScope::Team { team_id },
    };
    let mut stack = vec![Frame {
        folder_id: root.id,
        next_child_after: None,
        next_file_after: None,
        entered: false,
        files_done: false,
        depth: 0,
    }];
    let mut staged = folder_tree_operation_repo::count(state.writer_db(), task.id).await?;

    while let Some(frame) = stack.last_mut() {
        context.ensure_active()?;
        ensure_background_depth(frame.depth)?;
        if !frame.entered {
            let folder_id = frame.folder_id;
            transaction::with_transaction(state.writer_db(), async |txn| {
                enforce_task_root_lock(txn, &root, token).await?;
                folder_tree_operation_repo::stage_ids(
                    txn,
                    task.id,
                    EntityType::Folder,
                    &[folder_id],
                )
                .await
            })
            .await?;
            frame.entered = true;
            staged = staged.saturating_add(1);
            update_task_progress(state, lease_guard, staged, "Scanning folder tree", steps).await?;
            continue;
        }
        if !frame.files_done {
            let folder_id = frame.folder_id;
            let after = frame.next_file_after;
            let ids = transaction::with_transaction(state.writer_db(), async |txn| {
                enforce_task_root_lock(txn, &root, token).await?;
                file_repo::find_ids_by_folder_after_id_in_scope(
                    txn,
                    file_scope,
                    folder_id,
                    after,
                    include_deleted,
                    PAGE_SIZE,
                )
                .await
            })
            .await?;
            if ids.is_empty() {
                frame.files_done = true;
            } else {
                let Some(last) = ids.last().copied() else {
                    continue;
                };
                transaction::with_transaction(state.writer_db(), async |txn| {
                    enforce_task_root_lock(txn, &root, token).await?;
                    folder_tree_operation_repo::stage_ids(txn, task.id, EntityType::File, &ids)
                        .await
                })
                .await?;
                frame.next_file_after = Some(last);
                staged = staged.saturating_add(u64::try_from(ids.len()).unwrap_or(u64::MAX));
                update_task_progress(state, lease_guard, staged, "Scanning folder tree", steps)
                    .await?;
            }
            continue;
        }

        let parent_id = frame.folder_id;
        let after = frame.next_child_after;
        let child = transaction::with_transaction(state.writer_db(), async |txn| {
            enforce_task_root_lock(txn, &root, token).await?;
            folder_repo::find_child_ids_after_id_in_scope(
                txn,
                folder_scope,
                parent_id,
                after,
                include_deleted,
                1,
            )
            .await
        })
        .await?
        .into_iter()
        .next();
        if let Some(child_id) = child {
            let child_depth = frame.depth.saturating_add(1);
            frame.next_child_after = Some(child_id);
            let _ = frame;
            stack.push(Frame {
                folder_id: child_id,
                next_child_after: None,
                next_file_after: None,
                entered: false,
                files_done: false,
                depth: child_depth,
            });
        } else {
            stack.pop();
        }
    }
    Ok(())
}

fn ensure_background_depth(depth: usize) -> Result<()> {
    if depth > MAX_BACKGROUND_DEPTH {
        return Err(AsterError::operation_resource_limit_exceeded(
            "folder tree background traversal exceeds the configured depth bound",
        ));
    }
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "finalization keeps task, lease, scope, payload, lock, and progress state explicit"
)]
async fn finalize_operation(
    state: &PrimaryAppState,
    task: &background_task::Model,
    context: &TaskExecutionContext,
    lease_guard: &TaskLeaseGuard,
    scope: WorkspaceStorageScope,
    payload: &FolderTreeMutationTaskPayload,
    token: &str,
    steps: &mut [TaskStepInfo],
) -> Result<()> {
    context.ensure_active()?;
    let now = Utc::now();
    let marker = operation_lock_marker(task.id);
    let result = transaction::with_transaction(state.writer_db(), async |txn| {
        let root = folder_repo::find_by_id(txn, payload.folder_id).await?;
        ensure_root_scope(&root, scope)?;
        enforce_task_root_lock(txn, &root, token).await?;
        let original_parent_id = root.parent_id;
        let mut restored_parent_id = root.parent_id;
        let counts = match payload.operation {
            FolderTreeMutationOperation::Delete => {
                folder_tree_operation_repo::apply_delete(txn, task.id, now).await?
            }
            FolderTreeMutationOperation::Restore => {
                let mut root_active: folder_entity::ActiveModel = root.clone().into();
                if let Some(parent_id) = root.parent_id {
                    match folder_repo::find_by_id(txn, parent_id).await {
                        Ok(parent)
                            if parent.deleted_at.is_some()
                                || ensure_root_scope(&parent, scope).is_err() =>
                        {
                            root_active.parent_id = Set(None);
                            restored_parent_id = None;
                        }
                        Ok(_) => {}
                        Err(AsterError::RecordNotFound(_)) => {
                            root_active.parent_id = Set(None);
                            restored_parent_id = None;
                        }
                        Err(error) => return Err(error),
                    }
                }
                root_active.deleted_at = Set(None);
                root_active
                    .update(txn)
                    .await
                    .map_err(|error| folder_repo::map_name_db_err(error, &root.name))?;
                folder_tree_operation_repo::apply_restore(txn, task.id).await?
            }
        };
        let (file_count, folder_count) = counts;
        let progress = i64::try_from(file_count.saturating_add(folder_count)).unwrap_or(i64::MAX);
        set_task_step_succeeded(
            steps,
            TASK_STEP_FOLDER_TREE,
            Some("Folder tree mutation finished"),
            Some((progress, progress)),
        )?;
        let task_result = FolderTreeMutationTaskResult {
            file_count,
            folder_count,
        };
        let encoded = spec::serialize_result::<FolderTreeMutationTask>(&task_result)?;
        let steps_json = serialize_task_steps(steps)?;

        folder_tree_operation_repo::clear(txn, task.id).await?;
        let operation_lock = lock_repo::find_by_token_for_update(txn, token)
            .await?
            .ok_or_else(|| AsterError::resource_locked("folder-tree task lock disappeared"))?;
        if !operation_lock_matches(&operation_lock, payload.folder_id, &marker)? {
            return Err(AsterError::resource_locked(
                "folder-tree task lock no longer matches the operation",
            ));
        }
        let namespace = lock_namespace_repo::lock_by_id(txn, operation_lock.namespace_id).await?;
        lock_repo::delete_by_id(txn, operation_lock.id).await?;
        lock_namespace_repo::increment_generation(txn, namespace).await?;

        let lease = lease_guard.lease();
        if !background_task_repo::mark_succeeded(
            txn,
            background_task_repo::TaskSuccessUpdate {
                id: lease.task_id,
                processing_token: lease.processing_token,
                result_json: Some(encoded.as_ref()),
                steps_json: Some(steps_json.as_ref()),
                current: progress,
                total: progress,
                status_text: Some("Folder tree mutation finished"),
                finished_at: now,
                expires_at: super::task_expiration_from(state, now),
            },
        )
        .await?
        {
            return Err(lease_guard.mark_lost().into());
        }
        Ok::<(FolderTreeMutationTaskResult, Option<i64>, Option<i64>), AsterError>((
            task_result,
            original_parent_id,
            restored_parent_id,
        ))
    })
    .await?;
    lease_guard.record_renewed();
    let (_task_result, original_parent_id, restored_parent_id) = result;
    let affected_parent_ids = match payload.operation {
        FolderTreeMutationOperation::Delete => vec![original_parent_id],
        FolderTreeMutationOperation::Restore => vec![restored_parent_id],
    };
    storage_change::publish(
        state,
        storage_change::StorageChangeEvent::new(
            match payload.operation {
                FolderTreeMutationOperation::Delete => {
                    storage_change::StorageChangeKind::FolderTrashed
                }
                FolderTreeMutationOperation::Restore => {
                    storage_change::StorageChangeKind::FolderRestoredFromTrash
                }
            },
            scope,
            vec![],
            vec![payload.folder_id],
            affected_parent_ids,
        ),
    );
    folder::invalidate_folder_path_cache_for_ids(state, &[payload.folder_id]).await;
    Ok(())
}

async fn ensure_operation_lock(
    state: &PrimaryAppState,
    task: &background_task::Model,
    lease_guard: &TaskLeaseGuard,
    scope: WorkspaceStorageScope,
    root: &folder_entity::Model,
) -> Result<String> {
    let marker = operation_lock_marker(task.id);
    if let Some(raw) = task.runtime_json.as_ref()
        && let Ok(token) = serde_json::from_str::<String>(raw.as_ref())
        && let Some(existing) = lock_repo::find_by_token(state.writer_db(), &token).await?
        && operation_lock_matches(&existing, root.id, &marker)?
    {
        return Ok(token);
    }
    for lock in
        lock_repo::find_all_by_entity(state.writer_db(), EntityType::Folder, root.id).await?
    {
        if operation_lock_matches(&lock, root.id, &marker)? {
            persist_lock_token(state, lease_guard, &lock.token).await?;
            return Ok(lock.token);
        }
    }
    let workspace = match scope {
        WorkspaceStorageScope::Personal { user_id } => lock::LockWorkspace::Personal { user_id },
        WorkspaceStorageScope::Team { team_id, .. } => lock::LockWorkspace::Team { team_id },
    };
    let lock = lock::acquire(
        state,
        lock::LockTarget {
            workspace,
            root: lock::LockRoot::Folder { folder_id: root.id },
            depth: LockDepth::Infinity,
        },
        LockMode::Exclusive,
        LockOrigin::Product,
        None,
        Some(lock::ResourceLockOwnerInfo::Text(lock::TextLockOwnerInfo {
            value: marker,
        })),
        None,
        lock::resolve_entity_path(state.writer_db(), EntityType::Folder, root.id)
            .await
            .map(Some)?,
    )
    .await?;
    persist_lock_token(state, lease_guard, &lock.token).await?;
    Ok(lock.token)
}

fn operation_lock_marker(task_id: i64) -> String {
    format!("{LOCK_MARKER_PREFIX}{task_id}")
}

fn operation_lock_matches(
    model: &resource_lock::Model,
    folder_id: i64,
    marker: &str,
) -> Result<bool> {
    if model.root_folder_id != Some(folder_id)
        || model.root_file_id.is_some()
        || model.depth != LockDepth::Infinity
        || model.mode != LockMode::Exclusive
        || model.origin != LockOrigin::Product
    {
        return Ok(false);
    }
    Ok(matches!(
        lock::deserialize_resource_lock_owner_info(model)?,
        Some(lock::ResourceLockOwnerInfo::Text(lock::TextLockOwnerInfo { value }))
            if value == marker
    ))
}

async fn persist_lock_token(
    state: &PrimaryAppState,
    lease_guard: &TaskLeaseGuard,
    token: &str,
) -> Result<()> {
    let raw = serde_json::to_string(token).map_err(|error| {
        AsterError::internal_error(format!("serialize folder-tree lock token: {error}"))
    })?;
    super::set_task_runtime_json(state, lease_guard, Some(&raw)).await
}

async fn cleanup_operation(state: &PrimaryAppState, task_id: i64, token: &str) {
    if let Err(error) = folder_tree_operation_repo::clear(state.writer_db(), task_id).await {
        tracing::warn!(task_id, %error, "failed to clear staged folder-tree operation members");
    }
    if let Err(error) = lock::unlock_by_token_on(state.writer_db(), token).await {
        tracing::warn!(task_id, %error, "failed to release folder-tree operation lock");
    }
}

async fn enforce_task_root_lock<C: ConnectionTrait>(
    db: &C,
    root: &folder_entity::Model,
    token: &str,
) -> Result<()> {
    let tokens = [token.to_string()];
    lock::enforce_folder_mutation_on(
        db,
        root,
        LockDepth::Infinity,
        &SubmittedLockCredentials {
            holder_user_id: None,
            tokens: &tokens,
        },
    )
    .await
    .map(|_| ())
}

async fn update_task_progress(
    state: &PrimaryAppState,
    lease_guard: &TaskLeaseGuard,
    current: u64,
    status: &str,
    steps: &[TaskStepInfo],
) -> Result<()> {
    mark_task_progress(
        state,
        lease_guard,
        i64::try_from(current).unwrap_or(i64::MAX),
        0,
        Some(status),
        steps,
    )
    .await
}

fn ensure_root_scope(root: &folder_entity::Model, scope: WorkspaceStorageScope) -> Result<()> {
    match scope {
        WorkspaceStorageScope::Personal { user_id }
            if root.team_id.is_none() && root.owner_user_id == Some(user_id) =>
        {
            Ok(())
        }
        WorkspaceStorageScope::Team { team_id, .. } if root.team_id == Some(team_id) => Ok(()),
        _ => Err(AsterError::record_not_found(
            "folder not found in workspace scope",
        )),
    }
}

fn ensure_operation_state(
    root: &folder_entity::Model,
    operation: FolderTreeMutationOperation,
) -> Result<()> {
    match operation {
        FolderTreeMutationOperation::Delete if root.deleted_at.is_none() => Ok(()),
        FolderTreeMutationOperation::Restore if root.deleted_at.is_some() => Ok(()),
        _ => Err(AsterError::record_not_found(
            "folder is not in the requested trash state",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{Frame, MAX_BACKGROUND_DEPTH, PAGE_SIZE, ensure_background_depth};

    #[test]
    fn background_depth_accepts_exact_limit_and_rejects_limit_plus_one() {
        ensure_background_depth(MAX_BACKGROUND_DEPTH)
            .expect("background traversal should accept the exact depth limit");
        let error = ensure_background_depth(MAX_BACKGROUND_DEPTH + 1)
            .expect_err("background traversal should reject depth limit plus one");
        assert!(matches!(
            error,
            crate::errors::AsterError::OperationResourceLimitExceeded(_)
        ));
    }

    async fn exercise_bounded_working_set(total_resources: usize) {
        let mut stack = Vec::with_capacity(MAX_BACKGROUND_DEPTH + 1);
        for depth in 0..=MAX_BACKGROUND_DEPTH {
            stack.push(Frame {
                folder_id: i64::try_from(depth).expect("fixture depth should fit i64"),
                next_child_after: None,
                next_file_after: None,
                entered: true,
                files_done: false,
                depth,
            });
        }

        let page_size = usize::try_from(PAGE_SIZE).expect("page size should fit usize");
        let mut remaining = total_resources;
        while remaining > 0 {
            let current_page = remaining.min(page_size);
            let ids: Vec<i64> = (0..current_page)
                .map(|id| i64::try_from(id).expect("fixture id should fit i64"))
                .collect();
            std::hint::black_box(&ids);
            remaining -= current_page;
        }
        std::hint::black_box(&stack);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn large_fixtures_keep_peak_working_set_bounded() {
        let mut peaks = Vec::new();
        for total_resources in [100_000, 500_000, 1_000_000] {
            let (_, allocations) = crate::test_support::allocations::measure_future(
                exercise_bounded_working_set(total_resources),
            )
            .await;
            peaks.push(allocations.peak_bytes);
        }

        let smallest = peaks[0];
        for peak in peaks {
            assert!(
                peak <= smallest.saturating_add(4096),
                "bounded traversal peak grew with fixture size: baseline={smallest}, peak={peak}"
            );
        }
    }
}

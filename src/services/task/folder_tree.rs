//! Bounded, resumable folder-tree delete/restore tasks.

use aster_forge_db::transaction;
use aster_forge_tasks::{
    TaskDedupeKey, TaskExecutionContext, TaskLeaseGuard, TaskStepInfo, set_task_step_active,
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

use super::{TypedTaskCreate, insert_typed_task_record, mark_task_progress, task_scope};

const PAGE_SIZE: u64 = 512;
const MAX_BACKGROUND_DEPTH: usize = 4096;
const LOCK_MARKER_PREFIX: &str = "folder-tree-task:";

#[derive(Debug, Clone)]
struct Frame {
    folder_id: i64,
    next_child_after: Option<i64>,
    child_page: Vec<i64>,
    next_child_index: usize,
    next_file_after: Option<i64>,
    entered: bool,
    files_done: bool,
    depth: usize,
}

#[cfg(test)]
#[derive(Debug, Default)]
struct TraversalWorkingSetObserver {
    current_bytes: usize,
    peak_bytes: usize,
    peak_bytes_by_stack_len: Vec<usize>,
    stop_at_stack_len: Option<usize>,
}

#[cfg(test)]
impl TraversalWorkingSetObserver {
    fn stopping_at_stack_len(stack_len: usize) -> Self {
        Self {
            stop_at_stack_len: Some(stack_len),
            ..Self::default()
        }
    }

    fn observe(&mut self, stack: &[Frame]) -> bool {
        let frame_bytes = stack.len().saturating_mul(std::mem::size_of::<Frame>());
        let cached_child_bytes = stack.iter().fold(0usize, |total, frame| {
            total.saturating_add(
                frame
                    .child_page
                    .capacity()
                    .saturating_mul(std::mem::size_of::<i64>()),
            )
        });
        self.current_bytes = frame_bytes.saturating_add(cached_child_bytes);
        self.peak_bytes = self.peak_bytes.max(self.current_bytes);
        if self.peak_bytes_by_stack_len.len() <= stack.len() {
            self.peak_bytes_by_stack_len
                .resize(stack.len().saturating_add(1), 0);
        }
        self.peak_bytes_by_stack_len[stack.len()] =
            self.peak_bytes_by_stack_len[stack.len()].max(self.current_bytes);
        self.stop_at_stack_len == Some(stack.len())
            && stack.last().is_some_and(|frame| frame.entered)
    }

    fn observe_transient_page(&mut self, capacity: usize) {
        self.peak_bytes = self.peak_bytes.max(
            self.current_bytes
                .saturating_add(capacity.saturating_mul(std::mem::size_of::<i64>())),
        );
    }

    fn peak_at_stack_len(&self, stack_len: usize) -> Option<usize> {
        self.peak_bytes_by_stack_len
            .get(stack_len)
            .copied()
            .filter(|peak| *peak > 0)
    }
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
    let dedupe_key = folder_tree_dedupe_key(scope, &root, operation)?;
    let create_request = || {
        TypedTaskCreate::<FolderTreeMutationTask>::new(display_name, payload.clone())
            .in_scope(scope)
            .dedupe_key(dedupe_key.clone())
    };
    let mut task = insert_typed_task_record(state, state.writer_db(), create_request()).await?;
    if task.status == aster_drive_model::types::BackgroundTaskStatus::Failed
        && background_task_repo::clear_failed_dedupe_key(
            state.writer_db(),
            task.id,
            dedupe_key.as_str(),
        )
        .await?
    {
        task = insert_typed_task_record(state, state.writer_db(), create_request()).await?;
    }
    state.wake_background_task_dispatcher();
    super::get_task_in_scope(state, scope, task.id).await
}

fn folder_tree_dedupe_key(
    scope: WorkspaceStorageScope,
    root: &folder_entity::Model,
    operation: FolderTreeMutationOperation,
) -> Result<TaskDedupeKey> {
    let scope_key = match scope {
        WorkspaceStorageScope::Personal { user_id } => format!("personal:{user_id}"),
        WorkspaceStorageScope::Team { team_id, .. } => format!("team:{team_id}"),
    };
    let operation_key = match operation {
        FolderTreeMutationOperation::Delete => "delete",
        FolderTreeMutationOperation::Restore => "restore",
    };
    Ok(TaskDedupeKey::new(format!(
        "folder-tree:{scope_key}:{}:{operation_key}:{}",
        root.id,
        root.updated_at.timestamp_micros()
    ))?)
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
            #[cfg(test)]
            None,
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
            let preserves_resumable_state = should_preserve_resumable_state(task.kind, &error);
            if !preserves_resumable_state {
                cleanup_operation(state, task.id, &token).await;
            }
            Err(error)
        }
    }
}

fn should_preserve_resumable_state(
    kind: aster_drive_model::types::BackgroundTaskKind,
    error: &AsterError,
) -> bool {
    super::is_task_lease_lost(error)
        || super::is_task_lease_renewal_timed_out(error)
        || super::registry::task_retry_class(kind, error).should_auto_retry()
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
    #[cfg(test)] mut working_set_observer: Option<&mut TraversalWorkingSetObserver>,
) -> Result<()> {
    let lock_tokens = [token.to_string()];
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
        child_page: Vec::new(),
        next_child_index: 0,
        next_file_after: None,
        entered: false,
        files_done: false,
        depth: 0,
    }];
    let mut staged: u64;

    while !stack.is_empty() {
        #[cfg(test)]
        if let Some(observer) = working_set_observer.as_deref_mut()
            && observer.observe(&stack)
        {
            break;
        }
        let Some(frame) = stack.last_mut() else {
            break;
        };
        context.ensure_active()?;
        ensure_background_depth(frame.depth)?;
        if !frame.entered {
            let folder_id = frame.folder_id;
            let staged_total = transaction::with_transaction(state.writer_db(), async |txn| {
                enforce_task_root_lock(txn, &root, &lock_tokens).await?;
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
            staged = staged_total;
            update_task_progress(state, lease_guard, staged, "Scanning folder tree", steps).await?;
            continue;
        }
        if !frame.files_done {
            let folder_id = frame.folder_id;
            let after = frame.next_file_after;
            let ids = transaction::with_transaction(state.writer_db(), async |txn| {
                enforce_task_root_lock(txn, &root, &lock_tokens).await?;
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
                #[cfg(test)]
                if let Some(observer) = working_set_observer.as_deref_mut() {
                    observer.observe_transient_page(ids.capacity());
                }
                let Some(last) = ids.last().copied() else {
                    return Err(AsterError::internal_error(
                        "non-empty folder-tree file page lost its final ID",
                    ));
                };
                let staged_total = transaction::with_transaction(state.writer_db(), async |txn| {
                    enforce_task_root_lock(txn, &root, &lock_tokens).await?;
                    folder_tree_operation_repo::stage_ids(txn, task.id, EntityType::File, &ids)
                        .await
                })
                .await?;
                frame.next_file_after = Some(last);
                staged = staged_total;
                update_task_progress(state, lease_guard, staged, "Scanning folder tree", steps)
                    .await?;
            }
            continue;
        }

        if frame.next_child_index >= frame.child_page.len() {
            let parent_id = frame.folder_id;
            let after = frame.next_child_after;
            let mut child_page = transaction::with_transaction(state.writer_db(), async |txn| {
                enforce_task_root_lock(txn, &root, &lock_tokens).await?;
                folder_repo::find_child_ids_after_id_in_scope(
                    txn,
                    folder_scope,
                    parent_id,
                    after,
                    include_deleted,
                    PAGE_SIZE,
                )
                .await
            })
            .await?;
            if child_page.is_empty() {
                stack.pop();
                continue;
            }
            #[cfg(test)]
            if let Some(observer) = working_set_observer.as_deref_mut() {
                observer.observe_transient_page(child_page.capacity());
            }
            child_page.shrink_to_fit();
            frame.next_child_after = child_page.last().copied();
            frame.child_page = child_page;
            frame.next_child_index = 0;
        }

        let child_frame = take_next_child_frame(frame)?;
        stack.push(child_frame);
    }
    Ok(())
}

fn take_next_child_frame(frame: &mut Frame) -> Result<Frame> {
    let Some(child_id) = frame.child_page.get(frame.next_child_index).copied() else {
        return Err(AsterError::internal_error(
            "folder-tree child page index moved past its cached page",
        ));
    };
    let child_depth = frame.depth.saturating_add(1);
    frame.next_child_index = frame.next_child_index.saturating_add(1);
    Ok(Frame {
        folder_id: child_id,
        next_child_after: None,
        child_page: Vec::new(),
        next_child_index: 0,
        next_file_after: None,
        entered: false,
        files_done: false,
        depth: child_depth,
    })
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
    let lock_tokens = [token.to_string()];
    context.ensure_active()?;
    let now = Utc::now();
    let marker = operation_lock_marker(task.id);
    let result = transaction::with_transaction(state.writer_db(), async |txn| {
        let root = folder_repo::find_by_id(txn, payload.folder_id).await?;
        ensure_root_scope(&root, scope)?;
        enforce_task_root_lock(txn, &root, &lock_tokens).await?;
        let original_parent_id = root.parent_id;
        let mut restored_parent_id = root.parent_id;
        let counts = match payload.operation {
            FolderTreeMutationOperation::Delete => {
                let counts = folder_tree_operation_repo::apply_delete(txn, task.id, now).await?;
                let mut root_active: folder_entity::ActiveModel = root.clone().into();
                root_active.updated_at = Set(now);
                root_active.update(txn).await.map_err(AsterError::from)?;
                counts
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
                root_active.updated_at = Set(now);
                root_active
                    .update(txn)
                    .await
                    .map_err(|error| folder_repo::map_name_db_err(error, &root.name))?;
                // Staged restore changes only trash/update timestamps. It must not overwrite the
                // root parent selected above, which may have been detached from an unavailable
                // parent.
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

/// Cleans staging and the product lock for one terminal folder-tree task.
///
/// Callers must provide a transaction-backed connection because lock deletion and namespace
/// generation changes form one atomic cleanup operation.
pub(super) async fn cleanup_terminal_operation_on<C: ConnectionTrait>(
    db: &C,
    task: &background_task::Model,
) -> Result<()> {
    if task.kind != aster_drive_model::types::BackgroundTaskKind::FolderTreeMutation {
        return Ok(());
    }
    let payload = match decode_payload_as::<FolderTreeMutationTask>(task) {
        Ok(payload) => payload,
        Err(error) => {
            tracing::error!(
                task_id = task.id,
                %error,
                "failed to decode folder-tree task payload during terminal cleanup"
            );
            folder_tree_operation_repo::clear(db, task.id).await?;
            return Ok(());
        }
    };
    let marker = operation_lock_marker(task.id);
    folder_tree_operation_repo::clear(db, task.id).await?;
    for operation_lock in
        lock_repo::find_all_by_entity_for_update(db, EntityType::Folder, payload.folder_id).await?
    {
        if !operation_lock_matches(&operation_lock, payload.folder_id, &marker)? {
            continue;
        }
        let namespace = lock_namespace_repo::lock_by_id(db, operation_lock.namespace_id).await?;
        lock_repo::delete_by_id(db, operation_lock.id).await?;
        lock_namespace_repo::increment_generation(db, namespace).await?;
    }
    Ok(())
}

async fn enforce_task_root_lock<C: ConnectionTrait>(
    db: &C,
    root: &folder_entity::Model,
    tokens: &[String],
) -> Result<()> {
    lock::enforce_folder_mutation_on(
        db,
        root,
        LockDepth::Infinity,
        &SubmittedLockCredentials {
            holder_user_id: None,
            tokens,
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
    use std::sync::Arc;

    use aster_forge_tasks::{TaskExecutionContext, TaskLease};
    use chrono::Utc;
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};
    use tokio_util::sync::CancellationToken;

    use super::{
        Frame, MAX_BACKGROUND_DEPTH, ensure_background_depth, should_preserve_resumable_state,
        take_next_child_frame,
    };
    use crate::config::DatabaseConfig;
    use crate::db::repository::{background_task_repo, config_repo, folder_repo};
    use crate::services::task::types::FolderTreeMutationOperation;
    use crate::services::workspace::storage::WorkspaceStorageScope;
    use aster_drive_migration::Migrator;
    use aster_drive_model::entities::{background_task, folder, user};
    use aster_drive_model::types::{
        BackgroundTaskKind, BackgroundTaskStatus, EntityType, StoredTaskPayload, UserRole,
        UserStatus,
    };

    async fn build_test_state() -> crate::runtime::PrimaryAppState {
        let db = crate::db::connect_with_metrics(
            &DatabaseConfig {
                url: "sqlite::memory:".into(),
                pool_size: 1,
                retry_count: 0,
            },
            aster_drive_metrics::NoopMetrics::arc(),
        )
        .await
        .expect("folder-tree memory test DB should connect");
        Migrator::up(&db, None)
            .await
            .expect("folder-tree memory test migrations should apply");
        config_repo::ensure_defaults_with_env(&db, &|_| None)
            .await
            .expect("folder-tree memory test config defaults should exist");
        let runtime_config = Arc::new(crate::config::RuntimeConfig::new());
        runtime_config
            .reload(&db)
            .await
            .expect("folder-tree memory test runtime config should reload");
        let cache = aster_forge_cache::create_cache(&aster_forge_cache::CacheConfig {
            ..Default::default()
        })
        .await;
        let storage_change_bus = crate::services::events::storage_change::StorageChangeBus::new(
            crate::services::events::storage_change::STORAGE_CHANGE_CHANNEL_CAPACITY,
        );
        let (share_download_rollback, _worker) =
            crate::services::share::build_share_download_rollback_queue(
                db.clone(),
                1,
                aster_drive_metrics::NoopMetrics::arc(),
            );
        crate::runtime::PrimaryAppState {
            db_handles: aster_forge_db::DbHandles::single(db),
            driver_registry: Arc::new(
                crate::storage::DriverRegistry::noop()
                    .expect("built-in storage connector registry"),
            ),
            runtime_config,
            policy_snapshot: Arc::new(crate::storage::PolicySnapshot::new()),
            config: Arc::new(crate::config::Config::default()),
            cache,
            config_sync: aster_forge_config::ConfigSyncRuntime::disabled_for_test("aster_drive"),
            metrics: aster_drive_metrics::NoopMetrics::arc(),
            mail_sender: aster_forge_mail::memory_sender(),
            storage_change_bus,
            share_download_rollback,
            background_task_dispatch_wakeup:
                crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
            remote_protocol: crate::runtime::PrimaryAppState::new_remote_protocol(),
        }
    }

    async fn insert_test_user_and_root(
        state: &crate::runtime::PrimaryAppState,
        fixture: &str,
        deleted_at: Option<chrono::DateTime<Utc>>,
    ) -> (user::Model, folder::Model) {
        let now = Utc::now();
        let user = user::ActiveModel {
            username: Set(format!("folder-tree-{fixture}")),
            email: Set(format!("folder-tree-{fixture}@example.com")),
            password_hash: Set("not-used".to_string()),
            role: Set(UserRole::User),
            status: Set(UserStatus::Active),
            session_version: Set(0),
            email_verified_at: Set(Some(now)),
            pending_email: Set(None),
            storage_used: Set(0),
            storage_quota: Set(0),
            policy_group_id: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            config: Set(None),
            ..Default::default()
        }
        .insert(state.writer_db())
        .await
        .expect("folder-tree fixture user should insert");
        let root = folder::ActiveModel {
            name: Set(format!("{fixture}-root")),
            parent_id: Set(None),
            team_id: Set(None),
            owner_user_id: Set(Some(user.id)),
            created_by_user_id: Set(Some(user.id)),
            created_by_username: Set(user.username.clone()),
            policy_id: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            deleted_at: Set(deleted_at),
            ..Default::default()
        }
        .insert(state.writer_db())
        .await
        .expect("folder-tree fixture root should insert");
        (user, root)
    }

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

    #[test]
    fn transient_retry_preserves_staging_but_manual_rescan_errors_do_not() {
        assert!(should_preserve_resumable_state(
            BackgroundTaskKind::FolderTreeMutation,
            &crate::errors::AsterError::database_connection("transient disconnect"),
        ));
        assert!(!should_preserve_resumable_state(
            BackgroundTaskKind::FolderTreeMutation,
            &crate::errors::AsterError::resource_locked("tree member locked"),
        ));
    }

    #[test]
    fn child_page_out_of_bounds_is_an_error_instead_of_a_stalled_frame() {
        let mut frame = Frame {
            folder_id: 1,
            next_child_after: Some(2),
            child_page: vec![2],
            next_child_index: 1,
            next_file_after: None,
            entered: true,
            files_done: true,
            depth: 0,
        };

        let error = take_next_child_frame(&mut frame)
            .expect_err("an exhausted child page must not leave the traversal frame unchanged");
        assert!(error.message().contains("child page index"));
    }

    #[tokio::test]
    async fn duplicate_queued_delete_and_restore_submissions_reuse_the_same_task() {
        for (fixture, operation, deleted_at) in [
            ("dedupe-delete", FolderTreeMutationOperation::Delete, None),
            (
                "dedupe-restore",
                FolderTreeMutationOperation::Restore,
                Some(Utc::now()),
            ),
        ] {
            let state = build_test_state().await;
            let (user, root) = insert_test_user_and_root(&state, fixture, deleted_at).await;
            let scope = WorkspaceStorageScope::Personal { user_id: user.id };

            let first =
                super::create_folder_tree_mutation_task_in_scope(&state, scope, root.id, operation)
                    .await
                    .expect("first folder-tree task should create");
            let duplicate =
                super::create_folder_tree_mutation_task_in_scope(&state, scope, root.id, operation)
                    .await
                    .expect("duplicate folder-tree task should resolve");

            assert_eq!(first.id, duplicate.id);
            assert_eq!(
                background_task::Entity::find()
                    .filter(
                        background_task::Column::Kind.eq(BackgroundTaskKind::FolderTreeMutation)
                    )
                    .count(state.writer_db())
                    .await
                    .expect("folder-tree task count should load"),
                1
            );
        }
    }

    #[tokio::test]
    async fn failed_folder_tree_task_releases_dedupe_key_for_resubmission() {
        let state = build_test_state().await;
        let (user, root) = insert_test_user_and_root(&state, "dedupe-failed", None).await;
        let scope = WorkspaceStorageScope::Personal { user_id: user.id };
        let first = super::create_folder_tree_mutation_task_in_scope(
            &state,
            scope,
            root.id,
            FolderTreeMutationOperation::Delete,
        )
        .await
        .expect("first folder-tree task should create");
        let first_model = background_task_repo::find_by_id(state.writer_db(), first.id)
            .await
            .expect("first folder-tree task should load");
        let failed_at = Utc::now();
        let mut failed: background_task::ActiveModel = first_model.into();
        failed.status = Set(BackgroundTaskStatus::Failed);
        failed.finished_at = Set(Some(failed_at));
        failed.updated_at = Set(failed_at);
        failed
            .update(state.writer_db())
            .await
            .expect("folder-tree task should enter failed terminal state");

        let resubmitted = super::create_folder_tree_mutation_task_in_scope(
            &state,
            scope,
            root.id,
            FolderTreeMutationOperation::Delete,
        )
        .await
        .expect("failed folder-tree task should not block resubmission");

        assert_ne!(first.id, resubmitted.id);
        assert!(
            background_task_repo::find_by_id(state.writer_db(), first.id)
                .await
                .expect("failed folder-tree task should remain available for history")
                .dedupe_key
                .is_none()
        );
        assert!(
            background_task_repo::find_by_id(state.writer_db(), resubmitted.id)
                .await
                .expect("resubmitted folder-tree task should load")
                .dedupe_key
                .is_some()
        );
    }

    #[tokio::test]
    async fn folder_tree_dedupe_key_separates_operation_scope_and_mutation_cycle() {
        let state = build_test_state().await;
        let (user, root) = insert_test_user_and_root(&state, "dedupe-key", None).await;
        let personal_scope = WorkspaceStorageScope::Personal { user_id: user.id };
        let team_scope = WorkspaceStorageScope::Team {
            team_id: 42,
            actor_user_id: user.id,
        };
        let delete = super::folder_tree_dedupe_key(
            personal_scope,
            &root,
            FolderTreeMutationOperation::Delete,
        )
        .unwrap();
        let restore = super::folder_tree_dedupe_key(
            personal_scope,
            &root,
            FolderTreeMutationOperation::Restore,
        )
        .unwrap();
        let team =
            super::folder_tree_dedupe_key(team_scope, &root, FolderTreeMutationOperation::Delete)
                .unwrap();
        let mut next_cycle_root = root.clone();
        next_cycle_root.updated_at += chrono::Duration::microseconds(1);
        let next_cycle = super::folder_tree_dedupe_key(
            personal_scope,
            &next_cycle_root,
            FolderTreeMutationOperation::Delete,
        )
        .unwrap();

        assert_ne!(delete, restore);
        assert_ne!(delete, team);
        assert_ne!(delete, next_cycle);
    }

    fn memory_folder(
        user: &user::Model,
        parent_id: i64,
        name: String,
        now: chrono::DateTime<Utc>,
    ) -> folder::ActiveModel {
        folder::ActiveModel {
            name: Set(name),
            parent_id: Set(Some(parent_id)),
            team_id: Set(None),
            owner_user_id: Set(Some(user.id)),
            created_by_user_id: Set(Some(user.id)),
            created_by_username: Set(user.username.clone()),
            policy_id: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            deleted_at: Set(None),
            ..Default::default()
        }
    }

    async fn measure_real_traversal_peak(
        state: &crate::runtime::PrimaryAppState,
        user: &user::Model,
        root: folder::Model,
        expected_staged_folder_count: usize,
        mut observer: super::TraversalWorkingSetObserver,
    ) -> super::TraversalWorkingSetObserver {
        let task_info = super::create_folder_tree_mutation_task_in_scope(
            state,
            WorkspaceStorageScope::Personal { user_id: user.id },
            root.id,
            FolderTreeMutationOperation::Delete,
        )
        .await
        .expect("folder-tree memory task should create");
        let claimed_at = Utc::now();
        assert!(
            background_task_repo::try_claim(
                state.writer_db(),
                task_info.id,
                0,
                claimed_at,
                claimed_at - chrono::Duration::minutes(10),
                1,
                claimed_at + chrono::Duration::minutes(1),
            )
            .await
            .expect("folder-tree memory task claim should execute")
        );
        let task = background_task_repo::find_by_id(state.writer_db(), task_info.id)
            .await
            .expect("claimed folder-tree memory task should load");
        let context = TaskExecutionContext::new(
            TaskLease::new(task.id, task.processing_token),
            std::time::Duration::from_secs(60),
            CancellationToken::new(),
        );
        let lease_guard = context.lease_guard().clone();
        let scope = WorkspaceStorageScope::Personal { user_id: user.id };
        let token = super::ensure_operation_lock(state, &task, &lease_guard, scope, &root)
            .await
            .expect("folder-tree memory operation lock should acquire");
        let payload = crate::services::task::types::FolderTreeMutationTaskPayload {
            folder_id: root.id,
            operation: FolderTreeMutationOperation::Delete,
        };
        let mut steps = crate::services::task::steps::parse_task_steps_json(
            task.steps_json.as_ref().map(|raw| raw.as_ref()),
        )
        .expect("folder-tree memory task steps should decode");
        super::process_staging(
            state,
            &task,
            &context,
            &lease_guard,
            scope,
            &payload,
            &token,
            root,
            &mut steps,
            Some(&mut observer),
        )
        .await
        .expect("real folder-tree staging traversal should finish");
        assert_eq!(
            crate::db::repository::folder_tree_operation_repo::count(state.writer_db(), task.id)
                .await
                .expect("folder-tree memory staging count should load"),
            u64::try_from(expected_staged_folder_count).unwrap()
        );
        super::cleanup_operation(state, task.id, &token).await;
        assert_eq!(
            crate::db::repository::folder_tree_operation_repo::count(state.writer_db(), task.id)
                .await
                .expect("cleaned folder-tree memory staging count should load"),
            0
        );
        observer
    }

    async fn measure_wide_traversal_peak(child_count: usize) -> usize {
        const INSERT_BATCH: usize = 400;

        let state = build_test_state().await;
        let (user, root) =
            insert_test_user_and_root(&state, &format!("memory-wide-{child_count}"), None).await;
        let now = Utc::now();
        for batch_start in (0..child_count).step_by(INSERT_BATCH) {
            let batch_end = (batch_start + INSERT_BATCH).min(child_count);
            let children = (batch_start..batch_end).map(|index| {
                memory_folder(&user, root.id, format!("memory-child-{index:06}"), now)
            });
            folder_repo::create_many(state.writer_db(), children.collect())
                .await
                .expect("folder-tree memory children should insert");
        }

        measure_real_traversal_peak(
            &state,
            &user,
            root,
            child_count.saturating_add(1),
            super::TraversalWorkingSetObserver::default(),
        )
        .await
        .peak_bytes
    }

    async fn measure_deep_traversal(depth: usize) -> super::TraversalWorkingSetObserver {
        const INSERT_BATCH: usize = 400;

        let page_width = usize::try_from(super::PAGE_SIZE).expect("page size should fit usize");
        let state = build_test_state().await;
        let (user, root) =
            insert_test_user_and_root(&state, &format!("memory-deep-{depth}"), None).await;
        let now = Utc::now();
        let mut parent_id = root.id;
        for level in 0..depth {
            let spine = memory_folder(
                &user,
                parent_id,
                format!("memory-depth-{level:04}-0000"),
                now,
            )
            .insert(state.writer_db())
            .await
            .expect("folder-tree memory spine should insert");
            for batch_start in (1..page_width).step_by(INSERT_BATCH) {
                let batch_end = (batch_start + INSERT_BATCH).min(page_width);
                let siblings = (batch_start..batch_end).map(|index| {
                    memory_folder(
                        &user,
                        parent_id,
                        format!("memory-depth-{level:04}-{index:04}"),
                        now,
                    )
                });
                folder_repo::create_many(state.writer_db(), siblings.collect())
                    .await
                    .expect("folder-tree memory depth siblings should insert");
            }
            parent_id = spine.id;
        }

        measure_real_traversal_peak(
            &state,
            &user,
            root,
            depth.saturating_add(1),
            super::TraversalWorkingSetObserver::stopping_at_stack_len(depth.saturating_add(1)),
        )
        .await
    }

    #[tokio::test(flavor = "current_thread")]
    async fn large_fixtures_keep_peak_working_set_bounded() {
        let mut peaks = Vec::new();
        for child_count in [2_000, 10_000, 20_000] {
            peaks.push(measure_wide_traversal_peak(child_count).await);
        }

        let baseline = peaks[0];
        for &peak in &peaks {
            assert!(
                peak <= baseline.saturating_add(4096),
                "bounded traversal peak grew with fixture size: baseline={baseline}, peak={peak}, all={peaks:?}"
            );
        }

        let deepest_fixture = 64usize;
        let depth_observer = measure_deep_traversal(deepest_fixture).await;
        let measured_depth_peaks = [8, 32, deepest_fixture].map(|depth| {
            (
                depth,
                depth_observer
                    .peak_at_stack_len(depth.saturating_add(1))
                    .expect("deep traversal should observe every stack depth"),
            )
        });
        let page_bytes = usize::try_from(super::PAGE_SIZE)
            .expect("page size should fit usize")
            .saturating_mul(std::mem::size_of::<i64>());
        for pair in measured_depth_peaks.windows(2) {
            let (shallower_depth, shallower_peak) = pair[0];
            let (deeper_depth, deeper_peak) = pair[1];
            let added_depth = deeper_depth.saturating_sub(shallower_depth);
            let added_bytes = deeper_peak.saturating_sub(shallower_peak);
            assert!(
                deeper_peak > shallower_peak,
                "cached child pages should grow with traversal depth: {measured_depth_peaks:?}"
            );
            assert!(
                added_bytes >= added_depth.saturating_mul(page_bytes),
                "each additional depth should retain one full child page: {measured_depth_peaks:?}"
            );
            assert!(
                added_bytes
                    <= added_depth.saturating_mul(
                        page_bytes
                            .saturating_mul(2)
                            .saturating_add(std::mem::size_of::<Frame>()),
                    ),
                "depth working set should remain linear in cached pages: {measured_depth_peaks:?}"
            );
        }
        let maximum_bounded_working_set = MAX_BACKGROUND_DEPTH.saturating_add(2).saturating_mul(
            page_bytes
                .saturating_mul(2)
                .saturating_add(std::mem::size_of::<Frame>()),
        );
        assert!(
            measured_depth_peaks
                .iter()
                .all(|(_, peak)| *peak <= maximum_bounded_working_set),
            "depth fixtures must remain within the configured traversal bound: bound={maximum_bounded_working_set}, peaks={measured_depth_peaks:?}"
        );
    }

    #[tokio::test]
    async fn admin_terminal_cleanup_releases_staging_and_operation_lock() {
        let state = build_test_state().await;
        let now = Utc::now();
        let user = user::ActiveModel {
            username: Set("folder-tree-terminal-cleanup".to_string()),
            email: Set("folder-tree-terminal-cleanup@example.com".to_string()),
            password_hash: Set("not-used".to_string()),
            role: Set(UserRole::User),
            status: Set(UserStatus::Active),
            session_version: Set(0),
            email_verified_at: Set(Some(now)),
            pending_email: Set(None),
            storage_used: Set(0),
            storage_quota: Set(0),
            policy_group_id: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            config: Set(None),
            ..Default::default()
        }
        .insert(state.writer_db())
        .await
        .expect("terminal cleanup user should insert");
        let root = folder::ActiveModel {
            name: Set("terminal-cleanup-root".to_string()),
            parent_id: Set(None),
            team_id: Set(None),
            owner_user_id: Set(Some(user.id)),
            created_by_user_id: Set(Some(user.id)),
            created_by_username: Set(user.username.clone()),
            policy_id: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            deleted_at: Set(None),
            ..Default::default()
        }
        .insert(state.writer_db())
        .await
        .expect("terminal cleanup root should insert");
        let task_info = super::create_folder_tree_mutation_task_in_scope(
            &state,
            WorkspaceStorageScope::Personal { user_id: user.id },
            root.id,
            FolderTreeMutationOperation::Delete,
        )
        .await
        .expect("terminal cleanup task should create");
        let claimed_at = Utc::now();
        assert!(
            background_task_repo::try_claim(
                state.writer_db(),
                task_info.id,
                0,
                claimed_at,
                claimed_at - chrono::Duration::minutes(10),
                1,
                claimed_at + chrono::Duration::minutes(1),
            )
            .await
            .expect("terminal cleanup task claim should execute")
        );
        let task = background_task_repo::find_by_id(state.writer_db(), task_info.id)
            .await
            .expect("claimed terminal cleanup task should load");
        let lease_guard = aster_forge_tasks::TaskLeaseGuard::new(
            TaskLease::new(task.id, task.processing_token),
            std::time::Duration::from_secs(60),
        );
        super::ensure_operation_lock(
            &state,
            &task,
            &lease_guard,
            WorkspaceStorageScope::Personal { user_id: user.id },
            &root,
        )
        .await
        .expect("terminal cleanup operation lock should acquire");
        crate::db::repository::folder_tree_operation_repo::stage_ids(
            state.writer_db(),
            task.id,
            EntityType::Folder,
            &[root.id],
        )
        .await
        .expect("terminal cleanup staging should insert");
        assert!(
            background_task_repo::mark_failed(
                state.writer_db(),
                background_task_repo::TaskFailureUpdate {
                    id: task.id,
                    processing_token: task.processing_token,
                    attempt_count: 1,
                    last_error: "terminal fixture",
                    finished_at: now,
                    expires_at: now + chrono::Duration::hours(1),
                    steps_json: None,
                    failure_can_retry: false,
                },
            )
            .await
            .expect("terminal cleanup task should mark failed")
        );

        let removed = crate::services::task::cleanup_tasks_for_admin(
            &state,
            crate::services::task::AdminTaskCleanupFilters {
                finished_before: now + chrono::Duration::seconds(1),
                kind: Some(BackgroundTaskKind::FolderTreeMutation),
                status: Some(BackgroundTaskStatus::Failed),
            },
        )
        .await
        .expect("admin terminal cleanup should succeed");
        assert_eq!(removed, 1);
        assert!(matches!(
            background_task_repo::find_by_id(state.writer_db(), task.id).await,
            Err(crate::errors::AsterError::RecordNotFound(_))
        ));
        assert_eq!(
            crate::db::repository::folder_tree_operation_repo::count(state.writer_db(), task.id)
                .await
                .expect("terminal staging count should load"),
            0
        );
        assert!(
            crate::db::repository::lock_repo::find_all_by_entity(
                state.writer_db(),
                EntityType::Folder,
                root.id,
            )
            .await
            .expect("terminal operation locks should load")
            .is_empty()
        );
    }

    #[tokio::test]
    async fn malformed_folder_tree_payload_does_not_block_admin_terminal_cleanup() {
        let state = build_test_state().await;
        let now = Utc::now();
        let (first_user, first_root) =
            insert_test_user_and_root(&state, "cleanup-corrupt", None).await;
        let (second_user, second_root) =
            insert_test_user_and_root(&state, "cleanup-valid", None).await;
        let first = super::create_folder_tree_mutation_task_in_scope(
            &state,
            WorkspaceStorageScope::Personal {
                user_id: first_user.id,
            },
            first_root.id,
            FolderTreeMutationOperation::Delete,
        )
        .await
        .expect("corrupt cleanup fixture task should create");
        let second = super::create_folder_tree_mutation_task_in_scope(
            &state,
            WorkspaceStorageScope::Personal {
                user_id: second_user.id,
            },
            second_root.id,
            FolderTreeMutationOperation::Delete,
        )
        .await
        .expect("valid cleanup fixture task should create");

        for (task_id, corrupt_payload) in [(first.id, true), (second.id, false)] {
            let task = background_task_repo::find_by_id(state.writer_db(), task_id)
                .await
                .expect("terminal cleanup fixture task should load");
            let mut active: background_task::ActiveModel = task.into();
            active.status = Set(BackgroundTaskStatus::Failed);
            active.finished_at = Set(Some(now));
            active.expires_at = Set(now + chrono::Duration::hours(1));
            active.updated_at = Set(now);
            if corrupt_payload {
                active.payload_json = Set(StoredTaskPayload("{".to_string()));
            }
            active
                .update(state.writer_db())
                .await
                .expect("terminal cleanup fixture task should update");
        }
        crate::db::repository::folder_tree_operation_repo::stage_ids(
            state.writer_db(),
            first.id,
            EntityType::Folder,
            &[first_root.id],
        )
        .await
        .expect("corrupt task staging should insert");

        let removed = crate::services::task::cleanup_tasks_for_admin(
            &state,
            crate::services::task::AdminTaskCleanupFilters {
                finished_before: now + chrono::Duration::seconds(1),
                kind: Some(BackgroundTaskKind::FolderTreeMutation),
                status: Some(BackgroundTaskStatus::Failed),
            },
        )
        .await
        .expect("one malformed payload should not abort terminal cleanup");

        assert_eq!(removed, 2);
        assert_eq!(
            background_task::Entity::find()
                .filter(background_task::Column::Id.is_in([first.id, second.id]))
                .count(state.writer_db())
                .await
                .expect("cleaned task count should load"),
            0
        );
        assert_eq!(
            crate::db::repository::folder_tree_operation_repo::count(state.writer_db(), first.id)
                .await
                .expect("corrupt task staging count should load"),
            0
        );
    }
}

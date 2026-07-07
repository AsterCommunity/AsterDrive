//! 回收站后台任务。

use crate::entities::background_task;
use crate::errors::Result;
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::workspace::storage::{self, WorkspaceStorageScope};

use super::spec::{self, TrashPurgeAllTask, decode_payload_as};
use super::steps::{
    TASK_STEP_PURGE_TRASH, TASK_STEP_WAITING, parse_task_steps_json, set_task_step_active,
    set_task_step_succeeded,
};
use super::types::{TaskInfo, TrashPurgeAllTaskPayload, TrashPurgeAllTaskResult};
use super::{
    TaskExecutionContext, create_typed_task_record, mark_task_progress, mark_task_succeeded,
    task_scope,
};

pub(crate) async fn create_trash_purge_all_task_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
) -> Result<TaskInfo> {
    storage::require_scope_access_with_db(state, state.writer_db(), scope)
        .await?;
    let payload = TrashPurgeAllTaskPayload {};
    let task = create_typed_task_record::<TrashPurgeAllTask>(state, scope, "Empty trash", &payload)
        .await?;
    super::get_task_in_scope(state, scope, task.id).await
}

pub(super) async fn process_trash_purge_all_task(
    state: &PrimaryAppState,
    task: &background_task::Model,
    context: TaskExecutionContext,
) -> Result<()> {
    let lease_guard = context.lease_guard().clone();
    let scope = task_scope(task)?;
    let _payload = decode_payload_as::<TrashPurgeAllTask>(task)?;
    let mut steps =
        parse_task_steps_json(task.steps_json.as_ref().map(|raw| raw.as_ref()), task.kind)?;
    set_task_step_succeeded(
        &mut steps,
        TASK_STEP_WAITING,
        Some("Worker claimed task"),
        None,
    )?;
    context.ensure_active()?;
    set_task_step_active(
        &mut steps,
        TASK_STEP_PURGE_TRASH,
        Some("Purging trash contents"),
        None,
    )?;
    mark_task_progress(state, &lease_guard, 0, 0, Some("Purging trash"), &steps).await?;

    let purge_summary =
        crate::services::files::trash::purge_all_in_scope_silent(state, scope).await?;
    let purged = purge_summary.purged;
    let progress = i64::from(purged);
    set_task_step_succeeded(
        &mut steps,
        TASK_STEP_PURGE_TRASH,
        Some("Trash purged"),
        Some((progress, progress)),
    )?;
    let result_json =
        spec::serialize_result::<TrashPurgeAllTask>(&TrashPurgeAllTaskResult { purged })?;
    mark_task_succeeded(
        state,
        &lease_guard,
        Some(&result_json),
        progress,
        progress,
        Some("Trash purged"),
        &steps,
    )
    .await?;
    crate::services::files::trash::publish_purge_all_storage_change(state, scope, &purge_summary);
    Ok(())
}

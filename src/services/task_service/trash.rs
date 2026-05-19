//! 回收站后台任务。

use crate::entities::background_task;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::workspace_storage_service::{self, WorkspaceStorageScope};
use crate::types::BackgroundTaskKind;

use super::steps::{
    TASK_STEP_PURGE_TRASH, TASK_STEP_WAITING, parse_task_steps_json, set_task_step_active,
    set_task_step_succeeded,
};
use super::types::{
    TaskInfo, TrashPurgeAllTaskPayload, TrashPurgeAllTaskResult, parse_task_payload,
    serialize_task_result,
};
use super::{
    TaskLeaseGuard, create_task_record, mark_task_progress, mark_task_succeeded, task_scope,
};

pub(crate) async fn create_trash_purge_all_task_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
) -> Result<TaskInfo> {
    workspace_storage_service::require_scope_access(state, scope).await?;
    let payload = TrashPurgeAllTaskPayload {};
    let task = create_task_record(
        state,
        scope,
        BackgroundTaskKind::TrashPurgeAll,
        "Empty trash",
        &payload,
    )
    .await?;
    super::get_task_in_scope(state, scope, task.id).await
}

pub(super) async fn process_trash_purge_all_task(
    state: &PrimaryAppState,
    task: &background_task::Model,
    lease_guard: TaskLeaseGuard,
) -> Result<()> {
    let scope = task_scope(task)?;
    let _: TrashPurgeAllTaskPayload = parse_task_payload(task)?;
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
        TASK_STEP_PURGE_TRASH,
        Some("Purging trash contents"),
        None,
    )?;
    mark_task_progress(state, &lease_guard, 0, 0, Some("Purging trash"), &steps).await?;

    let purge_summary =
        crate::services::trash_service::purge_all_in_scope_silent(state, scope).await?;
    let purged = purge_summary.purged;
    let progress = i64::from(purged);
    set_task_step_succeeded(
        &mut steps,
        TASK_STEP_PURGE_TRASH,
        Some("Trash purged"),
        Some((progress, progress)),
    )?;
    let result_json = serialize_task_result(&TrashPurgeAllTaskResult { purged })?;
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
    crate::services::trash_service::publish_purge_all_storage_change(state, scope, &purge_summary);
    Ok(())
}

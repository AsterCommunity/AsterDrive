//! 后台任务服务子模块：`steps`。

use chrono::Utc;

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::types::{BackgroundTaskKind, StoredTaskSteps};

use super::types::{TaskStepInfo, TaskStepStatus};

pub(super) const TASK_STEP_WAITING: &str = "waiting";
pub(super) const TASK_STEP_PREPARE_SOURCES: &str = "prepare_sources";
pub(super) const TASK_STEP_BUILD_ARCHIVE: &str = "build_archive";
pub(super) const TASK_STEP_STORE_RESULT: &str = "store_result";
pub(super) const TASK_STEP_VALIDATE_SOURCE: &str = "validate_source";
pub(super) const TASK_STEP_DOWNLOAD_SOURCE: &str = "download_source";
pub(super) const TASK_STEP_VERIFY_SOURCE: &str = "verify_source";
pub(super) const TASK_STEP_EXTRACT_ARCHIVE: &str = "extract_archive";
pub(super) const TASK_STEP_IMPORT_RESULT: &str = "import_result";
pub(super) const TASK_STEP_SCAN_ARCHIVE: &str = "scan_archive";
pub(super) const TASK_STEP_PERSIST_MANIFEST: &str = "persist_manifest";
pub(super) const TASK_STEP_INSPECT_SOURCE: &str = "inspect_source";
pub(super) const TASK_STEP_RENDER_THUMBNAIL: &str = "render_thumbnail";
pub(super) const TASK_STEP_PERSIST_THUMBNAIL: &str = "persist_thumbnail";
pub(super) const TASK_STEP_EXTRACT_METADATA: &str = "extract_metadata";
pub(super) const TASK_STEP_PERSIST_METADATA: &str = "persist_metadata";
pub(super) const TASK_STEP_CLEANUP_OBJECTS: &str = "cleanup_objects";
pub(super) const TASK_STEP_PURGE_TRASH: &str = "purge_trash";
pub(super) const TASK_STEP_SCAN_BLOBS: &str = "scan_blobs";
pub(super) const TASK_STEP_MIGRATE_BLOBS: &str = "migrate_blobs";
pub(super) const TASK_STEP_CHECK_BLOBS: &str = "check_blobs";
pub(super) const TASK_STEP_RECONCILE_REFS: &str = "reconcile_refs";
pub(super) const TASK_STEP_FINISH: &str = "finish";

#[derive(Debug, Clone, Copy)]
pub(super) struct TaskStepSpec {
    pub(super) key: &'static str,
    pub(super) title: &'static str,
}

fn new_task_step(spec: TaskStepSpec, status: TaskStepStatus, detail: Option<&str>) -> TaskStepInfo {
    let now = (status == TaskStepStatus::Active).then(Utc::now);
    TaskStepInfo {
        key: spec.key.to_string(),
        title: spec.title.to_string(),
        status,
        progress_current: 0,
        progress_total: 0,
        detail: detail.map(str::to_string),
        started_at: now,
        finished_at: None,
    }
}

pub(super) fn initial_task_steps_from_specs(specs: &[TaskStepSpec]) -> Vec<TaskStepInfo> {
    let mut steps = Vec::with_capacity(specs.len());
    for (index, spec) in specs.iter().enumerate() {
        steps.push(new_task_step(
            *spec,
            if index == 0 {
                TaskStepStatus::Active
            } else {
                TaskStepStatus::Pending
            },
            if index == 0 {
                Some("Waiting for worker")
            } else {
                None
            },
        ));
    }
    steps
}

pub(super) fn parse_task_steps_json(
    steps_json: Option<&str>,
    _kind: BackgroundTaskKind,
) -> Result<Vec<TaskStepInfo>> {
    match steps_json {
        Some(raw) if !raw.trim().is_empty() => serde_json::from_str(raw)
            .map_aster_err_ctx("parse task steps json", AsterError::internal_error),
        _ => Ok(Vec::new()),
    }
}

pub(super) fn serialize_task_steps(steps: &[TaskStepInfo]) -> Result<StoredTaskSteps> {
    serde_json::to_string(steps)
        .map(StoredTaskSteps)
        .map_aster_err_ctx("serialize task steps", AsterError::internal_error)
}

fn find_task_step_mut<'a>(
    steps: &'a mut [TaskStepInfo],
    key: &str,
) -> Result<&'a mut TaskStepInfo> {
    steps
        .iter_mut()
        .find(|step| step.key == key)
        .ok_or_else(|| AsterError::internal_error(format!("task step '{key}' not found")))
}

pub(super) fn set_task_step_active(
    steps: &mut [TaskStepInfo],
    key: &str,
    detail: Option<&str>,
    progress: Option<(i64, i64)>,
) -> Result<()> {
    let now = Utc::now();
    let step = find_task_step_mut(steps, key)?;
    step.status = TaskStepStatus::Active;
    if step.started_at.is_none() {
        step.started_at = Some(now);
    }
    step.finished_at = None;
    step.detail = detail.map(str::to_string);
    if let Some((current, total)) = progress {
        step.progress_current = current;
        step.progress_total = total;
    }
    Ok(())
}

pub(super) fn set_task_step_succeeded(
    steps: &mut [TaskStepInfo],
    key: &str,
    detail: Option<&str>,
    progress: Option<(i64, i64)>,
) -> Result<()> {
    let now = Utc::now();
    let step = find_task_step_mut(steps, key)?;
    step.status = TaskStepStatus::Succeeded;
    if step.started_at.is_none() {
        step.started_at = Some(now);
    }
    step.finished_at = Some(now);
    step.detail = detail.map(str::to_string);
    if let Some((current, total)) = progress {
        step.progress_current = current;
        step.progress_total = total;
    } else if step.progress_total > 0 {
        step.progress_current = step.progress_total;
    }
    Ok(())
}

pub(super) fn set_task_step_skipped(
    steps: &mut [TaskStepInfo],
    key: &str,
    detail: Option<&str>,
) -> Result<()> {
    let now = Utc::now();
    let step = find_task_step_mut(steps, key)?;
    step.status = TaskStepStatus::Skipped;
    if step.started_at.is_none() {
        step.started_at = Some(now);
    }
    step.finished_at = Some(now);
    step.detail = detail.map(str::to_string);
    Ok(())
}

pub(super) fn mark_active_step_failed(steps: &mut [TaskStepInfo], detail: Option<&str>) {
    let now = Utc::now();
    if let Some(step) = steps
        .iter_mut()
        .find(|step| step.status == TaskStepStatus::Active)
    {
        step.status = TaskStepStatus::Failed;
        if step.started_at.is_none() {
            step.started_at = Some(now);
        }
        step.finished_at = Some(now);
        step.detail = detail.map(str::to_string);
        return;
    }
    if let Some(step) = steps
        .iter_mut()
        .rev()
        .find(|step| step.status == TaskStepStatus::Pending)
    {
        step.status = TaskStepStatus::Failed;
        step.started_at = Some(now);
        step.finished_at = Some(now);
        step.detail = detail.map(str::to_string);
    }
}

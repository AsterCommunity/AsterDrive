//! 后台任务服务子模块：`steps`。

use chrono::Utc;

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::types::{BackgroundTaskKind, StoredTaskSteps};

use super::types::{TaskStepInfo, TaskStepStatus};

pub(super) const TASK_STEP_WAITING: &str = "waiting";
pub(super) const TASK_STEP_PREPARE_SOURCES: &str = "prepare_sources";
pub(super) const TASK_STEP_BUILD_ARCHIVE: &str = "build_archive";
pub(super) const TASK_STEP_STORE_RESULT: &str = "store_result";
pub(super) const TASK_STEP_DOWNLOAD_SOURCE: &str = "download_source";
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
struct TaskStepSpec {
    key: &'static str,
    title: &'static str,
}

fn task_step_specs(kind: BackgroundTaskKind) -> &'static [TaskStepSpec] {
    match kind {
        BackgroundTaskKind::ArchiveCompress => &[
            TaskStepSpec {
                key: TASK_STEP_WAITING,
                title: "Waiting",
            },
            TaskStepSpec {
                key: TASK_STEP_PREPARE_SOURCES,
                title: "Prepare sources",
            },
            TaskStepSpec {
                key: TASK_STEP_BUILD_ARCHIVE,
                title: "Build archive",
            },
            TaskStepSpec {
                key: TASK_STEP_STORE_RESULT,
                title: "Save archive",
            },
        ],
        BackgroundTaskKind::ArchiveExtract => &[
            TaskStepSpec {
                key: TASK_STEP_WAITING,
                title: "Waiting",
            },
            TaskStepSpec {
                key: TASK_STEP_DOWNLOAD_SOURCE,
                title: "Download source archive",
            },
            TaskStepSpec {
                key: TASK_STEP_EXTRACT_ARCHIVE,
                title: "Extract archive",
            },
            TaskStepSpec {
                key: TASK_STEP_IMPORT_RESULT,
                title: "Import extracted files",
            },
        ],
        BackgroundTaskKind::ArchivePreviewGenerate => &[
            TaskStepSpec {
                key: TASK_STEP_WAITING,
                title: "Waiting",
            },
            TaskStepSpec {
                key: TASK_STEP_DOWNLOAD_SOURCE,
                title: "Download source archive",
            },
            TaskStepSpec {
                key: TASK_STEP_SCAN_ARCHIVE,
                title: "Scan archive manifest",
            },
            TaskStepSpec {
                key: TASK_STEP_PERSIST_MANIFEST,
                title: "Persist manifest",
            },
        ],
        BackgroundTaskKind::ThumbnailGenerate => &[
            TaskStepSpec {
                key: TASK_STEP_WAITING,
                title: "Waiting",
            },
            TaskStepSpec {
                key: TASK_STEP_INSPECT_SOURCE,
                title: "Inspect source blob",
            },
            TaskStepSpec {
                key: TASK_STEP_RENDER_THUMBNAIL,
                title: "Render thumbnail",
            },
            TaskStepSpec {
                key: TASK_STEP_PERSIST_THUMBNAIL,
                title: "Persist thumbnail",
            },
        ],
        BackgroundTaskKind::MediaMetadataExtract => &[
            TaskStepSpec {
                key: TASK_STEP_WAITING,
                title: "Waiting",
            },
            TaskStepSpec {
                key: TASK_STEP_INSPECT_SOURCE,
                title: "Inspect source blob",
            },
            TaskStepSpec {
                key: TASK_STEP_EXTRACT_METADATA,
                title: "Extract metadata",
            },
            TaskStepSpec {
                key: TASK_STEP_PERSIST_METADATA,
                title: "Persist metadata",
            },
        ],
        BackgroundTaskKind::TrashPurgeAll => &[
            TaskStepSpec {
                key: TASK_STEP_WAITING,
                title: "Waiting",
            },
            TaskStepSpec {
                key: TASK_STEP_PURGE_TRASH,
                title: "Purge trash",
            },
        ],
        BackgroundTaskKind::StoragePolicyTempCleanup => &[
            TaskStepSpec {
                key: TASK_STEP_WAITING,
                title: "Waiting",
            },
            TaskStepSpec {
                key: TASK_STEP_PREPARE_SOURCES,
                title: "Prepare storage driver",
            },
            TaskStepSpec {
                key: TASK_STEP_CLEANUP_OBJECTS,
                title: "Clean temporary objects",
            },
        ],
        BackgroundTaskKind::StoragePolicyMigration => &[
            TaskStepSpec {
                key: TASK_STEP_WAITING,
                title: "Waiting",
            },
            TaskStepSpec {
                key: TASK_STEP_PREPARE_SOURCES,
                title: "Prepare storage policies",
            },
            TaskStepSpec {
                key: TASK_STEP_SCAN_BLOBS,
                title: "Scan source blobs",
            },
            TaskStepSpec {
                key: TASK_STEP_MIGRATE_BLOBS,
                title: "Migrate blobs",
            },
            TaskStepSpec {
                key: TASK_STEP_FINISH,
                title: "Finish migration",
            },
        ],
        BackgroundTaskKind::BlobMaintenance => &[
            TaskStepSpec {
                key: TASK_STEP_WAITING,
                title: "Waiting",
            },
            TaskStepSpec {
                key: TASK_STEP_SCAN_BLOBS,
                title: "Load blob records",
            },
            TaskStepSpec {
                key: TASK_STEP_CHECK_BLOBS,
                title: "Check storage objects",
            },
            TaskStepSpec {
                key: TASK_STEP_RECONCILE_REFS,
                title: "Reconcile references",
            },
            TaskStepSpec {
                key: TASK_STEP_CLEANUP_OBJECTS,
                title: "Clean orphan blobs",
            },
            TaskStepSpec {
                key: TASK_STEP_FINISH,
                title: "Finish maintenance",
            },
        ],
        BackgroundTaskKind::SystemRuntime => &[],
    }
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

pub(super) fn initial_task_steps(kind: BackgroundTaskKind) -> Vec<TaskStepInfo> {
    let mut steps = Vec::with_capacity(task_step_specs(kind).len());
    for (index, spec) in task_step_specs(kind).iter().enumerate() {
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

//! Background task spec registry.
//!
//! 这个文件只做一件事：把 `BackgroundTaskKind` 映射到对应的 typed spec，
//! 然后把所有需要跨层复用的行为统一从 spec 往外转发。
//!
//! 也就是说，dispatch、presentation、payload/result 解码、初始 steps、lane、
//! max attempts 和 retry class 都不应该在上层各自重复写一份 kind match。
//! 如果要给后台任务增加一种新 kind，先改 `spec.rs`，再在这里注册即可。

use crate::entities::background_task;
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::types::{BackgroundTaskKind, BackgroundTaskStatus};

use super::retry::TaskRetryClass;
use super::spec::{
    ArchiveCompressTask, ArchiveExtractTask, ArchivePreviewGenerateTask, BlobMaintenanceTask,
    ErasedBackgroundTaskSpec, MediaMetadataExtractTask, StoragePolicyMigrationTask,
    StoragePolicyTempCleanupTask, SystemRuntimeTask, TaskProcessFuture, TaskSpecAdapter,
    ThumbnailGenerateTask, TrashPurgeAllTask,
};
use super::steps::initial_task_steps_from_specs;
use super::types::{TaskPayload, TaskPresentation, TaskResult, TaskStepInfo};
use super::{TaskLeaseGuard, dispatch::TaskLane};

static ARCHIVE_COMPRESS: TaskSpecAdapter<ArchiveCompressTask> = TaskSpecAdapter::new();
static ARCHIVE_EXTRACT: TaskSpecAdapter<ArchiveExtractTask> = TaskSpecAdapter::new();
static ARCHIVE_PREVIEW_GENERATE: TaskSpecAdapter<ArchivePreviewGenerateTask> =
    TaskSpecAdapter::new();
static THUMBNAIL_GENERATE: TaskSpecAdapter<ThumbnailGenerateTask> = TaskSpecAdapter::new();
static MEDIA_METADATA_EXTRACT: TaskSpecAdapter<MediaMetadataExtractTask> = TaskSpecAdapter::new();
static TRASH_PURGE_ALL: TaskSpecAdapter<TrashPurgeAllTask> = TaskSpecAdapter::new();
static STORAGE_POLICY_TEMP_CLEANUP: TaskSpecAdapter<StoragePolicyTempCleanupTask> =
    TaskSpecAdapter::new();
static STORAGE_POLICY_MIGRATION: TaskSpecAdapter<StoragePolicyMigrationTask> =
    TaskSpecAdapter::new();
static BLOB_MAINTENANCE: TaskSpecAdapter<BlobMaintenanceTask> = TaskSpecAdapter::new();
static SYSTEM_RUNTIME: TaskSpecAdapter<SystemRuntimeTask> = TaskSpecAdapter::new();

pub(super) fn spec_for_kind(kind: BackgroundTaskKind) -> &'static dyn ErasedBackgroundTaskSpec {
    match kind {
        BackgroundTaskKind::ArchiveCompress => &ARCHIVE_COMPRESS,
        BackgroundTaskKind::ArchiveExtract => &ARCHIVE_EXTRACT,
        BackgroundTaskKind::ArchivePreviewGenerate => &ARCHIVE_PREVIEW_GENERATE,
        BackgroundTaskKind::ThumbnailGenerate => &THUMBNAIL_GENERATE,
        BackgroundTaskKind::MediaMetadataExtract => &MEDIA_METADATA_EXTRACT,
        BackgroundTaskKind::TrashPurgeAll => &TRASH_PURGE_ALL,
        BackgroundTaskKind::StoragePolicyTempCleanup => &STORAGE_POLICY_TEMP_CLEANUP,
        BackgroundTaskKind::StoragePolicyMigration => &STORAGE_POLICY_MIGRATION,
        BackgroundTaskKind::BlobMaintenance => &BLOB_MAINTENANCE,
        BackgroundTaskKind::SystemRuntime => &SYSTEM_RUNTIME,
    }
}

pub(super) fn decode_task_payload(task: &background_task::Model) -> Result<TaskPayload> {
    spec_for_kind(task.kind).decode_payload(task)
}

pub(super) fn decode_task_result(task: &background_task::Model) -> Result<Option<TaskResult>> {
    spec_for_kind(task.kind).decode_result(task)
}

pub(super) fn build_task_presentation(
    kind: BackgroundTaskKind,
    payload: &TaskPayload,
    result: Option<&TaskResult>,
    status: BackgroundTaskStatus,
) -> Result<Option<TaskPresentation>> {
    spec_for_kind(kind).presentation(payload, result, status)
}

pub(super) fn task_retry_class(kind: BackgroundTaskKind, error: &AsterError) -> TaskRetryClass {
    spec_for_kind(kind).retry_class(error)
}

pub(super) fn process_task<'a>(
    state: &'a PrimaryAppState,
    task: &'a background_task::Model,
    lease_guard: TaskLeaseGuard,
) -> TaskProcessFuture<'a> {
    spec_for_kind(task.kind).process(state, task, lease_guard)
}

pub(super) fn initial_task_steps(kind: BackgroundTaskKind) -> Vec<TaskStepInfo> {
    initial_task_steps_from_specs(spec_for_kind(kind).step_specs())
}

pub(super) fn max_attempts(state: &PrimaryAppState, kind: BackgroundTaskKind) -> i32 {
    spec_for_kind(kind).max_attempts(state)
}

pub(in crate::services::task_service) fn task_lane(kind: BackgroundTaskKind) -> TaskLane {
    spec_for_kind(kind).lane()
}

pub(in crate::services::task_service) fn task_lane_kinds(
    lane: TaskLane,
) -> &'static [BackgroundTaskKind] {
    match lane {
        TaskLane::Archive => &[
            BackgroundTaskKind::ArchiveCompress,
            BackgroundTaskKind::ArchiveExtract,
            BackgroundTaskKind::ArchivePreviewGenerate,
        ],
        TaskLane::Thumbnail => &[
            BackgroundTaskKind::ThumbnailGenerate,
            BackgroundTaskKind::MediaMetadataExtract,
        ],
        TaskLane::StorageMigration => &[BackgroundTaskKind::StoragePolicyMigration],
        TaskLane::Fallback => &[
            BackgroundTaskKind::SystemRuntime,
            BackgroundTaskKind::StoragePolicyTempCleanup,
            BackgroundTaskKind::TrashPurgeAll,
            BackgroundTaskKind::BlobMaintenance,
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{StoredTaskPayload, StoredTaskResult};
    use chrono::{Duration, Utc};

    #[test]
    fn registry_covers_every_background_task_kind() {
        for kind in [
            BackgroundTaskKind::ArchiveCompress,
            BackgroundTaskKind::ArchiveExtract,
            BackgroundTaskKind::ArchivePreviewGenerate,
            BackgroundTaskKind::ThumbnailGenerate,
            BackgroundTaskKind::MediaMetadataExtract,
            BackgroundTaskKind::TrashPurgeAll,
            BackgroundTaskKind::StoragePolicyTempCleanup,
            BackgroundTaskKind::StoragePolicyMigration,
            BackgroundTaskKind::BlobMaintenance,
            BackgroundTaskKind::SystemRuntime,
        ] {
            let _ = spec_for_kind(kind);
        }
    }

    fn task_model(
        kind: BackgroundTaskKind,
        payload_json: serde_json::Value,
        result_json: Option<serde_json::Value>,
    ) -> background_task::Model {
        let now = Utc::now();
        background_task::Model {
            id: 9001,
            kind,
            status: BackgroundTaskStatus::Succeeded,
            creator_user_id: None,
            team_id: None,
            share_id: None,
            display_name: "registry test".to_string(),
            payload_json: StoredTaskPayload(payload_json.to_string()),
            result_json: result_json.map(|value| StoredTaskResult(value.to_string())),
            steps_json: None,
            progress_current: 1,
            progress_total: 1,
            status_text: None,
            attempt_count: 0,
            max_attempts: 1,
            next_run_at: now,
            processing_token: 0,
            processing_started_at: None,
            last_heartbeat_at: None,
            lease_expires_at: None,
            started_at: Some(now - Duration::milliseconds(1)),
            finished_at: Some(now),
            last_error: None,
            failure_can_retry: None,
            expires_at: now + Duration::hours(1),
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn decode_runtime_payload_and_absent_result_through_registry() {
        let task = task_model(
            BackgroundTaskKind::SystemRuntime,
            serde_json::json!({"task_name": "system-health-check"}),
            None,
        );

        let payload = decode_task_payload(&task).expect("runtime payload should decode");
        assert!(matches!(payload, TaskPayload::SystemRuntime(_)));
        let result = decode_task_result(&task).expect("missing result should decode");
        assert!(result.is_none());
    }

    #[test]
    fn decode_runtime_result_rejects_missing_required_duration() {
        let task = task_model(
            BackgroundTaskKind::SystemRuntime,
            serde_json::json!({"task_name": "system-health-check"}),
            Some(serde_json::json!({"summary": "legacy partial result"})),
        );

        let error = decode_task_result(&task).expect_err("duration_ms is required");
        assert!(error.message().contains("missing field `duration_ms`"));
    }

    #[test]
    fn decode_payload_uses_task_kind_not_json_shape_guessing() {
        let task = task_model(
            BackgroundTaskKind::ThumbnailGenerate,
            serde_json::json!({"task_name": "system-health-check"}),
            None,
        );

        let error = decode_task_payload(&task).expect_err("wrong payload shape should fail");
        assert!(error.message().contains("parse payload for task #9001"));
        assert!(error.message().contains("missing field `blob_id`"));
    }
}

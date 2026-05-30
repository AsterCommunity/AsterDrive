//! Strongly typed background task specifications.
//!
//! `BackgroundTaskSpec` 是后台任务的单一类型契约。这里集中声明每种 task 的：
//! payload/result 类型、数据库 kind、初始 steps、dispatcher lane、max attempts、
//! retry policy 和 process 入口。
//!
//! 业务代码不要根据 `BackgroundTaskKind` 自己猜 JSON shape，也不要单独维护一份
//! steps/lane/max-attempts 逻辑。所有这些元数据都应该从 spec 进入 `registry`，
//! 再由 task service 的创建、展示、调度和执行路径复用。
//!
//! 新增 task 的最小清单：
//! - 在 `types.rs` 定义 payload/result，并接入 `TaskPayload` / `TaskResult` enum。
//! - 在本文件新增 spec，优先使用 `define_task_spec!`，只有 payload 展示形态不同
//!   或 runtime 这种不可 dispatch 的任务才手写 impl。
//! - 在 `registry.rs::spec_for_kind` 注册 spec，并把 kind 放入对应 lane。
//! - 创建记录时走 `TypedTaskCreate` / `create_typed_task_record`，不要直接 serialize JSON。

use std::future::Future;
use std::pin::Pin;

use sea_orm::ActiveEnum;
use serde::{Serialize, de::DeserializeOwned};

use crate::entities::background_task;
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::types::{BackgroundTaskKind, BackgroundTaskStatus};

use super::TaskLeaseGuard;
use super::dispatch::TaskLane;
use super::presentation;
use super::retry::{TaskRetryClass, TaskRetryPolicy, default_retry_class};
use super::steps::{
    TASK_STEP_BUILD_ARCHIVE, TASK_STEP_CHECK_BLOBS, TASK_STEP_CLEANUP_OBJECTS,
    TASK_STEP_DOWNLOAD_SOURCE, TASK_STEP_EXTRACT_ARCHIVE, TASK_STEP_EXTRACT_METADATA,
    TASK_STEP_FINISH, TASK_STEP_IMPORT_RESULT, TASK_STEP_INSPECT_SOURCE, TASK_STEP_MIGRATE_BLOBS,
    TASK_STEP_PERSIST_MANIFEST, TASK_STEP_PERSIST_METADATA, TASK_STEP_PERSIST_THUMBNAIL,
    TASK_STEP_PREPARE_SOURCES, TASK_STEP_PURGE_TRASH, TASK_STEP_RECONCILE_REFS,
    TASK_STEP_RENDER_THUMBNAIL, TASK_STEP_SCAN_ARCHIVE, TASK_STEP_SCAN_BLOBS,
    TASK_STEP_STORE_RESULT, TASK_STEP_WAITING, TaskStepSpec,
};
use super::types::{
    ArchiveCompressTaskPayload, ArchiveCompressTaskResult, ArchiveExtractTaskPayload,
    ArchiveExtractTaskResult, ArchivePreviewTaskPayload, ArchivePreviewTaskResult,
    BlobMaintenanceTaskPayload, BlobMaintenanceTaskResult, MediaMetadataExtractTaskPayload,
    MediaMetadataExtractTaskResult, RuntimeTaskPayload, RuntimeTaskResult,
    StoragePolicyMigrationTaskPayload, StoragePolicyMigrationTaskResult,
    StoragePolicyTempCleanupTaskPayload, StoragePolicyTempCleanupTaskPayloadInfo,
    StoragePolicyTempCleanupTaskResult, TaskPayload, TaskPresentation, TaskResult,
    ThumbnailGenerateTaskPayload, ThumbnailGenerateTaskResult, TrashPurgeAllTaskPayload,
    TrashPurgeAllTaskResult,
};
use super::{
    archive, blob_maintenance, media_metadata, runtime, storage_migration, storage_policy_cleanup,
    thumbnail, trash,
};
use crate::config::operations;

pub(super) type TaskProcessFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

pub(super) trait BackgroundTaskSpec {
    type Payload: Serialize + DeserializeOwned + Clone + Send + Sync + 'static;
    type Result: Serialize + DeserializeOwned + Clone + Send + Sync + 'static;

    const KIND: BackgroundTaskKind;

    fn step_specs() -> &'static [TaskStepSpec];

    fn lane() -> TaskLane;

    fn max_attempts(state: &PrimaryAppState) -> i32 {
        operations::background_task_max_attempts(&state.runtime_config)
    }

    fn wrap_payload(payload: Self::Payload) -> TaskPayload;

    fn wrap_result(result: Self::Result) -> TaskResult;

    fn process<'a>(
        state: &'a PrimaryAppState,
        task: &'a background_task::Model,
        lease_guard: TaskLeaseGuard,
    ) -> TaskProcessFuture<'a>;

    fn retry_class(error: &AsterError) -> TaskRetryClass {
        default_retry_class(error)
    }
}

pub(super) fn serialize_payload<S: BackgroundTaskSpec>(
    payload: &S::Payload,
) -> Result<crate::types::StoredTaskPayload> {
    serde_json::to_string(payload)
        .map(crate::types::StoredTaskPayload)
        .map_err(|error| {
            AsterError::internal_error(format!(
                "serialize {} task payload: {error}",
                S::KIND.to_value()
            ))
        })
}

pub(super) fn serialize_result<S: BackgroundTaskSpec>(
    result: &S::Result,
) -> Result<crate::types::StoredTaskResult> {
    serde_json::to_string(result)
        .map(crate::types::StoredTaskResult)
        .map_err(|error| {
            AsterError::internal_error(format!(
                "serialize {} task result: {error}",
                S::KIND.to_value()
            ))
        })
}

pub(super) fn decode_payload_as<S: BackgroundTaskSpec>(
    task: &background_task::Model,
) -> Result<S::Payload> {
    if task.kind != S::KIND {
        return Err(AsterError::internal_error(format!(
            "task #{} kind mismatch: expected {}, got {}",
            task.id,
            S::KIND.to_value(),
            task.kind.to_value()
        )));
    }

    serde_json::from_str(task.payload_json.as_ref()).map_err(|error| {
        AsterError::internal_error(format!(
            "parse payload for task #{} ({}): {error}",
            task.id,
            task.kind.to_value()
        ))
    })
}

pub(super) fn decode_result_as<S: BackgroundTaskSpec>(
    task: &background_task::Model,
) -> Result<Option<S::Result>> {
    let Some(raw) = task.result_json.as_ref() else {
        return Ok(None);
    };

    if task.kind != S::KIND {
        return Err(AsterError::internal_error(format!(
            "task #{} kind mismatch: expected {}, got {}",
            task.id,
            S::KIND.to_value(),
            task.kind.to_value()
        )));
    }

    serde_json::from_str(raw.as_ref())
        .map(Some)
        .map_err(|error| {
            AsterError::internal_error(format!(
                "parse result for task #{} ({}): {error}",
                task.id,
                task.kind.to_value()
            ))
        })
}

pub(super) trait ErasedBackgroundTaskSpec: Sync {
    fn step_specs(&self) -> &'static [TaskStepSpec];

    fn lane(&self) -> TaskLane;

    fn max_attempts(&self, state: &PrimaryAppState) -> i32;

    fn decode_payload(&self, task: &background_task::Model) -> Result<TaskPayload>;

    fn decode_result(&self, task: &background_task::Model) -> Result<Option<TaskResult>>;

    fn presentation(
        &self,
        payload: &TaskPayload,
        result: Option<&TaskResult>,
        status: BackgroundTaskStatus,
    ) -> Result<Option<TaskPresentation>>;

    fn retry_class(&self, error: &AsterError) -> TaskRetryClass;

    fn process<'a>(
        &self,
        state: &'a PrimaryAppState,
        task: &'a background_task::Model,
        lease_guard: TaskLeaseGuard,
    ) -> TaskProcessFuture<'a>;
}

pub(super) struct TaskSpecAdapter<S>(std::marker::PhantomData<S>);

impl<S> TaskSpecAdapter<S> {
    pub(super) const fn new() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<S> ErasedBackgroundTaskSpec for TaskSpecAdapter<S>
where
    S: BackgroundTaskSpec + Sync,
{
    fn step_specs(&self) -> &'static [TaskStepSpec] {
        S::step_specs()
    }

    fn lane(&self) -> TaskLane {
        S::lane()
    }

    fn max_attempts(&self, state: &PrimaryAppState) -> i32 {
        S::max_attempts(state)
    }

    fn decode_payload(&self, task: &background_task::Model) -> Result<TaskPayload> {
        let payload = decode_payload_as::<S>(task)?;
        Ok(S::wrap_payload(payload))
    }

    fn decode_result(&self, task: &background_task::Model) -> Result<Option<TaskResult>> {
        let result = decode_result_as::<S>(task)?;
        Ok(result.map(S::wrap_result))
    }

    fn presentation(
        &self,
        payload: &TaskPayload,
        result: Option<&TaskResult>,
        status: BackgroundTaskStatus,
    ) -> Result<Option<TaskPresentation>> {
        Ok(presentation::build_task_presentation(
            payload, result, status,
        ))
    }

    fn retry_class(&self, error: &AsterError) -> TaskRetryClass {
        S::retry_class(error)
    }

    fn process<'a>(
        &self,
        state: &'a PrimaryAppState,
        task: &'a background_task::Model,
        lease_guard: TaskLeaseGuard,
    ) -> TaskProcessFuture<'a> {
        S::process(state, task, lease_guard)
    }
}

macro_rules! define_task_spec {
    (
        $spec:ident,
        $kind:ident,
        $payload:ty,
        $result:ty,
        $payload_variant:ident,
        $result_variant:ident,
        steps = $steps:expr,
        lane = $lane:expr,
        process = $process:path
        $(, max_attempts = $max_attempts:expr)?
        $(, retry = $retry:path)?
        $(, payload_wrap = $payload_wrap:expr)?
    ) => {
        pub(super) struct $spec;

        impl BackgroundTaskSpec for $spec {
            type Payload = $payload;
            type Result = $result;

            const KIND: BackgroundTaskKind = BackgroundTaskKind::$kind;

            fn step_specs() -> &'static [TaskStepSpec] {
                $steps
            }

            fn lane() -> TaskLane {
                $lane
            }

            fn wrap_payload(payload: Self::Payload) -> TaskPayload {
                define_task_spec!(@payload_wrap payload, $payload_variant $(, $payload_wrap)?)
            }

            fn wrap_result(result: Self::Result) -> TaskResult {
                TaskResult::$result_variant(result)
            }

            fn process<'a>(
                state: &'a PrimaryAppState,
                task: &'a background_task::Model,
                lease_guard: TaskLeaseGuard,
            ) -> TaskProcessFuture<'a> {
                Box::pin($process(state, task, lease_guard))
            }

            $(
                fn max_attempts(state: &PrimaryAppState) -> i32 {
                    let _ = state;
                    $max_attempts
                }
            )?

            $(
                fn retry_class(error: &AsterError) -> TaskRetryClass {
                    <$retry>::retry_class(error)
                }
            )?
        }
    };
    (@payload_wrap $payload:ident, $variant:ident) => {
        TaskPayload::$variant($payload)
    };
    (@payload_wrap $payload:ident, $variant:ident, $payload_wrap:expr) => {
        TaskPayload::$variant($payload_wrap($payload))
    };
}

const ARCHIVE_COMPRESS_STEPS: &[TaskStepSpec] = &[
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
];

const ARCHIVE_EXTRACT_STEPS: &[TaskStepSpec] = &[
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
];

const ARCHIVE_PREVIEW_STEPS: &[TaskStepSpec] = &[
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
];

const THUMBNAIL_STEPS: &[TaskStepSpec] = &[
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
];

const MEDIA_METADATA_STEPS: &[TaskStepSpec] = &[
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
];

const TRASH_PURGE_STEPS: &[TaskStepSpec] = &[
    TaskStepSpec {
        key: TASK_STEP_WAITING,
        title: "Waiting",
    },
    TaskStepSpec {
        key: TASK_STEP_PURGE_TRASH,
        title: "Purge trash",
    },
];

const STORAGE_POLICY_TEMP_CLEANUP_STEPS: &[TaskStepSpec] = &[
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
];

const STORAGE_POLICY_MIGRATION_STEPS: &[TaskStepSpec] = &[
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
];

const BLOB_MAINTENANCE_STEPS: &[TaskStepSpec] = &[
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
];

const NO_STEPS: &[TaskStepSpec] = &[];

define_task_spec!(
    ArchiveCompressTask,
    ArchiveCompress,
    ArchiveCompressTaskPayload,
    ArchiveCompressTaskResult,
    ArchiveCompress,
    ArchiveCompress,
    steps = ARCHIVE_COMPRESS_STEPS,
    lane = TaskLane::Archive,
    process = archive::process_archive_compress_task,
    retry = archive::ArchiveCompressRetryPolicy
);

define_task_spec!(
    ArchiveExtractTask,
    ArchiveExtract,
    ArchiveExtractTaskPayload,
    ArchiveExtractTaskResult,
    ArchiveExtract,
    ArchiveExtract,
    steps = ARCHIVE_EXTRACT_STEPS,
    lane = TaskLane::Archive,
    process = archive::process_archive_extract_task,
    retry = archive::ArchiveExtractRetryPolicy
);

define_task_spec!(
    ArchivePreviewGenerateTask,
    ArchivePreviewGenerate,
    ArchivePreviewTaskPayload,
    ArchivePreviewTaskResult,
    ArchivePreviewGenerate,
    ArchivePreviewGenerate,
    steps = ARCHIVE_PREVIEW_STEPS,
    lane = TaskLane::Archive,
    process = archive::process_archive_preview_task,
    retry = archive::ArchivePreviewRetryPolicy
);

define_task_spec!(
    ThumbnailGenerateTask,
    ThumbnailGenerate,
    ThumbnailGenerateTaskPayload,
    ThumbnailGenerateTaskResult,
    ThumbnailGenerate,
    ThumbnailGenerate,
    steps = THUMBNAIL_STEPS,
    lane = TaskLane::Thumbnail,
    process = thumbnail::process_thumbnail_generate_task,
    max_attempts = 1,
    retry = thumbnail::ThumbnailRetryPolicy
);

define_task_spec!(
    MediaMetadataExtractTask,
    MediaMetadataExtract,
    MediaMetadataExtractTaskPayload,
    MediaMetadataExtractTaskResult,
    MediaMetadataExtract,
    MediaMetadataExtract,
    steps = MEDIA_METADATA_STEPS,
    lane = TaskLane::Thumbnail,
    process = media_metadata::process_media_metadata_extract_task,
    max_attempts = 3,
    retry = media_metadata::MediaMetadataRetryPolicy
);

define_task_spec!(
    TrashPurgeAllTask,
    TrashPurgeAll,
    TrashPurgeAllTaskPayload,
    TrashPurgeAllTaskResult,
    TrashPurgeAll,
    TrashPurgeAll,
    steps = TRASH_PURGE_STEPS,
    lane = TaskLane::Fallback,
    process = trash::process_trash_purge_all_task
);

pub(super) struct StoragePolicyTempCleanupTask;

impl BackgroundTaskSpec for StoragePolicyTempCleanupTask {
    type Payload = StoragePolicyTempCleanupTaskPayload;
    type Result = StoragePolicyTempCleanupTaskResult;

    const KIND: BackgroundTaskKind = BackgroundTaskKind::StoragePolicyTempCleanup;

    fn step_specs() -> &'static [TaskStepSpec] {
        STORAGE_POLICY_TEMP_CLEANUP_STEPS
    }

    fn lane() -> TaskLane {
        TaskLane::Fallback
    }

    fn wrap_payload(payload: Self::Payload) -> TaskPayload {
        TaskPayload::StoragePolicyTempCleanup(StoragePolicyTempCleanupTaskPayloadInfo::from(
            payload,
        ))
    }

    fn wrap_result(result: Self::Result) -> TaskResult {
        TaskResult::StoragePolicyTempCleanup(result)
    }

    fn process<'a>(
        state: &'a PrimaryAppState,
        task: &'a background_task::Model,
        lease_guard: TaskLeaseGuard,
    ) -> TaskProcessFuture<'a> {
        Box::pin(
            storage_policy_cleanup::process_storage_policy_temp_cleanup_task(
                state,
                task,
                lease_guard,
            ),
        )
    }
}

define_task_spec!(
    StoragePolicyMigrationTask,
    StoragePolicyMigration,
    StoragePolicyMigrationTaskPayload,
    StoragePolicyMigrationTaskResult,
    StoragePolicyMigration,
    StoragePolicyMigration,
    steps = STORAGE_POLICY_MIGRATION_STEPS,
    lane = TaskLane::StorageMigration,
    process = storage_migration::process_storage_policy_migration_task
);

define_task_spec!(
    BlobMaintenanceTask,
    BlobMaintenance,
    BlobMaintenanceTaskPayload,
    BlobMaintenanceTaskResult,
    BlobMaintenance,
    BlobMaintenance,
    steps = BLOB_MAINTENANCE_STEPS,
    lane = TaskLane::Fallback,
    process = blob_maintenance::process_blob_maintenance_task,
    max_attempts = 1
);

pub(super) struct SystemRuntimeTask;

impl BackgroundTaskSpec for SystemRuntimeTask {
    type Payload = RuntimeTaskPayload;
    type Result = RuntimeTaskResult;

    const KIND: BackgroundTaskKind = BackgroundTaskKind::SystemRuntime;

    fn step_specs() -> &'static [TaskStepSpec] {
        NO_STEPS
    }

    fn lane() -> TaskLane {
        TaskLane::Fallback
    }

    fn max_attempts(_state: &PrimaryAppState) -> i32 {
        1
    }

    fn wrap_payload(payload: Self::Payload) -> TaskPayload {
        TaskPayload::SystemRuntime(payload)
    }

    fn wrap_result(result: Self::Result) -> TaskResult {
        TaskResult::SystemRuntime(result)
    }

    fn process<'a>(
        _state: &'a PrimaryAppState,
        task: &'a background_task::Model,
        _lease_guard: TaskLeaseGuard,
    ) -> TaskProcessFuture<'a> {
        Box::pin(async move {
            Err(AsterError::internal_error(format!(
                "system runtime task #{} should not be dispatched",
                task.id
            )))
        })
    }

    fn retry_class(error: &AsterError) -> TaskRetryClass {
        runtime::RuntimeRetryPolicy::retry_class(error)
    }
}

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

use aster_forge_tasks::TaskExecutionContext;

use crate::config::RuntimeConfig;
use crate::entities::background_task;
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::types::BackgroundTaskKind;

use super::dispatch::TaskLane;
use super::types::{TaskPayload, TaskResult};

pub(super) type TaskProcessFuture<'a> = aster_forge_tasks::TaskProcessFuture<'a, AsterError>;
pub(super) type ErasedBackgroundTaskSpec = dyn aster_forge_tasks::ErasedBackgroundTaskSpec<
        PrimaryAppState,
        background_task::Model,
        RuntimeConfig,
        TaskExecutionContext,
        BackgroundTaskKind,
        TaskLane,
        TaskPayload,
        TaskResult,
        AsterError,
    >;

pub(super) trait BackgroundTaskSpec:
    aster_forge_tasks::BackgroundTaskSpec<
        PrimaryAppState,
        background_task::Model,
        RuntimeConfig,
        TaskExecutionContext,
        AsterError,
        Kind = BackgroundTaskKind,
        Lane = TaskLane,
        PayloadEnvelope = TaskPayload,
        ResultEnvelope = TaskResult,
    >
{
}

impl<T> BackgroundTaskSpec for T where
    T: aster_forge_tasks::BackgroundTaskSpec<
            PrimaryAppState,
            background_task::Model,
            RuntimeConfig,
            TaskExecutionContext,
            AsterError,
            Kind = BackgroundTaskKind,
            Lane = TaskLane,
            PayloadEnvelope = TaskPayload,
            ResultEnvelope = TaskResult,
        >
{
}

pub(super) fn serialize_payload<S>(payload: &S::Payload) -> Result<crate::types::StoredTaskPayload>
where
    S: BackgroundTaskSpec,
{
    aster_forge_tasks::serialize_payload::<
        S,
        PrimaryAppState,
        background_task::Model,
        RuntimeConfig,
        TaskExecutionContext,
        AsterError,
    >(payload)
    .map(crate::types::StoredTaskPayload)
    .map_err(AsterError::from)
}

impl aster_forge_tasks::TaskRecord<BackgroundTaskKind> for background_task::Model {
    fn id(&self) -> i64 {
        self.id
    }

    fn kind(&self) -> BackgroundTaskKind {
        self.kind
    }

    fn payload_json(&self) -> &str {
        self.payload_json.as_ref()
    }

    fn result_json(&self) -> Option<&str> {
        self.result_json.as_ref().map(AsRef::as_ref)
    }
}

pub(super) fn serialize_result<S>(result: &S::Result) -> Result<crate::types::StoredTaskResult>
where
    S: BackgroundTaskSpec,
{
    aster_forge_tasks::serialize_result::<
        S,
        PrimaryAppState,
        background_task::Model,
        RuntimeConfig,
        TaskExecutionContext,
        AsterError,
    >(result)
    .map(crate::types::StoredTaskResult)
    .map_err(AsterError::from)
}

pub(super) fn decode_payload_as<S: BackgroundTaskSpec>(
    task: &background_task::Model,
) -> Result<S::Payload> {
    aster_forge_tasks::decode_payload_as::<
        S,
        PrimaryAppState,
        background_task::Model,
        RuntimeConfig,
        TaskExecutionContext,
        AsterError,
    >(task)
    .map_err(AsterError::from)
}

#[cfg(test)]
pub(super) fn decode_result_as<S: BackgroundTaskSpec>(
    task: &background_task::Model,
) -> Result<Option<S::Result>> {
    aster_forge_tasks::decode_result_as::<
        S,
        PrimaryAppState,
        background_task::Model,
        RuntimeConfig,
        TaskExecutionContext,
        AsterError,
    >(task)
    .map_err(AsterError::from)
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
        pub(crate) struct $spec;

        impl aster_forge_tasks::BackgroundTaskSpec<
            $crate::runtime::PrimaryAppState,
            $crate::entities::background_task::Model,
            $crate::config::RuntimeConfig,
            aster_forge_tasks::TaskExecutionContext,
            $crate::errors::AsterError,
        > for $spec {
            type Kind = $crate::types::BackgroundTaskKind;
            type Lane = $crate::services::task::dispatch::TaskLane;
            type Payload = $payload;
            type Result = $result;
            type PayloadEnvelope = $crate::services::task::types::TaskPayload;
            type ResultEnvelope = $crate::services::task::types::TaskResult;

            const KIND: $crate::types::BackgroundTaskKind =
                $crate::types::BackgroundTaskKind::$kind;

            fn step_specs() -> &'static [aster_forge_tasks::TaskStepSpec] {
                $steps
            }

            fn lane() -> $crate::services::task::dispatch::TaskLane {
                $lane
            }

            fn wrap_payload(
                payload: Self::Payload,
            ) -> $crate::services::task::types::TaskPayload {
                define_task_spec!(@payload_wrap payload, $payload_variant $(, $payload_wrap)?)
            }

            fn wrap_result(
                result: Self::Result,
            ) -> $crate::services::task::types::TaskResult {
                $crate::services::task::types::TaskResult::$result_variant(result)
            }

            fn process<'a>(
                state: &'a $crate::runtime::PrimaryAppState,
                task: &'a $crate::entities::background_task::Model,
                context: aster_forge_tasks::TaskExecutionContext,
            ) -> $crate::services::task::spec::TaskProcessFuture<'a> {
                Box::pin($process(state, task, context))
            }

            fn max_attempts(runtime_config: &$crate::config::RuntimeConfig) -> i32 {
                define_task_spec!(@max_attempts runtime_config $(, $max_attempts)?)
            }

            fn retry_class(
                error: &$crate::errors::AsterError,
            ) -> aster_forge_tasks::TaskRetryClass {
                define_task_spec!(@retry error $(, $retry)?)
            }
        }
    };
    (@payload_wrap $payload:ident, $variant:ident) => {
        $crate::services::task::types::TaskPayload::$variant($payload)
    };
    (@payload_wrap $payload:ident, $variant:ident, $payload_wrap:expr) => {
        $crate::services::task::types::TaskPayload::$variant($payload_wrap($payload))
    };
    (@max_attempts $runtime_config:ident) => {
        $crate::config::operations::background_task_max_attempts($runtime_config)
    };
    (@max_attempts $runtime_config:ident, $max_attempts:expr) => {{
        let _ = $runtime_config;
        $max_attempts
    }};
    (@retry $error:ident) => {
        $crate::services::task::retry::default_retry_class($error)
    };
    (@retry $error:ident, $retry:path) => {
        <$retry>::retry_class($error)
    };
}

pub(crate) mod archive;
pub(crate) mod maintenance;
pub(crate) mod media;
pub(crate) mod offline_download;
pub(crate) mod runtime;
pub(crate) mod storage;

pub(crate) use archive::{ArchiveCompressTask, ArchiveExtractTask, ArchivePreviewGenerateTask};
pub(crate) use maintenance::{BlobMaintenanceTask, TrashPurgeAllTask};
pub(crate) use media::{ImagePreviewGenerateTask, MediaMetadataExtractTask, ThumbnailGenerateTask};
pub(crate) use offline_download::OfflineDownloadTask;
pub(crate) use runtime::SystemRuntimeTask;
pub(crate) use storage::{StoragePolicyMigrationTask, StoragePolicyTempCleanupTask};

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

use crate::config::{RuntimeConfig, operations};
use crate::entities::background_task;
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::types::{BackgroundTaskKind, BackgroundTaskStatus};

use super::TaskExecutionContext;
use super::dispatch::TaskLane;
use super::presentation;
use super::retry::{TaskRetryClass, default_retry_class};
use super::steps::TaskStepSpec;
use super::types::{TaskPayload, TaskPresentation, TaskResult};

pub(super) type TaskProcessFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

pub(super) trait BackgroundTaskSpec {
    type Payload: Serialize + DeserializeOwned + Clone + Send + Sync + 'static;
    type Result: Serialize + DeserializeOwned + Clone + Send + Sync + 'static;

    const KIND: BackgroundTaskKind;

    fn step_specs() -> &'static [TaskStepSpec];

    fn lane() -> TaskLane;

    fn max_attempts(runtime_config: &RuntimeConfig) -> i32 {
        operations::background_task_max_attempts(runtime_config)
    }

    fn wrap_payload(payload: Self::Payload) -> TaskPayload;

    fn wrap_result(result: Self::Result) -> TaskResult;

    fn process<'a>(
        state: &'a PrimaryAppState,
        task: &'a background_task::Model,
        context: TaskExecutionContext,
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
    if task.kind != S::KIND {
        return Err(AsterError::internal_error(format!(
            "task #{} kind mismatch: expected {}, got {}",
            task.id,
            S::KIND.to_value(),
            task.kind.to_value()
        )));
    }

    let Some(raw) = task.result_json.as_ref() else {
        return Ok(None);
    };

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

    fn max_attempts(&self, runtime_config: &RuntimeConfig) -> i32;

    fn decode_payload(&self, task: &background_task::Model) -> Result<TaskPayload>;

    fn decode_result(&self, task: &background_task::Model) -> Result<Option<TaskResult>>;

    fn presentation(
        &self,
        payload: &TaskPayload,
        result: Option<&TaskResult>,
        status: BackgroundTaskStatus,
        context: presentation::TaskPresentationContext,
    ) -> Result<Option<TaskPresentation>>;

    fn retry_class(&self, error: &AsterError) -> TaskRetryClass;

    fn process<'a>(
        &self,
        state: &'a PrimaryAppState,
        task: &'a background_task::Model,
        context: TaskExecutionContext,
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

    fn max_attempts(&self, runtime_config: &RuntimeConfig) -> i32 {
        S::max_attempts(runtime_config)
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
        context: presentation::TaskPresentationContext,
    ) -> Result<Option<TaskPresentation>> {
        Ok(presentation::build_task_presentation(
            payload, result, status, context,
        ))
    }

    fn retry_class(&self, error: &AsterError) -> TaskRetryClass {
        S::retry_class(error)
    }

    fn process<'a>(
        &self,
        state: &'a PrimaryAppState,
        task: &'a background_task::Model,
        context: TaskExecutionContext,
    ) -> TaskProcessFuture<'a> {
        S::process(state, task, context)
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
        pub(crate) struct $spec;

        impl $crate::services::task::spec::BackgroundTaskSpec for $spec {
            type Payload = $payload;
            type Result = $result;

            const KIND: $crate::types::BackgroundTaskKind =
                $crate::types::BackgroundTaskKind::$kind;

            fn step_specs() -> &'static [$crate::services::task::steps::TaskStepSpec] {
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
                context: $crate::services::task::TaskExecutionContext,
            ) -> $crate::services::task::spec::TaskProcessFuture<'a> {
                Box::pin($process(state, task, context))
            }

            $(
                fn max_attempts(runtime_config: &$crate::config::RuntimeConfig) -> i32 {
                    let _ = runtime_config;
                    $max_attempts
                }
            )?

            $(
                fn retry_class(
                    error: &$crate::errors::AsterError,
                ) -> $crate::services::task::retry::TaskRetryClass {
                    <$retry>::retry_class(error)
                }
            )?
        }
    };
    (@payload_wrap $payload:ident, $variant:ident) => {
        $crate::services::task::types::TaskPayload::$variant($payload)
    };
    (@payload_wrap $payload:ident, $variant:ident, $payload_wrap:expr) => {
        $crate::services::task::types::TaskPayload::$variant($payload_wrap($payload))
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

//! 后台任务服务子模块：`runtime`。

use chrono::{DateTime, Utc};

use crate::db::repository::background_task_repo;
use crate::entities::background_task;
use crate::errors::Result;
use crate::runtime::SharedRuntimeState;
use crate::services::task_service::types::TaskPresentationCode;
use crate::types::{BackgroundTaskStatus, StoredTaskPayload};

use super::spec::{self, SystemRuntimeTask};
use super::types::{RuntimeSystemHealthResult, RuntimeTaskPayload, RuntimeTaskResult};
use super::{
    TypedTaskCreate, insert_typed_task_record, task_expiration_from, truncate_error,
    truncate_status_text,
};

pub(crate) fn system_runtime_payload_json(
    task_name: SystemRuntimeTaskKind,
) -> Result<StoredTaskPayload> {
    spec::serialize_payload::<SystemRuntimeTask>(&RuntimeTaskPayload {
        task_name: task_name.into(),
    })
}

pub(crate) async fn find_latest_system_runtime_by_task_name(
    state: &impl SharedRuntimeState,
    task_name: SystemRuntimeTaskKind,
) -> Result<Option<background_task::Model>> {
    let payload_json = system_runtime_payload_json(task_name)?;
    background_task_repo::find_latest_system_runtime_by_payload(state.reader_db(), &payload_json)
        .await
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SystemRuntimeTaskKind {
    MailOutboxDispatch,
    BackgroundTaskDispatch,
    UploadCleanup,
    CompletedUploadCleanup,
    BlobReconcile,
    SystemHealthCheck,
    RemoteNodeHealthTest,
    TrashCleanup,
    TeamArchiveCleanup,
    LockCleanup,
    AuthSessionCleanup,
    ExternalAuthFlowCleanup,
    MfaFlowCleanup,
    AuditCleanup,
    TaskCleanup,
    WopiSessionCleanup,
}

impl SystemRuntimeTaskKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MailOutboxDispatch => "mail-outbox-dispatch",
            Self::BackgroundTaskDispatch => "background-task-dispatch",
            Self::UploadCleanup => "upload-cleanup",
            Self::CompletedUploadCleanup => "completed-upload-cleanup",
            Self::BlobReconcile => "blob-reconcile",
            Self::SystemHealthCheck => "system-health-check",
            Self::RemoteNodeHealthTest => "remote-node-health-test",
            Self::TrashCleanup => "trash-cleanup",
            Self::TeamArchiveCleanup => "team-archive-cleanup",
            Self::LockCleanup => "lock-cleanup",
            Self::AuthSessionCleanup => "auth-session-cleanup",
            Self::ExternalAuthFlowCleanup => "external-auth-flow-cleanup",
            Self::MfaFlowCleanup => "mfa-flow-cleanup",
            Self::AuditCleanup => "audit-cleanup",
            Self::TaskCleanup => "task-cleanup",
            Self::WopiSessionCleanup => "wopi-session-cleanup",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::MailOutboxDispatch => "Mail outbox dispatch",
            Self::BackgroundTaskDispatch => "Background task dispatch",
            Self::UploadCleanup => "Upload cleanup",
            Self::CompletedUploadCleanup => "Completed upload cleanup",
            Self::BlobReconcile => "Blob reconcile",
            Self::SystemHealthCheck => "System health check",
            Self::RemoteNodeHealthTest => "Remote node health test",
            Self::TrashCleanup => "Trash cleanup",
            Self::TeamArchiveCleanup => "Team archive cleanup",
            Self::LockCleanup => "Lock cleanup",
            Self::AuthSessionCleanup => "Auth session cleanup",
            Self::ExternalAuthFlowCleanup => "External auth flow cleanup",
            Self::MfaFlowCleanup => "MFA flow cleanup",
            Self::AuditCleanup => "Audit log cleanup",
            Self::TaskCleanup => "Task artifact cleanup",
            Self::WopiSessionCleanup => "WOPI session cleanup",
        }
    }

    pub const fn presentation_code(self) -> TaskPresentationCode {
        match self {
            Self::MailOutboxDispatch => TaskPresentationCode::RuntimeTaskMailOutboxDispatch,
            Self::BackgroundTaskDispatch => TaskPresentationCode::RuntimeTaskBackgroundTaskDispatch,
            Self::UploadCleanup => TaskPresentationCode::RuntimeTaskUploadCleanup,
            Self::CompletedUploadCleanup => TaskPresentationCode::RuntimeTaskCompletedUploadCleanup,
            Self::BlobReconcile => TaskPresentationCode::RuntimeTaskBlobReconcile,
            Self::SystemHealthCheck => TaskPresentationCode::RuntimeTaskSystemHealthCheck,
            Self::RemoteNodeHealthTest => TaskPresentationCode::RuntimeTaskRemoteNodeHealthTest,
            Self::TrashCleanup => TaskPresentationCode::RuntimeTaskTrashCleanup,
            Self::TeamArchiveCleanup => TaskPresentationCode::RuntimeTaskTeamArchiveCleanup,
            Self::LockCleanup => TaskPresentationCode::RuntimeTaskLockCleanup,
            Self::AuthSessionCleanup => TaskPresentationCode::RuntimeTaskAuthSessionCleanup,
            Self::ExternalAuthFlowCleanup => {
                TaskPresentationCode::RuntimeTaskExternalAuthFlowCleanup
            }
            Self::MfaFlowCleanup => TaskPresentationCode::RuntimeTaskMfaFlowCleanup,
            Self::AuditCleanup => TaskPresentationCode::RuntimeTaskAuditCleanup,
            Self::TaskCleanup => TaskPresentationCode::RuntimeTaskTaskCleanup,
            Self::WopiSessionCleanup => TaskPresentationCode::RuntimeTaskWopiSessionCleanup,
        }
    }

    pub fn from_wire_value(value: &str) -> Option<Self> {
        match value {
            "mail-outbox-dispatch" => Some(Self::MailOutboxDispatch),
            "background-task-dispatch" => Some(Self::BackgroundTaskDispatch),
            "upload-cleanup" => Some(Self::UploadCleanup),
            "completed-upload-cleanup" => Some(Self::CompletedUploadCleanup),
            "blob-reconcile" => Some(Self::BlobReconcile),
            "system-health-check" => Some(Self::SystemHealthCheck),
            "remote-node-health-test" => Some(Self::RemoteNodeHealthTest),
            "trash-cleanup" => Some(Self::TrashCleanup),
            "team-archive-cleanup" => Some(Self::TeamArchiveCleanup),
            "lock-cleanup" => Some(Self::LockCleanup),
            "auth-session-cleanup" => Some(Self::AuthSessionCleanup),
            "external-auth-flow-cleanup" => Some(Self::ExternalAuthFlowCleanup),
            "mfa-flow-cleanup" => Some(Self::MfaFlowCleanup),
            "audit-cleanup" => Some(Self::AuditCleanup),
            "task-cleanup" => Some(Self::TaskCleanup),
            "wopi-session-cleanup" => Some(Self::WopiSessionCleanup),
            _ => None,
        }
    }
}

impl std::fmt::Display for SystemRuntimeTaskKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeTaskRunOutcome {
    Quiet,
    Succeeded {
        summary: Option<String>,
        system_health: Option<RuntimeSystemHealthResult>,
    },
    Failed {
        summary: Option<String>,
        error: String,
        system_health: Option<RuntimeSystemHealthResult>,
    },
}

impl RuntimeTaskRunOutcome {
    pub fn quiet() -> Self {
        Self::Quiet
    }

    pub fn succeeded(summary: Option<String>) -> Self {
        Self::Succeeded {
            summary,
            system_health: None,
        }
    }

    pub fn succeeded_with_system_health(
        summary: Option<String>,
        system_health: RuntimeSystemHealthResult,
    ) -> Self {
        Self::Succeeded {
            summary,
            system_health: Some(system_health),
        }
    }

    pub fn failed(summary: Option<String>, error: impl Into<String>) -> Self {
        Self::Failed {
            summary,
            error: error.into(),
            system_health: None,
        }
    }

    pub fn failed_with_system_health(
        summary: Option<String>,
        error: impl Into<String>,
        system_health: RuntimeSystemHealthResult,
    ) -> Self {
        Self::Failed {
            summary,
            error: error.into(),
            system_health: Some(system_health),
        }
    }

    fn should_record(&self) -> bool {
        !matches!(self, Self::Quiet)
    }

    fn status(&self) -> BackgroundTaskStatus {
        match self {
            Self::Quiet | Self::Succeeded { .. } => BackgroundTaskStatus::Succeeded,
            Self::Failed { .. } => BackgroundTaskStatus::Failed,
        }
    }

    fn summary(&self) -> Option<&str> {
        match self {
            Self::Quiet => None,
            Self::Succeeded { summary, .. } | Self::Failed { summary, .. } => summary.as_deref(),
        }
    }

    fn error(&self) -> Option<&str> {
        match self {
            Self::Failed { error, .. } => Some(error.as_str()),
            Self::Quiet | Self::Succeeded { .. } => None,
        }
    }

    fn system_health(&self) -> Option<RuntimeSystemHealthResult> {
        match self {
            Self::Succeeded { system_health, .. } | Self::Failed { system_health, .. } => {
                system_health.clone()
            }
            Self::Quiet => None,
        }
    }
}

pub async fn record_runtime_task_run(
    state: &impl SharedRuntimeState,
    task_name: SystemRuntimeTaskKind,
    started_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
    outcome: &RuntimeTaskRunOutcome,
) -> Result<Option<background_task::Model>> {
    if !outcome.should_record() {
        // Quiet 代表“这轮什么都没发生，不值得留痕”。
        // 比如 background-task-dispatch 空轮询时，不会每 5 秒往任务表里灌一行噪音数据。
        return Ok(None);
    }

    let payload = RuntimeTaskPayload {
        task_name: task_name.into(),
    };
    let payload_json = spec::serialize_payload::<SystemRuntimeTask>(&payload)?;
    let summary = outcome.summary().map(truncate_status_text);
    let last_error = outcome.error().map(truncate_error);
    let result = RuntimeTaskResult::from_timestamps(
        started_at,
        finished_at,
        summary.clone(),
        outcome.system_health(),
    );
    let result_json = spec::serialize_result::<SystemRuntimeTask>(&result)?;

    if should_refresh_latest_success(task_name, outcome)
        && let Some(existing) = background_task_repo::find_latest_system_runtime_by_payload(
            state.writer_db(),
            &payload_json,
        )
        .await?
        && existing.status == BackgroundTaskStatus::Succeeded
        && background_task_repo::refresh_system_runtime_success(
            state.writer_db(),
            background_task_repo::SystemRuntimeSuccessRefresh {
                id: existing.id,
                result_json: result_json.as_ref(),
                status_text: summary.as_deref(),
                next_run_at: finished_at,
                started_at,
                finished_at,
                expires_at: task_expiration_from(state, finished_at),
            },
        )
        .await?
    {
        return background_task_repo::find_by_id(state.writer_db(), existing.id)
            .await
            .map(Some);
    }

    // 系统周期任务和用户后台任务共用 background_task 表。
    // 区别在于 runtime 任务的 kind 是 SystemRuntime，它们只是执行事件记录，
    // 不会再被 dispatcher 拿去执行。
    let progress_current = if matches!(outcome, RuntimeTaskRunOutcome::Failed { .. }) {
        0
    } else {
        1
    };
    let mut create = TypedTaskCreate::<SystemRuntimeTask>::new(task_name.display_name(), payload)
        .status(outcome.status())
        .without_steps()
        .progress(progress_current, 1)
        .started_at(started_at)
        .finished_at(finished_at)
        .last_error(last_error)
        .failure_can_retry(if matches!(outcome, RuntimeTaskRunOutcome::Failed { .. }) {
            Some(false)
        } else {
            None
        })
        .result(&result)?;
    if let Some(summary) = summary {
        create = create.status_text(summary);
    }

    let task = insert_typed_task_record(state, state.writer_db(), create).await?;

    Ok(Some(task))
}

fn should_refresh_latest_success(
    task_name: SystemRuntimeTaskKind,
    outcome: &RuntimeTaskRunOutcome,
) -> bool {
    task_name == SystemRuntimeTaskKind::SystemHealthCheck
        && matches!(
            outcome,
            RuntimeTaskRunOutcome::Succeeded {
                system_health: Some(RuntimeSystemHealthResult {
                    status: super::types::RuntimeSystemHealthStatus::Healthy,
                    ..
                }),
                ..
            }
        )
}

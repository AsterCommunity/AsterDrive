//! 后台任务服务子模块：`runtime`。

use chrono::{DateTime, Utc};

use crate::db::repository::background_task_repo;
use crate::errors::Result;
use crate::runtime::SharedRuntimeState;
use crate::services::task::types::TaskPresentationCode;
use aster_drive_model::entities::background_task;
use aster_drive_model::types::{BackgroundTaskStatus, StoredTaskPayload};

use super::spec::{self, SystemRuntimeTask};
use super::types::{RuntimeSystemHealthResult, RuntimeTaskPayload, RuntimeTaskResult};
use super::{
    TypedTaskCreate, insert_typed_task_record, task_expiration_from, truncate_error,
    truncate_status_text,
};

/// Builds the stored payload JSON for a system runtime task of the given
/// kind.
pub(crate) fn system_runtime_payload_json(
    task_name: SystemRuntimeTaskKind,
) -> Result<StoredTaskPayload> {
    spec::serialize_payload::<SystemRuntimeTask>(&RuntimeTaskPayload {
        task_name: task_name.into(),
    })
}

/// Finds the most recent `background_task` row recorded for the given system
/// runtime task kind, or `None` when the task has never run.
pub(crate) async fn find_latest_system_runtime_by_task_name(
    state: &impl SharedRuntimeState,
    task_name: SystemRuntimeTaskKind,
) -> Result<Option<background_task::Model>> {
    let payload_json = system_runtime_payload_json(task_name)?;
    background_task_repo::find_latest_system_runtime_by_payload(state.reader_db(), &payload_json)
        .await
}

/// Identifies a Drive-owned system runtime task known to the background task
/// runtime.
///
/// Every variant is registered with a stable wire value, display name, and
/// presentation code; the scheduled definitions that execute them live in
/// [`crate::runtime::tasks`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SystemRuntimeTaskKind {
    /// Periodically claims and sends queued mail outbox messages.
    MailOutboxDispatch,
    /// Runs the background task consumer loop that claims and executes due
    /// tasks across all lanes.
    BackgroundTaskDispatch,
    /// Removes upload sessions whose expiry timestamp has passed.
    UploadCleanup,
    /// Purges completed upload sessions and their file-create idempotency
    /// records once they expire.
    CompletedUploadCleanup,
    /// Reconciles blob reference counts and removes orphaned blobs.
    BlobReconcile,
    /// Runs the Primary system health checks and records the resulting report.
    SystemHealthCheck,
    /// Probes managed remote follower nodes for health as part of the
    /// remote-nodes diagnostics component.
    RemoteNodeHealthTest,
    /// Purges trash entries that exceeded their retention window.
    TrashCleanup,
    /// Removes archived teams past their archive retention window.
    TeamArchiveCleanup,
    /// Removes file locks that reached their expiry.
    LockCleanup,
    /// Removes expired local authentication sessions.
    AuthSessionCleanup,
    /// Removes expired external authentication flows.
    ExternalAuthFlowCleanup,
    /// Removes expired multi-factor authentication flows.
    MfaFlowCleanup,
    /// Removes expired workspace activity records and audit log entries.
    AuditCleanup,
    /// Removes expired background task artifact directories from the temp
    /// roots.
    TaskCleanup,
    /// Removes expired WOPI sessions.
    WopiSessionCleanup,
}

pub(crate) type SystemRuntimeTaskDefinition =
    aster_forge_tasks::RuntimeTaskDefinition<SystemRuntimeTaskKind, TaskPresentationCode>;

aster_forge_tasks::runtime_task_registry! {
    pub(super) mod system_runtime_task_registry {
        kind: super::SystemRuntimeTaskKind;
        presentation: crate::services::task::types::TaskPresentationCode;
        tasks {
            super::SystemRuntimeTaskKind::MailOutboxDispatch => {
                wire: "mail-outbox-dispatch",
                display: "Mail outbox dispatch",
                presentation: crate::services::task::types::TaskPresentationCode::RuntimeTaskMailOutboxDispatch,
            },
            super::SystemRuntimeTaskKind::BackgroundTaskDispatch => {
                wire: "background-task-dispatch",
                display: "Background task dispatch",
                presentation: crate::services::task::types::TaskPresentationCode::RuntimeTaskBackgroundTaskDispatch,
            },
            super::SystemRuntimeTaskKind::UploadCleanup => {
                wire: "upload-cleanup",
                display: "Upload cleanup",
                presentation: crate::services::task::types::TaskPresentationCode::RuntimeTaskUploadCleanup,
            },
            super::SystemRuntimeTaskKind::CompletedUploadCleanup => {
                wire: "completed-upload-cleanup",
                display: "Completed upload cleanup",
                presentation: crate::services::task::types::TaskPresentationCode::RuntimeTaskCompletedUploadCleanup,
            },
            super::SystemRuntimeTaskKind::BlobReconcile => {
                wire: "blob-reconcile",
                display: "Blob reconcile",
                presentation: crate::services::task::types::TaskPresentationCode::RuntimeTaskBlobReconcile,
            },
            super::SystemRuntimeTaskKind::SystemHealthCheck => {
                wire: "system-health-check",
                display: "System health check",
                presentation: crate::services::task::types::TaskPresentationCode::RuntimeTaskSystemHealthCheck,
            },
            super::SystemRuntimeTaskKind::RemoteNodeHealthTest => {
                wire: "remote-node-health-test",
                display: "Remote node health test",
                presentation: crate::services::task::types::TaskPresentationCode::RuntimeTaskRemoteNodeHealthTest,
            },
            super::SystemRuntimeTaskKind::TrashCleanup => {
                wire: "trash-cleanup",
                display: "Trash cleanup",
                presentation: crate::services::task::types::TaskPresentationCode::RuntimeTaskTrashCleanup,
            },
            super::SystemRuntimeTaskKind::TeamArchiveCleanup => {
                wire: "team-archive-cleanup",
                display: "Team archive cleanup",
                presentation: crate::services::task::types::TaskPresentationCode::RuntimeTaskTeamArchiveCleanup,
            },
            super::SystemRuntimeTaskKind::LockCleanup => {
                wire: "lock-cleanup",
                display: "Lock cleanup",
                presentation: crate::services::task::types::TaskPresentationCode::RuntimeTaskLockCleanup,
            },
            super::SystemRuntimeTaskKind::AuthSessionCleanup => {
                wire: "auth-session-cleanup",
                display: "Auth session cleanup",
                presentation: crate::services::task::types::TaskPresentationCode::RuntimeTaskAuthSessionCleanup,
            },
            super::SystemRuntimeTaskKind::ExternalAuthFlowCleanup => {
                wire: "external-auth-flow-cleanup",
                display: "External auth flow cleanup",
                presentation: crate::services::task::types::TaskPresentationCode::RuntimeTaskExternalAuthFlowCleanup,
            },
            super::SystemRuntimeTaskKind::MfaFlowCleanup => {
                wire: "mfa-flow-cleanup",
                display: "MFA flow cleanup",
                presentation: crate::services::task::types::TaskPresentationCode::RuntimeTaskMfaFlowCleanup,
            },
            super::SystemRuntimeTaskKind::AuditCleanup => {
                wire: "audit-cleanup",
                display: "Audit log cleanup",
                presentation: crate::services::task::types::TaskPresentationCode::RuntimeTaskAuditCleanup,
            },
            super::SystemRuntimeTaskKind::TaskCleanup => {
                wire: "task-cleanup",
                display: "Task artifact cleanup",
                presentation: crate::services::task::types::TaskPresentationCode::RuntimeTaskTaskCleanup,
            },
            super::SystemRuntimeTaskKind::WopiSessionCleanup => {
                wire: "wopi-session-cleanup",
                display: "WOPI session cleanup",
                presentation: crate::services::task::types::TaskPresentationCode::RuntimeTaskWopiSessionCleanup,
            },
        }
    }
}

impl SystemRuntimeTaskKind {
    /// Returns the stable wire value of this task kind, as stored in task
    /// payloads and used by the runtime registry.
    pub const fn as_str(self) -> &'static str {
        system_runtime_task_registry::as_str(self)
    }

    /// Returns the human-readable display name of this task kind.
    pub const fn display_name(self) -> &'static str {
        system_runtime_task_registry::display_name(self)
    }

    /// Returns the presentation code used to render this task kind to API
    /// consumers.
    pub const fn presentation_code(self) -> TaskPresentationCode {
        system_runtime_task_registry::presentation_code(self)
    }

    /// Parses a wire value back into a task kind, returning `None` for values
    /// that no registered kind uses.
    pub fn from_wire_value(value: &str) -> Option<Self> {
        system_runtime_task_registry::from_wire_value(value)
    }
}

/// Returns every registered system runtime task definition in declaration
/// order.
pub fn registered_system_runtime_tasks() -> &'static [SystemRuntimeTaskDefinition] {
    system_runtime_task_registry::DEFINITIONS
}

impl aster_forge_tasks::RegisteredRuntimeTaskKind for SystemRuntimeTaskKind {
    fn as_str(self) -> &'static str {
        Self::as_str(self)
    }

    fn display_name(self) -> &'static str {
        Self::display_name(self)
    }

    fn from_wire_value(value: &str) -> Option<Self> {
        Self::from_wire_value(value)
    }
}

impl std::fmt::Display for SystemRuntimeTaskKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Outcome of one system runtime task execution.
///
/// The outcome decides whether a `background_task` row is written and which
/// status it receives. `Quiet` runs leave no record behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeTaskRunOutcome {
    /// The run found nothing to do and should not be recorded in the task
    /// table.
    Quiet,
    /// The run completed successfully.
    Succeeded {
        /// Optional run summary stored as the task's status text.
        summary: Option<String>,
        /// Optional system health report captured alongside the run.
        system_health: Option<RuntimeSystemHealthResult>,
    },
    /// The run failed.
    Failed {
        /// Optional run summary stored as the task's status text.
        summary: Option<String>,
        /// Description of the failure stored as the task's last error.
        error: String,
        /// Optional system health report captured alongside the run.
        system_health: Option<RuntimeSystemHealthResult>,
    },
}

impl RuntimeTaskRunOutcome {
    /// Creates a `Quiet` outcome for a run that did nothing worth recording.
    pub fn quiet() -> Self {
        Self::Quiet
    }

    /// Creates a successful outcome with an optional summary and no health
    /// report.
    pub fn succeeded(summary: Option<String>) -> Self {
        Self::Succeeded {
            summary,
            system_health: None,
        }
    }

    /// Creates a successful outcome that carries a system health report.
    pub fn succeeded_with_system_health(
        summary: Option<String>,
        system_health: RuntimeSystemHealthResult,
    ) -> Self {
        Self::Succeeded {
            summary,
            system_health: Some(system_health),
        }
    }

    /// Creates a failed outcome with an optional summary and an error
    /// description, without a health report.
    pub fn failed(summary: Option<String>, error: impl Into<String>) -> Self {
        Self::Failed {
            summary,
            error: error.into(),
            system_health: None,
        }
    }

    /// Creates a failed outcome that carries a system health report.
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

/// Records one manual runtime task run in the `background_task` table.
///
/// `Quiet` outcomes are not recorded and return `None`. Runs recorded through
/// this helper carry no dedupe key, so repeated calls always insert new rows.
pub async fn record_runtime_task_run(
    state: &impl SharedRuntimeState,
    task_name: SystemRuntimeTaskKind,
    started_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
    outcome: &RuntimeTaskRunOutcome,
) -> Result<Option<background_task::Model>> {
    record_runtime_task_run_with_dedupe_key(
        state,
        task_name,
        started_at,
        finished_at,
        outcome,
        None,
    )
    .await
}

/// Records one scheduler-driven runtime task run in the `background_task`
/// table.
///
/// The run is deduplicated per scheduler firing: `scheduled_at` derives a
/// dedupe key, so a second record for the same firing updates the existing row
/// instead of inserting a new one. `Quiet` outcomes are not recorded and
/// return `None`.
pub async fn record_scheduled_runtime_task_run(
    state: &impl SharedRuntimeState,
    task_name: SystemRuntimeTaskKind,
    scheduled_at: DateTime<Utc>,
    started_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
    outcome: &RuntimeTaskRunOutcome,
) -> Result<Option<background_task::Model>> {
    let dedupe_key = aster_forge_tasks::scheduled_task_dedupe_key(
        "aster_drive",
        task_name.as_str(),
        scheduled_at,
    )?;
    record_runtime_task_run_with_dedupe_key(
        state,
        task_name,
        started_at,
        finished_at,
        outcome,
        Some(dedupe_key),
    )
    .await
}

async fn record_runtime_task_run_with_dedupe_key(
    state: &impl SharedRuntimeState,
    task_name: SystemRuntimeTaskKind,
    started_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
    outcome: &RuntimeTaskRunOutcome,
    dedupe_key: Option<aster_forge_tasks::TaskDedupeKey>,
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
    if let Some(dedupe_key) = dedupe_key {
        create = create.dedupe_key(dedupe_key);
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

#[cfg(test)]
mod tests {
    use super::{
        RuntimeTaskRunOutcome, SystemRuntimeTaskKind, record_runtime_task_run,
        record_scheduled_runtime_task_run, registered_system_runtime_tasks,
    };
    use chrono::{Duration, Utc};
    use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};

    #[test]
    fn runtime_task_registry_has_unique_complete_metadata() {
        let definitions = registered_system_runtime_tasks();
        assert_eq!(definitions.len(), 16);

        let mut wire_values = std::collections::BTreeSet::new();
        for definition in definitions {
            assert!(!definition.wire_value.is_empty());
            assert!(!definition.display_name.is_empty());
            assert!(wire_values.insert(definition.wire_value));
            assert_eq!(definition.kind.as_str(), definition.wire_value);
            assert_eq!(definition.kind.display_name(), definition.display_name);
        }
    }

    #[tokio::test]
    async fn scheduled_runtime_record_deduplicates_same_firing() {
        let state = crate::runtime::tasks::test_support::setup_primary_state().await;
        let scheduled_at = Utc::now() - Duration::minutes(1);
        let outcome = RuntimeTaskRunOutcome::succeeded(Some("completed".to_string()));

        let first = record_scheduled_runtime_task_run(
            state.get_ref(),
            SystemRuntimeTaskKind::AuditCleanup,
            scheduled_at,
            scheduled_at + Duration::seconds(1),
            scheduled_at + Duration::seconds(2),
            &outcome,
        )
        .await
        .unwrap()
        .unwrap();
        let second = record_scheduled_runtime_task_run(
            state.get_ref(),
            SystemRuntimeTaskKind::AuditCleanup,
            scheduled_at,
            scheduled_at + Duration::seconds(3),
            scheduled_at + Duration::seconds(4),
            &outcome,
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(first.id, second.id);
        assert_eq!(first.dedupe_key, second.dedupe_key);
        assert!(first.dedupe_key.is_some());
        assert_eq!(
            aster_drive_model::entities::background_task::Entity::find()
                .filter(
                    aster_drive_model::entities::background_task::Column::DedupeKey.is_not_null()
                )
                .count(state.writer_db())
                .await
                .unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn scheduled_runtime_record_distinguishes_firings_and_manual_records() {
        let state = crate::runtime::tasks::test_support::setup_primary_state().await;
        let scheduled_at = Utc::now() - Duration::minutes(2);
        let outcome = RuntimeTaskRunOutcome::succeeded(Some("completed".to_string()));

        let first = record_scheduled_runtime_task_run(
            state.get_ref(),
            SystemRuntimeTaskKind::TaskCleanup,
            scheduled_at,
            scheduled_at,
            scheduled_at + Duration::seconds(1),
            &outcome,
        )
        .await
        .unwrap()
        .unwrap();
        let second = record_scheduled_runtime_task_run(
            state.get_ref(),
            SystemRuntimeTaskKind::TaskCleanup,
            scheduled_at + Duration::minutes(1),
            scheduled_at + Duration::minutes(1),
            scheduled_at + Duration::minutes(1) + Duration::seconds(1),
            &outcome,
        )
        .await
        .unwrap()
        .unwrap();
        let manual = record_runtime_task_run(
            state.get_ref(),
            SystemRuntimeTaskKind::TaskCleanup,
            scheduled_at + Duration::minutes(2),
            scheduled_at + Duration::minutes(2) + Duration::seconds(1),
            &outcome,
        )
        .await
        .unwrap()
        .unwrap();

        assert_ne!(first.id, second.id);
        assert_ne!(first.dedupe_key, second.dedupe_key);
        assert!(manual.dedupe_key.is_none());
    }
}

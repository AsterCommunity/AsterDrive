use std::time::Duration as StdDuration;

use chrono::Utc;
use tokio_util::sync::CancellationToken;

use super::{
    TASK_HEARTBEAT_INTERVAL_SECS, TASK_PROCESSING_STALE_SECS, task_expiration_from, truncate_error,
};
use crate::db::repository::background_task_repo;
use crate::entities::background_task;
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState, TaskRuntimeState};
use crate::services::task::{
    registry,
    steps::{parse_task_steps_json, serialize_task_steps},
};
use crate::types::{BackgroundTaskKind, BackgroundTaskStatus};
use aster_forge_tasks::{DispatchStats, TaskLease, mark_active_step_failed};

pub(super) async fn run_claimed_tasks(
    state: &PrimaryAppState,
    claimed_tasks: Vec<(background_task::Model, TaskLease)>,
    shutdown_token: CancellationToken,
) -> Result<DispatchStats> {
    aster_forge_tasks::run_claimed_task_batch_with_store(
        BackgroundTaskExecutionStore::new(state.clone()),
        claimed_tasks,
        |(task, _)| (task.created_at, task.id),
        shutdown_token,
        aster_forge_tasks::ClaimedTaskExecutionConfig {
            renewal_timeout: aster_forge_tasks::task_lease_renewal_timeout(
                TASK_PROCESSING_STALE_SECS,
                TASK_HEARTBEAT_INTERVAL_SECS,
            ),
            heartbeat_interval: StdDuration::from_secs(TASK_HEARTBEAT_INTERVAL_SECS),
            lease_expires_at: |now| {
                aster_forge_tasks::task_lease_expires_at(now, TASK_PROCESSING_STALE_SECS)
            },
            retry_delay_secs: aster_forge_tasks::default_task_retry_delay_secs,
        },
    )
    .await
}

impl aster_forge_tasks::ExecutableTaskRecord<BackgroundTaskKind> for background_task::Model {
    fn attempt_count(&self) -> i32 {
        self.attempt_count
    }

    fn max_attempts(&self) -> i32 {
        self.max_attempts
    }
}

#[derive(Clone)]
pub(super) struct BackgroundTaskExecutionStore {
    state: PrimaryAppState,
}

impl BackgroundTaskExecutionStore {
    pub(super) fn new(state: PrimaryAppState) -> Self {
        Self { state }
    }
}

#[async_trait::async_trait]
impl aster_forge_tasks::TaskHeartbeatStore for BackgroundTaskExecutionStore {
    type Error = AsterError;

    async fn touch_task_heartbeat(
        &self,
        lease: TaskLease,
        now: chrono::DateTime<Utc>,
        lease_expires_at: chrono::DateTime<Utc>,
    ) -> Result<bool> {
        background_task_repo::touch_heartbeat(
            self.state.writer_db(),
            lease.task_id,
            lease.processing_token,
            now,
            lease_expires_at,
        )
        .await
    }
}

#[async_trait::async_trait]
impl aster_forge_tasks::ClaimedTaskExecutionStore<background_task::Model, BackgroundTaskKind>
    for BackgroundTaskExecutionStore
{
    async fn process_task(
        &self,
        task: &background_task::Model,
        context: aster_forge_tasks::TaskExecutionContext,
    ) -> Result<()> {
        registry::process_task(&self.state, task, context).await
    }

    fn is_lease_lost_error(&self, error: &AsterError) -> bool {
        super::super::is_task_lease_lost(error)
    }

    fn is_lease_renewal_timed_out_error(&self, error: &AsterError) -> bool {
        super::super::is_task_lease_renewal_timed_out(error)
    }

    fn is_worker_shutdown_requested_error(&self, error: &AsterError) -> bool {
        super::super::is_task_worker_shutdown_requested(error)
    }

    fn retry_class(
        &self,
        task: &background_task::Model,
        error: &AsterError,
    ) -> aster_forge_tasks::TaskRetryClass {
        registry::task_retry_class(task.kind, error)
    }

    fn storage_error(&self, error: &AsterError) -> String {
        truncate_error(&crate::errors::encode_task_error_for_storage(error))
    }

    fn display_error(&self, storage_error: &str) -> String {
        crate::errors::task_error_display_message(storage_error).to_string()
    }

    async fn failed_steps_json(
        &self,
        task: &background_task::Model,
        display_error: &str,
    ) -> Option<String> {
        let latest = background_task_repo::find_by_id(self.state.writer_db(), task.id)
            .await
            .ok()?;
        let mut steps =
            parse_task_steps_json(latest.steps_json.as_ref().map(|raw| raw.as_ref())).ok()?;
        if steps.is_empty() {
            return None;
        }
        mark_active_step_failed(&mut steps, Some(display_error));
        serialize_task_steps(&steps).ok().map(Into::into)
    }

    async fn mark_task_failed(
        &self,
        task: &background_task::Model,
        lease: TaskLease,
        failure: aster_forge_tasks::TaskPermanentFailure<'_>,
    ) -> Result<bool> {
        background_task_repo::mark_failed(
            self.state.writer_db(),
            background_task_repo::TaskFailureUpdate {
                id: task.id,
                processing_token: lease.processing_token,
                attempt_count: failure.attempt_count,
                last_error: failure.storage_error,
                finished_at: failure.finished_at,
                expires_at: task_expiration_from(&self.state, failure.finished_at),
                steps_json: failure.failed_steps_json,
                failure_can_retry: failure.failure_can_retry,
            },
        )
        .await
    }

    async fn mark_task_retry(
        &self,
        task: &background_task::Model,
        lease: TaskLease,
        retry: aster_forge_tasks::TaskRetryUpdate<'_>,
    ) -> Result<bool> {
        background_task_repo::mark_retry(
            self.state.writer_db(),
            task.id,
            lease.processing_token,
            retry.attempt_count,
            retry.retry_at,
            retry.storage_error,
            retry.failed_steps_json,
        )
        .await
    }

    async fn release_task_for_shutdown(
        &self,
        task: &background_task::Model,
        lease: TaskLease,
    ) -> Result<bool> {
        background_task_repo::release_processing(
            self.state.writer_db(),
            task.id,
            lease.processing_token,
            Utc::now(),
            BackgroundTaskStatus::Retry,
        )
        .await
    }

    fn record_task_transition(&self, task: &background_task::Model, status: &'static str) {
        self.state
            .metrics()
            .record_background_task_transition(task.kind.as_str(), status);
    }

    fn wake_dispatcher(&self) {
        self.state.wake_background_task_dispatcher();
    }
}

//! Drive adapters for Forge's independently scalable lane consumers.

use super::{
    claim::claim_due_for_lane,
    execute::BackgroundTaskExecutionStore,
    lane::{TASK_LANES, TaskLane, TaskLaneConfig, task_lane_configs},
};
use crate::{errors::Result, runtime::PrimaryAppState};
use actix_web::web;
use aster_drive_model::entities::background_task;
use aster_forge_tasks::{
    BackgroundTasks, TaskConsumer, TaskDispatchOutcome, TaskLease, TaskRuntimeStatus,
};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct LaneConsumer {
    state: web::Data<PrimaryAppState>,
    lane: TaskLane,
}

impl LaneConsumer {
    fn config(&self) -> TaskLaneConfig {
        // Every variant has a configuration; select by enum rather than a global registry.
        task_lane_configs(self.state.get_ref())
            .into_iter()
            .find(|config| config.lane == self.lane)
            .unwrap_or(TaskLaneConfig {
                lane: self.lane,
                kinds: &[],
                limit: 0,
                cluster_limit: Some(0),
            })
    }
}

#[async_trait::async_trait]
impl TaskConsumer for LaneConsumer {
    type Task = (background_task::Model, TaskLease);
    type Error = crate::errors::AsterError;
    fn concurrency(&self) -> usize {
        self.config().limit
    }
    fn poll_interval(&self) -> Duration {
        Duration::from_secs(
            crate::config::operations::background_task_dispatch_interval_secs(
                &self.state.runtime_config,
            ),
        )
    }
    fn idle_max_interval(&self) -> Duration {
        Duration::from_secs(
            crate::config::operations::background_task_dispatch_idle_max_interval_secs(
                &self.state.runtime_config,
            ),
        )
    }
    async fn notified(&self) {
        self.state.background_task_dispatch_wakeup.notified().await;
    }
    async fn claim(&self, available: usize) -> Result<Vec<Self::Task>> {
        let result = claim_due_for_lane(self.state.get_ref(), self.config(), available).await;
        if let Ok(tasks) = &result {
            for (task, _) in tasks {
                if matches!(
                    task.status,
                    aster_drive_model::types::BackgroundTaskStatus::Processing
                        | aster_drive_model::types::BackgroundTaskStatus::Canceling
                ) {
                    self.state
                        .metrics
                        .forge_recorder()
                        .record_task_lease_takeover(task.kind.as_str());
                }
            }
        }
        super::refresh_pending_metric(self.state.get_ref()).await;
        result
    }
    async fn execute(
        &self,
        (task, lease): Self::Task,
        cancel: CancellationToken,
    ) -> Result<TaskDispatchOutcome> {
        aster_forge_tasks::process_claimed_task(
            BackgroundTaskExecutionStore::new(self.state.get_ref().clone()),
            task,
            lease,
            cancel,
            aster_forge_tasks::ClaimedTaskExecutionConfig {
                renewal_timeout: aster_forge_tasks::task_lease_renewal_timeout(
                    super::TASK_PROCESSING_STALE_SECS,
                    super::TASK_HEARTBEAT_INTERVAL_SECS,
                ),
                heartbeat_interval: Duration::from_secs(super::TASK_HEARTBEAT_INTERVAL_SECS),
                lease_expires_at: |now| {
                    aster_forge_tasks::task_lease_expires_at(now, super::TASK_PROCESSING_STALE_SECS)
                },
                retry_delay_secs: aster_forge_tasks::default_task_retry_delay_secs,
            },
        )
        .await
    }
    async fn release_unstarted(&self, (_task, lease): Self::Task) -> Result<()> {
        crate::db::repository::background_task_repo::release_processing(
            self.state.writer_db(),
            lease.task_id,
            lease.processing_token,
            chrono::Utc::now(),
            aster_drive_model::types::BackgroundTaskStatus::Retry,
        )
        .await?;
        Ok(())
    }
}

pub(crate) fn consumer_statuses() -> Vec<TaskRuntimeStatus> {
    TASK_LANES
        .iter()
        .map(|_| TaskRuntimeStatus::default())
        .collect()
}

pub(crate) fn spawn_consumers(
    state: web::Data<PrimaryAppState>,
    statuses: Vec<TaskRuntimeStatus>,
    stop: CancellationToken,
    tasks: &mut BackgroundTasks,
) {
    for (lane, status) in TASK_LANES.into_iter().zip(statuses) {
        let consumer = LaneConsumer {
            state: state.clone(),
            lane,
        };
        let stop = stop.clone();
        let config = state.config.worker.consumer_config();
        tasks.push(async move {
            let report = aster_forge_tasks::run_task_consumer(consumer, stop, config, status).await;
            tracing::info!(?lane, ?report, "task lane consumer stopped");
        });
    }
}

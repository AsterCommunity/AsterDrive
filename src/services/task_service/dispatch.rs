//! 后台任务 dispatcher。
//!
//! 这层负责从数据库认领可执行任务、按并发上限驱动执行，并在 lease 丢失时
//! 阻止旧 worker 继续把状态写回数据库。

mod claim;
mod execute;
mod lane;
mod maintenance;
#[cfg(test)]
mod tests;

use futures::stream::{self, StreamExt};

use crate::errors::Result;
use crate::runtime::PrimaryAppState;

use claim::claim_due_for_lane;
use execute::run_claimed_tasks;
use lane::{TASK_LANES, TaskLaneConfig, task_lane_configs};

use super::{
    TASK_DRAIN_MAX_ROUNDS, TASK_HEARTBEAT_INTERVAL_SECS, TASK_PROCESSING_STALE_SECS, TaskLease,
    TaskLeaseGuard, is_task_lease_lost, is_task_lease_renewal_timed_out, task_expiration_from,
    task_lease_expires_at, truncate_error,
};

pub use maintenance::{cleanup_expired, drain};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DispatchStats {
    pub claimed: usize,
    pub succeeded: usize,
    pub retried: usize,
    pub failed: usize,
}

impl DispatchStats {
    fn add(&mut self, other: Self) {
        self.claimed += other.claimed;
        self.succeeded += other.succeeded;
        self.retried += other.retried;
        self.failed += other.failed;
    }

    pub fn has_activity(&self) -> bool {
        self.claimed > 0 || self.succeeded > 0 || self.retried > 0 || self.failed > 0
    }

    pub(super) fn add_outcome(&mut self, outcome: TaskDispatchOutcome) {
        self.succeeded += outcome.succeeded;
        self.retried += outcome.retried;
        self.failed += outcome.failed;
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct TaskDispatchOutcome {
    succeeded: usize,
    retried: usize,
    failed: usize,
}

pub async fn dispatch_due(state: &PrimaryAppState) -> Result<DispatchStats> {
    let mut stats = DispatchStats::default();
    let lane_results = stream::iter(
        task_lane_configs(state)
            .into_iter()
            .map(|lane_config| dispatch_lane(state, lane_config)),
    )
    .buffer_unordered(TASK_LANES.len())
    .collect::<Vec<_>>()
    .await;
    let mut first_error = None;

    for result in lane_results {
        match result {
            Ok(lane_stats) => stats.add(lane_stats),
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    if let Some(first_error) = first_error {
        tracing::warn!(
            stats = ?stats,
            error = %first_error,
            "partial background task dispatch results due to lane error"
        );
        return Err(first_error);
    }

    Ok(stats)
}

async fn dispatch_lane(
    state: &PrimaryAppState,
    lane_config: TaskLaneConfig,
) -> Result<DispatchStats> {
    let mut total = DispatchStats::default();

    loop {
        let claimed_tasks = claim_due_for_lane(state, lane_config).await?;
        if claimed_tasks.is_empty() {
            break;
        }

        let claimed = claimed_tasks.len();
        total.claimed += claimed;
        total.add(run_claimed_tasks(state, claimed_tasks).await?);

        if !lane_config.fast_continue {
            break;
        }
    }

    Ok(total)
}

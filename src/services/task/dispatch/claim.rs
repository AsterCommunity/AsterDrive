use chrono::{Duration, Utc};
use sea_orm::ActiveEnum;

use crate::db::{
    repository::{background_task_repo, config_repo},
    transaction,
};
use crate::entities::background_task;
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;

use super::lane::{TaskLaneConfig, task_lane};
use super::{TASK_PROCESSING_STALE_SECS, TaskLease, task_lease_expires_at};

#[derive(Debug, Clone, Copy)]
pub(super) struct TaskClaimCandidate {
    pub(super) index: usize,
    pub(super) task_id: i64,
    pub(super) expected_processing_token: i64,
    pub(super) next_processing_token: i64,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ClaimedTask {
    pub(super) index: usize,
    pub(super) task_id: i64,
    pub(super) processing_token: i64,
}
pub(super) async fn claim_due_for_lane(
    state: &impl SharedRuntimeState,
    lane_config: TaskLaneConfig,
) -> Result<Vec<(background_task::Model, TaskLease)>> {
    if lane_config.limit == 0 {
        return Ok(Vec::new());
    }

    let now = Utc::now();
    let stale_before = now - Duration::seconds(TASK_PROCESSING_STALE_SECS);
    let due = background_task_repo::list_claimable_by_kinds(
        state.writer_db(),
        now,
        stale_before,
        lane_config.kinds(),
        claim_limit_to_u64(lane_config.limit),
    )
    .await?;
    if due.is_empty() {
        return Ok(Vec::new());
    }

    let active = background_task_repo::count_active_processing_by_kinds(
        state.writer_db(),
        now,
        lane_config.kinds(),
    )
    .await?;
    let available = available_lane_capacity(lane_config.limit, active);
    if available == 0 {
        tracing::debug!(
            lane = ?lane_config.lane,
            active,
            limit = lane_config.limit,
            "background task lane is at capacity; skipping claim"
        );
        return Ok(Vec::new());
    }

    let mut candidates = Vec::with_capacity(due.len());
    for (index, task) in due.iter().enumerate() {
        if task_lane(task.kind) != lane_config.lane {
            tracing::warn!(
                task_id = task.id,
                kind = %task.kind.to_value(),
                lane = ?lane_config.lane,
                "claimable task kind does not match lane config; skipping"
            );
            continue;
        }
        let next_processing_token = task.processing_token.checked_add(1).ok_or_else(|| {
            AsterError::internal_error("background task processing token overflow")
        })?;

        candidates.push(TaskClaimCandidate {
            index,
            task_id: task.id,
            expected_processing_token: task.processing_token,
            next_processing_token,
        });
    }
    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    let claimed =
        claim_candidates_for_lane(state.writer_db(), lane_config, &candidates, stale_before)
            .await?;
    let mut claimed_tasks = Vec::with_capacity(claimed.len());
    for claim in claimed {
        claimed_tasks.push((
            due[claim.index].clone(),
            TaskLease::new(claim.task_id, claim.processing_token),
        ));
    }

    Ok(claimed_tasks)
}

pub(super) async fn claim_candidates_for_lane<C>(
    db: &C,
    lane_config: TaskLaneConfig,
    candidates: &[TaskClaimCandidate],
    stale_before: chrono::DateTime<Utc>,
) -> Result<Vec<ClaimedTask>>
where
    C: sea_orm::TransactionTrait,
{
    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    transaction::with_transaction(db, async |txn| {
        let claimed_at = Utc::now();
        // 锁住 system_config 里的 lane 配置行，让同一时间只有一个 dispatcher
        // 能为同一个 lane 做容量复核和本批 CAS claim。SQLite 单连接也会自然串行化这个事务。
        config_repo::lock_by_key(txn, lane_config.lock_key()).await?;
        let active = background_task_repo::count_active_processing_by_kinds(
            txn,
            claimed_at,
            lane_config.kinds(),
        )
        .await?;
        let available = available_lane_capacity(lane_config.limit, active);
        if available == 0 {
            tracing::debug!(
                lane = ?lane_config.lane,
                active,
                limit = lane_config.limit,
                "background task lane reached capacity before batch claim"
            );
            return Ok(Vec::new());
        }

        let mut claimed = Vec::with_capacity(available.min(candidates.len()));
        for candidate in candidates {
            if claimed.len() >= available {
                break;
            }

            let did_claim = background_task_repo::try_claim(
                txn,
                candidate.task_id,
                candidate.expected_processing_token,
                claimed_at,
                stale_before,
                candidate.next_processing_token,
                task_lease_expires_at(claimed_at),
            )
            .await?;
            if !did_claim {
                continue;
            }

            claimed.push(ClaimedTask {
                index: candidate.index,
                task_id: candidate.task_id,
                processing_token: candidate.next_processing_token,
            });
        }

        Ok(claimed)
    })
    .await
}
pub(super) fn available_lane_capacity(limit: usize, active: u64) -> usize {
    let active = usize::try_from(active).unwrap_or(usize::MAX);
    limit.saturating_sub(active)
}

pub(super) fn claim_limit_to_u64(limit: usize) -> u64 {
    u64::try_from(limit).unwrap_or_else(|_| {
        tracing::warn!(
            limit,
            "background task lane limit exceeds u64; falling back to u64::MAX"
        );
        u64::MAX
    })
}

//! Atomic queue selection, shared quota authority, and token-fenced claims.
use super::{
    TASK_PROCESSING_STALE_SECS,
    lane::{TaskLane, TaskLaneConfig, lane_lock_key, task_lane},
};
use crate::{
    db::repository::{background_task_repo, config_repo},
    errors::{AsterError, Result},
    runtime::SharedRuntimeState,
};
use aster_drive_model::{entities::background_task, types::BackgroundTaskKind};
use aster_forge_db::transaction;
use aster_forge_tasks::{ClaimedTask, TaskClaimCandidate, TaskClaimRequest, TaskLease};
use chrono::{DateTime, Utc};
use sea_orm::ConnectionTrait;
use tracing::Instrument;

struct BackgroundTaskClaimStore<'a, S> {
    state: &'a S,
}

#[async_trait::async_trait]
impl<S: SharedRuntimeState + Sync>
    aster_forge_tasks::TaskClaimStore<background_task::Model, BackgroundTaskKind, TaskLane>
    for BackgroundTaskClaimStore<'_, S>
{
    type Error = AsterError;
    async fn claim(
        &self,
        request: TaskClaimRequest<BackgroundTaskKind, TaskLane>,
    ) -> Result<Vec<(background_task::Model, TaskLease)>> {
        let span = tracing::trace_span!(
            "drive_task_claim",
            operation = "background_task_claim",
            lane = ?request.lane.lane
        );
        async {
            transaction::with_transaction(self.state.writer_db(), async |txn| {
                let key = lane_lock_key(request.lane.lane);
                let started = std::time::Instant::now();
                tracing::trace!(phase = "quota_lock", "task claim phase started");
                config_repo::lock_by_key(txn, key).await?;
                tracing::trace!(
                    phase = "quota_lock",
                    elapsed_seconds = started.elapsed().as_secs_f64(),
                    "task claim phase finished"
                );
                let request = request.at(Utc::now());
                let setting = config_repo::find_by_key(txn, key).await?.ok_or_else(|| {
                    AsterError::config_error(format!("missing shared task quota {key}"))
                })?;
                let limit =
                    crate::config::operations::parse_cluster_concurrency(key, &setting.value)?;
                let available = if let Some(limit) = limit {
                    let active = background_task_repo::count_active_processing_by_kinds(
                        txn,
                        request.now,
                        request.lane.kinds,
                    )
                    .await?;
                    let remaining = aster_forge_tasks::available_lane_capacity(limit, active)
                        .min(request.max_claim);
                    tracing::trace!(
                        phase = "quota",
                        cluster_limit = limit,
                        active,
                        available = remaining,
                        "task claim quota checked"
                    );
                    remaining
                } else {
                    request.max_claim
                };
                let selection_started = std::time::Instant::now();
                let due = background_task_repo::list_claimable_by_kinds_for_update(
                    txn,
                    request.now,
                    request.stale_before,
                    request.lane.kinds,
                    aster_forge_tasks::claim_limit_to_u64(available),
                )
                .await?;
                tracing::trace!(
                    phase = "selection",
                    available,
                    candidates = due.len(),
                    elapsed_seconds = selection_started.elapsed().as_secs_f64(),
                    reason = if available == 0 {
                        "shared_quota_full"
                    } else if due.is_empty() {
                        "no_candidates"
                    } else {
                        "candidates_found"
                    },
                    "task claim candidates selected"
                );
                let candidates = aster_forge_tasks::task_claim_candidates(&due)?;
                let claimed = claim_candidates_on(
                    txn,
                    available,
                    &candidates,
                    request.stale_before,
                    request.now,
                    request.lease_expires_at,
                )
                .await?;
                Ok(claimed
                    .into_iter()
                    .map(|claim| {
                        (
                            due[claim.index].clone(),
                            TaskLease::new(claim.task_id, claim.processing_token),
                        )
                    })
                    .collect())
            })
            .await
        }
        .instrument(span)
        .await
    }
}

pub(super) async fn claim_due_for_lane(
    state: &(impl SharedRuntimeState + Sync),
    lane_config: TaskLaneConfig,
    available: usize,
) -> Result<Vec<(background_task::Model, TaskLease)>> {
    aster_forge_tasks::claim_due_for_lane(
        &BackgroundTaskClaimStore { state },
        lane_config,
        available,
        TASK_PROCESSING_STALE_SECS,
        task_lane,
    )
    .await
}

async fn claim_candidates_on<C: ConnectionTrait>(
    db: &C,
    available: usize,
    candidates: &[TaskClaimCandidate],
    stale_before: DateTime<Utc>,
    claimed_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
) -> Result<Vec<ClaimedTask>> {
    let mut claimed = Vec::with_capacity(available.min(candidates.len()));
    for candidate in candidates {
        if claimed.len() >= available {
            break;
        }
        let started = std::time::Instant::now();
        let won = background_task_repo::try_claim(
            db,
            candidate.task_id,
            candidate.expected_processing_token,
            claimed_at,
            stale_before,
            candidate.next_processing_token,
            expires_at,
        )
        .await?;
        tracing::trace!(
            phase = "cas",
            task_id = candidate.task_id,
            elapsed_seconds = started.elapsed().as_secs_f64(),
            outcome = if won { "claimed" } else { "cas_miss" },
            "task claim compare-and-swap finished"
        );
        if won {
            claimed.push(ClaimedTask {
                index: candidate.index,
                task_id: candidate.task_id,
                processing_token: candidate.next_processing_token,
            });
        }
    }
    Ok(claimed)
}

#[cfg(test)]
pub(super) async fn claim_candidates_for_lane<C: sea_orm::TransactionTrait>(
    db: &C,
    lane: TaskLaneConfig,
    max_claim: usize,
    candidates: &[TaskClaimCandidate],
    stale_before: DateTime<Utc>,
    claimed_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
) -> Result<Vec<ClaimedTask>> {
    transaction::with_transaction(db, async |txn| {
        config_repo::lock_by_key(txn, lane_lock_key(lane.lane)).await?;
        let available = if let Some(limit) = lane.cluster_limit {
            let active =
                background_task_repo::count_active_processing_by_kinds(txn, claimed_at, lane.kinds)
                    .await?;
            aster_forge_tasks::available_lane_capacity(limit, active).min(max_claim)
        } else {
            max_claim
        };
        claim_candidates_on(
            txn,
            available,
            candidates,
            stale_before,
            claimed_at,
            expires_at,
        )
        .await
    })
    .await
}

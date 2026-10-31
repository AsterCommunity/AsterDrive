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
        transaction::with_transaction(self.state.writer_db(), async |txn| {
            let key = lane_lock_key(request.lane.lane);
            config_repo::lock_by_key(txn, key).await?;
            let request = request.at(Utc::now());
            let setting = config_repo::find_by_key(txn, key).await?.ok_or_else(|| {
                AsterError::config_error(format!("missing shared task quota {key}"))
            })?;
            let limit = crate::config::operations::parse_cluster_concurrency(key, &setting.value)?;
            let available = if let Some(limit) = limit {
                let active = background_task_repo::count_active_processing_by_kinds(
                    txn,
                    request.now,
                    request.lane.kinds,
                )
                .await?;
                aster_forge_tasks::available_lane_capacity(limit, active).min(request.max_claim)
            } else {
                request.max_claim
            };
            let due = background_task_repo::list_claimable_by_kinds_for_update(
                txn,
                request.now,
                request.stale_before,
                request.lane.kinds,
                aster_forge_tasks::claim_limit_to_u64(available),
            )
            .await?;
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
        if background_task_repo::try_claim(
            db,
            candidate.task_id,
            candidate.expected_processing_token,
            claimed_at,
            stale_before,
            candidate.next_processing_token,
            expires_at,
        )
        .await?
        {
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

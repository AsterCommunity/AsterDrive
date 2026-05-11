//! 后台任务 dispatcher。
//!
//! 这层负责从数据库认领可执行任务、按并发上限驱动执行，并在 lease 丢失时
//! 阻止旧 worker 继续把状态写回数据库。

use std::future::Future;

use chrono::{Duration, Utc};
use futures::stream::{self, StreamExt};
use sea_orm::ActiveEnum;
use tokio::time::MissedTickBehavior;

use crate::config::operations;
use crate::db::{
    repository::{background_task_repo, config_repo},
    transaction,
};
use crate::entities::background_task;
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::types::{BackgroundTaskKind, BackgroundTaskStatus};

use super::archive;
use super::retry::{TaskRetryClass, TaskRetryPolicy};
use super::runtime;
use super::steps::{mark_active_step_failed, parse_task_steps_json, serialize_task_steps};
use super::thumbnail;
use super::{
    TASK_DRAIN_MAX_ROUNDS, TASK_HEARTBEAT_INTERVAL_SECS, TASK_PROCESSING_STALE_SECS, TaskLease,
    TaskLeaseGuard, is_task_lease_lost, is_task_lease_renewal_timed_out, task_expiration_from,
    task_lease_expires_at, truncate_error,
};

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

    fn add_outcome(&mut self, outcome: TaskDispatchOutcome) {
        self.succeeded += outcome.succeeded;
        self.retried += outcome.retried;
        self.failed += outcome.failed;
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct TaskDispatchOutcome {
    succeeded: usize,
    retried: usize,
    failed: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskLane {
    Archive,
    Thumbnail,
    Fallback,
}

#[derive(Debug, Clone, Copy)]
struct TaskLaneConfig {
    lane: TaskLane,
    limit: usize,
    fast_continue: bool,
}

#[derive(Debug, Clone, Copy)]
struct TaskClaimCandidate {
    index: usize,
    task_id: i64,
    expected_processing_token: i64,
    next_processing_token: i64,
}

#[derive(Debug, Clone, Copy)]
struct ClaimedTask {
    index: usize,
    task_id: i64,
    processing_token: i64,
}

const ARCHIVE_TASK_KINDS: [BackgroundTaskKind; 2] = [
    BackgroundTaskKind::ArchiveCompress,
    BackgroundTaskKind::ArchiveExtract,
];
const THUMBNAIL_TASK_KINDS: [BackgroundTaskKind; 1] = [BackgroundTaskKind::ThumbnailGenerate];
const FALLBACK_TASK_KINDS: [BackgroundTaskKind; 1] = [BackgroundTaskKind::SystemRuntime];
const TASK_LANES: [TaskLane; 3] = [TaskLane::Archive, TaskLane::Thumbnail, TaskLane::Fallback];

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

async fn run_claimed_tasks(
    state: &PrimaryAppState,
    mut claimed_tasks: Vec<(background_task::Model, TaskLease)>,
) -> Result<DispatchStats> {
    let concurrency = claimed_tasks.len().max(1);
    claimed_tasks.sort_by_key(|(task, _)| (task.created_at, task.id));

    // 先把认领结果固定下来，再启动 worker。每个 lane 的容量已经在 claim 阶段扣过，
    // 这里直接把本批已认领任务全部放出去；fast_continue lane 会在本批结束后继续补位。
    let results = run_with_concurrency_limit(claimed_tasks, concurrency, |(task, lease)| {
        let state = state.clone();
        async move { process_claimed_task(&state, task, lease).await }
    })
    .await;
    let mut stats = DispatchStats::default();
    let mut first_error = None;

    for result in results {
        match result {
            Ok(outcome) => stats.add_outcome(outcome),
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    if let Some(error) = first_error {
        return Err(error);
    }

    Ok(stats)
}

async fn claim_due_for_lane(
    state: &PrimaryAppState,
    lane_config: TaskLaneConfig,
) -> Result<Vec<(background_task::Model, TaskLease)>> {
    if lane_config.limit == 0 {
        return Ok(Vec::new());
    }

    let now = Utc::now();
    let stale_before = now - Duration::seconds(TASK_PROCESSING_STALE_SECS);
    let due = background_task_repo::list_claimable_by_kinds(
        &state.db,
        now,
        stale_before,
        lane_config.kinds(),
        claim_limit_to_u64(lane_config.limit),
    )
    .await?;
    if due.is_empty() {
        return Ok(Vec::new());
    }

    let active =
        background_task_repo::count_active_processing_by_kinds(&state.db, now, lane_config.kinds())
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
        claim_candidates_for_lane(&state.db, lane_config, &candidates, stale_before).await?;
    let mut claimed_tasks = Vec::with_capacity(claimed.len());
    for claim in claimed {
        claimed_tasks.push((
            due[claim.index].clone(),
            TaskLease::new(claim.task_id, claim.processing_token),
        ));
    }

    Ok(claimed_tasks)
}

async fn claim_candidates_for_lane<C>(
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

async fn process_claimed_task(
    state: &PrimaryAppState,
    task: background_task::Model,
    lease: TaskLease,
) -> Result<TaskDispatchOutcome> {
    let mut heartbeat =
        tokio::time::interval(std::time::Duration::from_secs(TASK_HEARTBEAT_INTERVAL_SECS));
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
    heartbeat.tick().await;
    let lease_guard = TaskLeaseGuard::new(lease);

    // 外层 select! 同时盯两件事：
    // 1. 真实业务流程是否完成；
    // 2. heartbeat 是否还能继续证明“我还是当前合法 worker”。
    //
    // 注意这里只能取消 async 外壳。真正耗时的压缩/解压是在 spawn_blocking 里，
    // 所以业务代码内部也必须周期性检查 lease guard，才能把旧 worker 真正停下来。
    let process_future = process_task(state, &task, lease_guard.clone());
    tokio::pin!(process_future);

    let task_result = loop {
        tokio::select! {
            biased;
            result = &mut process_future => break result,
            _ = heartbeat.tick() => {
                // 心跳写入返回 Err 时不能直接把 worker 判死，否则一次瞬时 DB 抖动
                // 就会在 60 秒后把长任务误判成 stale 并触发二次认领。
                match evaluate_heartbeat_result(
                    &lease_guard,
                    {
                        let now = Utc::now();
                        background_task_repo::touch_heartbeat(
                            &state.db,
                            task.id,
                            lease.processing_token,
                            now,
                            task_lease_expires_at(now),
                        )
                        .await
                    },
                ) {
                    Ok(()) => {}
                    Err(error) => break Err(error),
                }
            }
        }
    };

    match task_result {
        Ok(()) => Ok(TaskDispatchOutcome {
            succeeded: 1,
            ..Default::default()
        }),
        Err(error) => {
            // lease 丢失 / 续约超时代表“这条执行流已经过期”，不是业务失败。
            // 这时不要再把任务改成 Failed/Retry，否则旧 worker 可能覆盖新 lease 的结果。
            if is_task_lease_lost(&error) || is_task_lease_renewal_timed_out(&error) {
                tracing::info!(
                    task_id = task.id,
                    processing_token = lease.processing_token,
                    "background task worker stopped because its lease is no longer active; skipping stale completion"
                );
                return Ok(TaskDispatchOutcome::default());
            }
            let attempt_count = task.attempt_count + 1;
            let error_message = truncate_error(&error.to_string());
            let failed_steps_json =
                build_failed_task_steps_json(state, task.id, task.kind, &error_message).await;
            let retry_class = task_retry_class(task.kind, &error);
            let should_auto_retry =
                retry_class.should_auto_retry() && attempt_count < task.max_attempts;
            if !should_auto_retry {
                let finished_at = Utc::now();
                let failed = background_task_repo::mark_failed(
                    &state.db,
                    background_task_repo::TaskFailureUpdate {
                        id: task.id,
                        processing_token: lease.processing_token,
                        attempt_count,
                        last_error: &error_message,
                        finished_at,
                        expires_at: task_expiration_from(state, finished_at),
                        steps_json: failed_steps_json.as_deref(),
                        failure_can_retry: retry_class.can_manual_retry(),
                    },
                )
                .await?;
                if !failed {
                    tracing::info!(
                        task_id = task.id,
                        processing_token = lease.processing_token,
                        "background task lease moved before failure state update; ignoring stale worker"
                    );
                    return Ok(TaskDispatchOutcome::default());
                }
                tracing::warn!(
                    task_id = task.id,
                    kind = %task.kind.to_value(),
                    attempt_count,
                    error = %error_message,
                    "background task permanently failed"
                );
                Ok(TaskDispatchOutcome {
                    failed: usize::from(failed),
                    ..Default::default()
                })
            } else {
                let retry_at = Utc::now() + Duration::seconds(retry_delay_secs(attempt_count));
                let retried = background_task_repo::mark_retry(
                    &state.db,
                    task.id,
                    lease.processing_token,
                    attempt_count,
                    retry_at,
                    &error_message,
                    failed_steps_json.as_deref(),
                )
                .await?;
                if !retried {
                    tracing::info!(
                        task_id = task.id,
                        processing_token = lease.processing_token,
                        "background task lease moved before retry state update; ignoring stale worker"
                    );
                    return Ok(TaskDispatchOutcome::default());
                }
                tracing::warn!(
                    task_id = task.id,
                    kind = %task.kind.to_value(),
                    attempt_count,
                    retry_at = %retry_at,
                    error = %error_message,
                    "background task failed; scheduled retry"
                );
                Ok(TaskDispatchOutcome {
                    retried: usize::from(retried),
                    ..Default::default()
                })
            }
        }
    }
}

fn evaluate_heartbeat_result(lease_guard: &TaskLeaseGuard, result: Result<bool>) -> Result<()> {
    let lease = lease_guard.lease();
    match result {
        Ok(true) => {
            lease_guard.record_renewed();
            Ok(())
        }
        Ok(false) => {
            // false 不是数据库故障，而是条件更新没命中：
            // 一般表示 status/token 已经不是当前 worker 的了，任务已被别的 lease 接管。
            tracing::info!(
                task_id = lease.task_id,
                processing_token = lease.processing_token,
                "background task lease lost; stopping outdated worker"
            );
            Err(lease_guard.mark_lost())
        }
        Err(error) => {
            // 这里只记录并等待下一轮 heartbeat 重试；真正要停 worker 的信号只能是
            // token 不再匹配，或者连续太久没有任何成功续约。
            //
            // 也就是说，瞬时 DB 抖动不会立刻触发二次认领；只有抖动长到超过
            // renewal_timeout，lease guard 才会把当前 worker 判定为不再安全继续执行。
            tracing::warn!(
                task_id = lease.task_id,
                processing_token = lease.processing_token,
                error = %error,
                "background task heartbeat update failed; continuing and retrying next heartbeat"
            );
            lease_guard.ensure_active()
        }
    }
}

async fn build_failed_task_steps_json(
    state: &PrimaryAppState,
    task_id: i64,
    kind: BackgroundTaskKind,
    error_message: &str,
) -> Option<String> {
    let latest = background_task_repo::find_by_id(&state.db, task_id)
        .await
        .ok()?;
    let mut steps =
        parse_task_steps_json(latest.steps_json.as_ref().map(|raw| raw.as_ref()), kind).ok()?;
    if steps.is_empty() {
        return None;
    }
    mark_active_step_failed(&mut steps, Some(error_message));
    serialize_task_steps(&steps).ok().map(Into::into)
}

pub async fn drain(state: &PrimaryAppState) -> Result<DispatchStats> {
    let mut total = DispatchStats::default();

    for _ in 0..TASK_DRAIN_MAX_ROUNDS {
        let stats = dispatch_due(state).await?;
        let claimed = stats.claimed;
        total.claimed += stats.claimed;
        total.succeeded += stats.succeeded;
        total.retried += stats.retried;
        total.failed += stats.failed;
        if claimed > 0 {
            continue;
        }

        if background_task_repo::count_processing(&state.db).await? == 0 {
            break;
        }

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    Ok(total)
}

pub async fn cleanup_expired(state: &PrimaryAppState) -> Result<u64> {
    let now = Utc::now();
    let tasks_root = crate::utils::paths::temp_file_path(&state.config.server.temp_dir, "tasks");
    let mut entries = match tokio::fs::read_dir(&tasks_root).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => {
            return Err(AsterError::storage_driver_error(format!(
                "read task temp root {tasks_root}: {error}"
            )));
        }
    };
    let mut cleaned = 0;

    while let Some(entry) = entries.next_entry().await.map_err(|error| {
        AsterError::storage_driver_error(format!("iterate task temp root {tasks_root}: {error}"))
    })? {
        let path = entry.path();
        let path_display = path.to_string_lossy().to_string();
        let file_type = entry.file_type().await.map_err(|error| {
            AsterError::storage_driver_error(format!(
                "read task temp entry type {path_display}: {error}"
            ))
        })?;
        if !file_type.is_dir() {
            continue;
        }

        let dir_name = entry.file_name();
        let Some(dir_name) = dir_name.to_str() else {
            tracing::warn!(path = %path_display, "skipping task temp dir with non-utf8 name");
            continue;
        };
        let Ok(task_id) = dir_name.parse::<i64>() else {
            tracing::warn!(path = %path_display, "skipping task temp dir with invalid task id");
            continue;
        };

        // 这里只删“产物目录”，不删 background_task 记录：
        // - 终态且 expires_at 已到的任务：删 temp 目录，保留历史行；
        // - 数据库里已经没有任务行的孤儿目录：直接删，避免长期泄露磁盘。
        let should_cleanup = match background_task_repo::find_by_id(&state.db, task_id).await {
            Ok(task) => {
                task.expires_at <= now
                    && matches!(
                        task.status,
                        BackgroundTaskStatus::Succeeded
                            | BackgroundTaskStatus::Failed
                            | BackgroundTaskStatus::Canceled
                    )
            }
            Err(AsterError::RecordNotFound(_)) => {
                tracing::warn!(
                    task_id,
                    path = %path_display,
                    "cleaning orphaned task temp dir without task record"
                );
                true
            }
            Err(error) => return Err(error),
        };
        if !should_cleanup {
            continue;
        }

        crate::utils::cleanup_temp_dir(&path_display).await;
        let still_exists = tokio::fs::try_exists(&path).await.map_err(|error| {
            AsterError::storage_driver_error(format!(
                "verify task temp dir cleanup {path_display}: {error}"
            ))
        })?;
        if still_exists {
            tracing::warn!(
                task_id,
                path = %path_display,
                "task temp dir still exists after cleanup attempt"
            );
            continue;
        }

        cleaned += 1;
    }

    Ok(cleaned)
}

async fn process_task(
    state: &PrimaryAppState,
    task: &background_task::Model,
    lease_guard: TaskLeaseGuard,
) -> Result<()> {
    match task.kind {
        BackgroundTaskKind::ArchiveCompress => {
            archive::process_archive_compress_task(state, task, lease_guard).await
        }
        BackgroundTaskKind::ArchiveExtract => {
            archive::process_archive_extract_task(state, task, lease_guard).await
        }
        BackgroundTaskKind::ThumbnailGenerate => {
            thumbnail::process_thumbnail_generate_task(state, task, lease_guard).await
        }
        BackgroundTaskKind::SystemRuntime => Err(crate::errors::AsterError::internal_error(
            format!("system runtime task #{} should not be dispatched", task.id),
        )),
    }
}

fn task_lane_configs(state: &PrimaryAppState) -> Vec<TaskLaneConfig> {
    TASK_LANES
        .into_iter()
        .map(|lane| TaskLaneConfig {
            lane,
            limit: match lane {
                TaskLane::Archive => {
                    operations::background_task_archive_max_concurrency(&state.runtime_config)
                }
                TaskLane::Thumbnail => {
                    operations::background_task_thumbnail_max_concurrency(&state.runtime_config)
                }
                TaskLane::Fallback => {
                    operations::background_task_max_concurrency(&state.runtime_config)
                }
            },
            fast_continue: matches!(lane, TaskLane::Archive | TaskLane::Thumbnail),
        })
        .collect()
}

impl TaskLaneConfig {
    fn kinds(self) -> &'static [BackgroundTaskKind] {
        task_lane_kinds(self.lane)
    }

    fn lock_key(self) -> &'static str {
        match self.lane {
            TaskLane::Archive => operations::BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY_KEY,
            TaskLane::Thumbnail => operations::BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY_KEY,
            TaskLane::Fallback => operations::BACKGROUND_TASK_MAX_CONCURRENCY_KEY,
        }
    }
}

fn task_lane(kind: BackgroundTaskKind) -> TaskLane {
    match kind {
        BackgroundTaskKind::ArchiveCompress | BackgroundTaskKind::ArchiveExtract => {
            TaskLane::Archive
        }
        BackgroundTaskKind::ThumbnailGenerate => TaskLane::Thumbnail,
        BackgroundTaskKind::SystemRuntime => TaskLane::Fallback,
    }
}

fn task_lane_kinds(lane: TaskLane) -> &'static [BackgroundTaskKind] {
    match lane {
        TaskLane::Archive => &ARCHIVE_TASK_KINDS,
        TaskLane::Thumbnail => &THUMBNAIL_TASK_KINDS,
        TaskLane::Fallback => &FALLBACK_TASK_KINDS,
    }
}

fn available_lane_capacity(limit: usize, active: u64) -> usize {
    let active = usize::try_from(active).unwrap_or(usize::MAX);
    limit.saturating_sub(active)
}

fn claim_limit_to_u64(limit: usize) -> u64 {
    u64::try_from(limit).unwrap_or_else(|_| {
        tracing::warn!(
            limit,
            "background task lane limit exceeds u64; falling back to u64::MAX"
        );
        u64::MAX
    })
}

fn retry_delay_secs(attempt_count: i32) -> i64 {
    match attempt_count {
        1 => 5,
        2 => 15,
        3 => 60,
        _ => 300,
    }
}

fn task_retry_class(kind: BackgroundTaskKind, error: &AsterError) -> TaskRetryClass {
    match kind {
        BackgroundTaskKind::ArchiveCompress => {
            archive::ArchiveCompressRetryPolicy::retry_class(error)
        }
        BackgroundTaskKind::ArchiveExtract => {
            archive::ArchiveExtractRetryPolicy::retry_class(error)
        }
        BackgroundTaskKind::ThumbnailGenerate => {
            thumbnail::ThumbnailRetryPolicy::retry_class(error)
        }
        BackgroundTaskKind::SystemRuntime => runtime::RuntimeRetryPolicy::retry_class(error),
    }
}

async fn run_with_concurrency_limit<T, O, F, Fut>(items: Vec<T>, limit: usize, handler: F) -> Vec<O>
where
    F: FnMut(T) -> Fut,
    Fut: Future<Output = O>,
{
    stream::iter(items.into_iter().map(handler))
        .buffer_unordered(limit.max(1))
        .collect()
        .await
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use chrono::Utc;
    use sea_orm::{ActiveModelTrait, EntityTrait, Set};
    use tokio::time::{Duration, sleep};

    use crate::config::DatabaseConfig;
    use crate::db::repository::background_task_repo;
    use crate::db::{self, repository::config_repo};
    use crate::entities::background_task;
    use crate::errors::AsterError;
    use crate::storage::error::{StorageErrorKind, storage_driver_error};
    use crate::types::{BackgroundTaskKind, BackgroundTaskStatus, StoredTaskPayload};

    use super::{
        TaskClaimCandidate, TaskLane, TaskLaneConfig, available_lane_capacity,
        claim_candidates_for_lane, task_lane, task_retry_class,
    };
    use super::{evaluate_heartbeat_result, run_with_concurrency_limit};
    use crate::services::task_service::{
        TaskLease, TaskLeaseGuard, is_task_lease_lost, is_task_lease_renewal_timed_out,
    };
    use migration::Migrator;

    async fn build_dispatch_test_db() -> sea_orm::DatabaseConnection {
        let db = db::connect(&DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            pool_size: 1,
            retry_count: 0,
        })
        .await
        .expect("dispatch test DB should connect");
        Migrator::up(&db, None)
            .await
            .expect("dispatch test migrations should succeed");
        config_repo::ensure_defaults(&db)
            .await
            .expect("dispatch test config defaults should exist");
        db
    }

    async fn insert_dispatch_test_task(
        db: &sea_orm::DatabaseConnection,
        kind: BackgroundTaskKind,
        status: BackgroundTaskStatus,
        created_offset_secs: i64,
        lease_expires_at: Option<chrono::DateTime<Utc>>,
    ) -> background_task::Model {
        let now = Utc::now();
        background_task::ActiveModel {
            kind: Set(kind),
            status: Set(status),
            creator_user_id: Set(None),
            team_id: Set(None),
            share_id: Set(None),
            display_name: Set(format!("dispatch-claim-{created_offset_secs}")),
            payload_json: Set(StoredTaskPayload("{}".to_string())),
            result_json: Set(None),
            steps_json: Set(None),
            progress_current: Set(0),
            progress_total: Set(0),
            status_text: Set(None),
            attempt_count: Set(0),
            max_attempts: Set(1),
            next_run_at: Set(now - chrono::Duration::seconds(1)),
            processing_token: Set(0),
            processing_started_at: Set(match status {
                BackgroundTaskStatus::Processing => Some(now - chrono::Duration::seconds(30)),
                _ => None,
            }),
            last_heartbeat_at: Set(match status {
                BackgroundTaskStatus::Processing => Some(now - chrono::Duration::seconds(30)),
                _ => None,
            }),
            lease_expires_at: Set(lease_expires_at),
            started_at: Set(match status {
                BackgroundTaskStatus::Processing => Some(now - chrono::Duration::seconds(30)),
                _ => None,
            }),
            finished_at: Set(None),
            last_error: Set(None),
            failure_can_retry: Set(None),
            expires_at: Set(now + chrono::Duration::hours(1)),
            created_at: Set(now + chrono::Duration::seconds(created_offset_secs)),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("dispatch test task should insert")
    }

    fn claim_candidate(index: usize, task: &background_task::Model) -> TaskClaimCandidate {
        TaskClaimCandidate {
            index,
            task_id: task.id,
            expected_processing_token: task.processing_token,
            next_processing_token: task.processing_token + 1,
        }
    }

    #[tokio::test]
    async fn run_with_concurrency_limit_caps_parallelism() {
        let in_flight = Arc::new(AtomicUsize::new(0));
        let max_in_flight = Arc::new(AtomicUsize::new(0));

        let mut results = run_with_concurrency_limit(vec![1, 2, 3, 4, 5], 2, {
            let in_flight = in_flight.clone();
            let max_in_flight = max_in_flight.clone();
            move |value| {
                let in_flight = in_flight.clone();
                let max_in_flight = max_in_flight.clone();
                async move {
                    let current = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                    if let Err(e) =
                        max_in_flight.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |existing| {
                            Some(existing.max(current))
                        })
                    {
                        tracing::trace!("max_in_flight fetch_update failed: {e}");
                    }
                    sleep(Duration::from_millis(20)).await;
                    in_flight.fetch_sub(1, Ordering::SeqCst);
                    value * 2
                }
            }
        })
        .await;

        results.sort_unstable();
        assert_eq!(results, vec![2, 4, 6, 8, 10]);
        assert_eq!(max_in_flight.load(Ordering::SeqCst), 2);
        assert_eq!(in_flight.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn task_lane_keeps_archive_and_thumbnail_separate() {
        assert_eq!(
            task_lane(BackgroundTaskKind::ArchiveCompress),
            super::TaskLane::Archive
        );
        assert_eq!(
            task_lane(BackgroundTaskKind::ArchiveExtract),
            super::TaskLane::Archive
        );
        assert_eq!(
            task_lane(BackgroundTaskKind::ThumbnailGenerate),
            super::TaskLane::Thumbnail
        );
        assert_eq!(
            task_lane(BackgroundTaskKind::SystemRuntime),
            super::TaskLane::Fallback
        );
    }

    #[test]
    fn available_lane_capacity_saturates_when_active_exceeds_limit() {
        assert_eq!(available_lane_capacity(3, 1), 2);
        assert_eq!(available_lane_capacity(3, 3), 0);
        assert_eq!(available_lane_capacity(3, 4), 0);
        assert_eq!(available_lane_capacity(3, u64::MAX), 0);
    }

    #[tokio::test]
    async fn claim_candidates_for_lane_claims_batch_up_to_rechecked_capacity() {
        let db = build_dispatch_test_db().await;
        let tasks = [
            insert_dispatch_test_task(
                &db,
                BackgroundTaskKind::ArchiveCompress,
                BackgroundTaskStatus::Pending,
                -3,
                None,
            )
            .await,
            insert_dispatch_test_task(
                &db,
                BackgroundTaskKind::ArchiveExtract,
                BackgroundTaskStatus::Pending,
                -2,
                None,
            )
            .await,
            insert_dispatch_test_task(
                &db,
                BackgroundTaskKind::ArchiveCompress,
                BackgroundTaskStatus::Pending,
                -1,
                None,
            )
            .await,
        ];
        let candidates = tasks
            .iter()
            .enumerate()
            .map(|(index, task)| claim_candidate(index, task))
            .collect::<Vec<_>>();

        let claimed = claim_candidates_for_lane(
            &db,
            TaskLaneConfig {
                lane: TaskLane::Archive,
                limit: 2,
                fast_continue: true,
            },
            &candidates,
            Utc::now() - chrono::Duration::seconds(60),
        )
        .await
        .expect("batch claim should succeed");

        assert_eq!(claimed.len(), 2);
        assert_eq!(claimed[0].task_id, tasks[0].id);
        assert_eq!(claimed[1].task_id, tasks[1].id);
        assert_eq!(claimed[0].processing_token, 1);
        assert_eq!(claimed[1].processing_token, 1);

        let stored = background_task::Entity::find()
            .all(&db)
            .await
            .expect("stored tasks should load");
        let processing = stored
            .iter()
            .filter(|task| task.status == BackgroundTaskStatus::Processing)
            .map(|task| task.id)
            .collect::<Vec<_>>();
        assert!(processing.contains(&tasks[0].id));
        assert!(processing.contains(&tasks[1].id));
        assert!(!processing.contains(&tasks[2].id));
    }

    #[tokio::test]
    async fn claim_candidates_for_lane_skips_claim_when_rechecked_capacity_is_full() {
        let db = build_dispatch_test_db().await;
        let now = Utc::now();
        insert_dispatch_test_task(
            &db,
            BackgroundTaskKind::ThumbnailGenerate,
            BackgroundTaskStatus::Processing,
            -3,
            Some(now + chrono::Duration::seconds(60)),
        )
        .await;
        let pending = insert_dispatch_test_task(
            &db,
            BackgroundTaskKind::ThumbnailGenerate,
            BackgroundTaskStatus::Pending,
            -1,
            None,
        )
        .await;
        let candidates = vec![claim_candidate(0, &pending)];

        let claimed = claim_candidates_for_lane(
            &db,
            TaskLaneConfig {
                lane: TaskLane::Thumbnail,
                limit: 1,
                fast_continue: true,
            },
            &candidates,
            Utc::now() - chrono::Duration::seconds(60),
        )
        .await
        .expect("full lane batch claim should succeed without claiming");

        assert!(claimed.is_empty());
        let stored = background_task_repo::find_by_id(&db, pending.id)
            .await
            .expect("pending task should still exist");
        assert_eq!(stored.status, BackgroundTaskStatus::Pending);
        assert_eq!(stored.processing_token, 0);
    }

    #[tokio::test]
    async fn claim_candidates_for_lane_continues_after_stale_candidate_loses_cas() {
        let db = build_dispatch_test_db().await;
        let stale = insert_dispatch_test_task(
            &db,
            BackgroundTaskKind::ArchiveCompress,
            BackgroundTaskStatus::Pending,
            -2,
            None,
        )
        .await;
        let next = insert_dispatch_test_task(
            &db,
            BackgroundTaskKind::ArchiveCompress,
            BackgroundTaskStatus::Pending,
            -1,
            None,
        )
        .await;
        let candidates = vec![
            TaskClaimCandidate {
                index: 0,
                task_id: stale.id,
                expected_processing_token: stale.processing_token + 1,
                next_processing_token: stale.processing_token + 2,
            },
            claim_candidate(1, &next),
        ];

        let claimed = claim_candidates_for_lane(
            &db,
            TaskLaneConfig {
                lane: TaskLane::Archive,
                limit: 1,
                fast_continue: true,
            },
            &candidates,
            Utc::now() - chrono::Duration::seconds(60),
        )
        .await
        .expect("batch claim should skip stale CAS misses");

        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].task_id, next.id);
        let stale = background_task_repo::find_by_id(&db, stale.id)
            .await
            .expect("stale candidate should still exist");
        assert_eq!(stale.status, BackgroundTaskStatus::Pending);
        assert_eq!(stale.processing_token, 0);
    }

    #[test]
    fn evaluate_heartbeat_result_keeps_retrying_after_transient_error() {
        let lease = TaskLease::new(42, 7);
        let lease_guard = TaskLeaseGuard::with_renewal_timeout(lease, Duration::from_secs(60));
        let result =
            evaluate_heartbeat_result(&lease_guard, Err(AsterError::database_operation("boom")));
        assert!(result.is_ok());
    }

    #[test]
    fn evaluate_heartbeat_result_reports_lease_loss_when_claim_replaced() {
        let lease = TaskLease::new(42, 7);
        let lease_guard = TaskLeaseGuard::with_renewal_timeout(lease, Duration::from_secs(60));
        let error = evaluate_heartbeat_result(&lease_guard, Ok(false))
            .expect_err("heartbeat mismatch should report lease loss");
        assert!(is_task_lease_lost(&error));
    }

    #[tokio::test]
    async fn evaluate_heartbeat_result_stops_worker_after_renewal_timeout() {
        let lease = TaskLease::new(42, 7);
        let lease_guard = TaskLeaseGuard::with_renewal_timeout(lease, Duration::from_millis(20));
        sleep(Duration::from_millis(30)).await;

        let error =
            evaluate_heartbeat_result(&lease_guard, Err(AsterError::database_operation("boom")))
                .expect_err("expired renewal window should stop the worker");
        assert!(is_task_lease_renewal_timed_out(&error));
    }

    #[test]
    fn thumbnail_retry_only_keeps_transient_storage_errors() {
        let transient = storage_driver_error(StorageErrorKind::Transient, "remote timeout");
        let misconfigured = storage_driver_error(StorageErrorKind::Misconfigured, "missing bucket");

        assert!(
            task_retry_class(BackgroundTaskKind::ThumbnailGenerate, &transient).should_auto_retry()
        );
        assert!(
            !task_retry_class(BackgroundTaskKind::ThumbnailGenerate, &misconfigured)
                .can_manual_retry()
        );
    }

    #[test]
    fn archive_validation_errors_are_not_retryable() {
        let error = AsterError::validation_error("archive entry compression ratio exceeds limit");
        let retry_class = task_retry_class(BackgroundTaskKind::ArchiveExtract, &error);

        assert!(!retry_class.should_auto_retry());
        assert!(!retry_class.can_manual_retry());
    }

    #[test]
    fn archive_transient_storage_errors_are_auto_retryable() {
        let error = storage_driver_error(StorageErrorKind::Transient, "remote timeout");
        let retry_class = task_retry_class(BackgroundTaskKind::ArchiveCompress, &error);

        assert!(retry_class.should_auto_retry());
        assert!(retry_class.can_manual_retry());
    }
}

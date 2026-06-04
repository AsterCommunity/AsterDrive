use std::future::Future;
use std::time::Duration as StdDuration;

use chrono::{Duration, Utc};
use futures::stream::{self, StreamExt};
use sea_orm::ActiveEnum;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;

use super::{
    DispatchStats, TASK_HEARTBEAT_INTERVAL_SECS, TaskDispatchOutcome, TaskLease, TaskLeaseGuard,
    is_task_lease_lost, is_task_lease_renewal_timed_out, task_expiration_from,
    task_lease_expires_at, truncate_error,
};
use crate::db::repository::background_task_repo;
use crate::entities::background_task;
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::services::task_service::{
    TaskExecutionContext, registry,
    retry::TaskRetryClass,
    steps::{mark_active_step_failed, parse_task_steps_json, serialize_task_steps},
};
use crate::types::{BackgroundTaskKind, BackgroundTaskStatus};

pub(super) async fn run_claimed_tasks(
    state: &PrimaryAppState,
    mut claimed_tasks: Vec<(background_task::Model, TaskLease)>,
    shutdown_token: CancellationToken,
) -> Result<DispatchStats> {
    let concurrency = claimed_tasks.len().max(1);
    claimed_tasks.sort_by_key(|(task, _)| (task.created_at, task.id));

    // 先把认领结果固定下来，再启动 worker。每个 lane 的容量已经在 claim 阶段扣过，
    // 这里直接把本批已认领任务全部放出去；fast_continue lane 会在本批结束后继续补位。
    let results = run_with_concurrency_limit(claimed_tasks, concurrency, |(task, lease)| {
        let state = state.clone();
        let shutdown_token = shutdown_token.clone();
        async move { process_claimed_task(&state, task, lease, shutdown_token).await }
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
async fn process_claimed_task(
    state: &PrimaryAppState,
    task: background_task::Model,
    lease: TaskLease,
    shutdown_token: CancellationToken,
) -> Result<TaskDispatchOutcome> {
    let context = TaskExecutionContext::new(lease, shutdown_token);
    let lease_guard = context.lease_guard().clone();
    let heartbeat_stop = CancellationToken::new();
    // Heartbeat must run in its own task. With SQLite the writer pool has one
    // connection; keeping heartbeat in a select! with the business future can
    // pause a future that already acquired that connection, then wait forever
    // for a second writer connection.
    let heartbeat_handle = spawn_task_heartbeat(
        state.clone(),
        task.id,
        lease.processing_token,
        lease_guard.clone(),
        heartbeat_stop.clone(),
    );
    let heartbeat_cancel_guard = heartbeat_stop.clone().drop_guard();

    let task_result = match context.ensure_active() {
        Ok(()) => registry::process_task(state, &task, context).await,
        Err(error) => Err(error),
    };
    drop(heartbeat_cancel_guard);
    stop_task_heartbeat(heartbeat_stop, heartbeat_handle).await;

    match task_result {
        Ok(()) => {
            record_task_metric(state, task.kind, "succeeded");
            Ok(TaskDispatchOutcome {
                succeeded: 1,
                ..Default::default()
            })
        }
        Err(error) => {
            // lease 丢失 / 续约超时代表“这条执行流已经过期”，不是业务失败。
            // 这时不要再把任务改成 Failed/Retry，否则旧 worker 可能覆盖新 lease 的结果。
            if is_task_lease_lost(&error)
                || is_task_lease_renewal_timed_out(&error)
                || super::super::is_task_worker_shutdown_requested(&error)
            {
                if super::super::is_task_worker_shutdown_requested(&error) {
                    release_task_for_shutdown(state, task.id, lease.processing_token).await?;
                }
                tracing::info!(
                    task_id = task.id,
                    processing_token = lease.processing_token,
                    "background task worker stopped before completion; skipping stale completion"
                );
                return Ok(TaskDispatchOutcome::default());
            }
            let attempt_count = task.attempt_count + 1;
            let error_message =
                truncate_error(&crate::errors::encode_task_error_for_storage(&error));
            let display_error_message =
                crate::errors::task_error_display_message(&error_message).to_string();
            let failed_steps_json =
                build_failed_task_steps_json(state, task.id, task.kind, &display_error_message)
                    .await;
            let retry_class = task_retry_class(task.kind, &error);
            let should_auto_retry =
                retry_class.should_auto_retry() && attempt_count < task.max_attempts;
            if !should_auto_retry {
                let finished_at = Utc::now();
                let failed = background_task_repo::mark_failed(
                    state.writer_db(),
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
                    error = %display_error_message,
                    "background task permanently failed"
                );
                if failed {
                    record_task_metric(state, task.kind, "failed");
                }
                Ok(TaskDispatchOutcome {
                    failed: usize::from(failed),
                    ..Default::default()
                })
            } else {
                let retry_at = Utc::now() + Duration::seconds(retry_delay_secs(attempt_count));
                let retried = background_task_repo::mark_retry(
                    state.writer_db(),
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
                    error = %display_error_message,
                    "background task failed; scheduled retry"
                );
                state.wake_background_task_dispatcher();
                if retried {
                    record_task_metric(state, task.kind, "retry");
                }
                Ok(TaskDispatchOutcome {
                    retried: usize::from(retried),
                    ..Default::default()
                })
            }
        }
    }
}

fn spawn_task_heartbeat(
    state: PrimaryAppState,
    task_id: i64,
    processing_token: i64,
    lease_guard: TaskLeaseGuard,
    stop_token: CancellationToken,
) -> JoinHandle<()> {
    spawn_task_heartbeat_with_interval(
        state,
        task_id,
        processing_token,
        lease_guard,
        stop_token,
        StdDuration::from_secs(TASK_HEARTBEAT_INTERVAL_SECS),
    )
}

pub(super) fn spawn_task_heartbeat_with_interval(
    state: PrimaryAppState,
    task_id: i64,
    processing_token: i64,
    lease_guard: TaskLeaseGuard,
    stop_token: CancellationToken,
    interval: StdDuration,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        run_task_heartbeat_loop(
            state,
            task_id,
            processing_token,
            lease_guard,
            stop_token,
            interval,
        )
        .await;
    })
}

async fn run_task_heartbeat_loop(
    state: PrimaryAppState,
    task_id: i64,
    processing_token: i64,
    lease_guard: TaskLeaseGuard,
    stop_token: CancellationToken,
    interval: StdDuration,
) {
    let mut heartbeat = tokio::time::interval(interval);
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
    heartbeat.tick().await;

    loop {
        tokio::select! {
            _ = stop_token.cancelled() => return,
            _ = heartbeat.tick() => {
                let now = Utc::now();
                // Let task completion cancel an in-flight pool acquire quickly.
                // This keeps shutdown/finish paths from waiting on a heartbeat
                // write that is no longer useful.
                let result = tokio::select! {
                    _ = stop_token.cancelled() => return,
                    result = background_task_repo::touch_heartbeat(
                        state.writer_db(),
                        task_id,
                        processing_token,
                        now,
                        task_lease_expires_at(now),
                    ) => result,
                };

                if evaluate_heartbeat_result(&lease_guard, result).is_err() {
                    return;
                }
            }
        }
    }
}

async fn stop_task_heartbeat(stop_token: CancellationToken, heartbeat_handle: JoinHandle<()>) {
    stop_token.cancel();
    if let Err(error) = heartbeat_handle.await {
        tracing::warn!(error = %error, "background task heartbeat worker stopped unexpectedly");
    }
}

async fn release_task_for_shutdown(
    state: &PrimaryAppState,
    task_id: i64,
    processing_token: i64,
) -> Result<()> {
    // Graceful shutdown is neither task success nor task failure. Release the
    // current processing lease back into Retry so the next dispatcher round can
    // resume it with a fresh processing token.
    let released = background_task_repo::release_processing(
        state.writer_db(),
        task_id,
        processing_token,
        Utc::now(),
        BackgroundTaskStatus::Retry,
    )
    .await?;
    if released {
        state.wake_background_task_dispatcher();
    }
    Ok(())
}

fn record_task_metric(state: &PrimaryAppState, kind: BackgroundTaskKind, status: &'static str) {
    state
        .metrics
        .record_background_task_transition(kind.as_str(), status);
}

pub(super) fn evaluate_heartbeat_result(
    lease_guard: &TaskLeaseGuard,
    result: Result<bool>,
) -> Result<()> {
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
    let latest = background_task_repo::find_by_id(state.writer_db(), task_id)
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
fn retry_delay_secs(attempt_count: i32) -> i64 {
    match attempt_count {
        1 => 5,
        2 => 15,
        3 => 60,
        _ => 300,
    }
}

pub(super) fn task_retry_class(kind: BackgroundTaskKind, error: &AsterError) -> TaskRetryClass {
    super::super::registry::task_retry_class(kind, error)
}

pub(super) async fn run_with_concurrency_limit<T, O, F, Fut>(
    items: Vec<T>,
    limit: usize,
    handler: F,
) -> Vec<O>
where
    F: FnMut(T) -> Fut,
    Fut: Future<Output = O>,
{
    stream::iter(items.into_iter().map(handler))
        .buffer_unordered(limit.max(1))
        .collect()
        .await
}

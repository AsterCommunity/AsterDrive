//! 运行时子模块：`tasks`。

use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::time::Duration;

use actix_web::web;
use chrono::Utc;
use futures::FutureExt;
use rand::RngExt;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use super::{FollowerAppState, PrimaryAppState};
use crate::services::share::ShareDownloadRollbackWorker;
use crate::services::task::SystemRuntimeTaskKind;
use crate::utils::numbers::u128_to_u64;

const BACKGROUND_TASK_SHUTDOWN_GRACE: Duration = Duration::from_secs(30);
const BACKGROUND_TASK_DISPATCH_ERROR_BACKOFF_CAP: Duration = Duration::from_secs(5);
const MAINTENANCE_CLEANUP_JITTER_CAP: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackgroundTaskDispatchTrigger {
    Startup,
    Timer,
    Wakeup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BackgroundTaskDispatchIteration {
    has_activity: bool,
    failed: bool,
}

impl BackgroundTaskDispatchIteration {
    fn idle() -> Self {
        Self {
            has_activity: false,
            failed: false,
        }
    }

    fn active() -> Self {
        Self {
            has_activity: true,
            failed: false,
        }
    }

    fn failed() -> Self {
        Self {
            has_activity: false,
            failed: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BackgroundTaskDispatchBackoff {
    idle_interval: Duration,
    last_error: bool,
}

impl BackgroundTaskDispatchBackoff {
    fn new(base_interval: Duration, max_interval: Duration) -> Self {
        Self {
            idle_interval: effective_dispatch_base_interval(base_interval, max_interval),
            last_error: false,
        }
    }

    fn sleep_duration(&self, base_interval: Duration, max_interval: Duration) -> Duration {
        let base_interval = effective_dispatch_base_interval(base_interval, max_interval);
        let max_interval = effective_dispatch_max_interval(base_interval, max_interval);
        if self.last_error {
            return base_interval.max(BACKGROUND_TASK_DISPATCH_ERROR_BACKOFF_CAP);
        }
        self.idle_interval.max(base_interval).min(max_interval)
    }

    fn record_iteration(
        &mut self,
        trigger: BackgroundTaskDispatchTrigger,
        iteration: BackgroundTaskDispatchIteration,
        base_interval: Duration,
        max_interval: Duration,
    ) {
        let base_interval = effective_dispatch_base_interval(base_interval, max_interval);
        let max_interval = effective_dispatch_max_interval(base_interval, max_interval);
        if iteration.failed {
            self.idle_interval = base_interval;
            self.last_error = true;
            return;
        }
        if iteration.has_activity || matches!(trigger, BackgroundTaskDispatchTrigger::Wakeup) {
            self.idle_interval = base_interval;
            self.last_error = false;
            return;
        }
        self.idle_interval = self
            .idle_interval
            .max(base_interval)
            .saturating_mul(2)
            .min(max_interval);
        self.last_error = false;
    }
}

pub struct BackgroundTasks {
    shutdown_token: CancellationToken,
    handles: JoinSet<()>,
}

impl BackgroundTasks {
    #[cfg(test)]
    fn new() -> Self {
        Self::with_shutdown_token(CancellationToken::new())
    }

    fn with_shutdown_token(shutdown_token: CancellationToken) -> Self {
        Self {
            shutdown_token,
            handles: JoinSet::new(),
        }
    }

    fn shutdown_token(&self) -> CancellationToken {
        self.shutdown_token.clone()
    }

    fn push<F>(&mut self, task: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.handles.spawn(task);
    }

    pub async fn shutdown(self) {
        let BackgroundTasks {
            shutdown_token,
            mut handles,
        } = self;
        // 发送停止信号：各 worker 在当前 sleep 或下一次 select! 时退出。
        // 注意：正在执行中的迭代（task_fn）会跑到自然结束，不会在迭代中途被截断。
        // 这是期望行为：保持 DB 事务完整性，避免 lease 半写。
        shutdown_token.cancel();

        // 等所有 worker 自然退出，给 grace 时间让跑了一半的迭代完成。
        // JoinSet 会在 join_next 时移除已完成任务，避免对同一个句柄重复 await。
        let graceful_shutdown = async { while handles.join_next().await.is_some() {} };
        if tokio::time::timeout(BACKGROUND_TASK_SHUTDOWN_GRACE, graceful_shutdown)
            .await
            .is_err()
        {
            // grace 期内未能结束的 worker 才强制 abort。
            // 正常情况下 task_fn 的最长单次执行不会超 grace 时间。
            let aborted = handles.len();
            handles.abort_all();
            tracing::warn!(
                aborted,
                grace_secs = BACKGROUND_TASK_SHUTDOWN_GRACE.as_secs(),
                "background tasks did not stop before the shutdown grace period; aborting remaining workers"
            );
            while handles.join_next().await.is_some() {}
        }
    }
}

/// Spawn a periodic background task with panic recovery.
///
/// Each iteration sleeps using the latest runtime-configured interval before
/// the next run. Panics are caught inside the loop so one failed iteration
/// does not kill the whole periodic worker.
async fn spawn_periodic<F, I, Fut>(
    name: SystemRuntimeTaskKind,
    interval_fn: I,
    jitter_cap: Option<Duration>,
    shutdown_token: CancellationToken,
    state: web::Data<PrimaryAppState>,
    task_fn: F,
) where
    I: Fn(&PrimaryAppState) -> Duration + Send + Sync + 'static,
    F: Fn(web::Data<PrimaryAppState>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = crate::services::task::RuntimeTaskRunOutcome> + Send + 'static,
{
    let task_name = name.as_str();
    // 每轮迭代独立 span，并发清理任务在 trace 里可按 task.name 区分。
    // 必须用 `Instrument::instrument` 而非 `Span::enter`：后者返回的 guard
    // 跨 await 会被 drop（tracing 文档警告），span 只对同步段生效，
    // 而我们的 task_fn 全是 async 跨 await 的。
    if shutdown_token.is_cancelled() {
        return;
    }
    run_periodic_iteration(name, &state, &task_fn)
        .instrument(tracing::info_span!("bg_task", task.name = task_name))
        .await;

    loop {
        let sleep_duration = periodic_sleep_duration(interval_fn(state.get_ref()), jitter_cap);
        tokio::select! {
            biased;
            _ = shutdown_token.cancelled() => break,
            _ = tokio::time::sleep(sleep_duration) => {}
        }

        if shutdown_token.is_cancelled() {
            break;
        }

        run_periodic_iteration(name, &state, &task_fn)
            .instrument(tracing::info_span!("bg_task", task.name = task_name))
            .await;
    }
}

async fn run_periodic_iteration<F, Fut>(
    name: SystemRuntimeTaskKind,
    state: &web::Data<PrimaryAppState>,
    task_fn: &F,
) where
    F: Fn(web::Data<PrimaryAppState>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = crate::services::task::RuntimeTaskRunOutcome> + Send + 'static,
{
    let task_name = name.as_str();
    let started_at = Utc::now();
    let s = state.clone();
    let outcome = match AssertUnwindSafe(task_fn(s)).catch_unwind().await {
        Ok(outcome) => outcome,
        Err(panic) => {
            let panic_message = if let Some(message) = panic.downcast_ref::<&str>() {
                (*message).to_string()
            } else if let Some(message) = panic.downcast_ref::<String>() {
                message.clone()
            } else {
                "unknown panic payload".to_string()
            };
            tracing::error!("background task '{task_name}' panicked: {panic_message}");
            crate::services::task::RuntimeTaskRunOutcome::failed(
                Some("Task panicked".to_string()),
                panic_message,
            )
        }
    };
    let finished_at = Utc::now();

    // 每轮周期任务结束后，都会尝试把执行结果记成一条 SystemRuntime 事件。
    // 这样管理员面板里的任务表可以同时看到：
    // - 用户创建的后台任务
    // - 系统调度/清理任务的执行历史
    if let Err(error) = crate::services::task::record_runtime_task_run(
        state.get_ref(),
        name,
        started_at,
        finished_at,
        &outcome,
    )
    .await
    {
        tracing::warn!("failed to record runtime task '{task_name}': {error}");
    }
}

async fn spawn_background_task_dispatcher(
    shutdown_token: CancellationToken,
    state: web::Data<PrimaryAppState>,
) {
    let mut backoff = BackgroundTaskDispatchBackoff::new(
        background_task_dispatch_interval(&state),
        background_task_dispatch_idle_max_interval(&state),
    );
    if shutdown_token.is_cancelled() {
        return;
    }
    let iteration = run_background_task_dispatch_iteration(&state, shutdown_token.clone())
        .instrument(tracing::info_span!(
            "bg_task",
            task.name = SystemRuntimeTaskKind::BackgroundTaskDispatch.as_str()
        ))
        .await;
    backoff.record_iteration(
        BackgroundTaskDispatchTrigger::Startup,
        iteration,
        background_task_dispatch_interval(&state),
        background_task_dispatch_idle_max_interval(&state),
    );

    loop {
        let sleep_duration = backoff.sleep_duration(
            background_task_dispatch_interval(&state),
            background_task_dispatch_idle_max_interval(&state),
        );
        let trigger = tokio::select! {
            biased;
            _ = shutdown_token.cancelled() => break,
            _ = state.background_task_dispatch_wakeup.notified() => {
                BackgroundTaskDispatchTrigger::Wakeup
            }
            _ = tokio::time::sleep(sleep_duration) => {
                BackgroundTaskDispatchTrigger::Timer
            }
        };

        if shutdown_token.is_cancelled() {
            break;
        }

        let iteration = run_background_task_dispatch_iteration(&state, shutdown_token.clone())
            .instrument(tracing::info_span!(
                "bg_task",
                task.name = SystemRuntimeTaskKind::BackgroundTaskDispatch.as_str()
            ))
            .await;
        backoff.record_iteration(
            trigger,
            iteration,
            background_task_dispatch_interval(&state),
            background_task_dispatch_idle_max_interval(&state),
        );
    }
}

async fn run_background_task_dispatch_iteration(
    state: &web::Data<PrimaryAppState>,
    shutdown_token: CancellationToken,
) -> BackgroundTaskDispatchIteration {
    let started_at = Utc::now();
    let (iteration, outcome) = match AssertUnwindSafe(
        crate::services::task::dispatch::dispatch_due_with_shutdown(
            state.get_ref(),
            shutdown_token,
        ),
    )
    .catch_unwind()
    .await
    {
        Ok(result) => {
            let iteration = match &result {
                Ok(stats) if stats.has_activity() => BackgroundTaskDispatchIteration::active(),
                Ok(_) => BackgroundTaskDispatchIteration::idle(),
                Err(_) => BackgroundTaskDispatchIteration::failed(),
            };
            (iteration, background_task_dispatch_outcome(result))
        }
        Err(panic) => {
            let panic_message = if let Some(message) = panic.downcast_ref::<&str>() {
                (*message).to_string()
            } else if let Some(message) = panic.downcast_ref::<String>() {
                message.clone()
            } else {
                "unknown panic payload".to_string()
            };
            tracing::error!("background task 'background-task-dispatch' panicked: {panic_message}");
            (
                BackgroundTaskDispatchIteration::failed(),
                crate::services::task::RuntimeTaskRunOutcome::failed(
                    Some("Task panicked".to_string()),
                    panic_message,
                ),
            )
        }
    };
    let finished_at = Utc::now();

    if let Err(error) = crate::services::task::record_runtime_task_run(
        state.get_ref(),
        SystemRuntimeTaskKind::BackgroundTaskDispatch,
        started_at,
        finished_at,
        &outcome,
    )
    .await
    {
        tracing::warn!("failed to record runtime task 'background-task-dispatch': {error}");
    }

    iteration
}

fn background_task_dispatch_outcome(
    result: crate::errors::Result<crate::services::task::DispatchStats>,
) -> crate::services::task::RuntimeTaskRunOutcome {
    match result {
        Ok(stats) => {
            if stats.has_activity() {
                tracing::info!(
                    claimed = stats.claimed,
                    succeeded = stats.succeeded,
                    retried = stats.retried,
                    failed = stats.failed,
                    "processed background task batch"
                );
            }
            crate::services::task::RuntimeTaskRunOutcome::quiet()
        }
        Err(error) => {
            tracing::warn!("background task dispatch failed: {error}");
            crate::services::task::RuntimeTaskRunOutcome::failed(
                Some("Background task dispatch failed".to_string()),
                error.to_string(),
            )
        }
    }
}

fn periodic_sleep_duration(base_interval: Duration, jitter_cap: Option<Duration>) -> Duration {
    let Some(jitter_cap) = jitter_cap else {
        return base_interval;
    };

    let max_jitter_ms = effective_jitter_cap(base_interval, jitter_cap).as_millis();
    if max_jitter_ms == 0 {
        return base_interval;
    }

    let jitter_ms = rand::rng().random_range(
        0..=u128_to_u64(
            max_jitter_ms.min(u128::from(u64::MAX)),
            "background task jitter",
        )
        .unwrap_or(u64::MAX),
    );
    base_interval.saturating_add(Duration::from_millis(jitter_ms))
}

fn effective_jitter_cap(base_interval: Duration, jitter_cap: Duration) -> Duration {
    let bounded_ms = u128_to_u64(
        base_interval.as_millis().min(u128::from(u64::MAX)),
        "base interval millis",
    )
    .unwrap_or(u64::MAX)
        / 10;
    jitter_cap.min(Duration::from_millis(bounded_ms))
}

fn effective_dispatch_base_interval(base_interval: Duration, _max_interval: Duration) -> Duration {
    if base_interval.is_zero() {
        return Duration::from_secs(1);
    }
    base_interval
}

fn effective_dispatch_max_interval(base_interval: Duration, max_interval: Duration) -> Duration {
    max_interval.max(base_interval)
}

fn build_background_tasks_base(
    metrics: &crate::metrics_core::SharedMetricsRecorder,
    shutdown_token: CancellationToken,
) -> BackgroundTasks {
    let mut tasks = BackgroundTasks::with_shutdown_token(shutdown_token);
    if let Some(task) = metrics.system_metrics_updater_task(tasks.shutdown_token()) {
        tasks.push(task);
    }
    tasks
}

/// Spawn all primary-only periodic background cleanup tasks.
pub fn spawn_primary_background_tasks(
    state: web::Data<PrimaryAppState>,
    share_download_rollback_worker: ShareDownloadRollbackWorker,
    shutdown_token: CancellationToken,
) -> BackgroundTasks {
    let mut tasks = build_background_tasks_base(&state.metrics, shutdown_token);
    let shutdown_token = tasks.shutdown_token();

    tasks.push(
        crate::services::share::share_download_rollback_worker_task(
            shutdown_token.clone(),
            share_download_rollback_worker,
        )
        .instrument(tracing::info_span!(
            "bg_task",
            task.name = "share-download-rollback"
        )),
    );

    tasks.push(spawn_periodic(
        SystemRuntimeTaskKind::MailOutboxDispatch,
        mail_outbox_dispatch_interval,
        None,
        shutdown_token.clone(),
        state.clone(),
        |s| async move {
            match crate::services::mail::outbox::dispatch_due(s.get_ref()).await {
                Ok(stats) if stats.claimed > 0 || stats.failed > 0 => {
                    tracing::info!(
                        claimed = stats.claimed,
                        sent = stats.sent,
                        retried = stats.retried,
                        failed = stats.failed,
                        "processed mail outbox batch"
                    );
                    crate::services::task::RuntimeTaskRunOutcome::succeeded(Some(format!(
                        "claimed {}, sent {}, retried {}, failed {}",
                        stats.claimed, stats.sent, stats.retried, stats.failed
                    )))
                }
                Ok(_) => crate::services::task::RuntimeTaskRunOutcome::quiet(),
                Err(error) => {
                    tracing::warn!("mail outbox dispatch failed: {error}");
                    crate::services::task::RuntimeTaskRunOutcome::failed(
                        Some("Mail outbox dispatch failed".to_string()),
                        error.to_string(),
                    )
                }
            }
        },
    ));

    tasks.push(spawn_background_task_dispatcher(
        shutdown_token.clone(),
        state.clone(),
    ));

    tasks.push(spawn_periodic(
        SystemRuntimeTaskKind::UploadCleanup,
        maintenance_cleanup_interval,
        Some(MAINTENANCE_CLEANUP_JITTER_CAP),
        shutdown_token.clone(),
        state.clone(),
        |s| async move {
            match crate::services::files::upload::cleanup_expired(s.get_ref()).await {
                Ok(count) if count > 0 => {
                    tracing::info!("cleaned up {count} expired upload sessions");
                    crate::services::task::RuntimeTaskRunOutcome::succeeded(Some(format!(
                        "cleaned up {count} expired upload sessions"
                    )))
                }
                Ok(_) => crate::services::task::RuntimeTaskRunOutcome::quiet(),
                Err(error) => {
                    tracing::warn!("upload cleanup failed: {error}");
                    crate::services::task::RuntimeTaskRunOutcome::failed(
                        Some("Upload cleanup failed".to_string()),
                        error.to_string(),
                    )
                }
            }
        },
    ));

    tasks.push(spawn_periodic(
        SystemRuntimeTaskKind::CompletedUploadCleanup,
        maintenance_cleanup_interval,
        Some(MAINTENANCE_CLEANUP_JITTER_CAP),
        shutdown_token.clone(),
        state.clone(),
        |s| async move {
            match crate::services::ops::maintenance::cleanup_expired_completed_upload_sessions(
                s.get_ref(),
            )
            .await
            {
                Ok(stats) if stats.completed_sessions_deleted > 0 => {
                    tracing::info!(
                        deleted = stats.completed_sessions_deleted,
                        broken = stats.broken_completed_sessions_deleted,
                        "cleaned up expired completed upload sessions"
                    );
                    crate::services::task::RuntimeTaskRunOutcome::succeeded(Some(format!(
                        "deleted {} completed sessions ({} broken)",
                        stats.completed_sessions_deleted, stats.broken_completed_sessions_deleted
                    )))
                }
                Ok(_) => crate::services::task::RuntimeTaskRunOutcome::quiet(),
                Err(error) => {
                    tracing::warn!("completed upload session cleanup failed: {error}");
                    crate::services::task::RuntimeTaskRunOutcome::failed(
                        Some("Completed upload cleanup failed".to_string()),
                        error.to_string(),
                    )
                }
            }
        },
    ));

    // Full-table blob reconciliation is intentionally less frequent than lightweight cleanups.
    tasks.push(spawn_periodic(
        SystemRuntimeTaskKind::BlobReconcile,
        blob_reconcile_interval,
        None,
        shutdown_token.clone(),
        state.clone(),
        |s| async move {
            match crate::services::ops::maintenance::reconcile_blob_state(s.get_ref()).await {
                Ok(stats) if stats.ref_count_fixed > 0 || stats.orphan_blobs_deleted > 0 => {
                    tracing::info!(
                        ref_count_fixed = stats.ref_count_fixed,
                        orphan_blobs_deleted = stats.orphan_blobs_deleted,
                        "reconciled blob state"
                    );
                    crate::services::task::RuntimeTaskRunOutcome::succeeded(Some(format!(
                        "fixed {} ref counts, deleted {} orphan blobs",
                        stats.ref_count_fixed, stats.orphan_blobs_deleted
                    )))
                }
                Ok(_) => crate::services::task::RuntimeTaskRunOutcome::quiet(),
                Err(error) => {
                    tracing::warn!("blob reconcile failed: {error}");
                    crate::services::task::RuntimeTaskRunOutcome::failed(
                        Some("Blob reconcile failed".to_string()),
                        error.to_string(),
                    )
                }
            }
        },
    ));

    tasks.push(spawn_periodic(
        SystemRuntimeTaskKind::SystemHealthCheck,
        system_health_check_interval,
        None,
        shutdown_token.clone(),
        state.clone(),
        |s| async move {
            let report =
                crate::services::ops::health::run_primary_system_health_checks(s.get_ref()).await;
            if report.has_issues() {
                tracing::warn!(
                    details = %report.details(),
                    "system health check found unhealthy components"
                );
            } else {
                tracing::info!(
                    summary = %report.summary(),
                    "system health check completed"
                );
            }
            report.into_runtime_outcome()
        },
    ));

    tasks.push(spawn_periodic(
        SystemRuntimeTaskKind::TrashCleanup,
        maintenance_cleanup_interval,
        Some(MAINTENANCE_CLEANUP_JITTER_CAP),
        shutdown_token.clone(),
        state.clone(),
        |s| async move {
            match crate::services::files::trash::cleanup_expired(s.get_ref()).await {
                Ok(count) if count > 0 => {
                    tracing::info!("cleaned up {count} expired trash entries");
                    crate::services::task::RuntimeTaskRunOutcome::succeeded(Some(format!(
                        "cleaned up {count} expired trash entries"
                    )))
                }
                Ok(_) => crate::services::task::RuntimeTaskRunOutcome::quiet(),
                Err(error) => {
                    tracing::warn!("trash cleanup failed: {error}");
                    crate::services::task::RuntimeTaskRunOutcome::failed(
                        Some("Trash cleanup failed".to_string()),
                        error.to_string(),
                    )
                }
            }
        },
    ));

    tasks.push(spawn_periodic(
        SystemRuntimeTaskKind::TeamArchiveCleanup,
        maintenance_cleanup_interval,
        Some(MAINTENANCE_CLEANUP_JITTER_CAP),
        shutdown_token.clone(),
        state.clone(),
        |s| async move {
            match crate::services::workspace::team::cleanup_expired_archived_teams(s.get_ref())
                .await
            {
                Ok(count) if count > 0 => {
                    tracing::info!("cleaned up {count} expired archived teams");
                    crate::services::task::RuntimeTaskRunOutcome::succeeded(Some(format!(
                        "cleaned up {count} expired archived teams"
                    )))
                }
                Ok(_) => crate::services::task::RuntimeTaskRunOutcome::quiet(),
                Err(error) => {
                    tracing::warn!("team archive cleanup failed: {error}");
                    crate::services::task::RuntimeTaskRunOutcome::failed(
                        Some("Team archive cleanup failed".to_string()),
                        error.to_string(),
                    )
                }
            }
        },
    ));

    tasks.push(spawn_periodic(
        SystemRuntimeTaskKind::LockCleanup,
        maintenance_cleanup_interval,
        Some(MAINTENANCE_CLEANUP_JITTER_CAP),
        shutdown_token.clone(),
        state.clone(),
        |s| async move {
            match crate::services::files::lock::cleanup_expired(s.get_ref()).await {
                Ok(count) if count > 0 => {
                    tracing::info!("cleaned up {count} expired locks");
                    crate::services::task::RuntimeTaskRunOutcome::succeeded(Some(format!(
                        "cleaned up {count} expired locks"
                    )))
                }
                Ok(_) => crate::services::task::RuntimeTaskRunOutcome::quiet(),
                Err(error) => {
                    tracing::warn!("lock cleanup failed: {error}");
                    crate::services::task::RuntimeTaskRunOutcome::failed(
                        Some("Lock cleanup failed".to_string()),
                        error.to_string(),
                    )
                }
            }
        },
    ));

    tasks.push(spawn_periodic(
        SystemRuntimeTaskKind::AuthSessionCleanup,
        maintenance_cleanup_interval,
        Some(MAINTENANCE_CLEANUP_JITTER_CAP),
        shutdown_token.clone(),
        state.clone(),
        |s| async move {
            match crate::services::auth::local::cleanup_expired_auth_sessions(s.get_ref()).await {
                Ok(count) if count > 0 => {
                    tracing::info!("cleaned up {count} expired auth sessions");
                    crate::services::task::RuntimeTaskRunOutcome::succeeded(Some(format!(
                        "cleaned up {count} expired auth sessions"
                    )))
                }
                Ok(_) => crate::services::task::RuntimeTaskRunOutcome::quiet(),
                Err(error) => {
                    tracing::warn!("auth session cleanup failed: {error}");
                    crate::services::task::RuntimeTaskRunOutcome::failed(
                        Some("Auth session cleanup failed".to_string()),
                        error.to_string(),
                    )
                }
            }
        },
    ));

    tasks.push(spawn_periodic(
        SystemRuntimeTaskKind::ExternalAuthFlowCleanup,
        maintenance_cleanup_interval,
        Some(MAINTENANCE_CLEANUP_JITTER_CAP),
        shutdown_token.clone(),
        state.clone(),
        |s| async move {
            match crate::services::auth::external::cleanup_expired_flows(s.get_ref()).await {
                Ok(count) if count > 0 => {
                    tracing::info!("cleaned up {count} expired external auth flows");
                    crate::services::task::RuntimeTaskRunOutcome::succeeded(Some(format!(
                        "cleaned up {count} expired external auth flows"
                    )))
                }
                Ok(_) => crate::services::task::RuntimeTaskRunOutcome::quiet(),
                Err(error) => {
                    tracing::warn!("external auth flow cleanup failed: {error}");
                    crate::services::task::RuntimeTaskRunOutcome::failed(
                        Some("External auth flow cleanup failed".to_string()),
                        error.to_string(),
                    )
                }
            }
        },
    ));

    tasks.push(spawn_periodic(
        SystemRuntimeTaskKind::MfaFlowCleanup,
        maintenance_cleanup_interval,
        Some(MAINTENANCE_CLEANUP_JITTER_CAP),
        shutdown_token.clone(),
        state.clone(),
        |s| async move {
            match crate::services::auth::mfa::cleanup_expired_flows(s.get_ref()).await {
                Ok(count) if count > 0 => {
                    tracing::info!("cleaned up {count} expired MFA flows");
                    crate::services::task::RuntimeTaskRunOutcome::succeeded(Some(format!(
                        "cleaned up {count} expired MFA flows"
                    )))
                }
                Ok(_) => crate::services::task::RuntimeTaskRunOutcome::quiet(),
                Err(error) => {
                    tracing::warn!("MFA flow cleanup failed: {error}");
                    crate::services::task::RuntimeTaskRunOutcome::failed(
                        Some("MFA flow cleanup failed".to_string()),
                        error.to_string(),
                    )
                }
            }
        },
    ));

    tasks.push(spawn_periodic(
        SystemRuntimeTaskKind::AuditCleanup,
        maintenance_cleanup_interval,
        Some(MAINTENANCE_CLEANUP_JITTER_CAP),
        shutdown_token.clone(),
        state.clone(),
        |s| async move {
            match crate::services::ops::audit::cleanup_expired(s.get_ref()).await {
                Ok(count) if count > 0 => crate::services::task::RuntimeTaskRunOutcome::succeeded(
                    Some(format!("cleaned up {count} expired audit log entries")),
                ),
                Ok(_) => crate::services::task::RuntimeTaskRunOutcome::quiet(),
                Err(error) => {
                    tracing::warn!("audit log cleanup failed: {error}");
                    crate::services::task::RuntimeTaskRunOutcome::failed(
                        Some("Audit log cleanup failed".to_string()),
                        error.to_string(),
                    )
                }
            }
        },
    ));

    tasks.push(spawn_periodic(
        SystemRuntimeTaskKind::TaskCleanup,
        maintenance_cleanup_interval,
        Some(MAINTENANCE_CLEANUP_JITTER_CAP),
        shutdown_token.clone(),
        state.clone(),
        |s| async move {
            // task-cleanup 只清理过期任务产物，不删任务记录。
            // 也就是说 admin/tasks 里的历史事件仍然保留，只是 temp 目录会被回收。
            match crate::services::task::cleanup_expired(s.get_ref()).await {
                Ok(count) if count > 0 => {
                    tracing::info!("cleaned up {count} expired task artifacts");
                    crate::services::task::RuntimeTaskRunOutcome::succeeded(Some(format!(
                        "cleaned up {count} expired task artifacts"
                    )))
                }
                Ok(_) => crate::services::task::RuntimeTaskRunOutcome::quiet(),
                Err(error) => {
                    tracing::warn!("background task cleanup failed: {error}");
                    crate::services::task::RuntimeTaskRunOutcome::failed(
                        Some("Task artifact cleanup failed".to_string()),
                        error.to_string(),
                    )
                }
            }
        },
    ));

    tasks.push(spawn_periodic(
        SystemRuntimeTaskKind::WopiSessionCleanup,
        maintenance_cleanup_interval,
        Some(MAINTENANCE_CLEANUP_JITTER_CAP),
        shutdown_token,
        state,
        |s| async move {
            match crate::services::preview::wopi::cleanup_expired(s.get_ref()).await {
                Ok(count) if count > 0 => {
                    tracing::info!("cleaned up {count} expired WOPI sessions");
                    crate::services::task::RuntimeTaskRunOutcome::succeeded(Some(format!(
                        "cleaned up {count} expired WOPI sessions"
                    )))
                }
                Ok(_) => crate::services::task::RuntimeTaskRunOutcome::quiet(),
                Err(error) => {
                    tracing::warn!("WOPI session cleanup failed: {error}");
                    crate::services::task::RuntimeTaskRunOutcome::failed(
                        Some("WOPI session cleanup failed".to_string()),
                        error.to_string(),
                    )
                }
            }
        },
    ));

    tasks
}

/// Spawn only follower-safe background tasks.
pub fn spawn_follower_background_tasks(
    state: web::Data<FollowerAppState>,
    shutdown_token: CancellationToken,
) -> BackgroundTasks {
    tracing::info!("follower mode enabled; skipping primary-only background tasks");
    let mut tasks = build_background_tasks_base(&state.metrics, shutdown_token);
    let shutdown_token = tasks.shutdown_token();
    tasks.push(
        crate::storage::remote_protocol::tunnel::client::run_follower_tunnel_worker(
            state,
            shutdown_token,
        ),
    );
    tasks
}

fn mail_outbox_dispatch_interval(state: &PrimaryAppState) -> Duration {
    Duration::from_secs(
        crate::config::operations::mail_outbox_dispatch_interval_secs(&state.runtime_config),
    )
}

fn background_task_dispatch_interval(state: &PrimaryAppState) -> Duration {
    Duration::from_secs(
        crate::config::operations::background_task_dispatch_interval_secs(&state.runtime_config),
    )
}

fn background_task_dispatch_idle_max_interval(state: &PrimaryAppState) -> Duration {
    Duration::from_secs(
        crate::config::operations::background_task_dispatch_idle_max_interval_secs(
            &state.runtime_config,
        ),
    )
}

fn maintenance_cleanup_interval(state: &PrimaryAppState) -> Duration {
    Duration::from_secs(
        crate::config::operations::maintenance_cleanup_interval_secs(&state.runtime_config),
    )
}

fn blob_reconcile_interval(state: &PrimaryAppState) -> Duration {
    Duration::from_secs(crate::config::operations::blob_reconcile_interval_secs(
        &state.runtime_config,
    ))
}

fn system_health_check_interval(state: &PrimaryAppState) -> Duration {
    Duration::from_secs(
        crate::config::operations::remote_node_health_test_interval_secs(&state.runtime_config),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::definitions::CONFIG_CATEGORY_RUNTIME_BACKGROUND_TASK;
    use crate::config::operations::{
        BACKGROUND_TASK_DISPATCH_IDLE_MAX_INTERVAL_SECS_KEY,
        BACKGROUND_TASK_DISPATCH_INTERVAL_SECS_KEY, BLOB_RECONCILE_INTERVAL_SECS_KEY,
        MAIL_OUTBOX_DISPATCH_INTERVAL_SECS_KEY, MAINTENANCE_CLEANUP_INTERVAL_SECS_KEY,
        REMOTE_NODE_HEALTH_TEST_INTERVAL_SECS_KEY,
    };
    use crate::errors::AsterError;
    use crate::runtime::SharedRuntimeState;
    use crate::services::task::{DispatchStats, RuntimeTaskRunOutcome};
    use chrono::Utc;
    use migration::Migrator;
    use sea_orm::{ActiveModelTrait, EntityTrait, Set};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    async fn setup_state() -> web::Data<PrimaryAppState> {
        let db = crate::db::connect_with_metrics(
            &crate::config::DatabaseConfig {
                url: "sqlite::memory:".to_string(),
                pool_size: 1,
                retry_count: 0,
            },
            crate::metrics_core::NoopMetrics::arc(),
        )
        .await
        .unwrap();
        Migrator::up(&db, None).await.unwrap();
        crate::db::repository::config_repo::ensure_defaults_with_env(&db, &|_| None)
            .await
            .unwrap();

        let cache = aster_forge_cache::create_cache(&crate::config::CacheConfig {
            ..Default::default()
        })
        .await;
        let runtime_config = Arc::new(crate::config::RuntimeConfig::new());
        runtime_config.reload(&db).await.unwrap();
        let (storage_change_tx, _) = tokio::sync::broadcast::channel(
            crate::services::events::storage_change::STORAGE_CHANGE_CHANNEL_CAPACITY,
        );
        let (share_download_rollback, _worker) =
            crate::services::share::build_share_download_rollback_queue(
                db.clone(),
                1,
                crate::metrics_core::NoopMetrics::arc(),
            );

        web::Data::new(PrimaryAppState {
            db_handles: crate::db::DbHandles::single(db),
            driver_registry: Arc::new(crate::storage::DriverRegistry::noop()),
            runtime_config,
            policy_snapshot: Arc::new(crate::storage::PolicySnapshot::new()),
            config: Arc::new(crate::config::Config::default()),
            cache,
            metrics: crate::metrics_core::NoopMetrics::arc(),
            mail_sender: crate::services::mail::sender::memory_sender(),
            storage_change_tx,
            share_download_rollback,
            background_task_dispatch_wakeup:
                crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
            remote_protocol: crate::runtime::PrimaryAppState::new_remote_protocol(),
        })
    }

    fn apply_runtime_value(state: &web::Data<PrimaryAppState>, key: &str, value: &str) {
        state
            .runtime_config
            .apply(crate::entities::system_config::Model {
                id: 0,
                key: key.to_string(),
                value: value.to_string(),
                value_type: crate::types::SystemConfigValueType::String,
                requires_restart: false,
                is_sensitive: false,
                source: crate::types::SystemConfigSource::System,
                visibility: crate::types::SystemConfigVisibility::Private,
                namespace: String::new(),
                category: CONFIG_CATEGORY_RUNTIME_BACKGROUND_TASK.to_string(),
                description: "test".to_string(),
                updated_at: Utc::now(),
                updated_by: None,
            });
    }

    #[test]
    fn periodic_sleep_duration_is_unchanged_without_jitter() {
        let base = Duration::from_secs(5);
        assert_eq!(periodic_sleep_duration(base, None), base);
    }

    #[test]
    fn periodic_sleep_duration_caps_jitter_to_ten_percent_of_interval() {
        let base = Duration::from_secs(5);
        let cap = Duration::from_secs(30);

        for _ in 0..64 {
            let delay = periodic_sleep_duration(base, Some(cap));
            assert!(delay >= base);
            assert!(delay <= base + Duration::from_millis(500));
        }
    }

    #[test]
    fn periodic_sleep_duration_uses_requested_cap_when_it_is_smaller() {
        let base = Duration::from_secs(3600);
        let cap = Duration::from_secs(30);

        for _ in 0..64 {
            let delay = periodic_sleep_duration(base, Some(cap));
            assert!(delay >= base);
            assert!(delay <= base + cap);
        }
    }

    #[tokio::test]
    async fn shutdown_only_awaits_each_handle_once() {
        let mut tasks = BackgroundTasks::new();
        tasks.push(async {});

        tasks.shutdown().await;
    }

    #[tokio::test]
    async fn external_shutdown_token_stops_background_worker_before_shutdown_join() {
        let shutdown_token = CancellationToken::new();
        let mut tasks = BackgroundTasks::with_shutdown_token(shutdown_token.clone());
        let (stopped_tx, stopped_rx) = tokio::sync::oneshot::channel();

        tasks.push({
            let shutdown_token = shutdown_token.clone();
            async move {
                shutdown_token.cancelled().await;
                let _ = stopped_tx.send(());
            }
        });

        shutdown_token.cancel();
        tokio::time::timeout(Duration::from_millis(50), stopped_rx)
            .await
            .expect("background worker should observe external shutdown")
            .expect("background worker should report shutdown");

        tasks.shutdown().await;
    }

    #[tokio::test]
    async fn pre_cancelled_shutdown_token_skips_periodic_startup_iteration() {
        let state = setup_state().await;
        let shutdown_token = CancellationToken::new();
        let calls = Arc::new(AtomicUsize::new(0));
        shutdown_token.cancel();

        spawn_periodic(
            SystemRuntimeTaskKind::MailOutboxDispatch,
            |_| Duration::from_secs(60),
            None,
            shutdown_token,
            state.clone(),
            {
                let calls = calls.clone();
                move |_| {
                    let calls = calls.clone();
                    async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        RuntimeTaskRunOutcome::succeeded(None)
                    }
                }
            },
        )
        .await;

        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let tasks = crate::entities::background_task::Entity::find()
            .all(state.writer_db())
            .await
            .unwrap();
        assert!(tasks.is_empty());
    }

    #[tokio::test]
    async fn pre_cancelled_shutdown_token_skips_dispatcher_startup_iteration() {
        let state = setup_state().await;
        let shutdown_token = CancellationToken::new();
        shutdown_token.cancel();

        spawn_background_task_dispatcher(shutdown_token, state.clone()).await;

        let tasks = crate::entities::background_task::Entity::find()
            .all(state.writer_db())
            .await
            .unwrap();
        assert!(tasks.is_empty());
    }

    #[tokio::test]
    async fn follower_background_tasks_can_shutdown_without_primary_workers() {
        let state = setup_state().await;
        let tasks = spawn_follower_background_tasks(
            web::Data::new(state.follower_view()),
            CancellationToken::new(),
        );

        tasks.shutdown().await;
    }

    #[tokio::test]
    async fn runtime_interval_helpers_read_runtime_config_values() {
        let state = setup_state().await;
        apply_runtime_value(&state, MAIL_OUTBOX_DISPATCH_INTERVAL_SECS_KEY, "11");
        apply_runtime_value(&state, BACKGROUND_TASK_DISPATCH_INTERVAL_SECS_KEY, "12");
        apply_runtime_value(
            &state,
            BACKGROUND_TASK_DISPATCH_IDLE_MAX_INTERVAL_SECS_KEY,
            "30",
        );
        apply_runtime_value(&state, MAINTENANCE_CLEANUP_INTERVAL_SECS_KEY, "13");
        apply_runtime_value(&state, BLOB_RECONCILE_INTERVAL_SECS_KEY, "14");
        apply_runtime_value(&state, REMOTE_NODE_HEALTH_TEST_INTERVAL_SECS_KEY, "15");

        assert_eq!(
            mail_outbox_dispatch_interval(&state),
            Duration::from_secs(11)
        );
        assert_eq!(
            background_task_dispatch_interval(&state),
            Duration::from_secs(12)
        );
        assert_eq!(
            background_task_dispatch_idle_max_interval(&state),
            Duration::from_secs(30)
        );
        assert_eq!(
            maintenance_cleanup_interval(&state),
            Duration::from_secs(13)
        );
        assert_eq!(blob_reconcile_interval(&state), Duration::from_secs(14));
        assert_eq!(
            system_health_check_interval(&state),
            Duration::from_secs(15)
        );
    }

    #[test]
    fn background_task_dispatch_success_is_quiet_even_when_tasks_were_processed() {
        let outcome = background_task_dispatch_outcome(Ok(DispatchStats {
            claimed: 1,
            succeeded: 1,
            retried: 0,
            failed: 1,
        }));

        assert_eq!(outcome, RuntimeTaskRunOutcome::quiet());
    }

    #[test]
    fn background_task_dispatch_failure_is_recorded() {
        let outcome =
            background_task_dispatch_outcome(Err(AsterError::internal_error("dispatcher crashed")));

        assert_eq!(
            outcome,
            RuntimeTaskRunOutcome::failed(
                Some("Background task dispatch failed".to_string()),
                "Internal Server Error: dispatcher crashed",
            )
        );
    }

    async fn insert_pending_system_runtime_task(
        state: &web::Data<PrimaryAppState>,
    ) -> crate::entities::background_task::Model {
        let now = Utc::now();
        crate::entities::background_task::ActiveModel {
            kind: Set(crate::types::BackgroundTaskKind::SystemRuntime),
            status: Set(crate::types::BackgroundTaskStatus::Pending),
            creator_user_id: Set(None),
            team_id: Set(None),
            share_id: Set(None),
            display_name: Set("dispatch runtime task".to_string()),
            payload_json: Set(crate::types::StoredTaskPayload(
                serde_json::json!({"task_name": "background-task-dispatch"}).to_string(),
            )),
            result_json: Set(None),
            runtime_json: Set(None),
            steps_json: Set(None),
            progress_current: Set(0),
            progress_total: Set(1),
            status_text: Set(None),
            attempt_count: Set(0),
            max_attempts: Set(1),
            next_run_at: Set(now - chrono::Duration::seconds(1)),
            processing_token: Set(0),
            processing_started_at: Set(None),
            last_heartbeat_at: Set(None),
            lease_expires_at: Set(None),
            started_at: Set(None),
            finished_at: Set(None),
            last_error: Set(None),
            failure_can_retry: Set(None),
            expires_at: Set(now + chrono::Duration::hours(1)),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(state.writer_db())
        .await
        .expect("pending runtime task should insert")
    }

    #[tokio::test]
    async fn background_task_dispatch_iteration_is_idle_for_empty_queue() {
        let state = setup_state().await;

        let iteration =
            run_background_task_dispatch_iteration(&state, CancellationToken::new()).await;

        assert_eq!(iteration, BackgroundTaskDispatchIteration::idle());
        let tasks = crate::entities::background_task::Entity::find()
            .all(state.writer_db())
            .await
            .unwrap();
        assert!(tasks.is_empty());
    }

    #[tokio::test]
    async fn background_task_dispatch_iteration_is_active_when_task_was_processed() {
        let state = setup_state().await;
        let task = insert_pending_system_runtime_task(&state).await;

        let iteration =
            run_background_task_dispatch_iteration(&state, CancellationToken::new()).await;

        assert_eq!(iteration, BackgroundTaskDispatchIteration::active());
        let stored =
            crate::db::repository::background_task_repo::find_by_id(state.writer_db(), task.id)
                .await
                .unwrap();
        assert_eq!(stored.status, crate::types::BackgroundTaskStatus::Failed);
        assert!(
            stored
                .last_error
                .as_deref()
                .is_some_and(|error| error.contains("should not be dispatched"))
        );
    }

    #[test]
    fn background_task_dispatch_zero_base_interval_uses_minimum_delay() {
        let base = Duration::ZERO;
        let max = Duration::from_secs(30);
        let mut backoff = BackgroundTaskDispatchBackoff::new(base, max);

        assert_eq!(backoff.sleep_duration(base, max), Duration::from_secs(1));

        backoff.record_iteration(
            BackgroundTaskDispatchTrigger::Timer,
            BackgroundTaskDispatchIteration::idle(),
            base,
            max,
        );
        assert_eq!(backoff.sleep_duration(base, max), Duration::from_secs(2));
    }

    #[test]
    fn background_task_dispatch_backoff_grows_on_idle_and_caps() {
        let base = Duration::from_secs(5);
        let max = Duration::from_secs(30);
        let mut backoff = BackgroundTaskDispatchBackoff::new(base, max);

        assert_eq!(backoff.sleep_duration(base, max), base);

        backoff.record_iteration(
            BackgroundTaskDispatchTrigger::Timer,
            BackgroundTaskDispatchIteration::idle(),
            base,
            max,
        );
        assert_eq!(backoff.sleep_duration(base, max), Duration::from_secs(10));

        backoff.record_iteration(
            BackgroundTaskDispatchTrigger::Timer,
            BackgroundTaskDispatchIteration::idle(),
            base,
            max,
        );
        assert_eq!(backoff.sleep_duration(base, max), Duration::from_secs(20));

        backoff.record_iteration(
            BackgroundTaskDispatchTrigger::Timer,
            BackgroundTaskDispatchIteration::idle(),
            base,
            max,
        );
        assert_eq!(backoff.sleep_duration(base, max), max);

        backoff.record_iteration(
            BackgroundTaskDispatchTrigger::Timer,
            BackgroundTaskDispatchIteration::idle(),
            base,
            max,
        );
        assert_eq!(backoff.sleep_duration(base, max), max);
    }

    #[test]
    fn background_task_dispatch_backoff_resets_on_wakeup_and_activity() {
        let base = Duration::from_secs(5);
        let max = Duration::from_secs(60);
        let mut backoff = BackgroundTaskDispatchBackoff::new(base, max);

        backoff.record_iteration(
            BackgroundTaskDispatchTrigger::Timer,
            BackgroundTaskDispatchIteration::idle(),
            base,
            max,
        );
        backoff.record_iteration(
            BackgroundTaskDispatchTrigger::Timer,
            BackgroundTaskDispatchIteration::idle(),
            base,
            max,
        );
        assert_eq!(backoff.sleep_duration(base, max), Duration::from_secs(20));

        backoff.record_iteration(
            BackgroundTaskDispatchTrigger::Wakeup,
            BackgroundTaskDispatchIteration::idle(),
            base,
            max,
        );
        assert_eq!(backoff.sleep_duration(base, max), base);

        backoff.record_iteration(
            BackgroundTaskDispatchTrigger::Timer,
            BackgroundTaskDispatchIteration::idle(),
            base,
            max,
        );
        assert_eq!(backoff.sleep_duration(base, max), Duration::from_secs(10));

        backoff.record_iteration(
            BackgroundTaskDispatchTrigger::Timer,
            BackgroundTaskDispatchIteration::active(),
            base,
            max,
        );
        assert_eq!(backoff.sleep_duration(base, max), base);
    }

    #[test]
    fn background_task_dispatch_backoff_never_polls_faster_than_normal_after_error() {
        let base = Duration::from_secs(30);
        let max = Duration::from_secs(120);
        let mut backoff = BackgroundTaskDispatchBackoff::new(base, max);

        backoff.record_iteration(
            BackgroundTaskDispatchTrigger::Timer,
            BackgroundTaskDispatchIteration::failed(),
            base,
            max,
        );
        assert_eq!(backoff.sleep_duration(base, max), base);

        let short_base = Duration::from_secs(1);
        let mut short_backoff = BackgroundTaskDispatchBackoff::new(short_base, max);
        short_backoff.record_iteration(
            BackgroundTaskDispatchTrigger::Timer,
            BackgroundTaskDispatchIteration::failed(),
            short_base,
            max,
        );
        assert_eq!(
            short_backoff.sleep_duration(short_base, max),
            BACKGROUND_TASK_DISPATCH_ERROR_BACKOFF_CAP
        );

        backoff.record_iteration(
            BackgroundTaskDispatchTrigger::Timer,
            BackgroundTaskDispatchIteration::idle(),
            base,
            max,
        );
        assert_eq!(backoff.sleep_duration(base, max), Duration::from_secs(60));
    }

    #[tokio::test]
    async fn run_periodic_iteration_records_successful_runtime_outcome() {
        let state = setup_state().await;

        run_periodic_iteration(
            SystemRuntimeTaskKind::MailOutboxDispatch,
            &state,
            &|_| async { RuntimeTaskRunOutcome::succeeded(Some("ok".to_string())) },
        )
        .await;

        let tasks = crate::entities::background_task::Entity::find()
            .all(state.writer_db())
            .await
            .unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].display_name, "Mail outbox dispatch");
        assert_eq!(
            tasks[0].status,
            crate::types::BackgroundTaskStatus::Succeeded
        );
        assert_eq!(tasks[0].status_text.as_deref(), Some("ok"));
        let result: serde_json::Value =
            serde_json::from_str(tasks[0].result_json.as_ref().unwrap().as_ref()).unwrap();
        assert_eq!(result["summary"], "ok");
        assert!(result["duration_ms"].as_i64().is_some());
    }

    #[tokio::test]
    async fn run_periodic_iteration_catches_panics_and_records_failure() {
        let state = setup_state().await;

        run_periodic_iteration(
            SystemRuntimeTaskKind::BackgroundTaskDispatch,
            &state,
            &|_| async { panic!("runtime task exploded") },
        )
        .await;

        let tasks = crate::entities::background_task::Entity::find()
            .all(state.writer_db())
            .await
            .unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].display_name, "Background task dispatch");
        assert_eq!(tasks[0].status, crate::types::BackgroundTaskStatus::Failed);
        assert_eq!(tasks[0].status_text.as_deref(), Some("Task panicked"));
        assert_eq!(
            tasks[0].last_error.as_deref(),
            Some("runtime task exploded")
        );
        assert_eq!(tasks[0].failure_can_retry, Some(false));
    }
}

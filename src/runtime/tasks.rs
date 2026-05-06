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

use super::PrimaryAppState;
use crate::services::share_service::ShareDownloadRollbackWorker;
use crate::utils::numbers::u128_to_u64;

const BACKGROUND_TASK_SHUTDOWN_GRACE: Duration = Duration::from_secs(30);
const MAINTENANCE_CLEANUP_JITTER_CAP: Duration = Duration::from_secs(30);

pub struct BackgroundTasks {
    shutdown_token: CancellationToken,
    handles: JoinSet<()>,
}

impl BackgroundTasks {
    fn new() -> Self {
        Self {
            shutdown_token: CancellationToken::new(),
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
    name: &'static str,
    interval_fn: I,
    jitter_cap: Option<Duration>,
    shutdown_token: CancellationToken,
    state: web::Data<PrimaryAppState>,
    task_fn: F,
) where
    I: Fn(&PrimaryAppState) -> Duration + Send + Sync + 'static,
    F: Fn(web::Data<PrimaryAppState>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = crate::services::task_service::RuntimeTaskRunOutcome> + Send + 'static,
{
    // 每轮迭代独立 span，并发清理任务在 trace 里可按 task.name 区分。
    // 必须用 `Instrument::instrument` 而非 `Span::enter`：后者返回的 guard
    // 跨 await 会被 drop（tracing 文档警告），span 只对同步段生效，
    // 而我们的 task_fn 全是 async 跨 await 的。
    run_periodic_iteration(name, &state, &task_fn)
        .instrument(tracing::info_span!("bg_task", task.name = name))
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
            .instrument(tracing::info_span!("bg_task", task.name = name))
            .await;
    }
}

async fn run_periodic_iteration<F, Fut>(
    name: &'static str,
    state: &web::Data<PrimaryAppState>,
    task_fn: &F,
) where
    F: Fn(web::Data<PrimaryAppState>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = crate::services::task_service::RuntimeTaskRunOutcome> + Send + 'static,
{
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
            tracing::error!("background task '{name}' panicked: {panic_message}");
            crate::services::task_service::RuntimeTaskRunOutcome::failed(
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
    if let Err(error) = crate::services::task_service::record_runtime_task_run(
        state.get_ref(),
        name,
        started_at,
        finished_at,
        &outcome,
    )
    .await
    {
        tracing::warn!("failed to record runtime task '{name}': {error}");
    }
}

fn background_task_dispatch_outcome(
    result: crate::errors::Result<crate::services::task_service::DispatchStats>,
) -> crate::services::task_service::RuntimeTaskRunOutcome {
    match result {
        Ok(stats) => {
            if stats.claimed > 0 || stats.failed > 0 {
                tracing::info!(
                    claimed = stats.claimed,
                    succeeded = stats.succeeded,
                    retried = stats.retried,
                    failed = stats.failed,
                    "processed background task batch"
                );
            }
            crate::services::task_service::RuntimeTaskRunOutcome::quiet()
        }
        Err(error) => {
            tracing::warn!("background task dispatch failed: {error}");
            crate::services::task_service::RuntimeTaskRunOutcome::failed(
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

fn build_background_tasks_base() -> BackgroundTasks {
    #[cfg(not(feature = "metrics"))]
    {
        BackgroundTasks::new()
    }

    #[cfg(feature = "metrics")]
    {
        let mut tasks = BackgroundTasks::new();
        tasks.push(crate::metrics::system_metrics_updater_task(
            tasks.shutdown_token(),
        ));

        tasks
    }
}

/// Spawn all primary-only periodic background cleanup tasks.
pub fn spawn_primary_background_tasks(
    state: web::Data<PrimaryAppState>,
    share_download_rollback_worker: ShareDownloadRollbackWorker,
) -> BackgroundTasks {
    let mut tasks = build_background_tasks_base();
    let shutdown_token = tasks.shutdown_token();

    tasks.push(
        crate::services::share_service::share_download_rollback_worker_task(
            shutdown_token.clone(),
            share_download_rollback_worker,
        )
        .instrument(tracing::info_span!(
            "bg_task",
            task.name = "share-download-rollback"
        )),
    );

    tasks.push(spawn_periodic(
        "mail-outbox-dispatch",
        mail_outbox_dispatch_interval,
        None,
        shutdown_token.clone(),
        state.clone(),
        |s| async move {
            match crate::services::mail_outbox_service::dispatch_due(&s).await {
                Ok(stats) if stats.claimed > 0 || stats.failed > 0 => {
                    tracing::info!(
                        claimed = stats.claimed,
                        sent = stats.sent,
                        retried = stats.retried,
                        failed = stats.failed,
                        "processed mail outbox batch"
                    );
                    crate::services::task_service::RuntimeTaskRunOutcome::succeeded(Some(format!(
                        "claimed {}, sent {}, retried {}, failed {}",
                        stats.claimed, stats.sent, stats.retried, stats.failed
                    )))
                }
                Ok(_) => crate::services::task_service::RuntimeTaskRunOutcome::quiet(),
                Err(error) => {
                    tracing::warn!("mail outbox dispatch failed: {error}");
                    crate::services::task_service::RuntimeTaskRunOutcome::failed(
                        Some("Mail outbox dispatch failed".to_string()),
                        error.to_string(),
                    )
                }
            }
        },
    ));

    tasks.push(spawn_periodic(
        "background-task-dispatch",
        background_task_dispatch_interval,
        None,
        shutdown_token.clone(),
        state.clone(),
        |s| async move {
            // 这是后台任务主调度器：
            // 周期性捞取 Pending / Retry / stale Processing 任务，并发起认领与执行。
            background_task_dispatch_outcome(crate::services::task_service::dispatch_due(&s).await)
        },
    ));

    tasks.push(spawn_periodic(
        "upload-cleanup",
        maintenance_cleanup_interval,
        Some(MAINTENANCE_CLEANUP_JITTER_CAP),
        shutdown_token.clone(),
        state.clone(),
        |s| async move {
            match crate::services::upload_service::cleanup_expired(&s).await {
                Ok(count) if count > 0 => {
                    tracing::info!("cleaned up {count} expired upload sessions");
                    crate::services::task_service::RuntimeTaskRunOutcome::succeeded(Some(format!(
                        "cleaned up {count} expired upload sessions"
                    )))
                }
                Ok(_) => crate::services::task_service::RuntimeTaskRunOutcome::quiet(),
                Err(error) => {
                    tracing::warn!("upload cleanup failed: {error}");
                    crate::services::task_service::RuntimeTaskRunOutcome::failed(
                        Some("Upload cleanup failed".to_string()),
                        error.to_string(),
                    )
                }
            }
        },
    ));

    tasks.push(spawn_periodic(
        "completed-upload-cleanup",
        maintenance_cleanup_interval,
        Some(MAINTENANCE_CLEANUP_JITTER_CAP),
        shutdown_token.clone(),
        state.clone(),
        |s| async move {
            match crate::services::maintenance_service::cleanup_expired_completed_upload_sessions(
                &s,
            )
            .await
            {
                Ok(stats) if stats.completed_sessions_deleted > 0 => {
                    tracing::info!(
                        deleted = stats.completed_sessions_deleted,
                        broken = stats.broken_completed_sessions_deleted,
                        "cleaned up expired completed upload sessions"
                    );
                    crate::services::task_service::RuntimeTaskRunOutcome::succeeded(Some(format!(
                        "deleted {} completed sessions ({} broken)",
                        stats.completed_sessions_deleted, stats.broken_completed_sessions_deleted
                    )))
                }
                Ok(_) => crate::services::task_service::RuntimeTaskRunOutcome::quiet(),
                Err(error) => {
                    tracing::warn!("completed upload session cleanup failed: {error}");
                    crate::services::task_service::RuntimeTaskRunOutcome::failed(
                        Some("Completed upload cleanup failed".to_string()),
                        error.to_string(),
                    )
                }
            }
        },
    ));

    // Full-table blob reconciliation is intentionally less frequent than lightweight cleanups.
    tasks.push(spawn_periodic(
        "blob-reconcile",
        blob_reconcile_interval,
        None,
        shutdown_token.clone(),
        state.clone(),
        |s| async move {
            match crate::services::maintenance_service::reconcile_blob_state(&s).await {
                Ok(stats) if stats.ref_count_fixed > 0 || stats.orphan_blobs_deleted > 0 => {
                    tracing::info!(
                        ref_count_fixed = stats.ref_count_fixed,
                        orphan_blobs_deleted = stats.orphan_blobs_deleted,
                        "reconciled blob state"
                    );
                    crate::services::task_service::RuntimeTaskRunOutcome::succeeded(Some(format!(
                        "fixed {} ref counts, deleted {} orphan blobs",
                        stats.ref_count_fixed, stats.orphan_blobs_deleted
                    )))
                }
                Ok(_) => crate::services::task_service::RuntimeTaskRunOutcome::quiet(),
                Err(error) => {
                    tracing::warn!("blob reconcile failed: {error}");
                    crate::services::task_service::RuntimeTaskRunOutcome::failed(
                        Some("Blob reconcile failed".to_string()),
                        error.to_string(),
                    )
                }
            }
        },
    ));

    tasks.push(spawn_periodic(
        "system-health-check",
        system_health_check_interval,
        None,
        shutdown_token.clone(),
        state.clone(),
        |s| async move {
            let report =
                crate::services::health_service::run_primary_system_health_checks(&s).await;
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
        "trash-cleanup",
        maintenance_cleanup_interval,
        Some(MAINTENANCE_CLEANUP_JITTER_CAP),
        shutdown_token.clone(),
        state.clone(),
        |s| async move {
            match crate::services::trash_service::cleanup_expired(&s).await {
                Ok(count) if count > 0 => {
                    tracing::info!("cleaned up {count} expired trash entries");
                    crate::services::task_service::RuntimeTaskRunOutcome::succeeded(Some(format!(
                        "cleaned up {count} expired trash entries"
                    )))
                }
                Ok(_) => crate::services::task_service::RuntimeTaskRunOutcome::quiet(),
                Err(error) => {
                    tracing::warn!("trash cleanup failed: {error}");
                    crate::services::task_service::RuntimeTaskRunOutcome::failed(
                        Some("Trash cleanup failed".to_string()),
                        error.to_string(),
                    )
                }
            }
        },
    ));

    tasks.push(spawn_periodic(
        "team-archive-cleanup",
        maintenance_cleanup_interval,
        Some(MAINTENANCE_CLEANUP_JITTER_CAP),
        shutdown_token.clone(),
        state.clone(),
        |s| async move {
            match crate::services::team_service::cleanup_expired_archived_teams(&s).await {
                Ok(count) if count > 0 => {
                    tracing::info!("cleaned up {count} expired archived teams");
                    crate::services::task_service::RuntimeTaskRunOutcome::succeeded(Some(format!(
                        "cleaned up {count} expired archived teams"
                    )))
                }
                Ok(_) => crate::services::task_service::RuntimeTaskRunOutcome::quiet(),
                Err(error) => {
                    tracing::warn!("team archive cleanup failed: {error}");
                    crate::services::task_service::RuntimeTaskRunOutcome::failed(
                        Some("Team archive cleanup failed".to_string()),
                        error.to_string(),
                    )
                }
            }
        },
    ));

    tasks.push(spawn_periodic(
        "lock-cleanup",
        maintenance_cleanup_interval,
        Some(MAINTENANCE_CLEANUP_JITTER_CAP),
        shutdown_token.clone(),
        state.clone(),
        |s| async move {
            match crate::services::lock_service::cleanup_expired(&s).await {
                Ok(count) if count > 0 => {
                    tracing::info!("cleaned up {count} expired locks");
                    crate::services::task_service::RuntimeTaskRunOutcome::succeeded(Some(format!(
                        "cleaned up {count} expired locks"
                    )))
                }
                Ok(_) => crate::services::task_service::RuntimeTaskRunOutcome::quiet(),
                Err(error) => {
                    tracing::warn!("lock cleanup failed: {error}");
                    crate::services::task_service::RuntimeTaskRunOutcome::failed(
                        Some("Lock cleanup failed".to_string()),
                        error.to_string(),
                    )
                }
            }
        },
    ));

    tasks.push(spawn_periodic(
        "auth-session-cleanup",
        maintenance_cleanup_interval,
        Some(MAINTENANCE_CLEANUP_JITTER_CAP),
        shutdown_token.clone(),
        state.clone(),
        |s| async move {
            match crate::services::auth_service::cleanup_expired_auth_sessions(&s).await {
                Ok(count) if count > 0 => {
                    tracing::info!("cleaned up {count} expired auth sessions");
                    crate::services::task_service::RuntimeTaskRunOutcome::succeeded(Some(format!(
                        "cleaned up {count} expired auth sessions"
                    )))
                }
                Ok(_) => crate::services::task_service::RuntimeTaskRunOutcome::quiet(),
                Err(error) => {
                    tracing::warn!("auth session cleanup failed: {error}");
                    crate::services::task_service::RuntimeTaskRunOutcome::failed(
                        Some("Auth session cleanup failed".to_string()),
                        error.to_string(),
                    )
                }
            }
        },
    ));

    tasks.push(spawn_periodic(
        "audit-cleanup",
        maintenance_cleanup_interval,
        Some(MAINTENANCE_CLEANUP_JITTER_CAP),
        shutdown_token.clone(),
        state.clone(),
        |s| async move {
            match crate::services::audit_service::cleanup_expired(&s).await {
                Ok(count) if count > 0 => {
                    crate::services::task_service::RuntimeTaskRunOutcome::succeeded(Some(format!(
                        "cleaned up {count} expired audit log entries"
                    )))
                }
                Ok(_) => crate::services::task_service::RuntimeTaskRunOutcome::quiet(),
                Err(error) => {
                    tracing::warn!("audit log cleanup failed: {error}");
                    crate::services::task_service::RuntimeTaskRunOutcome::failed(
                        Some("Audit log cleanup failed".to_string()),
                        error.to_string(),
                    )
                }
            }
        },
    ));

    tasks.push(spawn_periodic(
        "task-cleanup",
        maintenance_cleanup_interval,
        Some(MAINTENANCE_CLEANUP_JITTER_CAP),
        shutdown_token.clone(),
        state.clone(),
        |s| async move {
            // task-cleanup 只清理过期任务产物，不删任务记录。
            // 也就是说 admin/tasks 里的历史事件仍然保留，只是 temp 目录会被回收。
            match crate::services::task_service::cleanup_expired(&s).await {
                Ok(count) if count > 0 => {
                    tracing::info!("cleaned up {count} expired task artifacts");
                    crate::services::task_service::RuntimeTaskRunOutcome::succeeded(Some(format!(
                        "cleaned up {count} expired task artifacts"
                    )))
                }
                Ok(_) => crate::services::task_service::RuntimeTaskRunOutcome::quiet(),
                Err(error) => {
                    tracing::warn!("background task cleanup failed: {error}");
                    crate::services::task_service::RuntimeTaskRunOutcome::failed(
                        Some("Task artifact cleanup failed".to_string()),
                        error.to_string(),
                    )
                }
            }
        },
    ));

    tasks.push(spawn_periodic(
        "wopi-session-cleanup",
        maintenance_cleanup_interval,
        Some(MAINTENANCE_CLEANUP_JITTER_CAP),
        shutdown_token,
        state,
        |s| async move {
            match crate::services::wopi_service::cleanup_expired(&s).await {
                Ok(count) if count > 0 => {
                    tracing::info!("cleaned up {count} expired WOPI sessions");
                    crate::services::task_service::RuntimeTaskRunOutcome::succeeded(Some(format!(
                        "cleaned up {count} expired WOPI sessions"
                    )))
                }
                Ok(_) => crate::services::task_service::RuntimeTaskRunOutcome::quiet(),
                Err(error) => {
                    tracing::warn!("WOPI session cleanup failed: {error}");
                    crate::services::task_service::RuntimeTaskRunOutcome::failed(
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
pub fn spawn_follower_background_tasks() -> BackgroundTasks {
    tracing::info!("follower mode enabled; skipping primary-only background tasks");
    build_background_tasks_base()
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
    use crate::errors::AsterError;
    use crate::services::task_service::{DispatchStats, RuntimeTaskRunOutcome};

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
}

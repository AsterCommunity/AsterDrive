//! 运行时子模块：`tasks`。

use std::future::Future;
use std::time::Duration;

use actix_web::web;
use chrono::Utc;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use super::{FollowerAppState, PrimaryAppState, SharedRuntimeState};
use crate::services::share::ShareDownloadRollbackWorker;
use crate::services::task::SystemRuntimeTaskKind;
use aster_forge_tasks::BackgroundTasks;

const MAINTENANCE_CLEANUP_JITTER_CAP: Duration = Duration::from_secs(30);

async fn spawn_background_task_dispatcher(
    shutdown_token: CancellationToken,
    state: web::Data<PrimaryAppState>,
) {
    aster_forge_tasks::run_dispatch_worker(
        SystemRuntimeTaskKind::BackgroundTaskDispatch.as_str(),
        shutdown_token,
        state,
        |state| background_task_dispatch_interval(state.get_ref()),
        |state| background_task_dispatch_idle_max_interval(state.get_ref()),
        |state: web::Data<PrimaryAppState>| async move {
            state.background_task_dispatch_wakeup.notified().await;
        },
        run_background_task_dispatch_iteration,
    )
    .await;
}

async fn run_background_task_dispatch_iteration(
    state: web::Data<PrimaryAppState>,
    shutdown_token: CancellationToken,
) -> aster_forge_tasks::BackgroundTaskDispatchIteration {
    let iteration = std::sync::Arc::new(std::sync::Mutex::new(
        aster_forge_tasks::BackgroundTaskDispatchIteration::idle(),
    ));
    let iteration_for_record = iteration.clone();
    let iteration_for_panic = iteration.clone();
    aster_forge_tasks::run_recorded_task_iteration(
        SystemRuntimeTaskKind::BackgroundTaskDispatch,
        SystemRuntimeTaskKind::BackgroundTaskDispatch.as_str(),
        state,
        &move |state: web::Data<PrimaryAppState>| {
            let shutdown_token = shutdown_token.clone();
            let iteration_for_record = iteration_for_record.clone();
            async move {
                let result = crate::services::task::dispatch::dispatch_due_with_shutdown(
                    state.get_ref(),
                    shutdown_token,
                )
                .await;
                let value = match &result {
                    Ok(stats) if stats.has_activity() => {
                        aster_forge_tasks::BackgroundTaskDispatchIteration::active()
                    }
                    Ok(_) => aster_forge_tasks::BackgroundTaskDispatchIteration::idle(),
                    Err(_) => aster_forge_tasks::BackgroundTaskDispatchIteration::failed(),
                };
                match iteration_for_record.lock() {
                    Ok(mut stored) => *stored = value,
                    Err(poisoned) => *poisoned.into_inner() = value,
                }
                background_task_dispatch_outcome(result)
            }
        },
        &move |panic_message| {
            match iteration_for_panic.lock() {
                Ok(mut stored) => {
                    *stored = aster_forge_tasks::BackgroundTaskDispatchIteration::failed()
                }
                Err(poisoned) => {
                    *poisoned.into_inner() =
                        aster_forge_tasks::BackgroundTaskDispatchIteration::failed()
                }
            }
            crate::services::task::RuntimeTaskRunOutcome::failed(
                Some("Task panicked".to_string()),
                panic_message,
            )
        },
        &record_runtime_task_outcome,
    )
    .await;
    match iteration.lock() {
        Ok(stored) => *stored,
        Err(poisoned) => *poisoned.into_inner(),
    }
}

async fn record_runtime_task_outcome(
    state: web::Data<PrimaryAppState>,
    name: SystemRuntimeTaskKind,
    started_at: chrono::DateTime<Utc>,
    finished_at: chrono::DateTime<Utc>,
    outcome: crate::services::task::RuntimeTaskRunOutcome,
) {
    if let Err(error) = crate::services::task::record_runtime_task_run(
        state.get_ref(),
        name,
        started_at,
        finished_at,
        &outcome,
    )
    .await
    {
        tracing::warn!(%error, task = %name, "failed to record runtime task outcome");
    }
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
        Err(error) => crate::services::task::RuntimeTaskRunOutcome::failed(
            Some("Background task dispatch failed".to_string()),
            error.to_string(),
        ),
    }
}

fn build_background_tasks_base(
    metrics: &crate::metrics::SharedMetricsRecorder,
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

    if state.config_sync().enabled() {
        tasks.push(spawn_config_reload_subscription(
            shutdown_token.clone(),
            state.clone(),
        ));
    }

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

    tasks.push(run_primary_runtime_group(shutdown_token.clone(), state));

    tasks
}

async fn run_primary_runtime_group(
    shutdown_token: CancellationToken,
    state: web::Data<PrimaryAppState>,
) {
    let config = aster_forge_tasks::LeasedScheduledRuntimeConfig::new(
        "aster_drive",
        "aster_drive.background_tasks",
        aster_forge_db::RuntimeLeaseDbStore::new(state.writer_db().clone()),
        aster_forge_db::ScheduledTaskDbStore::new(state.writer_db().clone()),
        state,
        |panic_message| {
            crate::services::task::RuntimeTaskRunOutcome::failed(
                Some("Task panicked".to_string()),
                panic_message,
            )
        },
        record_scheduled_runtime_task_outcome,
    )
    .claim_ttl(Duration::from_secs(120))
    .lease_ttl(Duration::from_secs(30))
    .lease_renew_interval(Duration::from_secs(10))
    .lease_standby_retry_interval(Duration::from_secs(5));

    config
        .run(shutdown_token, |runtime| {
            runtime.worker(spawn_background_task_dispatcher);
            runtime.scheduled(
                SystemRuntimeTaskKind::MailOutboxDispatch,
                mail_outbox_dispatch_interval_for_web_state,
                None,
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
            );
            runtime.scheduled(
                SystemRuntimeTaskKind::UploadCleanup,
                maintenance_cleanup_interval_for_web_state,
                Some(MAINTENANCE_CLEANUP_JITTER_CAP),
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
            );
            runtime.scheduled(
            SystemRuntimeTaskKind::CompletedUploadCleanup,
            maintenance_cleanup_interval_for_web_state,
            Some(MAINTENANCE_CLEANUP_JITTER_CAP),
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
        }
        );
            runtime.scheduled(
                SystemRuntimeTaskKind::BlobReconcile,
                blob_reconcile_interval_for_web_state,
                None,
                |s| async move {
                    match crate::services::ops::maintenance::reconcile_blob_state(s.get_ref()).await
                    {
                        Ok(stats)
                            if stats.ref_count_fixed > 0 || stats.orphan_blobs_deleted > 0 =>
                        {
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
            );
            runtime.scheduled(
                SystemRuntimeTaskKind::SystemHealthCheck,
                system_health_check_interval_for_web_state,
                None,
                |s| async move {
                    let report =
                        crate::services::ops::health::run_primary_system_health_checks(s.get_ref())
                            .await;
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
            );
            runtime.scheduled(
                SystemRuntimeTaskKind::TrashCleanup,
                maintenance_cleanup_interval_for_web_state,
                Some(MAINTENANCE_CLEANUP_JITTER_CAP),
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
            );
            runtime.scheduled(
                SystemRuntimeTaskKind::TeamArchiveCleanup,
                maintenance_cleanup_interval_for_web_state,
                Some(MAINTENANCE_CLEANUP_JITTER_CAP),
                |s| async move {
                    match crate::services::workspace::team::cleanup_expired_archived_teams(
                        s.get_ref(),
                    )
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
            );
            runtime.scheduled(
                SystemRuntimeTaskKind::LockCleanup,
                maintenance_cleanup_interval_for_web_state,
                Some(MAINTENANCE_CLEANUP_JITTER_CAP),
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
            );
            runtime.scheduled(
                SystemRuntimeTaskKind::AuthSessionCleanup,
                maintenance_cleanup_interval_for_web_state,
                Some(MAINTENANCE_CLEANUP_JITTER_CAP),
                |s| async move {
                    match crate::services::auth::local::cleanup_expired_auth_sessions(s.get_ref())
                        .await
                    {
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
            );
            runtime.scheduled(
                SystemRuntimeTaskKind::ExternalAuthFlowCleanup,
                maintenance_cleanup_interval_for_web_state,
                Some(MAINTENANCE_CLEANUP_JITTER_CAP),
                |s| async move {
                    match crate::services::auth::external::cleanup_expired_flows(s.get_ref()).await
                    {
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
            );
            runtime.scheduled(
                SystemRuntimeTaskKind::MfaFlowCleanup,
                maintenance_cleanup_interval_for_web_state,
                Some(MAINTENANCE_CLEANUP_JITTER_CAP),
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
            );
            runtime.scheduled(
                SystemRuntimeTaskKind::AuditCleanup,
                maintenance_cleanup_interval_for_web_state,
                Some(MAINTENANCE_CLEANUP_JITTER_CAP),
                |s| async move {
                    match crate::services::ops::audit::cleanup_expired(s.get_ref()).await {
                        Ok(count) if count > 0 => {
                            crate::services::task::RuntimeTaskRunOutcome::succeeded(Some(format!(
                                "cleaned up {count} expired audit log entries"
                            )))
                        }
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
            );
            runtime.scheduled(
                SystemRuntimeTaskKind::TaskCleanup,
                maintenance_cleanup_interval_for_web_state,
                Some(MAINTENANCE_CLEANUP_JITTER_CAP),
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
            );
            runtime.scheduled(
                SystemRuntimeTaskKind::WopiSessionCleanup,
                maintenance_cleanup_interval_for_web_state,
                Some(MAINTENANCE_CLEANUP_JITTER_CAP),
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
            );
        })
        .await;
}

async fn record_scheduled_runtime_task_outcome(
    state: web::Data<PrimaryAppState>,
    name: SystemRuntimeTaskKind,
    claim: aster_forge_tasks::ScheduledTaskClaim,
    started_at: chrono::DateTime<Utc>,
    finished_at: chrono::DateTime<Utc>,
    outcome: crate::services::task::RuntimeTaskRunOutcome,
) {
    if let Err(error) = crate::services::task::record_scheduled_runtime_task_run(
        state.get_ref(),
        name,
        claim.scheduled_at,
        started_at,
        finished_at,
        &outcome,
    )
    .await
    {
        tracing::warn!(%error, task = %name, "failed to record scheduled runtime task outcome");
    }
}

/// Spawn only follower-safe background tasks.
pub fn spawn_follower_background_tasks(
    state: web::Data<FollowerAppState>,
    shutdown_token: CancellationToken,
) -> BackgroundTasks {
    tracing::info!("follower mode enabled; skipping primary-only background tasks");
    let mut tasks = build_background_tasks_base(&state.metrics, shutdown_token);
    let shutdown_token = tasks.shutdown_token();
    if state.config_sync().enabled() {
        tasks.push(spawn_config_reload_subscription(
            shutdown_token.clone(),
            state.clone(),
        ));
    }
    tasks.push(
        crate::storage::remote_protocol::tunnel::client::run_follower_tunnel_worker(
            state,
            shutdown_token,
        ),
    );
    tasks
}

fn spawn_config_reload_subscription<S>(
    shutdown_token: CancellationToken,
    state: web::Data<S>,
) -> impl Future<Output = ()> + Send + 'static
where
    S: super::SharedRuntimeState + Send + Sync + 'static,
{
    let runtime = state.config_sync().clone();
    let state = state.into_inner();
    async move {
        if let Err(error) = crate::services::ops::config::runtime::run_config_reload_subscription(
            state,
            runtime,
            shutdown_token,
        )
        .await
        {
            tracing::warn!(
                error = %error,
                "runtime config reload subscription stopped"
            );
        }
    }
}

fn mail_outbox_dispatch_interval_for_web_state(state: &web::Data<PrimaryAppState>) -> Duration {
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

fn maintenance_cleanup_interval_for_web_state(state: &web::Data<PrimaryAppState>) -> Duration {
    Duration::from_secs(
        crate::config::operations::maintenance_cleanup_interval_secs(&state.runtime_config),
    )
}

fn blob_reconcile_interval_for_web_state(state: &web::Data<PrimaryAppState>) -> Duration {
    Duration::from_secs(crate::config::operations::blob_reconcile_interval_secs(
        &state.runtime_config,
    ))
}

fn system_health_check_interval_for_web_state(state: &web::Data<PrimaryAppState>) -> Duration {
    Duration::from_secs(
        crate::config::operations::remote_node_health_test_interval_secs(&state.runtime_config),
    )
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::Arc;

    use actix_web::web;
    use migration::Migrator;

    use super::PrimaryAppState;

    pub async fn setup_primary_state() -> web::Data<PrimaryAppState> {
        let db = crate::db::connect_with_metrics(
            &crate::config::DatabaseConfig {
                url: "sqlite::memory:".to_string(),
                pool_size: 1,
                retry_count: 0,
            },
            crate::metrics::NoopMetrics::arc(),
        )
        .await
        .expect("runtime task test database should connect");
        Migrator::up(&db, None)
            .await
            .expect("runtime task test migrations should apply");
        crate::db::repository::config_repo::ensure_defaults_with_env(&db, &|_| None)
            .await
            .expect("runtime config defaults should initialize");

        let cache =
            aster_forge_cache::create_cache(&aster_forge_cache::CacheConfig::default()).await;
        let runtime_config = Arc::new(crate::config::RuntimeConfig::new());
        runtime_config
            .reload(&db)
            .await
            .expect("runtime config should load");
        let (storage_change_tx, _) = tokio::sync::broadcast::channel(
            crate::services::events::storage_change::STORAGE_CHANGE_CHANNEL_CAPACITY,
        );
        let (share_download_rollback, _worker) =
            crate::services::share::build_share_download_rollback_queue(
                db.clone(),
                1,
                crate::metrics::NoopMetrics::arc(),
            );

        web::Data::new(PrimaryAppState {
            db_handles: aster_forge_db::DbHandles::single(db),
            driver_registry: Arc::new(crate::storage::DriverRegistry::noop()),
            runtime_config,
            policy_snapshot: Arc::new(crate::storage::PolicySnapshot::new()),
            config: Arc::new(crate::config::Config::default()),
            cache,
            config_sync: aster_forge_config::ConfigSyncRuntime::disabled_for_test("aster_drive"),
            metrics: crate::metrics::NoopMetrics::arc(),
            mail_sender: aster_forge_mail::memory_sender(),
            storage_change_tx,
            share_download_rollback,
            background_task_dispatch_wakeup:
                crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
            remote_protocol: crate::runtime::PrimaryAppState::new_remote_protocol(),
        })
    }
}

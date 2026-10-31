//! 运行时子模块：`tasks`。

use std::future::Future;
use std::time::Duration;

use actix_web::web;
use chrono::Utc;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use super::{FollowerAppState, PrimaryAppState};
use crate::services::share::ShareDownloadRollbackWorker;
use crate::services::task::SystemRuntimeTaskKind;
use aster_forge_tasks::BackgroundTasks;

const MAINTENANCE_CLEANUP_JITTER_CAP: Duration = Duration::from_secs(30);

async fn cleanup_local_task_artifacts(
    s: web::Data<PrimaryAppState>,
) -> crate::services::task::RuntimeTaskRunOutcome {
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

fn build_background_tasks_base(
    metrics: &aster_drive_metrics::SharedMetricsRecorder,
    shutdown_token: CancellationToken,
) -> BackgroundTasks {
    let mut tasks = BackgroundTasks::with_shutdown_token(shutdown_token);
    if let Some(task) = metrics.system_metrics_updater_task(tasks.shutdown_token()) {
        tasks.push(task);
    }
    tasks
}

/// Spawn maintenance for resources owned by this Primary process, independently of its role.
pub fn spawn_primary_background_tasks(
    state: web::Data<PrimaryAppState>,
    share_download_rollback_worker: ShareDownloadRollbackWorker,
    shutdown_token: CancellationToken,
) -> BackgroundTasks {
    let grace = state.config.worker.consumer_config();
    let mut tasks = build_background_tasks_base(&state.metrics, shutdown_token)
        .with_shutdown_grace(
            grace.drain_timeout + grace.cancellation_timeout + Duration::from_secs(5),
        );
    let shutdown_token = tasks.shutdown_token();
    tasks.push(aster_forge_tasks::run_periodic_task(
        aster_forge_tasks::PeriodicTask {
            name: SystemRuntimeTaskKind::TaskCleanup,
            task_name: SystemRuntimeTaskKind::TaskCleanup.as_str(),
            interval_fn: maintenance_cleanup_interval_for_web_state,
            jitter_cap: Some(MAINTENANCE_CLEANUP_JITTER_CAP),
            shutdown_token: shutdown_token.clone(),
            state: state.clone(),
            hooks: aster_forge_tasks::RecordedTaskHooks::new(
                cleanup_local_task_artifacts,
                |panic| {
                    crate::services::task::RuntimeTaskRunOutcome::failed(
                        Some("Task cleanup panicked".to_string()),
                        panic,
                    )
                },
                record_runtime_task_outcome,
            ),
        },
    ));
    if state.metrics.enabled() {
        tasks.push(metrics_sampling_loop(
            state.clone(),
            shutdown_token.clone(),
            |state| async move {
                crate::services::ops::metrics::sample_primary(state.get_ref()).await;
            },
        ));
    }

    if state.config_sync().enabled() {
        tasks.push(spawn_config_reload_subscription(
            shutdown_token.clone(),
            state.clone(),
        ));
    }
    if state.storage_change_bus.has_transport() {
        tasks.push(spawn_storage_change_subscription(
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

    tasks
}

pub(crate) fn spawn_task_consumers(
    state: web::Data<PrimaryAppState>,
    statuses: Vec<aster_forge_tasks::TaskRuntimeStatus>,
    stop: CancellationToken,
) -> BackgroundTasks {
    let budget = state.config.worker.consumer_config();
    let mut tasks = BackgroundTasks::with_shutdown_token(stop.clone()).with_shutdown_grace(
        budget.drain_timeout + budget.cancellation_timeout + Duration::from_secs(5),
    );
    crate::services::task::dispatch::spawn_consumers(state, statuses, stop, &mut tasks);
    tasks
}

pub(crate) fn spawn_scheduler(
    state: web::Data<PrimaryAppState>,
    status: aster_forge_tasks::TaskRuntimeStatus,
    stop: CancellationToken,
) -> BackgroundTasks {
    let mut tasks = BackgroundTasks::with_shutdown_token(stop.clone());
    tasks.push(run_primary_runtime_group(stop, state, status));
    tasks
}

async fn spawn_storage_change_subscription(
    shutdown_token: CancellationToken,
    state: web::Data<PrimaryAppState>,
) {
    crate::services::events::storage_change::run_cross_instance_subscription(
        state.into_inner(),
        shutdown_token,
    )
    .await;
}

async fn run_primary_runtime_group(
    shutdown_token: CancellationToken,
    state: web::Data<PrimaryAppState>,
    status: aster_forge_tasks::TaskRuntimeStatus,
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
    .status(status)
    .claim_ttl(Duration::from_secs(120))
    .lease_ttl(Duration::from_secs(30))
    .lease_renew_interval(Duration::from_secs(10))
    .lease_standby_retry_interval(Duration::from_secs(5));

    config
        .run(shutdown_token, |runtime| {
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
                Ok(stats)
                    if stats.completed_sessions_deleted > 0
                        || stats.file_create_idempotencies_deleted > 0 =>
                {
                    tracing::info!(
                        deleted = stats.completed_sessions_deleted,
                        broken = stats.broken_completed_sessions_deleted,
                        file_create_idempotencies_deleted = stats.file_create_idempotencies_deleted,
                        "cleaned up expired completed uploads and file-create idempotencies"
                    );
                    crate::services::task::RuntimeTaskRunOutcome::succeeded(Some(format!(
                        "deleted {} completed sessions ({} broken) and {} file-create idempotencies",
                        stats.completed_sessions_deleted,
                        stats.broken_completed_sessions_deleted,
                        stats.file_create_idempotencies_deleted,
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
                    report.into()
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
                    // Attempt both policies independently, but report either failure to task health.
                    let activities = crate::services::workspace::activity::cleanup_expired(s.get_ref()).await;
                    let audit = crate::services::ops::audit::cleanup_expired(s.get_ref()).await;
                    match (activities, audit) {
                        (Ok(activities), Ok(audit)) if activities > 0 || audit > 0 => {
                            crate::services::task::RuntimeTaskRunOutcome::succeeded(Some(format!(
                                "cleaned up {activities} expired workspace activities and {audit} audit log entries"
                            )))
                        }
                        (Ok(_), Ok(_)) => crate::services::task::RuntimeTaskRunOutcome::quiet(),
                        (Err(error), _) | (_, Err(error)) => {
                            tracing::warn!("activity/audit cleanup failed: {error}");
                            crate::services::task::RuntimeTaskRunOutcome::failed(
                                Some("Activity/audit cleanup failed".to_string()),
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
    if state.metrics.enabled() {
        tasks.push(metrics_sampling_loop(
            state.clone(),
            shutdown_token.clone(),
            |state| async move {
                crate::services::ops::metrics::sample_capacity(state.get_ref()).await;
                state
                    .db_handles
                    .record_pool_metrics("main", state.metrics.forge_recorder().as_ref());
            },
        ));
    }

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
                "runtime config reload subscription terminated unexpectedly"
            );
        }
    }
}

fn mail_outbox_dispatch_interval_for_web_state(state: &web::Data<PrimaryAppState>) -> Duration {
    Duration::from_secs(
        crate::config::operations::mail_outbox_dispatch_interval_secs(&state.runtime_config),
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

async fn metrics_sampling_loop<S, F, Fut>(
    state: web::Data<S>,
    shutdown: CancellationToken,
    sample: F,
) where
    S: Send + Sync + 'static,
    F: Fn(web::Data<S>) -> Fut,
    Fut: Future<Output = ()>,
{
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! { biased; () = shutdown.cancelled() => break, _ = interval.tick() => {} }
        tokio::select! { biased; () = shutdown.cancelled() => break, () = sample(state.clone()) => {} }
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::Arc;

    use super::PrimaryAppState;
    use actix_web::web;

    pub async fn setup_primary_state() -> web::Data<PrimaryAppState> {
        let db = crate::db::connect_with_metrics(
            &crate::config::DatabaseConfig {
                url: "sqlite::memory:".into(),
                pool_size: 1,
                retry_count: 0,
            },
            aster_drive_metrics::NoopMetrics::arc(),
        )
        .await
        .expect("runtime task test database should connect");
        crate::storage::connectors::test_support::migrate_current_storage_test_schema(&db).await;
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
        let storage_change_bus = crate::services::events::storage_change::StorageChangeBus::new(
            crate::services::events::storage_change::STORAGE_CHANGE_CHANNEL_CAPACITY,
        );
        let (share_download_rollback, _worker) =
            crate::services::share::build_share_download_rollback_queue(
                db.clone(),
                1,
                aster_drive_metrics::NoopMetrics::arc(),
            );

        web::Data::new(PrimaryAppState {
            audit_writer: crate::runtime::test_support::audit_writer(&db).await,
            db_handles: aster_forge_db::DbHandles::single(db),
            driver_registry: Arc::new(
                crate::storage::DriverRegistry::noop()
                    .expect("built-in storage connector registry"),
            ),
            runtime_config,
            policy_snapshot: Arc::new(crate::storage::PolicySnapshot::new()),
            config: Arc::new(crate::config::Config::default()),
            cache,
            config_sync: aster_forge_config::ConfigSyncRuntime::disabled_for_test("aster_drive"),
            metrics: aster_drive_metrics::NoopMetrics::arc(),
            mail_sender: aster_forge_mail::memory_sender(),
            storage_change_bus,
            share_download_rollback,
            background_task_dispatch_wakeup:
                crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
            remote_protocol: crate::runtime::PrimaryAppState::new_remote_protocol(),
        })
    }
}

#[cfg(test)]
mod metrics_sampling_tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[cfg(feature = "metrics")]
    #[tokio::test]
    async fn follower_background_sampler_publishes_native_pool_and_capacity_then_stops() {
        use aster_forge_metrics::MetricsProvider;
        let mut primary = test_support::setup_primary_state().await.get_ref().clone();
        let mut config = primary.config.as_ref().clone();
        config.server.upload_temp_dir = std::env::temp_dir().to_string_lossy().into_owned();
        primary.config = Arc::new(config);
        let provider = MetricsProvider::builder()
            .with_prometheus()
            .unwrap()
            .build()
            .unwrap();
        primary.metrics = aster_drive_metrics::create_metrics_recorder(provider.clone()).unwrap();
        let state = web::Data::new(primary.follower_view());
        let stop = CancellationToken::new();
        let tasks = spawn_follower_background_tasks(state, stop.clone());
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let raw = provider
                    .export_prometheus(Duration::from_secs(1))
                    .await
                    .unwrap();
                if raw.contains(
                    "db_client_connection_max{db_client_connection_pool_name=\"main.writer\"} 1",
                ) && raw.contains(
                    "aster_drive_observability_snapshot_valid{source=\"staging_filesystem\"} 1",
                ) {
                    assert!(!raw.contains("db_client_connection_pool_name=\"main.reader\""));
                    assert!(!raw.contains("aster_drive_upload_sessions_active"));
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        tokio::time::timeout(Duration::from_secs(5), tasks.shutdown())
            .await
            .unwrap();
        assert!(stop.is_cancelled());
        provider.shutdown(Duration::from_secs(1)).await.unwrap();
    }

    #[tokio::test]
    async fn cancelled_sampler_never_starts_database_work() {
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let stop = CancellationToken::new();
        stop.cancel();
        metrics_sampling_loop(web::Data::new(()), stop, move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
            async {}
        })
        .await;
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn sampler_runs_immediately_and_cancels_an_inflight_sample() {
        let entered = Arc::new(tokio::sync::Notify::new());
        let stop = CancellationToken::new();
        let worker_stop = stop.clone();
        let worker_entered = entered.clone();
        let worker = tokio::spawn(metrics_sampling_loop(
            web::Data::new(()),
            worker_stop,
            move |_| {
                let entered = worker_entered.clone();
                async move {
                    entered.notify_one();
                    std::future::pending::<()>().await;
                }
            },
        ));
        tokio::time::timeout(Duration::from_secs(2), entered.notified())
            .await
            .unwrap();
        stop.cancel();
        tokio::time::timeout(Duration::from_secs(2), worker)
            .await
            .unwrap()
            .unwrap();
    }
}

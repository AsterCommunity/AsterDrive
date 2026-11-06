//! Cross-process synchronization for runtime system configuration.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use aster_forge_config::{
    ConfigReloadObservation, ConfigSyncConnectionObservation, ConfigSyncRuntime,
};
use tokio_util::sync::CancellationToken;

use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;

pub const CONFIG_RELOAD_NAMESPACE: &str = "aster_drive";
pub const STORAGE_TOPOLOGY_RELOAD_KEY: &str = "__aster_drive.storage_topology";
const USER_POLICY_GROUP_RELOAD_KEY_PREFIX: &str = "__aster_drive.user_policy_group.";
// Redis pub/sub is a prompt reload hint, not durable delivery. Every enabled
// instance independently repairs missed placement/assignment hints off the hot path.
const POLICY_SNAPSHOT_RECONCILE_INTERVAL: Duration = Duration::from_secs(30);
const RELOAD_PUBLISH_ATTEMPTS: u32 = 3;
const RELOAD_PUBLISH_INITIAL_DELAY: Duration = Duration::from_millis(50);
const RELOAD_PUBLISH_MAX_DELAY: Duration = Duration::from_millis(200);

pub async fn reconcile_storage_topology(state: &impl SharedRuntimeState) -> Result<()> {
    state.driver_registry().invalidate_all();
    match state.config().server.start_mode {
        crate::config::node_mode::NodeRuntimeMode::Primary => {
            state
                .driver_registry()
                .reload_primary_state(state.writer_db(), state.config())
                .await?;
        }
        crate::config::node_mode::NodeRuntimeMode::Follower => {
            state
                .driver_registry()
                .reload_follower_state(state.writer_db())
                .await?;
        }
    }
    state
        .driver_registry()
        .reload_policy_snapshot(state.policy_snapshot(), state.writer_db())
        .await?;
    state.driver_registry().invalidate_all();
    super::system::invalidate_all_dependent_public_config_caches();
    Ok(())
}

pub async fn publish_storage_topology_reload_after_commit(
    state: &impl SharedRuntimeState,
    mutation: &'static str,
    entity_kind: &'static str,
    entity_id: i64,
) {
    publish_reload_after_commit(
        mutation,
        entity_kind,
        entity_id,
        "storage_topology",
        || async {
            state
                .config_sync()
                .publish_reload(
                    [STORAGE_TOPOLOGY_RELOAD_KEY],
                    aster_forge_config::ConfigNotificationSource::Other(
                        "storage_topology".to_string(),
                    ),
                )
                .await
                .map_err(map_config_core_error)
        },
    )
    .await;
}

async fn publish_reload_after_commit<F, Fut>(
    mutation: &'static str,
    entity_kind: &'static str,
    entity_id: i64,
    reload_kind: &'static str,
    mut publish: F,
) where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let mut last_error = None;
    for attempt in 0..RELOAD_PUBLISH_ATTEMPTS {
        match publish().await {
            Ok(()) => return,
            Err(error) => last_error = Some(error),
        }
        if attempt + 1 < RELOAD_PUBLISH_ATTEMPTS {
            let delay = aster_forge_utils::backoff::cap_delay(
                aster_forge_utils::backoff::exponential_delay(
                    RELOAD_PUBLISH_INITIAL_DELAY,
                    attempt,
                ),
                RELOAD_PUBLISH_MAX_DELAY,
            );
            tokio::time::sleep(delay).await;
        }
    }

    let Some(error) = last_error else {
        return;
    };
    tracing::warn!(
        mutation,
        entity_kind,
        entity_id,
        reload_kind,
        attempts = RELOAD_PUBLISH_ATTEMPTS,
        %error,
        "authoritative mutation committed but cross-instance reload notification failed"
    );
}

pub async fn publish_user_policy_group_reload(
    state: &impl SharedRuntimeState,
    user_id: i64,
) -> Result<()> {
    state
        .config_sync()
        .publish_reload(
            [format!("{USER_POLICY_GROUP_RELOAD_KEY_PREFIX}{user_id}")],
            aster_forge_config::ConfigNotificationSource::Other("user_policy_group".to_string()),
        )
        .await
        .map_err(map_config_core_error)
}

pub async fn publish_user_policy_group_reload_after_commit(
    state: &impl SharedRuntimeState,
    mutation: &'static str,
    user_id: i64,
) {
    publish_reload_after_commit(
        mutation,
        "user_policy_group",
        user_id,
        "user_policy_group",
        || publish_user_policy_group_reload(state, user_id),
    )
    .await;
}

async fn reconcile_user_policy_groups(
    state: &impl SharedRuntimeState,
    keys: &[String],
) -> Result<()> {
    for key in keys {
        let Some(user_id) = key
            .strip_prefix(USER_POLICY_GROUP_RELOAD_KEY_PREFIX)
            .and_then(|value| value.parse::<i64>().ok())
        else {
            continue;
        };
        match crate::db::repository::user_repo::find_by_id(state.writer_db(), user_id).await {
            Ok(user) => match user.policy_group_id {
                Some(group_id) => state
                    .policy_snapshot()
                    .set_user_policy_group(user_id, group_id),
                None => state.policy_snapshot().remove_user_policy_group(user_id),
            },
            Err(crate::errors::AsterError::RecordNotFound(_)) => {
                state.policy_snapshot().remove_user_policy_group(user_id);
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

/// Run notification reloads and periodic writer-backed snapshot reconciliation until shutdown.
/// Recover missed publish hints off the upload hot path while preserving the owned audit writer and drivers.
pub async fn run_config_reload_subscription<S>(
    state: Arc<S>,
    runtime: ConfigSyncRuntime,
    shutdown: CancellationToken,
) -> Result<()>
where
    S: SharedRuntimeState + Send + Sync + 'static,
{
    if !runtime.enabled() {
        shutdown.cancelled().await;
        return Ok(());
    }
    let periodic_state = state.clone();
    let metrics = state.metrics().clone();
    let reload_observer = move |observation: ConfigReloadObservation| {
        metrics.record_config_reload(
            observation.source,
            observation.decision.as_label(),
            observation.status,
            observation.changed_keys,
            observation.duration_seconds,
        );
    };
    let connection_metrics = state.metrics().forge_recorder();
    let connection_observer = move |observation: ConfigSyncConnectionObservation| {
        connection_metrics.record_application_event(
            "config_sync",
            observation.state.as_label(),
            "ok",
        );
    };
    let reconcile_state = state.clone();

    let subscription = runtime.run_reload_subscription_with_reconcile_and_observers(
        shutdown.clone(),
        move || {
            let state = reconcile_state.clone();
            async move {
                tracing::debug!(
                    "reconciling runtime config after config sync subscription connected"
                );
                state
                    .runtime_config()
                    .reload(state.writer_db())
                    .await
                    .map_err(|error| {
                        aster_forge_config::ConfigCoreError::store(error.to_string())
                    })?;
                reconcile_storage_topology(state.as_ref())
                    .await
                    .map_err(|error| {
                        aster_forge_config::ConfigCoreError::store(error.to_string())
                    })?;
                super::system::invalidate_all_dependent_public_config_caches();
                Ok(())
            }
        },
        move |message| {
            let state = state.clone();
            async move {
                tracing::debug!(
                    keys = ?message.keys,
                    origin_runtime_id = %message.origin_runtime_id,
                    "reloading runtime config after remote config sync notification"
                );
                state
                    .runtime_config()
                    .reload(state.writer_db())
                    .await
                    .map_err(|error| {
                        aster_forge_config::ConfigCoreError::store(error.to_string())
                    })?;
                if message
                    .keys
                    .iter()
                    .any(|key| key == STORAGE_TOPOLOGY_RELOAD_KEY)
                {
                    reconcile_storage_topology(state.as_ref())
                        .await
                        .map_err(|error| {
                            aster_forge_config::ConfigCoreError::store(error.to_string())
                        })?;
                }
                reconcile_user_policy_groups(state.as_ref(), &message.keys)
                    .await
                    .map_err(|error| {
                        aster_forge_config::ConfigCoreError::store(error.to_string())
                    })?;
                if message.keys.is_empty() {
                    super::system::invalidate_all_dependent_public_config_caches();
                } else {
                    for key in &message.keys {
                        super::system::invalidate_dependent_public_config_caches(key);
                    }
                }
                Ok(())
            }
        },
        Some(&reload_observer),
        Some(&connection_observer),
    );
    let periodic_reconciliation = async move {
        let mut reconcile_tick = tokio::time::interval_at(
            tokio::time::Instant::now() + POLICY_SNAPSHOT_RECONCILE_INTERVAL,
            POLICY_SNAPSHOT_RECONCILE_INTERVAL,
        );
        reconcile_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            reconcile_tick.tick().await;
            // Refresh the immutable routing snapshot only. Do not tear down
            // cached drivers or reload/decrypt credentials on every timer tick.
            if let Err(error) = periodic_state
                .driver_registry()
                .reload_policy_snapshot(
                    periodic_state.policy_snapshot(),
                    periodic_state.writer_db(),
                )
                .await
            {
                tracing::warn!(error = %error,
                    "periodic storage policy snapshot reconciliation failed; retrying next interval");
            }
        }
    };
    // Poll both futures while either is awaiting DB I/O. Pausing the subscriber
    // during a timer reload could retain the only SQLite pool connection.
    tokio::select! {
        biased;
        () = shutdown.cancelled() => Ok(()),
        result = subscription => result.map_err(map_config_core_error),
        () = periodic_reconciliation => Ok(()),
    }
}

pub(crate) fn map_config_core_error(error: aster_forge_config::ConfigCoreError) -> AsterError {
    AsterError::internal_error(format!("config sync failed: {error}"))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    use aster_forge_config::ConfigSyncConfig;
    use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};

    use crate::runtime::{SharedRuntimeState, StorageConnectorRuntimeState};

    struct ReloadTestState {
        audit_writer: Arc<aster_forge_audit::AuditWriter>,
        db: sea_orm::DatabaseConnection,
        runtime_config: Arc<crate::config::RuntimeConfig>,
        driver_registry: Arc<crate::storage::DriverRegistry>,
        policy_snapshot: Arc<crate::storage::PolicySnapshot>,
        config: Arc<crate::config::Config>,
        cache: Arc<dyn aster_forge_cache::CacheBackend>,
        config_sync: aster_forge_config::ConfigSyncRuntime,
        metrics: aster_drive_metrics::SharedMetricsRecorder,
    }

    impl StorageConnectorRuntimeState for ReloadTestState {
        fn writer_db(&self) -> &sea_orm::DatabaseConnection {
            &self.db
        }

        fn driver_registry(&self) -> &Arc<crate::storage::DriverRegistry> {
            &self.driver_registry
        }

        fn runtime_config(&self) -> &Arc<crate::config::RuntimeConfig> {
            &self.runtime_config
        }

        fn config(&self) -> &Arc<crate::config::Config> {
            &self.config
        }
    }

    impl SharedRuntimeState for ReloadTestState {
        fn audit_writer(&self) -> &Arc<aster_forge_audit::AuditWriter> {
            &self.audit_writer
        }

        fn reader_db(&self) -> &sea_orm::DatabaseConnection {
            &self.db
        }

        fn policy_snapshot(&self) -> &Arc<crate::storage::PolicySnapshot> {
            &self.policy_snapshot
        }

        fn cache(&self) -> &Arc<dyn aster_forge_cache::CacheBackend> {
            &self.cache
        }

        fn config_sync(&self) -> &aster_forge_config::ConfigSyncRuntime {
            &self.config_sync
        }

        fn metrics(&self) -> &aster_drive_metrics::SharedMetricsRecorder {
            &self.metrics
        }
    }

    #[test]
    fn config_sync_settings_are_disabled_by_default() {
        let runtime = aster_forge_config::build_config_sync_runtime(
            &ConfigSyncConfig::default(),
            super::CONFIG_RELOAD_NAMESPACE,
        )
        .expect("default config sync should be valid");

        assert!(!runtime.enabled());
        assert_eq!(runtime.namespace(), "aster_drive");
        assert!(runtime.runtime_id().starts_with("runtime-"));
    }

    #[test]
    fn redis_config_sync_requires_endpoint() {
        let result = aster_forge_config::build_config_sync_runtime(
            &ConfigSyncConfig {
                backend: aster_forge_config::CONFIG_SYNC_BACKEND_REDIS.to_string(),
                endpoint: aster_forge_config::ConfigSyncEndpoint::default(),
                topic: "aster_drive.test".to_string(),
            },
            super::CONFIG_RELOAD_NAMESPACE,
        );
        let Err(error) = result else {
            panic!("redis config sync without endpoint should fail");
        };

        assert!(
            error
                .to_string()
                .contains("config_sync.endpoint is required")
        );
    }

    #[tokio::test]
    async fn post_commit_notification_retries_until_publish_succeeds() {
        let attempts = AtomicU32::new(0);

        super::publish_reload_after_commit("create", "user_policy_group", 7, "test", || async {
            let attempt = attempts.fetch_add(1, Ordering::SeqCst);
            if attempt < 2 {
                Err(crate::errors::AsterError::internal_error(
                    "transient publish failure",
                ))
            } else {
                Ok(())
            }
        })
        .await;

        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn post_commit_notification_stops_after_bounded_failures() {
        let attempts = AtomicU32::new(0);

        super::publish_reload_after_commit("delete", "user_policy_group", 7, "test", || async {
            attempts.fetch_add(1, Ordering::SeqCst);
            Err(crate::errors::AsterError::internal_error(
                "persistent publish failure",
            ))
        })
        .await;

        assert_eq!(
            attempts.load(Ordering::SeqCst),
            super::RELOAD_PUBLISH_ATTEMPTS
        );
    }

    /// Build a single-writer reload fixture with a valid policy, defaults and initial snapshots.
    async fn reload_state() -> (
        Arc<ReloadTestState>,
        aster_drive_model::entities::storage_policy::Model,
    ) {
        let db = crate::db::connect_with_metrics(
            &crate::config::DatabaseConfig {
                url: "sqlite::memory:".into(),
                pool_size: 1,
                retry_count: 0,
            },
            aster_drive_metrics::NoopMetrics::arc(),
        )
        .await
        .expect("config reload test database should connect");
        crate::storage::connectors::test_support::migrate_current_storage_test_schema(&db).await;
        crate::db::repository::config_repo::ensure_defaults_with_env(&db, &|_| None)
            .await
            .expect("config reload test defaults should load");
        let now = chrono::Utc::now();
        let mut policy = crate::storage::connectors::test_support::s3_policy(
            "https://old.example.com",
            "test",
            "",
            aster_drive_model::types::ObjectStorageUploadStrategy::RelayStream,
            aster_drive_model::types::ObjectStorageDownloadStrategy::RelayStream,
        );
        policy.name = "config reload policy".to_string();
        policy.is_default = true;
        policy.chunk_size = 5_242_880;
        policy.created_at = now;
        policy.updated_at = now;
        let policy = crate::db::repository::policy_repo::create(&db, policy.into_active_model())
            .await
            .expect("config reload test policy should insert");
        crate::services::storage_policy::policy::ensure_policy_groups_seeded(&db)
            .await
            .expect("config reload test policy groups should seed");

        let runtime_config = Arc::new(crate::config::RuntimeConfig::new());
        runtime_config
            .reload(&db)
            .await
            .expect("initial runtime config should load");
        let config = Arc::new(crate::config::Config::default());
        let driver_registry = Arc::new(
            crate::storage::DriverRegistry::noop().expect("built-in storage connector registry"),
        );
        driver_registry
            .reload_primary_state(&db, config.as_ref())
            .await
            .expect("initial driver registry state should load");
        let policy_snapshot = Arc::new(crate::storage::PolicySnapshot::new());
        policy_snapshot
            .reload(&db, driver_registry.connectors())
            .await
            .expect("initial policy snapshot should load");
        let state = Arc::new(ReloadTestState {
            audit_writer: crate::runtime::test_support::audit_writer(&db).await,
            db: db.clone(),
            runtime_config: runtime_config.clone(),
            driver_registry,
            policy_snapshot: policy_snapshot.clone(),
            config,
            cache: aster_forge_cache::create_cache(&aster_forge_cache::CacheConfig::default())
                .await,
            config_sync: aster_forge_config::ConfigSyncRuntime::disabled_for_test("aster_drive"),
            metrics: aster_drive_metrics::NoopMetrics::arc(),
        });

        (state, policy)
    }

    /// Verify notification reloads and periodic reconciliation preserve the owned audit writer.
    #[tokio::test]
    async fn remote_notification_reloads_runtime_config_from_authoritative_database() {
        let (state, policy) = reload_state().await;
        let db = state.db.clone();
        let runtime_config = state.runtime_config.clone();
        let policy_snapshot = state.policy_snapshot.clone();
        // Runtime reload must leave accepted audit records and their owner intact.
        let audit_writer = state.audit_writer().clone();
        crate::runtime::startup::record_server_start(state.as_ref()).await;

        let notifier: aster_forge_config::SharedConfigChangeNotifier =
            Arc::new(aster_forge_config::InMemoryConfigNotifier::default());
        let receiver = aster_forge_config::ConfigSyncRuntime::with_notifier_for_test(
            super::CONFIG_RELOAD_NAMESPACE,
            "receiver-runtime",
            notifier.clone(),
        );
        let publisher = aster_forge_config::ConfigSyncRuntime::with_notifier_for_test(
            super::CONFIG_RELOAD_NAMESPACE,
            "publisher-runtime",
            notifier,
        );
        let shutdown = tokio_util::sync::CancellationToken::new();
        let worker = tokio::spawn(super::run_config_reload_subscription(
            state.clone(),
            receiver,
            shutdown.clone(),
        ));
        tokio::task::yield_now().await;

        crate::db::repository::config_repo::upsert(
            &db,
            "gravatar_base_url",
            "https://config-sync.example/avatar",
            1,
        )
        .await
        .expect("authoritative config should update");
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if runtime_config.get("gravatar_base_url").as_deref()
                    == Some("https://config-sync.example/avatar")
                {
                    break;
                }
                publisher
                    .publish_reload(
                        std::iter::empty::<&str>(),
                        aster_forge_config::ConfigNotificationSource::Api,
                    )
                    .await
                    .expect("reload notification should publish");
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("remote notification should reload runtime config");

        let policy_id = policy.id;
        let mut active: aster_drive_model::entities::storage_policy::ActiveModel = policy.into();
        active.max_file_size = Set(1);
        active
            .update(&db)
            .await
            .expect("authoritative storage policy should update");
        assert_eq!(
            policy_snapshot
                .get_policy(policy_id)
                .expect("policy should remain in old snapshot")
                .max_file_size,
            0
        );
        publisher
            .publish_reload(
                [super::STORAGE_TOPOLOGY_RELOAD_KEY],
                aster_forge_config::ConfigNotificationSource::Other(
                    "storage_topology_test".to_string(),
                ),
            )
            .await
            .expect("storage topology reload notification should publish");
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if policy_snapshot
                    .get_policy(policy_id)
                    .is_some_and(|policy| policy.max_file_size == 1)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("storage topology notification should reload policy snapshot");

        // A healthy subscriber cannot observe a publisher-only transport failure.
        // Commit a new team without publishing anything: periodic reconciliation
        // must discover it while the original subscription remains connected.
        use sea_orm::ConnectionTrait;
        db.execute_unprepared("INSERT INTO users (id, username, email, password_hash, created_at, updated_at) VALUES (744, 'silent-owner', 'silent-owner@example.test', 'hash', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)")
            .await.unwrap();
        let group_id = policy_snapshot.system_default_policy_group().unwrap().id;
        db.execute_unprepared(&format!("INSERT INTO teams (id, name, description, created_by, policy_group_id, created_at, updated_at) VALUES (744, 'Silent team', '', 744, {group_id}, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"))
            .await.unwrap();
        assert_eq!(policy_snapshot.resolve_team_policy_group_id(744), None);
        tokio::time::timeout(Duration::from_secs(40), async {
            loop {
                if policy_snapshot.resolve_team_policy_group_id(744) == Some(group_id) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("missed publisher notification must recover without reconnect or another message");

        shutdown.cancel();
        worker
            .await
            .expect("config reload worker should join")
            .expect("config reload worker should stop cleanly");
        assert!(Arc::ptr_eq(&audit_writer, state.audit_writer()));
        crate::runtime::startup::record_server_start(state.as_ref()).await;
        crate::services::ops::audit::flush(state.as_ref())
            .await
            .unwrap();
        assert_eq!(audit_writer.snapshot().persisted, 2);
        audit_writer.stop(tokio::time::Instant::now()).await;
    }
    /// Disabled synchronization waits for shutdown and performs no database reload.
    #[tokio::test]
    async fn disabled_subscription_cancels_without_database_access() {
        let (state, _) = reload_state().await;
        state.db.close_by_ref().await.unwrap();
        let shutdown = tokio_util::sync::CancellationToken::new();
        let worker = tokio::spawn(super::run_config_reload_subscription(
            state,
            aster_forge_config::ConfigSyncRuntime::disabled_for_test("aster_drive"),
            shutdown.clone(),
        ));
        tokio::task::yield_now().await;
        assert!(!worker.is_finished());
        shutdown.cancel();
        worker.await.unwrap().unwrap();
    }

    /// A periodic reload failure retains the accepted snapshot and recovers without a new notification.
    #[tokio::test]
    async fn periodic_snapshot_failure_keeps_old_state_then_recovers_next_interval() {
        use sea_orm::ConnectionTrait;
        let (state, policy) = reload_state().await;
        let notifier: aster_forge_config::SharedConfigChangeNotifier =
            Arc::new(aster_forge_config::InMemoryConfigNotifier::default());
        let runtime = aster_forge_config::ConfigSyncRuntime::with_notifier_for_test(
            super::CONFIG_RELOAD_NAMESPACE,
            "periodic-recovery",
            notifier,
        );
        let shutdown = tokio_util::sync::CancellationToken::new();
        // Observe a DB-only marker to prove initial reconciliation has completed.
        crate::db::repository::config_repo::upsert(
            &state.db,
            "gravatar_base_url",
            "https://periodic-initial.example/avatar",
            1,
        )
        .await
        .unwrap();
        let worker = tokio::spawn(super::run_config_reload_subscription(
            state.clone(),
            runtime,
            shutdown.clone(),
        ));
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if state.runtime_config.get("gravatar_base_url").as_deref()
                    == Some("https://periodic-initial.example/avatar")
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("initial subscription must load the authoritative marker");
        state
            .db
            .execute_unprepared(
                "ALTER TABLE storage_policy_group_rules RENAME TO unavailable_rules",
            )
            .await
            .unwrap();
        let mut updated = policy.clone().into_active_model();
        updated.max_file_size = Set(744);
        updated.update(&state.db).await.unwrap();
        tokio::time::sleep(super::POLICY_SNAPSHOT_RECONCILE_INTERVAL + Duration::from_millis(300))
            .await;
        assert!(!worker.is_finished());
        assert_eq!(
            state
                .policy_snapshot
                .get_policy(policy.id)
                .unwrap()
                .max_file_size,
            0
        );
        state
            .db
            .execute_unprepared(
                "ALTER TABLE unavailable_rules RENAME TO storage_policy_group_rules",
            )
            .await
            .unwrap();
        tokio::time::timeout(
            super::POLICY_SNAPSHOT_RECONCILE_INTERVAL + Duration::from_secs(5),
            async {
                loop {
                    if state
                        .policy_snapshot
                        .get_policy(policy.id)
                        .unwrap()
                        .max_file_size
                        == 744
                    {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            },
        )
        .await
        .expect("periodic retry must recover after the database read becomes available");
        shutdown.cancel();
        worker.await.unwrap().unwrap();
    }

    /// Failed initial reconciliation is retried without requiring a transport reconnect or message.
    #[tokio::test]
    async fn initial_config_read_failure_recovers_from_authoritative_database() {
        use aster_forge_db::system_config;
        use sea_orm::{
            ConnectionTrait, ExprTrait,
            sea_query::{Alias, Expr, Query},
        };
        let (state, _) = reload_state().await;
        let notifier: aster_forge_config::SharedConfigChangeNotifier =
            Arc::new(aster_forge_config::InMemoryConfigNotifier::default());
        let runtime = aster_forge_config::ConfigSyncRuntime::with_notifier_for_test(
            super::CONFIG_RELOAD_NAMESPACE,
            "initial-recovery",
            notifier,
        );
        state
            .db
            .execute_unprepared("ALTER TABLE system_config RENAME TO unavailable_config")
            .await
            .unwrap();
        let shutdown = tokio_util::sync::CancellationToken::new();
        let logs = Arc::new(aster_forge_test::logs::CapturedLogs::default());
        let worker_logs = logs.clone();
        let reload =
            super::run_config_reload_subscription(state.clone(), runtime, shutdown.clone());
        let worker = tokio::spawn(async move { worker_logs.capture_future(reload).await });
        tokio::time::timeout(Duration::from_secs(5), async {
            while !logs
                .text()
                .contains("failed to reconcile runtime config after subscription connected")
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("initial reconcile must actually fail while the config table is unavailable");
        assert!(!worker.is_finished());
        // Commit the marker before restoring the table, so a retry cannot
        // finish between table restoration and the authoritative update.
        state
            .db
            .execute(
                &Query::update()
                    .table(Alias::new("unavailable_config"))
                    .value(
                        system_config::Column::Value,
                        "https://initial-retry.example/avatar",
                    )
                    .and_where(Expr::col(system_config::Column::Key).eq("gravatar_base_url"))
                    .to_owned(),
            )
            .await
            .unwrap();
        let marker = state
            .db
            .query_one(
                &Query::select()
                    .column(system_config::Column::Value)
                    .from(Alias::new("unavailable_config"))
                    .and_where(Expr::col(system_config::Column::Key).eq("gravatar_base_url"))
                    .to_owned(),
            )
            .await
            .unwrap()
            .unwrap()
            .try_get_by_index::<String>(0)
            .unwrap();
        assert_eq!(marker, "https://initial-retry.example/avatar");
        state
            .db
            .execute_unprepared("ALTER TABLE unavailable_config RENAME TO system_config")
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if state.runtime_config.get("gravatar_base_url").as_deref()
                    == Some("https://initial-retry.example/avatar")
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|error| panic!("initial failed config read must retry without another publication: {error}; logs: {}",logs.text()));
        shutdown.cancel();
        worker.await.unwrap().unwrap();
    }
    /// Observe a delivered callback failure, then verify its retry publishes the changed policy.
    #[tokio::test]
    async fn notification_read_failures_recover_without_republishing() {
        use aster_forge_config::{ConfigChangeNotifier, ConfigNotification, ConfigReloadMessage};
        use sea_orm::ConnectionTrait;

        #[derive(Default)]
        struct SubscriptionProbe {
            notifier: aster_forge_config::InMemoryConfigNotifier,
            subscribed: tokio::sync::Notify,
        }

        #[async_trait::async_trait]
        impl ConfigChangeNotifier for SubscriptionProbe {
            /// Deliver the real in-memory message without altering notification behavior.
            async fn publish_reload(
                &self,
                message: ConfigReloadMessage,
            ) -> std::result::Result<(), aster_forge_config::ConfigCoreError> {
                self.notifier.publish_reload(message).await
            }

            /// Signal only after the receiver exists so the first test hint cannot be lost.
            async fn subscribe(
                &self,
            ) -> std::result::Result<ConfigNotification, aster_forge_config::ConfigCoreError>
            {
                let notification = self.notifier.subscribe().await?;
                self.subscribed.notify_one();
                Ok(notification)
            }
        }

        struct ReloadOutcomeProbe {
            outcomes: tokio::sync::mpsc::UnboundedSender<(&'static str, &'static str)>,
        }

        impl aster_drive_metrics::MetricsRecorder for ReloadOutcomeProbe {
            /// A callback outcome is emitted after its database work succeeds or fails.
            fn record_config_reload(
                &self,
                source: &'static str,
                decision: &'static str,
                status: &'static str,
                _: u64,
                _: f64,
            ) {
                assert_eq!(source, "pubsub");
                self.outcomes.send((decision, status)).unwrap();
            }
        }

        let (mut state, policy) = reload_state().await;
        let (outcomes, mut observed) = tokio::sync::mpsc::unbounded_channel();
        Arc::get_mut(&mut state).unwrap().metrics = Arc::new(ReloadOutcomeProbe { outcomes });
        let notifier = Arc::new(SubscriptionProbe::default());
        let transport: aster_forge_config::SharedConfigChangeNotifier = notifier.clone();
        let receiver = aster_forge_config::ConfigSyncRuntime::with_notifier_for_test(
            super::CONFIG_RELOAD_NAMESPACE,
            "failure-receiver",
            transport.clone(),
        );
        let publisher = aster_forge_config::ConfigSyncRuntime::with_notifier_for_test(
            super::CONFIG_RELOAD_NAMESPACE,
            "failure-publisher",
            transport,
        );
        let shutdown = tokio_util::sync::CancellationToken::new();
        let worker = tokio::spawn(super::run_config_reload_subscription(
            state.clone(),
            receiver,
            shutdown.clone(),
        ));
        // The whole scenario must finish before the 30-second periodic snapshot
        // timer. Only the failed notification's authoritative retry may recover it.
        tokio::time::timeout(Duration::from_secs(15), async {
            notifier.subscribed.notified().await;
            publisher
                .publish_reload(
                    [super::STORAGE_TOPOLOGY_RELOAD_KEY],
                    aster_forge_config::ConfigNotificationSource::Api,
                )
                .await
                .unwrap();
            // The supervisor processes notifications only after completing its
            // initial reconcile, so this successful callback is an explicit barrier.
            assert_eq!(observed.recv().await, Some(("reloaded", "ok")));
            let mut previous_size = policy.max_file_size;
            for (table, expected_size) in
                [("system_config", 744), ("storage_policy_group_rules", 745)]
            {
                state
                    .db
                    .execute_unprepared(&format!(
                        "ALTER TABLE {table} RENAME TO retry_unavailable_table"
                    ))
                    .await
                    .unwrap();
                let mut updated = policy.clone().into_active_model();
                updated.max_file_size = Set(expected_size);
                updated.update(&state.db).await.unwrap();
                publisher
                    .publish_reload(
                        [super::STORAGE_TOPOLOGY_RELOAD_KEY],
                        aster_forge_config::ConfigNotificationSource::Api,
                    )
                    .await
                    .unwrap();
                // Keep the table unavailable until the actual callback reports
                // failure; a publication acknowledgement alone proves no DB read.
                assert_eq!(observed.recv().await, Some(("reloaded", "error")));
                assert!(!worker.is_finished());
                assert_eq!(
                    state
                        .policy_snapshot
                        .get_policy(policy.id)
                        .unwrap()
                        .max_file_size,
                    previous_size,
                );
                state
                    .db
                    .execute_unprepared(&format!(
                        "ALTER TABLE retry_unavailable_table RENAME TO {table}"
                    ))
                    .await
                    .unwrap();
                // Do not publish another hint. Reading the changed policy from
                // the snapshot proves the failed callback's retry republished it.
                while state
                    .policy_snapshot
                    .get_policy(policy.id)
                    .unwrap()
                    .max_file_size
                    != expected_size
                {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                previous_size = expected_size;
            }
        })
        .await
        .expect("notification retries must publish both policies before periodic reconciliation");
        shutdown.cancel();
        worker.await.unwrap().unwrap();
    }
}

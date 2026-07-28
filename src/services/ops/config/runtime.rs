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
    state.policy_snapshot().reload(state.writer_db()).await?;
    state.driver_registry().invalidate_all();
    super::system::invalidate_all_dependent_public_config_caches();
    Ok(())
}

pub async fn publish_storage_topology_reload(state: &impl SharedRuntimeState) -> Result<()> {
    state
        .config_sync()
        .publish_reload(
            [STORAGE_TOPOLOGY_RELOAD_KEY],
            aster_forge_config::ConfigNotificationSource::Other("storage_topology".to_string()),
        )
        .await
        .map_err(map_config_core_error)
}

pub async fn publish_storage_topology_reload_after_commit(
    state: &impl SharedRuntimeState,
    mutation: &'static str,
    entity_kind: &'static str,
    entity_id: i64,
) {
    publish_reload_after_commit(mutation, entity_kind, entity_id, "storage_topology", || {
        publish_storage_topology_reload(state)
    })
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

pub async fn run_config_reload_subscription<S>(
    state: Arc<S>,
    runtime: ConfigSyncRuntime,
    shutdown: CancellationToken,
) -> Result<()>
where
    S: SharedRuntimeState + Send + Sync + 'static,
{
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

    runtime
        .run_reload_subscription_with_reconcile_and_observers(
            shutdown,
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
        )
        .await
        .map_err(map_config_core_error)
}

pub(crate) fn map_config_core_error(error: aster_forge_config::ConfigCoreError) -> AsterError {
    AsterError::internal_error(format!("config sync failed: {error}"))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    use aster_drive_migration::Migrator;
    use aster_forge_config::ConfigSyncConfig;
    use sea_orm::{ActiveModelTrait, Set};

    use crate::runtime::SharedRuntimeState;

    struct ReloadTestState {
        db: sea_orm::DatabaseConnection,
        runtime_config: Arc<crate::config::RuntimeConfig>,
        driver_registry: Arc<crate::storage::DriverRegistry>,
        policy_snapshot: Arc<crate::storage::PolicySnapshot>,
        config: Arc<crate::config::Config>,
        cache: Arc<dyn aster_forge_cache::CacheBackend>,
        config_sync: aster_forge_config::ConfigSyncRuntime,
        metrics: crate::metrics::SharedMetricsRecorder,
    }

    impl SharedRuntimeState for ReloadTestState {
        fn writer_db(&self) -> &sea_orm::DatabaseConnection {
            &self.db
        }

        fn reader_db(&self) -> &sea_orm::DatabaseConnection {
            &self.db
        }

        fn driver_registry(&self) -> &Arc<crate::storage::DriverRegistry> {
            &self.driver_registry
        }

        fn runtime_config(&self) -> &Arc<crate::config::RuntimeConfig> {
            &self.runtime_config
        }

        fn policy_snapshot(&self) -> &Arc<crate::storage::PolicySnapshot> {
            &self.policy_snapshot
        }

        fn config(&self) -> &Arc<crate::config::Config> {
            &self.config
        }

        fn cache(&self) -> &Arc<dyn aster_forge_cache::CacheBackend> {
            &self.cache
        }

        fn config_sync(&self) -> &aster_forge_config::ConfigSyncRuntime {
            &self.config_sync
        }

        fn metrics(&self) -> &crate::metrics::SharedMetricsRecorder {
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

    #[tokio::test]
    async fn remote_notification_reloads_runtime_config_from_authoritative_database() {
        let db = crate::db::connect_with_metrics(
            &crate::config::DatabaseConfig {
                url: "sqlite::memory:".into(),
                pool_size: 1,
                retry_count: 0,
            },
            crate::metrics::NoopMetrics::arc(),
        )
        .await
        .expect("config reload test database should connect");
        Migrator::up(&db, None)
            .await
            .expect("config reload test migrations should apply");
        crate::db::repository::config_repo::ensure_defaults_with_env(&db, &|_| None)
            .await
            .expect("config reload test defaults should load");
        let now = chrono::Utc::now();
        let policy = crate::db::repository::policy_repo::create(
            &db,
            crate::entities::storage_policy::ActiveModel {
                name: Set("config reload policy".to_string()),
                driver_type: Set(crate::types::DriverType::S3),
                endpoint: Set("https://old.example.com".to_string()),
                bucket: Set("test".to_string()),
                access_key: Set("access".to_string()),
                secret_key: Set("secret".to_string()),
                base_path: Set(String::new()),
                max_file_size: Set(0),
                allowed_types: Set(crate::types::StoredStoragePolicyAllowedTypes::empty()),
                options: Set(crate::types::StoredStoragePolicyOptions::empty()),
                is_default: Set(true),
                chunk_size: Set(5_242_880),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
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
        let policy_snapshot = Arc::new(crate::storage::PolicySnapshot::new());
        policy_snapshot
            .reload(&db)
            .await
            .expect("initial policy snapshot should load");
        let config = Arc::new(crate::config::Config::default());
        let driver_registry = Arc::new(crate::storage::DriverRegistry::noop());
        driver_registry
            .reload_primary_state(&db, config.as_ref())
            .await
            .expect("initial driver registry state should load");
        let state = Arc::new(ReloadTestState {
            db: db.clone(),
            runtime_config: runtime_config.clone(),
            driver_registry,
            policy_snapshot: policy_snapshot.clone(),
            config,
            cache: aster_forge_cache::create_cache(&aster_forge_cache::CacheConfig::default())
                .await,
            config_sync: aster_forge_config::ConfigSyncRuntime::disabled_for_test("aster_drive"),
            metrics: crate::metrics::NoopMetrics::arc(),
        });

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
            state,
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
        let mut active: crate::entities::storage_policy::ActiveModel = policy.into();
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
        tokio::time::timeout(Duration::from_secs(2), async {
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

        shutdown.cancel();
        worker
            .await
            .expect("config reload worker should join")
            .expect("config reload worker should stop cleanly");
    }
}

//! Cross-process synchronization for runtime system configuration.

use std::sync::Arc;

use aster_forge_config::{
    ConfigReloadObservation, ConfigSyncConnectionObservation, ConfigSyncRuntime,
};
use tokio_util::sync::CancellationToken;

use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;

pub const CONFIG_RELOAD_NAMESPACE: &str = "aster_drive";

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
    use std::time::Duration;

    use aster_forge_config::ConfigSyncConfig;
    use migration::Migrator;

    use crate::runtime::SharedRuntimeState;

    struct ReloadTestState {
        db: sea_orm::DatabaseConnection,
        runtime_config: Arc<crate::config::RuntimeConfig>,
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
            panic!("config reload test must not access driver_registry")
        }

        fn runtime_config(&self) -> &Arc<crate::config::RuntimeConfig> {
            &self.runtime_config
        }

        fn policy_snapshot(&self) -> &Arc<crate::storage::PolicySnapshot> {
            panic!("config reload test must not access policy_snapshot")
        }

        fn config(&self) -> &Arc<crate::config::Config> {
            panic!("config reload test must not access static config")
        }

        fn cache(&self) -> &Arc<dyn aster_forge_cache::CacheBackend> {
            panic!("config reload test must not access cache")
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
                endpoint: String::new(),
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
    async fn remote_notification_reloads_runtime_config_from_authoritative_database() {
        let db = crate::db::connect_with_metrics(
            &crate::config::DatabaseConfig {
                url: "sqlite::memory:".to_string(),
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

        let runtime_config = Arc::new(crate::config::RuntimeConfig::new());
        runtime_config
            .reload(&db)
            .await
            .expect("initial runtime config should load");
        let state = Arc::new(ReloadTestState {
            db: db.clone(),
            runtime_config: runtime_config.clone(),
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

        shutdown.cancel();
        worker
            .await
            .expect("config reload worker should join")
            .expect("config reload worker should stop cleanly");
    }
}

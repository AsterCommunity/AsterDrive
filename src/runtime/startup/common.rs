use crate::config;
use crate::config::auth_runtime::AUTH_COOKIE_SECURE_KEY;
use crate::config::node_mode::NodeRuntimeMode;
use crate::db;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::storage::DriverRegistry;
use aster_drive_metrics::SharedMetricsRecorder;
use aster_drive_migration::Migrator;
use sea_orm::TransactionTrait;
use std::sync::Arc;

pub(super) struct CommonRuntimeParts {
    pub cfg: Arc<crate::config::Config>,
    pub db_handles: aster_forge_db::DbHandles,
    pub database: sea_orm::DatabaseConnection,
    pub driver_registry: Arc<DriverRegistry>,
    pub policy_snapshot: Arc<crate::storage::PolicySnapshot>,
    pub cache: Arc<dyn aster_forge_cache::CacheBackend>,
    pub config_sync: aster_forge_config::ConfigSyncRuntime,
    pub metrics: SharedMetricsRecorder,
}

pub(super) async fn prepare_common(mode: NodeRuntimeMode) -> Result<CommonRuntimeParts> {
    let cfg = config::get_config();
    crate::config::deployment::validate_static(cfg.as_ref())?;
    crate::services::mail::template::validate_template_registry()?;
    let metrics = aster_drive_metrics::create_metrics_recorder();
    let database = db::connect_with_metrics(&cfg.database, metrics.clone()).await?;
    let connector_registry =
        Arc::new(initialize_database_state(&database, cfg.as_ref(), mode).await?);
    if matches!(mode, NodeRuntimeMode::Primary) {
        crate::services::ops::deployment::validate_primary_topology(
            &connector_registry,
            &database,
            cfg.as_ref(),
        )
        .await?;
    }
    let db_handles = db::connect_reader_for_writer_with_metrics(
        &cfg.database,
        database.clone(),
        metrics.clone(),
    )
    .await?;

    let driver_registry = Arc::new(DriverRegistry::with_connectors(
        metrics.clone(),
        connector_registry,
    ));
    let policy_snapshot = Arc::new(crate::storage::PolicySnapshot::new());
    driver_registry
        .reload_policy_snapshot(&policy_snapshot, &database)
        .await?;
    match mode {
        NodeRuntimeMode::Primary => {
            driver_registry
                .reload_primary_state(&database, cfg.as_ref())
                .await?
        }
        NodeRuntimeMode::Follower => driver_registry.reload_follower_state(&database).await?,
    }

    let cache = create_runtime_cache(cfg.as_ref()).await?;
    let config_sync = aster_forge_config::build_config_sync_runtime(
        &cfg.config_sync,
        crate::services::ops::config::runtime::CONFIG_RELOAD_NAMESPACE,
    )
    .map_err(crate::services::ops::config::runtime::map_config_core_error)?;

    Ok(CommonRuntimeParts {
        cfg,
        db_handles,
        database,
        driver_registry,
        policy_snapshot,
        cache,
        config_sync,
        metrics,
    })
}

async fn create_runtime_cache(
    cfg: &crate::config::Config,
) -> Result<Arc<dyn aster_forge_cache::CacheBackend>> {
    let failure_policy = if cfg.deployment.requires_shared_runtime() {
        aster_forge_cache::CacheBackendFailurePolicy::ReturnError
    } else {
        aster_forge_cache::CacheBackendFailurePolicy::FallbackToMemory
    };

    aster_forge_cache::create_cache_with_policy(&cfg.cache, failure_policy)
        .await
        .map_err(|error| {
            AsterError::config_error(format!(
                "configured cache backend could not be created: {error}"
            ))
        })
}

pub async fn initialize_database_state(
    database: &sea_orm::DatabaseConnection,
    cfg: &crate::config::Config,
    mode: NodeRuntimeMode,
) -> Result<crate::storage::connectors::StorageConnectorRegistry> {
    Migrator::up(database, None)
        .await
        .map_aster_err(AsterError::database_operation)?;
    let connector_registry = crate::storage::connectors::builtin_storage_connector_registry()?;
    let upgrade_config = cfg.clone();
    let upgrade_connectors = connector_registry.clone();
    aster_drive_migration::with_database_migration_lock(database, move |connection| {
        Box::pin(async move {
            let credential_transaction = connection.begin().await?;
            crate::services::storage_policy::credential::migrate_legacy_storage_credentials(
                &credential_transaction,
                &upgrade_config,
                &upgrade_connectors,
            )
            .await
            .map_err(|error| sea_orm::DbErr::Custom(error.to_string()))?;
            credential_transaction.commit().await?;
            aster_drive_migration::finalize_storage_policy_upgrade(connection).await
        })
    })
    .await
    .map_err(|error| AsterError::database_operation(error.to_string()))?;

    if let Some(sqlite_search) = db::sqlite_search::ensure_sqlite_search_ready(database).await? {
        tracing::info!(
            sqlite_version = %sqlite_search.sqlite_version,
            "SQLite search acceleration ready"
        );
    }

    if matches!(mode, NodeRuntimeMode::Primary) {
        crate::services::storage_policy::policy::ensure_policy_groups_seeded(database).await?;
    }

    let bootstrap_cookie_secure = (!cfg.auth.bootstrap_insecure_cookies).to_string();
    crate::db::repository::config_repo::ensure_system_value_if_missing(
        database,
        AUTH_COOKIE_SECURE_KEY,
        &bootstrap_cookie_secure,
    )
    .await?;
    crate::db::repository::config_repo::ensure_defaults_with_env(database, &|name| {
        std::env::var(name).ok()
    })
    .await?;
    if matches!(mode, NodeRuntimeMode::Follower) {
        handle_optional_follower_bootstrap(
            crate::services::remote::node_enrollment::bootstrap_from_env_if_configured(database)
                .await,
        );
    }
    Ok(connector_registry)
}

fn handle_optional_follower_bootstrap<T>(result: Result<T>) {
    if let Err(error) = result {
        tracing::warn!(
            error = %error,
            master_url_env = crate::services::remote::node_enrollment::BOOTSTRAP_REMOTE_MASTER_URL_ENV,
            token_env = crate::services::remote::node_enrollment::BOOTSTRAP_REMOTE_ENROLLMENT_TOKEN_ENV,
            "follower enrollment bootstrap from environment failed; continuing startup without applying bootstrap env"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aster_forge_config::ConfigSource;
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter};
    use std::net::TcpListener;

    #[test]
    fn optional_follower_bootstrap_success_keeps_startup_flow() {
        handle_optional_follower_bootstrap::<()>(Ok(()));
    }

    #[test]
    fn optional_follower_bootstrap_error_does_not_abort_startup() {
        handle_optional_follower_bootstrap::<()>(Err(AsterError::validation_error(
            "enrollment token has already been completed",
        )));
    }

    #[tokio::test]
    async fn runtime_cache_construction_falls_back_only_for_single_profile() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .expect("cache startup test should reserve a local port");
        let unavailable_endpoint = format!(
            "redis://{}/0",
            listener
                .local_addr()
                .expect("cache startup test should resolve the local port")
        );
        drop(listener);

        let mut config = crate::config::Config::default();
        config.cache.backend = "redis".to_string();
        config.cache.endpoint = unavailable_endpoint.into();

        config.deployment.profile = crate::config::DeploymentProfile::Single;
        let cache = create_runtime_cache(&config)
            .await
            .expect("single profile should fall back when Redis is unavailable");
        assert_eq!(cache.backend_name(), "memory");
        cache
            .health_check()
            .await
            .expect("single profile memory fallback should be healthy");

        config.deployment.profile = crate::config::DeploymentProfile::Cluster;
        let error = create_runtime_cache(&config)
            .await
            .map(|_| ())
            .expect_err("cluster profile should require the configured Redis backend");
        let message = error.to_string();
        assert!(message.contains("configured cache backend could not be created"));
        assert!(message.contains("redis cache connection"));
    }

    #[tokio::test]
    async fn initialize_database_state_keeps_product_setup_storage_agnostic() {
        for profile in [
            crate::config::DeploymentProfile::Single,
            crate::config::DeploymentProfile::Cluster,
        ] {
            let db = crate::db::connect_with_metrics(
                &crate::config::DatabaseConfig {
                    url: "sqlite::memory:".into(),
                    pool_size: 1,
                    retry_count: 0,
                },
                aster_drive_metrics::NoopMetrics::arc(),
            )
            .await
            .unwrap();
            let config = crate::config::Config {
                auth: crate::config::AuthConfig {
                    bootstrap_insecure_cookies: true,
                    ..Default::default()
                },
                deployment: crate::config::DeploymentConfig {
                    profile,
                    ..Default::default()
                },
                ..Default::default()
            };

            initialize_database_state(&db, &config, NodeRuntimeMode::Primary)
                .await
                .unwrap();

            assert!(
                crate::db::repository::policy_repo::find_all(&db)
                    .await
                    .unwrap()
                    .is_empty()
            );
            let auth_cookie_secure =
                crate::db::repository::config_repo::find_by_key(&db, AUTH_COOKIE_SECURE_KEY)
                    .await
                    .unwrap()
                    .unwrap();
            assert_eq!(auth_cookie_secure.value, "false");

            let groups = aster_drive_model::entities::storage_policy_group::Entity::find()
                .all(&db)
                .await
                .unwrap();
            assert!(groups.is_empty());

            let obsolete = aster_forge_db::system_config::Entity::find()
                .filter(aster_forge_db::system_config::Column::Source.eq(ConfigSource::Custom))
                .all(&db)
                .await
                .unwrap();
            assert!(obsolete.is_empty());

            let schema = aster_drive_migration::SchemaManager::new(&db);
            for legacy_column in [
                "driver_type",
                "endpoint",
                "bucket",
                "access_key",
                "secret_key",
                "base_path",
                "remote_node_id",
                "remote_storage_target_key",
                "options",
            ] {
                assert!(
                    !schema
                        .has_column("storage_policies", legacy_column)
                        .await
                        .unwrap(),
                    "startup should remove legacy storage policy column {legacy_column}"
                );
            }
            crate::storage::connectors::test_support::insertable_policy(
                crate::storage::connectors::test_support::local_policy("./data/uploads"),
            )
            .insert(&db)
            .await
            .expect("current storage policy entity should insert after startup finalization");
        }
    }
}

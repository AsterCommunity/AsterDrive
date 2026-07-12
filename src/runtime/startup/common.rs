use crate::config;
use crate::config::auth_runtime::AUTH_COOKIE_SECURE_KEY;
use crate::config::node_mode::NodeRuntimeMode;
use crate::db;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::metrics::SharedMetricsRecorder;
use crate::storage::DriverRegistry;
use migration::Migrator;
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
    let metrics = crate::metrics::create_metrics_recorder();

    let database = db::connect_with_metrics(&cfg.database, metrics.clone()).await?;
    initialize_database_state(&database, cfg.as_ref(), mode).await?;
    let db_handles = db::connect_reader_for_writer_with_metrics(
        &cfg.database,
        database.clone(),
        metrics.clone(),
    )
    .await?;

    let policy_snapshot = Arc::new(crate::storage::PolicySnapshot::new());
    policy_snapshot.reload(&database).await?;

    let driver_registry = Arc::new(DriverRegistry::new(metrics.clone()));
    match mode {
        NodeRuntimeMode::Primary => {
            driver_registry
                .reload_primary_state(&database, cfg.as_ref())
                .await?
        }
        NodeRuntimeMode::Follower => driver_registry.reload_follower_state(&database).await?,
    }

    let cache = aster_forge_cache::create_cache(&cfg.cache).await;
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

pub async fn initialize_database_state(
    database: &sea_orm::DatabaseConnection,
    cfg: &crate::config::Config,
    mode: NodeRuntimeMode,
) -> Result<()> {
    Migrator::up(database, None)
        .await
        .map_aster_err(AsterError::database_operation)?;

    if let Some(sqlite_search) = db::sqlite_search::ensure_sqlite_search_ready(database).await? {
        tracing::info!(
            sqlite_version = %sqlite_search.sqlite_version,
            "SQLite search acceleration ready"
        );
    }

    ensure_default_policy(database).await?;
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
    Ok(())
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

async fn ensure_default_policy(db: &sea_orm::DatabaseConnection) -> Result<()> {
    use crate::db::repository::policy_repo;

    if policy_repo::find_default(db).await?.is_some() {
        return Ok(());
    }

    let all = policy_repo::find_all(db).await?;
    if !all.is_empty() {
        return Ok(());
    }

    let data_dir = "data/uploads";
    std::fs::create_dir_all(data_dir).map_aster_err(|e| {
        AsterError::storage_driver_error(format!("failed to create data dir '{}': {e}", data_dir))
    })?;

    use chrono::Utc;
    use sea_orm::Set;
    let now = Utc::now();
    let model = crate::entities::storage_policy::ActiveModel {
        name: Set("Local Default".to_string()),
        driver_type: Set(crate::types::DriverType::Local),
        endpoint: Set(String::new()),
        bucket: Set(String::new()),
        access_key: Set(String::new()),
        secret_key: Set(String::new()),
        base_path: Set(data_dir.to_string()),
        max_file_size: Set(0),
        allowed_types: Set(crate::types::StoredStoragePolicyAllowedTypes::empty()),
        options: Set(crate::types::StoredStoragePolicyOptions::empty()),
        is_default: Set(true),
        chunk_size: Set(5_242_880),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    policy_repo::create(db, model).await?;

    tracing::info!("created default local storage policy (data dir: {data_dir})");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::DriverType;
    use aster_forge_config::ConfigSource;
    use migration::Migrator;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, Set};

    async fn setup_db() -> sea_orm::DatabaseConnection {
        let db = crate::db::connect_with_metrics(
            &crate::config::DatabaseConfig {
                url: "sqlite::memory:".to_string(),
                pool_size: 1,
                retry_count: 0,
            },
            crate::metrics::NoopMetrics::arc(),
        )
        .await
        .unwrap();
        Migrator::up(&db, None).await.unwrap();
        db
    }

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
    async fn ensure_default_policy_creates_local_default_when_no_policies_exist() {
        let db = setup_db().await;

        ensure_default_policy(&db).await.unwrap();

        let default = crate::db::repository::policy_repo::find_default(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(default.name, "Local Default");
        assert_eq!(default.driver_type, DriverType::Local);
        assert!(default.is_default);
        assert_eq!(default.base_path, "data/uploads");
    }

    #[tokio::test]
    async fn ensure_default_policy_keeps_existing_non_default_policy() {
        let db = setup_db().await;
        let now = chrono::Utc::now();
        crate::db::repository::policy_repo::create(
            &db,
            crate::entities::storage_policy::ActiveModel {
                name: Set("Existing".to_string()),
                driver_type: Set(DriverType::Local),
                endpoint: Set(String::new()),
                bucket: Set(String::new()),
                access_key: Set(String::new()),
                secret_key: Set(String::new()),
                base_path: Set("existing".to_string()),
                max_file_size: Set(0),
                allowed_types: Set(crate::types::StoredStoragePolicyAllowedTypes::empty()),
                options: Set(crate::types::StoredStoragePolicyOptions::empty()),
                is_default: Set(false),
                chunk_size: Set(5_242_880),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        ensure_default_policy(&db).await.unwrap();

        let policies = crate::db::repository::policy_repo::find_all(&db)
            .await
            .unwrap();
        assert_eq!(policies.len(), 1);
        assert_eq!(policies[0].name, "Existing");
    }

    #[tokio::test]
    async fn initialize_database_state_seeds_primary_runtime_defaults() {
        let db = crate::db::connect_with_metrics(
            &crate::config::DatabaseConfig {
                url: "sqlite::memory:".to_string(),
                pool_size: 1,
                retry_count: 0,
            },
            crate::metrics::NoopMetrics::arc(),
        )
        .await
        .unwrap();
        let config = crate::config::Config {
            auth: crate::config::AuthConfig {
                bootstrap_insecure_cookies: true,
                ..Default::default()
            },
            ..Default::default()
        };

        initialize_database_state(&db, &config, NodeRuntimeMode::Primary)
            .await
            .unwrap();

        assert!(
            crate::db::repository::policy_repo::find_default(&db)
                .await
                .unwrap()
                .is_some()
        );
        let auth_cookie_secure =
            crate::db::repository::config_repo::find_by_key(&db, AUTH_COOKIE_SECURE_KEY)
                .await
                .unwrap()
                .unwrap();
        assert_eq!(auth_cookie_secure.value, "false");

        let groups = crate::entities::storage_policy_group::Entity::find()
            .all(&db)
            .await
            .unwrap();
        assert!(!groups.is_empty());

        let obsolete = aster_forge_db::system_config::Entity::find()
            .filter(aster_forge_db::system_config::Column::Source.eq(ConfigSource::Custom))
            .all(&db)
            .await
            .unwrap();
        assert!(obsolete.is_empty());
    }
}

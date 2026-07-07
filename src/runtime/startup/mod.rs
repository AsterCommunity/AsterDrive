//! 运行时子模块：`startup`。
mod common;
mod follower;
mod primary;

use crate::runtime::SharedRuntimeState;
use crate::services::ops::audit;

pub use common::initialize_database_state;
pub use follower::{PreparedFollowerRuntime, prepare_follower};
pub use primary::{PreparedPrimaryRuntime, prepare_primary};

/// 记录服务器启动事件。
pub async fn record_server_start<S: SharedRuntimeState>(state: &S) {
    audit::log(
        state,
        &audit::AuditContext::system(),
        audit::AuditAction::ServerStart,
        audit::AuditEntityType::SystemConfig,
        None,
        None,
        None,
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::record_server_start;
    use crate::runtime::FollowerAppState;
    use migration::Migrator;
    use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
    use std::sync::Arc;

    async fn follower_state() -> (FollowerAppState, sea_orm::DatabaseConnection) {
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

        let runtime_config = Arc::new(crate::config::RuntimeConfig::new());
        runtime_config
            .reload(&db)
            .await
            .expect("runtime config should load");
        let cache = aster_forge_cache::create_cache(&crate::config::CacheConfig {
            ..Default::default()
        })
        .await;

        let state = FollowerAppState {
            db_handles: crate::db::DbHandles::single(db.clone()),
            driver_registry: Arc::new(crate::storage::DriverRegistry::noop()),
            runtime_config,
            policy_snapshot: Arc::new(crate::storage::PolicySnapshot::new()),
            config: Arc::new(crate::config::Config::default()),
            cache,
            metrics: crate::metrics_core::NoopMetrics::arc(),
        };

        (state, db)
    }

    #[tokio::test]
    async fn follower_startup_audit_records_server_start() {
        let (state, db) = follower_state().await;

        record_server_start(&state).await;

        let count = crate::entities::audit_log::Entity::find()
            .filter(
                crate::entities::audit_log::Column::Action
                    .eq(crate::types::AuditAction::ServerStart),
            )
            .count(&db)
            .await
            .expect("audit log query should succeed");
        assert_eq!(count, 1);
    }
}

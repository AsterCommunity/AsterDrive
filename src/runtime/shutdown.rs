//! Product shutdown audit behavior used by the Forge runtime graph.

use crate::runtime::SharedRuntimeState;
use crate::services::ops::audit;

/// 记录服务器关闭事件。
pub async fn record_server_shutdown<S: SharedRuntimeState>(state: &S) {
    audit::log(
        state,
        &audit::AuditContext::system(),
        audit::AuditAction::ServerShutdown,
        audit::AuditEntityType::SystemConfig,
        None,
        None,
        None,
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::record_server_shutdown;
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
            crate::metrics::NoopMetrics::arc(),
        )
        .await
        .unwrap();
        Migrator::up(&db, None).await.unwrap();

        let runtime_config = Arc::new(crate::config::RuntimeConfig::new());
        runtime_config
            .reload(&db)
            .await
            .expect("runtime config should load");
        let cache = aster_forge_cache::create_cache(&aster_forge_cache::CacheConfig {
            ..Default::default()
        })
        .await;

        let state = FollowerAppState {
            db_handles: aster_forge_db::DbHandles::single(db.clone()),
            driver_registry: Arc::new(crate::storage::DriverRegistry::noop()),
            runtime_config,
            policy_snapshot: Arc::new(crate::storage::PolicySnapshot::new()),
            config: Arc::new(crate::config::Config::default()),
            cache,
            config_sync: aster_forge_config::ConfigSyncRuntime::disabled_for_test("aster_drive"),
            metrics: crate::metrics::NoopMetrics::arc(),
        };

        (state, db)
    }

    async fn audit_action_count(
        db: &sea_orm::DatabaseConnection,
        action: crate::types::AuditAction,
    ) -> u64 {
        crate::entities::audit_log::Entity::find()
            .filter(crate::entities::audit_log::Column::Action.eq(action))
            .count(db)
            .await
            .expect("audit log query should succeed")
    }

    #[tokio::test]
    async fn follower_shutdown_audit_records_server_shutdown() {
        let (state, db) = follower_state().await;

        record_server_shutdown(&state).await;

        assert_eq!(
            audit_action_count(&db, crate::types::AuditAction::ServerShutdown).await,
            1
        );
    }
}

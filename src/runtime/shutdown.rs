//! 运行时子模块：`shutdown`。

use super::tasks::BackgroundTasks;
use crate::runtime::SharedRuntimeState;
use crate::services::audit_service;
use sea_orm::DatabaseConnection;

/// 等待 SIGINT 或 SIGTERM 信号，然后进行优雅关闭
pub async fn wait_for_signal() {
    wait_for_termination_signal().await;
}

#[cfg(unix)]
async fn wait_for_termination_signal() {
    use tokio::signal::unix::{SignalKind, signal};

    let mut sigint = signal(SignalKind::interrupt()).expect("failed to install SIGINT handler");
    let mut sigterm = signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");

    tokio::select! {
        _ = sigint.recv() => tracing::info!("received SIGINT, shutting down gracefully..."),
        _ = sigterm.recv() => tracing::info!("received SIGTERM, shutting down gracefully..."),
    }
}

#[cfg(not(unix))]
async fn wait_for_termination_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install Ctrl+C handler");
    tracing::info!("received Ctrl+C, shutting down gracefully...");
}

/// 执行关闭收尾：确认后台任务停止，再关闭审计和数据库连接。
pub async fn perform_shutdown(background_tasks: BackgroundTasks, db: DatabaseConnection) {
    tracing::info!("stopping background tasks...");
    background_tasks.shutdown().await;
    tracing::info!("background tasks stopped");

    tracing::info!("flushing audit logs...");
    crate::services::audit_service::shutdown_global_audit_log_manager().await;

    tracing::info!("closing database connection...");
    if let Err(e) = db.close().await {
        tracing::error!("error closing database connection: {}", e);
    } else {
        tracing::info!("database connection closed");
    }
    tracing::info!("shutdown complete");
}

/// 记录服务器关闭事件。
pub async fn record_server_shutdown<S: SharedRuntimeState>(state: &S) {
    audit_service::log(
        state,
        &audit_service::AuditContext::system(),
        audit_service::AuditAction::ServerShutdown,
        audit_service::AuditEntityType::SystemConfig,
        None,
        None,
        None,
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::{perform_shutdown, record_server_shutdown};
    use crate::runtime::FollowerAppState;
    use crate::runtime::tasks::spawn_follower_background_tasks;
    use actix_web::web;
    use migration::Migrator;
    use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

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
        let cache = crate::cache::create_cache(&crate::config::CacheConfig {
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

    #[tokio::test]
    async fn perform_shutdown_stops_empty_background_tasks_and_closes_database() {
        let (state, db) = follower_state().await;
        let state = web::Data::new(state);

        perform_shutdown(
            spawn_follower_background_tasks(state, CancellationToken::new()),
            db,
        )
        .await;
    }
}

//! 运行时子模块：`shutdown`。

use super::tasks::BackgroundTasks;
use crate::runtime::SharedRuntimeState;
use crate::services::ops::audit;
use sea_orm::DatabaseConnection;

/// 等待 SIGINT 或 SIGTERM 信号，然后进行优雅关闭
pub async fn wait_for_signal() {
    wait_for_termination_signal().await;
}

#[cfg(unix)]
async fn wait_for_termination_signal() {
    use tokio::signal::unix::{SignalKind, signal};

    let mut sigint = match signal(SignalKind::interrupt()) {
        Ok(signal) => signal,
        Err(error) => {
            tracing::error!(%error, "failed to install SIGINT handler");
            wait_forever_after_signal_install_failure().await;
            return;
        }
    };
    let mut sigterm = match signal(SignalKind::terminate()) {
        Ok(signal) => signal,
        Err(error) => {
            tracing::error!(%error, "failed to install SIGTERM handler");
            wait_forever_after_signal_install_failure().await;
            return;
        }
    };

    tokio::select! {
        _ = sigint.recv() => tracing::info!("received SIGINT, shutting down gracefully..."),
        _ = sigterm.recv() => tracing::info!("received SIGTERM, shutting down gracefully..."),
    }
}

#[cfg(not(unix))]
async fn wait_for_termination_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(%error, "failed to install Ctrl+C handler");
        wait_forever_after_signal_install_failure().await;
        return;
    }
    tracing::info!("received Ctrl+C, shutting down gracefully...");
}

async fn wait_forever_after_signal_install_failure() {
    std::future::pending::<()>().await;
}

/// 执行关闭收尾：确认后台任务停止，再关闭审计和数据库连接。
pub async fn perform_shutdown(background_tasks: BackgroundTasks, db: DatabaseConnection) {
    tracing::info!("stopping background tasks...");
    background_tasks.shutdown().await;
    tracing::info!("background tasks stopped");

    tracing::info!("flushing audit logs...");
    crate::services::ops::audit::shutdown_global_audit_log_manager().await;

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

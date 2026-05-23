//! 运行时子模块：`shutdown`。

use super::tasks::BackgroundTasks;
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

/// 执行关闭流程：先停止后台任务，再关闭数据库连接并记录日志
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

#[cfg(test)]
mod tests {
    use super::perform_shutdown;
    use crate::runtime::tasks::spawn_follower_background_tasks;
    use migration::Migrator;

    #[tokio::test]
    async fn perform_shutdown_stops_empty_background_tasks_and_closes_database() {
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

        perform_shutdown(
            spawn_follower_background_tasks(crate::metrics_core::NoopMetrics::arc()),
            db,
        )
        .await;
    }
}

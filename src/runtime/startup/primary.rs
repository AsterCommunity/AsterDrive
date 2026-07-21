use super::common::prepare_common;
use crate::config::node_mode::NodeRuntimeMode;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use std::sync::Arc;

pub struct PreparedPrimaryRuntime {
    pub state: PrimaryAppState,
    pub share_download_rollback_worker: crate::services::share::ShareDownloadRollbackWorker,
}

/// 准备主节点运行时（配置和日志应在此之前初始化）
pub async fn prepare_primary() -> Result<PreparedPrimaryRuntime> {
    let common = prepare_common(NodeRuntimeMode::Primary).await?;

    let runtime_config = Arc::new(crate::config::RuntimeConfig::new());
    runtime_config.reload(&common.database).await?;
    let mail_sender = crate::services::mail::sender::runtime_sender(runtime_config.clone());
    let (storage_change_tx, _) = tokio::sync::broadcast::channel(
        crate::services::events::storage_change::STORAGE_CHANGE_CHANNEL_CAPACITY,
    );
    let storage_change_bus =
        crate::services::events::storage_change::build_cross_instance_bus(&common.cfg.config_sync)?;
    let rollback_queue_capacity =
        crate::config::operations::share_download_rollback_queue_capacity(&runtime_config);
    let (share_download_rollback, share_download_rollback_worker) =
        crate::services::share::build_share_download_rollback_queue(
            common.database.clone(),
            rollback_queue_capacity,
            common.metrics.clone(),
        );
    aster_forge_audit::init_global_audit_log_manager(common.database.clone());

    let remote_protocol = crate::runtime::PrimaryAppState::new_remote_protocol();
    remote_protocol.set_persistence_db(common.database.clone());
    common
        .driver_registry
        .set_remote_protocol(remote_protocol.clone());

    tracing::info!(
        mode = NodeRuntimeMode::Primary.as_str(),
        "startup complete — listening on {}:{}",
        common.cfg.server.host,
        common.cfg.server.port
    );

    Ok(PreparedPrimaryRuntime {
        state: PrimaryAppState {
            db_handles: common.db_handles,
            driver_registry: common.driver_registry,
            runtime_config,
            policy_snapshot: common.policy_snapshot,
            config: common.cfg,
            cache: common.cache,
            config_sync: common.config_sync,
            metrics: common.metrics,
            mail_sender,
            storage_change_tx,
            storage_change_bus,
            share_download_rollback,
            background_task_dispatch_wakeup:
                crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
            remote_protocol,
            upload_runtime: crate::runtime::PrimaryAppState::new_upload_runtime(),
        },
        share_download_rollback_worker,
    })
}

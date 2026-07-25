use super::common::prepare_common;
use crate::config::node_mode::NodeRuntimeMode;
use crate::errors::Result;
use crate::runtime::FollowerAppState;
use std::sync::Arc;

pub struct PreparedFollowerRuntime {
    pub state: FollowerAppState,
}

/// 准备从节点运行时（配置和日志应在此之前初始化）
pub async fn prepare_follower() -> Result<PreparedFollowerRuntime> {
    let common = prepare_common(NodeRuntimeMode::Follower).await?;
    let runtime_config = Arc::new(crate::config::RuntimeConfig::new());
    runtime_config.reload(&common.database).await?;
    aster_forge_audit::init_global_audit_log_manager(common.database.clone());

    tracing::debug!(
        mode = NodeRuntimeMode::Follower.as_str(),
        "follower runtime state prepared"
    );

    Ok(PreparedFollowerRuntime {
        state: FollowerAppState {
            db_handles: common.db_handles,
            driver_registry: common.driver_registry,
            runtime_config,
            policy_snapshot: common.policy_snapshot,
            config: common.cfg,
            cache: common.cache,
            config_sync: common.config_sync,
            metrics: common.metrics,
        },
    })
}

use super::common::prepare_common;
use crate::config::node_mode::NodeRuntimeMode;
use crate::errors::Result;
use crate::runtime::FollowerAppState;

pub struct PreparedFollowerRuntime {
    pub state: FollowerAppState,
}

/// 准备从节点运行时（配置和日志应在此之前初始化）
pub async fn prepare_follower() -> Result<PreparedFollowerRuntime> {
    let common = prepare_common(NodeRuntimeMode::Follower).await?;

    tracing::info!(
        mode = NodeRuntimeMode::Follower.as_str(),
        "startup complete — listening on {}:{}",
        common.cfg.server.host,
        common.cfg.server.port
    );

    Ok(PreparedFollowerRuntime {
        state: FollowerAppState {
            db_handles: common.db_handles,
            driver_registry: common.driver_registry,
            policy_snapshot: common.policy_snapshot,
            config: common.cfg,
            cache: common.cache,
            metrics: common.metrics,
        },
    })
}

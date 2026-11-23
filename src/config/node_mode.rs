//! 节点启动模式配置。

use crate::config::Config;
use serde::{Deserialize, Serialize};

// Keep the follower failure detector and its configuration lower bound tied to
// the control-plane cadence rather than duplicating independent magic numbers.
pub(crate) const FOLLOWER_BINDING_RECONCILE_INTERVAL: std::time::Duration =
    std::time::Duration::from_secs(30);
pub(crate) const FOLLOWER_BINDING_CONTROL_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(30);
pub(crate) const FOLLOWER_BINDING_CONTROL_CONCURRENCY: usize = 8;
pub(crate) const FOLLOWER_MASTER_READINESS_MIN_SECS: u64 =
    FOLLOWER_BINDING_RECONCILE_INTERVAL.as_secs() + FOLLOWER_BINDING_CONTROL_TIMEOUT.as_secs();

/// Return the effective freshness floor needed to observe one complete pull
/// round when control-plane requests are bounded in batches.
pub(crate) const fn follower_master_readiness_min_secs(binding_count: usize) -> u64 {
    let batches = binding_count.div_ceil(FOLLOWER_BINDING_CONTROL_CONCURRENCY);
    let batches = if batches == 0 { 1 } else { batches } as u64;
    FOLLOWER_BINDING_RECONCILE_INTERVAL.as_secs()
        + FOLLOWER_BINDING_CONTROL_TIMEOUT.as_secs() * (batches + 1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum NodeRuntimeMode {
    #[default]
    Primary,
    Follower,
}

impl NodeRuntimeMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Follower => "follower",
        }
    }
}

pub fn parse_node_runtime_mode(value: &str) -> Option<NodeRuntimeMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "primary" => Some(NodeRuntimeMode::Primary),
        "follower" => Some(NodeRuntimeMode::Follower),
        _ => None,
    }
}

pub fn start_mode(config: &Config) -> NodeRuntimeMode {
    config.server.start_mode
}

#[cfg(test)]
mod tests {
    use super::{
        NodeRuntimeMode, follower_master_readiness_min_secs, parse_node_runtime_mode, start_mode,
    };
    use crate::config::Config;

    #[test]
    fn parse_node_runtime_mode_accepts_supported_values() {
        assert_eq!(
            parse_node_runtime_mode(" follower "),
            Some(NodeRuntimeMode::Follower)
        );
        assert_eq!(
            parse_node_runtime_mode("PRIMARY"),
            Some(NodeRuntimeMode::Primary)
        );
        assert_eq!(parse_node_runtime_mode("worker"), None);
    }

    #[test]
    fn start_mode_defaults_to_primary() {
        let config = Config::default();
        assert_eq!(start_mode(&config), NodeRuntimeMode::Primary);
    }

    #[test]
    fn start_mode_reads_follower_value() {
        let mut config = Config::default();
        config.server.start_mode = NodeRuntimeMode::Follower;

        assert_eq!(start_mode(&config), NodeRuntimeMode::Follower);
    }

    #[test]
    fn readiness_floor_covers_control_pull_batches() {
        assert_eq!(follower_master_readiness_min_secs(0), 90);
        assert_eq!(follower_master_readiness_min_secs(8), 90);
        assert_eq!(follower_master_readiness_min_secs(9), 120);
        assert_eq!(follower_master_readiness_min_secs(16), 120);
        assert_eq!(follower_master_readiness_min_secs(17), 150);
        assert_eq!(follower_master_readiness_min_secs(33), 210);
    }
}

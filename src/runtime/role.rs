//! Process responsibilities are independent from the configured Primary/Follower node mode.

use crate::config::node_mode::NodeRuntimeMode;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum ServeRole {
    #[default]
    All,
    Api,
    Worker,
}

impl ServeRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Api => "api",
            Self::Worker => "worker",
        }
    }
    pub const fn serves_api(self) -> bool {
        !matches!(self, Self::Worker)
    }
    pub const fn consumes_tasks(self) -> bool {
        !matches!(self, Self::Api)
    }

    pub fn validate(self, mode: NodeRuntimeMode) -> crate::errors::Result<()> {
        if mode == NodeRuntimeMode::Follower && self != Self::All {
            return Err(crate::errors::AsterError::config_error(format!(
                "serve {} requires server.start_mode = primary; Follower responsibilities are selected by config",
                self.as_str()
            )));
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct RoleHealth {
    pub role: ServeRole,
    pub consumers: Vec<aster_forge_tasks::TaskRuntimeStatus>,
    pub shutdown: tokio_util::sync::CancellationToken,
    pub initialized: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl RoleHealth {
    pub fn ready(&self) -> bool {
        self.initialized.load(std::sync::atomic::Ordering::Acquire)
            && (!self.role.consumes_tasks() || !self.consumers.is_empty())
            && !self.shutdown.is_cancelled()
            && self
                .consumers
                .iter()
                .all(aster_forge_tasks::TaskRuntimeStatus::ready)
    }
    pub fn live(&self) -> bool {
        self.initialized.load(std::sync::atomic::Ordering::Acquire)
            && (self.shutdown.is_cancelled()
                || ((!self.role.consumes_tasks() || !self.consumers.is_empty())
                    && self
                        .consumers
                        .iter()
                        .all(aster_forge_tasks::TaskRuntimeStatus::is_running)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn node_mode_and_role_matrix() {
        for role in [ServeRole::All, ServeRole::Api, ServeRole::Worker] {
            assert!(role.validate(NodeRuntimeMode::Primary).is_ok());
            assert_eq!(
                role.validate(NodeRuntimeMode::Follower).is_ok(),
                role == ServeRole::All
            );
            assert_eq!(role.serves_api(), role != ServeRole::Worker);
            assert_eq!(role.consumes_tasks(), role != ServeRole::Api);
        }
    }
    #[test]
    fn draining_and_unstarted_consumers_are_not_ready() {
        let mut health = RoleHealth {
            role: ServeRole::Api,
            consumers: vec![],
            shutdown: Default::default(),
            initialized: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
        };
        assert!(health.ready());
        health.consumers.push(Default::default());
        assert!(!health.ready());
        health.consumers.clear();
        health.shutdown.cancel();
        assert!(!health.ready());
    }
}

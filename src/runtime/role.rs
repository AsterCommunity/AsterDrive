//! Process responsibilities are independent from the configured Primary/Follower node mode.

use crate::config::node_mode::NodeRuntimeMode;

/// Subset of node responsibilities a single process serves, selected by CLI.
///
/// Responsibilities default to all of them and are orthogonal to the
/// configured `NodeRuntimeMode`: a Follower node always serves `All`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum ServeRole {
    /// Serve API routes and background task workers in one process.
    #[default]
    All,
    /// Serve API routes only; no task consumers are started.
    Api,
    /// Run background task workers only; API routes are replaced by health routes.
    Worker,
}

impl ServeRole {
    /// Stable lowercase name used in logs and metrics.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Api => "api",
            Self::Worker => "worker",
        }
    }

    /// Whether this role exposes the primary API routes.
    pub const fn serves_api(self) -> bool {
        !matches!(self, Self::Worker)
    }

    /// Whether this role starts background task consumers.
    pub const fn consumes_tasks(self) -> bool {
        !matches!(self, Self::Api)
    }

    /// Rejects role/mode combinations that cannot boot, e.g. a partial role on
    /// a Follower node whose responsibilities are fixed by configuration.
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

/// Liveness and readiness signals for the responsibilities selected by
/// [`ServeRole`], consumed by health routes.
#[derive(Clone)]
pub struct RoleHealth {
    /// Responsibilities this process serves.
    pub role: ServeRole,
    /// Status of each registered task consumer; empty when the role does not
    /// consume tasks.
    pub consumers: Vec<aster_forge_tasks::TaskRuntimeStatus>,
    /// Cancelled during graceful shutdown; readiness turns off while liveness
    /// stays on until draining completes.
    pub shutdown: tokio_util::sync::CancellationToken,
    /// Set once startup has finished initializing the selected responsibilities.
    pub initialized: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl RoleHealth {
    /// Whether the process may receive traffic: startup finished, all task
    /// consumers are ready, and shutdown has not been requested.
    pub fn ready(&self) -> bool {
        self.initialized.load(std::sync::atomic::Ordering::Acquire)
            && (!self.role.consumes_tasks() || !self.consumers.is_empty())
            && !self.shutdown.is_cancelled()
            && self
                .consumers
                .iter()
                .all(aster_forge_tasks::TaskRuntimeStatus::ready)
    }

    /// Whether the process is alive: initialized and either still draining
    /// after a shutdown request or all task consumers keep running.
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

//! Static per-process shutdown budgets. Shared quotas live in authoritative system_config.

use serde::{Deserialize, Serialize};

/// Per-process task consumer shutdown budgets (`[worker]`).
///
/// Shared cluster-wide quotas are runtime configuration in the authoritative
/// system_config table, not static file settings.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct WorkerConfig {
    /// Grace period, in seconds, for in-flight tasks to finish during shutdown.
    pub drain_timeout_secs: u64,
    /// Additional grace period, in seconds, before cancellation is forced
    /// once draining has stopped accepting new work.
    pub cancellation_timeout_secs: u64,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            drain_timeout_secs: 20,
            cancellation_timeout_secs: 5,
        }
    }
}

impl WorkerConfig {
    /// Rejects shutdown budgets that overflow or exceed 24 hours in total.
    pub fn validate(&self) -> crate::errors::Result<()> {
        if self
            .drain_timeout_secs
            .checked_add(self.cancellation_timeout_secs)
            .is_none_or(|sum| sum > 86_400)
        {
            return Err(crate::errors::AsterError::config_error(
                "worker shutdown budget must not exceed 24 hours",
            ));
        }
        Ok(())
    }

    /// Maps the configured budgets onto the Forge task consumer settings.
    pub fn consumer_config(&self) -> aster_forge_tasks::TaskConsumerConfig {
        aster_forge_tasks::TaskConsumerConfig {
            drain_timeout: std::time::Duration::from_secs(self.drain_timeout_secs),
            cancellation_timeout: std::time::Duration::from_secs(self.cancellation_timeout_secs),
        }
    }
}

impl aster_forge_config::ValidatableConfig for WorkerConfig {
    fn validate(&self) -> aster_forge_config::Result<()> {
        Self::validate(self).map_err(|error| {
            aster_forge_config::ConfigCoreError::invalid_value(error.message().to_string())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budgets_validate_overflow_and_immediate_shutdown() {
        assert!(WorkerConfig::default().validate().is_ok());
        for (drain, cancel, valid) in [
            (0, 0, true),
            (86_400, 0, true),
            (86_400, 1, false),
            (u64::MAX, 1, false),
        ] {
            let cfg = WorkerConfig {
                drain_timeout_secs: drain,
                cancellation_timeout_secs: cancel,
            };
            assert_eq!(cfg.validate().is_ok(), valid);
        }
    }

    #[test]
    fn cluster_quota_bounds_and_unlimited_value() {
        use crate::config::operations::{
            normalize_cluster_concurrency_config_value, parse_cluster_concurrency,
        };
        assert_eq!(parse_cluster_concurrency("quota", "0").unwrap(), None);
        assert_eq!(parse_cluster_concurrency("quota", "12").unwrap(), Some(12));
        for value in ["-1", "65537", "1.5", "invalid", "184467440737095516160"] {
            assert!(normalize_cluster_concurrency_config_value("quota", value).is_err());
        }
    }
}

//! Static per-process shutdown budgets. Shared quotas live in authoritative system_config.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct WorkerConfig {
    pub drain_timeout_secs: u64,
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

    pub fn consumer_config(&self) -> aster_forge_tasks::TaskConsumerConfig {
        aster_forge_tasks::TaskConsumerConfig {
            drain_timeout: std::time::Duration::from_secs(self.drain_timeout_secs),
            cancellation_timeout: std::time::Duration::from_secs(self.cancellation_timeout_secs),
        }
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

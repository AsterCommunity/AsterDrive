//! 服务模块：`ops::health`。

use crate::config::CacheConfig;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::{FollowerRuntimeState, RemoteProtocolRuntimeState, SharedRuntimeState};
use crate::services::task::types::{
    RuntimeSystemHealthComponent, RuntimeSystemHealthResult, RuntimeSystemHealthStatus,
};
use crate::services::{remote::remote_node, task};
use sea_orm::DatabaseConnection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

impl HealthStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Degraded => "degraded",
            Self::Unhealthy => "unhealthy",
        }
    }

    fn is_issue(self) -> bool {
        !matches!(self, Self::Healthy)
    }

    fn into_runtime_status(self) -> RuntimeSystemHealthStatus {
        match self {
            Self::Healthy => RuntimeSystemHealthStatus::Healthy,
            Self::Degraded => RuntimeSystemHealthStatus::Degraded,
            Self::Unhealthy => RuntimeSystemHealthStatus::Unhealthy,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthComponentReport {
    pub name: &'static str,
    pub status: HealthStatus,
    pub message: String,
}

impl HealthComponentReport {
    fn healthy(name: &'static str, message: impl Into<String>) -> Self {
        Self {
            name,
            status: HealthStatus::Healthy,
            message: message.into(),
        }
    }

    fn unhealthy(name: &'static str, message: impl Into<String>) -> Self {
        Self {
            name,
            status: HealthStatus::Unhealthy,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SystemHealthReport {
    pub components: Vec<HealthComponentReport>,
}

impl SystemHealthReport {
    pub fn has_issues(&self) -> bool {
        self.components
            .iter()
            .any(|component| component.status.is_issue())
    }

    pub fn summary(&self) -> String {
        if self.components.is_empty() {
            return "system health check did not run any components".to_string();
        }

        self.components
            .iter()
            .map(|component| format!("{} {}", component.name, component.status.as_str()))
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn details(&self) -> String {
        self.components
            .iter()
            .map(|component| {
                format!(
                    "{}={}: {}",
                    component.name,
                    component.status.as_str(),
                    component.message
                )
            })
            .collect::<Vec<_>>()
            .join("; ")
    }

    pub fn into_runtime_outcome(self) -> task::RuntimeTaskRunOutcome {
        let summary = if self.has_issues() {
            self.issue_summary()
        } else {
            "system healthy".to_string()
        };
        let system_health = self.to_runtime_result();

        if self.has_issues() {
            task::RuntimeTaskRunOutcome::failed_with_system_health(
                Some(summary),
                self.issue_details(),
                system_health,
            )
        } else {
            task::RuntimeTaskRunOutcome::succeeded_with_system_health(Some(summary), system_health)
        }
    }

    fn to_runtime_result(&self) -> RuntimeSystemHealthResult {
        let status = if self
            .components
            .iter()
            .any(|component| matches!(component.status, HealthStatus::Unhealthy))
        {
            RuntimeSystemHealthStatus::Unhealthy
        } else if self
            .components
            .iter()
            .any(|component| matches!(component.status, HealthStatus::Degraded))
        {
            RuntimeSystemHealthStatus::Degraded
        } else {
            RuntimeSystemHealthStatus::Healthy
        };

        RuntimeSystemHealthResult {
            status,
            components: self
                .components
                .iter()
                .map(|component| RuntimeSystemHealthComponent {
                    name: component.name.to_string(),
                    status: component.status.into_runtime_status(),
                    message: component.message.clone(),
                })
                .collect(),
        }
    }

    fn issue_summary(&self) -> String {
        let summary = self
            .components
            .iter()
            .filter(|component| component.status.is_issue())
            .map(|component| format!("{} {}", component.name, component.status.as_str()))
            .collect::<Vec<_>>()
            .join(", ");

        if summary.is_empty() {
            self.summary()
        } else {
            summary
        }
    }

    fn issue_details(&self) -> String {
        let details = self
            .components
            .iter()
            .filter(|component| component.status.is_issue())
            .map(|component| {
                format!(
                    "{}={}: {}",
                    component.name,
                    component.status.as_str(),
                    component.message
                )
            })
            .collect::<Vec<_>>()
            .join("; ");

        if details.is_empty() {
            self.details()
        } else {
            details
        }
    }
}

pub async fn ping_database(db: &DatabaseConnection) -> Result<()> {
    db.ping()
        .await
        .map_aster_err(AsterError::database_operation)
}

pub async fn check_primary_ready<S: SharedRuntimeState>(state: &S) -> Result<()> {
    let policy = state
        .policy_snapshot()
        .system_default_policy()
        .ok_or_else(|| {
            AsterError::storage_policy_not_found("system default storage policy not found")
        })?;
    let driver = state.driver_registry().get_driver(&policy)?;
    driver.readiness_check().await
}

pub async fn check_follower_ready<S: FollowerRuntimeState>(state: &S) -> Result<()> {
    crate::services::remote::master_binding::assert_follower_ready(state).await
}

pub async fn run_primary_system_health_checks<S: RemoteProtocolRuntimeState>(
    state: &S,
) -> SystemHealthReport {
    let mut components = Vec::with_capacity(3);
    components.push(check_database_component(state.writer_db()).await);
    components.push(check_cache_component(state).await);
    components.push(check_remote_nodes_component(state).await);
    SystemHealthReport { components }
}

async fn check_database_component(db: &DatabaseConnection) -> HealthComponentReport {
    match ping_database(db).await {
        Ok(()) => HealthComponentReport::healthy("database", "database ping succeeded"),
        Err(error) => {
            HealthComponentReport::unhealthy("database", format!("database ping failed: {error}"))
        }
    }
}

async fn check_cache_component<S: SharedRuntimeState>(state: &S) -> HealthComponentReport {
    check_cache_backend(&state.config().cache, state.cache().as_ref()).await
}

async fn check_cache_backend(
    config: &CacheConfig,
    cache: &dyn aster_forge_cache::CacheBackend,
) -> HealthComponentReport {
    let report = aster_forge_cache::check_cache_component(config, cache).await;
    let status = match report.status.as_str() {
        "healthy" => HealthStatus::Healthy,
        "degraded" => HealthStatus::Degraded,
        _ => HealthStatus::Unhealthy,
    };

    HealthComponentReport {
        name: report.name,
        status,
        message: report.message,
    }
}

async fn check_remote_nodes_component<S: RemoteProtocolRuntimeState>(
    state: &S,
) -> HealthComponentReport {
    match remote_node::run_health_tests(state).await {
        Ok(stats) if stats.failed > 0 => HealthComponentReport::unhealthy(
            "remote_nodes",
            format!(
                "checked {} remote nodes: {} healthy, {} failed, {} skipped",
                stats.checked, stats.healthy, stats.failed, stats.skipped
            ),
        ),
        Ok(stats) if stats.checked > 0 => HealthComponentReport::healthy(
            "remote_nodes",
            format!(
                "checked {} remote nodes: {} healthy, {} failed, {} skipped",
                stats.checked, stats.healthy, stats.failed, stats.skipped
            ),
        ),
        Ok(stats) => HealthComponentReport::healthy(
            "remote_nodes",
            format!(
                "no eligible remote nodes checked, {} skipped",
                stats.skipped
            ),
        ),
        Err(error) => HealthComponentReport::unhealthy(
            "remote_nodes",
            format!("remote node health tests failed: {error}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{HealthComponentReport, HealthStatus, SystemHealthReport, check_cache_backend};
    use aster_forge_cache::{CacheBackend, CacheError};
    use crate::config::CacheConfig;
    use crate::services::task::{RuntimeTaskRunOutcome, types::RuntimeSystemHealthStatus};
    use async_trait::async_trait;

    struct FakeCache {
        backend_name: &'static str,
        healthy: bool,
    }

    impl FakeCache {
        fn new(backend_name: &'static str) -> Self {
            Self {
                backend_name,
                healthy: true,
            }
        }

        fn unhealthy(backend_name: &'static str) -> Self {
            Self {
                backend_name,
                healthy: false,
            }
        }
    }

    #[async_trait]
    impl CacheBackend for FakeCache {
        fn backend_name(&self) -> &'static str {
            self.backend_name
        }

        async fn health_check(&self) -> aster_forge_cache::Result<()> {
            if self.healthy {
                Ok(())
            } else {
                Err(CacheError::RedisHealthCheck("cache probe failed".to_string()))
            }
        }

        async fn get_bytes(&self, _key: &str) -> Option<Vec<u8>> {
            None
        }

        async fn take_bytes(&self, _key: &str) -> Option<Vec<u8>> {
            None
        }

        async fn set_bytes(&self, _key: &str, _value: Vec<u8>, _ttl_secs: Option<u64>) {}

        async fn set_bytes_if_absent(
            &self,
            _key: &str,
            _value: Vec<u8>,
            _ttl_secs: Option<u64>,
        ) -> bool {
            false
        }

        async fn delete(&self, _key: &str) {}

        async fn invalidate_prefix(&self, _prefix: &str) {}
    }

    #[test]
    fn system_health_report_has_issues_when_component_is_degraded() {
        let report = SystemHealthReport {
            components: vec![HealthComponentReport {
                name: "cache",
                status: HealthStatus::Degraded,
                message: "fallback active".to_string(),
            }],
        };

        assert!(report.has_issues());
        assert_eq!(report.summary(), "cache degraded");
        assert_eq!(report.details(), "cache=degraded: fallback active");
    }

    #[test]
    fn system_health_report_is_healthy_when_all_components_are_healthy() {
        let report = SystemHealthReport {
            components: vec![HealthComponentReport {
                name: "database",
                status: HealthStatus::Healthy,
                message: "ok".to_string(),
            }],
        };

        assert!(!report.has_issues());
        assert_eq!(report.summary(), "database healthy");
    }

    #[test]
    fn runtime_outcome_uses_compact_summary_when_system_is_healthy() {
        let report = SystemHealthReport {
            components: vec![
                HealthComponentReport {
                    name: "database",
                    status: HealthStatus::Healthy,
                    message: "database ping succeeded".to_string(),
                },
                HealthComponentReport {
                    name: "cache",
                    status: HealthStatus::Healthy,
                    message: "cache probe succeeded".to_string(),
                },
            ],
        };

        let outcome = report.into_runtime_outcome();

        match outcome {
            RuntimeTaskRunOutcome::Succeeded {
                summary,
                system_health,
            } => {
                assert_eq!(summary, Some("system healthy".to_string()));
                let system_health = system_health.expect("system health metadata should exist");
                assert_eq!(system_health.status, RuntimeSystemHealthStatus::Healthy);
                assert_eq!(system_health.components.len(), 2);
            }
            other => panic!("expected succeeded system health outcome, got {other:?}"),
        }
    }

    #[test]
    fn runtime_outcome_reports_only_problem_components() {
        let report = SystemHealthReport {
            components: vec![
                HealthComponentReport {
                    name: "database",
                    status: HealthStatus::Healthy,
                    message: "database ping succeeded".to_string(),
                },
                HealthComponentReport {
                    name: "cache",
                    status: HealthStatus::Degraded,
                    message: "fallback active".to_string(),
                },
            ],
        };

        let outcome = report.into_runtime_outcome();

        match outcome {
            RuntimeTaskRunOutcome::Failed {
                summary,
                error,
                system_health,
            } => {
                assert_eq!(summary, Some("cache degraded".to_string()));
                assert_eq!(error, "cache=degraded: fallback active");
                let system_health = system_health.expect("system health metadata should exist");
                assert_eq!(system_health.status, RuntimeSystemHealthStatus::Degraded);
                assert_eq!(system_health.components[1].name, "cache");
                assert_eq!(
                    system_health.components[1].status,
                    RuntimeSystemHealthStatus::Degraded
                );
            }
            other => panic!("expected failed system health outcome, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn cache_report_is_degraded_when_configured_backend_falls_back() {
        let config = CacheConfig {
            backend: "redis".to_string(),
            endpoint: "redis://example.com:6379/0".to_string(),
            default_ttl: 60,
        };
        let cache = FakeCache::new("memory");

        let report = check_cache_backend(&config, &cache).await;

        assert_eq!(report.name, "cache");
        assert_eq!(report.status, HealthStatus::Degraded);
        assert_eq!(
            report.message,
            "configured cache backend 'redis' is using active backend 'memory'"
        );
    }

    #[tokio::test]
    async fn cache_report_is_unhealthy_when_backend_probe_fails() {
        let config = CacheConfig {
            backend: "redis".to_string(),
            endpoint: "redis://example.com:6379/0".to_string(),
            default_ttl: 60,
        };
        let cache = FakeCache::unhealthy("redis");

        let report = check_cache_backend(&config, &cache).await;

        assert_eq!(report.name, "cache");
        assert_eq!(report.status, HealthStatus::Unhealthy);
        assert!(
            report
                .message
                .contains("cache backend 'redis' health check failed")
        );
    }
}

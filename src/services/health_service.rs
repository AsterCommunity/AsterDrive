//! 服务模块：`health_service`。

use crate::cache::CacheBackend;
use crate::config::CacheConfig;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::{FollowerRuntimeState, PrimaryRuntimeState};
use crate::services::{managed_follower_service, task_service};
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

    fn degraded(name: &'static str, message: impl Into<String>) -> Self {
        Self {
            name,
            status: HealthStatus::Degraded,
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

    pub fn into_runtime_outcome(self) -> task_service::RuntimeTaskRunOutcome {
        let summary = Some(self.summary());
        if self.has_issues() {
            task_service::RuntimeTaskRunOutcome::failed(summary, self.details())
        } else {
            task_service::RuntimeTaskRunOutcome::succeeded(summary)
        }
    }
}

pub async fn ping_database(db: &DatabaseConnection) -> Result<()> {
    db.ping()
        .await
        .map_aster_err(AsterError::database_operation)
}

pub async fn check_primary_ready<S: PrimaryRuntimeState>(state: &S) -> Result<()> {
    crate::services::policy_service::test_default_connection(state).await
}

pub async fn check_follower_ready<S: FollowerRuntimeState>(state: &S) -> Result<()> {
    crate::services::master_binding_service::assert_follower_ready(state).await
}

pub async fn run_primary_system_health_checks<S: PrimaryRuntimeState>(
    state: &S,
) -> SystemHealthReport {
    let mut components = Vec::with_capacity(3);
    components.push(check_database_component(state.db()).await);
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

async fn check_cache_component<S: PrimaryRuntimeState>(state: &S) -> HealthComponentReport {
    check_cache_backend(&state.config().cache, state.cache().as_ref()).await
}

async fn check_cache_backend(
    config: &CacheConfig,
    cache: &dyn CacheBackend,
) -> HealthComponentReport {
    let active_backend = cache.backend_name();

    if !config.enabled {
        return HealthComponentReport::healthy("cache", "cache is disabled by configuration");
    }

    if let Err(error) = cache.health_check().await {
        return HealthComponentReport::unhealthy(
            "cache",
            format!("cache backend '{active_backend}' health check failed: {error}"),
        );
    }

    if config.backend.as_str() != active_backend {
        return HealthComponentReport::degraded(
            "cache",
            format!(
                "configured cache backend '{}' is using active backend '{}'",
                config.backend, active_backend
            ),
        );
    }

    HealthComponentReport::healthy(
        "cache",
        format!("cache backend '{active_backend}' health check succeeded"),
    )
}

async fn check_remote_nodes_component<S: PrimaryRuntimeState>(state: &S) -> HealthComponentReport {
    match managed_follower_service::run_health_tests(state).await {
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
    use crate::cache::CacheBackend;
    use crate::config::CacheConfig;
    use crate::errors::{AsterError, Result};
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

        async fn health_check(&self) -> Result<()> {
            if self.healthy {
                Ok(())
            } else {
                Err(AsterError::internal_error("cache probe failed"))
            }
        }

        async fn get_bytes(&self, _key: &str) -> Option<Vec<u8>> {
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

    #[tokio::test]
    async fn cache_report_is_healthy_when_cache_is_disabled() {
        let config = CacheConfig {
            enabled: false,
            backend: "redis".to_string(),
            redis_url: "redis://example.com:6379/0".to_string(),
            default_ttl: 60,
        };
        let cache = FakeCache::new("noop");

        let report = check_cache_backend(&config, &cache).await;

        assert_eq!(report.name, "cache");
        assert_eq!(report.status, HealthStatus::Healthy);
        assert_eq!(report.message, "cache is disabled by configuration");
    }

    #[tokio::test]
    async fn cache_report_is_degraded_when_configured_backend_falls_back() {
        let config = CacheConfig {
            enabled: true,
            backend: "redis".to_string(),
            redis_url: "redis://example.com:6379/0".to_string(),
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
            enabled: true,
            backend: "redis".to_string(),
            redis_url: "redis://example.com:6379/0".to_string(),
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

//! 服务模块：`ops::health`。

use crate::errors::{AsterError, Result};
use crate::runtime::{FollowerRuntimeState, RemoteProtocolRuntimeState, SharedRuntimeState};
use crate::services::{remote::remote_node, task};
use aster_forge_runtime::{
    HealthCheckOptions, HealthCheckScope, HealthCheckScopes, HealthComponentReport, HealthStatus,
    RuntimeComponentBundle, RuntimeComponentBundleRegistration, RuntimeComponentKind,
    RuntimeComponentRegistry, SystemHealthReport,
};

const REMOTE_NODES_HEALTH_COMPONENT: &str = "remote_nodes";

impl From<SystemHealthReport> for task::RuntimeTaskRunOutcome {
    fn from(report: SystemHealthReport) -> Self {
        let has_issues = report.has_issues();
        let summary = if has_issues {
            report.issue_summary()
        } else {
            "system healthy".to_string()
        };
        let system_health = (&report).into();

        if has_issues {
            Self::failed_with_system_health(Some(summary), report.issue_details(), system_health)
        } else {
            Self::succeeded_with_system_health(Some(summary), system_health)
        }
    }
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

pub async fn run_primary_system_health_checks<S>(state: &S) -> SystemHealthReport
where
    S: RemoteProtocolRuntimeState + Clone + Send + Sync + 'static,
{
    tracing::debug!("running primary system health checks");
    let mut registry = RuntimeComponentRegistry::new();
    registry.register_bundle(primary_health_component(state.clone()));
    let report = registry.run_health(HealthCheckScope::Diagnostics).await;
    crate::metrics::record_health_report(HealthCheckScope::Diagnostics, &report);
    tracing::debug!(
        component_count = report.components.len(),
        unhealthy_count = report
            .components
            .iter()
            .filter(|component| matches!(component.status, HealthStatus::Unhealthy))
            .count(),
        degraded_count = report
            .components
            .iter()
            .filter(|component| matches!(component.status, HealthStatus::Degraded))
            .count(),
        "completed primary system health checks"
    );
    report
}

pub fn primary_health_component<S>(
    state: S,
) -> RuntimeComponentBundleRegistration<impl RuntimeComponentBundle + use<S>>
where
    S: RemoteProtocolRuntimeState + Clone + Send + Sync + 'static,
{
    let database = aster_forge_db::database_health_component(state.writer_db().clone());
    let cache = aster_forge_cache::cache_health_component(
        state.config().cache.clone(),
        state.cache().clone(),
    );

    aster_forge_runtime::runtime_component(move |registry: &mut RuntimeComponentRegistry| {
        registry.register_bundle(database).register_bundle(cache);
        registry.component_health_with_options(
            REMOTE_NODES_HEALTH_COMPONENT,
            RuntimeComponentKind::Product,
            REMOTE_NODES_HEALTH_COMPONENT,
            HealthCheckOptions::required(None).with_scopes(HealthCheckScopes::diagnostics()),
            move || {
                let state = state.clone();
                async move { check_remote_nodes_component(&state).await }
            },
        );
    })
}

async fn check_remote_nodes_component<S: RemoteProtocolRuntimeState>(
    state: &S,
) -> HealthComponentReport {
    match remote_node::run_health_tests(state).await {
        Ok(stats) => {
            let message = if stats.checked > 0 {
                format!(
                    "checked {} remote nodes: {} healthy, {} failed, {} skipped",
                    stats.checked, stats.healthy, stats.failed, stats.skipped
                )
            } else {
                format!(
                    "no eligible remote nodes checked, {} skipped",
                    stats.skipped
                )
            };
            let report = if stats.failed > 0 {
                HealthComponentReport::unhealthy(REMOTE_NODES_HEALTH_COMPONENT, message)
            } else {
                HealthComponentReport::healthy(REMOTE_NODES_HEALTH_COMPONENT, message)
            };

            report
                .with_detail("checked", stats.checked)
                .with_detail("healthy", stats.healthy)
                .with_detail("failed", stats.failed)
                .with_detail("skipped", stats.skipped)
        }
        Err(error) => HealthComponentReport::unhealthy(
            REMOTE_NODES_HEALTH_COMPONENT,
            format!("remote node health tests failed: {error}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        HealthComponentReport, HealthStatus, SystemHealthReport, primary_health_component,
    };
    use crate::services::task::{RuntimeTaskRunOutcome, types::RuntimeSystemHealthStatus};
    use aster_forge_runtime::{
        HealthCheckScope, HealthComponentDetailValue, RuntimeComponentBundle,
    };

    #[test]
    fn runtime_outcome_uses_compact_summary_when_system_is_healthy() {
        let report = SystemHealthReport::new(vec![
            HealthComponentReport {
                name: "database",
                status: HealthStatus::Healthy,
                message: "database ping succeeded".to_string(),
                duration: None,
                details: Vec::new(),
            },
            HealthComponentReport::healthy("cache", "cache probe succeeded")
                .with_detail("active_backend", "memory"),
        ]);

        let outcome: RuntimeTaskRunOutcome = report.into();

        match outcome {
            RuntimeTaskRunOutcome::Succeeded {
                summary,
                system_health,
            } => {
                assert_eq!(summary, Some("system healthy".to_string()));
                let system_health = system_health.expect("system health metadata should exist");
                assert_eq!(system_health.status, RuntimeSystemHealthStatus::Healthy);
                assert_eq!(system_health.components.len(), 2);
                assert_eq!(system_health.components[1].details.len(), 1);
            }
            other => panic!("expected succeeded system health outcome, got {other:?}"),
        }
    }

    #[test]
    fn runtime_outcome_reports_only_problem_components() {
        let report = SystemHealthReport::new(vec![
            HealthComponentReport::healthy("database", "database ping succeeded"),
            HealthComponentReport::degraded("cache", "fallback active")
                .with_detail("active_backend", "memory"),
        ]);

        let outcome: RuntimeTaskRunOutcome = report.into();

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
                assert_eq!(
                    system_health.components[1].details[0].value.as_text(),
                    Some("memory")
                );
            }
            other => panic!("expected failed system health outcome, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn primary_health_component_registers_complete_drive_diagnostics() {
        let state = crate::runtime::tasks::test_support::setup_primary_state().await;
        let mut registry = aster_forge_runtime::RuntimeComponentRegistry::new();
        primary_health_component(state.get_ref().clone()).register(&mut registry);

        let component_names = registry
            .descriptors()
            .iter()
            .map(|component| component.name)
            .collect::<Vec<_>>();
        assert_eq!(component_names, vec!["database", "cache", "remote_nodes"]);

        let report = registry.run_health(HealthCheckScope::Diagnostics).await;
        assert_eq!(report.status(), HealthStatus::Healthy);
        assert_eq!(report.components.len(), 3);
        let remote_nodes = report
            .components
            .iter()
            .find(|component| component.name == "remote_nodes")
            .expect("remote node health component should run");
        assert_eq!(
            remote_nodes.detail("checked"),
            Some(&HealthComponentDetailValue::Unsigned(0))
        );
        assert_eq!(
            remote_nodes.detail("skipped"),
            Some(&HealthComponentDetailValue::Unsigned(0))
        );
    }

    #[tokio::test]
    async fn primary_health_component_readiness_scope_only_runs_database() {
        let state = crate::runtime::tasks::test_support::setup_primary_state().await;
        let mut registry = aster_forge_runtime::RuntimeComponentRegistry::new();
        primary_health_component(state.get_ref().clone()).register(&mut registry);

        let report = registry.run_health(HealthCheckScope::Readiness).await;
        assert_eq!(report.status(), HealthStatus::Healthy);
        assert_eq!(
            report
                .components
                .iter()
                .map(|component| component.name)
                .collect::<Vec<_>>(),
            vec!["database"]
        );
    }
}

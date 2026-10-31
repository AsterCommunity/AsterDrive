//! AsterDrive runtime component assembly.
//!
//! This module turns prepared primary or follower state into the concrete
//! Forge runtime graph. Product entrypoints choose a node mode and prepare its
//! state; component ownership and shutdown dependencies stay centralized here.

use std::io;

use actix_web::web;

/// Assembles and runs the primary Forge runtime.
pub async fn run_primary(
    prepared: crate::runtime::startup::PreparedPrimaryRuntime,
    role: crate::runtime::role::ServeRole,
) -> io::Result<()> {
    let crate::runtime::startup::PreparedPrimaryRuntime {
        state,
        share_download_rollback_worker,
    } = prepared;
    let host = state.config.server.host.clone();
    let port = state.config.server.port;
    let workers = http_worker_count(state.config.server.workers, role.serves_api());
    tracing::info!(mode = "primary", role = role.as_str(), host = %host, port, workers, "starting HTTP service");

    let db_handles = state.db_handles.clone();
    let audit_state = state.clone();
    let consumers = if role.consumes_tasks() {
        crate::services::task::dispatch::consumer_statuses()
    } else {
        Vec::new()
    };
    let scheduler_status = aster_forge_tasks::TaskRuntimeStatus::default();
    let mut health = consumers.clone();
    if role.consumes_tasks() {
        health.push(scheduler_status.clone());
    }
    let state = web::Data::new(state);
    let telemetry_provider = state.metrics.forge_recorder().provider();
    let metrics =
        web::Data::<dyn aster_forge_metrics::MetricsRecorder>::from(state.metrics.forge_recorder());
    let initialized = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let initialization = initialized.clone();
    let maintenance_state = state.clone();
    let runtime = aster_forge_runtime::AsterRuntime::builder()
        .component(crate::runtime::components::primary_http_component(
            host,
            port,
            workers,
            state.clone(),
            metrics,
            super::role::RoleHealth {
                role,
                consumers: health,
                shutdown: Default::default(),
                initialized,
            },
        ))?
        .component(
            aster_forge_tasks::background_task_component_with_definitions_from_shutdown(
                crate::services::task::registered_system_runtime_tasks(),
                move |stop| {
                    crate::runtime::tasks::spawn_primary_background_tasks(
                        maintenance_state,
                        share_download_rollback_worker,
                        stop,
                    )
                },
            )
            .after_service()
            .with_definition_filter(|kind| {
                *kind == crate::services::task::SystemRuntimeTaskKind::TaskCleanup
            }),
        );
    let runtime = if role.consumes_tasks() {
        let consumer_state = state.clone();
        let scheduler_state = state.clone();
        runtime
            .component(
                aster_forge_tasks::background_task_component_with_definitions_from_shutdown(
                    crate::services::task::registered_system_runtime_tasks(),
                    move |stop| {
                        crate::runtime::tasks::spawn_task_consumers(consumer_state, consumers, stop)
                    },
                )
                .named("task_consumers")
                .with_definition_filter(|kind| {
                    *kind == crate::services::task::SystemRuntimeTaskKind::BackgroundTaskDispatch
                }),
            )
            .component(
                aster_forge_tasks::background_task_component_with_definitions_from_shutdown(
                    crate::services::task::registered_system_runtime_tasks(),
                    move |stop| {
                        crate::runtime::tasks::spawn_scheduler(
                            scheduler_state,
                            scheduler_status,
                            stop,
                        )
                    },
                )
                .named("task_scheduler")
                .with_definition_filter(|kind| {
                    !matches!(
                        kind,
                        crate::services::task::SystemRuntimeTaskKind::BackgroundTaskDispatch
                            | crate::services::task::SystemRuntimeTaskKind::TaskCleanup
                    )
                }),
            )
    } else {
        runtime
    };
    let dependencies: &'static [&'static str] = if role.consumes_tasks() {
        &[
            aster_forge_tasks::BACKGROUND_TASKS_COMPONENT,
            "task_consumers",
            "task_scheduler",
        ]
    } else {
        &[aster_forge_tasks::BACKGROUND_TASKS_COMPONENT]
    };
    let runtime = runtime
        .component(aster_forge_audit::audit_component_with_writer_after(
            audit_state.clone(),
            audit_state.audit_writer.clone(),
            std::time::Duration::from_secs(5),
            dependencies,
            |state| async move {
                crate::runtime::startup::record_server_start(&state).await;
                Ok(())
            },
            |state| async move {
                crate::runtime::shutdown::record_server_shutdown(&state).await;
                Ok(())
            },
        ))
        .component(aster_forge_db::database_component_after(
            db_handles,
            &[aster_forge_audit::AUDIT_MANAGER_COMPONENT],
        ))
        .component(aster_forge_runtime::runtime_component(
            move |registry: &mut aster_forge_runtime::RuntimeComponentRegistry| {
                let initialization = initialization.clone();
                registry.component_startup(
                    "process_role",
                    aster_forge_runtime::RuntimeComponentKind::Core,
                    "role_initialized",
                    aster_forge_runtime::StartupPhaseFailurePolicy::Required,
                    move || {
                        let initialization = initialization.clone();
                        async move {
                            initialization.store(true, std::sync::atomic::Ordering::Release);
                            Ok(())
                        }
                    },
                );
            },
        ));

    let result = runtime
        .run()
        .await
        .map_err(to_io_error)
        .and_then(std::convert::identity);
    let result = super::audit_recovery::finish_audit(
        result,
        &audit_state.audit_writer,
        std::path::Path::new("data/audit-recovery"),
    )
    .await;
    finish_metrics(result, telemetry_provider).await
}

/// Assembles and runs the follower Forge runtime.
pub async fn run_follower(
    prepared: crate::runtime::startup::PreparedFollowerRuntime,
) -> io::Result<()> {
    let state = prepared.state;
    let host = state.config.server.host.clone();
    let port = state.config.server.port;
    let workers = http_worker_count(state.config.server.workers, true);
    tracing::info!(mode = "follower", host = %host, port, workers, "starting HTTP service");

    let db_handles = state.db_handles.clone();
    let audit_state = state.clone();
    let state = web::Data::new(state);
    let telemetry_provider = state.metrics.forge_recorder().provider();
    let metrics =
        web::Data::<dyn aster_forge_metrics::MetricsRecorder>::from(state.metrics.forge_recorder());

    let runtime = aster_forge_runtime::AsterRuntime::builder()
        .component(crate::runtime::components::follower_http_component(
            host,
            port,
            workers,
            state.clone(),
            metrics,
        ))?
        .component(
            aster_forge_tasks::background_task_component_with_definitions_from_shutdown(
                crate::services::task::registered_system_runtime_tasks(),
                move |shutdown_token| {
                    crate::runtime::tasks::spawn_follower_background_tasks(state, shutdown_token)
                },
            ),
        )
        .component(aster_forge_audit::audit_component_with_writer_after(
            audit_state.clone(),
            audit_state.audit_writer.clone(),
            std::time::Duration::from_secs(5),
            &[aster_forge_tasks::BACKGROUND_TASKS_COMPONENT],
            |state| async move {
                crate::runtime::startup::record_server_start(&state).await;
                Ok(())
            },
            |state| async move {
                crate::runtime::shutdown::record_server_shutdown(&state).await;
                Ok(())
            },
        ))
        .component(aster_forge_db::database_component_after(
            db_handles,
            &[aster_forge_audit::AUDIT_MANAGER_COMPONENT],
        ));

    let result = runtime
        .run()
        .await
        .map_err(to_io_error)
        .and_then(std::convert::identity);
    let result = super::audit_recovery::finish_audit(
        result,
        &audit_state.audit_writer,
        std::path::Path::new("data/audit-recovery"),
    )
    .await;
    finish_metrics(result, telemetry_provider).await
}

async fn finish_metrics(
    result: io::Result<()>,
    provider: Option<aster_forge_metrics::MetricsProvider>,
) -> io::Result<()> {
    if let Some(provider) = provider {
        let shutdown = provider
            .shutdown(std::time::Duration::from_secs(10))
            .await
            .map_err(to_io_error);
        if let Err(error) = &shutdown {
            tracing::warn!(%error, "metrics shutdown failed");
        }
        if result.is_ok() {
            shutdown?;
        }
    }
    result
}

fn http_worker_count(configured_workers: usize, serves_api: bool) -> usize {
    if configured_workers == 0 && !serves_api {
        1
    } else if configured_workers == 0 {
        num_cpus::get()
    } else {
        configured_workers
    }
}

fn to_io_error(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::http_worker_count;

    #[tokio::test]
    async fn final_metrics_shutdown_preserves_runtime_error_and_closes_provider() {
        use super::finish_metrics;
        let provider = aster_forge_metrics::MetricsProvider::builder()
            .build()
            .unwrap();
        let error = finish_metrics(
            Err(std::io::Error::other("runtime failed")),
            Some(provider.clone()),
        )
        .await
        .unwrap_err();
        assert_eq!(error.to_string(), "runtime failed");
        assert_eq!(provider.shutdown_result(), Some(Ok(())));
        assert!(!provider.enabled());
        finish_metrics(Ok(()), Some(provider)).await.unwrap();
        finish_metrics(Ok(()), None).await.unwrap();
        assert_eq!(
            finish_metrics(Err(std::io::Error::other("original")), None)
                .await
                .unwrap_err()
                .to_string(),
            "original"
        );
    }

    #[test]
    fn worker_count_uses_cpu_count_when_configured_zero() {
        assert_eq!(http_worker_count(0, true), num_cpus::get());
    }

    #[test]
    fn worker_count_uses_explicit_value() {
        assert_eq!(http_worker_count(4, true), 4);
    }
    #[test]
    fn worker_operational_http_defaults_to_one_thread() {
        assert_eq!(http_worker_count(0, false), 1);
        assert_eq!(http_worker_count(3, false), 3);
    }
}

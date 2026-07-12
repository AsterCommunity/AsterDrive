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
) -> io::Result<()> {
    let crate::runtime::startup::PreparedPrimaryRuntime {
        state,
        share_download_rollback_worker,
    } = prepared;
    let host = state.config.server.host.clone();
    let port = state.config.server.port;
    let workers = worker_count(state.config.server.workers);
    tracing::info!(mode = "primary", host = %host, port, workers, "starting HTTP service");

    let db_handles = state.db_handles.clone();
    let audit_state = state.clone();
    let mail_state = state.clone();
    let state = web::Data::new(state);
    let metrics =
        web::Data::<dyn aster_forge_metrics::MetricsRecorder>::from(state.metrics.forge_recorder());

    let runtime = aster_forge_runtime::AsterRuntime::builder()
        .component(crate::runtime::components::primary_http_component(
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
                    crate::runtime::tasks::spawn_primary_background_tasks(
                        state,
                        share_download_rollback_worker,
                        shutdown_token,
                    )
                },
            ),
        )
        .component(aster_forge_mail::mail_outbox_component(
            crate::runtime::components::MailOutboxRuntimeResources::from_state(&mail_state),
            crate::runtime::components::drain_mail_outbox_on_shutdown,
        ))
        .component(aster_forge_audit::audit_component_infallible(
            audit_state,
            |state| async move { crate::runtime::startup::record_server_start(&state).await },
            |state| async move { crate::runtime::shutdown::record_server_shutdown(&state).await },
            |()| async { crate::services::ops::audit::shutdown_global_audit_log_manager().await },
        ))
        .component(aster_forge_db::database_component_after(
            db_handles,
            &[aster_forge_audit::AUDIT_MANAGER_COMPONENT],
        ));

    runtime.run().await.map_err(to_io_error)?
}

/// Assembles and runs the follower Forge runtime.
pub async fn run_follower(
    prepared: crate::runtime::startup::PreparedFollowerRuntime,
) -> io::Result<()> {
    let state = prepared.state;
    let host = state.config.server.host.clone();
    let port = state.config.server.port;
    let workers = worker_count(state.config.server.workers);
    tracing::info!(mode = "follower", host = %host, port, workers, "starting HTTP service");

    let db_handles = state.db_handles.clone();
    let audit_state = state.clone();
    let state = web::Data::new(state);
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
        .component(aster_forge_audit::audit_component_after_infallible(
            audit_state,
            &[aster_forge_tasks::BACKGROUND_TASKS_COMPONENT],
            |state| async move { crate::runtime::startup::record_server_start(&state).await },
            |state| async move { crate::runtime::shutdown::record_server_shutdown(&state).await },
            |()| async { crate::services::ops::audit::shutdown_global_audit_log_manager().await },
        ))
        .component(aster_forge_db::database_component_after(
            db_handles,
            &[aster_forge_audit::AUDIT_MANAGER_COMPONENT],
        ));

    runtime.run().await.map_err(to_io_error)?
}

fn worker_count(configured_workers: usize) -> usize {
    if configured_workers == 0 {
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
    use super::worker_count;

    #[test]
    fn worker_count_uses_cpu_count_when_configured_zero() {
        assert_eq!(worker_count(0), num_cpus::get());
    }

    #[test]
    fn worker_count_uses_explicit_value() {
        assert_eq!(worker_count(4), 4);
    }
}

//! Forge runtime component adapters for AsterDrive-owned resources.

use std::io;
use std::sync::Arc;

use actix_web::web;
use actix_web::{App, HttpServer};
use aster_forge_runtime::{
    RuntimeComponentKind, RuntimeServiceComponent, TryRuntimeComponentWithShutdown,
};
use tokio_util::sync::CancellationToken;

use super::{FollowerAppState, PrimaryAppState};
use aster_forge_mail::MailSender;

const HTTP_SHUTDOWN_TIMEOUT_SECS: u64 = 8;

type HttpServiceComponent = RuntimeServiceComponent<actix_web::dev::Server>;

#[derive(Clone)]
pub struct MailOutboxRuntimeResources {
    db: sea_orm::DatabaseConnection,
    runtime_config: Arc<crate::config::RuntimeConfig>,
    mail_sender: Arc<dyn MailSender>,
}

impl MailOutboxRuntimeResources {
    fn new(
        db: sea_orm::DatabaseConnection,
        runtime_config: Arc<crate::config::RuntimeConfig>,
        mail_sender: Arc<dyn MailSender>,
    ) -> Self {
        Self {
            db,
            runtime_config,
            mail_sender,
        }
    }

    pub fn from_state(state: &PrimaryAppState) -> Self {
        Self::new(
            state.writer_db().clone(),
            state.runtime_config.clone(),
            state.mail_sender.clone(),
        )
    }
}

pub fn primary_http_component(
    host: String,
    port: u16,
    workers: usize,
    state: web::Data<PrimaryAppState>,
    metrics: web::Data<dyn aster_forge_metrics::MetricsRecorder>,
    mut role_health: crate::runtime::role::RoleHealth,
) -> TryRuntimeComponentWithShutdown<
    HttpServiceComponent,
    impl FnOnce(CancellationToken) -> io::Result<HttpServiceComponent>,
    io::Error,
> {
    aster_forge_runtime::try_runtime_component_with_shutdown(move |shutdown_token| {
        let configure_db = state.writer_db().clone();
        let shutdown_data = web::Data::new(shutdown_token.clone());
        role_health.shutdown = shutdown_token.clone();
        let role = role_health.role;
        let role_health = web::Data::new(role_health);
        let app_state = state.clone();
        #[cfg(feature = "metrics")]
        let metrics_auth = web::Data::new(crate::api::middleware::metrics_auth::MetricsAuth::new(
            state.config.observability.prometheus.basic_auth.clone(),
        ));
        #[cfg(feature = "metrics")]
        let metrics_provider = state
            .metrics
            .forge_recorder()
            .provider()
            .map(web::Data::new);
        let server = HttpServer::new(move || {
            let db = configure_db.clone();
            let route_config = app_state.config.clone();
            let app = App::new();
            #[cfg(feature = "metrics")]
            let app = {
                let app = app.app_data(metrics_auth.clone());
                match metrics_provider.clone() {
                    Some(provider) => app.app_data(provider),
                    None => app,
                }
            };
            app.wrap(actix_web::middleware::Compress::default())
                .wrap(aster_forge_middleware::actix::metrics::MetricsMiddleware)
                .wrap(aster_forge_middleware::actix::request_id::RequestIdMiddleware)
                .wrap(crate::api::middleware::cors::runtime_cors())
                .wrap(aster_forge_middleware::actix::security_headers::default_headers())
                .app_data(actix_web::web::PayloadConfig::new(
                    crate::api::extractors::DEFAULT_PAYLOAD_LIMIT,
                ))
                .app_data(crate::api::extractors::json_config(
                    crate::api::extractors::DEFAULT_JSON_LIMIT,
                ))
                .app_data(app_state.clone())
                .app_data(metrics.clone())
                .app_data(shutdown_data.clone())
                .app_data(role_health.clone())
                .configure(move |cfg| {
                    if role.serves_api() {
                        crate::api::configure_primary(cfg, &db, &route_config.observability);
                    } else {
                        cfg.service(crate::api::routes::health::primary_routes(
                            &route_config.observability,
                        ));
                    }
                })
        })
        .keep_alive(std::time::Duration::from_secs(30))
        .client_request_timeout(std::time::Duration::from_millis(5000))
        .client_disconnect_timeout(std::time::Duration::from_millis(1000))
        .shutdown_timeout(HTTP_SHUTDOWN_TIMEOUT_SECS)
        .disable_signals()
        .bind((host.as_str(), port))?
        .workers(workers)
        .run();
        Ok(http_service_component(
            server,
            shutdown_token,
            state.runtime_config.refresh_coordinator().clone(),
        ))
    })
}

pub fn follower_http_component(
    host: String,
    port: u16,
    workers: usize,
    state: web::Data<FollowerAppState>,
    metrics: web::Data<dyn aster_forge_metrics::MetricsRecorder>,
) -> TryRuntimeComponentWithShutdown<
    HttpServiceComponent,
    impl FnOnce(CancellationToken) -> io::Result<HttpServiceComponent>,
    io::Error,
> {
    aster_forge_runtime::try_runtime_component_with_shutdown(move |shutdown_token| {
        let shutdown_data = web::Data::new(shutdown_token.clone());
        let app_state = state.clone();
        #[cfg(feature = "metrics")]
        let metrics_auth = web::Data::new(crate::api::middleware::metrics_auth::MetricsAuth::new(
            state.config.observability.prometheus.basic_auth.clone(),
        ));
        #[cfg(feature = "metrics")]
        let metrics_provider = state
            .metrics
            .forge_recorder()
            .provider()
            .map(web::Data::new);
        let server = HttpServer::new(move || {
            let route_config = app_state.config.clone();
            let app = App::new();
            #[cfg(feature = "metrics")]
            let app = {
                let app = app.app_data(metrics_auth.clone());
                match metrics_provider.clone() {
                    Some(provider) => app.app_data(provider),
                    None => app,
                }
            };
            app.wrap(actix_web::middleware::Compress::default())
                .wrap(aster_forge_middleware::actix::metrics::MetricsMiddleware)
                .wrap(aster_forge_middleware::actix::request_id::RequestIdMiddleware)
                .wrap(aster_forge_middleware::actix::security_headers::default_headers())
                .app_data(actix_web::web::PayloadConfig::new(
                    crate::api::extractors::DEFAULT_PAYLOAD_LIMIT,
                ))
                .app_data(crate::api::extractors::json_config(
                    crate::api::extractors::DEFAULT_JSON_LIMIT,
                ))
                .app_data(app_state.clone())
                .app_data(metrics.clone())
                .app_data(shutdown_data.clone())
                .configure(move |cfg| {
                    crate::api::configure_follower(cfg, &route_config.observability)
                })
        })
        .keep_alive(std::time::Duration::from_secs(30))
        .client_request_timeout(std::time::Duration::from_millis(5000))
        .client_disconnect_timeout(std::time::Duration::from_millis(1000))
        .shutdown_timeout(HTTP_SHUTDOWN_TIMEOUT_SECS)
        .disable_signals()
        .bind((host.as_str(), port))?
        .workers(workers)
        .run();
        Ok(http_service_component(
            server,
            shutdown_token,
            state.runtime_config.refresh_coordinator().clone(),
        ))
    })
}

fn http_service_component(
    server: actix_web::dev::Server,
    shutdown_token: CancellationToken,
    refresh_coordinator: aster_forge_auth::RefreshCoordinator,
) -> HttpServiceComponent {
    let handle = server.handle();
    RuntimeServiceComponent::new(
        "http",
        RuntimeComponentKind::Core,
        server,
        shutdown_token,
        move || async move {
            // Wake queued refreshes before draining HTTP; started writer transactions may finish.
            refresh_coordinator.close();
            handle.stop(true).await;
        },
    )
}

pub async fn drain_mail_outbox_on_shutdown(
    resources: MailOutboxRuntimeResources,
) -> Result<(), String> {
    crate::services::mail::outbox::drain_with(
        &resources.db,
        &resources.runtime_config,
        &resources.mail_sender,
    )
    .await
    .map(|_| ())
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use aster_drive_migration::Migrator;
    use aster_forge_runtime::RuntimeComponentBundle;

    use super::{MailOutboxRuntimeResources, drain_mail_outbox_on_shutdown};
    use crate::runtime::FollowerAppState;

    fn register_background_tasks(registry: &mut aster_forge_runtime::RuntimeComponentRegistry) {
        registry
            .component(aster_forge_tasks::BACKGROUND_TASKS_COMPONENT)
            .kind(aster_forge_runtime::RuntimeComponentKind::Product);
    }

    async fn follower_state() -> FollowerAppState {
        let db = crate::db::connect_with_metrics(
            &crate::config::DatabaseConfig {
                url: "sqlite::memory:".into(),
                pool_size: 1,
                retry_count: 0,
            },
            aster_drive_metrics::NoopMetrics::arc(),
        )
        .await
        .expect("runtime graph database should connect");
        Migrator::up(&db, None)
            .await
            .expect("runtime graph migrations should apply");
        let runtime_config = Arc::new(crate::config::RuntimeConfig::new());
        runtime_config
            .reload(&db)
            .await
            .expect("runtime config should load");
        let cache =
            aster_forge_cache::create_cache(&aster_forge_cache::CacheConfig::default()).await;

        FollowerAppState {
            audit_writer: std::sync::Arc::new(aster_forge_audit::AuditWriter::new(db.clone())),
            db_handles: aster_forge_db::DbHandles::single(db),
            driver_registry: Arc::new(
                crate::storage::DriverRegistry::noop()
                    .expect("built-in storage connector registry"),
            ),
            runtime_config,
            policy_snapshot: Arc::new(crate::storage::PolicySnapshot::new()),
            config: Arc::new(crate::config::Config::default()),
            cache,
            config_sync: aster_forge_config::ConfigSyncRuntime::disabled_for_test("aster_drive"),
            metrics: aster_drive_metrics::NoopMetrics::arc(),
        }
    }

    #[tokio::test]
    async fn audit_and_database_components_have_explicit_shutdown_order() {
        let state = follower_state().await;
        let mut registry = aster_forge_runtime::RuntimeComponentRegistry::configured(|registry| {
            aster_forge_runtime::runtime_component(register_background_tasks).register(registry);
            aster_forge_audit::audit_component_with_writer_after(
                state.clone(),
                state.audit_writer.clone(),
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
            )
            .register(registry);
            aster_forge_db::database_component_after(
                state.db_handles.clone(),
                &[aster_forge_audit::AUDIT_MANAGER_COMPONENT],
            )
            .register(registry);
        });

        registry
            .validate()
            .expect("Drive runtime component graph should validate");
        assert_eq!(
            registry
                .descriptor(aster_forge_audit::AUDIT_LOGS_COMPONENT)
                .expect("audit logs component should exist")
                .dependencies,
            vec![aster_forge_tasks::BACKGROUND_TASKS_COMPONENT]
        );
        assert_eq!(
            registry
                .descriptor(aster_forge_db::DATABASE_COMPONENT)
                .expect("database component should exist")
                .dependencies,
            vec![aster_forge_audit::AUDIT_MANAGER_COMPONENT]
        );
        assert!(!registry.startup().await.aborted());
        assert!(!registry.shutdown().await.has_failures());
        assert_eq!(state.audit_writer.snapshot().persisted, 2);
        assert_eq!(
            state.audit_writer.snapshot().state,
            aster_forge_runtime::BufferedWriterState::Stopped
        );
    }

    #[tokio::test]
    async fn primary_mail_audit_and_database_components_have_explicit_shutdown_order() {
        let state = follower_state().await;
        let resources = MailOutboxRuntimeResources::new(
            state.writer_db().clone(),
            state.runtime_config.clone(),
            aster_forge_mail::memory_sender(),
        );
        let mut registry = aster_forge_runtime::RuntimeComponentRegistry::configured(|registry| {
            aster_forge_runtime::runtime_component(register_background_tasks).register(registry);
            aster_forge_mail::mail_outbox_component(resources, drain_mail_outbox_on_shutdown)
                .register(registry);
            aster_forge_audit::audit_component_with_writer(
                state.clone(),
                state.audit_writer.clone(),
                std::time::Duration::from_secs(5),
                |state| async move {
                    crate::runtime::startup::record_server_start(&state).await;
                    Ok(())
                },
                |state| async move {
                    crate::runtime::shutdown::record_server_shutdown(&state).await;
                    Ok(())
                },
            )
            .register(registry);
            aster_forge_db::database_component_after(
                state.db_handles.clone(),
                &[aster_forge_audit::AUDIT_MANAGER_COMPONENT],
            )
            .register(registry);
        });

        registry
            .validate()
            .expect("Drive primary runtime component graph should validate");
        assert_eq!(
            registry
                .descriptor(aster_forge_mail::MAIL_OUTBOX_COMPONENT)
                .expect("mail outbox component should exist")
                .dependencies,
            vec![aster_forge_tasks::BACKGROUND_TASKS_COMPONENT]
        );
        assert_eq!(
            registry
                .descriptor(aster_forge_audit::AUDIT_LOGS_COMPONENT)
                .expect("audit logs component should exist")
                .dependencies,
            vec![aster_forge_mail::MAIL_OUTBOX_COMPONENT]
        );
        assert_eq!(
            registry
                .descriptor(aster_forge_db::DATABASE_COMPONENT)
                .expect("database component should exist")
                .dependencies,
            vec![aster_forge_audit::AUDIT_MANAGER_COMPONENT]
        );
        assert!(!registry.startup().await.aborted());
        assert!(!registry.shutdown().await.has_failures());
        assert_eq!(state.audit_writer.snapshot().persisted, 2);
        assert_eq!(
            state.audit_writer.snapshot().state,
            aster_forge_runtime::BufferedWriterState::Stopped
        );
    }

    #[tokio::test]
    async fn product_lifecycle_preserves_audit_and_database_cleanup_on_success_error_and_panic() {
        use aster_forge_runtime::*;
        use sea_orm::{EntityTrait, PaginatorTrait};
        for scenario in [
            "success",
            "product_error",
            "startup",
            "before_shutdown",
            "shutdown",
            "service",
        ] {
            let state = follower_state().await;
            let closed = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let close_flag = closed.clone();
            let close_state = state.clone();
            let result = AsterRuntime::builder()
                .component(runtime_component_with_shutdown(move |shutdown| {
                    RuntimeServiceComponent::new(
                        "http",
                        RuntimeComponentKind::Core,
                        async move {
                            assert_ne!(scenario, "service", "injected service panic");
                            if scenario == "product_error" {
                                Err("product error")
                            } else {
                                Ok(())
                            }
                        },
                        shutdown,
                        || async {},
                    )
                }))
                .component(runtime_component(register_background_tasks))
                .component(aster_forge_audit::audit_component_with_writer_after(
                    state.clone(),
                    state.audit_writer.clone(),
                    std::time::Duration::from_secs(5),
                    &[aster_forge_tasks::BACKGROUND_TASKS_COMPONENT],
                    move |state| async move {
                        crate::runtime::startup::record_server_start(&state).await;
                        assert_ne!(scenario, "startup", "injected startup panic");
                        Ok(())
                    },
                    move |state| async move {
                        crate::runtime::shutdown::record_server_shutdown(&state).await;
                        assert_ne!(scenario, "shutdown", "injected shutdown panic");
                        Ok(())
                    },
                ))
                .before_shutdown(move || async move {
                    assert_ne!(
                        scenario, "before_shutdown",
                        "injected before-shutdown panic"
                    );
                })
                .component(runtime_component(
                    move |registry: &mut RuntimeComponentRegistry| {
                        registry
                            .component("database")
                            .depends_on(aster_forge_audit::AUDIT_MANAGER_COMPONENT)
                            .shutdown("database_close", None, move || {
                                let close_state = close_state.clone();
                                let close_flag = close_flag.clone();
                                async move {
                                    assert_eq!(
                                        close_state.audit_writer.snapshot().state,
                                        BufferedWriterState::Stopped
                                    );
                                    assert_eq!(
                                        aster_drive_model::entities::audit_log::Entity::find()
                                            .count(close_state.writer_db())
                                            .await
                                            .unwrap(),
                                        2
                                    );
                                    close_state
                                        .writer_db()
                                        .clone()
                                        .close()
                                        .await
                                        .map_err(|error| error.to_string())?;
                                    close_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                                    Ok(())
                                }
                            });
                    },
                ))
                .run()
                .await;
            assert!(
                closed.load(std::sync::atomic::Ordering::SeqCst),
                "{scenario}"
            );
            assert!(state.audit_writer.unconfirmed().is_empty());
            match (scenario, result) {
                ("success", Ok(Ok(()))) | ("product_error", Ok(Err("product error"))) => {}
                (
                    "startup",
                    Err(AsterRuntimeError::Startup {
                        shutdown_report, ..
                    }),
                )
                | ("before_shutdown", Err(AsterRuntimeError::BeforeShutdown { shutdown_report }))
                | (
                    "service",
                    Err(AsterRuntimeError::Service {
                        shutdown_report, ..
                    }),
                ) => assert!(!shutdown_report.has_failures()),
                ("shutdown", Err(AsterRuntimeError::Shutdown { report })) => {
                    assert!(report.has_failures())
                }
                other => panic!("unexpected lifecycle outcome: {other:?}"),
            }
        }
    }

    #[test]
    fn forge_background_task_component_registers_complete_drive_catalog() {
        let registry = aster_forge_runtime::RuntimeComponentRegistry::configured(|registry| {
            aster_forge_tasks::background_task_component_with_definitions(
                aster_forge_tasks::BackgroundTasks::new(),
                crate::services::task::registered_system_runtime_tasks(),
            )
            .register(registry);
        });

        registry
            .validate()
            .expect("Forge background task component should validate");
        let descriptor = registry
            .descriptor(aster_forge_tasks::BACKGROUND_TASKS_COMPONENT)
            .expect("background task component should exist");
        assert_eq!(
            descriptor.kind,
            aster_forge_runtime::RuntimeComponentKind::Tasks
        );
        assert_eq!(descriptor.tasks.len(), 16);
        assert_eq!(
            descriptor
                .tasks
                .iter()
                .map(|task| task.task_name)
                .collect::<Vec<_>>(),
            crate::services::task::registered_system_runtime_tasks()
                .iter()
                .map(|task| task.wire_value)
                .collect::<Vec<_>>()
        );
    }
}

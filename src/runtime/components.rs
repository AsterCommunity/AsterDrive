//! Forge runtime component adapters for AsterDrive-owned resources.

use std::io;
use std::sync::Arc;

use actix_web::web;
use actix_web::{App, HttpServer};
use aster_forge_runtime::{
    RuntimeComponentBundle, RuntimeComponentBundleRegistration, RuntimeComponentKind,
    RuntimeComponentWithShutdown, RuntimeServiceComponent, TryRuntimeComponentWithShutdown,
};
use tokio_util::sync::CancellationToken;

use super::{FollowerAppState, PrimaryAppState, SharedRuntimeState};
use crate::runtime::tasks::BackgroundTasks;
use crate::services::mail::sender::MailSender;
use crate::services::share::ShareDownloadRollbackWorker;

pub const BACKGROUND_TASKS_COMPONENT: &str = "background_tasks";
const BACKGROUND_TASKS_SHUTDOWN_PHASE: &str = "background_tasks";
const DATABASE_SHUTDOWN_DEPENDENCIES: &[&str] = &[aster_forge_audit::AUDIT_MANAGER_COMPONENT];
const HTTP_SHUTDOWN_TIMEOUT_SECS: u64 = 8;

type BackgroundTasksRegistration = RuntimeComponentBundleRegistration<
    aster_forge_runtime::ShutdownResourceComponent<BackgroundTasks>,
>;

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
) -> TryRuntimeComponentWithShutdown<
    HttpServiceComponent,
    impl FnOnce(CancellationToken) -> io::Result<HttpServiceComponent>,
    io::Error,
> {
    aster_forge_runtime::try_runtime_component_with_shutdown(move |shutdown_token| {
        let configure_db = state.writer_db().clone();
        let shutdown_data = web::Data::new(shutdown_token.clone());
        let app_state = state.clone();
        let server = HttpServer::new(move || {
            let db = configure_db.clone();
            App::new()
                .wrap(actix_web::middleware::Compress::default())
                .wrap(aster_forge_actix_middleware::metrics::MetricsMiddleware)
                .wrap(crate::api::middleware::request_id::RequestIdMiddleware)
                .wrap(crate::api::middleware::cors::RuntimeCors)
                .wrap(crate::api::middleware::security_headers::default_headers())
                .app_data(actix_web::web::PayloadConfig::new(
                    crate::api::extractors::DEFAULT_PAYLOAD_LIMIT,
                ))
                .app_data(crate::api::extractors::json_config(
                    crate::api::extractors::DEFAULT_JSON_LIMIT,
                ))
                .app_data(app_state.clone())
                .app_data(metrics.clone())
                .app_data(shutdown_data.clone())
                .configure(move |cfg| crate::api::configure_primary(cfg, &db))
        })
        .keep_alive(std::time::Duration::from_secs(30))
        .client_request_timeout(std::time::Duration::from_millis(5000))
        .client_disconnect_timeout(std::time::Duration::from_millis(1000))
        .shutdown_timeout(HTTP_SHUTDOWN_TIMEOUT_SECS)
        .disable_signals()
        .bind((host.as_str(), port))?
        .workers(workers)
        .run();
        Ok(http_service_component(server, shutdown_token))
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
        let server = HttpServer::new(move || {
            App::new()
                .wrap(actix_web::middleware::Compress::default())
                .wrap(aster_forge_actix_middleware::metrics::MetricsMiddleware)
                .wrap(crate::api::middleware::request_id::RequestIdMiddleware)
                .wrap(crate::api::middleware::security_headers::default_headers())
                .app_data(actix_web::web::PayloadConfig::new(
                    crate::api::extractors::DEFAULT_PAYLOAD_LIMIT,
                ))
                .app_data(crate::api::extractors::json_config(
                    crate::api::extractors::DEFAULT_JSON_LIMIT,
                ))
                .app_data(app_state.clone())
                .app_data(metrics.clone())
                .app_data(shutdown_data.clone())
                .configure(crate::api::configure_follower)
        })
        .keep_alive(std::time::Duration::from_secs(30))
        .client_request_timeout(std::time::Duration::from_millis(5000))
        .client_disconnect_timeout(std::time::Duration::from_millis(1000))
        .shutdown_timeout(HTTP_SHUTDOWN_TIMEOUT_SECS)
        .disable_signals()
        .bind((host.as_str(), port))?
        .workers(workers)
        .run();
        Ok(http_service_component(server, shutdown_token))
    })
}

fn http_service_component(
    server: actix_web::dev::Server,
    shutdown_token: CancellationToken,
) -> HttpServiceComponent {
    let handle = server.handle();
    RuntimeServiceComponent::new(
        "http",
        RuntimeComponentKind::Core,
        server,
        shutdown_token,
        move || async move {
            handle.stop(true).await;
        },
    )
}

pub fn primary_background_tasks_component(
    state: web::Data<PrimaryAppState>,
    share_download_rollback_worker: ShareDownloadRollbackWorker,
) -> RuntimeComponentWithShutdown<
    BackgroundTasksRegistration,
    impl FnOnce(CancellationToken) -> BackgroundTasksRegistration,
> {
    aster_forge_runtime::runtime_component_with_shutdown(move |shutdown_token| {
        background_tasks_component(crate::runtime::tasks::spawn_primary_background_tasks(
            state,
            share_download_rollback_worker,
            shutdown_token,
        ))
    })
}

pub fn follower_background_tasks_component(
    state: web::Data<FollowerAppState>,
) -> RuntimeComponentWithShutdown<
    BackgroundTasksRegistration,
    impl FnOnce(CancellationToken) -> BackgroundTasksRegistration,
> {
    aster_forge_runtime::runtime_component_with_shutdown(move |shutdown_token| {
        background_tasks_component(crate::runtime::tasks::spawn_follower_background_tasks(
            state,
            shutdown_token,
        ))
    })
}

fn background_tasks_component(background_tasks: BackgroundTasks) -> BackgroundTasksRegistration {
    aster_forge_runtime::shutdown_resource_component(
        BACKGROUND_TASKS_COMPONENT,
        RuntimeComponentKind::Product,
        BACKGROUND_TASKS_SHUTDOWN_PHASE,
        background_tasks,
        |background_tasks| async move {
            background_tasks.shutdown().await;
            Ok(())
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

pub fn primary_audit_component<S>(
    state: S,
) -> RuntimeComponentBundleRegistration<impl RuntimeComponentBundle + use<S>>
where
    S: SharedRuntimeState + Clone + Send + Sync + 'static,
{
    audit_component_after(state, &[aster_forge_mail::MAIL_OUTBOX_COMPONENT])
}

pub fn follower_audit_component<S>(
    state: S,
) -> RuntimeComponentBundleRegistration<impl RuntimeComponentBundle + use<S>>
where
    S: SharedRuntimeState + Clone + Send + Sync + 'static,
{
    audit_component_after(state, &[BACKGROUND_TASKS_COMPONENT])
}

fn audit_component_after<S>(
    state: S,
    dependencies: &'static [&'static str],
) -> RuntimeComponentBundleRegistration<impl RuntimeComponentBundle + use<S>>
where
    S: SharedRuntimeState + Clone + Send + Sync + 'static,
{
    aster_forge_audit::audit_component_after(
        state,
        dependencies,
        |state| async move {
            crate::runtime::startup::record_server_start(&state).await;
            Ok(())
        },
        |state| async move {
            crate::runtime::shutdown::record_server_shutdown(&state).await;
            Ok(())
        },
        |()| async {
            crate::services::ops::audit::shutdown_global_audit_log_manager().await;
            Ok(())
        },
    )
}

pub fn database_component(
    db_handles: aster_forge_db::DbHandles,
) -> RuntimeComponentBundleRegistration<aster_forge_db::DatabaseRuntimeComponent> {
    aster_forge_db::database_component_after(db_handles, DATABASE_SHUTDOWN_DEPENDENCIES)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use aster_forge_runtime::RuntimeComponentBundle;
    use migration::Migrator;

    use super::{
        BACKGROUND_TASKS_COMPONENT, MailOutboxRuntimeResources, database_component,
        drain_mail_outbox_on_shutdown, follower_audit_component, primary_audit_component,
    };
    use crate::runtime::{FollowerAppState, SharedRuntimeState};

    fn register_background_tasks(registry: &mut aster_forge_runtime::RuntimeComponentRegistry) {
        registry
            .component(BACKGROUND_TASKS_COMPONENT)
            .kind(aster_forge_runtime::RuntimeComponentKind::Product);
    }

    async fn follower_state() -> FollowerAppState {
        let db = crate::db::connect_with_metrics(
            &crate::config::DatabaseConfig {
                url: "sqlite::memory:".to_string(),
                pool_size: 1,
                retry_count: 0,
            },
            crate::metrics::NoopMetrics::arc(),
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
        let cache = aster_forge_cache::create_cache(&crate::config::CacheConfig::default()).await;

        FollowerAppState {
            db_handles: aster_forge_db::DbHandles::single(db),
            driver_registry: Arc::new(crate::storage::DriverRegistry::noop()),
            runtime_config,
            policy_snapshot: Arc::new(crate::storage::PolicySnapshot::new()),
            config: Arc::new(crate::config::Config::default()),
            cache,
            config_sync: aster_forge_config::ConfigSyncRuntime::disabled_for_test("aster_drive"),
            metrics: crate::metrics::NoopMetrics::arc(),
        }
    }

    #[tokio::test]
    async fn audit_and_database_components_have_explicit_shutdown_order() {
        let state = follower_state().await;
        let registry = aster_forge_runtime::RuntimeComponentRegistry::configured(|registry| {
            aster_forge_runtime::runtime_component(register_background_tasks).register(registry);
            follower_audit_component(state.clone()).register(registry);
            database_component(state.db_handles.clone()).register(registry);
        });

        registry
            .validate()
            .expect("Drive runtime component graph should validate");
        assert_eq!(
            registry
                .descriptor(aster_forge_audit::AUDIT_LOGS_COMPONENT)
                .expect("audit logs component should exist")
                .dependencies,
            vec![BACKGROUND_TASKS_COMPONENT]
        );
        assert_eq!(
            registry
                .descriptor(aster_forge_db::DATABASE_COMPONENT)
                .expect("database component should exist")
                .dependencies,
            vec![aster_forge_audit::AUDIT_MANAGER_COMPONENT]
        );
    }

    #[tokio::test]
    async fn primary_mail_audit_and_database_components_have_explicit_shutdown_order() {
        let state = follower_state().await;
        let resources = MailOutboxRuntimeResources::new(
            state.writer_db().clone(),
            state.runtime_config.clone(),
            crate::services::mail::sender::memory_sender(),
        );
        let registry = aster_forge_runtime::RuntimeComponentRegistry::configured(|registry| {
            aster_forge_runtime::runtime_component(register_background_tasks).register(registry);
            aster_forge_mail::mail_outbox_component(resources, drain_mail_outbox_on_shutdown)
                .register(registry);
            primary_audit_component(state.clone()).register(registry);
            database_component(state.db_handles.clone()).register(registry);
        });

        registry
            .validate()
            .expect("Drive primary runtime component graph should validate");
        assert_eq!(
            registry
                .descriptor(aster_forge_mail::MAIL_OUTBOX_COMPONENT)
                .expect("mail outbox component should exist")
                .dependencies,
            vec![BACKGROUND_TASKS_COMPONENT]
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
    }
}

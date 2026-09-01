//! API 路由：`health`。

use crate::api::api_error_code::ApiErrorCode;
use crate::api::response::{ApiResponse, HealthResponse, MemoryStatsResponse, SystemInfoResponse};
use crate::runtime::{FollowerAppState, PrimaryAppState, SharedRuntimeState};
use crate::services::ops::health;
use actix_web::{HttpResponse, web};
use aster_forge_runtime::HealthStatus;

const READY_DB_UNAVAILABLE_MESSAGE: &str = "Database unavailable";
const READY_CACHE_UNAVAILABLE_MESSAGE: &str = "Cache unavailable";
const READY_STORAGE_UNAVAILABLE_MESSAGE: &str = "Storage unavailable";

pub fn primary_routes() -> actix_web::Scope {
    let scope = web::scope("/health")
        .route("", web::get().to(health))
        .route("", web::head().to(health))
        .route("/ready", web::get().to(primary_ready))
        .route("/ready", web::head().to(primary_ready));

    attach_optional_routes(scope)
}

pub fn follower_routes() -> actix_web::Scope {
    let scope = web::scope("/health")
        .route("", web::get().to(health))
        .route("", web::head().to(health))
        .route("/ready", web::get().to(follower_ready))
        .route("/ready", web::head().to(follower_ready));

    attach_optional_routes(scope)
}

fn attach_optional_routes(scope: actix_web::Scope) -> actix_web::Scope {
    #[cfg(all(debug_assertions, feature = "openapi"))]
    let scope = scope.route("/memory", web::get().to(memory));

    aster_forge_actix_observability::configure_prometheus_route(scope)
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/health",
    tag = "health",
    operation_id = "health",
    responses(
        (status = 200, description = "Service is healthy", body = inline(crate::api::response::HealthResponse)),
    ),
)]
pub async fn health() -> HttpResponse {
    HttpResponse::Ok().json(status_response("ok"))
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/health/ready",
    tag = "health",
    operation_id = "ready",
    responses(
        (status = 200, description = "Service is ready", body = inline(ApiResponse<crate::api::response::HealthResponse>)),
        (status = 503, description = "Service unavailable"),
    ),
)]
pub async fn primary_ready(state: web::Data<PrimaryAppState>) -> HttpResponse {
    if let Err(error) = aster_forge_db::ping_database(state.get_ref().writer_db()).await {
        return ready_database_error(error);
    }
    if let Err(error) = check_cache_ready(state.get_ref()).await {
        return ready_cache_error(error);
    }

    match health::check_primary_ready(state.get_ref()).await {
        Ok(setup_state) => {
            HttpResponse::Ok().json(ApiResponse::ok(status_response(setup_state.as_str())))
        }
        Err(error) => ready_storage_error(error),
    }
}

pub async fn follower_ready(state: web::Data<FollowerAppState>) -> HttpResponse {
    if let Err(error) = aster_forge_db::ping_database(state.get_ref().writer_db()).await {
        return ready_database_error(error);
    }
    if let Err(error) = check_cache_ready(state.get_ref()).await {
        return ready_cache_error(error);
    }

    match health::check_follower_ready(state.get_ref()).await {
        Ok(_) => HttpResponse::Ok().json(ApiResponse::ok(status_response("ready"))),
        Err(error) => ready_storage_error(error),
    }
}

async fn check_cache_ready<S: SharedRuntimeState>(state: &S) -> Result<(), String> {
    if state.config().deployment.requires_shared_runtime()
        && state.config().cache.normalized_backend() != "redis"
    {
        return Err(format!(
            "cluster profile requires redis cache, configured backend is {}",
            state.config().cache.normalized_backend()
        ));
    }

    let report =
        aster_forge_cache::check_cache_component(&state.config().cache, state.cache().as_ref())
            .await;
    match report.status {
        HealthStatus::Healthy => Ok(()),
        HealthStatus::Degraded if !state.config().deployment.requires_shared_runtime() => {
            tracing::warn!(
                message = %report.message,
                "single-profile readiness accepted degraded cache backend"
            );
            Ok(())
        }
        HealthStatus::Degraded | HealthStatus::Unhealthy => Err(report.message),
    }
}

fn ready_database_error(error: impl std::fmt::Display) -> HttpResponse {
    tracing::error!(error = %error, "health readiness database ping failed");
    HttpResponse::ServiceUnavailable().json(ApiResponse::<()>::error(
        ApiErrorCode::DatabaseError,
        READY_DB_UNAVAILABLE_MESSAGE,
    ))
}

fn ready_cache_error(error: impl std::fmt::Display) -> HttpResponse {
    tracing::error!(error = %error, "health readiness cache check failed");
    HttpResponse::ServiceUnavailable().json(ApiResponse::<()>::error(
        ApiErrorCode::ConfigError,
        READY_CACHE_UNAVAILABLE_MESSAGE,
    ))
}

fn ready_storage_error(error: crate::errors::AsterError) -> HttpResponse {
    tracing::error!(error = %error, "health readiness storage probe failed");
    HttpResponse::ServiceUnavailable().json(ApiResponse::<()>::error_with_details(
        error.api_error_code(),
        READY_STORAGE_UNAVAILABLE_MESSAGE,
        None,
    ))
}

pub async fn ready(state: web::Data<PrimaryAppState>) -> HttpResponse {
    primary_ready(state).await
}

pub async fn memory() -> HttpResponse {
    let (allocated, peak) = aster_forge_alloc::stats();
    HttpResponse::Ok().json(ApiResponse::ok(MemoryStatsResponse {
        heap_allocated_mb: format!("{allocated:.2}"),
        heap_peak_mb: format!("{peak:.2}"),
    }))
}

pub fn system_info_response() -> SystemInfoResponse {
    SystemInfoResponse {
        version: crate::build_info::VERSION.to_string(),
        build_time: crate::build_info::BUILD_TIME.to_string(),
    }
}

fn status_response(status: &str) -> HealthResponse {
    HealthResponse {
        status: status.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{READY_STORAGE_UNAVAILABLE_MESSAGE, follower_ready, ready};
    use crate::config::{Config, DatabaseConfig, RuntimeConfig};
    use crate::runtime::PrimaryAppState;
    use crate::services::mail::sender;
    use crate::storage::{DriverRegistry, PolicySnapshot};
    use actix_web::{body, http::StatusCode, web};
    use aster_drive_model::entities::{
        storage_policy_group, storage_policy_group_rule, storage_policy_group_rule_target, user,
    };
    use aster_drive_model::types::{UserRole, UserStatus};
    use aster_drive_storage::{BlobMetadata, StorageDriver};
    use aster_forge_cache as cache;
    use aster_forge_cache::{CacheBackend, CacheConfig, CacheError};
    use async_trait::async_trait;
    use chrono::Utc;
    use sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, Set};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tokio::io::AsyncRead;

    #[derive(Clone, Default)]
    struct ProbeDriver {
        fail_ready: bool,
        ready_calls: Arc<AtomicUsize>,
        put_calls: Arc<AtomicUsize>,
        delete_calls: Arc<AtomicUsize>,
    }

    struct FakeCache {
        backend_name: &'static str,
        healthy: bool,
    }

    #[async_trait]
    impl CacheBackend for FakeCache {
        fn backend_name(&self) -> &'static str {
            self.backend_name
        }

        async fn health_check(&self) -> cache::Result<()> {
            if self.healthy {
                Ok(())
            } else {
                Err(CacheError::RedisHealthCheck("probe failed".to_string()))
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

    impl ProbeDriver {
        fn healthy() -> Self {
            Self::default()
        }

        fn failing() -> Self {
            Self {
                fail_ready: true,
                ..Self::default()
            }
        }
    }

    #[async_trait]
    impl StorageDriver for ProbeDriver {
        async fn put(&self, path: &str, _data: &[u8]) -> aster_drive_storage::Result<String> {
            self.put_calls.fetch_add(1, Ordering::SeqCst);
            Ok(path.to_string())
        }

        async fn get(&self, _path: &str) -> aster_drive_storage::Result<Vec<u8>> {
            Ok(Vec::new())
        }

        async fn get_stream(
            &self,
            _path: &str,
        ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
            Ok(Box::new(tokio::io::empty()))
        }

        async fn delete(&self, _path: &str) -> aster_drive_storage::Result<()> {
            self.delete_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn exists(&self, _path: &str) -> aster_drive_storage::Result<bool> {
            Ok(false)
        }

        async fn metadata(&self, _path: &str) -> aster_drive_storage::Result<BlobMetadata> {
            Ok(BlobMetadata {
                size: 0,
                content_type: None,
            })
        }

        async fn readiness_check(&self) -> aster_drive_storage::Result<()> {
            self.ready_calls.fetch_add(1, Ordering::SeqCst);
            if self.fail_ready {
                Err(aster_drive_storage::StorageError::new(
                    aster_drive_storage::StorageErrorKind::Transient,
                    "readiness probe failed",
                ))
            } else {
                Ok(())
            }
        }
    }

    async fn build_test_state(driver: Option<ProbeDriver>) -> PrimaryAppState {
        let db = crate::db::connect_with_metrics(
            &DatabaseConfig {
                url: "sqlite::memory:".into(),
                ..Default::default()
            },
            aster_drive_metrics::NoopMetrics::arc(),
        )
        .await
        .expect("health test db should connect");
        crate::storage::connectors::test_support::migrate_current_storage_test_schema(&db).await;

        let driver_registry =
            Arc::new(DriverRegistry::noop().expect("built-in storage connector registry"));
        let now = Utc::now();
        let policy_group_id = if let Some(driver) = driver.clone() {
            let mut policy = crate::storage::connectors::test_support::local_policy("");
            policy.name = "Default Policy".to_string();
            policy.is_default = true;
            policy.chunk_size = 5_242_880;
            policy.created_at = now;
            policy.updated_at = now;
            let policy = policy
                .into_active_model()
                .insert(&db)
                .await
                .expect("health test policy should insert");
            driver_registry.insert_for_test(policy.id, Arc::new(driver));
            let group = storage_policy_group::ActiveModel {
                name: Set("Default Policy Group".to_string()),
                description: Set(String::new()),
                is_enabled: Set(true),
                is_default: Set(true),
                admission_config: Set(serde_json::to_string(&crate::services::storage_policy::policy::placement::PlacementPayloadEnvelope::new(crate::services::storage_policy::policy::placement::StorageAdmissionConstraints::default())).unwrap()),
                upload_execution_preference: Set("automatic".to_string()),
                routing_revision: Set(1),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            }
            .insert(&db)
            .await
            .expect("health test policy group should insert");
            let rule = storage_policy_group_rule::ActiveModel {
                group_id: Set(group.id),
                name: Set("Default Rule".to_string()),
                description: Set(String::new()),
                priority: Set(1),
                is_enabled: Set(true),
                matcher: Set(serde_json::to_string(&crate::services::storage_policy::policy::placement::PlacementPayloadEnvelope::new(crate::services::storage_policy::policy::placement::PlacementMatcher::default())).unwrap()),
                selection_mode: Set("first_available".to_string()),
                unavailable_behavior: Set("reject".to_string()),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            }
            .insert(&db)
            .await
            .expect("health test policy rule should insert");
            storage_policy_group_rule_target::ActiveModel {
                rule_id: Set(rule.id),
                policy_id: Set(policy.id),
                weight: Set(100),
                is_enabled: Set(true),
                accepting_new_writes: Set(true),
                stable_order: Set(1),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            }
            .insert(&db)
            .await
            .expect("health test policy target should insert");
            Some(group.id)
        } else {
            None
        };
        user::ActiveModel {
            username: Set("health-admin".to_string()),
            email: Set("health-admin@example.com".to_string()),
            password_hash: Set("test-password-hash".to_string()),
            role: Set(UserRole::Admin),
            status: Set(UserStatus::Active),
            must_change_password: Set(false),
            session_version: Set(1),
            email_verified_at: Set(Some(now)),
            pending_email: Set(None),
            storage_used: Set(0),
            storage_quota: Set(0),
            policy_group_id: Set(policy_group_id),
            created_at: Set(now),
            updated_at: Set(now),
            config: Set(None),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("health test administrator should insert");

        let policy_snapshot = Arc::new(PolicySnapshot::new());
        policy_snapshot
            .reload(&db, driver_registry.connectors())
            .await
            .expect("health test policy snapshot should reload");

        let runtime_config = Arc::new(RuntimeConfig::new());
        let cache = cache::create_cache(&CacheConfig {
            ..Default::default()
        })
        .await;
        let storage_change_bus = crate::services::events::storage_change::StorageChangeBus::new(
            crate::services::events::storage_change::STORAGE_CHANGE_CHANNEL_CAPACITY,
        );
        let share_download_rollback =
            crate::services::share::spawn_detached_share_download_rollback_queue(
                db.clone(),
                crate::config::operations::share_download_rollback_queue_capacity(&runtime_config),
            );

        PrimaryAppState {
            db_handles: aster_forge_db::DbHandles::single(db),
            driver_registry,
            runtime_config: runtime_config.clone(),
            policy_snapshot,
            config: Arc::new(Config::default()),
            cache,
            config_sync: aster_forge_config::ConfigSyncRuntime::disabled_for_test("aster_drive"),
            metrics: aster_drive_metrics::NoopMetrics::arc(),
            mail_sender: sender::runtime_sender(runtime_config),
            storage_change_bus,
            share_download_rollback,
            background_task_dispatch_wakeup:
                crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
            remote_protocol: crate::runtime::PrimaryAppState::new_remote_protocol(),
        }
    }

    fn configure_cluster(state: &mut PrimaryAppState) {
        let mut config = state.config.as_ref().clone();
        config.deployment.profile = crate::config::DeploymentProfile::Cluster;
        config.cache.backend = "redis".to_string();
        config.cache.endpoint = "redis://cache.test:6379/0".into();
        state.config = Arc::new(config);
        state.cache = Arc::new(FakeCache {
            backend_name: "redis",
            healthy: true,
        });
    }

    #[actix_web::test]
    async fn ready_checks_default_storage_readiness_without_write_probe() {
        let driver = ProbeDriver::healthy();
        let response = ready(web::Data::new(build_test_state(Some(driver.clone())).await)).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(driver.ready_calls.load(Ordering::SeqCst), 1);
        assert_eq!(driver.put_calls.load(Ordering::SeqCst), 0);
        assert_eq!(driver.delete_calls.load(Ordering::SeqCst), 0);

        let body = body::to_bytes(response.into_body())
            .await
            .expect("health response body should read");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("health response should be valid json");
        assert_eq!(payload["data"]["status"], "ready");
    }

    #[actix_web::test]
    async fn ready_returns_503_when_default_storage_readiness_fails() {
        let driver = ProbeDriver::failing();
        let response = ready(web::Data::new(build_test_state(Some(driver.clone())).await)).await;

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(driver.ready_calls.load(Ordering::SeqCst), 1);
        assert_eq!(driver.put_calls.load(Ordering::SeqCst), 0);
        assert_eq!(driver.delete_calls.load(Ordering::SeqCst), 0);

        let body = body::to_bytes(response.into_body())
            .await
            .expect("health response body should read");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("health response should be valid json");
        assert_eq!(payload["code"], "storage.transient");
        assert_eq!(payload["msg"], READY_STORAGE_UNAVAILABLE_MESSAGE);
    }

    #[actix_web::test]
    async fn ready_allows_storage_setup_when_default_policy_is_missing() {
        let response = ready(web::Data::new(build_test_state(None).await)).await;

        assert_eq!(response.status(), StatusCode::OK);

        let body = body::to_bytes(response.into_body())
            .await
            .expect("health response body should read");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("health response should be valid json");
        assert_eq!(payload["data"]["status"], "needs_storage");
    }

    #[actix_web::test]
    async fn ready_allows_initial_admin_setup() {
        let state = build_test_state(None).await;
        user::Entity::delete_many()
            .exec(state.db_handles.writer())
            .await
            .expect("health test administrator should delete");

        let response = ready(web::Data::new(state)).await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = body::to_bytes(response.into_body())
            .await
            .expect("health response body should read");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("health response should be valid json");
        assert_eq!(payload["data"]["status"], "needs_admin");
    }

    #[actix_web::test]
    async fn cluster_ready_allows_storage_setup_after_base_dependencies_pass() {
        let mut state = build_test_state(None).await;
        configure_cluster(&mut state);

        let response = ready(web::Data::new(state)).await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = body::to_bytes(response.into_body())
            .await
            .expect("health response body should read");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("health response should be valid json");
        assert_eq!(payload["data"]["status"], "needs_storage");
    }

    #[actix_web::test]
    async fn ready_rejects_local_storage_under_cluster_profile() {
        let driver = ProbeDriver::healthy();
        let mut state = build_test_state(Some(driver.clone())).await;
        configure_cluster(&mut state);

        let response = ready(web::Data::new(state)).await;

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(driver.ready_calls.load(Ordering::SeqCst), 0);
    }

    #[actix_web::test]
    async fn cluster_ready_rejects_memory_fallback_before_storage_probe() {
        let driver = ProbeDriver::healthy();
        let mut state = build_test_state(Some(driver.clone())).await;
        configure_cluster(&mut state);
        state.cache = Arc::new(FakeCache {
            backend_name: "memory",
            healthy: true,
        });

        let response = ready(web::Data::new(state)).await;

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(driver.ready_calls.load(Ordering::SeqCst), 0);
        let body = body::to_bytes(response.into_body())
            .await
            .expect("health response body should read");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("health response should be valid json");
        assert_eq!(payload["code"], "config.error");
        assert_eq!(payload["msg"], super::READY_CACHE_UNAVAILABLE_MESSAGE);
    }

    #[actix_web::test]
    async fn single_ready_accepts_healthy_memory_fallback() {
        let driver = ProbeDriver::healthy();
        let mut state = build_test_state(Some(driver.clone())).await;
        let mut config = state.config.as_ref().clone();
        config.cache.backend = "redis".to_string();
        config.cache.endpoint = "redis://unavailable-cache.test:6379/0".into();
        state.config = Arc::new(config);
        state.cache = Arc::new(FakeCache {
            backend_name: "memory",
            healthy: true,
        });

        let response = ready(web::Data::new(state)).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(driver.ready_calls.load(Ordering::SeqCst), 1);
    }

    #[actix_web::test]
    async fn cluster_ready_rejects_failed_redis_health_check_before_storage_probe() {
        let driver = ProbeDriver::healthy();
        let mut state = build_test_state(Some(driver.clone())).await;
        configure_cluster(&mut state);
        state.cache = Arc::new(FakeCache {
            backend_name: "redis",
            healthy: false,
        });

        let response = ready(web::Data::new(state)).await;

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(driver.ready_calls.load(Ordering::SeqCst), 0);
    }

    #[actix_web::test]
    async fn cluster_follower_ready_rejects_memory_fallback() {
        let mut state = build_test_state(None).await;
        configure_cluster(&mut state);
        state.cache = Arc::new(FakeCache {
            backend_name: "memory",
            healthy: true,
        });

        let response = follower_ready(web::Data::new(state.follower_view())).await;

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = body::to_bytes(response.into_body())
            .await
            .expect("health response body should read");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("health response should be valid json");
        assert_eq!(payload["code"], "config.error");
        assert_eq!(payload["msg"], super::READY_CACHE_UNAVAILABLE_MESSAGE);
    }

    #[actix_web::test]
    async fn cluster_ready_rejects_non_redis_configuration() {
        let mut state = build_test_state(None).await;
        let mut config = state.config.as_ref().clone();
        config.deployment.profile = crate::config::DeploymentProfile::Cluster;
        state.config = Arc::new(config);

        let response = ready(web::Data::new(state)).await;

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = body::to_bytes(response.into_body())
            .await
            .expect("health response body should read");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("health response should be valid json");
        assert_eq!(payload["code"], "config.error");
        assert_eq!(payload["msg"], super::READY_CACHE_UNAVAILABLE_MESSAGE);
    }

    #[actix_web::test]
    async fn readiness_checks_the_configured_cache_for_every_deployment_profile() {
        let driver = ProbeDriver::healthy();
        let mut state = build_test_state(Some(driver.clone())).await;
        state.cache = Arc::new(FakeCache {
            backend_name: "memory",
            healthy: false,
        });

        let response = ready(web::Data::new(state)).await;

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(driver.ready_calls.load(Ordering::SeqCst), 0);
    }
}

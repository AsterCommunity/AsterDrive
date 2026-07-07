//! API 路由：`health`。

use crate::api::api_error_code::ApiErrorCode;
use crate::api::response::{ApiResponse, HealthResponse, MemoryStatsResponse, SystemInfoResponse};
use crate::runtime::{FollowerAppState, PrimaryAppState, SharedRuntimeState};
use crate::services::ops::health;
use actix_web::{HttpResponse, web};

const READY_DB_UNAVAILABLE_MESSAGE: &str = "Database unavailable";
const READY_STORAGE_UNAVAILABLE_MESSAGE: &str = "Storage unavailable";
#[cfg(feature = "metrics")]
const METRICS_EXPORT_FAILED_MESSAGE: &str = "Failed to export metrics";

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

    #[cfg(feature = "metrics")]
    let scope = scope.route("/metrics", web::get().to(metrics_endpoint));

    scope
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
    if let Err(error) = health::ping_database(state.get_ref().writer_db()).await {
        return ready_database_error(error);
    }

    match health::check_primary_ready(state.get_ref()).await {
        Ok(_) => HttpResponse::Ok().json(ApiResponse::ok(status_response("ready"))),
        Err(error) => ready_storage_error(error),
    }
}

pub async fn follower_ready(state: web::Data<FollowerAppState>) -> HttpResponse {
    if let Err(error) = health::ping_database(state.get_ref().writer_db()).await {
        return ready_database_error(error);
    }

    match health::check_follower_ready(state.get_ref()).await {
        Ok(_) => HttpResponse::Ok().json(ApiResponse::ok(status_response("ready"))),
        Err(error) => ready_storage_error(error),
    }
}

fn ready_database_error(error: crate::errors::AsterError) -> HttpResponse {
    tracing::error!(error = %error, "health readiness database ping failed");
    HttpResponse::ServiceUnavailable().json(ApiResponse::<()>::error(
        ApiErrorCode::DatabaseError,
        READY_DB_UNAVAILABLE_MESSAGE,
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

#[cfg_attr(not(all(debug_assertions, feature = "openapi")), allow(dead_code))]
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

#[cfg(feature = "metrics")]
pub async fn metrics_endpoint() -> HttpResponse {
    let Some(metrics) = crate::metrics::get_metrics() else {
        return HttpResponse::ServiceUnavailable()
            .content_type("text/plain")
            .body("Metrics not initialized");
    };

    match metrics.export() {
        Ok(output) => HttpResponse::Ok()
            .content_type("text/plain; version=0.0.4; charset=utf-8")
            .body(output),
        Err(e) => {
            tracing::error!(error = %e, "metrics export failed");
            HttpResponse::InternalServerError().body(METRICS_EXPORT_FAILED_MESSAGE)
        }
    }
}

pub fn system_info_response() -> SystemInfoResponse {
    SystemInfoResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        build_time: compile_time().to_string(),
    }
}

#[inline]
pub(crate) fn compile_time() -> &'static str {
    option_env!("ASTER_BUILD_TIME").unwrap_or("unknown")
}

fn status_response(status: &str) -> HealthResponse {
    HealthResponse {
        status: status.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{READY_STORAGE_UNAVAILABLE_MESSAGE, ready};
    use aster_forge_cache as cache;
    use crate::config::{CacheConfig, Config, DatabaseConfig, RuntimeConfig};
    use crate::entities::storage_policy;
    use crate::runtime::PrimaryAppState;
    use crate::services::mail::sender;
    use crate::storage::BlobMetadata;
    use crate::storage::{DriverRegistry, PolicySnapshot, StorageDriver};
    use crate::types::{DriverType, StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions};
    use actix_web::{body, http::StatusCode, web};
    use async_trait::async_trait;
    use chrono::Utc;
    use migration::Migrator;
    use sea_orm::{ActiveModelTrait, Set};
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
        async fn put(&self, path: &str, _data: &[u8]) -> crate::errors::Result<String> {
            self.put_calls.fetch_add(1, Ordering::SeqCst);
            Ok(path.to_string())
        }

        async fn get(&self, _path: &str) -> crate::errors::Result<Vec<u8>> {
            Ok(Vec::new())
        }

        async fn get_stream(
            &self,
            _path: &str,
        ) -> crate::errors::Result<Box<dyn AsyncRead + Unpin + Send>> {
            Ok(Box::new(tokio::io::empty()))
        }

        async fn delete(&self, _path: &str) -> crate::errors::Result<()> {
            self.delete_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn exists(&self, _path: &str) -> crate::errors::Result<bool> {
            Ok(false)
        }

        async fn metadata(&self, _path: &str) -> crate::errors::Result<BlobMetadata> {
            Ok(BlobMetadata {
                size: 0,
                content_type: None,
            })
        }

        async fn readiness_check(&self) -> crate::errors::Result<()> {
            self.ready_calls.fetch_add(1, Ordering::SeqCst);
            if self.fail_ready {
                Err(crate::errors::AsterError::storage_driver_error(
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
                url: "sqlite::memory:".to_string(),
                ..Default::default()
            },
            crate::metrics_core::NoopMetrics::arc(),
        )
        .await
        .expect("health test db should connect");
        Migrator::up(&db, None)
            .await
            .expect("health test migrations should apply");

        let driver_registry = Arc::new(DriverRegistry::noop());
        if let Some(driver) = driver.clone() {
            let now = Utc::now();
            let policy = storage_policy::ActiveModel {
                name: Set("Default Policy".to_string()),
                driver_type: Set(DriverType::Local),
                endpoint: Set(String::new()),
                bucket: Set(String::new()),
                access_key: Set(String::new()),
                secret_key: Set(String::new()),
                base_path: Set(String::new()),
                max_file_size: Set(0),
                allowed_types: Set(StoredStoragePolicyAllowedTypes::empty()),
                options: Set(StoredStoragePolicyOptions::empty()),
                is_default: Set(true),
                chunk_size: Set(5_242_880),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            }
            .insert(&db)
            .await
            .expect("health test policy should insert");
            driver_registry.insert_for_test(policy.id, Arc::new(driver));
        }

        let policy_snapshot = Arc::new(PolicySnapshot::new());
        policy_snapshot
            .reload(&db)
            .await
            .expect("health test policy snapshot should reload");

        let runtime_config = Arc::new(RuntimeConfig::new());
        let cache = cache::create_cache(&CacheConfig {
            ..Default::default()
        })
        .await;
        let (storage_change_tx, _) = tokio::sync::broadcast::channel(
            crate::services::events::storage_change::STORAGE_CHANGE_CHANNEL_CAPACITY,
        );
        let share_download_rollback =
            crate::services::share::spawn_detached_share_download_rollback_queue(
                db.clone(),
                crate::config::operations::share_download_rollback_queue_capacity(&runtime_config),
            );

        PrimaryAppState {
            db_handles: crate::db::DbHandles::single(db),
            driver_registry,
            runtime_config: runtime_config.clone(),
            policy_snapshot,
            config: Arc::new(Config::default()),
            cache,
            metrics: crate::metrics_core::NoopMetrics::arc(),
            mail_sender: sender::runtime_sender(runtime_config),
            storage_change_tx,
            share_download_rollback,
            background_task_dispatch_wakeup:
                crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
            remote_protocol: crate::runtime::PrimaryAppState::new_remote_protocol(),
        }
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
        assert_eq!(payload["code"], "storage.unknown");
        assert_eq!(payload["msg"], READY_STORAGE_UNAVAILABLE_MESSAGE);
    }

    #[actix_web::test]
    async fn ready_returns_503_when_default_storage_policy_is_missing() {
        let response = ready(web::Data::new(build_test_state(None).await)).await;

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        let body = body::to_bytes(response.into_body())
            .await
            .expect("health response body should read");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("health response should be valid json");
        assert_eq!(payload["code"], "storage.policy_not_found");
        assert_eq!(payload["msg"], READY_STORAGE_UNAVAILABLE_MESSAGE);
    }
}

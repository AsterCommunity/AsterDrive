//! CORS 中间件测试。

use super::constants::{ALLOWED_HEADERS, ALLOWED_METHODS};
use super::{
    RuntimeCors, apply_origin_headers, ensure_vary, is_cors_exempt_path, request_is_same_origin,
    requested_headers_are_allowed, requested_method_is_allowed,
};
use crate::config::cors::{CorsAllowedOrigins, RuntimeCorsPolicy};
use crate::config::definitions::CONFIG_CATEGORY_NETWORK;
use crate::config::{CacheConfig, Config, DatabaseConfig, RuntimeConfig};
use crate::runtime::PrimaryAppState;
use actix_web::{
    App, HttpResponse,
    http::header::{self, HeaderMap, HeaderValue},
    test as actix_test, web,
};
use aster_forge_cache as cache;
use aster_forge_db::system_config;
use chrono::Utc;
use std::sync::Arc;

fn config_model(key: &str, value: &str) -> system_config::Model {
    system_config::Model {
        id: 0,
        key: key.to_string(),
        value: value.to_string(),
        value_type: crate::types::ConfigValueType::String,
        requires_restart: false,
        is_sensitive: false,
        source: crate::types::ConfigSource::System,
        visibility: crate::types::ConfigVisibility::Private,
        namespace: String::new(),
        category: CONFIG_CATEGORY_NETWORK.to_string(),
        description: "test".to_string(),
        updated_at: Utc::now(),
        updated_by: None,
    }
}

fn test_policy(
    enabled: bool,
    allowed_origins: CorsAllowedOrigins,
    allow_credentials: bool,
) -> RuntimeCorsPolicy {
    RuntimeCorsPolicy {
        enabled,
        allowed_origins,
        allow_credentials,
        max_age_secs: 600,
    }
}

async fn test_state(configs: &[(&str, &str)]) -> PrimaryAppState {
    let db = crate::db::connect_with_metrics(
        &DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            pool_size: 1,
            retry_count: 0,
        },
        crate::metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();

    let runtime_config = Arc::new(RuntimeConfig::new());
    for (key, value) in configs {
        runtime_config.apply(config_model(key, value));
    }

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
        driver_registry: Arc::new(crate::storage::DriverRegistry::noop()),
        runtime_config: runtime_config.clone(),
        policy_snapshot: Arc::new(crate::storage::PolicySnapshot::new()),
        config: Arc::new(Config::default()),
        cache,
        metrics: crate::metrics::NoopMetrics::arc(),
        mail_sender: crate::services::mail::sender::runtime_sender(runtime_config.clone()),
        storage_change_tx,
        share_download_rollback,
        background_task_dispatch_wakeup:
            crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
        remote_protocol: crate::runtime::PrimaryAppState::new_remote_protocol(),
    }
}

#[test]
fn cors_exempt_paths_cover_static_assets_and_manifest() {
    for path in [
        "/",
        "/index.html",
        "/favicon.svg",
        "/manifest.webmanifest",
        "/sw.js",
        "/workbox-abc123.js",
        "/assets/app.js",
        "/static/logo.png",
        "/pdfjs/viewer.js",
    ] {
        assert!(
            is_cors_exempt_path(path),
            "{path} should bypass CORS checks"
        );
    }

    assert!(!is_cors_exempt_path("/api/v1/auth/check"));
    assert!(!is_cors_exempt_path("/manifest.json"));
}

#[actix_web::test]
async fn request_same_origin_matches_scheme_and_host_case_insensitively() {
    let req = actix_test::TestRequest::get()
        .uri("/health")
        .insert_header((header::HOST, "Drive.EXAMPLE.com:8443"))
        .to_srv_request();

    assert!(request_is_same_origin(
        &req,
        "http://drive.example.com:8443",
    ));
    assert!(!request_is_same_origin(
        &req,
        "https://drive.example.com:8443",
    ));
}

#[actix_web::test]
async fn requested_method_validation_accepts_known_and_rejects_unknown_methods() {
    let req = actix_test::TestRequest::default()
        .insert_header((header::ACCESS_CONTROL_REQUEST_METHOD, "GET"))
        .to_srv_request();
    assert!(requested_method_is_allowed(&req));

    let req = actix_test::TestRequest::default()
        .insert_header((header::ACCESS_CONTROL_REQUEST_METHOD, "TRACE"))
        .to_srv_request();
    assert!(!requested_method_is_allowed(&req));

    assert!(ALLOWED_METHODS.contains(&"PROPFIND"));
    assert!(ALLOWED_METHODS.contains(&"LOCK"));
}

#[actix_web::test]
async fn requested_headers_validation_accepts_known_headers_case_insensitively() {
    let req = actix_test::TestRequest::default()
        .insert_header((
            header::ACCESS_CONTROL_REQUEST_HEADERS,
            "Authorization, Content-Type, LOCK-TOKEN, Range, X-WOPI-Override, X-WOPI-Lock",
        ))
        .to_srv_request();

    assert!(requested_headers_are_allowed(&req).unwrap());
    assert!(ALLOWED_HEADERS.contains(&"authorization"));
    assert!(ALLOWED_HEADERS.contains(&"lock-token"));
    assert!(ALLOWED_HEADERS.contains(&"range"));
    assert!(ALLOWED_HEADERS.contains(&"x-wopi-override"));
    assert!(ALLOWED_HEADERS.contains(&"x-wopi-lock"));
}

#[actix_web::test]
async fn requested_headers_validation_rejects_unknown_header_names() {
    let req = actix_test::TestRequest::default()
        .insert_header((header::ACCESS_CONTROL_REQUEST_HEADERS, "x-custom-header"))
        .to_srv_request();

    assert!(!requested_headers_are_allowed(&req).unwrap());
}

#[actix_web::test]
async fn requested_headers_validation_rejects_invalid_header_syntax() {
    let req = actix_test::TestRequest::default()
        .insert_header((header::ACCESS_CONTROL_REQUEST_HEADERS, "bad header"))
        .to_srv_request();

    let err = requested_headers_are_allowed(&req).unwrap_err();
    assert!(err.message().contains("Access-Control-Request-Headers"));
}

#[test]
fn apply_origin_headers_sets_origin_credentials_and_vary() {
    let policy = test_policy(
        true,
        CorsAllowedOrigins::List(vec!["https://drive.example.com".to_string()]),
        true,
    );
    let mut headers = HeaderMap::new();

    apply_origin_headers(&mut headers, &policy, "https://drive.example.com").unwrap();

    assert_eq!(
        headers
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap()
            .to_str()
            .unwrap(),
        "https://drive.example.com"
    );
    assert_eq!(
        headers
            .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
            .unwrap()
            .to_str()
            .unwrap(),
        "true"
    );
    assert!(
        headers
            .get(header::VARY)
            .unwrap()
            .to_str()
            .unwrap()
            .contains("Origin")
    );
}

#[test]
fn apply_origin_headers_preserves_existing_allow_origin_header() {
    let policy = test_policy(true, CorsAllowedOrigins::Any, true);
    let mut headers = HeaderMap::new();
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("https://existing.example.com"),
    );

    apply_origin_headers(&mut headers, &policy, "https://drive.example.com").unwrap();

    assert_eq!(
        headers
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap()
            .to_str()
            .unwrap(),
        "https://existing.example.com"
    );
    assert_eq!(
        headers
            .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
            .unwrap()
            .to_str()
            .unwrap(),
        "true"
    );
}

#[test]
fn ensure_vary_deduplicates_values() {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::VARY,
        HeaderValue::from_static("Origin, Access-Control-Request-Method"),
    );

    ensure_vary(&mut headers, "Origin").unwrap();
    ensure_vary(&mut headers, "Access-Control-Request-Headers").unwrap();

    assert_eq!(
        headers.get(header::VARY).unwrap().to_str().unwrap(),
        "Access-Control-Request-Headers, Access-Control-Request-Method, Origin"
    );
}

#[actix_web::test]
async fn middleware_does_not_allow_public_site_origin_preflight_without_whitelist() {
    let state = test_state(&[
        ("cors_enabled", "true"),
        ("public_site_url", r#"["https://drive.example.com"]"#),
    ])
    .await;
    let app = actix_test::init_service(
        App::new()
            .wrap(RuntimeCors)
            .app_data(web::Data::new(state))
            .route(
                "/health",
                web::get().to(|| async { HttpResponse::Ok().finish() }),
            ),
    )
    .await;

    let req = actix_test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri("/health")
        .insert_header((header::HOST, "internal.example.local"))
        .insert_header((header::ORIGIN, "https://drive.example.com"))
        .insert_header((header::ACCESS_CONTROL_REQUEST_METHOD, "GET"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;

    assert_eq!(resp.status(), 404);
    assert!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );
}

#[actix_web::test]
async fn middleware_does_not_add_public_site_origin_to_passthrough_response_without_origin_header()
{
    let state = test_state(&[
        ("cors_enabled", "true"),
        ("public_site_url", r#"["https://drive.example.com"]"#),
    ])
    .await;
    let app = actix_test::init_service(
        App::new()
            .wrap(RuntimeCors)
            .app_data(web::Data::new(state))
            .route(
                "/health",
                web::get().to(|| async { HttpResponse::Ok().finish() }),
            ),
    )
    .await;

    let req = actix_test::TestRequest::get().uri("/health").to_request();
    let resp = actix_test::call_service(&app, req).await;

    assert_eq!(resp.status(), 200);
    assert!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );
}

#[actix_web::test]
async fn middleware_preserves_existing_allow_origin_header() {
    let state = test_state(&[
        ("cors_enabled", "true"),
        ("public_site_url", r#"["https://drive.example.com"]"#),
    ])
    .await;
    let app = actix_test::init_service(
        App::new()
            .wrap(RuntimeCors)
            .app_data(web::Data::new(state))
            .route(
                "/custom",
                web::get().to(|| async {
                    HttpResponse::Ok()
                        .insert_header((
                            header::ACCESS_CONTROL_ALLOW_ORIGIN,
                            "https://existing.example.com",
                        ))
                        .finish()
                }),
            ),
    )
    .await;

    let req = actix_test::TestRequest::get().uri("/custom").to_request();
    let resp = actix_test::call_service(&app, req).await;

    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap()
            .to_str()
            .unwrap(),
        "https://existing.example.com"
    );
}

#[actix_web::test]
async fn middleware_exposes_pdf_range_response_headers() {
    let state = test_state(&[
        ("cors_enabled", "true"),
        ("cors_allowed_origins", "https://drive.example.com"),
    ])
    .await;
    let app = actix_test::init_service(
        App::new()
            .wrap(RuntimeCors)
            .app_data(web::Data::new(state))
            .route(
                "/api/v1/files/1/download",
                web::get().to(|| async {
                    HttpResponse::PartialContent()
                        .insert_header(("Accept-Ranges", "bytes"))
                        .insert_header(("Content-Range", "bytes 0-99/1000"))
                        .insert_header(("Content-Length", "100"))
                        .finish()
                }),
            ),
    )
    .await;

    let req = actix_test::TestRequest::get()
        .uri("/api/v1/files/1/download")
        .insert_header((header::HOST, "internal.example.local"))
        .insert_header((header::ORIGIN, "https://drive.example.com"))
        .insert_header((header::RANGE, "bytes=0-99"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;

    assert_eq!(resp.status(), 206);
    let exposed = resp
        .headers()
        .get(header::ACCESS_CONTROL_EXPOSE_HEADERS)
        .expect("CORS actual responses should expose headers")
        .to_str()
        .unwrap();
    assert!(
        exposed
            .split(',')
            .any(|header| header.trim() == "accept-ranges")
    );
    assert!(
        exposed
            .split(',')
            .any(|header| header.trim() == "content-range")
    );
    assert!(
        exposed
            .split(',')
            .any(|header| header.trim() == "content-length")
    );
}

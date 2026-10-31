//! 集成测试：`cors`。

use crate::common;

use actix_web::{App, body::to_bytes, http::header, test, web};
use serde_json::Value;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

const EXPECTED_ALLOW_HEADERS: &str = "authorization, accept, content-type, depth, destination, if, lock-token, overwrite, range, timeout, x-csrf-token, x-wopi-lock, x-wopi-oldlock, x-wopi-override, x-wopi-overwriterelativetarget, x-wopi-requestedname, x-wopi-relativetarget, x-wopi-size, x-wopi-suggestedtarget";
const EXPECTED_EXPOSE_HEADERS: &str = "accept-ranges, content-length, content-range, dav, etag, lock-token, x-wopi-itemversion, x-wopi-invalidfilenameerror, x-wopi-lock, x-wopi-lockfailurereason, x-wopi-validrelativetarget";

async fn configured_cors_state() -> aster_drive::runtime::PrimaryAppState {
    let state = common::setup().await;
    for (key, value) in [
        ("cors_enabled", "true"),
        ("cors_allowed_origins", r#"["https://allowed.example.com"]"#),
    ] {
        state
            .runtime_config
            .apply(common::system_config_model(key, value));
    }
    state
}

#[actix_web::test]
async fn test_runtime_cors_rejection_factory_uses_drive_envelope_without_executing_handler() {
    let state = configured_cors_state().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let handler_calls = calls.clone();
    let app = test::init_service(
        App::new()
            .wrap(aster_drive::api::middleware::cors::runtime_cors())
            .app_data(web::Data::new(state))
            .default_service(web::to(move || {
                handler_calls.fetch_add(1, Ordering::SeqCst);
                async { actix_web::HttpResponse::Ok().finish() }
            })),
    )
    .await;
    for (method, origin, requested_method, requested_headers, message) in [
        (
            "GET",
            "https://blocked.example.com",
            None,
            None,
            "CORS origin is not allowed",
        ),
        (
            "OPTIONS",
            "https://blocked.example.com",
            Some("GET"),
            None,
            "CORS origin is not allowed",
        ),
        (
            "OPTIONS",
            "https://allowed.example.com",
            Some("TRACE"),
            None,
            "CORS preflight method is not allowed",
        ),
        (
            "OPTIONS",
            "https://allowed.example.com",
            Some("GET"),
            Some("x-denied"),
            "CORS preflight headers are not allowed",
        ),
    ] {
        let mut request = test::TestRequest::default()
            .method(method.parse().unwrap())
            .uri("/probe")
            .insert_header((header::ORIGIN, origin));
        if let Some(method) = requested_method {
            request = request.insert_header((header::ACCESS_CONTROL_REQUEST_METHOD, method));
        }
        if let Some(headers) = requested_headers {
            request = request.insert_header((header::ACCESS_CONTROL_REQUEST_HEADERS, headers));
        }
        let response = test::call_service(&app, request.to_request()).await;
        assert_eq!(response.status(), 403);
        for name in [
            header::ORIGIN.as_str(),
            "Access-Control-Request-Method",
            "Access-Control-Request-Headers",
        ] {
            header_contains(&response, header::VARY, name);
        }
        for name in [
            "access-control-allow-origin",
            "access-control-allow-credentials",
            "access-control-allow-methods",
            "access-control-allow-headers",
            "access-control-expose-headers",
            "access-control-max-age",
        ] {
            assert!(
                !response.headers().contains_key(name),
                "rejection granted {name}"
            );
        }
        let body: Value = test::read_body_json(response).await;
        assert_eq!(body["code"], "forbidden");
        assert_eq!(body["msg"], message);
        assert!(body["data"].is_null());
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[actix_web::test]
async fn test_runtime_cors_factory_does_not_rewrite_handler_responses_or_bypasses() {
    let state = configured_cors_state().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let handler_calls = calls.clone();
    let app = test::init_service(
        App::new()
            .wrap(aster_drive::api::middleware::cors::runtime_cors())
            .app_data(web::Data::new(state.clone()))
            .default_service(web::to(move |request: actix_web::HttpRequest| {
                handler_calls.fetch_add(1, Ordering::SeqCst);
                async move {
                    let mut response = actix_web::HttpResponse::Forbidden();
                    response.append_header((header::VARY, "Accept-Encoding"));
                    response.append_header((
                        header::VARY,
                        if request.path() == "/wildcard" {
                            "*"
                        } else {
                            "Accept-Language"
                        },
                    ));
                    response.insert_header(("X-Product-Header", "preserved"));
                    response.finish()
                }
            })),
    )
    .await;
    for (path, origin) in [
        ("/probe", Some("https://allowed.example.com")),
        ("/probe", Some("http://localhost:8080")),
        ("/probe", None),
        ("/assets/example.js", Some("https://blocked.example.com")),
        ("/wildcard", Some("https://allowed.example.com")),
    ] {
        let mut request = test::TestRequest::get()
            .uri(path)
            .insert_header((header::HOST, "localhost:8080"));
        if let Some(origin) = origin {
            request = request.insert_header((header::ORIGIN, origin));
        }
        let response = test::call_service(&app, request.to_request()).await;
        assert_eq!(response.status(), 403);
        assert_eq!(
            response.headers().get("X-Product-Header").unwrap(),
            "preserved"
        );
        header_contains(&response, header::VARY, "Accept-Encoding");
        header_contains(
            &response,
            header::VARY,
            if path == "/wildcard" {
                "*"
            } else {
                "Accept-Language"
            },
        );
        assert!(
            test::read_body(response).await.is_empty(),
            "handler's own empty 403 must remain empty"
        );
    }
    // Dynamic inactive policy bypasses even malformed Origin; no rejection or parser inference.
    for (key, value) in [("cors_enabled", "false"), ("cors_allowed_origins", "[]")] {
        state
            .runtime_config
            .apply(common::system_config_model(key, value));
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/probe")
                .insert_header((header::ORIGIN, "null"))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), 403);
        assert!(test::read_body(response).await.is_empty());
        state
            .runtime_config
            .apply(common::system_config_model("cors_enabled", "true"));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 7);
}

#[derive(Default)]
struct CorsMetricsProbe {
    statuses: std::sync::Mutex<Vec<u16>>,
}

impl aster_forge_metrics::DbMetricsRecorder for CorsMetricsProbe {
    fn enabled(&self) -> bool {
        true
    }
    fn record_db_query(&self, _: &aster_forge_metrics::DbQueryMetric) {}
}

impl aster_forge_metrics::MetricsRecorder for CorsMetricsProbe {
    fn record_http_request(&self, _: &str, _: &str, _: &str, status: u16, _: f64) {
        self.statuses.lock().unwrap().push(status);
    }
}

#[actix_web::test]
async fn test_runtime_cors_rejection_and_rate_limit_keep_outer_response_middleware() {
    use actix_governor::Governor;
    use std::num::{NonZeroU32, NonZeroU64};
    let state = configured_cors_state().await;
    let metrics = Arc::new(CorsMetricsProbe::default());
    let recorder: aster_forge_metrics::SharedMetricsRecorder = metrics.clone();
    let governor = aster_drive::api::middleware::rate_limit::build_governor(
        &aster_drive::config::RateLimitTier {
            seconds_per_request: NonZeroU64::new(3600).unwrap(),
            burst_size: NonZeroU32::new(1).unwrap(),
        },
        &[],
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let handler_calls = calls.clone();
    let app = test::init_service(
        App::new()
            .wrap(aster_drive::api::middleware::cors::runtime_cors())
            .wrap(aster_forge_middleware::actix::request_id::RequestIdMiddleware)
            .wrap(aster_forge_middleware::actix::metrics::MetricsMiddleware)
            .wrap(aster_forge_middleware::actix::security_headers::default_headers())
            .app_data(web::Data::new(state))
            .app_data(web::Data::new(recorder))
            .service(
                web::resource("/probe")
                    .wrap(Governor::new(&governor))
                    .to(move || {
                        handler_calls.fetch_add(1, Ordering::SeqCst);
                        async { actix_web::HttpResponse::Ok().finish() }
                    }),
            ),
    )
    .await;
    for (origin, expected_status, expected_code) in [
        ("https://blocked.example.com", 403, Some("forbidden")),
        ("https://allowed.example.com", 200, None),
        ("https://allowed.example.com", 429, Some("rate_limited")),
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/probe")
                .peer_addr("127.0.0.1:6340".parse().unwrap())
                .insert_header((header::ORIGIN, origin))
                .to_request(),
        )
        .await;
        assert_eq!(response.status().as_u16(), expected_status);
        assert!(
            uuid::Uuid::parse_str(
                response
                    .headers()
                    .get("X-Request-ID")
                    .unwrap()
                    .to_str()
                    .unwrap()
            )
            .is_ok()
        );
        assert_eq!(
            response.headers().get("X-Content-Type-Options").unwrap(),
            "nosniff"
        );
        if expected_status == 429 {
            assert!(response.headers().contains_key("Retry-After"));
            assert_eq!(
                response
                    .headers()
                    .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                    .unwrap(),
                origin
            );
        }
        let body = test::read_body(response).await;
        if let Some(code) = expected_code {
            assert_eq!(
                serde_json::from_slice::<Value>(&body).unwrap()["code"],
                code
            );
        }
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(*metrics.statuses.lock().unwrap(), vec![403, 200, 429]);
}

macro_rules! create_test_app_with_cors {
    ($state:expr) => {{
        let state = $state;
        let db = state.writer_db().clone();
        test::init_service(
            App::new()
                .wrap(aster_drive::api::middleware::cors::runtime_cors())
                .wrap(aster_forge_middleware::actix::security_headers::default_headers())
                .app_data(web::PayloadConfig::new(10 * 1024 * 1024))
                .app_data(web::JsonConfig::default().limit(1024 * 1024))
                .app_data(web::Data::new(state))
                .configure(move |cfg| {
                    aster_drive::api::configure_primary(cfg, &db, &Default::default())
                }),
        )
        .await
    }};
}

macro_rules! set_config {
    ($app:expr, $token:expr, $key:expr, $value:expr) => {{
        let req = test::TestRequest::put()
            .uri(&format!("/api/v1/admin/config/{}", $key))
            .insert_header(("Cookie", common::access_cookie_header(&$token)))
            .insert_header(common::csrf_header_for(&$token))
            .set_json(serde_json::json!({ "value": $value }))
            .to_request();
        test::call_service(&$app, req).await
    }};
}

macro_rules! enable_cors {
    ($app:expr, $token:expr) => {{
        let resp = set_config!($app, $token, "cors_enabled", "true");
        assert_eq!(resp.status(), 200);
    }};
}

fn header_contains<B>(
    resp: &actix_web::dev::ServiceResponse<B>,
    name: header::HeaderName,
    value: &str,
) {
    let actual = resp
        .headers()
        .get_all(name)
        .map(|value| value.to_str().unwrap())
        .collect::<Vec<_>>()
        .join(", ")
        .to_ascii_lowercase();
    assert!(
        actual.contains(&value.to_ascii_lowercase()),
        "expected header to contain '{value}', got '{actual}'"
    );
}

#[actix_web::test]
async fn test_runtime_cors_defaults_passthrough_cross_origin_actual_request() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);

    let req = test::TestRequest::get()
        .uri("/health")
        .insert_header((header::ORIGIN, "https://app.example.com"))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 200);
    assert!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );
}

#[actix_web::test]
async fn test_runtime_cors_defaults_passthrough_browser_extension_webdav_request() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);

    let req = test::TestRequest::get()
        .uri("/webdav/")
        .insert_header((
            header::ORIGIN,
            "chrome-extension://iikmkjmpaadaobahmlepeloendndfphd",
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 401);
    assert!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );
}

#[actix_web::test]
async fn test_runtime_cors_allows_configured_browser_extension_origin() {
    const EXTENSION_ORIGIN: &str = "chrome-extension://iikmkjmpaadaobahmlepeloendndfphd";

    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);
    let (token, _) = register_and_login!(app);
    enable_cors!(app, token);
    let resp = set_config!(
        app,
        token,
        "cors_allowed_origins",
        serde_json::json!([EXTENSION_ORIGIN])
    );
    assert_eq!(resp.status(), 200);
    let resp = set_config!(app, token, "cors_allow_credentials", "true");
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri("/webdav/")
        .insert_header((header::ORIGIN, EXTENSION_ORIGIN))
        .insert_header((header::ACCESS_CONTROL_REQUEST_METHOD, "PROPFIND"))
        .insert_header((
            header::ACCESS_CONTROL_REQUEST_HEADERS,
            "authorization, depth",
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 204);
    assert_eq!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap()
            .to_str()
            .unwrap(),
        EXTENSION_ORIGIN
    );
    assert_eq!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
            .unwrap(),
        "true"
    );

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri("/webdav/")
        .insert_header((
            header::ORIGIN,
            "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ))
        .insert_header((header::ACCESS_CONTROL_REQUEST_METHOD, "PROPFIND"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn test_runtime_cors_admin_rejects_legacy_string_origin_value() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);
    let (token, _) = register_and_login!(app);

    let resp = set_config!(
        app,
        token,
        "cors_allowed_origins",
        "https://app.example.com"
    );
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn test_runtime_cors_same_origin_origin_header_is_not_blocked() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);

    let req = test::TestRequest::get()
        .uri("/health")
        .insert_header((header::HOST, "localhost:8080"))
        .insert_header((header::ORIGIN, "http://localhost:8080"))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 200);
    assert!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );
}

#[actix_web::test]
async fn test_runtime_cors_hot_reload_updates_whitelist_and_max_age() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);
    let (token, _) = register_and_login!(app);
    enable_cors!(app, token);

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/cors_allowed_origins")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "value": ["https://app.example.com/"] }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["value"],
        serde_json::json!(["https://app.example.com"])
    );

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri("/health")
        .insert_header((header::ORIGIN, "https://app.example.com"))
        .insert_header((header::ACCESS_CONTROL_REQUEST_METHOD, "GET"))
        .insert_header((header::ACCESS_CONTROL_REQUEST_HEADERS, "authorization"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 204);
    assert_eq!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap()
            .to_str()
            .unwrap(),
        "https://app.example.com"
    );
    assert_eq!(
        resp.headers()
            .get(header::ACCESS_CONTROL_MAX_AGE)
            .unwrap()
            .to_str()
            .unwrap(),
        "3600"
    );

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/cors_allowed_origins")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "value": ["https://dashboard.example.com"] }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/cors_max_age_secs")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "value": "600" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri("/health")
        .insert_header((header::ORIGIN, "https://app.example.com"))
        .insert_header((header::ACCESS_CONTROL_REQUEST_METHOD, "GET"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri("/health")
        .insert_header((header::ORIGIN, "https://dashboard.example.com"))
        .insert_header((header::ACCESS_CONTROL_REQUEST_METHOD, "GET"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 204);
    assert_eq!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap()
            .to_str()
            .unwrap(),
        "https://dashboard.example.com"
    );
    assert_eq!(
        resp.headers()
            .get(header::ACCESS_CONTROL_MAX_AGE)
            .unwrap()
            .to_str()
            .unwrap(),
        "600"
    );
}

#[actix_web::test]
async fn test_runtime_cors_credentials_require_explicit_origin_list() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/cors_allowed_origins")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "value": ["*"] }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/cors_allow_credentials")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "value": "true" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert!(
        body["msg"]
            .as_str()
            .unwrap()
            .contains("cors_allow_credentials cannot be true")
    );
}

#[actix_web::test]
async fn test_runtime_cors_adds_credentials_header_for_allowed_origin() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);
    let (token, _) = register_and_login!(app);
    enable_cors!(app, token);

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/cors_allowed_origins")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "value": ["https://panel.example.com"] }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/cors_allow_credentials")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "value": "true" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/health")
        .insert_header((header::ORIGIN, "https://panel.example.com"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap()
            .to_str()
            .unwrap(),
        "https://panel.example.com"
    );
    assert_eq!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
            .unwrap()
            .to_str()
            .unwrap(),
        "true"
    );
}

#[actix_web::test]
async fn test_runtime_cors_admin_normalizes_and_deduplicates_origin_list() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);
    let (token, _) = register_and_login!(app);

    let resp = set_config!(
        app,
        token,
        "cors_allowed_origins",
        serde_json::json!([
            "https://b.example.com/",
            "",
            "https://a.example.com",
            "https://b.example.com"
        ])
    );
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["value"],
        serde_json::json!(["https://a.example.com", "https://b.example.com"])
    );
}

#[actix_web::test]
async fn test_runtime_cors_admin_rejects_origin_with_path() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);
    let (token, _) = register_and_login!(app);

    let resp = set_config!(
        app,
        token,
        "cors_allowed_origins",
        serde_json::json!(["https://app.example.com/path"])
    );
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert!(
        body["msg"]
            .as_str()
            .unwrap()
            .contains("must not include a path")
    );
}

#[actix_web::test]
async fn test_runtime_cors_admin_rejects_mixed_wildcard_and_explicit_origins() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);
    let (token, _) = register_and_login!(app);

    let resp = set_config!(
        app,
        token,
        "cors_allowed_origins",
        serde_json::json!(["*", "https://app.example.com"])
    );
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["msg"].as_str().unwrap().contains("explicit origins"));
}

#[actix_web::test]
async fn test_runtime_cors_wildcard_preflight_returns_star_and_vary_headers() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);
    let (token, _) = register_and_login!(app);
    enable_cors!(app, token);

    let resp = set_config!(app, token, "cors_allowed_origins", serde_json::json!(["*"]));
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri("/health")
        .insert_header((header::ORIGIN, "https://wildcard.example.com"))
        .insert_header((header::ACCESS_CONTROL_REQUEST_METHOD, "GET"))
        .insert_header((
            header::ACCESS_CONTROL_REQUEST_HEADERS,
            "authorization, x-wopi-override, x-wopi-lock",
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 204);
    assert_eq!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap()
            .to_str()
            .unwrap(),
        "*"
    );
    assert!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
            .is_none()
    );
    assert_eq!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_HEADERS)
            .unwrap()
            .to_str()
            .unwrap(),
        EXPECTED_ALLOW_HEADERS
    );
    header_contains(&resp, header::VARY, "Origin");
    header_contains(&resp, header::VARY, "Access-Control-Request-Method");
    header_contains(&resp, header::VARY, "Access-Control-Request-Headers");
}

#[actix_web::test]
async fn test_runtime_cors_disallowed_actual_request_returns_403_with_vary() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);
    let (token, _) = register_and_login!(app);
    enable_cors!(app, token);

    let resp = set_config!(
        app,
        token,
        "cors_allowed_origins",
        serde_json::json!(["https://allowed.example.com"])
    );
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/health")
        .insert_header((header::ORIGIN, "https://blocked.example.com"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
    header_contains(&resp, header::VARY, "Origin");
}

#[actix_web::test]
async fn test_runtime_cors_preflight_rejects_unknown_method() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);
    let (token, _) = register_and_login!(app);
    enable_cors!(app, token);

    let resp = set_config!(
        app,
        token,
        "cors_allowed_origins",
        serde_json::json!(["https://app.example.com"])
    );
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri("/health")
        .insert_header((header::ORIGIN, "https://app.example.com"))
        .insert_header((header::ACCESS_CONTROL_REQUEST_METHOD, "TRACE"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn test_runtime_cors_preflight_rejects_unknown_header() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);
    let (token, _) = register_and_login!(app);
    enable_cors!(app, token);

    let resp = set_config!(
        app,
        token,
        "cors_allowed_origins",
        serde_json::json!(["https://app.example.com"])
    );
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri("/health")
        .insert_header((header::ORIGIN, "https://app.example.com"))
        .insert_header((header::ACCESS_CONTROL_REQUEST_METHOD, "GET"))
        .insert_header((header::ACCESS_CONTROL_REQUEST_HEADERS, "x-custom-header"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn test_runtime_cors_preflight_invalid_request_headers_returns_400() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);
    let (token, _) = register_and_login!(app);
    enable_cors!(app, token);

    let resp = set_config!(
        app,
        token,
        "cors_allowed_origins",
        serde_json::json!(["https://app.example.com"])
    );
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri("/health")
        .insert_header((header::ORIGIN, "https://app.example.com"))
        .insert_header((header::ACCESS_CONTROL_REQUEST_METHOD, "GET"))
        .insert_header((header::ACCESS_CONTROL_REQUEST_HEADERS, "bad header"))
        .to_request();
    let err = test::try_call_service(&app, req).await.unwrap_err();
    let resp = err.error_response();
    assert_eq!(resp.status(), 400);
    let body = to_bytes(resp.into_body()).await.unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert!(
        body["msg"]
            .as_str()
            .unwrap()
            .contains("Access-Control-Request-Headers")
    );
}

#[actix_web::test]
async fn test_runtime_cors_allowed_actual_request_sets_expose_and_vary_headers() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);
    let (token, _) = register_and_login!(app);
    enable_cors!(app, token);

    let resp = set_config!(
        app,
        token,
        "cors_allowed_origins",
        serde_json::json!(["https://panel.example.com"])
    );
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/health")
        .insert_header((header::ORIGIN, "https://panel.example.com"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get(header::ACCESS_CONTROL_EXPOSE_HEADERS)
            .unwrap()
            .to_str()
            .unwrap(),
        EXPECTED_EXPOSE_HEADERS
    );
    header_contains(&resp, header::VARY, "Origin");
}

#[actix_web::test]
async fn test_runtime_cors_invalid_origin_header_returns_400() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);
    let (token, _) = register_and_login!(app);
    enable_cors!(app, token);
    let resp = set_config!(
        app,
        token,
        "cors_allowed_origins",
        serde_json::json!(["https://app.example.com"])
    );
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/health")
        .insert_header((header::ORIGIN, "not-a-valid-origin"))
        .to_request();
    let err = test::try_call_service(&app, req).await.unwrap_err();
    let resp = err.error_response();
    assert_eq!(resp.status(), 400);
    let body = to_bytes(resp.into_body()).await.unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert!(body["msg"].as_str().unwrap().contains("CORS origin"));
}

#[actix_web::test]
async fn test_runtime_cors_wildcard_actual_request_returns_star_and_expose_headers() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);
    let (token, _) = register_and_login!(app);
    enable_cors!(app, token);

    let resp = set_config!(app, token, "cors_allowed_origins", serde_json::json!(["*"]));
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/health")
        .insert_header((header::ORIGIN, "https://any-origin.example.com"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap()
            .to_str()
            .unwrap(),
        "*"
    );
    assert!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
            .is_none()
    );
    assert_eq!(
        resp.headers()
            .get(header::ACCESS_CONTROL_EXPOSE_HEADERS)
            .unwrap()
            .to_str()
            .unwrap(),
        EXPECTED_EXPOSE_HEADERS
    );
}

#[actix_web::test]
async fn test_runtime_cors_preflight_max_age_zero_is_reflected() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);
    let (token, _) = register_and_login!(app);
    enable_cors!(app, token);

    let resp = set_config!(
        app,
        token,
        "cors_allowed_origins",
        serde_json::json!(["https://app.example.com"])
    );
    assert_eq!(resp.status(), 200);
    let resp = set_config!(app, token, "cors_max_age_secs", "0");
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri("/health")
        .insert_header((header::ORIGIN, "https://app.example.com"))
        .insert_header((header::ACCESS_CONTROL_REQUEST_METHOD, "GET"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 204);
    assert_eq!(
        resp.headers()
            .get(header::ACCESS_CONTROL_MAX_AGE)
            .unwrap()
            .to_str()
            .unwrap(),
        "0"
    );
}

#[actix_web::test]
async fn test_runtime_cors_admin_rejects_invalid_boolean_value() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);
    let (token, _) = register_and_login!(app);

    let resp = set_config!(app, token, "cors_allow_credentials", "yes");
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["msg"].as_str().unwrap().contains("true"));
}

#[actix_web::test]
async fn test_runtime_cors_admin_rejects_invalid_max_age_value() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);
    let (token, _) = register_and_login!(app);

    let resp = set_config!(app, token, "cors_max_age_secs", "-1");
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert!(
        body["msg"]
            .as_str()
            .unwrap()
            .contains("non-negative integer")
    );
}

#[actix_web::test]
async fn test_runtime_cors_rejects_setting_wildcard_after_credentials_enabled() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);
    let (token, _) = register_and_login!(app);

    let resp = set_config!(
        app,
        token,
        "cors_allowed_origins",
        serde_json::json!(["https://app.example.com"])
    );
    assert_eq!(resp.status(), 200);
    let resp = set_config!(app, token, "cors_allow_credentials", "true");
    assert_eq!(resp.status(), 200);

    let resp = set_config!(app, token, "cors_allowed_origins", serde_json::json!(["*"]));
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert!(
        body["msg"]
            .as_str()
            .unwrap()
            .contains("cors_allow_credentials cannot be true")
    );
}

#[actix_web::test]
async fn test_runtime_cors_schema_contains_network_defaults() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/config/schema")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let items = body["data"].as_array().unwrap();

    let enabled = items
        .iter()
        .find(|item| item["key"] == "cors_enabled")
        .unwrap();
    assert_eq!(enabled["category"], "network");
    assert!(enabled.get("default_value").is_none());
    assert_eq!(enabled["requires_restart"], false);

    let allowed_origins = items
        .iter()
        .find(|item| item["key"] == "cors_allowed_origins")
        .unwrap();
    assert_eq!(allowed_origins["category"], "network");
    assert_eq!(allowed_origins["value_type"], "string_array");
    assert!(allowed_origins.get("default_value").is_none());
    assert_eq!(allowed_origins["requires_restart"], false);

    let allow_credentials = items
        .iter()
        .find(|item| item["key"] == "cors_allow_credentials")
        .unwrap();
    assert!(allow_credentials.get("default_value").is_none());

    let max_age = items
        .iter()
        .find(|item| item["key"] == "cors_max_age_secs")
        .unwrap();
    assert!(max_age.get("default_value").is_none());
}

#[actix_web::test]
async fn test_runtime_cors_clearing_whitelist_disables_cross_origin_immediately() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);
    let (token, _) = register_and_login!(app);
    enable_cors!(app, token);

    let resp = set_config!(
        app,
        token,
        "cors_allowed_origins",
        serde_json::json!(["https://app.example.com"])
    );
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/health")
        .insert_header((header::ORIGIN, "https://app.example.com"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert!(
        resp.headers()
            .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
    );

    let resp = set_config!(app, token, "cors_allowed_origins", serde_json::json!([]));
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/health")
        .insert_header((header::ORIGIN, "https://app.example.com"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );
}

#[actix_web::test]
async fn test_runtime_cors_disabling_credentials_removes_header_immediately() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);
    let (token, _) = register_and_login!(app);
    enable_cors!(app, token);

    let resp = set_config!(
        app,
        token,
        "cors_allowed_origins",
        serde_json::json!(["https://panel.example.com"])
    );
    assert_eq!(resp.status(), 200);
    let resp = set_config!(app, token, "cors_allow_credentials", "true");
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/health")
        .insert_header((header::ORIGIN, "https://panel.example.com"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
            .unwrap()
            .to_str()
            .unwrap(),
        "true"
    );

    let resp = set_config!(app, token, "cors_allow_credentials", "false");
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/health")
        .insert_header((header::ORIGIN, "https://panel.example.com"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
            .is_none()
    );
}

#[actix_web::test]
async fn test_runtime_cors_no_origin_header_passthrough_has_no_cors_headers() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);

    let req = test::TestRequest::get().uri("/health").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );
    assert!(
        resp.headers()
            .get(header::ACCESS_CONTROL_EXPOSE_HEADERS)
            .is_none()
    );
}

#[actix_web::test]
async fn test_runtime_cors_allows_request_headers_case_insensitively() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);
    let (token, _) = register_and_login!(app);
    enable_cors!(app, token);

    let resp = set_config!(
        app,
        token,
        "cors_allowed_origins",
        serde_json::json!(["HTTPS://APP.EXAMPLE.COM/"])
    );
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["value"],
        serde_json::json!(["https://app.example.com"])
    );

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri("/health")
        .insert_header((header::ORIGIN, "https://app.example.com"))
        .insert_header((header::ACCESS_CONTROL_REQUEST_METHOD, "GET"))
        .insert_header((
            header::ACCESS_CONTROL_REQUEST_HEADERS,
            "Authorization, Content-Type",
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 204);
}

#[actix_web::test]
async fn test_runtime_cors_enabled_without_whitelist_passthrough_has_no_cors_headers() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);
    let (token, _) = register_and_login!(app);
    enable_cors!(app, token);

    let req = test::TestRequest::get()
        .uri("/health")
        .insert_header((header::ORIGIN, "https://app.example.com"))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 200);
    assert!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );
    assert!(
        resp.headers()
            .get(header::ACCESS_CONTROL_EXPOSE_HEADERS)
            .is_none()
    );
}

#[actix_web::test]
async fn test_runtime_cors_public_site_url_does_not_bypass_empty_whitelist() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);
    let (token, _) = register_and_login!(app);
    enable_cors!(app, token);

    let resp = set_config!(
        app,
        token,
        "public_site_url",
        serde_json::json!(["https://drive.example.com"])
    );
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/health")
        .insert_header((header::HOST, "internal.example.local"))
        .insert_header((header::ORIGIN, "https://drive.example.com"))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 200);
    assert!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );
}

#[actix_web::test]
async fn test_runtime_cors_public_site_url_still_requires_whitelist_match() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);
    let (token, _) = register_and_login!(app);
    enable_cors!(app, token);

    let resp = set_config!(
        app,
        token,
        "public_site_url",
        serde_json::json!(["https://drive.example.com"])
    );
    assert_eq!(resp.status(), 200);
    let resp = set_config!(
        app,
        token,
        "cors_allowed_origins",
        serde_json::json!(["https://api.example.com"])
    );
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/health")
        .insert_header((header::HOST, "internal.example.local"))
        .insert_header((header::ORIGIN, "https://drive.example.com"))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 403);
    header_contains(&resp, header::VARY, "Origin");
}

#[actix_web::test]
async fn test_runtime_cors_public_site_url_is_not_added_to_passthrough_response() {
    let state = common::setup().await;
    let app = create_test_app_with_cors!(state);
    let (token, _) = register_and_login!(app);
    enable_cors!(app, token);

    let resp = set_config!(
        app,
        token,
        "public_site_url",
        serde_json::json!(["https://drive.example.com"])
    );
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get().uri("/health").to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 200);
    assert!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );
}

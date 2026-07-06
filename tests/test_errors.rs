//! 集成测试：`errors`。

use actix_web::{App, HttpResponse, ResponseError, body::to_bytes, http::StatusCode, web};
use aster_drive::errors::AsterError;
use serde::Deserialize;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::{
    layer::{Context, Layer},
    prelude::*,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecordedEvent {
    level: Level,
    message: Option<String>,
}

#[derive(Clone, Default)]
struct EventRecorder {
    events: Arc<Mutex<Vec<RecordedEvent>>>,
}

#[derive(Default)]
struct MessageVisitor {
    message: Option<String>,
}

impl tracing::field::Visit for MessageVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{value:?}").trim_matches('"').to_string());
        }
    }
}

impl<S> Layer<S> for EventRecorder
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        self.events.lock().unwrap().push(RecordedEvent {
            level: *event.metadata().level(),
            message: visitor.message,
        });
    }
}

fn capture_events(f: impl FnOnce()) -> Vec<RecordedEvent> {
    let recorder = EventRecorder::default();
    let subscriber = tracing_subscriber::registry().with(recorder.clone());

    tracing::subscriber::with_default(subscriber, f);

    recorder.events.lock().unwrap().clone()
}

async fn response_body_json(resp: actix_web::HttpResponse) -> Value {
    let body = to_bytes(resp.into_body()).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

#[test]
fn storage_quota_exceeded_logs_warn_for_507() {
    let err = AsterError::storage_quota_exceeded("quota 1024, used 1000, need 100");

    let events = capture_events(|| {
        let resp = err.error_response();
        assert_eq!(resp.status(), StatusCode::INSUFFICIENT_STORAGE);
    });

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].level, Level::WARN);
    assert_eq!(events[0].message.as_deref(), Some("request error"));
}

#[test]
fn internal_error_logs_error() {
    let err = AsterError::internal_error("db pool poisoned");

    let events = capture_events(|| {
        let resp = err.error_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    });

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].level, Level::ERROR);
    assert_eq!(events[0].message.as_deref(), Some("server error"));
}

#[test]
fn unauthorized_error_skips_logging() {
    let err = AsterError::auth_token_invalid("invalid token");

    let events = capture_events(|| {
        let resp = err.error_response();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    });

    assert!(events.is_empty());
}

#[test]
fn validation_error_logs_warn() {
    let err = AsterError::validation_error("file name is invalid");

    let events = capture_events(|| {
        let resp = err.error_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    });

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].level, Level::WARN);
    assert_eq!(events[0].message.as_deref(), Some("request error"));
}

#[actix_web::test]
async fn internal_error_redacts_response_message() {
    let err = AsterError::internal_error("db pool poisoned");

    let resp = err.error_response();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let body = response_body_json(resp).await;
    assert_eq!(body["msg"], "Internal Server Error");
}

#[actix_web::test]
async fn storage_driver_error_redacts_response_message() {
    let err = AsterError::storage_driver_error("read file: /tmp/private/secret.txt");

    let resp = err.error_response();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let body = response_body_json(resp).await;
    assert_eq!(body["msg"], "Storage Driver Error");
}

#[actix_web::test]
async fn storage_quota_exceeded_keeps_response_message() {
    let err = AsterError::storage_quota_exceeded("quota 1024, used 1000, need 100");

    let resp = err.error_response();
    assert_eq!(resp.status(), StatusCode::INSUFFICIENT_STORAGE);

    let body = response_body_json(resp).await;
    assert_eq!(body["msg"], "quota 1024, used 1000, need 100");
}

#[derive(Deserialize)]
struct JsonExtractorRequiredFieldReq {
    client_id: String,
}

async fn json_extractor_required_field_handler(
    body: web::Json<JsonExtractorRequiredFieldReq>,
) -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({ "client_id": body.client_id }))
}

async fn json_extractor_echo_handler(body: web::Json<Value>) -> HttpResponse {
    HttpResponse::Ok().json(body.into_inner())
}

async fn assert_json_extractor_error(
    resp: actix_web::dev::ServiceResponse,
    status: StatusCode,
    code: &str,
    msg_contains: &str,
) {
    assert_eq!(resp.status(), status);
    let body: Value = actix_web::test::read_body_json(resp).await;
    assert_eq!(body["code"], code);
    assert_eq!(body["data"], Value::Null);
    assert_eq!(body["error"]["retryable"], false);
    assert!(
        body["msg"]
            .as_str()
            .is_some_and(|msg| msg.contains(msg_contains)),
        "unexpected response body: {body:?}"
    );
}

#[actix_web::test]
async fn json_deserialize_error_uses_aster_error_envelope() {
    let app = actix_web::test::init_service(
        App::new()
            .app_data(aster_drive::api::extractors::json_config(1024))
            .route(
                "/requires-client-id",
                web::post().to(json_extractor_required_field_handler),
            ),
    )
    .await;

    let req = actix_web::test::TestRequest::post()
        .uri("/requires-client-id")
        .insert_header(("Content-Type", "application/json"))
        .set_payload(r#"{"redirect_uri":"https://example.test/callback"}"#)
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;

    assert_json_extractor_error(resp, StatusCode::BAD_REQUEST, "bad_request", "client_id").await;
}

#[actix_web::test]
async fn malformed_json_uses_aster_error_envelope() {
    let app = actix_web::test::init_service(
        App::new()
            .app_data(aster_drive::api::extractors::json_config(1024))
            .route("/echo", web::post().to(json_extractor_echo_handler)),
    )
    .await;

    let req = actix_web::test::TestRequest::post()
        .uri("/echo")
        .insert_header(("Content-Type", "application/json"))
        .set_payload("{")
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;

    assert_json_extractor_error(
        resp,
        StatusCode::BAD_REQUEST,
        "bad_request",
        "invalid JSON request body",
    )
    .await;
}

#[actix_web::test]
async fn json_content_type_error_uses_aster_error_envelope() {
    let app = actix_web::test::init_service(
        App::new()
            .app_data(aster_drive::api::extractors::json_config(1024))
            .route("/echo", web::post().to(json_extractor_echo_handler)),
    )
    .await;

    let req = actix_web::test::TestRequest::post()
        .uri("/echo")
        .insert_header(("Content-Type", "text/plain"))
        .set_payload("{}")
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;

    assert_json_extractor_error(
        resp,
        StatusCode::BAD_REQUEST,
        "bad_request",
        "application/json",
    )
    .await;
}

#[actix_web::test]
async fn json_payload_at_limit_is_accepted() {
    let app = actix_web::test::init_service(
        App::new()
            .app_data(aster_drive::api::extractors::json_config(2))
            .route("/echo", web::post().to(json_extractor_echo_handler)),
    )
    .await;

    let req = actix_web::test::TestRequest::post()
        .uri("/echo")
        .insert_header(("Content-Type", "application/json"))
        .set_payload("{}")
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = actix_web::test::read_body_json(resp).await;
    assert_eq!(body, serde_json::json!({}));
}

#[actix_web::test]
async fn json_payload_over_limit_uses_aster_error_envelope() {
    let app = actix_web::test::init_service(
        App::new()
            .app_data(aster_drive::api::extractors::json_config(2))
            .route("/echo", web::post().to(json_extractor_echo_handler)),
    )
    .await;

    let req = actix_web::test::TestRequest::post()
        .uri("/echo")
        .insert_header(("Content-Type", "application/json"))
        .set_payload(r#"{"a":1}"#)
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;

    assert_json_extractor_error(
        resp,
        StatusCode::PAYLOAD_TOO_LARGE,
        "file.too_large",
        "JSON payload is too large",
    )
    .await;
}

#[actix_web::test]
async fn json_payload_stream_overflow_uses_aster_error_envelope() {
    let app = actix_web::test::init_service(
        App::new()
            .app_data(aster_drive::api::extractors::json_config(
                aster_drive::api::extractors::DEFAULT_JSON_LIMIT,
            ))
            .route("/echo", web::post().to(json_extractor_echo_handler)),
    )
    .await;

    let (mut sender, payload) = actix_http::h1::Payload::create(false);
    sender.set_error(actix_web::error::PayloadError::Overflow);

    let req = actix_web::test::TestRequest::post()
        .uri("/echo")
        .insert_header(("Content-Type", "application/json"))
        .to_request();
    let (req, _) = req.replace_payload(actix_http::Payload::from(payload));
    let resp = actix_web::test::call_service(&app, req).await;

    assert_json_extractor_error(
        resp,
        StatusCode::PAYLOAD_TOO_LARGE,
        "file.too_large",
        "configured size limit",
    )
    .await;
}

//! 集成测试：`auth`。

#[macro_use]
mod common;

use actix_web::body::{MessageBody, to_bytes};
use actix_web::cookie::SameSite;
use actix_web::test;
use aster_drive::api::api_error_code::ApiErrorCode;
use aster_drive::api::pagination::{AdminAuditLogSortBy, SortOrder};
use aster_drive::config::branding::DEFAULT_BRANDING_TITLE;
use aster_drive::db::repository::{audit_log_repo, auth_session_repo, passkey_repo, user_repo};
use aster_drive::entities::passkey;
use aster_drive::runtime::SharedRuntimeState;
use aster_drive::services::auth::local;
use aster_drive::types::{AuditAction, UserStatus};
use base64::Engine as _;
use serde_json::Value;
use std::io::Cursor;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use webauthn_authenticator_rs::prelude::{Url, WebauthnAuthenticator};
use webauthn_authenticator_rs::softpasskey::SoftPasskey;
use webauthn_rs::prelude::{CreationChallengeResponse, RequestChallengeResponse};
use webauthn_rs_proto::{AllowCredentials, Mediation, ResidentKeyRequirement};

const TEST_BROWSER_ORIGIN: &str = "http://localhost:8080";
const TEST_PUBLIC_SITE_ORIGIN: &str = "https://pan.esaps.net";
const ONE_SECOND_WINDOW_ELAPSED: Duration = Duration::from_millis(1100);

struct RefreshHookGuard {
    _hook: local::test_support::RefreshRotationTestHook,
}

impl RefreshHookGuard {
    fn new(hook: local::test_support::RefreshRotationTestHook) -> Self {
        Self { _hook: hook }
    }
}

impl Drop for RefreshHookGuard {
    fn drop(&mut self) {
        tokio::spawn(async {
            local::test_support::clear_refresh_rotation_test_hook().await;
        });
    }
}

macro_rules! login_user_with_credentials {
    ($app:expr, $identifier:expr, $password:expr) => {{
        let req = test::TestRequest::post()
            .uri("/api/v1/auth/login")
            .peer_addr("127.0.0.1:12345".parse().unwrap())
            .set_json(serde_json::json!({
                "identifier": $identifier,
                "password": $password
            }))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 200);
        common::extract_cookie(&resp, "aster_access").unwrap()
    }};
}

macro_rules! login_user_with_auth_cookies {
    ($app:expr, $identifier:expr, $password:expr) => {{
        let req = test::TestRequest::post()
            .uri("/api/v1/auth/login")
            .peer_addr("127.0.0.1:12345".parse().unwrap())
            .set_json(serde_json::json!({
                "identifier": $identifier,
                "password": $password
            }))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 200);
        let access =
            common::extract_cookie(&resp, "aster_access").expect("access cookie missing");
        let refresh =
            common::extract_cookie(&resp, "aster_refresh").expect("refresh cookie missing");
        (access, refresh)
    }};
}

macro_rules! admin_create_user_with_credentials {
    ($app:expr, $admin_token:expr, $username:expr, $email:expr, $password:expr) => {{
        let req = test::TestRequest::post()
            .uri("/api/v1/admin/users")
            .insert_header(("Cookie", common::access_cookie_header(&$admin_token)))
            .insert_header(common::csrf_header_for(&$admin_token))
            .peer_addr("127.0.0.1:12345".parse().unwrap())
            .set_json(serde_json::json!({
                "username": $username,
                "email": $email,
                "password": $password
            }))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 201);
        let body: Value = test::read_body_json(resp).await;
        body["data"]["user"]["id"].as_i64().unwrap()
    }};
}

macro_rules! team_upload_request {
    ($team_id:expr, $token:expr, $filename:expr, $content:expr $(,)?) => {{
        let boundary = "----TeamStorageEventBoundary";
        let payload = format!(
            "------TeamStorageEventBoundary\r\n\
             Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n\
             Content-Type: text/plain\r\n\r\n\
             {content}\r\n\
             ------TeamStorageEventBoundary--\r\n",
            filename = $filename,
            content = $content,
        );

        test::TestRequest::post()
            .uri(&format!("/api/v1/teams/{}/files/upload", $team_id))
            .insert_header(("Cookie", common::access_cookie_header(&$token)))
            .insert_header(common::csrf_header_for(&$token))
            .insert_header((
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            ))
            .set_payload(payload)
            .to_request()
    }};
}

fn avatar_upload_payload() -> (String, Vec<u8>) {
    let boundary = "----AsterAvatarBoundary".to_string();
    let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        8,
        8,
        image::Rgba([255, 120, 0, 255]),
    ));
    let mut png = Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();

    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"file\"; filename=\"avatar.png\"\r\n\
             Content-Type: image/png\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(&png.into_inner());
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (boundary, body)
}

fn extract_verification_token(
    message: &aster_drive::services::mail::sender::MailMessage,
) -> String {
    common::extract_token_from_mail_message(
        message,
        "/api/v1/auth/contact-verification/confirm?token=",
    )
    .expect("verification link missing from mail body")
}

fn extract_password_reset_token(
    message: &aster_drive::services::mail::sender::MailMessage,
) -> String {
    common::extract_token_from_mail_message(message, "/reset-password?token=")
        .expect("password reset link missing from mail body")
}

async fn read_next_sse_json<B>(body: &mut B) -> Value
where
    B: MessageBody + Unpin,
    B::Error: std::fmt::Debug,
{
    for _ in 0..4 {
        let frame = tokio::time::timeout(
            Duration::from_secs(2),
            std::future::poll_fn(|cx| std::pin::Pin::new(&mut *body).poll_next(cx)),
        )
        .await
        .expect("timed out waiting for SSE frame")
        .expect("SSE stream ended unexpectedly")
        .expect("SSE body chunk should not fail");

        let text = std::str::from_utf8(&frame).expect("SSE frame should be utf-8");
        for chunk in text.split("\n\n") {
            if let Some(json) = chunk.strip_prefix("data: ") {
                return serde_json::from_str(json).expect("SSE data should be valid JSON");
            }
        }
    }

    panic!("did not receive SSE data frame");
}

async fn read_next_sse_json_with_timeout<B>(body: &mut B, timeout: Duration) -> Option<Value>
where
    B: MessageBody + Unpin,
    B::Error: std::fmt::Debug,
{
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return None;
        }

        let frame = match tokio::time::timeout(
            remaining,
            std::future::poll_fn(|cx| std::pin::Pin::new(&mut *body).poll_next(cx)),
        )
        .await
        {
            Ok(frame) => frame
                .expect("SSE stream ended unexpectedly")
                .expect("SSE body chunk should not fail"),
            Err(_) => return None,
        };

        let text = std::str::from_utf8(&frame).expect("SSE frame should be utf-8");
        for chunk in text.split("\n\n") {
            if let Some(json) = chunk.strip_prefix("data: ") {
                return Some(serde_json::from_str(json).expect("SSE data should be valid JSON"));
            }
        }
    }
}

async fn expect_sse_stream_end<B>(body: &mut B)
where
    B: MessageBody + Unpin,
    B::Error: std::fmt::Debug,
{
    let frame = tokio::time::timeout(
        Duration::from_secs(2),
        std::future::poll_fn(|cx| std::pin::Pin::new(&mut *body).poll_next(cx)),
    )
    .await
    .expect("timed out waiting for SSE stream to close");

    assert!(
        frame.is_none(),
        "SSE stream should close after auth revalidation fails"
    );
}

async fn service_response_json<B>(resp: actix_web::dev::ServiceResponse<B>) -> Value
where
    B: MessageBody,
    B::Error: std::fmt::Debug,
{
    let body = to_bytes(resp.into_body()).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn http_response_json(resp: actix_web::HttpResponse) -> Value {
    let body = to_bytes(resp.into_body()).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn configure_passkey_public_site_url(state: &aster_drive::runtime::PrimaryAppState) {
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::site_url::PUBLIC_SITE_URL_KEY,
        r#"["http://localhost:8080"]"#,
    ));
}

fn configure_local_email_policy(
    state: &aster_drive::runtime::PrimaryAppState,
    allowlist: &[&str],
    blocklist: &[&str],
) {
    let allowlist = serde_json::to_string(allowlist).expect("allowlist should serialize");
    let blocklist = serde_json::to_string(blocklist).expect("blocklist should serialize");
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::local_email_policy::AUTH_LOCAL_EMAIL_ALLOWLIST_KEY,
        &allowlist,
    ));
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::local_email_policy::AUTH_LOCAL_EMAIL_BLOCKLIST_KEY,
        &blocklist,
    ));
}

async fn register_test_passkey<S, B, E>(
    app: &S,
    access_token: &str,
    name: &str,
) -> (SoftPasskey, Value)
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = E,
        >,
    B: MessageBody,
    E: std::fmt::Debug,
{
    let origin = Url::parse(TEST_BROWSER_ORIGIN).unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/passkeys/register/start")
        .insert_header(("Cookie", common::access_cookie_header(access_token)))
        .insert_header(common::csrf_header_for(access_token))
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({ "name": name }))
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 200);
    let start_body: Value = test::read_body_json(resp).await;
    let flow_id = start_body["data"]["flow_id"].as_str().unwrap().to_string();
    let mut challenge = serde_json::from_value::<CreationChallengeResponse>(
        start_body["data"]["public_key"].clone(),
    )
    .expect("registration challenge should deserialize");
    let selection = challenge
        .public_key
        .authenticator_selection
        .as_ref()
        .expect("registration should include authenticator selection");
    assert_eq!(
        selection.resident_key,
        Some(ResidentKeyRequirement::Required)
    );
    assert!(selection.require_resident_key);

    let selection = challenge
        .public_key
        .authenticator_selection
        .as_mut()
        .expect("registration should include authenticator selection");
    selection.resident_key = Some(ResidentKeyRequirement::Discouraged);
    selection.require_resident_key = false;

    let mut softpasskey = SoftPasskey::new(true);
    let credential = softpasskey
        .do_registration(origin, challenge)
        .expect("soft passkey registration should succeed");
    let credential = serde_json::to_value(credential).expect("credential should serialize");

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/passkeys/register/finish")
        .insert_header(("Cookie", common::access_cookie_header(access_token)))
        .insert_header(common::csrf_header_for(access_token))
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "flow_id": flow_id,
            "credential": credential,
        }))
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 200);
    let passkey_body: Value = test::read_body_json(resp).await;

    (softpasskey, passkey_body["data"].clone())
}

async fn passkey_login_start<S, B, E>(
    app: &S,
    identifier: Option<&str>,
) -> (String, RequestChallengeResponse)
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = E,
        >,
    B: MessageBody,
    E: std::fmt::Debug,
{
    let payload = match identifier {
        Some(identifier) => serde_json::json!({ "identifier": identifier }),
        None => serde_json::json!({}),
    };
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/passkeys/login/start")
        .insert_header(("Origin", TEST_BROWSER_ORIGIN))
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(payload)
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 200);
    let start_body: Value = test::read_body_json(resp).await;
    let flow_id = start_body["data"]["flow_id"].as_str().unwrap().to_string();
    let challenge = serde_json::from_value::<RequestChallengeResponse>(
        start_body["data"]["public_key"].clone(),
    )
    .expect("login challenge should deserialize");
    (flow_id, challenge)
}

async fn conditional_passkey_login_start<S, B, E>(app: &S) -> (String, RequestChallengeResponse)
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = E,
        >,
    B: MessageBody,
    E: std::fmt::Debug,
{
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/passkeys/login/start")
        .insert_header(("Origin", TEST_BROWSER_ORIGIN))
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({ "conditional": true }))
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 200);
    let start_body: Value = test::read_body_json(resp).await;
    let flow_id = start_body["data"]["flow_id"].as_str().unwrap().to_string();
    let challenge = serde_json::from_value::<RequestChallengeResponse>(
        start_body["data"]["public_key"].clone(),
    )
    .expect("conditional login challenge should deserialize");
    (flow_id, challenge)
}

async fn passkey_login_finish<S, B, E>(
    app: &S,
    flow_id: &str,
    credential: Value,
) -> actix_web::dev::ServiceResponse<B>
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = E,
        >,
    B: MessageBody,
    E: std::fmt::Debug,
{
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/passkeys/login/finish")
        .insert_header(("User-Agent", "AsterDrive Passkey Test/1.0"))
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "flow_id": flow_id,
            "credential": credential,
        }))
        .to_request();
    test::call_service(app, req).await
}

fn allow_test_passkey_credential(
    mut challenge: RequestChallengeResponse,
    stored_passkey: &passkey::Model,
) -> (RequestChallengeResponse, uuid::Uuid) {
    let credential_id = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(stored_passkey.credential_id.as_bytes())
        .unwrap();
    let user_handle = uuid::Uuid::parse_str(&stored_passkey.user_handle).unwrap();
    challenge.public_key.allow_credentials = vec![AllowCredentials {
        type_: "public-key".to_string(),
        id: credential_id,
        transports: None,
    }];
    (challenge, user_handle)
}

async fn testuser_id<C: sea_orm::ConnectionTrait>(db: &C) -> i64 {
    user_repo::find_by_username(db, "testuser")
        .await
        .unwrap()
        .expect("testuser should exist")
        .id
}

#[actix_web::test]
async fn test_register_and_login() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    // 注册
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "alice",
            "email": "alice@example.com",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "success");
    assert_eq!(body["data"]["username"], "alice");
    // password_hash 不应该暴露
    assert!(body["data"]["password_hash"].is_null());

    // 重复注册应失败
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "alice",
            "email": "alice2@example.com",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    // 登录
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .insert_header(("User-Agent", "AsterDrive Test Browser/1.0"))
        .set_json(serde_json::json!({
            "identifier": "alice",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let access = common::extract_cookie(&resp, "aster_access");
    let refresh = common::extract_cookie(&resp, "aster_refresh");
    let access_cookie_path = resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "aster_access")
        .expect("access cookie missing")
        .path()
        .map(str::to_string);
    let access_cookie_same_site = resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "aster_access")
        .expect("access cookie missing")
        .same_site();
    let refresh_cookie_path = resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "aster_refresh")
        .expect("refresh cookie missing")
        .path()
        .map(str::to_string);
    let refresh_cookie_same_site = resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "aster_refresh")
        .expect("refresh cookie missing")
        .same_site();
    let csrf = common::extract_cookie(&resp, "aster_csrf");
    let csrf_cookie_path = resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "aster_csrf")
        .expect("csrf cookie missing")
        .path()
        .map(str::to_string);
    let csrf_cookie_same_site = resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "aster_csrf")
        .expect("csrf cookie missing")
        .same_site();
    let csrf_cookie_http_only = resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "aster_csrf")
        .expect("csrf cookie missing")
        .http_only();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "success");
    assert_eq!(body["data"]["expires_in"], 900);
    // tokens 在 cookie 里
    assert!(access.is_some());
    assert!(refresh.is_some());
    assert!(csrf.is_some());
    assert_eq!(access_cookie_path.as_deref(), Some("/"));
    assert_eq!(access_cookie_same_site, Some(SameSite::Lax));
    assert_eq!(refresh_cookie_path.as_deref(), Some("/api/v1/auth"));
    assert_eq!(refresh_cookie_same_site, Some(SameSite::Lax));
    assert_eq!(csrf_cookie_path.as_deref(), Some("/"));
    assert_eq!(csrf_cookie_same_site, Some(SameSite::Lax));
    assert_ne!(csrf_cookie_http_only, Some(true));
    let refresh_claims = local::verify_token(
        refresh.as_deref().expect("refresh cookie should exist"),
        &state.config.auth.jwt_secret,
    )
    .expect("refresh token should verify");
    let refresh_jti = refresh_claims.jti.expect("refresh token should carry jti");
    let auth_session = auth_session_repo::find_by_refresh_jti(state.writer_db(), &refresh_jti)
        .await
        .unwrap()
        .expect("auth session row should exist");
    assert_eq!(
        auth_session.user_agent.as_deref(),
        Some("AsterDrive Test Browser/1.0")
    );
    assert!(auth_session.last_seen_at >= auth_session.created_at);

    // 错误密码
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "alice",
            "password": "wrongpassword"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn test_login_rejects_untrusted_origin() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "alice",
            "email": "alice@example.com",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .insert_header(("Origin", "https://evil.example.com"))
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "alice",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn test_login_uses_generic_invalid_credentials_message() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "ghost-user",
            "password": "wrongpassword"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "auth.credentials_failed");
    assert_eq!(body["msg"], "Invalid Credentials");
    assert_eq!(body["error"]["retryable"], false);
    assert!(body["error"].get("code").is_none());
    assert!(
        body["msg"]
            .as_str()
            .is_some_and(|msg| !msg.contains("user not found") && !msg.contains("wrong password"))
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "alice",
            "email": "alice@example.com",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "alice",
            "password": "wrongpassword"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "auth.credentials_failed");
    assert_eq!(body["msg"], "Invalid Credentials");
    assert_eq!(body["error"]["retryable"], false);
    assert!(body["error"].get("code").is_none());

    {
        use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};

        let user = user_repo::find_by_username(state.writer_db(), "alice")
            .await
            .unwrap()
            .unwrap();
        let mut active = user.into_active_model();
        active.status = Set(UserStatus::Disabled);
        active.update(state.writer_db()).await.unwrap();
    }

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "alice",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "auth.credentials_failed");
    assert_eq!(body["msg"], "Invalid Credentials");
    assert_eq!(body["error"]["retryable"], false);
    assert!(body["error"].get("code").is_none());
}

#[actix_web::test]
async fn test_cookie_authenticated_write_rejects_same_site_request_source() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (access, _) = register_and_login!(app);

    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/profile")
        .insert_header(("Cookie", common::access_cookie_header(&access)))
        .insert_header(common::csrf_header_for(&access))
        .insert_header(("Sec-Fetch-Site", "same-site"))
        .set_json(serde_json::json!({
            "display_name": "Evil Mirror"
        }))
        .to_request();
    assert_service_status!(app, req, 403);
}

#[actix_web::test]
async fn test_cookie_authenticated_write_accepts_trusted_same_site_public_origin() {
    let state = common::setup().await;
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::site_url::PUBLIC_SITE_URL_KEY,
        r#"["https://api.example.com","https://panel.example.com"]"#,
    ));
    let app = create_test_app!(state);

    let (access, _) = register_and_login!(app);

    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/profile")
        .insert_header(("Cookie", common::access_cookie_header(&access)))
        .insert_header(common::csrf_header_for(&access))
        .insert_header(("Origin", "https://panel.example.com"))
        .insert_header(("Sec-Fetch-Site", "same-site"))
        .set_json(serde_json::json!({
            "display_name": "Trusted Panel"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["display_name"], "Trusted Panel");
}

#[actix_web::test]
async fn test_cookie_authenticated_write_rejects_fetch_metadata_none_request_source() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (access, _) = register_and_login!(app);

    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/profile")
        .insert_header(("Cookie", common::access_cookie_header(&access)))
        .insert_header(common::csrf_header_for(&access))
        .insert_header(("Sec-Fetch-Site", "none"))
        .set_json(serde_json::json!({
            "display_name": "Manual Navigation?"
        }))
        .to_request();
    assert_service_status!(app, req, 403);
}

#[actix_web::test]
async fn test_cookie_authenticated_write_requires_csrf_token_for_same_origin_browser_request() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (access, _) = register_and_login!(app);

    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/profile")
        .insert_header(("Origin", TEST_BROWSER_ORIGIN))
        .insert_header(("Cookie", common::access_cookie_header(&access)))
        .set_json(serde_json::json!({
            "display_name": "Missing Token"
        }))
        .to_request();
    assert_service_status!(app, req, 403);
}

#[actix_web::test]
async fn test_cookie_authenticated_write_accepts_matching_csrf_token() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "alice",
            "email": "alice@example.com",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "alice",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let access = common::extract_cookie(&resp, "aster_access").expect("access cookie missing");
    let csrf = common::extract_cookie(&resp, "aster_csrf").expect("csrf cookie missing");

    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/profile")
        .insert_header(("Origin", TEST_BROWSER_ORIGIN))
        .insert_header(("X-CSRF-Token", csrf.clone()))
        .insert_header((
            "Cookie",
            format!("aster_access={access}; aster_csrf={csrf}"),
        ))
        .set_json(serde_json::json!({
            "display_name": "Browser Path"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["display_name"], "Browser Path");
}

#[actix_web::test]
async fn test_refresh_rejects_untrusted_origin() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (_, refresh) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("Cookie", common::refresh_cookie_header(&refresh)))
        .insert_header(common::csrf_header_for(&refresh))
        .insert_header(("Origin", "https://evil.example.com"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn test_refresh_accepts_matching_csrf_token_and_rotates_cookie() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "alice",
            "email": "alice@example.com",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .insert_header(("User-Agent", "AsterDrive Login Agent/1.0"))
        .set_json(serde_json::json!({
            "identifier": "alice",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let refresh = common::extract_cookie(&resp, "aster_refresh").expect("refresh cookie missing");
    let csrf = common::extract_cookie(&resp, "aster_csrf").expect("csrf cookie missing");

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("User-Agent", "AsterDrive Refresh Agent/1.0"))
        .insert_header(("Origin", TEST_BROWSER_ORIGIN))
        .insert_header(("X-CSRF-Token", csrf.clone()))
        .insert_header((
            "Cookie",
            format!("aster_refresh={refresh}; aster_csrf={csrf}"),
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let rotated_refresh =
        common::extract_cookie(&resp, "aster_refresh").expect("rotated refresh cookie missing");
    let rotated_csrf =
        common::extract_cookie(&resp, "aster_csrf").expect("rotated csrf cookie missing");
    assert_ne!(rotated_refresh, refresh);
    assert_ne!(rotated_csrf, csrf);
    let rotated_claims = local::verify_token(&rotated_refresh, &state.config.auth.jwt_secret)
        .expect("rotated refresh token should verify");
    let rotated_jti = rotated_claims
        .jti
        .clone()
        .expect("rotated refresh token should carry jti");
    let rotated_session = auth_session_repo::find_by_refresh_jti(state.writer_db(), &rotated_jti)
        .await
        .unwrap()
        .expect("rotated auth session should exist");
    assert_eq!(
        rotated_session.user_agent.as_deref(),
        Some("AsterDrive Refresh Agent/1.0")
    );
    assert!(rotated_session.last_seen_at >= rotated_session.created_at);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("User-Agent", "AsterDrive Refresh Agent/2.0"))
        .insert_header(("Origin", TEST_BROWSER_ORIGIN))
        .insert_header(("X-CSRF-Token", rotated_csrf.clone()))
        .insert_header((
            "Cookie",
            format!("aster_refresh={rotated_refresh}; aster_csrf={rotated_csrf}"),
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let next_refresh =
        common::extract_cookie(&resp, "aster_refresh").expect("next refresh cookie missing");
    assert_ne!(next_refresh, rotated_refresh);
    let next_claims = local::verify_token(&next_refresh, &state.config.auth.jwt_secret)
        .expect("next refresh token should verify");
    let next_jti = next_claims
        .jti
        .clone()
        .expect("next refresh token should carry jti");
    let next_session = auth_session_repo::find_by_refresh_jti(state.writer_db(), &next_jti)
        .await
        .unwrap()
        .expect("next auth session should exist");
    assert_eq!(
        next_session.user_agent.as_deref(),
        Some("AsterDrive Refresh Agent/2.0")
    );
    assert!(next_session.last_seen_at >= next_session.created_at);
}

#[actix_web::test]
async fn test_refresh_rotation_isolated_across_devices() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (_, refresh_a) = register_and_login!(app);
    let (_, refresh_b) = login_user_with_auth_cookies!(app, "testuser", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("Cookie", common::refresh_cookie_header(&refresh_a)))
        .insert_header(common::csrf_header_for(&refresh_a))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let rotated_a = common::extract_cookie(&resp, "aster_refresh").unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("Cookie", common::refresh_cookie_header(&refresh_b)))
        .insert_header(common::csrf_header_for(&refresh_b))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let rotated_b = common::extract_cookie(&resp, "aster_refresh").unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("Cookie", common::refresh_cookie_header(&rotated_a)))
        .insert_header(common::csrf_header_for(&rotated_a))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let next_rotated_a = common::extract_cookie(&resp, "aster_refresh").unwrap();

    assert_ne!(rotated_a, rotated_b);
    assert_ne!(next_rotated_a, rotated_a);
}

#[actix_web::test]
async fn test_refresh_reuse_detected_revokes_all_sessions() {
    use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};

    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    let (_, refresh_a) = register_and_login!(app);
    let (access_b, refresh_b) = login_user_with_auth_cookies!(app, "testuser", "password123");

    let original_claims = local::verify_token(&refresh_a, &state.config.auth.jwt_secret)
        .expect("refresh token should verify");
    let original_jti = original_claims
        .jti
        .clone()
        .expect("refresh token should carry jti");
    assert!(
        auth_session_repo::find_by_refresh_jti(state.writer_db(), &original_jti)
            .await
            .unwrap()
            .is_some()
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("Cookie", common::refresh_cookie_header(&refresh_a)))
        .insert_header(common::csrf_header_for(&refresh_a))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let rotated_a = common::extract_cookie(&resp, "aster_refresh").unwrap();
    let rotated_claims = local::verify_token(&rotated_a, &state.config.auth.jwt_secret)
        .expect("rotated refresh should verify");
    let rotated_jti = rotated_claims
        .jti
        .clone()
        .expect("rotated refresh token should carry jti");
    assert!(
        auth_session_repo::find_by_refresh_jti(state.writer_db(), &original_jti)
            .await
            .unwrap()
            .is_none()
    );
    let rotated_session = auth_session_repo::find_by_refresh_jti(state.writer_db(), &rotated_jti)
        .await
        .unwrap()
        .expect("rotated auth session should exist");
    let mut active = rotated_session.into_active_model();
    active.last_seen_at = Set(chrono::Utc::now() - chrono::Duration::seconds(60));
    active.update(state.writer_db()).await.unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("Cookie", common::refresh_cookie_header(&refresh_a)))
        .insert_header(common::csrf_header_for(&refresh_a))
        .to_request();
    assert_service_status!(app, req, 401, "refresh token reuse should be rejected");

    let user = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .expect("user should exist");
    assert_eq!(user.session_version, 2);
    assert!(
        auth_session_repo::list_active_for_user(state.writer_db(), user.id)
            .await
            .unwrap()
            .is_empty()
    );

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&access_b)))
        .insert_header(common::csrf_header_for(&access_b))
        .to_request();
    assert_service_status!(
        app,
        req,
        401,
        "reuse should revoke access tokens on all devices"
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("Cookie", common::refresh_cookie_header(&refresh_b)))
        .insert_header(common::csrf_header_for(&refresh_b))
        .to_request();
    assert_service_status!(
        app,
        req,
        401,
        "reuse should revoke sibling device refresh tokens"
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("Cookie", common::refresh_cookie_header(&rotated_a)))
        .insert_header(common::csrf_header_for(&rotated_a))
        .to_request();
    assert_service_status!(
        app,
        req,
        401,
        "reuse should revoke newly rotated refresh tokens"
    );
}

#[actix_web::test]
async fn test_concurrent_refresh_same_token_has_single_winner() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    let (_, refresh) = register_and_login!(app);
    let hook = local::test_support::install_refresh_rotation_test_hook(
        &refresh,
        &state.config.auth.jwt_secret,
    )
    .await
    .unwrap();
    let _hook_guard = RefreshHookGuard::new(hook.clone());

    let first_state = state.clone();
    let first_refresh = refresh.clone();
    let first_task = tokio::spawn(async move {
        local::refresh_tokens(
            &first_state,
            &first_refresh,
            Some("127.0.0.1"),
            Some("AsterDrive Test Client/1.0"),
        )
        .await
    });

    tokio::time::timeout(Duration::from_secs(2), hook.wait_until_lock_acquired())
        .await
        .expect("first refresh should acquire rotation lock");

    let second_state = state.clone();
    let second_refresh = refresh.clone();
    let second_task = tokio::spawn(async move {
        local::refresh_tokens(
            &second_state,
            &second_refresh,
            Some("127.0.0.1"),
            Some("AsterDrive Test Client/1.0"),
        )
        .await
    });

    tokio::time::timeout(Duration::from_secs(2), hook.wait_until_lock_contended())
        .await
        .expect("second refresh should wait on the same rotation lock");
    hook.release_lock();

    let (first, second) = tokio::join!(first_task, second_task);
    let first = first.expect("first refresh task should not panic");
    let second = second.expect("second refresh task should not panic");

    assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    assert_eq!(
        usize::from(first.is_err()) + usize::from(second.is_err()),
        1
    );

    let winner = first.as_ref().ok().or(second.as_ref().ok()).unwrap();
    assert!(!winner.0.is_empty());
    assert!(!winner.1.is_empty());
    let winner_access = winner.0.clone();
    common::seed_csrf_token(&winner_access);
    let loser = first.as_ref().err().or(second.as_ref().err()).unwrap();
    assert_eq!(loser.code(), "E019");

    let user = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .expect("user should exist");
    assert_eq!(user.session_version, 1);
    assert!(
        auth_session_repo::list_active_for_user(state.writer_db(), user.id)
            .await
            .unwrap()
            .len()
            == 1
    );

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&winner_access)))
        .insert_header(common::csrf_header_for(&winner_access))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_recent_refresh_reuse_from_different_client_revokes_all_sessions() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    let (_, refresh) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("Cookie", common::refresh_cookie_header(&refresh)))
        .insert_header(common::csrf_header_for(&refresh))
        .insert_header(("User-Agent", "AsterDrive Original Client/1.0"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("Cookie", common::refresh_cookie_header(&refresh)))
        .insert_header(common::csrf_header_for(&refresh))
        .insert_header(("User-Agent", "AsterDrive Suspicious Client/1.0"))
        .to_request();
    assert_service_status!(
        app,
        req,
        401,
        "recent refresh reuse from a different client should be treated as compromise"
    );

    let user = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .expect("user should exist");
    assert_eq!(user.session_version, 2);
    assert!(
        auth_session_repo::list_active_for_user(state.writer_db(), user.id)
            .await
            .unwrap()
            .is_empty()
    );
}

#[actix_web::test]
async fn test_recent_refresh_reuse_from_spoofed_forwarded_ip_stays_stale() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "testuser",
            "email": "test@example.com",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .insert_header(("User-Agent", "AsterDrive Same Client/1.0"))
        .insert_header(("X-Forwarded-For", "203.0.113.10"))
        .set_json(serde_json::json!({
            "identifier": "testuser",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let refresh = common::extract_cookie(&resp, "aster_refresh").unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .insert_header(("Cookie", common::refresh_cookie_header(&refresh)))
        .insert_header(common::csrf_header_for(&refresh))
        .insert_header(("User-Agent", "AsterDrive Same Client/1.0"))
        .insert_header(("X-Forwarded-For", "203.0.113.11"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .insert_header(("Cookie", common::refresh_cookie_header(&refresh)))
        .insert_header(common::csrf_header_for(&refresh))
        .insert_header(("User-Agent", "AsterDrive Same Client/1.0"))
        .insert_header(("X-Forwarded-For", "203.0.113.12"))
        .to_request();
    assert_service_status!(
        app,
        req,
        401,
        "untrusted forwarded IP changes should not make same-client stale refresh look compromised"
    );

    let user = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .expect("user should exist");
    assert_eq!(user.session_version, 1);
    assert_eq!(
        auth_session_repo::list_active_for_user(state.writer_db(), user.id)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[actix_web::test]
async fn test_recent_refresh_reuse_without_client_evidence_revokes_all_sessions() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    let (_, refresh) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("Cookie", common::refresh_cookie_header(&refresh)))
        .insert_header(common::csrf_header_for(&refresh))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("Cookie", common::refresh_cookie_header(&refresh)))
        .insert_header(common::csrf_header_for(&refresh))
        .to_request();
    assert_service_status!(
        app,
        req,
        401,
        "recent refresh reuse without client evidence should be treated as compromise"
    );

    let user = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .expect("user should exist");
    assert_eq!(user.session_version, 2);
    assert!(
        auth_session_repo::list_active_for_user(state.writer_db(), user.id)
            .await
            .unwrap()
            .is_empty()
    );
}

#[actix_web::test]
async fn test_bearer_authenticated_write_allows_missing_request_source() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (access, _) = register_and_login!(app);

    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/profile")
        .insert_header(("Authorization", format!("Bearer {access}")))
        .set_json(serde_json::json!({
            "display_name": "Bearer Path"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["display_name"], "Bearer Path");
}

#[actix_web::test]
async fn test_setup_still_works_when_public_registration_is_disabled() {
    let state = common::setup().await;
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::auth_runtime::AUTH_ALLOW_USER_REGISTRATION_KEY,
        "false",
    ));
    let app = create_test_app!(state);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/setup")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "adminuser",
            "email": "admin@example.com",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
}

#[actix_web::test]
async fn test_setup_bootstraps_public_site_url_from_request_origin() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/setup")
        .insert_header(("Origin", TEST_PUBLIC_SITE_ORIGIN))
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "adminuser",
            "email": "admin@example.com",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let stored = aster_drive::db::repository::config_repo::find_by_key(
        state.writer_db(),
        aster_drive::config::site_url::PUBLIC_SITE_URL_KEY,
    )
    .await
    .unwrap()
    .expect("public_site_url should exist");
    assert_eq!(stored.value, format!(r#"["{TEST_PUBLIC_SITE_ORIGIN}"]"#));
    assert_eq!(stored.updated_by, Some(1));
}

#[actix_web::test]
async fn test_setup_does_not_overwrite_existing_public_site_url() {
    let state = common::setup().await;
    aster_drive::services::config_service::set(
        &state,
        aster_drive::config::site_url::PUBLIC_SITE_URL_KEY,
        vec!["https://pan-cloudreve.esaps.net".to_string()],
        1,
    )
    .await
    .expect("public_site_url should be writable");
    let app = create_test_app!(state.clone());

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/setup")
        .insert_header(("Origin", TEST_PUBLIC_SITE_ORIGIN))
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "adminuser",
            "email": "admin@example.com",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let stored = aster_drive::db::repository::config_repo::find_by_key(
        state.writer_db(),
        aster_drive::config::site_url::PUBLIC_SITE_URL_KEY,
    )
    .await
    .unwrap()
    .expect("public_site_url should exist");
    assert_eq!(stored.value, r#"["https://pan-cloudreve.esaps.net"]"#);
}

#[actix_web::test]
async fn test_passkey_login_start_rejects_missing_public_site_url_with_config_error() {
    let state = common::setup_with_memory_cache().await;
    let app = create_test_app!(state);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/passkeys/login/start")
        .insert_header(("Origin", TEST_BROWSER_ORIGIN))
        .set_json(serde_json::json!({ "identifier": "testuser" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::ConfigPublicSiteUrlRequired.as_str()
    );
    assert_eq!(
        body["msg"],
        "public_site_url must be configured before enabling passkey authentication"
    );
}

#[actix_web::test]
async fn test_passkey_login_start_rejects_insecure_public_site_url_with_config_error() {
    let state = common::setup_with_memory_cache().await;
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::site_url::PUBLIC_SITE_URL_KEY,
        r#"["http://example.com"]"#,
    ));
    let app = create_test_app!(state);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/passkeys/login/start")
        .insert_header(("Origin", TEST_BROWSER_ORIGIN))
        .set_json(serde_json::json!({ "identifier": "testuser" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::ConfigPublicSiteUrlInvalid.as_str()
    );
    assert_eq!(
        body["msg"],
        "passkey authentication requires HTTPS public_site_url, except localhost"
    );
}

#[actix_web::test]
async fn test_passkey_login_start_rejects_localhost_prefix_spoofing() {
    let state = common::setup_with_memory_cache().await;
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::site_url::PUBLIC_SITE_URL_KEY,
        r#"["http://localhost.evil.example"]"#,
    ));
    let app = create_test_app!(state);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/passkeys/login/start")
        .insert_header(("Origin", TEST_BROWSER_ORIGIN))
        .set_json(serde_json::json!({ "identifier": "testuser" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::ConfigPublicSiteUrlInvalid.as_str()
    );
}

#[actix_web::test]
async fn test_check_reports_public_registration_flag() {
    let state = common::setup().await;
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::auth_runtime::AUTH_ALLOW_USER_REGISTRATION_KEY,
        "false",
    ));
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::auth_runtime::AUTH_PASSKEY_LOGIN_ENABLED_KEY,
        "false",
    ));
    let app = create_test_app!(state);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/setup")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "adminuser",
            "email": "admin@example.com",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/check")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["has_users"], true);
    assert_eq!(body["data"]["allow_user_registration"], false);
    assert_eq!(body["data"]["passkey_login_enabled"], false);
}

#[actix_web::test]
async fn test_register_is_blocked_when_public_registration_is_disabled() {
    let state = common::setup().await;
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::auth_runtime::AUTH_ALLOW_USER_REGISTRATION_KEY,
        "false",
    ));
    let app = create_test_app!(state);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/setup")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "adminuser",
            "email": "admin@example.com",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "blockeduser",
            "email": "blocked@example.com",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["msg"], "new user registration is disabled");
    assert_eq!(body["code"], "auth.registration_disabled");
    assert_eq!(body["error"]["retryable"], false);
    assert!(body["error"].get("internal_code").is_none());
}

#[actix_web::test]
async fn test_local_email_policy_allows_registration_when_lists_are_empty() {
    let state = common::setup().await;
    configure_local_email_policy(&state, &[], &[]);
    let app = create_test_app!(state);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/setup")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "adminuser",
            "email": "admin@example.com",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "openuser",
            "email": "open@anywhere.test",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
}

#[actix_web::test]
async fn test_local_email_policy_allows_registration_by_exact_domain_allowlist() {
    let state = common::setup().await;
    configure_local_email_policy(&state, &["example.com"], &[]);
    let app = create_test_app!(state);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/setup")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "adminuser",
            "email": "admin@outside.test",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "aliceuser",
            "email": "Alice@Example.COM",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
}

#[actix_web::test]
async fn test_local_email_policy_rejects_registration_outside_allowlist() {
    let state = common::setup().await;
    configure_local_email_policy(&state, &["example.com"], &[]);
    let app = create_test_app!(state);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/setup")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "adminuser",
            "email": "admin@outside.test",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "bobuser",
            "email": "bob@other.test",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["msg"],
        "email address is not allowed by local account policy"
    );
    assert_eq!(body["code"], "auth.email_not_allowlisted");
}

#[actix_web::test]
async fn test_local_email_policy_allows_registration_by_full_email_allowlist_only() {
    let state = common::setup().await;
    configure_local_email_policy(&state, &["alice@example.com"], &[]);
    let app = create_test_app!(state);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/setup")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "adminuser",
            "email": "admin@outside.test",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "aliceuser",
            "email": "ALICE@example.com",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "bobuser",
            "email": "bob@example.com",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "auth.email_not_allowlisted");
}

#[actix_web::test]
async fn test_local_email_policy_blocks_registration_by_domain_and_email() {
    let state = common::setup().await;
    configure_local_email_policy(&state, &[], &["tempmail.test", "blocked@example.com"]);
    let app = create_test_app!(state);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/setup")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "adminuser",
            "email": "admin@example.com",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    for (username, email) in [
        ("tempuser", "user@tempmail.test"),
        ("blockeduser", "BLOCKED@example.com"),
    ] {
        let req = test::TestRequest::post()
            .uri("/api/v1/auth/register")
            .peer_addr("127.0.0.1:12345".parse().unwrap())
            .set_json(serde_json::json!({
                "username": username,
                "email": email,
                "password": "password123"
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(
            body["msg"],
            "email address is blocked by local account policy"
        );
        assert_eq!(body["code"], "auth.email_blocked");
    }
}

#[actix_web::test]
async fn test_local_email_policy_blocklist_wins_over_allowlist() {
    let state = common::setup().await;
    configure_local_email_policy(&state, &["example.com"], &["blocked@example.com"]);
    let app = create_test_app!(state);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/setup")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "adminuser",
            "email": "admin@outside.test",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "aliceuser",
            "email": "alice@example.com",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "blockeduser",
            "email": "blocked@example.com",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "auth.email_blocked");
}

#[actix_web::test]
async fn test_local_email_policy_does_not_match_subdomains_for_registration() {
    let state = common::setup().await;
    configure_local_email_policy(&state, &["example.com"], &[]);
    let app = create_test_app!(state);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/setup")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "adminuser",
            "email": "admin@outside.test",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "subuser",
            "email": "user@sub.example.com",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "auth.email_not_allowlisted");
}

#[actix_web::test]
async fn test_setup_bypasses_local_email_policy() {
    let state = common::setup().await;
    configure_local_email_policy(&state, &["example.com"], &["blocked.test"]);
    let app = create_test_app!(state);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/setup")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "adminuser",
            "email": "admin@blocked.test",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
}

#[actix_web::test]
async fn test_admin_create_user_bypasses_local_email_policy() {
    let state = common::setup().await;
    configure_local_email_policy(&state, &["example.com"], &["blocked.test"]);
    let app = create_test_app!(state.clone());

    let (admin_token, _refresh) = register_and_login!(app);
    let user_id = admin_create_user_with_credentials!(
        app,
        admin_token,
        "manageduser",
        "managed@blocked.test",
        "password123"
    );

    let user = user_repo::find_by_id(state.writer_db(), user_id)
        .await
        .expect("user lookup should succeed");
    assert_eq!(user.email, "managed@blocked.test");
}

#[actix_web::test]
async fn test_register_requires_activation_until_confirmed() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let runtime_config = state.runtime_config.clone();
    let app = create_test_app!(state);

    let _ = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "pendinguser",
            "email": "pendinguser@example.com",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    common::flush_mail_outbox_with(&db, &runtime_config, &mail_sender).await;

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "pendinguser",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "auth.credentials_failed");
    assert_eq!(body["msg"], "Invalid Credentials");

    let memory_sender = aster_drive::services::mail::sender::memory_sender_ref(&mail_sender)
        .expect("memory mail sender should be available in tests");
    let token = extract_verification_token(
        &memory_sender
            .last_message()
            .expect("activation email should be sent"),
    );

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/auth/contact-verification/confirm?token={}",
            urlencoding::encode(&token)
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 302);
    common::flush_mail_outbox_with(&db, &runtime_config, &mail_sender).await;
    let location = resp
        .headers()
        .get("Location")
        .and_then(|value| value.to_str().ok())
        .expect("contact verification redirect location missing");
    assert_eq!(location, "/login?contact_verification=register-activated");

    let (_access, _refresh) = login_user!(app, "pendinguser", "password123");
}

#[actix_web::test]
async fn test_register_skips_activation_when_register_activation_is_disabled() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let runtime_config = state.runtime_config.clone();
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::auth_runtime::AUTH_REGISTER_ACTIVATION_ENABLED_KEY,
        "false",
    ));
    let app = create_test_app!(state);

    let _ = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "directuser",
            "email": "directuser@example.com",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["email_verified"], true);

    common::flush_mail_outbox_with(&db, &runtime_config, &mail_sender).await;
    let memory_sender = aster_drive::services::mail::sender::memory_sender_ref(&mail_sender)
        .expect("memory mail sender should be available in tests");
    assert!(memory_sender.messages().is_empty());

    let (_access, _refresh) = login_user!(app, "directuser", "password123");
}

#[actix_web::test]
async fn test_register_resend_is_generic_for_unknown_identifier_and_cooldown() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let runtime_config = state.runtime_config.clone();
    let app = create_test_app!(state.clone());

    let (admin_token, _) = register_and_login!(app);
    let active_user_id = admin_create_user_with_credentials!(
        app,
        admin_token,
        "activeuser",
        "activeuser@example.com",
        "password123"
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "pendinguser",
            "email": "pendinguser@example.com",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    common::flush_mail_outbox_with(&db, &runtime_config, &mail_sender).await;

    let resend = |identifier: &str| {
        test::TestRequest::post()
            .uri("/api/v1/auth/register/resend")
            .peer_addr("127.0.0.1:12345".parse().unwrap())
            .set_json(serde_json::json!({ "identifier": identifier }))
            .to_request()
    };

    let resp = test::call_service(&app, resend("missing@example.com")).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["message"],
        "If the account can be reactivated, an activation email will be sent"
    );

    let resp = test::call_service(&app, resend("activeuser@example.com")).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["message"],
        "If the account can be reactivated, an activation email will be sent"
    );

    {
        use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};

        let user = user_repo::find_by_id(state.writer_db(), active_user_id)
            .await
            .expect("user lookup should succeed");
        let mut active = user.into_active_model();
        active.status = Set(UserStatus::Disabled);
        active.update(state.writer_db()).await.unwrap();
    }

    let resp = test::call_service(&app, resend("activeuser@example.com")).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["message"],
        "If the account can be reactivated, an activation email will be sent"
    );

    let resp = test::call_service(&app, resend("pendinguser")).await;
    assert_eq!(resp.status(), 200);
    common::flush_mail_outbox_with(&db, &runtime_config, &mail_sender).await;

    let memory_sender = aster_drive::services::mail::sender::memory_sender_ref(&mail_sender)
        .expect("memory mail sender should be available in tests");
    assert_eq!(memory_sender.messages().len(), 1);
}

#[actix_web::test]
async fn test_register_activation_resend_ignores_allowlist_and_hides_blocklist() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let runtime_config = state.runtime_config.clone();
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::auth_runtime::AUTH_CONTACT_VERIFICATION_RESEND_COOLDOWN_SECS_KEY,
        "1",
    ));
    let app = create_test_app!(state.clone());

    let _ = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "pendinguser",
            "email": "pendinguser@example.com",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    common::flush_mail_outbox_with(&db, &runtime_config, &mail_sender).await;

    tokio::time::sleep(ONE_SECOND_WINDOW_ELAPSED).await;
    configure_local_email_policy(&state, &["other.test"], &[]);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register/resend")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({ "identifier": "pendinguser" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    common::flush_mail_outbox_with(&db, &runtime_config, &mail_sender).await;

    tokio::time::sleep(ONE_SECOND_WINDOW_ELAPSED).await;
    configure_local_email_policy(&state, &["other.test"], &["pendinguser@example.com"]);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register/resend")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({ "identifier": "pendinguser" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["message"],
        "If the account can be reactivated, an activation email will be sent"
    );
    common::flush_mail_outbox_with(&db, &runtime_config, &mail_sender).await;

    let memory_sender = aster_drive::services::mail::sender::memory_sender_ref(&mail_sender)
        .expect("memory mail sender should be available in tests");
    assert_eq!(memory_sender.messages().len(), 2);
}

#[actix_web::test]
async fn test_email_change_confirmation_redirects_and_notifies_previous_email() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let runtime_config = state.runtime_config.clone();
    let app = create_test_app!(state);

    let (access, _refresh) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/email/change")
        .insert_header(("Cookie", common::access_cookie_header(&access)))
        .insert_header(common::csrf_header_for(&access))
        .set_json(serde_json::json!({
            "new_email": "updated@example.com"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    common::flush_mail_outbox_with(&db, &runtime_config, &mail_sender).await;

    let memory_sender = aster_drive::services::mail::sender::memory_sender_ref(&mail_sender)
        .expect("memory mail sender should be available in tests");
    let token = extract_verification_token(
        &memory_sender
            .last_message()
            .expect("email change confirmation should be sent"),
    );

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/auth/contact-verification/confirm?token={}",
            urlencoding::encode(&token)
        ))
        .insert_header(("Cookie", common::access_cookie_header(&access)))
        .insert_header(common::csrf_header_for(&access))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 302);
    common::flush_mail_outbox_with(&db, &runtime_config, &mail_sender).await;
    let location = resp
        .headers()
        .get("Location")
        .and_then(|value| value.to_str().ok())
        .expect("contact verification redirect location missing");
    assert_eq!(
        location,
        "/settings/security?contact_verification=email-changed&email=updated%40example.com"
    );

    let messages = memory_sender.messages();
    let previous_email_notice = messages
        .last()
        .expect("email change notice should be sent to previous address");
    assert_eq!(previous_email_notice.to.address, "test@example.com");
    assert_eq!(
        previous_email_notice.subject,
        format!("Your {DEFAULT_BRANDING_TITLE} Email Address Was Updated")
    );
    assert!(
        previous_email_notice
            .text_body
            .contains("updated@example.com")
    );
}

#[actix_web::test]
async fn test_local_email_policy_applies_to_email_change_request() {
    let state = common::setup().await;
    configure_local_email_policy(&state, &["example.com"], &["blocked@example.com"]);
    let app = create_test_app!(state);

    let (access, _refresh) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/email/change")
        .insert_header(("Cookie", common::access_cookie_header(&access)))
        .insert_header(common::csrf_header_for(&access))
        .set_json(serde_json::json!({
            "new_email": "updated@example.com"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/email/change")
        .insert_header(("Cookie", common::access_cookie_header(&access)))
        .insert_header(common::csrf_header_for(&access))
        .set_json(serde_json::json!({
            "new_email": "blocked@example.com"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "auth.email_blocked");

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/email/change")
        .insert_header(("Cookie", common::access_cookie_header(&access)))
        .insert_header(common::csrf_header_for(&access))
        .set_json(serde_json::json!({
            "new_email": "updated@other.test"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "auth.email_not_allowlisted");
}

#[actix_web::test]
async fn test_email_change_resend_ignores_allowlist_but_rejects_blocklist() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let runtime_config = state.runtime_config.clone();
    let app = create_test_app!(state.clone());

    let (access, _refresh) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/email/change")
        .insert_header(("Cookie", common::access_cookie_header(&access)))
        .insert_header(common::csrf_header_for(&access))
        .set_json(serde_json::json!({
            "new_email": "pending@example.com"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    common::flush_mail_outbox_with(&db, &runtime_config, &mail_sender).await;

    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::auth_runtime::AUTH_CONTACT_VERIFICATION_RESEND_COOLDOWN_SECS_KEY,
        "1",
    ));
    tokio::time::sleep(ONE_SECOND_WINDOW_ELAPSED).await;
    configure_local_email_policy(&state, &["other.test"], &[]);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/email/change/resend")
        .insert_header(("Cookie", common::access_cookie_header(&access)))
        .insert_header(common::csrf_header_for(&access))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    configure_local_email_policy(&state, &["other.test"], &["pending@example.com"]);
    tokio::time::sleep(ONE_SECOND_WINDOW_ELAPSED).await;

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/email/change/resend")
        .insert_header(("Cookie", common::access_cookie_header(&access)))
        .insert_header(common::csrf_header_for(&access))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "auth.email_blocked");
}

#[actix_web::test]
async fn test_email_change_resend_returns_generic_success_during_cooldown() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let runtime_config = state.runtime_config.clone();
    let app = create_test_app!(state);

    let (access, _refresh) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/email/change")
        .insert_header(("Cookie", common::access_cookie_header(&access)))
        .insert_header(common::csrf_header_for(&access))
        .set_json(serde_json::json!({
            "new_email": "updated@example.com"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    common::flush_mail_outbox_with(&db, &runtime_config, &mail_sender).await;

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/email/change/resend")
        .insert_header(("Cookie", common::access_cookie_header(&access)))
        .insert_header(common::csrf_header_for(&access))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["message"],
        "If an email change is pending, a confirmation email will be sent"
    );
    common::flush_mail_outbox_with(&db, &runtime_config, &mail_sender).await;

    let memory_sender = aster_drive::services::mail::sender::memory_sender_ref(&mail_sender)
        .expect("memory mail sender should be available in tests");
    assert_eq!(memory_sender.messages().len(), 1);
}

#[actix_web::test]
async fn test_password_reset_request_is_generic_for_unknown_email() {
    let state = common::setup().await;
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/password/reset/request")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "email": "missing@example.com"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "success");

    let memory_sender = aster_drive::services::mail::sender::memory_sender_ref(&mail_sender)
        .expect("memory mail sender should be available in tests");
    assert!(memory_sender.messages().is_empty());
}

#[actix_web::test]
async fn test_password_reset_rotates_session_and_sends_notice_and_records_audit_logs() {
    use aster_drive::entities::audit_log;
    use sea_orm::EntityTrait;

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let runtime_config = state.runtime_config.clone();
    let app = create_test_app!(state);
    let (access, refresh) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/password/reset/request")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "email": "test@example.com"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    common::flush_mail_outbox_with(&db, &runtime_config, &mail_sender).await;

    let memory_sender = aster_drive::services::mail::sender::memory_sender_ref(&mail_sender)
        .expect("memory mail sender should be available in tests");
    let token = extract_password_reset_token(
        &memory_sender
            .last_message()
            .expect("password reset email should be sent"),
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/password/reset/confirm")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "token": token,
            "new_password": "newsecret456"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    common::flush_mail_outbox_with(&db, &runtime_config, &mail_sender).await;

    let messages = memory_sender.messages();
    let password_reset_notice = messages
        .last()
        .expect("password reset notice should be sent after confirmation");
    assert_eq!(password_reset_notice.to.address, "test@example.com");
    assert_eq!(
        password_reset_notice.subject,
        format!("Your {DEFAULT_BRANDING_TITLE} Password Was Reset")
    );
    assert!(
        password_reset_notice
            .text_body
            .contains("Your password was reset")
    );
    assert!(
        password_reset_notice
            .text_body
            .contains("If you did not reset your password")
    );

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&access)))
        .insert_header(common::csrf_header_for(&access))
        .to_request();
    assert_service_status!(app, req, 401);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("Cookie", common::refresh_cookie_header(&refresh)))
        .insert_header(common::csrf_header_for(&refresh))
        .to_request();
    assert_service_status!(app, req, 401);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "testuser",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "testuser",
            "password": "newsecret456"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let actions: Vec<String> = audit_log::Entity::find()
        .all(&db)
        .await
        .unwrap()
        .into_iter()
        .map(|entry| entry.action.to_string())
        .collect();
    assert!(actions.contains(&"user_request_password_reset".to_string()));
    assert!(actions.contains(&"user_confirm_password_reset".to_string()));
}

#[actix_web::test]
async fn test_password_reset_confirm_rejects_reused_token() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let runtime_config = state.runtime_config.clone();
    let app = create_test_app!(state);
    let _ = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/password/reset/request")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "email": "test@example.com"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    common::flush_mail_outbox_with(&db, &runtime_config, &mail_sender).await;

    let memory_sender = aster_drive::services::mail::sender::memory_sender_ref(&mail_sender)
        .expect("memory mail sender should be available in tests");
    let token = extract_password_reset_token(
        &memory_sender
            .last_message()
            .expect("password reset email should be sent"),
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/password/reset/confirm")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "token": token,
            "new_password": "newsecret456"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/password/reset/confirm")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "token": token,
            "new_password": "anothersecret789"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "auth.contact_verification_invalid");
}

#[actix_web::test]
async fn test_password_reset_confirm_rejects_expired_token() {
    use aster_drive::entities::contact_verification_token;
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter, Set};

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let runtime_config = state.runtime_config.clone();
    let app = create_test_app!(state);
    let _ = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/password/reset/request")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "email": "test@example.com"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    common::flush_mail_outbox_with(&db, &runtime_config, &mail_sender).await;

    let memory_sender = aster_drive::services::mail::sender::memory_sender_ref(&mail_sender)
        .expect("memory mail sender should be available in tests");
    let token = extract_password_reset_token(
        &memory_sender
            .last_message()
            .expect("password reset email should be sent"),
    );

    let token_hash = aster_drive::utils::hash::sha256_hex(token.as_bytes());
    let record = contact_verification_token::Entity::find()
        .filter(contact_verification_token::Column::TokenHash.eq(token_hash))
        .one(&db)
        .await
        .unwrap()
        .expect("password reset token record should exist");
    let mut active = record.into_active_model();
    active.expires_at = Set(chrono::Utc::now() - chrono::Duration::seconds(1));
    active.update(&db).await.unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/password/reset/confirm")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "token": token,
            "new_password": "newsecret456"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 410);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "auth.contact_verification_expired");
}

#[actix_web::test]
async fn test_contact_verification_confirm_rejects_password_reset_token() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let runtime_config = state.runtime_config.clone();
    let app = create_test_app!(state);
    let _ = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/password/reset/request")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "email": "test@example.com"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    common::flush_mail_outbox_with(&db, &runtime_config, &mail_sender).await;

    let memory_sender = aster_drive::services::mail::sender::memory_sender_ref(&mail_sender)
        .expect("memory mail sender should be available in tests");
    let token = extract_password_reset_token(
        &memory_sender
            .last_message()
            .expect("password reset email should be sent"),
    );

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/auth/contact-verification/confirm?token={}",
            urlencoding::encode(&token)
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 302);
    let location = resp
        .headers()
        .get("Location")
        .and_then(|value| value.to_str().ok())
        .expect("contact verification redirect location missing");
    assert_eq!(location, "/login?contact_verification=invalid");
}

#[actix_web::test]
async fn test_password_reset_request_cooldown_returns_generic_success() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let runtime_config = state.runtime_config.clone();
    let app = create_test_app!(state);
    let _ = register_and_login!(app);

    let request = || {
        test::TestRequest::post()
            .uri("/api/v1/auth/password/reset/request")
            .peer_addr("127.0.0.1:12345".parse().unwrap())
            .set_json(serde_json::json!({
                "email": "test@example.com"
            }))
            .to_request()
    };

    let resp = test::call_service(&app, request()).await;
    assert_eq!(resp.status(), 200);

    let resp = test::call_service(&app, request()).await;
    assert_eq!(resp.status(), 200);
    common::flush_mail_outbox_with(&db, &runtime_config, &mail_sender).await;

    let memory_sender = aster_drive::services::mail::sender::memory_sender_ref(&mail_sender)
        .expect("memory mail sender should be available in tests");
    assert_eq!(memory_sender.messages().len(), 1);
}

#[actix_web::test]
async fn test_contact_verification_tokens_allow_only_one_unconsumed_token_per_purpose() {
    use aster_drive::db::repository::{contact_verification_token_repo, user_repo};
    use aster_drive::entities::contact_verification_token;
    use aster_drive::types::{VerificationChannel, VerificationPurpose};
    use sea_orm::Set;

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let _ = register_and_login!(app);

    let user = user_repo::find_by_email(&db, "test@example.com")
        .await
        .unwrap()
        .expect("test user should exist");
    let now = chrono::Utc::now();

    contact_verification_token_repo::create(
        &db,
        contact_verification_token::ActiveModel {
            user_id: Set(user.id),
            channel: Set(VerificationChannel::Email),
            purpose: Set(VerificationPurpose::PasswordReset),
            target: Set(user.email.clone()),
            token_hash: Set("token-hash-1".to_string()),
            expires_at: Set(now + chrono::Duration::minutes(10)),
            consumed_at: Set(None),
            created_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let duplicate = contact_verification_token_repo::create(
        &db,
        contact_verification_token::ActiveModel {
            user_id: Set(user.id),
            channel: Set(VerificationChannel::Email),
            purpose: Set(VerificationPurpose::PasswordReset),
            target: Set(user.email.clone()),
            token_hash: Set("token-hash-2".to_string()),
            expires_at: Set(now + chrono::Duration::minutes(20)),
            consumed_at: Set(None),
            created_at: Set(now + chrono::Duration::seconds(1)),
            ..Default::default()
        },
    )
    .await;
    assert!(duplicate.is_err());

    let first = contact_verification_token_repo::find_latest_active_for_user(
        &db,
        user.id,
        VerificationChannel::Email,
        VerificationPurpose::PasswordReset,
    )
    .await
    .unwrap()
    .expect("first token should still be active");
    contact_verification_token_repo::mark_consumed(&db, first)
        .await
        .unwrap();

    contact_verification_token_repo::create(
        &db,
        contact_verification_token::ActiveModel {
            user_id: Set(user.id),
            channel: Set(VerificationChannel::Email),
            purpose: Set(VerificationPurpose::PasswordReset),
            target: Set(user.email),
            token_hash: Set("token-hash-3".to_string()),
            expires_at: Set(now + chrono::Duration::minutes(30)),
            consumed_at: Set(None),
            created_at: Set(now + chrono::Duration::seconds(2)),
            ..Default::default()
        },
    )
    .await
    .unwrap();
}

#[actix_web::test]
async fn test_token_refresh() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (_access, refresh) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .insert_header(("Cookie", common::refresh_cookie_header(&refresh)))
        .insert_header(common::csrf_header_for(&refresh))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let access = common::extract_cookie(&resp, "aster_access");
    let refresh = common::extract_cookie(&resp, "aster_refresh");
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "success");
    assert_eq!(body["data"]["expires_in"], 900);
    assert!(access.is_some());
    assert!(refresh.is_some());
}

#[actix_web::test]
async fn test_login_uses_runtime_auth_policy() {
    let state = common::setup().await;
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::auth_runtime::AUTH_COOKIE_SECURE_KEY,
        "true",
    ));
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::auth_runtime::AUTH_ACCESS_TOKEN_TTL_SECS_KEY,
        "120",
    ));
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::auth_runtime::AUTH_REFRESH_TOKEN_TTL_SECS_KEY,
        "3600",
    ));
    let app = create_test_app!(state);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "runtimeauth",
            "email": "runtimeauth@example.com",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "runtimeauth",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let access_cookie_max_age = resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "aster_access")
        .expect("access cookie missing")
        .max_age()
        .map(|duration| duration.whole_seconds());
    let refresh_cookie_max_age = resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "aster_refresh")
        .expect("refresh cookie missing")
        .max_age()
        .map(|duration| duration.whole_seconds());
    let access_cookie_secure = resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "aster_access")
        .expect("access cookie missing")
        .secure();
    let refresh_cookie_secure = resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "aster_refresh")
        .expect("refresh cookie missing")
        .secure();
    let body: Value = test::read_body_json(resp).await;

    assert_eq!(body["data"]["expires_in"], 120);
    assert_eq!(access_cookie_max_age, Some(120));
    assert_eq!(refresh_cookie_max_age, Some(3600));
    assert_eq!(access_cookie_secure, Some(true));
    assert_eq!(refresh_cookie_secure, Some(true));
}

#[actix_web::test]
async fn test_refresh_token_cannot_access_protected_routes() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (_access, refresh) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&refresh)))
        .insert_header(common::csrf_header_for(&refresh))
        .to_request();
    assert_service_status!(app, req, 401);
}

#[actix_web::test]
async fn test_storage_events_stream_receives_file_change_frames() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/events/storage")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let content_type = resp
        .headers()
        .get("Content-Type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    assert_eq!(content_type, "text/event-stream");

    let (_req, resp) = resp.into_parts();
    let mut body = resp.into_body();

    let _file_id = upload_test_file!(app, token);

    let event = read_next_sse_json(&mut body).await;
    assert_eq!(event["kind"], "file.created");
    assert_eq!(event["workspace"]["kind"], "personal");
    assert_eq!(event["affects_quota"], true);
    assert_eq!(
        event["storage_delta"].as_i64(),
        Some("test content".len() as i64)
    );
    assert!(
        event["file_ids"]
            .as_array()
            .is_some_and(|ids| ids.len() == 1)
    );
    assert!(
        event["folder_ids"]
            .as_array()
            .is_some_and(|ids| ids.is_empty())
    );
    assert_eq!(event["root_affected"], true);
}

#[actix_web::test]
async fn test_storage_events_stream_receives_team_file_change_frames_for_member() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (owner_token, _) = register_and_login!(app);
    let member_id = admin_create_user_with_credentials!(
        app,
        owner_token,
        "teammember",
        "teammember@example.com",
        "password123"
    );
    let member_token = login_user_with_credentials!(app, "teammember", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Storage Events Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/members"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "user_id": member_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/events/storage")
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let (_req, resp) = resp.into_parts();
    let mut body = resp.into_body();

    let req = team_upload_request!(team_id, &owner_token, "team-event.txt", "team event");
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let event = read_next_sse_json(&mut body).await;
    assert_eq!(event["kind"], "file.created");
    assert_eq!(event["workspace"]["kind"], "team");
    assert_eq!(event["workspace"]["team_id"].as_i64(), Some(team_id));
    assert!(
        event["file_ids"]
            .as_array()
            .is_some_and(|ids| ids.len() == 1)
    );
    assert_eq!(event["root_affected"], true);
}

#[actix_web::test]
async fn test_storage_events_stream_hides_team_frames_from_non_members() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (owner_token, _) = register_and_login!(app);
    let _outsider_id = admin_create_user_with_credentials!(
        app,
        owner_token,
        "teamoutsider",
        "teamoutsider@example.com",
        "password123"
    );
    let outsider_token = login_user_with_credentials!(app, "teamoutsider", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Hidden Team Events" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/events/storage")
        .insert_header(("Cookie", common::access_cookie_header(&outsider_token)))
        .insert_header(common::csrf_header_for(&outsider_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let (_req, resp) = resp.into_parts();
    let mut body = resp.into_body();

    let req = team_upload_request!(team_id, &owner_token, "hidden.txt", "hidden event");
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let hidden_event = read_next_sse_json_with_timeout(&mut body, Duration::from_millis(500)).await;
    assert!(
        hidden_event.is_none(),
        "non-member should not receive team storage event: {hidden_event:?}"
    );

    let _file_id = upload_test_file_named!(app, outsider_token, "outsider-visible.txt");
    let event = read_next_sse_json(&mut body).await;
    assert_eq!(event["kind"], "file.created");
    assert_eq!(event["workspace"]["kind"], "personal");
    assert_eq!(event["root_affected"], true);
}

#[actix_web::test]
async fn test_storage_events_stream_closes_after_session_revocation() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (admin_token, _) = register_and_login!(app);
    let user_id = admin_create_user_with_credentials!(
        app,
        admin_token,
        "sse_revoke_user",
        "sse_revoke_user@example.com",
        "password123"
    );
    let revoked_token = login_user_with_credentials!(app, "sse_revoke_user", "password123");

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/events/storage")
        .insert_header(("Cookie", common::access_cookie_header(&revoked_token)))
        .insert_header(common::csrf_header_for(&revoked_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let (_req, resp) = resp.into_parts();
    let mut body = resp.into_body();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/admin/users/{user_id}/sessions/revoke"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let fresh_token = login_user_with_credentials!(app, "sse_revoke_user", "password123");
    let _file_id = upload_test_file_named!(app, fresh_token, "post-revoke-personal.txt");

    expect_sse_stream_end(&mut body).await;
}

#[actix_web::test]
async fn test_storage_events_stream_closes_on_server_shutdown() {
    let state = common::setup().await;
    let shutdown_token = CancellationToken::new();
    let app = {
        use actix_web::{App, test, web};

        let db = state.writer_db().clone();
        test::init_service(
            App::new()
                .wrap(aster_drive::api::middleware::security_headers::default_headers())
                .app_data(web::PayloadConfig::new(10 * 1024 * 1024))
                .app_data(web::JsonConfig::default().limit(1024 * 1024))
                .app_data(web::Data::new(shutdown_token.clone()))
                .app_data(web::Data::new(state))
                .configure(move |cfg| aster_drive::api::configure_primary(cfg, &db)),
        )
        .await
    };
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/events/storage")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let (_req, resp) = resp.into_parts();
    let mut body = resp.into_body();

    shutdown_token.cancel();

    expect_sse_stream_end(&mut body).await;
}

#[actix_web::test]
async fn test_storage_events_stream_rejects_new_connections_after_shutdown() {
    let state = common::setup().await;
    let shutdown_token = CancellationToken::new();
    let app = {
        use actix_web::{App, test, web};

        let db = state.writer_db().clone();
        test::init_service(
            App::new()
                .wrap(aster_drive::api::middleware::security_headers::default_headers())
                .app_data(web::PayloadConfig::new(10 * 1024 * 1024))
                .app_data(web::JsonConfig::default().limit(1024 * 1024))
                .app_data(web::Data::new(shutdown_token.clone()))
                .app_data(web::Data::new(state))
                .configure(move |cfg| aster_drive::api::configure_primary(cfg, &db)),
        )
        .await
    };
    let (token, _) = register_and_login!(app);

    shutdown_token.cancel();

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/events/storage")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 204);
}

#[actix_web::test]
async fn test_storage_events_stream_refreshes_team_visibility_after_member_removal() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (owner_token, _) = register_and_login!(app);
    let member_id = admin_create_user_with_credentials!(
        app,
        owner_token,
        "formermember",
        "formermember@example.com",
        "password123"
    );
    let member_token = login_user_with_credentials!(app, "formermember", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Removal Visibility Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/members"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "user_id": member_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/events/storage")
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let (_req, resp) = resp.into_parts();
    let mut body = resp.into_body();

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/teams/{team_id}/members/{member_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = team_upload_request!(
        team_id,
        &owner_token,
        "removed-member-hidden.txt",
        "team event"
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let hidden_event = read_next_sse_json_with_timeout(&mut body, Duration::from_millis(500)).await;
    assert!(
        hidden_event.is_none(),
        "removed member should not receive team storage event: {hidden_event:?}"
    );

    let _file_id = upload_test_file_named!(app, member_token, "formermember-visible.txt");
    let event = read_next_sse_json(&mut body).await;
    assert_eq!(event["kind"], "file.created");
    assert_eq!(event["workspace"]["kind"], "personal");
    assert_eq!(event["root_affected"], true);
}

#[actix_web::test]
async fn test_logout_clears_cookies_and_revokes_refresh_token() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    let (access, refresh) = register_and_login!(app);
    let refresh_claims = local::verify_token(&refresh, &state.config.auth.jwt_secret)
        .expect("refresh token should verify");
    let refresh_jti = refresh_claims.jti.expect("refresh token should carry jti");
    assert!(
        auth_session_repo::find_by_refresh_jti(state.writer_db(), &refresh_jti)
            .await
            .unwrap()
            .is_some()
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/logout")
        .insert_header((
            "Cookie",
            common::access_and_refresh_cookie_header(&access, &refresh),
        ))
        .insert_header(common::csrf_header_for(&access))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let cleared_access_path = resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "aster_access")
        .expect("cleared access cookie missing")
        .path()
        .map(str::to_string);
    let cleared_refresh_path = resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "aster_refresh")
        .expect("cleared refresh cookie missing")
        .path()
        .map(str::to_string);
    assert_eq!(
        common::extract_cookie(&resp, "aster_access").as_deref(),
        Some("")
    );
    assert_eq!(
        common::extract_cookie(&resp, "aster_refresh").as_deref(),
        Some("")
    );
    assert_eq!(cleared_access_path.as_deref(), Some("/"));
    assert_eq!(cleared_refresh_path.as_deref(), Some("/api/v1/auth"));
    let revoked_session = auth_session_repo::find_by_refresh_jti(state.writer_db(), &refresh_jti)
        .await
        .unwrap()
        .expect("revoked auth session should remain as tombstone");
    assert!(revoked_session.revoked_at.is_some());

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&access)))
        .insert_header(common::csrf_header_for(&access))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("Cookie", common::refresh_cookie_header(&refresh)))
        .insert_header(common::csrf_header_for(&refresh))
        .to_request();
    assert_service_status!(app, req, 401, "logout should revoke refresh token reuse");
}

#[actix_web::test]
async fn test_auth_me() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["username"], "testuser");
    assert!(body["data"]["access_token_expires_at"].as_i64().unwrap() > 0);
    assert!(body["data"]["password_hash"].is_null());
    assert!(body["data"]["profile"]["display_name"].is_null());
    assert_eq!(body["data"]["profile"]["avatar"]["source"], "none");
}

#[actix_web::test]
async fn test_auth_me_supports_field_selection() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me?fields=quota")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let data = &body["data"];
    assert_eq!(data["username"], "testuser");
    assert!(data["storage_used"].is_i64());
    assert!(data["storage_quota"].is_i64());
    assert!(data.get("access_token_expires_at").is_none());
    assert!(data.get("preferences").is_none());
    assert!(data.get("profile").is_none());
}

#[actix_web::test]
async fn test_auth_me_rejects_unknown_field_selection() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me?fields=quota,secrets")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    assert_service_status!(app, req, 400);
}

#[actix_web::test]
async fn test_auth_sessions_list_and_revoke_specific_device() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "alice",
            "email": "alice@example.com",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .insert_header(("User-Agent", "AsterDrive Device Alpha/1.0"))
        .set_json(serde_json::json!({
            "identifier": "alice",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let access_a = common::extract_cookie(&resp, "aster_access").unwrap();
    let refresh_a = common::extract_cookie(&resp, "aster_refresh").unwrap();
    let refresh_a_claims = local::verify_token(&refresh_a, &state.config.auth.jwt_secret)
        .expect("current refresh token should verify");
    assert_eq!(refresh_a_claims.session_version, 1);
    let refresh_a_jti = refresh_a_claims
        .jti
        .clone()
        .expect("current refresh token should carry jti");
    tokio::time::sleep(ONE_SECOND_WINDOW_ELAPSED).await;

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .insert_header(("User-Agent", "AsterDrive Device Beta/2.0"))
        .set_json(serde_json::json!({
            "identifier": "alice",
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert!(
        auth_session_repo::find_by_refresh_jti(state.writer_db(), &refresh_a_jti)
            .await
            .unwrap()
            .is_some()
    );
    let user = user_repo::find_by_username(state.writer_db(), "alice")
        .await
        .unwrap()
        .expect("user should exist");
    assert_eq!(user.session_version, 1);
    let refresh_b = common::extract_cookie(&resp, "aster_refresh").unwrap();

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/sessions")
        .insert_header((
            "Cookie",
            common::access_and_refresh_cookie_header(&access_a, &refresh_a),
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let sessions = body["data"]
        .as_array()
        .expect("sessions should be an array");
    assert_eq!(sessions.len(), 2);
    let current = sessions
        .iter()
        .find(|session| session["is_current"] == true)
        .expect("current session should be present");
    assert_eq!(
        current["user_agent"].as_str(),
        Some("AsterDrive Device Alpha/1.0")
    );
    let other = sessions
        .iter()
        .find(|session| session["is_current"] == false)
        .expect("other session should be present");
    assert_eq!(
        other["user_agent"].as_str(),
        Some("AsterDrive Device Beta/2.0")
    );
    let other_session_id = other["id"]
        .as_str()
        .expect("other session id should exist")
        .to_string();

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/auth/sessions/{other_session_id}"))
        .insert_header((
            "Cookie",
            common::access_and_refresh_cookie_header(&access_a, &refresh_a),
        ))
        .insert_header(common::csrf_header_for(&access_a))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("Cookie", common::refresh_cookie_header(&refresh_b)))
        .insert_header(common::csrf_header_for(&refresh_b))
        .to_request();
    assert_service_status!(app, req, 401, "revoked device refresh should fail");

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/sessions")
        .insert_header((
            "Cookie",
            common::access_and_refresh_cookie_header(&access_a, &refresh_a),
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let sessions = body["data"]
        .as_array()
        .expect("sessions should be an array");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["is_current"], true);
}

#[actix_web::test]
async fn test_auth_sessions_can_revoke_other_devices() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    let (access_a, refresh_a) = register_and_login!(app);
    let refresh_a_claims = local::verify_token(&refresh_a, &state.config.auth.jwt_secret)
        .expect("current refresh token should verify");
    assert_eq!(refresh_a_claims.session_version, 1);
    let refresh_a_jti = refresh_a_claims
        .jti
        .clone()
        .expect("current refresh token should carry jti");
    tokio::time::sleep(ONE_SECOND_WINDOW_ELAPSED).await;
    let (_, refresh_b) = login_user_with_auth_cookies!(app, "testuser", "password123");
    tokio::time::sleep(ONE_SECOND_WINDOW_ELAPSED).await;
    let (_, refresh_c) = login_user_with_auth_cookies!(app, "testuser", "password123");

    let req = test::TestRequest::delete()
        .uri("/api/v1/auth/sessions/others")
        .insert_header((
            "Cookie",
            common::access_and_refresh_cookie_header(&access_a, &refresh_a),
        ))
        .insert_header(common::csrf_header_for(&access_a))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["removed"], 2);
    assert!(
        auth_session_repo::find_by_refresh_jti(state.writer_db(), &refresh_a_jti)
            .await
            .unwrap()
            .is_some()
    );
    let user = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .expect("user should exist");
    assert_eq!(user.session_version, 1);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("Cookie", common::refresh_cookie_header(&refresh_b)))
        .insert_header(common::csrf_header_for(&refresh_b))
        .to_request();
    assert_service_status!(app, req, 401, "other device refresh should be revoked");

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("Cookie", common::refresh_cookie_header(&refresh_c)))
        .insert_header(common::csrf_header_for(&refresh_c))
        .to_request();
    assert_service_status!(app, req, 401, "other device refresh should be revoked");

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("Cookie", common::refresh_cookie_header(&refresh_a)))
        .insert_header(common::csrf_header_for(&refresh_a))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let body = test::read_body(resp).await;
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
}

#[actix_web::test]
async fn test_auth_sessions_can_revoke_current_device_and_clear_cookies() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    let (access, refresh) = register_and_login!(app);
    let refresh_claims = local::verify_token(&refresh, &state.config.auth.jwt_secret)
        .expect("refresh token should verify");
    let refresh_jti = refresh_claims.jti.expect("refresh token should carry jti");
    let current_session = auth_session_repo::find_by_refresh_jti(state.writer_db(), &refresh_jti)
        .await
        .unwrap()
        .expect("current auth session should exist");

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/auth/sessions/{}", current_session.id))
        .insert_header((
            "Cookie",
            common::access_and_refresh_cookie_header(&access, &refresh),
        ))
        .insert_header(common::csrf_header_for(&access))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        common::extract_cookie(&resp, "aster_access").as_deref(),
        Some("")
    );
    assert_eq!(
        common::extract_cookie(&resp, "aster_refresh").as_deref(),
        Some("")
    );
    let revoked_session = auth_session_repo::find_by_id(state.writer_db(), &current_session.id)
        .await
        .unwrap()
        .expect("revoked auth session should remain as tombstone");
    assert!(revoked_session.revoked_at.is_some());
}

/// 注册时自动分配新用户默认策略组
#[actix_web::test]
async fn test_register_auto_assigns_policy() {
    use aster_drive::db::repository::policy_group_repo;

    let state = common::setup().await;
    let expected_default_id = policy_group_repo::find_default_group(state.writer_db())
        .await
        .unwrap()
        .expect("default policy group should exist")
        .id;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 获取用户 ID
    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["policy_group_id"].as_i64().unwrap(),
        expected_default_id
    );
}

#[actix_web::test]
async fn test_unauthorized_access() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    // 没 token 访问受保护端点
    let req = test::TestRequest::get().uri("/api/v1/folders").to_request();
    let result = test::try_call_service(&app, req).await;
    match result {
        Ok(resp) => assert_eq!(resp.status(), 401),
        Err(err) => {
            let resp = err.error_response();
            assert_eq!(resp.status(), 401);
        }
    }

    // 假 token
    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Authorization", "Bearer fake.token.here"))
        .to_request();
    assert_service_status!(app, req, 401);
}

/// 用户状态缓存：正常认证 → 连续请求不应查 DB（通过 MemoryCache 验证）
#[actix_web::test]
async fn test_user_status_cached_in_auth_middleware() {
    // 明确使用 MemoryCache 验证认证缓存命中。
    let cache_config = aster_drive::config::CacheConfig {
        backend: "memory".to_string(),
        default_ttl: 60,
        ..Default::default()
    };
    let cache = aster_drive::cache::create_cache(&cache_config).await;

    let base = common::setup().await;
    let state = aster_drive::runtime::PrimaryAppState {
        db_handles: base.db_handles,
        driver_registry: base.driver_registry,
        runtime_config: base.runtime_config,
        policy_snapshot: base.policy_snapshot,
        config: base.config,
        cache,
        metrics: aster_drive::metrics_core::NoopMetrics::arc(),
        mail_sender: base.mail_sender,
        storage_change_tx: base.storage_change_tx,
        share_download_rollback: base.share_download_rollback,
        background_task_dispatch_wakeup: base.background_task_dispatch_wakeup,
        remote_protocol: base.remote_protocol,
    };
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 第一次请求（cache miss → 查 DB → 写缓存）
    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // 第二次请求（cache hit → 不查 DB）—— 功能正确即可
    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

/// admin 禁用用户后，缓存立即失效，后续请求被拒
#[actix_web::test]
async fn test_disable_user_invalidates_status_cache() {
    let cache_config = aster_drive::config::CacheConfig {
        backend: "memory".to_string(),
        default_ttl: 60,
        ..Default::default()
    };
    let cache = aster_drive::cache::create_cache(&cache_config).await;

    let base = common::setup().await;
    let state = aster_drive::runtime::PrimaryAppState {
        db_handles: base.db_handles,
        driver_registry: base.driver_registry,
        runtime_config: base.runtime_config,
        policy_snapshot: base.policy_snapshot,
        config: base.config,
        cache,
        metrics: aster_drive::metrics_core::NoopMetrics::arc(),
        mail_sender: base.mail_sender,
        storage_change_tx: base.storage_change_tx,
        share_download_rollback: base.share_download_rollback,
        background_task_dispatch_wakeup: base.background_task_dispatch_wakeup,
        remote_protocol: base.remote_protocol,
    };
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let bob_id = admin_create_user_with_credentials!(
        app,
        admin_token,
        "bobuser",
        "bob@example.com",
        "password456"
    );

    // bob 登录
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "bobuser",
            "password": "password456"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let bob_token = common::extract_cookie(&resp, "aster_access").unwrap();
    let bob_refresh = common::extract_cookie(&resp, "aster_refresh").unwrap();

    // bob 正常访问（写入缓存）
    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&bob_token)))
        .insert_header(common::csrf_header_for(&bob_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // admin 禁用 bob
    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/users/{bob_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({ "status": "disabled" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // bob 再次访问——应被拒（缓存已失效）
    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&bob_token)))
        .insert_header(common::csrf_header_for(&bob_token))
        .to_request();
    assert_service_status!(app, req, 403, "disabled user should get 403");

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("Cookie", common::refresh_cookie_header(&bob_refresh)))
        .insert_header(common::csrf_header_for(&bob_refresh))
        .to_request();
    assert_service_status!(app, req, 403, "disabled user refresh should get 403");

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/users/{bob_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({ "status": "active" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&bob_token)))
        .insert_header(common::csrf_header_for(&bob_token))
        .to_request();
    assert_service_status!(app, req, 401, "old token should stay revoked");

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("Cookie", common::refresh_cookie_header(&bob_refresh)))
        .insert_header(common::csrf_header_for(&bob_refresh))
        .to_request();
    assert_service_status!(app, req, 401, "old refresh token should stay revoked");
}

// ── Preferences endpoint tests ──

/// Set preferences via PATCH, then verify they are returned by GET /me.
#[actix_web::test]
async fn test_patch_preferences_set_and_get() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // Patch all fields
    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/preferences")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "theme_mode": "dark",
            "color_preset": "#16a34a",
            "view_mode": "grid",
            "browser_open_mode": "double_click",
            "sort_by": "size",
            "sort_order": "desc",
            "language": "zh",
            "storage_event_stream_enabled": false
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "success");
    assert_eq!(body["data"]["theme_mode"], "dark");
    assert_eq!(body["data"]["color_preset"], "#16a34a");
    assert_eq!(body["data"]["view_mode"], "grid");
    assert_eq!(body["data"]["browser_open_mode"], "double_click");
    assert_eq!(body["data"]["sort_by"], "size");
    assert_eq!(body["data"]["sort_order"], "desc");
    assert_eq!(body["data"]["language"], "zh");
    assert_eq!(body["data"]["storage_event_stream_enabled"], false);

    // Verify via GET /me
    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["preferences"]["theme_mode"], "dark");
    assert_eq!(body["data"]["preferences"]["view_mode"], "grid");
    assert_eq!(
        body["data"]["preferences"]["browser_open_mode"],
        "double_click"
    );
    assert_eq!(body["data"]["preferences"]["language"], "zh");
    assert_eq!(
        body["data"]["preferences"]["storage_event_stream_enabled"],
        false
    );
}

/// Partial PATCH only updates specified fields; others remain unchanged.
#[actix_web::test]
async fn test_patch_preferences_partial_update() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // Set initial preferences
    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/preferences")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "theme_mode": "dark",
            "view_mode": "grid",
            "browser_open_mode": "double_click"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // Partial update: only change sort_by
    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/preferences")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "sort_by": "size"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    // Previously set fields should be preserved
    assert_eq!(body["data"]["theme_mode"], "dark");
    assert_eq!(body["data"]["view_mode"], "grid");
    assert_eq!(body["data"]["browser_open_mode"], "double_click");
    // Newly set field
    assert_eq!(body["data"]["sort_by"], "size");
}

/// Invalid enum values should be rejected with a 400 error.
#[actix_web::test]
async fn test_patch_preferences_invalid_enum_value() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/preferences")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "theme_mode": "invalid_value"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400, "invalid enum value should return 400");

    // sort_order with invalid value
    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/preferences")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "sort_order": "sideways"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400, "invalid sort_order should return 400");
}

/// PATCH with empty body should succeed (no-op, returns current prefs).
#[actix_web::test]
async fn test_patch_preferences_empty_body() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // Empty body — should succeed with no changes
    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/preferences")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    // All fields should be null for a fresh user
    assert!(body["data"]["theme_mode"].is_null());
    assert!(body["data"]["color_preset"].is_null());
    assert!(body["data"]["browser_open_mode"].is_null());
    assert!(body["data"]["language"].is_null());
    assert!(body["data"]["storage_event_stream_enabled"].is_null());

    // Verify via GET /me — fresh user has no stored config so preferences is null
    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"]["preferences"].is_null());
}

/// sort_by = "type" uses a special snake_case rename; verify it round-trips correctly.
#[actix_web::test]
async fn test_patch_preferences_sort_by_type() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/preferences")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "sort_by": "type" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["sort_by"], "type");
}

#[actix_web::test]
async fn test_patch_profile_display_name_round_trip_and_clear() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/profile")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "display_name": "  Test User  "
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["display_name"], "Test User");
    assert_eq!(body["data"]["avatar"]["source"], "none");

    let (boundary, payload) = avatar_upload_payload();
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/profile/avatar/upload")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["display_name"], "Test User");
    assert_eq!(body["data"]["avatar"]["source"], "upload");

    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/profile")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "display_name": "   "
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"]["display_name"].is_null());
    assert_eq!(body["data"]["avatar"]["source"], "upload");

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"]["profile"]["display_name"].is_null());
    assert_eq!(body["data"]["profile"]["avatar"]["source"], "upload");
}

#[actix_web::test]
async fn test_change_password_rotates_session_and_updates_login_secret() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, refresh) = register_and_login!(app);

    let req = test::TestRequest::put()
        .uri("/api/v1/auth/password")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("User-Agent", "AsterDrive Password Agent/1.0"))
        .set_json(serde_json::json!({
            "current_password": "password123",
            "new_password": "newsecret456"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let rotated_access = common::extract_cookie(&resp, "aster_access").unwrap();
    let rotated_refresh = common::extract_cookie(&resp, "aster_refresh").unwrap();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "success");
    assert_eq!(body["data"]["expires_in"], 900);
    let rotated_claims = local::verify_token(&rotated_refresh, &state.config.auth.jwt_secret)
        .expect("rotated refresh token should verify");
    let rotated_jti = rotated_claims
        .jti
        .clone()
        .expect("rotated refresh token should carry jti");
    let rotated_session = auth_session_repo::find_by_refresh_jti(state.writer_db(), &rotated_jti)
        .await
        .unwrap()
        .expect("rotated auth session should exist");
    assert_eq!(
        rotated_session.user_agent.as_deref(),
        Some("AsterDrive Password Agent/1.0")
    );
    assert!(rotated_session.last_seen_at >= rotated_session.created_at);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    assert_service_status!(app, req, 401);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&rotated_access)))
        .insert_header(common::csrf_header_for(&rotated_access))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("Cookie", common::refresh_cookie_header(&refresh)))
        .insert_header(common::csrf_header_for(&refresh))
        .to_request();
    assert_service_status!(app, req, 401);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("Cookie", common::refresh_cookie_header(&rotated_refresh)))
        .insert_header(common::csrf_header_for(&rotated_refresh))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "testuser",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "testuser",
            "password": "newsecret456"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_change_password_rejects_wrong_current_password() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::put()
        .uri("/api/v1/auth/password")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "current_password": "wrongpassword",
            "new_password": "newsecret456"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "testuser",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_change_password_rejects_reusing_current_password() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::put()
        .uri("/api/v1/auth/password")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "current_password": "password123",
            "new_password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "testuser",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_forced_password_change_restricts_session_and_clears_after_update() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let user_id = admin_create_user!(
        app,
        admin_token,
        "forcepwuser",
        "forcepwuser@example.com",
        "password123"
    );
    let (old_access, old_refresh) = login_user!(app, "forcepwuser", "password123");

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/users/{user_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "must_change_password": true
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["must_change_password"], true);
    let (admin_updates, _) = audit_log_repo::find_with_filters(
        state.reader_db(),
        audit_log_repo::AuditLogQuery {
            user_id: None,
            action: Some(AuditAction::AdminUpdateUser.as_str()),
            entity_type: Some(aster_drive::types::AuditEntityType::User.as_str()),
            entity_id: Some(user_id),
            after: None,
            before: None,
            limit: 10,
            offset: 0,
            sort_by: AdminAuditLogSortBy::CreatedAt,
            sort_order: SortOrder::Desc,
        },
    )
    .await
    .unwrap();
    assert!(
        admin_updates.iter().any(|entry| entry
            .details
            .as_deref()
            .is_some_and(|details| details.contains("\"must_change_password\":true"))),
        "admin update audit details should include must_change_password=true"
    );

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&old_access)))
        .insert_header(common::csrf_header_for(&old_access))
        .to_request();
    assert_service_status!(app, req, 401, "old access token should be revoked");

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("Cookie", common::refresh_cookie_header(&old_refresh)))
        .insert_header(common::csrf_header_for(&old_refresh))
        .to_request();
    assert_service_status!(app, req, 401, "old refresh token should be revoked");

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "forcepwuser",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let password_change_access =
        common::extract_cookie(&resp, "aster_access").expect("access cookie missing");
    let password_change_refresh =
        common::extract_cookie(&resp, "aster_refresh").expect("refresh cookie missing");
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "password_change_required");
    assert_eq!(body["data"]["expires_in"], 900);

    let claims = local::verify_token(
        &password_change_access,
        state.config.auth.jwt_secret.as_str(),
    )
    .expect("password-change access token should verify");
    assert!(
        claims.password_change,
        "access token should be scoped to password change"
    );
    let password_change_refresh_claims = local::verify_token(
        &password_change_refresh,
        state.config.auth.jwt_secret.as_str(),
    )
    .expect("password-change refresh token should verify");
    let password_change_refresh_jti = password_change_refresh_claims
        .jti
        .expect("password-change refresh token should carry a jti");

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header((
            "Cookie",
            common::refresh_cookie_header(&password_change_refresh),
        ))
        .insert_header(common::csrf_header_for(&password_change_refresh))
        .to_request();
    let result = test::try_call_service(&app, req).await;
    let body = match result {
        Ok(resp) => {
            assert_eq!(resp.status(), 403);
            service_response_json(resp).await
        }
        Err(err) => {
            let resp = err.error_response();
            assert_eq!(resp.status(), 403);
            http_response_json(resp).await
        }
    };
    assert_eq!(body["code"], "auth.password_change_required");

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header((
            "Cookie",
            common::access_cookie_header(&password_change_access),
        ))
        .insert_header(common::csrf_header_for(&password_change_access))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["must_change_password"], true);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/sessions")
        .insert_header((
            "Cookie",
            common::access_cookie_header(&password_change_access),
        ))
        .insert_header(common::csrf_header_for(&password_change_access))
        .to_request();
    let result = test::try_call_service(&app, req).await;
    let body = match result {
        Ok(resp) => {
            assert_eq!(resp.status(), 403);
            service_response_json(resp).await
        }
        Err(err) => {
            let resp = err.error_response();
            assert_eq!(resp.status(), 403);
            http_response_json(resp).await
        }
    };
    assert_eq!(body["code"], "auth.password_change_required");

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/logout")
        .insert_header((
            "Cookie",
            common::access_and_refresh_cookie_header(
                &password_change_access,
                &password_change_refresh,
            ),
        ))
        .insert_header(common::csrf_header_for(&password_change_access))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        200,
        "logout should be allowed before password change"
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header((
            "Cookie",
            common::refresh_cookie_header(&password_change_refresh),
        ))
        .insert_header(common::csrf_header_for(&password_change_refresh))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        !resp.status().is_success(),
        "restricted refresh token should be rejected after logout"
    );
    let revoked_session =
        auth_session_repo::find_by_refresh_jti(state.writer_db(), &password_change_refresh_jti)
            .await
            .unwrap()
            .expect("revoked restricted session should remain as tombstone");
    assert!(
        revoked_session.revoked_at.is_some(),
        "restricted refresh token should be revoked after logout"
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "forcepwuser",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let password_change_access =
        common::extract_cookie(&resp, "aster_access").expect("access cookie missing");

    let req = test::TestRequest::put()
        .uri("/api/v1/auth/password")
        .insert_header((
            "Cookie",
            common::access_cookie_header(&password_change_access),
        ))
        .insert_header(common::csrf_header_for(&password_change_access))
        .set_json(serde_json::json!({
            "current_password": "wrongpassword",
            "new_password": "newsecret456"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        401,
        "forced change still requires the current temporary password"
    );

    let req = test::TestRequest::put()
        .uri("/api/v1/auth/password")
        .insert_header((
            "Cookie",
            common::access_cookie_header(&password_change_access),
        ))
        .insert_header(common::csrf_header_for(&password_change_access))
        .set_json(serde_json::json!({
            "current_password": "password123",
            "new_password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        400,
        "forced change must reject reusing the temporary password"
    );
    let unchanged = user_repo::find_by_id(state.writer_db(), user_id)
        .await
        .unwrap();
    assert!(
        unchanged.must_change_password,
        "failed same-password update must not clear forced password change"
    );

    let req = test::TestRequest::put()
        .uri("/api/v1/auth/password")
        .insert_header((
            "Cookie",
            common::access_cookie_header(&password_change_access),
        ))
        .insert_header(common::csrf_header_for(&password_change_access))
        .set_json(serde_json::json!({
            "current_password": "password123",
            "new_password": "newsecret456"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let normal_access = common::extract_cookie(&resp, "aster_access").unwrap();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["expires_in"], 900);

    let updated = user_repo::find_by_id(state.writer_db(), user_id)
        .await
        .unwrap();
    assert!(!updated.must_change_password);
    let (password_changes, _) = audit_log_repo::find_with_filters(
        state.reader_db(),
        audit_log_repo::AuditLogQuery {
            user_id: Some(user_id),
            action: Some(AuditAction::UserChangePassword.as_str()),
            entity_type: Some(aster_drive::types::AuditEntityType::User.as_str()),
            entity_id: None,
            after: None,
            before: None,
            limit: 10,
            offset: 0,
            sort_by: AdminAuditLogSortBy::CreatedAt,
            sort_order: SortOrder::Desc,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        password_changes.len(),
        1,
        "forced password completion should record one password-change audit log"
    );

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&normal_access)))
        .insert_header(common::csrf_header_for(&normal_access))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["must_change_password"], false);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "forcepwuser",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401, "old password should no longer work");

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "forcepwuser",
            "password": "newsecret456"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "authenticated");
}

#[actix_web::test]
async fn test_admin_can_clear_forced_password_change_before_next_login() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let user_id = admin_create_user!(
        app,
        admin_token,
        "clearforcepw",
        "clearforcepw@example.com",
        "password123"
    );

    for value in [true, false] {
        let req = test::TestRequest::patch()
            .uri(&format!("/api/v1/admin/users/{user_id}"))
            .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
            .insert_header(common::csrf_header_for(&admin_token))
            .set_json(serde_json::json!({
                "must_change_password": value
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body["data"]["must_change_password"], value);
    }
    let (admin_updates, _) = audit_log_repo::find_with_filters(
        state.reader_db(),
        audit_log_repo::AuditLogQuery {
            user_id: None,
            action: Some(AuditAction::AdminUpdateUser.as_str()),
            entity_type: Some(aster_drive::types::AuditEntityType::User.as_str()),
            entity_id: Some(user_id),
            after: None,
            before: None,
            limit: 10,
            offset: 0,
            sort_by: AdminAuditLogSortBy::CreatedAt,
            sort_order: SortOrder::Desc,
        },
    )
    .await
    .unwrap();
    assert!(
        admin_updates.iter().any(|entry| entry
            .details
            .as_deref()
            .is_some_and(|details| details.contains("\"must_change_password\":true"))),
        "setting forced password-change should be recorded in audit details"
    );
    assert!(
        admin_updates.iter().any(|entry| entry
            .details
            .as_deref()
            .is_some_and(|details| details.contains("\"must_change_password\":false"))),
        "clearing forced password-change should be recorded in audit details"
    );

    let user = user_repo::find_by_id(state.writer_db(), user_id)
        .await
        .unwrap();
    assert!(!user.must_change_password);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "clearforcepw",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "authenticated");
}

#[actix_web::test]
async fn test_patch_profile_rejects_overlong_display_name() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/profile")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "display_name": "a".repeat(65)
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"]["profile"]["display_name"].is_null());
}

#[actix_web::test]
async fn test_display_name_survives_avatar_source_switches() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/profile")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "display_name": "Avatar User"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let (boundary, payload) = avatar_upload_payload();
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/profile/avatar/upload")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["display_name"], "Avatar User");
    assert_eq!(body["data"]["avatar"]["source"], "upload");

    for source in ["gravatar", "none"] {
        let req = test::TestRequest::put()
            .uri("/api/v1/auth/profile/avatar/source")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({ "source": source }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body["data"]["display_name"], "Avatar User");
        assert_eq!(body["data"]["avatar"]["source"], source);
    }
}

#[actix_web::test]
async fn test_avatar_upload_and_source_switch() {
    let state = common::setup().await;
    let avatar_base_path = state
        .runtime_config
        .get(aster_drive::config::avatar::AVATAR_DIR_KEY)
        .expect("avatar_dir should exist");
    let shared_policy_base_path =
        aster_drive::db::repository::policy_repo::find_default(state.writer_db())
            .await
            .unwrap()
            .expect("default policy should exist")
            .base_path;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let user_id = body["data"]["id"].as_i64().unwrap();

    let (boundary, payload) = avatar_upload_payload();
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/profile/avatar/upload")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["avatar"]["source"], "upload");
    assert_eq!(body["data"]["avatar"]["version"], 1);
    assert_eq!(
        body["data"]["avatar"]["url_512"],
        "/auth/profile/avatar/512?v=1"
    );
    let avatar_user_dir =
        std::path::PathBuf::from(&avatar_base_path).join(format!("user/{user_id}"));
    let avatar_v1_dir = avatar_user_dir.join("v1");
    let avatar_v1_512 = avatar_v1_dir.join("512.webp");
    let avatar_v1_1024 = avatar_v1_dir.join("1024.webp");
    assert!(avatar_v1_512.exists());
    assert!(avatar_v1_1024.exists());
    assert!(
        !std::path::PathBuf::from(&shared_policy_base_path)
            .join(format!("user/{user_id}/v1/512.webp"))
            .exists()
    );
    assert!(
        !std::path::PathBuf::from(&shared_policy_base_path)
            .join(format!("user/{user_id}/v1/1024.webp"))
            .exists()
    );

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/profile/avatar/512")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("content-type").unwrap(), "image/webp");

    let req = test::TestRequest::put()
        .uri("/api/v1/auth/profile/avatar/source")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "source": "gravatar"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["avatar"]["source"], "gravatar");
    assert_eq!(body["data"]["avatar"]["version"], 2);
    assert!(
        body["data"]["avatar"]["url_512"]
            .as_str()
            .unwrap()
            .contains("gravatar.com/avatar/")
    );
    assert!(!avatar_v1_512.exists());
    assert!(!avatar_v1_1024.exists());
    assert!(!avatar_v1_dir.exists());
    assert!(!avatar_user_dir.exists());

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/profile/avatar/512")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_avatar_read_rejects_tampered_stored_key() {
    use aster_drive::entities::user_profile;
    use sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, Set};

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let avatar_base_path = state
        .runtime_config
        .get(aster_drive::config::avatar::AVATAR_DIR_KEY)
        .expect("avatar_dir should exist");
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let user_id = body["data"]["id"].as_i64().unwrap();

    let (boundary, payload) = avatar_upload_payload();
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/profile/avatar/upload")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let outside_dir = std::path::PathBuf::from(&avatar_base_path)
        .parent()
        .expect("avatar_dir should have parent")
        .join("outside-avatar");
    std::fs::create_dir_all(&outside_dir).unwrap();
    std::fs::write(outside_dir.join("512.webp"), b"not this user's avatar").unwrap();

    let profile = user_profile::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .unwrap()
        .expect("profile should exist after avatar upload");
    let mut active = profile.into_active_model();
    active.avatar_key = Set(Some(outside_dir.to_string_lossy().into_owned()));
    active.update(&db).await.unwrap();

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/profile/avatar/512")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_avatar_reupload_replaces_previous_objects() {
    let state = common::setup().await;
    let avatar_base_path = state
        .runtime_config
        .get(aster_drive::config::avatar::AVATAR_DIR_KEY)
        .expect("avatar_dir should exist");
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let user_id = body["data"]["id"].as_i64().unwrap();

    let (boundary, payload) = avatar_upload_payload();
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/profile/avatar/upload")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let avatar_user_dir =
        std::path::PathBuf::from(&avatar_base_path).join(format!("user/{user_id}"));
    let avatar_v1_dir = avatar_user_dir.join("v1");
    let avatar_v1_512 = avatar_v1_dir.join("512.webp");
    let avatar_v1_1024 = avatar_v1_dir.join("1024.webp");
    assert!(avatar_v1_512.exists());
    assert!(avatar_v1_1024.exists());

    let (boundary, payload) = avatar_upload_payload();
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/profile/avatar/upload")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["avatar"]["source"], "upload");
    assert_eq!(body["data"]["avatar"]["version"], 2);
    assert_eq!(
        body["data"]["avatar"]["url_512"],
        "/auth/profile/avatar/512?v=2"
    );

    let avatar_v2_dir = avatar_user_dir.join("v2");
    let avatar_v2_512 = avatar_v2_dir.join("512.webp");
    let avatar_v2_1024 = avatar_v2_dir.join("1024.webp");
    assert!(!avatar_v1_512.exists());
    assert!(!avatar_v1_1024.exists());
    assert!(!avatar_v1_dir.exists());
    assert!(avatar_user_dir.exists());
    assert!(avatar_v2_dir.exists());
    assert!(avatar_v2_512.exists());
    assert!(avatar_v2_1024.exists());
}

#[actix_web::test]
async fn test_avatar_switch_to_none_deletes_uploaded_objects() {
    let state = common::setup().await;
    let avatar_base_path = state
        .runtime_config
        .get(aster_drive::config::avatar::AVATAR_DIR_KEY)
        .expect("avatar_dir should exist");
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let user_id = body["data"]["id"].as_i64().unwrap();

    let (boundary, payload) = avatar_upload_payload();
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/profile/avatar/upload")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let avatar_user_dir =
        std::path::PathBuf::from(&avatar_base_path).join(format!("user/{user_id}"));
    let avatar_v1_dir = avatar_user_dir.join("v1");
    let avatar_v1_512 = avatar_v1_dir.join("512.webp");
    let avatar_v1_1024 = avatar_v1_dir.join("1024.webp");
    assert!(avatar_v1_512.exists());
    assert!(avatar_v1_1024.exists());

    let req = test::TestRequest::put()
        .uri("/api/v1/auth/profile/avatar/source")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "source": "none"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["avatar"]["source"], "none");
    assert_eq!(body["data"]["avatar"]["version"], 2);
    assert!(body["data"]["avatar"]["url_512"].is_null());

    assert!(!avatar_v1_512.exists());
    assert!(!avatar_v1_1024.exists());
    assert!(!avatar_v1_dir.exists());
    assert!(!avatar_user_dir.exists());

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/profile/avatar/512")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_passkey_register_login_and_replay_protection() {
    let state = common::setup_with_memory_cache().await;
    configure_passkey_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (access, _) = register_and_login!(app);

    let (mut softpasskey, passkey) = register_test_passkey(&app, &access, "Laptop").await;
    assert_eq!(passkey["name"], "Laptop");
    assert_eq!(passkey["sign_count"], 0);
    let user_id = testuser_id(state.writer_db()).await;
    let passkey_id = passkey["id"].as_i64().unwrap();
    let stored_passkey = passkey_repo::find_by_id_for_user(state.writer_db(), passkey_id, user_id)
        .await
        .unwrap()
        .unwrap();

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/passkeys")
        .insert_header(("Cookie", common::access_cookie_header(&access)))
        .insert_header(common::csrf_header_for(&access))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"][0]["name"], "Laptop");

    let (flow_id, challenge) = passkey_login_start(&app, Some("testuser")).await;
    assert!(challenge.public_key.allow_credentials.is_empty());
    let (challenge, user_handle) = allow_test_passkey_credential(challenge, &stored_passkey);
    let mut credential = softpasskey
        .do_authentication(Url::parse(TEST_BROWSER_ORIGIN).unwrap(), challenge)
        .expect("soft passkey authentication should succeed");
    credential.response.user_handle = Some(user_handle.as_bytes().to_vec());
    let credential = serde_json::to_value(credential).expect("credential should serialize");
    let replay_credential = credential.clone();

    let resp = passkey_login_finish(&app, &flow_id, credential).await;
    assert_eq!(resp.status(), 200);
    let login_access = common::extract_cookie(&resp, "aster_access").unwrap();
    let login_refresh = common::extract_cookie(&resp, "aster_refresh").unwrap();
    assert!(!login_access.is_empty());
    assert!(!login_refresh.is_empty());
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "success");

    let sessions = auth_session_repo::list_active_for_user(state.writer_db(), user_id)
        .await
        .unwrap();
    assert_eq!(sessions.len(), 2);
    assert!(
        sessions
            .iter()
            .any(|session| session.user_agent.as_deref() == Some("AsterDrive Passkey Test/1.0"))
    );

    let resp = passkey_login_finish(&app, &flow_id, replay_credential).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn test_passkey_login_without_identifier() {
    let state = common::setup_with_memory_cache().await;
    configure_passkey_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (access, _) = register_and_login!(app);

    let (mut softpasskey, passkey) = register_test_passkey(&app, &access, "Laptop").await;
    let user_id = testuser_id(state.writer_db()).await;
    let passkey_id = passkey["id"].as_i64().unwrap();
    let stored_passkey = passkey_repo::find_by_id_for_user(state.writer_db(), passkey_id, user_id)
        .await
        .unwrap()
        .unwrap();
    let credential_id = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(stored_passkey.credential_id.as_bytes())
        .unwrap();
    let user_handle = uuid::Uuid::parse_str(&stored_passkey.user_handle).unwrap();

    let (flow_id, mut challenge) = passkey_login_start(&app, None).await;
    assert!(challenge.public_key.allow_credentials.is_empty());
    assert!(challenge.mediation.is_none());
    challenge.public_key.allow_credentials = vec![AllowCredentials {
        type_: "public-key".to_string(),
        id: credential_id,
        transports: None,
    }];
    let mut credential = softpasskey
        .do_authentication(Url::parse(TEST_BROWSER_ORIGIN).unwrap(), challenge)
        .expect("soft passkey authentication should succeed");
    credential.response.user_handle = Some(user_handle.as_bytes().to_vec());
    let credential = serde_json::to_value(credential).expect("credential should serialize");

    let resp = passkey_login_finish(&app, &flow_id, credential).await;
    assert_eq!(resp.status(), 200);
    assert!(common::extract_cookie(&resp, "aster_access").is_some());
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "success");
}

#[actix_web::test]
async fn test_passkey_login_policy_disables_start_and_finish_without_deleting_credentials() {
    let state = common::setup_with_memory_cache().await;
    configure_passkey_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (access, _) = register_and_login!(app);

    let (mut softpasskey, passkey) = register_test_passkey(&app, &access, "Laptop").await;
    let user_id = testuser_id(state.writer_db()).await;
    let passkey_id = passkey["id"].as_i64().unwrap();
    let stored_passkey = passkey_repo::find_by_id_for_user(state.writer_db(), passkey_id, user_id)
        .await
        .unwrap()
        .expect("registered passkey should remain stored");

    let (flow_id, challenge) = passkey_login_start(&app, Some("testuser")).await;
    let (challenge, user_handle) = allow_test_passkey_credential(challenge, &stored_passkey);
    let mut credential = softpasskey
        .do_authentication(Url::parse(TEST_BROWSER_ORIGIN).unwrap(), challenge)
        .expect("soft passkey authentication should succeed");
    credential.response.user_handle = Some(user_handle.as_bytes().to_vec());
    let credential = serde_json::to_value(credential).expect("credential should serialize");

    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::auth_runtime::AUTH_PASSKEY_LOGIN_ENABLED_KEY,
        "false",
    ));

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/passkeys/login/start")
        .insert_header(("Origin", TEST_BROWSER_ORIGIN))
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({ "identifier": "testuser" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "auth.passkey_login_disabled");
    assert_eq!(
        body["msg"],
        "passkey login is disabled by administrator policy"
    );
    assert_eq!(body["error"]["retryable"], false);
    assert!(body["error"].get("subcode").is_none());

    let resp = passkey_login_finish(&app, &flow_id, credential).await;
    assert_eq!(resp.status(), 403);
    assert!(common::extract_cookie(&resp, "aster_access").is_none());
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "auth.passkey_login_disabled");

    let stored_after_disable =
        passkey_repo::find_by_id_for_user(state.writer_db(), passkey_id, user_id)
            .await
            .unwrap();
    assert!(
        stored_after_disable.is_some(),
        "disabling passkey login must not delete registered credentials"
    );
}

#[actix_web::test]
async fn test_passkey_login_policy_hot_update_reenables_login() {
    let state = common::setup_with_memory_cache().await;
    configure_passkey_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (access, _) = register_and_login!(app);

    let (mut softpasskey, passkey) = register_test_passkey(&app, &access, "Laptop").await;
    let user_id = testuser_id(state.writer_db()).await;
    let passkey_id = passkey["id"].as_i64().unwrap();
    let stored_passkey = passkey_repo::find_by_id_for_user(state.writer_db(), passkey_id, user_id)
        .await
        .unwrap()
        .expect("registered passkey should remain stored");

    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::auth_runtime::AUTH_PASSKEY_LOGIN_ENABLED_KEY,
        "false",
    ));
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/passkeys/login/start")
        .insert_header(("Origin", TEST_BROWSER_ORIGIN))
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({ "conditional": true }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::auth_runtime::AUTH_PASSKEY_LOGIN_ENABLED_KEY,
        "true",
    ));
    let (flow_id, challenge) = passkey_login_start(&app, Some("testuser")).await;
    let (challenge, user_handle) = allow_test_passkey_credential(challenge, &stored_passkey);
    let mut credential = softpasskey
        .do_authentication(Url::parse(TEST_BROWSER_ORIGIN).unwrap(), challenge)
        .expect("soft passkey authentication should succeed");
    credential.response.user_handle = Some(user_handle.as_bytes().to_vec());
    let credential = serde_json::to_value(credential).expect("credential should serialize");

    let resp = passkey_login_finish(&app, &flow_id, credential).await;
    assert_eq!(resp.status(), 200);
    assert!(common::extract_cookie(&resp, "aster_access").is_some());
}

#[actix_web::test]
async fn test_passkey_conditional_login_preserves_mediation() {
    let state = common::setup_with_memory_cache().await;
    configure_passkey_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (access, _) = register_and_login!(app);

    let (mut softpasskey, passkey) = register_test_passkey(&app, &access, "Laptop").await;
    let user_id = testuser_id(state.writer_db()).await;
    let passkey_id = passkey["id"].as_i64().unwrap();
    let stored_passkey = passkey_repo::find_by_id_for_user(state.writer_db(), passkey_id, user_id)
        .await
        .unwrap()
        .unwrap();

    let (flow_id, challenge) = conditional_passkey_login_start(&app).await;
    assert!(challenge.public_key.allow_credentials.is_empty());
    assert!(matches!(challenge.mediation, Some(Mediation::Conditional)));

    let (challenge, user_handle) = allow_test_passkey_credential(challenge, &stored_passkey);
    let mut credential = softpasskey
        .do_authentication(Url::parse(TEST_BROWSER_ORIGIN).unwrap(), challenge)
        .expect("soft passkey authentication should succeed");
    credential.response.user_handle = Some(user_handle.as_bytes().to_vec());
    let credential = serde_json::to_value(credential).expect("credential should serialize");

    let resp = passkey_login_finish(&app, &flow_id, credential).await;
    assert_eq!(resp.status(), 200);
    assert!(common::extract_cookie(&resp, "aster_access").is_some());
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "success");
}

#[actix_web::test]
async fn test_passkey_login_start_does_not_reveal_unknown_identifier() {
    let state = common::setup_with_memory_cache().await;
    configure_passkey_public_site_url(&state);
    let app = create_test_app!(state.clone());

    let (_flow_id, challenge) = passkey_login_start(&app, Some("missing-user")).await;
    assert!(challenge.public_key.allow_credentials.is_empty());
    assert!(challenge.mediation.is_none());
}

#[actix_web::test]
async fn test_passkey_login_rejects_identifier_mismatch() {
    let state = common::setup_with_memory_cache().await;
    configure_passkey_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (access, _) = register_and_login!(app);

    let (mut softpasskey, passkey) = register_test_passkey(&app, &access, "Laptop").await;
    let user_id = testuser_id(state.writer_db()).await;
    let passkey_id = passkey["id"].as_i64().unwrap();
    let stored_passkey = passkey_repo::find_by_id_for_user(state.writer_db(), passkey_id, user_id)
        .await
        .unwrap()
        .unwrap();

    let (flow_id, challenge) = passkey_login_start(&app, Some("otheruser")).await;
    assert!(challenge.public_key.allow_credentials.is_empty());
    let (challenge, user_handle) = allow_test_passkey_credential(challenge, &stored_passkey);
    let mut credential = softpasskey
        .do_authentication(Url::parse(TEST_BROWSER_ORIGIN).unwrap(), challenge)
        .expect("soft passkey authentication should succeed");
    credential.response.user_handle = Some(user_handle.as_bytes().to_vec());
    let credential = serde_json::to_value(credential).expect("credential should serialize");

    let resp = passkey_login_finish(&app, &flow_id, credential).await;
    assert_eq!(resp.status(), 401);
    assert!(common::extract_cookie(&resp, "aster_access").is_none());
}

#[actix_web::test]
async fn test_passkey_registration_finish_is_one_time() {
    let state = common::setup_with_memory_cache().await;
    configure_passkey_public_site_url(&state);
    let app = create_test_app!(state);
    let (access, _) = register_and_login!(app);

    let origin = Url::parse(TEST_BROWSER_ORIGIN).unwrap();
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/passkeys/register/start")
        .insert_header(("Cookie", common::access_cookie_header(&access)))
        .insert_header(common::csrf_header_for(&access))
        .set_json(serde_json::json!({ "name": "Laptop" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let start_body: Value = test::read_body_json(resp).await;
    let flow_id = start_body["data"]["flow_id"].as_str().unwrap().to_string();
    let mut challenge = serde_json::from_value::<CreationChallengeResponse>(
        start_body["data"]["public_key"].clone(),
    )
    .unwrap();
    let selection = challenge
        .public_key
        .authenticator_selection
        .as_ref()
        .expect("registration should include authenticator selection");
    assert_eq!(
        selection.resident_key,
        Some(ResidentKeyRequirement::Required)
    );
    assert!(selection.require_resident_key);
    let selection = challenge
        .public_key
        .authenticator_selection
        .as_mut()
        .expect("registration should include authenticator selection");
    selection.resident_key = Some(ResidentKeyRequirement::Discouraged);
    selection.require_resident_key = false;

    let mut softpasskey = SoftPasskey::new(true);
    let credential = softpasskey
        .do_registration(origin, challenge)
        .expect("soft passkey registration should succeed");
    let credential = serde_json::to_value(credential).expect("credential should serialize");

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/passkeys/register/finish")
        .insert_header(("Cookie", common::access_cookie_header(&access)))
        .insert_header(common::csrf_header_for(&access))
        .set_json(serde_json::json!({
            "flow_id": flow_id,
            "credential": credential.clone(),
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/passkeys/register/finish")
        .insert_header(("Cookie", common::access_cookie_header(&access)))
        .insert_header(common::csrf_header_for(&access))
        .set_json(serde_json::json!({
            "flow_id": flow_id,
            "credential": credential,
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn test_passkey_delete_prevents_future_login() {
    let state = common::setup_with_memory_cache().await;
    configure_passkey_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (access, _) = register_and_login!(app);

    let (mut softpasskey, passkey) = register_test_passkey(&app, &access, "Laptop").await;
    let user_id = testuser_id(state.writer_db()).await;
    let passkey_id = passkey["id"].as_i64().unwrap();
    let stored_passkey = passkey_repo::find_by_id_for_user(state.writer_db(), passkey_id, user_id)
        .await
        .unwrap()
        .unwrap();

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/auth/passkeys/{passkey_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&access)))
        .insert_header(common::csrf_header_for(&access))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/passkeys")
        .insert_header(("Cookie", common::access_cookie_header(&access)))
        .insert_header(common::csrf_header_for(&access))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"].as_array().unwrap().is_empty());

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/passkeys/login/start")
        .insert_header(("Origin", TEST_BROWSER_ORIGIN))
        .set_json(serde_json::json!({ "identifier": "testuser" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let start_body: Value = test::read_body_json(resp).await;
    let flow_id = start_body["data"]["flow_id"].as_str().unwrap().to_string();
    let challenge = serde_json::from_value::<RequestChallengeResponse>(
        start_body["data"]["public_key"].clone(),
    )
    .unwrap();
    assert!(challenge.public_key.allow_credentials.is_empty());

    let (challenge, user_handle) = allow_test_passkey_credential(challenge, &stored_passkey);
    let mut credential = softpasskey
        .do_authentication(Url::parse(TEST_BROWSER_ORIGIN).unwrap(), challenge)
        .expect("soft passkey authentication should succeed");
    credential.response.user_handle = Some(user_handle.as_bytes().to_vec());
    let credential = serde_json::to_value(credential).expect("credential should serialize");

    let resp = passkey_login_finish(&app, &flow_id, credential).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn test_passkey_disabled_user_cannot_login() {
    use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};

    let state = common::setup_with_memory_cache().await;
    configure_passkey_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (access, _) = register_and_login!(app);

    let (mut softpasskey, passkey) = register_test_passkey(&app, &access, "Laptop").await;
    let user_id = testuser_id(state.writer_db()).await;
    let passkey_id = passkey["id"].as_i64().unwrap();
    let stored_passkey = passkey_repo::find_by_id_for_user(state.writer_db(), passkey_id, user_id)
        .await
        .unwrap()
        .unwrap();
    let mut user = user_repo::find_by_id(state.writer_db(), 1)
        .await
        .unwrap()
        .into_active_model();
    user.status = Set(UserStatus::Disabled);
    user.update(state.writer_db()).await.unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/passkeys/login/start")
        .insert_header(("Origin", TEST_BROWSER_ORIGIN))
        .set_json(serde_json::json!({ "identifier": "testuser" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let start_body: Value = test::read_body_json(resp).await;
    let flow_id = start_body["data"]["flow_id"].as_str().unwrap().to_string();
    let challenge = serde_json::from_value::<RequestChallengeResponse>(
        start_body["data"]["public_key"].clone(),
    )
    .unwrap();
    assert!(challenge.public_key.allow_credentials.is_empty());

    let (challenge, user_handle) = allow_test_passkey_credential(challenge, &stored_passkey);
    let mut credential = softpasskey
        .do_authentication(Url::parse(TEST_BROWSER_ORIGIN).unwrap(), challenge)
        .expect("soft passkey authentication should succeed");
    credential.response.user_handle = Some(user_handle.as_bytes().to_vec());
    let credential = serde_json::to_value(credential).expect("credential should serialize");

    let resp = passkey_login_finish(&app, &flow_id, credential).await;
    assert_eq!(resp.status(), 403);
}

/// Unauthenticated requests to PATCH /preferences should be rejected.
#[actix_web::test]
async fn test_patch_preferences_unauthenticated() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/preferences")
        .set_json(serde_json::json!({
            "theme_mode": "dark"
        }))
        .to_request();
    let result = test::try_call_service(&app, req).await;
    match result {
        Ok(resp) => assert_eq!(resp.status(), 401),
        Err(err) => assert_eq!(err.error_response().status(), 401),
    }
}

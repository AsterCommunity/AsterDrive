mod common;

use actix_web::{body::MessageBody, http::StatusCode, test};
use aster_drive::config::{auth_runtime, mail};
use aster_drive::entities::audit_log;
use aster_drive::runtime::SharedRuntimeState;
use aster_drive::services::mfa_service::totp;
use aster_drive::types::AuditAction;
use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, PaginatorTrait, QueryFilter,
    QueryOrder, Set,
};
use serde_json::Value;
use std::{any::Any, sync::Arc};

#[derive(Default)]
struct FailingMailSender;

#[async_trait::async_trait]
impl aster_drive::services::mail_service::MailSender for FailingMailSender {
    async fn send(
        &self,
        _message: aster_drive::services::mail_service::MailMessage,
    ) -> aster_drive::errors::Result<()> {
        Err(aster_drive::errors::AsterError::mail_delivery_failed(
            "forced mail delivery failure",
        ))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

async fn register_user<S, B, E>(app: &S, username: &str, email: &str, password: &str)
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
        .uri("/api/v1/auth/register")
        .set_json(serde_json::json!({
            "username": username,
            "email": email,
            "password": password
        }))
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
}

async fn login_raw<S, B, E>(
    app: &S,
    identifier: &str,
    password: &str,
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
        .uri("/api/v1/auth/login")
        .set_json(serde_json::json!({
            "identifier": identifier,
            "password": password
        }))
        .to_request();
    test::call_service(app, req).await
}

async fn enable_totp<S, B, E>(app: &S, access: &str) -> (i64, String, Vec<String>)
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
        .uri("/api/v1/auth/mfa/totp/setup/start")
        .insert_header(("Cookie", common::access_cookie_header(access)))
        .insert_header(common::csrf_header_for(access))
        .to_request();
    let resp = test::call_service(app, req).await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "{body:#?}");
    let flow_token = body["data"]["flow_token"].as_str().unwrap().to_string();
    let secret = body["data"]["secret"].as_str().unwrap().to_string();
    let code = totp_code(&secret);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/mfa/totp/setup/finish")
        .insert_header(("Cookie", common::access_cookie_header(access)))
        .insert_header(common::csrf_header_for(access))
        .set_json(serde_json::json!({
            "flow_token": flow_token,
            "code": code,
            "name": "Phone"
        }))
        .to_request();
    let resp = test::call_service(app, req).await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "{body:#?}");
    let factor_id = body["data"]["factor"]["id"].as_i64().unwrap();
    let recovery_codes = body["data"]["recovery_codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_string())
        .collect();
    (factor_id, secret, recovery_codes)
}

async fn verify_mfa<S, B, E>(
    app: &S,
    flow_token: &str,
    method: &str,
    code: &str,
) -> actix_web::dev::ServiceResponse<B>
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = E,
        >,
    E: std::fmt::Debug,
{
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/mfa/challenge/verify")
        .set_json(serde_json::json!({
            "flow_token": flow_token,
            "method": method,
            "code": code
        }))
        .to_request();
    test::call_service(app, req).await
}

async fn send_email_code<S, B, E>(app: &S, flow_token: &str) -> actix_web::dev::ServiceResponse<B>
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
        .uri("/api/v1/auth/mfa/challenge/email-code/send")
        .set_json(serde_json::json!({
            "flow_token": flow_token
        }))
        .to_request();
    test::call_service(app, req).await
}

fn apply_email_code_login_config(
    state: &aster_drive::runtime::PrimaryAppState,
    allow_totp_fallback: bool,
) {
    state.runtime_config.apply(common::system_config_model(
        auth_runtime::AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY,
        "true",
    ));
    state.runtime_config.apply(common::system_config_model(
        auth_runtime::AUTH_EMAIL_CODE_LOGIN_ALLOW_TOTP_FALLBACK_KEY,
        if allow_totp_fallback { "true" } else { "false" },
    ));
    state.runtime_config.apply(common::system_config_model(
        mail::MAIL_SMTP_HOST_KEY,
        "smtp.example.com",
    ));
    state.runtime_config.apply(common::system_config_model(
        mail::MAIL_FROM_ADDRESS_KEY,
        "noreply@example.com",
    ));
    state.runtime_config.apply(common::system_config_model(
        mail::MAIL_FROM_NAME_KEY,
        "Aster Test",
    ));
}

fn apply_mail_config(state: &aster_drive::runtime::PrimaryAppState) {
    state.runtime_config.apply(common::system_config_model(
        mail::MAIL_SMTP_HOST_KEY,
        "smtp.example.com",
    ));
    state.runtime_config.apply(common::system_config_model(
        mail::MAIL_FROM_ADDRESS_KEY,
        "noreply@example.com",
    ));
    state.runtime_config.apply(common::system_config_model(
        mail::MAIL_FROM_NAME_KEY,
        "Aster Test",
    ));
}

fn extract_email_code(content: &str) -> Option<String> {
    let mut current = String::new();
    for ch in content.chars() {
        if ch.is_ascii_digit() {
            current.push(ch);
            if current.len() == 8 {
                return Some(current);
            }
        } else {
            current.clear();
        }
    }
    None
}

fn extract_email_code_from_message(
    message: &aster_drive::services::mail_service::MailMessage,
) -> String {
    extract_email_code(&message.text_body)
        .or_else(|| extract_email_code(&message.html_body))
        .expect("email MFA message should contain an 8 digit code")
}

fn totp_code(secret: &str) -> String {
    let secret_bytes = totp::decode_secret(secret).unwrap();
    totp::code_for_time(&secret_bytes, chrono::Utc::now()).unwrap()
}

fn different_email_code(code: &str) -> String {
    let mut bytes = code.as_bytes().to_vec();
    bytes[0] = if bytes[0] == b'0' { b'1' } else { b'0' };
    String::from_utf8(bytes).unwrap()
}

async fn find_mfa_flow_by_token(
    db: &sea_orm::DatabaseConnection,
    flow_token: &str,
) -> aster_drive::entities::mfa_login_flow::Model {
    let flow_hash = aster_drive::utils::hash::sha256_hex(flow_token.as_bytes());
    aster_drive::entities::mfa_login_flow::Entity::find()
        .filter(aster_drive::entities::mfa_login_flow::Column::FlowTokenHash.eq(flow_hash))
        .one(db)
        .await
        .unwrap()
        .unwrap()
}

async fn audit_entries(
    db: &sea_orm::DatabaseConnection,
    action: AuditAction,
) -> Vec<audit_log::Model> {
    audit_log::Entity::find()
        .filter(audit_log::Column::Action.eq(action))
        .order_by_asc(audit_log::Column::Id)
        .all(db)
        .await
        .expect("audit query should succeed")
}

async fn audit_count(db: &sea_orm::DatabaseConnection, action: AuditAction) -> u64 {
    audit_log::Entity::find()
        .filter(audit_log::Column::Action.eq(action))
        .count(db)
        .await
        .expect("audit count should succeed")
}

#[tokio::test]
async fn test_password_login_without_mfa_still_sets_cookies() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    register_user(&app, "plainuser", "plain@example.com", "password123").await;

    let resp = login_raw(&app, "plainuser", "password123").await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(common::extract_cookie(&resp, "aster_access").is_some());
    assert!(common::extract_cookie(&resp, "aster_refresh").is_some());
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "authenticated");
}

#[tokio::test]
async fn test_email_code_login_requires_auth_toggle_and_mail_configuration() {
    let state = common::setup().await;
    apply_mail_config(&state);
    let app = create_test_app!(state);
    register_user(&app, "defaultoff", "defaultoff@example.com", "password123").await;

    let resp = login_raw(&app, "defaultoff", "password123").await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(common::extract_cookie(&resp, "aster_access").is_some());
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "authenticated");

    let state = common::setup().await;
    state.runtime_config.apply(common::system_config_model(
        auth_runtime::AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY,
        "true",
    ));
    let app = create_test_app!(state);
    register_user(&app, "nomailmfa", "nomailmfa@example.com", "password123").await;

    let resp = login_raw(&app, "nomailmfa", "password123").await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(common::extract_cookie(&resp, "aster_access").is_some());
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "authenticated");
}

#[tokio::test]
async fn test_email_code_login_requires_verified_email() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (admin_access, _) = register_and_login!(app);
    let user_id = admin_create_user!(
        app,
        &admin_access,
        "unverifiedmfa",
        "unverifiedmfa@example.com",
        "password123"
    );

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/users/{user_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_access)))
        .insert_header(common::csrf_header_for(&admin_access))
        .set_json(serde_json::json!({ "email_verified": false }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    apply_email_code_login_config(&state, false);
    let resp = login_raw(&app, "unverifiedmfa", "password123").await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body:#?}");
    assert_eq!(body["code"], "auth.pending_activation");
}

#[tokio::test]
async fn test_email_code_send_rejects_missing_invalid_and_unavailable_flows() {
    let state = common::setup().await;
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state.clone());
    register_user(
        &app,
        "emailunavailable",
        "emailunavailable@example.com",
        "password123",
    )
    .await;
    let (access, _) = login_user!(app, "emailunavailable", "password123");
    let _ = enable_totp(&app, &access).await;

    apply_email_code_login_config(&state, false);

    let resp = send_email_code(&app, "   ").await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body:#?}");
    assert_eq!(body["code"], "auth.mfa_flow_invalid");

    let resp = send_email_code(&app, "not-a-real-flow").await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body:#?}");
    assert_eq!(body["code"], "auth.mfa_flow_invalid");

    let resp = login_raw(&app, "emailunavailable", "password123").await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["methods"],
        serde_json::json!(["totp", "recovery_code"])
    );
    let flow_token = body["data"]["flow_token"].as_str().unwrap().to_string();
    let resp = send_email_code(&app, &flow_token).await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body:#?}");
    assert_eq!(body["code"], "auth.mfa_factor_required");

    let memory_sender = aster_drive::services::mail_service::memory_sender_ref(&mail_sender)
        .expect("memory mail sender should be available in tests");
    assert!(memory_sender.messages().is_empty());
}

#[tokio::test]
async fn test_email_code_login_send_and_verify_sets_cookies() {
    let state = common::setup().await;
    apply_email_code_login_config(&state, false);
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);
    register_user(&app, "emailmfa", "emailmfa@example.com", "password123").await;

    let resp = login_raw(&app, "emailmfa", "password123").await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(common::extract_cookie(&resp, "aster_access").is_none());
    assert!(common::extract_cookie(&resp, "aster_refresh").is_none());
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "mfa_required");
    assert_eq!(body["data"]["methods"], serde_json::json!(["email_code"]));
    let flow_token = body["data"]["flow_token"].as_str().unwrap().to_string();

    let resp = send_email_code(&app, &flow_token).await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "{body:#?}");
    let expires_in = body["data"]["expires_in"].as_u64().unwrap();
    assert!(expires_in <= 300, "{body:#?}");
    assert!(expires_in > 0, "{body:#?}");
    assert_eq!(body["data"]["resend_after"], 60);

    let memory_sender = aster_drive::services::mail_service::memory_sender_ref(&mail_sender)
        .expect("memory mail sender should be available in tests");
    let messages = memory_sender.messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].to.address, "emailmfa@example.com");
    assert!(
        !messages[0].text_body.contains("10 minutes"),
        "{:#?}",
        messages[0]
    );
    assert!(
        !messages[0].html_body.contains("10 minutes"),
        "{:#?}",
        messages[0]
    );
    let code = extract_email_code_from_message(&messages[0]);
    assert_eq!(code.len(), 8);
    assert!(code.chars().all(|ch| ch.is_ascii_digit()));

    let resp = verify_mfa(&app, &flow_token, "email_code", &code).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(common::extract_cookie(&resp, "aster_access").is_some());
    assert!(common::extract_cookie(&resp, "aster_refresh").is_some());
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "authenticated");

    let resp = verify_mfa(&app, &flow_token, "email_code", &code).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "auth.mfa_flow_invalid");
}

#[tokio::test]
async fn test_email_code_send_records_mail_delivery_and_security_audits() {
    let state = common::setup().await;
    apply_email_code_login_config(&state, false);
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    register_user(&app, "emailaudit", "emailaudit@example.com", "password123").await;

    let resp = login_raw(&app, "emailaudit", "password123").await;
    let body: Value = test::read_body_json(resp).await;
    let flow_token = body["data"]["flow_token"].as_str().unwrap().to_string();

    let resp = send_email_code(&app, &flow_token).await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "{body:#?}");

    let flow = find_mfa_flow_by_token(&db, &flow_token).await;
    let mail_entries = audit_entries(&db, AuditAction::MailSend).await;
    assert_eq!(mail_entries.len(), 1);
    let mail_entry = &mail_entries[0];
    assert_eq!(mail_entry.user_id, flow.user_id);
    assert_eq!(mail_entry.entity_type, "mail");
    assert_eq!(mail_entry.entity_id, None);
    let details: Value = serde_json::from_str(mail_entry.details.as_deref().unwrap()).unwrap();
    assert_eq!(details["to_address"], "emailaudit@example.com");
    assert_eq!(details["template_code"], "login_email_code");
    assert_eq!(details["to_name"], "emailaudit");
    assert!(details.get("outbox_id").is_none());
    assert!(details.get("attempt_count").is_none());
    assert!(details.get("error").is_none());

    let security_entries = audit_entries(&db, AuditAction::UserMfaEmailCodeSend).await;
    assert_eq!(security_entries.len(), 1);
    assert_eq!(security_entries[0].user_id, flow.user_id);
    assert_eq!(security_entries[0].entity_type, "mfa_factor");
    assert_eq!(security_entries[0].entity_id, Some(flow.id));
}

#[tokio::test]
async fn test_email_code_verify_requires_prior_send_without_consuming_flow() {
    let state = common::setup().await;
    apply_email_code_login_config(&state, false);
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);
    register_user(
        &app,
        "emailrequired",
        "emailrequired@example.com",
        "password123",
    )
    .await;

    let resp = login_raw(&app, "emailrequired", "password123").await;
    let body: Value = test::read_body_json(resp).await;
    let flow_token = body["data"]["flow_token"].as_str().unwrap().to_string();

    let resp = verify_mfa(&app, &flow_token, "email_code", "12345678").await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body:#?}");
    assert_eq!(body["code"], "auth.mfa_email_code_required");

    let resp = send_email_code(&app, &flow_token).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let memory_sender = aster_drive::services::mail_service::memory_sender_ref(&mail_sender)
        .expect("memory mail sender should be available in tests");
    let code = extract_email_code_from_message(&memory_sender.last_message().unwrap());

    let resp = verify_mfa(&app, &flow_token, "email_code", &code).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(common::extract_cookie(&resp, "aster_access").is_some());
}

#[tokio::test]
async fn test_email_code_wrong_code_keeps_code_available_until_attempt_limit() {
    let state = common::setup().await;
    apply_email_code_login_config(&state, false);
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);
    register_user(&app, "emailwrong", "emailwrong@example.com", "password123").await;

    let resp = login_raw(&app, "emailwrong", "password123").await;
    let body: Value = test::read_body_json(resp).await;
    let flow_token = body["data"]["flow_token"].as_str().unwrap().to_string();
    let resp = send_email_code(&app, &flow_token).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let memory_sender = aster_drive::services::mail_service::memory_sender_ref(&mail_sender)
        .expect("memory mail sender should be available in tests");
    let code = extract_email_code_from_message(&memory_sender.last_message().unwrap());
    let wrong_code = different_email_code(&code);

    let resp = verify_mfa(&app, &flow_token, "email_code", &wrong_code).await;
    let status = resp.status();
    assert!(common::extract_cookie(&resp, "aster_access").is_none());
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body:#?}");
    assert_eq!(body["code"], "auth.mfa_code_invalid");

    let resp = verify_mfa(&app, &flow_token, "email_code", &code).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(common::extract_cookie(&resp, "aster_access").is_some());
}

#[tokio::test]
async fn test_email_code_wrong_attempt_limit_consumes_flow() {
    let state = common::setup().await;
    apply_email_code_login_config(&state, false);
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);
    register_user(
        &app,
        "emailattempts",
        "emailattempts@example.com",
        "password123",
    )
    .await;

    let resp = login_raw(&app, "emailattempts", "password123").await;
    let body: Value = test::read_body_json(resp).await;
    let flow_token = body["data"]["flow_token"].as_str().unwrap().to_string();
    let resp = send_email_code(&app, &flow_token).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let memory_sender = aster_drive::services::mail_service::memory_sender_ref(&mail_sender)
        .expect("memory mail sender should be available in tests");
    let code = extract_email_code_from_message(&memory_sender.last_message().unwrap());
    let wrong_code = different_email_code(&code);

    for index in 0..5 {
        let resp = verify_mfa(&app, &flow_token, "email_code", &wrong_code).await;
        let status = resp.status();
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{body:#?}");
        if index < 4 {
            assert_eq!(body["code"], "auth.mfa_code_invalid");
        } else {
            assert_eq!(body["code"], "auth.mfa_attempts_exceeded");
        }
    }

    let resp = verify_mfa(&app, &flow_token, "email_code", &code).await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body:#?}");
    assert_eq!(body["code"], "auth.mfa_flow_invalid");
}

#[tokio::test]
async fn test_email_code_resend_is_rate_limited() {
    let state = common::setup().await;
    apply_email_code_login_config(&state, false);
    let app = create_test_app!(state);
    register_user(&app, "cooldown", "cooldown@example.com", "password123").await;

    let resp = login_raw(&app, "cooldown", "password123").await;
    let body: Value = test::read_body_json(resp).await;
    let flow_token = body["data"]["flow_token"].as_str().unwrap().to_string();

    let resp = send_email_code(&app, &flow_token).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = send_email_code(&app, &flow_token).await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body:#?}");
    assert_eq!(body["code"], "rate_limited");
    assert_eq!(body["error"]["retryable"], true);
}

#[tokio::test]
async fn test_expired_email_code_returns_email_code_expired_code() {
    let state = common::setup().await;
    apply_email_code_login_config(&state, false);
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);
    register_user(
        &app,
        "expiredemail",
        "expiredemail@example.com",
        "password123",
    )
    .await;

    let resp = login_raw(&app, "expiredemail", "password123").await;
    let body: Value = test::read_body_json(resp).await;
    let flow_token = body["data"]["flow_token"].as_str().unwrap().to_string();
    let resp = send_email_code(&app, &flow_token).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let memory_sender = aster_drive::services::mail_service::memory_sender_ref(&mail_sender)
        .expect("memory mail sender should be available in tests");
    let code = extract_email_code_from_message(&memory_sender.last_message().unwrap());
    let flow = find_mfa_flow_by_token(&db, &flow_token).await;
    let code_row = aster_drive::entities::mfa_email_code::Entity::find()
        .filter(aster_drive::entities::mfa_email_code::Column::FlowId.eq(flow.id))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let code_row_id = code_row.id;
    let mut active = code_row.into_active_model();
    active.expires_at = Set(Utc::now() - Duration::seconds(1));
    active.update(&db).await.unwrap();

    let resp = verify_mfa(&app, &flow_token, "email_code", &code).await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body:#?}");
    assert_eq!(body["code"], "auth.mfa_email_code_expired");

    let consumed_row = aster_drive::entities::mfa_email_code::Entity::find_by_id(code_row_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(consumed_row.consumed_at.is_some());

    let resp = verify_mfa(&app, &flow_token, "email_code", &code).await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body:#?}");
    assert_eq!(body["code"], "auth.mfa_email_code_required");
}

#[tokio::test]
async fn test_email_code_delivery_failure_consumes_created_code() {
    let mut state = common::setup().await;
    apply_email_code_login_config(&state, false);
    state.mail_sender = Arc::new(FailingMailSender);
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    register_user(&app, "emailfail", "emailfail@example.com", "password123").await;

    let resp = login_raw(&app, "emailfail", "password123").await;
    let body: Value = test::read_body_json(resp).await;
    let flow_token = body["data"]["flow_token"].as_str().unwrap().to_string();

    let resp = send_email_code(&app, &flow_token).await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body:#?}");
    assert_eq!(body["code"], "mail.delivery_failed");

    let flow = find_mfa_flow_by_token(&db, &flow_token).await;
    let code_row = aster_drive::entities::mfa_email_code::Entity::find()
        .filter(aster_drive::entities::mfa_email_code::Column::FlowId.eq(flow.id))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(code_row.consumed_at.is_some());

    let failed_entries = audit_entries(&db, AuditAction::MailDeliveryFailed).await;
    assert_eq!(failed_entries.len(), 1);
    let details: Value =
        serde_json::from_str(failed_entries[0].details.as_deref().unwrap()).unwrap();
    assert_eq!(details["to_address"], "emailfail@example.com");
    assert_eq!(details["template_code"], "login_email_code");
    assert_eq!(
        details["error"],
        "Mail Delivery Failed: forced mail delivery failure"
    );
    assert_eq!(audit_count(&db, AuditAction::MailSend).await, 0);
    assert_eq!(audit_count(&db, AuditAction::UserMfaEmailCodeSend).await, 0);
}

#[tokio::test]
async fn test_password_login_requires_mfa_without_setting_cookies_then_totp_completes() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    register_user(&app, "mfauser", "mfa@example.com", "password123").await;
    let (access, _) = login_user!(app, "mfauser", "password123");
    let (_factor_id, secret, _recovery_codes) = enable_totp(&app, &access).await;

    let resp = login_raw(&app, "mfauser", "password123").await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(common::extract_cookie(&resp, "aster_access").is_none());
    assert!(common::extract_cookie(&resp, "aster_refresh").is_none());
    assert!(common::extract_cookie(&resp, "aster_csrf").is_none());
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "mfa_required");
    let flow_token = body["data"]["flow_token"].as_str().unwrap().to_string();
    assert_eq!(body["data"]["methods"][0], "totp");

    let code = totp_code(&secret);
    let resp = verify_mfa(&app, &flow_token, "totp", &code).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(common::extract_cookie(&resp, "aster_access").is_some());
    assert!(common::extract_cookie(&resp, "aster_refresh").is_some());
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "authenticated");
}

#[tokio::test]
async fn test_totp_user_email_code_fallback_requires_separate_policy() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    register_user(
        &app,
        "totpfallback",
        "totpfallback@example.com",
        "password123",
    )
    .await;
    let (access, _) = login_user!(app, "totpfallback", "password123");
    let _ = enable_totp(&app, &access).await;

    apply_email_code_login_config(&state, false);
    let resp = login_raw(&app, "totpfallback", "password123").await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "mfa_required");
    assert_eq!(
        body["data"]["methods"],
        serde_json::json!(["totp", "recovery_code"])
    );

    state.runtime_config.apply(common::system_config_model(
        auth_runtime::AUTH_EMAIL_CODE_LOGIN_ALLOW_TOTP_FALLBACK_KEY,
        "true",
    ));
    let resp = login_raw(&app, "totpfallback", "password123").await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["methods"],
        serde_json::json!(["totp", "recovery_code", "email_code"])
    );
}

#[tokio::test]
async fn test_mfa_wrong_totp_attempts_eventually_consume_flow() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    register_user(&app, "attempts", "attempts@example.com", "password123").await;
    let (access, _) = login_user!(app, "attempts", "password123");
    let _ = enable_totp(&app, &access).await;

    let resp = login_raw(&app, "attempts", "password123").await;
    let body: Value = test::read_body_json(resp).await;
    let flow_token = body["data"]["flow_token"].as_str().unwrap().to_string();

    for index in 0..5 {
        let resp = verify_mfa(&app, &flow_token, "totp", "000000").await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let body: Value = test::read_body_json(resp).await;
        if index < 4 {
            assert_eq!(body["code"], "auth.mfa_code_invalid");
        } else {
            assert_eq!(body["code"], "auth.mfa_attempts_exceeded");
        }
    }
}

#[tokio::test]
async fn test_expired_mfa_flow_cannot_login() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    register_user(&app, "expiredmfa", "expiredmfa@example.com", "password123").await;
    let (access, _) = login_user!(app, "expiredmfa", "password123");
    let _ = enable_totp(&app, &access).await;

    let resp = login_raw(&app, "expiredmfa", "password123").await;
    let body: Value = test::read_body_json(resp).await;
    let flow_token = body["data"]["flow_token"].as_str().unwrap().to_string();
    let flow_hash = aster_drive::utils::hash::sha256_hex(flow_token.as_bytes());
    let flow = aster_drive::entities::mfa_login_flow::Entity::find()
        .filter(aster_drive::entities::mfa_login_flow::Column::FlowTokenHash.eq(flow_hash))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let mut active = flow.into_active_model();
    active.expires_at = Set(Utc::now() - Duration::seconds(1));
    active.update(&db).await.unwrap();

    let resp = verify_mfa(&app, &flow_token, "totp", "123456").await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    assert!(common::extract_cookie(&resp, "aster_access").is_none());
    assert!(common::extract_cookie(&resp, "aster_refresh").is_none());
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "auth.mfa_flow_expired");
}

#[tokio::test]
async fn test_recovery_code_can_be_used_once() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    register_user(&app, "recovery", "recovery@example.com", "password123").await;
    let (access, _) = login_user!(app, "recovery", "password123");
    let (_factor_id, _secret, recovery_codes) = enable_totp(&app, &access).await;
    let recovery_code = recovery_codes.first().unwrap().clone();

    let resp = login_raw(&app, "recovery", "password123").await;
    let body: Value = test::read_body_json(resp).await;
    let flow_token = body["data"]["flow_token"].as_str().unwrap().to_string();
    let resp = verify_mfa(&app, &flow_token, "recovery_code", &recovery_code).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = login_raw(&app, "recovery", "password123").await;
    let body: Value = test::read_body_json(resp).await;
    let second_flow = body["data"]["flow_token"].as_str().unwrap().to_string();
    let resp = verify_mfa(&app, &second_flow, "recovery_code", &recovery_code).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_totp_method_with_recovery_code_returns_mfa_code_invalid() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    register_user(
        &app,
        "wrongmethod",
        "wrongmethod@example.com",
        "password123",
    )
    .await;
    let (access, _) = login_user!(app, "wrongmethod", "password123");
    let (_factor_id, _secret, recovery_codes) = enable_totp(&app, &access).await;
    let recovery_code = recovery_codes.first().unwrap().clone();

    let resp = login_raw(&app, "wrongmethod", "password123").await;
    let body: Value = test::read_body_json(resp).await;
    let flow_token = body["data"]["flow_token"].as_str().unwrap().to_string();
    let resp = verify_mfa(&app, &flow_token, "totp", &recovery_code).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "auth.mfa_code_invalid");
    assert_ne!(body["msg"], "TOTP code must be a 6 digit number");
}

#[tokio::test]
async fn test_regenerate_recovery_codes_accepts_recovery_code_without_password() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    register_user(&app, "regen", "regen@example.com", "password123").await;
    let (access, _) = login_user!(app, "regen", "password123");
    let (_factor_id, _secret, recovery_codes) = enable_totp(&app, &access).await;
    let recovery_code = recovery_codes.first().unwrap().clone();

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/mfa/recovery-codes/regenerate")
        .insert_header(("Cookie", common::access_cookie_header(&access)))
        .insert_header(common::csrf_header_for(&access))
        .set_json(serde_json::json!({ "code": recovery_code }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "{body:#?}");
    assert_eq!(body["data"].as_array().unwrap().len(), 10);
}

#[tokio::test]
async fn test_regenerate_recovery_codes_wrong_code_returns_mfa_error_code() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    register_user(&app, "regenbad", "regenbad@example.com", "password123").await;
    let (access, _) = login_user!(app, "regenbad", "password123");
    let _ = enable_totp(&app, &access).await;

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/mfa/recovery-codes/regenerate")
        .insert_header(("Cookie", common::access_cookie_header(&access)))
        .insert_header(common::csrf_header_for(&access))
        .set_json(serde_json::json!({ "code": "000000" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body:#?}");
    assert_eq!(body["code"], "auth.mfa_code_invalid");
    assert_eq!(body["error"]["retryable"], false);
    assert!(body["error"].get("internal_code").is_none());
}

#[tokio::test]
async fn test_delete_mfa_factor_accepts_totp_without_password() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    register_user(&app, "disablemfa", "disablemfa@example.com", "password123").await;
    let (access, _) = login_user!(app, "disablemfa", "password123");
    let (factor_id, secret, _recovery_codes) = enable_totp(&app, &access).await;

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/auth/mfa/factors/{factor_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&access)))
        .insert_header(common::csrf_header_for(&access))
        .set_json(serde_json::json!({ "code": totp_code(&secret) }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "{body:#?}");

    let factors = aster_drive::entities::mfa_factor::Entity::find()
        .filter(aster_drive::entities::mfa_factor::Column::Id.eq(factor_id))
        .all(&db)
        .await
        .unwrap();
    assert!(factors.is_empty());
}

#[tokio::test]
async fn test_old_mfa_flow_cannot_login_after_password_reset() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (admin_access, _) = register_and_login!(app);
    let stale_user_id = admin_create_user!(
        app,
        &admin_access,
        "stale2",
        "stale2@example.com",
        "password123"
    );
    let (access, _) = login_user!(app, "stale2", "password123");
    let (_factor_id, secret, _codes) = enable_totp(&app, &access).await;

    let resp = login_raw(&app, "stale2", "password123").await;
    let body: Value = test::read_body_json(resp).await;
    let flow_token = body["data"]["flow_token"].as_str().unwrap().to_string();

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/admin/users/{stale_user_id}/password"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_access)))
        .insert_header(common::csrf_header_for(&admin_access))
        .set_json(serde_json::json!({ "password": "newpassword123" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let code = totp_code(&secret);
    let resp = verify_mfa(&app, &flow_token, "totp", &code).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "auth.mfa_flow_invalid");

    let active_sessions = aster_drive::entities::auth_session::Entity::find()
        .filter(aster_drive::entities::auth_session::Column::UserId.eq(stale_user_id))
        .all(&db)
        .await
        .unwrap();
    assert!(active_sessions.is_empty());
}

#[tokio::test]
async fn test_disabled_user_cannot_exchange_existing_mfa_flow() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_access, _) = register_and_login!(app);
    let user_id = admin_create_user!(
        app,
        &admin_access,
        "disabledmfa",
        "disabledmfa@example.com",
        "password123"
    );
    let (access, _) = login_user!(app, "disabledmfa", "password123");
    let (_factor_id, secret, _codes) = enable_totp(&app, &access).await;

    let resp = login_raw(&app, "disabledmfa", "password123").await;
    let body: Value = test::read_body_json(resp).await;
    let flow_token = body["data"]["flow_token"].as_str().unwrap().to_string();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/users/{user_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_access)))
        .insert_header(common::csrf_header_for(&admin_access))
        .set_json(serde_json::json!({ "status": "disabled" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let code = totp_code(&secret);
    let resp = verify_mfa(&app, &flow_token, "totp", &code).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert!(common::extract_cookie(&resp, "aster_access").is_none());
    assert!(common::extract_cookie(&resp, "aster_refresh").is_none());
}

#[tokio::test]
async fn test_admin_reset_mfa_clears_factors_and_revokes_sessions() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (admin_access, _) = register_and_login!(app);
    let user_id = admin_create_user!(
        app,
        &admin_access,
        "resetmfa",
        "resetmfa@example.com",
        "password123"
    );
    let (access, _) = login_user!(app, "resetmfa", "password123");
    let _ = enable_totp(&app, &access).await;

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/users/{user_id}/mfa"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_access)))
        .insert_header(common::csrf_header_for(&admin_access))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let factors = aster_drive::entities::mfa_factor::Entity::find()
        .filter(aster_drive::entities::mfa_factor::Column::UserId.eq(user_id))
        .all(&db)
        .await
        .unwrap();
    assert!(factors.is_empty());

    let resp = login_raw(&app, "resetmfa", "password123").await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(common::extract_cookie(&resp, "aster_access").is_some());
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "authenticated");
}

pub mod dex;
pub mod mock;

pub use dex::{
    DEX_TEST_IMAGE_TAG, DEX_TEST_USER_EMAIL, DEX_TEST_USER_SUBJECT, complete_dex_password_login,
    dex_config, reserve_localhost_port, wait_for_dex_discovery,
};
pub use mock::start_mock_external_auth_provider;

use actix_web::{body::MessageBody, dev::ServiceResponse, test};
use aster_drive::entities::{external_auth_provider, user};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, IntoActiveModel};
use serde_json::Value;

use crate::common;

pub const TEST_BROWSER_ORIGIN: &str = "http://localhost:8080";
pub const TEST_CLIENT_ID: &str = "aster-test-client";

pub async fn create_external_auth_provider<S, B, E>(
    app: &S,
    admin_token: &str,
    issuer_url: &str,
    enabled: bool,
    auto_provision_enabled: bool,
) -> Value
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = E,
        >,
    B: MessageBody,
    E: std::fmt::Debug,
{
    let mut options = TestOidcProviderOptions::mock(issuer_url);
    options.enabled = enabled;
    options.auto_provision_enabled = auto_provision_enabled;
    create_external_auth_provider_with(app, admin_token, options).await
}

pub async fn create_external_auth_provider_key<S, B, E>(
    app: &S,
    admin_token: &str,
    issuer_url: &str,
    enabled: bool,
    auto_provision_enabled: bool,
) -> String
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = E,
        >,
    B: MessageBody,
    E: std::fmt::Debug,
{
    let created = create_external_auth_provider(
        app,
        admin_token,
        issuer_url,
        enabled,
        auto_provision_enabled,
    )
    .await;
    created_provider_key(&created)
}

pub async fn create_external_auth_provider_with_key<S, B, E>(
    app: &S,
    admin_token: &str,
    options: TestOidcProviderOptions,
) -> String
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = E,
        >,
    B: MessageBody,
    E: std::fmt::Debug,
{
    let created = create_external_auth_provider_with(app, admin_token, options).await;
    created_provider_key(&created)
}

pub struct TestOidcProviderOptions {
    pub display_name_prefix: String,
    pub issuer_url: String,
    pub enabled: bool,
    pub auto_provision_enabled: bool,
    pub auto_link_verified_email_enabled: bool,
    pub require_email_verified: bool,
    pub allowed_domains: Vec<String>,
}

impl TestOidcProviderOptions {
    pub fn mock(issuer_url: &str) -> Self {
        Self {
            display_name_prefix: "mock".to_string(),
            issuer_url: issuer_url.to_string(),
            enabled: true,
            auto_provision_enabled: false,
            auto_link_verified_email_enabled: false,
            require_email_verified: true,
            allowed_domains: vec!["example.com".to_string()],
        }
    }
}

pub async fn create_external_auth_provider_with<S, B, E>(
    app: &S,
    admin_token: &str,
    options: TestOidcProviderOptions,
) -> Value
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
        .uri("/api/v1/admin/external-auth/providers")
        .insert_header(("Cookie", common::access_cookie_header(admin_token)))
        .insert_header(common::csrf_header_for(admin_token))
        .set_json(serde_json::json!({
            "provider_kind": "oidc",
            "display_name": format!("{} OIDC", options.display_name_prefix),
            "icon_url": "/static/external-auth/mock.svg",
            "issuer_url": options.issuer_url,
            "client_id": TEST_CLIENT_ID,
            "client_secret": "super-secret",
            "enabled": options.enabled,
            "auto_provision_enabled": options.auto_provision_enabled,
            "auto_link_verified_email_enabled": options.auto_link_verified_email_enabled,
            "require_email_verified": options.require_email_verified,
            "allowed_domains": options.allowed_domains
        }))
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 201);
    test::read_body_json(resp).await
}

pub fn created_provider_key(created: &Value) -> String {
    created["data"]["key"]
        .as_str()
        .expect("provider key should be returned")
        .to_string()
}

pub fn external_auth_provider_model(
    key: &str,
    issuer_url: &str,
    enabled: bool,
) -> external_auth_provider::ActiveModel {
    oidc_external_auth_provider_model(key, issuer_url, enabled, "openid email profile")
}

fn oidc_external_auth_provider_model(
    key: &str,
    issuer_url: &str,
    enabled: bool,
    scopes: &str,
) -> external_auth_provider::ActiveModel {
    let now = Utc::now();
    external_auth_provider::ActiveModel {
        key: Set(key.to_string()),
        display_name: Set(format!("{key} provider")),
        icon_url: Set(None),
        provider_kind: Set(aster_drive::types::ExternalAuthProviderKind::Oidc),
        protocol: Set(aster_drive::types::ExternalAuthProtocol::Oidc),
        issuer_url: Set(Some(issuer_url.to_string())),
        authorization_url: Set(None),
        token_url: Set(None),
        userinfo_url: Set(None),
        client_id: Set(TEST_CLIENT_ID.to_string()),
        client_secret: Set(None),
        scopes: Set(scopes.to_string()),
        enabled: Set(enabled),
        auto_provision_enabled: Set(false),
        auto_link_verified_email_enabled: Set(false),
        require_email_verified: Set(true),
        subject_claim: Set(None),
        username_claim: Set(None),
        display_name_claim: Set(None),
        email_claim: Set(None),
        email_verified_claim: Set(None),
        groups_claim: Set(None),
        avatar_url_claim: Set(None),
        allowed_domains: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
}

pub fn google_external_auth_provider_model(
    key: &str,
    issuer_url: &str,
    enabled: bool,
) -> external_auth_provider::ActiveModel {
    external_auth_provider::ActiveModel {
        provider_kind: Set(aster_drive::types::ExternalAuthProviderKind::Google),
        scopes: Set("openid profile email".to_string()),
        subject_claim: Set(Some("sub".to_string())),
        display_name_claim: Set(Some("name".to_string())),
        email_claim: Set(Some("email".to_string())),
        email_verified_claim: Set(Some("email_verified".to_string())),
        avatar_url_claim: Set(Some("picture".to_string())),
        ..oidc_external_auth_provider_model(key, issuer_url, enabled, "openid profile email")
    }
}

pub fn configure_oidc_public_site_url(state: &aster_drive::runtime::PrimaryAppState) {
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::site_url::PUBLIC_SITE_URL_KEY,
        r#"["http://localhost:8080"]"#,
    ));
}

pub async fn start_oidc_login<S, B, E>(
    app: &S,
    mock_provider: &mock::MockOidcProvider,
    provider_key: &str,
    return_path: &str,
) -> String
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
        .uri(&format!(
            "/api/v1/auth/external-auth/oidc/{provider_key}/start"
        ))
        .insert_header(("Origin", TEST_BROWSER_ORIGIN))
        .set_json(serde_json::json!({ "return_path": return_path }))
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let auth_url = body["data"]["authorization_url"]
        .as_str()
        .expect("authorization url should be returned");
    reqwest::get(auth_url)
        .await
        .expect("mock authorize request should succeed");
    mock_provider.last_authorize_request().state
}

pub async fn start_google_login<S, B, E>(
    app: &S,
    mock_provider: &mock::MockOidcProvider,
    provider_key: &str,
    return_path: &str,
) -> String
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
        .uri(&format!(
            "/api/v1/auth/external-auth/google/{provider_key}/start"
        ))
        .insert_header(("Origin", TEST_BROWSER_ORIGIN))
        .set_json(serde_json::json!({ "return_path": return_path }))
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let auth_url = body["data"]["authorization_url"]
        .as_str()
        .expect("authorization url should be returned");
    reqwest::get(auth_url)
        .await
        .expect("mock authorize request should succeed");
    mock_provider.last_authorize_request().state
}

pub async fn finish_oidc_callback<S, B, E>(
    app: &S,
    provider_key: &str,
    state_value: &str,
) -> ServiceResponse<B>
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = E,
        >,
    B: MessageBody,
    E: std::fmt::Debug,
{
    let callback = format!(
        "/api/v1/auth/external-auth/oidc/{provider_key}/callback?code=mock-code&state={state_value}"
    );
    let req = test::TestRequest::get()
        .uri(&callback)
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .to_request();
    test::call_service(app, req).await
}

pub async fn finish_google_callback<S, B, E>(
    app: &S,
    provider_key: &str,
    state_value: &str,
) -> ServiceResponse<B>
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = E,
        >,
    B: MessageBody,
    E: std::fmt::Debug,
{
    let callback = format!(
        "/api/v1/auth/external-auth/google/{provider_key}/callback?code=mock-code&state={state_value}"
    );
    let req = test::TestRequest::get()
        .uri(&callback)
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .to_request();
    test::call_service(app, req).await
}

pub fn assert_oidc_error_redirect<B>(resp: &ServiceResponse<B>) {
    assert_eq!(resp.status(), 302);
    let location = resp
        .headers()
        .get("Location")
        .and_then(|value| value.to_str().ok())
        .expect("OIDC error redirect location should exist");
    assert!(location.starts_with("http://localhost:8080/login?external_auth=error"));
    assert!(common::extract_cookie(resp, "aster_access").is_none());
    assert!(common::extract_cookie(resp, "aster_refresh").is_none());
}

pub fn oidc_email_required_flow<B>(resp: &ServiceResponse<B>) -> String {
    assert_eq!(resp.status(), 302);
    let location = resp
        .headers()
        .get("Location")
        .and_then(|value| value.to_str().ok())
        .expect("OIDC email required redirect location should exist");
    assert!(location.starts_with("http://localhost:8080/login?external_auth=email_required"));
    assert!(common::extract_cookie(resp, "aster_access").is_none());
    assert!(common::extract_cookie(resp, "aster_refresh").is_none());
    let parsed = reqwest::Url::parse(location).expect("redirect location should parse");
    parsed
        .query_pairs()
        .find(|(key, _)| key == "flow")
        .map(|(_, value)| value.into_owned())
        .expect("email required redirect should include flow token")
}

pub async fn start_oidc_email_verification<S, B, E>(
    app: &S,
    flow_token: &str,
    email: &str,
) -> ServiceResponse<B>
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
        .uri("/api/v1/auth/external-auth/email-verification/start")
        .insert_header(("Origin", TEST_BROWSER_ORIGIN))
        .set_json(serde_json::json!({
            "flow_token": flow_token,
            "email": email
        }))
        .to_request();
    test::call_service(app, req).await
}

pub async fn assert_start_oidc_email_verification_ok<S, B, E>(
    app: &S,
    flow_token: &str,
    email: &str,
) where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = E,
        >,
    B: MessageBody,
    E: std::fmt::Debug,
{
    let resp = start_oidc_email_verification(app, flow_token, email).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["message"],
        "external auth email verification email sent"
    );
}

pub async fn confirm_oidc_email_verification<S, B, E>(app: &S, token: &str) -> ServiceResponse<B>
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = E,
        >,
    B: MessageBody,
    E: std::fmt::Debug,
{
    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/auth/external-auth/email-verification/confirm?token={}",
            urlencoding::encode(token)
        ))
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .to_request();
    test::call_service(app, req).await
}

pub async fn link_oidc_with_password<S, B, E>(
    app: &S,
    flow_token: &str,
    identifier: &str,
    password: &str,
) -> ServiceResponse<B>
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
        .uri("/api/v1/auth/external-auth/password-link")
        .insert_header(("Origin", TEST_BROWSER_ORIGIN))
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "flow_token": flow_token,
            "identifier": identifier,
            "password": password
        }))
        .to_request();
    test::call_service(app, req).await
}

pub async fn latest_oidc_email_verification_token(
    state: &aster_drive::runtime::PrimaryAppState,
) -> String {
    common::flush_mail_outbox(state).await;
    let memory_sender = aster_drive::services::mail_service::memory_sender_ref(&state.mail_sender)
        .expect("memory mail sender should be available in tests");
    let message = memory_sender
        .last_message()
        .expect("OIDC email verification email should be sent");
    common::extract_token_from_mail_message(
        &message,
        "/api/v1/auth/external-auth/email-verification/confirm?token=",
    )
    .expect("OIDC email verification token should be present")
}

pub async fn disable_user(state: &aster_drive::runtime::PrimaryAppState, user_id: i64) {
    let user = user::Entity::find_by_id(user_id)
        .one(state.writer_db())
        .await
        .expect("user should query")
        .expect("user should exist");
    let mut active = user.into_active_model();
    active.status = Set(aster_drive::types::UserStatus::Disabled);
    active
        .update(state.writer_db())
        .await
        .expect("user should update");
}

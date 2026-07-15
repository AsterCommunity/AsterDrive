pub mod mock;

pub use mock::{GitHubEmailEntry, TokenAuthObservation, start_mock_oauth2_provider};

use actix_web::{body::MessageBody, dev::ServiceResponse, test};
use aster_drive::entities::{external_auth_provider, user};
use aster_drive::runtime::SharedRuntimeState;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, IntoActiveModel};
use serde_json::Value;
use std::time::Duration as StdDuration;

use crate::common;
use crate::external_auth::{
    external_auth_binding_cookie_from_response, external_auth_binding_cookie_header,
    remember_external_auth_binding,
};

pub const TEST_BROWSER_ORIGIN: &str = "http://localhost:8080";
pub const TEST_CLIENT_ID: &str = "aster-test-client";
pub const TEST_CLIENT_SECRET: &str = "super-secret";
const MOCK_AUTHORIZE_TIMEOUT: StdDuration = StdDuration::from_secs(5);

async fn request_mock_authorize(auth_url: &str) {
    let client = reqwest::Client::builder()
        .timeout(MOCK_AUTHORIZE_TIMEOUT)
        .build()
        .expect("mock authorize reqwest client should build");
    client
        .get(auth_url)
        .send()
        .await
        .expect("mock authorize request should succeed");
}

pub struct TestOAuth2ProviderOptions {
    pub base_url: String,
    pub client_secret: Option<String>,
    pub enabled: bool,
    pub auto_provision_enabled: bool,
    pub auto_link_verified_email_enabled: bool,
    pub require_email_verified: bool,
    pub allowed_domains: Vec<String>,
}

impl TestOAuth2ProviderOptions {
    pub fn mock(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            client_secret: Some(TEST_CLIENT_SECRET.to_string()),
            enabled: true,
            auto_provision_enabled: false,
            auto_link_verified_email_enabled: false,
            require_email_verified: false,
            allowed_domains: vec!["example.com".to_string()],
        }
    }
}

pub async fn create_oauth2_provider_with<S, B, E>(
    app: &S,
    admin_token: &str,
    options: TestOAuth2ProviderOptions,
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
    let mut payload = serde_json::json!({
        "provider_kind": "generic_oauth2",
        "display_name": "Generic OAuth2",
        "authorization_url": format!("{}/authorize", options.base_url),
        "token_url": format!("{}/token", options.base_url),
        "userinfo_url": format!("{}/userinfo", options.base_url),
        "client_id": TEST_CLIENT_ID,
        "scopes": "read:user user:email",
        "enabled": options.enabled,
        "auto_provision_enabled": options.auto_provision_enabled,
        "auto_link_verified_email_enabled": options.auto_link_verified_email_enabled,
        "require_email_verified": options.require_email_verified,
        "subject_claim": "id",
        "username_claim": "login",
        "display_name_claim": "name",
        "email_claim": "email",
        "email_verified_claim": "email_verified",
        "allowed_domains": options.allowed_domains
    });
    if let Some(client_secret) = options.client_secret {
        payload["client_secret"] = serde_json::json!(client_secret);
    }
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers")
        .insert_header(("Cookie", common::access_cookie_header(admin_token)))
        .insert_header(common::csrf_header_for(admin_token))
        .set_json(payload)
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

pub fn configure_oauth2_public_site_url(state: &aster_drive::runtime::PrimaryAppState) {
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::site_url::PUBLIC_SITE_URL_KEY,
        r#"["http://localhost:8080"]"#,
    ));
}

pub async fn start_oauth2_login<S, B, E>(
    app: &S,
    mock_provider: &mock::MockOAuth2Provider,
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
            "/api/v1/auth/external-auth/generic_oauth2/{provider_key}/start"
        ))
        .insert_header(("Origin", TEST_BROWSER_ORIGIN))
        .set_json(serde_json::json!({ "return_path": return_path }))
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 200);
    let binding_cookie = external_auth_binding_cookie_from_response(&resp);
    let body: Value = test::read_body_json(resp).await;
    let auth_url = body["data"]["authorization_url"]
        .as_str()
        .expect("authorization url should be returned");
    request_mock_authorize(auth_url).await;
    let state = mock_provider.last_authorize_request().state;
    remember_external_auth_binding(&state, binding_cookie);
    state
}

pub async fn finish_oauth2_callback<S, B, E>(
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
        "/api/v1/auth/external-auth/generic_oauth2/{provider_key}/callback?code=mock-code&state={state_value}"
    );
    let req = test::TestRequest::get()
        .uri(&callback)
        .insert_header(("Cookie", external_auth_binding_cookie_header(state_value)))
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .to_request();
    test::call_service(app, req).await
}

pub async fn start_github_login<S, B, E>(
    app: &S,
    mock_provider: &mock::MockOAuth2Provider,
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
            "/api/v1/auth/external-auth/github/{provider_key}/start"
        ))
        .insert_header(("Origin", TEST_BROWSER_ORIGIN))
        .set_json(serde_json::json!({ "return_path": return_path }))
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 200);
    let binding_cookie = external_auth_binding_cookie_from_response(&resp);
    let body: Value = test::read_body_json(resp).await;
    let auth_url = body["data"]["authorization_url"]
        .as_str()
        .expect("authorization url should be returned");
    request_mock_authorize(auth_url).await;
    let state = mock_provider.last_authorize_request().state;
    remember_external_auth_binding(&state, binding_cookie);
    state
}

pub async fn finish_github_callback<S, B, E>(
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
        "/api/v1/auth/external-auth/github/{provider_key}/callback?code=mock-code&state={state_value}"
    );
    let req = test::TestRequest::get()
        .uri(&callback)
        .insert_header(("Cookie", external_auth_binding_cookie_header(state_value)))
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .to_request();
    test::call_service(app, req).await
}

pub async fn start_qq_login<S, B, E>(
    app: &S,
    mock_provider: &mock::MockOAuth2Provider,
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
            "/api/v1/auth/external-auth/qq/{provider_key}/start"
        ))
        .insert_header(("Origin", TEST_BROWSER_ORIGIN))
        .set_json(serde_json::json!({ "return_path": return_path }))
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 200);
    let binding_cookie = external_auth_binding_cookie_from_response(&resp);
    let body: Value = test::read_body_json(resp).await;
    let auth_url = body["data"]["authorization_url"]
        .as_str()
        .expect("authorization url should be returned");
    request_mock_authorize(auth_url).await;
    let state = mock_provider.last_authorize_request().state;
    remember_external_auth_binding(&state, binding_cookie);
    state
}

pub async fn finish_qq_callback<S, B, E>(
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
        "/api/v1/auth/external-auth/qq/{provider_key}/callback?code=mock-code&state={state_value}"
    );
    let req = test::TestRequest::get()
        .uri(&callback)
        .insert_header(("Cookie", external_auth_binding_cookie_header(state_value)))
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .to_request();
    test::call_service(app, req).await
}

pub fn assert_oauth2_error_redirect<B>(resp: &ServiceResponse<B>) {
    assert_eq!(resp.status(), 302);
    let location = resp
        .headers()
        .get("Location")
        .and_then(|value| value.to_str().ok())
        .expect("OAuth2 error redirect location should exist");
    assert!(location.starts_with("http://localhost:8080/login?external_auth=error"));
    assert!(common::extract_cookie(resp, "aster_access").is_none());
    assert!(common::extract_cookie(resp, "aster_refresh").is_none());
}

pub fn oauth2_email_required_flow<B>(resp: &ServiceResponse<B>) -> String {
    assert_eq!(resp.status(), 302);
    let location = resp
        .headers()
        .get("Location")
        .and_then(|value| value.to_str().ok())
        .expect("OAuth2 email required redirect location should exist");
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

pub fn external_auth_provider_model(
    key: &str,
    base_url: &str,
    enabled: bool,
) -> external_auth_provider::ActiveModel {
    let now = Utc::now();
    external_auth_provider::ActiveModel {
        key: Set(key.to_string()),
        display_name: Set(format!("{key} provider")),
        icon_url: Set(None),
        provider_kind: Set(aster_forge_external_auth::ExternalAuthProviderKind::GenericOAuth2),
        protocol: Set(aster_forge_external_auth::ExternalAuthProtocol::OAuth2),
        options: Set(
            aster_drive::types::external_auth_provider::StoredExternalAuthProviderOptions::empty(),
        ),
        issuer_url: Set(None),
        authorization_url: Set(Some(format!("{base_url}/authorize"))),
        token_url: Set(Some(format!("{base_url}/token"))),
        userinfo_url: Set(Some(format!("{base_url}/userinfo"))),
        client_id: Set(TEST_CLIENT_ID.to_string()),
        client_secret: Set(Some(TEST_CLIENT_SECRET.to_string())),
        scopes: Set("read:user user:email".to_string()),
        enabled: Set(enabled),
        auto_provision_enabled: Set(false),
        auto_link_verified_email_enabled: Set(false),
        require_email_verified: Set(false),
        subject_claim: Set(Some("id".to_string())),
        username_claim: Set(Some("login".to_string())),
        display_name_claim: Set(Some("name".to_string())),
        email_claim: Set(Some("email".to_string())),
        email_verified_claim: Set(Some("email_verified".to_string())),
        groups_claim: Set(None),
        avatar_url_claim: Set(None),
        allowed_domains: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
}

pub fn github_external_auth_provider_model(
    key: &str,
    base_url: &str,
    enabled: bool,
) -> external_auth_provider::ActiveModel {
    let now = Utc::now();
    external_auth_provider::ActiveModel {
        key: Set(key.to_string()),
        display_name: Set(format!("{key} provider")),
        icon_url: Set(None),
        provider_kind: Set(aster_forge_external_auth::ExternalAuthProviderKind::GitHub),
        protocol: Set(aster_forge_external_auth::ExternalAuthProtocol::OAuth2),
        options: Set(
            aster_drive::types::external_auth_provider::StoredExternalAuthProviderOptions::empty(),
        ),
        issuer_url: Set(None),
        authorization_url: Set(Some(format!("{base_url}/authorize"))),
        token_url: Set(Some(format!("{base_url}/token"))),
        userinfo_url: Set(Some(format!("{base_url}/user"))),
        client_id: Set(TEST_CLIENT_ID.to_string()),
        client_secret: Set(Some(TEST_CLIENT_SECRET.to_string())),
        scopes: Set("read:user user:email".to_string()),
        enabled: Set(enabled),
        auto_provision_enabled: Set(true),
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

pub fn qq_external_auth_provider_model(
    key: &str,
    base_url: &str,
    enabled: bool,
    require_email_verified: bool,
) -> external_auth_provider::ActiveModel {
    let now = Utc::now();
    external_auth_provider::ActiveModel {
        key: Set(key.to_string()),
        display_name: Set(format!("{key} provider")),
        icon_url: Set(None),
        provider_kind: Set(aster_forge_external_auth::ExternalAuthProviderKind::Qq),
        protocol: Set(aster_forge_external_auth::ExternalAuthProtocol::OAuth2),
        options: Set(
            aster_drive::types::external_auth_provider::StoredExternalAuthProviderOptions::empty(),
        ),
        issuer_url: Set(None),
        authorization_url: Set(Some(format!("{base_url}/authorize"))),
        token_url: Set(Some(format!("{base_url}/qq/token"))),
        userinfo_url: Set(Some(format!("{base_url}/qq/get_user_info"))),
        client_id: Set(TEST_CLIENT_ID.to_string()),
        client_secret: Set(Some(TEST_CLIENT_SECRET.to_string())),
        scopes: Set("get_user_info".to_string()),
        enabled: Set(enabled),
        auto_provision_enabled: Set(true),
        auto_link_verified_email_enabled: Set(true),
        require_email_verified: Set(require_email_verified),
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

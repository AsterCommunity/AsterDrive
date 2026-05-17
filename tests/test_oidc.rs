//! 集成测试：`oidc`。

#[macro_use]
mod common;

use actix_web::{
    App, HttpResponse, HttpServer, Responder, body::MessageBody, dev::ServiceResponse, test, web,
};
use aster_drive::db::repository::{
    external_auth_identity_repo, external_auth_login_flow_repo, external_auth_provider_repo,
};
use aster_drive::entities::{
    external_auth_email_verification_flow, external_auth_identity, external_auth_login_flow,
    external_auth_provider, user,
};
use aster_drive::services::external_auth_service;
use base64::Engine as _;
use chrono::{Duration, Utc};
use jsonwebtoken::{
    Algorithm, EncodingKey, Header,
    jwk::{
        AlgorithmParameters, CommonParameters, Jwk, JwkSet, KeyAlgorithm, PublicKeyUse,
        RSAKeyParameters,
    },
};
use rsa::{
    RsaPrivateKey, RsaPublicKey, pkcs1::EncodeRsaPrivateKey, rand_core::OsRng,
    traits::PublicKeyParts,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter,
};
use serde::Deserialize;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;
use uuid::Uuid;

const TEST_BROWSER_ORIGIN: &str = "http://localhost:8080";
const TEST_CLIENT_ID: &str = "aster-test-client";
const TEST_KID: &str = "aster-test-kid";
const DEX_TEST_CLIENT_SECRET: &str = "super-secret";
const DEX_TEST_IMAGE_TAG: &str = "v2.42.0";
const DEX_TEST_USER_EMAIL: &str = "dex-user@example.com";
const DEX_TEST_USER_SUBJECT: &str = "CgtkZXgtdXNlci1pZBIFbG9jYWw";

#[derive(Clone)]
struct MockOidcProvider {
    issuer: String,
    key: Arc<RsaPrivateKey>,
    authorization_requests: Arc<Mutex<Vec<AuthorizeRequest>>>,
    token_subject: Arc<Mutex<String>>,
    token_email: Arc<Mutex<Option<String>>>,
    token_email_verified: Arc<Mutex<bool>>,
    token_audience: Arc<Mutex<String>>,
    token_nonce_override: Arc<Mutex<Option<String>>>,
    token_issuer_override: Arc<Mutex<Option<String>>>,
}

#[derive(Clone, Debug, Deserialize)]
struct AuthorizeRequest {
    response_type: String,
    client_id: String,
    redirect_uri: String,
    scope: Option<String>,
    state: String,
    nonce: String,
    code_challenge: Option<String>,
    code_challenge_method: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenRequest {
    grant_type: String,
    code: String,
    redirect_uri: String,
    client_id: Option<String>,
    client_secret: Option<String>,
    code_verifier: Option<String>,
}

impl MockOidcProvider {
    fn new() -> Self {
        let mut rng = OsRng;
        let key = RsaPrivateKey::new(&mut rng, 2048).expect("RSA key should generate");
        Self {
            issuer: String::new(),
            key: Arc::new(key),
            authorization_requests: Arc::new(Mutex::new(Vec::new())),
            token_subject: Arc::new(Mutex::new("oidc-subject-1".to_string())),
            token_email: Arc::new(Mutex::new(Some("oidc-user@example.com".to_string()))),
            token_email_verified: Arc::new(Mutex::new(true)),
            token_audience: Arc::new(Mutex::new(TEST_CLIENT_ID.to_string())),
            token_nonce_override: Arc::new(Mutex::new(None)),
            token_issuer_override: Arc::new(Mutex::new(None)),
        }
    }

    fn with_issuer(mut self, issuer: String) -> Self {
        self.issuer = issuer;
        self
    }

    fn last_authorize_request(&self) -> AuthorizeRequest {
        self.authorization_requests
            .lock()
            .expect("authorize requests lock should not be poisoned")
            .last()
            .expect("authorization request should be recorded")
            .clone()
    }

    fn set_issuer_override(&self, issuer: Option<String>) {
        *self
            .token_issuer_override
            .lock()
            .expect("issuer override lock should not be poisoned") = issuer;
    }

    fn set_subject(&self, subject: &str) {
        *self
            .token_subject
            .lock()
            .expect("subject lock should not be poisoned") = subject.to_string();
    }

    fn set_email(&self, email: &str) {
        *self
            .token_email
            .lock()
            .expect("email lock should not be poisoned") = Some(email.to_string());
    }

    fn clear_email(&self) {
        *self
            .token_email
            .lock()
            .expect("email lock should not be poisoned") = None;
    }

    fn set_email_verified(&self, verified: bool) {
        *self
            .token_email_verified
            .lock()
            .expect("email verified lock should not be poisoned") = verified;
    }

    fn set_audience(&self, audience: &str) {
        *self
            .token_audience
            .lock()
            .expect("audience lock should not be poisoned") = audience.to_string();
    }

    fn set_nonce_override(&self, nonce: Option<String>) {
        *self
            .token_nonce_override
            .lock()
            .expect("nonce override lock should not be poisoned") = nonce;
    }

    fn public_jwk(&self) -> Jwk {
        let public = RsaPublicKey::from(self.key.as_ref());
        Jwk {
            common: CommonParameters {
                public_key_use: Some(PublicKeyUse::Signature),
                key_algorithm: Some(KeyAlgorithm::RS256),
                key_id: Some(TEST_KID.to_string()),
                ..Default::default()
            },
            algorithm: AlgorithmParameters::RSA(RSAKeyParameters {
                n: base64_url(public.n().to_bytes_be()),
                e: base64_url(public.e().to_bytes_be()),
                ..Default::default()
            }),
        }
    }

    fn sign_id_token(&self, nonce: &str) -> String {
        let issuer_override = self
            .token_issuer_override
            .lock()
            .expect("issuer override lock should not be poisoned")
            .clone();
        let issuer = issuer_override.as_deref().unwrap_or(&self.issuer);
        let subject = self
            .token_subject
            .lock()
            .expect("subject lock should not be poisoned")
            .clone();
        let email = self
            .token_email
            .lock()
            .expect("email lock should not be poisoned")
            .clone();
        let email_verified = *self
            .token_email_verified
            .lock()
            .expect("email verified lock should not be poisoned");
        let audience = self
            .token_audience
            .lock()
            .expect("audience lock should not be poisoned")
            .clone();
        let nonce_override = self
            .token_nonce_override
            .lock()
            .expect("nonce override lock should not be poisoned")
            .clone();
        let nonce = nonce_override.as_deref().unwrap_or(nonce);
        let now = Utc::now();
        let mut claims = serde_json::json!({
            "iss": issuer,
            "sub": subject,
            "aud": audience,
            "exp": (now + Duration::minutes(5)).timestamp(),
            "iat": now.timestamp(),
            "nonce": nonce,
            "name": "OIDC Test User",
            "preferred_username": "oidctest"
        });
        if let Some(email) = email {
            claims["email"] = serde_json::json!(email);
            claims["email_verified"] = serde_json::json!(email_verified);
        }
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(TEST_KID.to_string());
        let der = self
            .key
            .to_pkcs1_der()
            .expect("private key should encode to pkcs1");
        jsonwebtoken::encode(&header, &claims, &EncodingKey::from_rsa_der(der.as_bytes()))
            .expect("id_token should sign")
    }
}

fn base64_url(bytes: Vec<u8>) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

async fn start_mock_external_auth_provider() -> (MockOidcProvider, actix_web::dev::ServerHandle) {
    let seed = MockOidcProvider::new();
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("listener should bind");
    let addr = listener
        .local_addr()
        .expect("listener address should exist");
    let provider = seed.with_issuer(format!(
        "http://127.0.0.1:{addr_port}",
        addr_port = addr.port()
    ));
    let app_provider = provider.clone();
    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(app_provider.clone()))
            .route(
                "/.well-known/openid-configuration",
                web::get().to(mock_discovery),
            )
            .route("/authorize", web::get().to(mock_authorize))
            .route("/token", web::post().to(mock_token))
            .route("/jwks", web::get().to(mock_jwks))
    })
    .listen(listener)
    .expect("mock OIDC server should listen")
    .run();
    let handle = server.handle();
    tokio::spawn(server);
    (provider, handle)
}

async fn mock_discovery(provider: web::Data<MockOidcProvider>) -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({
        "issuer": provider.issuer,
        "authorization_endpoint": format!("{}/authorize", provider.issuer),
        "token_endpoint": format!("{}/token", provider.issuer),
        "jwks_uri": format!("{}/jwks", provider.issuer),
        "response_types_supported": ["code"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["RS256"],
        "scopes_supported": ["openid", "email", "profile"],
        "token_endpoint_auth_methods_supported": ["none", "client_secret_post", "client_secret_basic"],
        "claims_supported": ["sub", "iss", "aud", "exp", "iat", "nonce", "email", "email_verified", "name", "preferred_username"],
        "code_challenge_methods_supported": ["S256"]
    }))
}

async fn mock_authorize(
    provider: web::Data<MockOidcProvider>,
    query: web::Query<AuthorizeRequest>,
) -> impl Responder {
    provider
        .authorization_requests
        .lock()
        .expect("authorize requests lock should not be poisoned")
        .push(query.into_inner());
    HttpResponse::Ok().finish()
}

async fn mock_token(
    provider: web::Data<MockOidcProvider>,
    form: web::Form<TokenRequest>,
) -> impl Responder {
    let request = form.into_inner();
    assert_eq!(request.grant_type, "authorization_code");
    assert_eq!(request.code, "mock-code");
    if let Some(client_id) = request.client_id.as_deref() {
        assert_eq!(client_id, TEST_CLIENT_ID);
    }
    if let Some(client_secret) = request.client_secret.as_deref() {
        assert_eq!(client_secret, "super-secret");
    }
    assert!(!request.redirect_uri.is_empty());
    assert!(
        request
            .code_verifier
            .as_deref()
            .is_some_and(|value| !value.is_empty()),
        "PKCE code_verifier should be sent to token endpoint"
    );
    let nonce = provider.last_authorize_request().nonce;
    HttpResponse::Ok().json(serde_json::json!({
        "access_token": "mock-access-token",
        "token_type": "Bearer",
        "expires_in": 300,
        "id_token": provider.sign_id_token(&nonce)
    }))
}

async fn mock_jwks(provider: web::Data<MockOidcProvider>) -> impl Responder {
    HttpResponse::Ok().json(JwkSet {
        keys: vec![provider.public_jwk()],
    })
}

async fn create_external_auth_provider<S, B, E>(
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

async fn create_external_auth_provider_key<S, B, E>(
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

async fn create_external_auth_provider_with_key<S, B, E>(
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

struct TestOidcProviderOptions {
    display_name_prefix: String,
    issuer_url: String,
    enabled: bool,
    auto_provision_enabled: bool,
    auto_link_verified_email_enabled: bool,
    require_email_verified: bool,
    allowed_domains: Vec<String>,
}

impl TestOidcProviderOptions {
    fn mock(issuer_url: &str) -> Self {
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

async fn create_external_auth_provider_with<S, B, E>(
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

fn created_provider_key(created: &Value) -> String {
    created["data"]["key"]
        .as_str()
        .expect("provider key should be returned")
        .to_string()
}

fn external_auth_provider_model(
    key: &str,
    issuer_url: &str,
    enabled: bool,
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
        scopes: Set("openid email profile".to_string()),
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

fn configure_oidc_public_site_url(state: &aster_drive::runtime::PrimaryAppState) {
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::site_url::PUBLIC_SITE_URL_KEY,
        r#"["http://localhost:8080"]"#,
    ));
}

async fn start_oidc_login<S, B, E>(
    app: &S,
    mock_provider: &MockOidcProvider,
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

async fn finish_oidc_callback<S, B, E>(
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

fn assert_oidc_error_redirect<B>(resp: &ServiceResponse<B>) {
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

fn oidc_email_required_flow<B>(resp: &ServiceResponse<B>) -> String {
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

async fn start_oidc_email_verification<S, B, E>(
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

async fn assert_start_oidc_email_verification_ok<S, B, E>(app: &S, flow_token: &str, email: &str)
where
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

async fn confirm_oidc_email_verification<S, B, E>(app: &S, token: &str) -> ServiceResponse<B>
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

async fn link_oidc_with_password<S, B, E>(
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

async fn latest_oidc_email_verification_token(
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

async fn disable_user(state: &aster_drive::runtime::PrimaryAppState, user_id: i64) {
    let user = user::Entity::find_by_id(user_id)
        .one(&state.db)
        .await
        .expect("user should query")
        .expect("user should exist");
    let mut active = user.into_active_model();
    active.status = Set(aster_drive::types::UserStatus::Disabled);
    active.update(&state.db).await.expect("user should update");
}

fn reserve_localhost_port() -> (u16, std::net::TcpListener) {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("free localhost port should be reserved");
    let port = listener
        .local_addr()
        .expect("reserved listener address should exist")
        .port();
    (port, listener)
}

fn dex_config(issuer: &str, provider_key: &str) -> String {
    format!(
        r#"issuer: {issuer}
storage:
  type: memory
web:
  http: 0.0.0.0:5556
oauth2:
  skipApprovalScreen: true
staticClients:
  - id: {TEST_CLIENT_ID}
    redirectURIs:
      - {TEST_BROWSER_ORIGIN}/api/v1/auth/external-auth/oidc/{provider_key}/callback
    name: AsterDrive Test
    secret: {DEX_TEST_CLIENT_SECRET}
enablePasswordDB: true
staticPasswords:
  - email: "{DEX_TEST_USER_EMAIL}"
    hash: "$2a$10$2b2cU8CPhOTaGrs1HRQuAueS7JTT5ZHsHSzYiFPm1leZck7Mc8T4W"
    username: "dex-user"
    userID: "dex-user-id"
"#
    )
}

async fn wait_for_dex_discovery(issuer: &str) {
    let discovery_url = format!("{issuer}/.well-known/openid-configuration");
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(StdDuration::from_secs(2))
        .build()
        .expect("reqwest client should build");
    let deadline = tokio::time::Instant::now() + StdDuration::from_secs(30);
    let mut last_error: Option<String>;
    loop {
        last_error = match client.get(&discovery_url).send().await {
            Ok(resp) if resp.status().is_success() => return,
            Ok(resp) => Some(format!("HTTP {}", resp.status())),
            Err(err) => Some(err.to_string()),
        };
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for Dex discovery at {discovery_url}: {}",
            last_error.unwrap_or_else(|| "unknown error".to_string())
        );
        tokio::time::sleep(StdDuration::from_millis(250)).await;
    }
}

fn absolute_location(base: &reqwest::Url, location: &str) -> reqwest::Url {
    reqwest::Url::parse(location)
        .or_else(|_| base.join(location))
        .expect("redirect location should be a valid URL")
}

async fn request_dex_redirect(
    client: &reqwest::Client,
    url: reqwest::Url,
) -> (reqwest::Url, Option<reqwest::Url>) {
    let resp = client
        .get(url.clone())
        .send()
        .await
        .expect("Dex redirect GET should succeed");
    if resp.status().is_redirection() {
        let location = resp
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .expect("Dex redirect should include Location");
        return (url.clone(), Some(absolute_location(&url, location)));
    }
    panic!(
        "Dex GET {url} returned non-redirect status {}",
        resp.status()
    );
}

async fn complete_dex_password_login(
    issuer: &str,
    provider_key: &str,
    authorization_url: &str,
) -> String {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(StdDuration::from_secs(10))
        .build()
        .expect("reqwest client should build");
    let issuer_url = reqwest::Url::parse(issuer).expect("Dex issuer URL should parse");
    let mut next =
        reqwest::Url::parse(authorization_url).expect("OIDC authorization URL should parse");
    let mut login_url = None;

    for _ in 0..6 {
        let (_, redirect) = request_dex_redirect(&client, next.clone()).await;
        let redirect = redirect.expect("Dex should keep redirecting until the login form");
        if redirect.path().ends_with("/auth/local/login") {
            login_url = Some(redirect);
            break;
        }
        assert_eq!(
            redirect.domain(),
            issuer_url.domain(),
            "Dex should not redirect to another host before login"
        );
        next = redirect;
    }

    let login_url = login_url.expect("Dex password login URL should be reached");
    let form = format!(
        "login={}&password={}",
        urlencoding::encode(DEX_TEST_USER_EMAIL),
        urlencoding::encode("password")
    );
    let resp = client
        .post(login_url.clone())
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(form)
        .send()
        .await
        .expect("Dex password POST should succeed");
    assert!(
        resp.status().is_redirection(),
        "Dex password POST should redirect, got {}",
        resp.status()
    );
    let location = resp
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .expect("Dex login redirect should include Location");
    let redirect = absolute_location(&login_url, location);
    let expected_callback =
        format!("{TEST_BROWSER_ORIGIN}/api/v1/auth/external-auth/oidc/{provider_key}/callback");
    assert_eq!(
        redirect.as_str().split('?').next(),
        Some(expected_callback.as_str()),
        "Dex should redirect back to AsterDrive callback"
    );
    redirect.to_string()
}

#[actix_web::test]
async fn admin_provider_api_masks_secret_and_public_list_only_shows_enabled() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let created =
        create_external_auth_provider(&app, &admin_token, &mock_provider.issuer, true, false).await;
    let provider_key = created_provider_key(&created);
    assert!(Uuid::parse_str(&provider_key).is_ok());
    assert_eq!(created["data"]["client_secret"], "***REDACTED***");
    assert_eq!(created["data"]["client_secret_configured"], true);
    assert_eq!(
        created["data"]["icon_url"],
        "/static/external-auth/mock.svg"
    );

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/external-auth/providers")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let list_body: Value = test::read_body_json(resp).await;
    assert_eq!(list_body["data"]["total"], 1);
    assert_eq!(
        list_body["data"]["items"][0]["client_secret"],
        "***REDACTED***"
    );

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/external-auth/providers")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let public_body: Value = test::read_body_json(resp).await;
    assert_eq!(public_body["data"][0]["key"], provider_key);
    assert_eq!(
        public_body["data"][0]["icon_url"],
        "/static/external-auth/mock.svg"
    );
    assert!(public_body["data"][0].get("client_secret").is_none());

    let req = test::TestRequest::patch()
        .uri("/api/v1/admin/external-auth/providers/1")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({ "enabled": false }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/external-auth/providers")
        .to_request();
    let resp = test::call_service(&app, req).await;
    let public_body: Value = test::read_body_json(resp).await;
    assert_eq!(public_body["data"].as_array().unwrap().len(), 0);

    server.stop(true).await;
}

#[actix_web::test]
async fn admin_provider_kind_api_drives_create_contract() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/external-auth/provider-kinds")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let kinds = body["data"]
        .as_array()
        .expect("provider kind list should be an array");
    assert_eq!(kinds.len(), 1);
    assert_eq!(kinds[0]["kind"], "oidc");
    assert_eq!(kinds[0]["protocol"], "oidc");
    assert_eq!(kinds[0]["default_scopes"], "openid email profile");
    assert_eq!(kinds[0]["supports_discovery"], true);
    assert_eq!(kinds[0]["supports_pkce"], true);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "provider_kind": "oidc",
            "display_name": "Default Enabled",
            "issuer_url": mock_provider.issuer,
            "client_id": TEST_CLIENT_ID,
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let created: Value = test::read_body_json(resp).await;
    assert_eq!(created["data"]["provider_kind"], "oidc");
    assert_eq!(created["data"]["protocol"], "oidc");
    assert_eq!(created["data"]["enabled"], true);
    assert!(Uuid::parse_str(created["data"]["key"].as_str().unwrap()).is_ok());

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "display_name": "Missing Kind",
            "issuer_url": mock_provider.issuer,
            "client_id": TEST_CLIENT_ID,
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    server.stop(true).await;
}

#[actix_web::test]
async fn admin_tests_external_auth_provider_draft_params_without_persisting() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers/test")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "provider_kind": "oidc",
            "issuer_url": mock_provider.issuer,
            "client_id": TEST_CLIENT_ID,
            "client_secret": "super-secret",
            "scopes": "openid email profile",
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["provider"], "OpenID Connect");
    assert_eq!(body["data"]["issuer"], mock_provider.issuer);
    assert_eq!(
        body["data"]["authorization_endpoint"],
        format!("{}/authorize", mock_provider.issuer)
    );
    assert_eq!(
        body["data"]["token_endpoint"],
        format!("{}/token", mock_provider.issuer)
    );
    assert_eq!(body["data"]["jwks_key_count"], 1);
    assert_eq!(body["data"]["checks"][0]["name"], "discovery");
    assert_eq!(body["data"]["checks"][1]["name"], "jwks");

    let providers = external_auth_provider::Entity::find()
        .all(&state.db)
        .await
        .expect("providers should query");
    assert!(providers.is_empty());

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers/test")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "provider_kind": "oidc",
            "client_id": TEST_CLIENT_ID,
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "provider_kind": "oidc",
            "display_name": "Saved IDP",
            "issuer_url": mock_provider.issuer,
            "client_id": TEST_CLIENT_ID,
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let created: Value = test::read_body_json(resp).await;
    let provider_id = created["data"]["id"]
        .as_i64()
        .expect("provider id should be returned");

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/admin/external-auth/providers/{provider_id}/test"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["issuer"], mock_provider.issuer);

    let providers = external_auth_provider::Entity::find()
        .all(&state.db)
        .await
        .expect("providers should query");
    assert_eq!(providers.len(), 1);

    server.stop(true).await;
}

#[actix_web::test]
async fn start_login_persists_pkce_flow_and_rejects_replayed_state() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    let state = common::setup().await;
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::site_url::PUBLIC_SITE_URL_KEY,
        r#"["http://localhost:8080"]"#,
    ));
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key =
        create_external_auth_provider_key(&app, &admin_token, &mock_provider.issuer, true, false)
            .await;

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/auth/external-auth/oidc/{provider_key}/start"
        ))
        .insert_header(("Origin", TEST_BROWSER_ORIGIN))
        .set_json(serde_json::json!({ "return_path": "/files?view=grid" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let auth_url = body["data"]["authorization_url"]
        .as_str()
        .expect("authorization url should be returned");
    assert!(auth_url.starts_with(&format!("{}/authorize?", mock_provider.issuer)));

    reqwest::get(auth_url)
        .await
        .expect("mock authorize request should succeed");
    let authorize_request = mock_provider.last_authorize_request();
    assert_eq!(authorize_request.response_type, "code");
    assert_eq!(authorize_request.client_id, TEST_CLIENT_ID);
    assert_eq!(
        authorize_request.redirect_uri,
        format!("http://localhost:8080/api/v1/auth/external-auth/oidc/{provider_key}/callback")
    );
    assert!(authorize_request.scope.unwrap().contains("openid"));
    assert_eq!(
        authorize_request.code_challenge_method.as_deref(),
        Some("S256")
    );
    assert!(
        authorize_request
            .code_challenge
            .as_deref()
            .is_some_and(|value| !value.is_empty())
    );

    let flows = external_auth_login_flow::Entity::find()
        .all(&state.db)
        .await
        .expect("flows should query");
    assert_eq!(flows.len(), 1);
    assert_eq!(flows[0].return_path.as_deref(), Some("/files?view=grid"));
    assert_ne!(flows[0].state_hash, authorize_request.state);

    let consumed = external_auth_login_flow_repo::consume_by_state_hash(
        &state.db,
        &aster_drive::utils::hash::sha256_hex(authorize_request.state.as_bytes()),
        Utc::now(),
    )
    .await
    .expect("flow consume should succeed");
    assert!(consumed.is_some());
    let replay = external_auth_login_flow_repo::consume_by_state_hash(
        &state.db,
        &aster_drive::utils::hash::sha256_hex(authorize_request.state.as_bytes()),
        Utc::now(),
    )
    .await
    .expect("flow replay should query");
    assert!(replay.is_none());

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_verifies_jwks_and_issues_asterdrive_cookies() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key =
        create_external_auth_provider_key(&app, &admin_token, &mock_provider.issuer, true, true)
            .await;

    let state_value =
        start_oidc_login(&app, &mock_provider, &provider_key, "/settings/security").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers()
            .get("Location")
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:8080/settings/security")
    );
    assert!(common::extract_cookie(&resp, "aster_access").is_some());
    assert!(common::extract_cookie(&resp, "aster_refresh").is_some());
    assert!(common::extract_cookie(&resp, "aster_csrf").is_some());

    let identities = external_auth_identity::Entity::find()
        .all(&state.db)
        .await
        .expect("identities should query");
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].identity_namespace, mock_provider.issuer);
    assert_eq!(identities[0].subject, "oidc-subject-1");

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_auto_links_verified_email_to_existing_user() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("auto-link-subject");
    mock_provider.set_email("linked@example.com");

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let linked_user_id = admin_create_user!(
        app,
        admin_token,
        "linked-user",
        "linked@example.com",
        "password123"
    );
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions {
            auto_link_verified_email_enabled: true,
            ..TestOidcProviderOptions::mock(&mock_provider.issuer)
        },
    )
    .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/files").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers()
            .get("Location")
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:8080/files")
    );
    assert!(common::extract_cookie(&resp, "aster_access").is_some());
    assert!(common::extract_cookie(&resp, "aster_refresh").is_some());

    let identities = external_auth_identity::Entity::find()
        .all(&state.db)
        .await
        .expect("identities should query");
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].user_id, linked_user_id);
    assert_eq!(identities[0].identity_namespace, mock_provider.issuer);
    assert_eq!(identities[0].subject, "auto-link-subject");

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_falls_back_to_manual_binding_for_unverified_auto_link_email() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("unverified-link-subject");
    mock_provider.set_email("unverified@example.com");
    mock_provider.set_email_verified(false);

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let linked_user_id = admin_create_user!(
        app,
        admin_token,
        "unverified-user",
        "unverified@example.com",
        "password123"
    );
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions {
            auto_link_verified_email_enabled: true,
            require_email_verified: false,
            ..TestOidcProviderOptions::mock(&mock_provider.issuer)
        },
    )
    .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    let flow_token = oidc_email_required_flow(&resp);

    let identities = external_auth_identity::Entity::find()
        .all(&state.db)
        .await
        .expect("identities should query");
    assert!(identities.is_empty());

    let resp = link_oidc_with_password(&app, &flow_token, "unverified-user", "password123").await;
    assert_eq!(resp.status(), 200);
    assert!(common::extract_cookie(&resp, "aster_access").is_some());

    let identities = external_auth_identity::Entity::find()
        .all(&state.db)
        .await
        .expect("identities should query");
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].user_id, linked_user_id);
    assert_eq!(identities[0].subject, "unverified-link-subject");
    assert_eq!(identities[0].email_snapshot.as_deref(), None);

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_rejects_disabled_user_with_existing_identity() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("disabled-subject");
    mock_provider.set_email("disabled@example.com");

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let disabled_user_id = admin_create_user!(
        app,
        admin_token,
        "disabled-user",
        "disabled@example.com",
        "password123"
    );
    let created =
        create_external_auth_provider(&app, &admin_token, &mock_provider.issuer, true, false).await;
    let provider_key = created_provider_key(&created);
    let provider_id = created["data"]["id"]
        .as_i64()
        .expect("provider id should be returned");
    external_auth_identity_repo::create_identity(
        &state.db,
        external_auth_identity_repo::CreateExternalAuthIdentityInput {
            user_id: disabled_user_id,
            provider_id,
            identity_namespace: mock_provider.issuer.clone(),
            subject: "disabled-subject".to_string(),
            email_snapshot: Some("disabled@example.com".to_string()),
            display_name_snapshot: Some("Disabled User".to_string()),
            now: Utc::now(),
        },
    )
    .await
    .expect("identity should create");
    disable_user(&state, disabled_user_id).await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    assert_oidc_error_redirect(&resp);

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_allows_existing_identity_without_email_claim() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("linked-no-email-subject");
    mock_provider.clear_email();

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let linked_user_id = admin_create_user!(
        app,
        admin_token,
        "linked-no-email",
        "linked-no-email@example.com",
        "password123"
    );
    let created =
        create_external_auth_provider(&app, &admin_token, &mock_provider.issuer, true, false).await;
    let provider_key = created_provider_key(&created);
    let provider_id = created["data"]["id"]
        .as_i64()
        .expect("provider id should be returned");
    external_auth_identity_repo::create_identity(
        &state.db,
        external_auth_identity_repo::CreateExternalAuthIdentityInput {
            user_id: linked_user_id,
            provider_id,
            identity_namespace: mock_provider.issuer.clone(),
            subject: "linked-no-email-subject".to_string(),
            email_snapshot: None,
            display_name_snapshot: Some("Linked No Email".to_string()),
            now: Utc::now(),
        },
    )
    .await
    .expect("identity should create");

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/files").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers()
            .get("Location")
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:8080/files")
    );
    assert!(common::extract_cookie(&resp, "aster_access").is_some());

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_falls_back_to_manual_binding_when_auto_provision_disabled() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("unlinked-subject");
    mock_provider.set_email("unlinked@example.com");

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let linked_user_id = admin_create_user!(
        app,
        admin_token,
        "manual-link-user",
        "manual-link@example.com",
        "password123"
    );
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions::mock(&mock_provider.issuer),
    )
    .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    let flow_token = oidc_email_required_flow(&resp);

    let identities = external_auth_identity::Entity::find()
        .all(&state.db)
        .await
        .expect("identities should query");
    assert!(identities.is_empty());
    let users = user::Entity::find()
        .all(&state.db)
        .await
        .expect("users should query");
    assert_eq!(users.len(), 2);

    let resp = link_oidc_with_password(&app, &flow_token, "manual-link-user", "password123").await;
    assert_eq!(resp.status(), 200);
    assert!(common::extract_cookie(&resp, "aster_access").is_some());

    let identities = external_auth_identity::Entity::find()
        .all(&state.db)
        .await
        .expect("identities should query");
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].user_id, linked_user_id);
    assert_eq!(identities[0].subject, "unlinked-subject");
    assert_eq!(identities[0].email_snapshot.as_deref(), None);

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_respects_global_registration_setting_for_auto_provision() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("registration-closed-subject");
    mock_provider.set_email("registration-closed@example.com");

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let existing_user_id = admin_create_user!(
        app,
        admin_token,
        "reg-closed",
        "existing-registration-closed@example.com",
        "password123"
    );
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions {
            auto_provision_enabled: true,
            auto_link_verified_email_enabled: true,
            ..TestOidcProviderOptions::mock(&mock_provider.issuer)
        },
    )
    .await;
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::auth_runtime::AUTH_ALLOW_USER_REGISTRATION_KEY,
        "false",
    ));

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    let flow_token = oidc_email_required_flow(&resp);

    let identities = external_auth_identity::Entity::find()
        .all(&state.db)
        .await
        .expect("identities should query");
    assert!(identities.is_empty());
    let users = user::Entity::find()
        .all(&state.db)
        .await
        .expect("users should query");
    assert_eq!(users.len(), 2);

    let resp = link_oidc_with_password(&app, &flow_token, "reg-closed", "password123").await;
    assert_eq!(resp.status(), 200);
    assert!(common::extract_cookie(&resp, "aster_access").is_some());

    let identities = external_auth_identity::Entity::find()
        .all(&state.db)
        .await
        .expect("identities should query");
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].user_id, existing_user_id);
    assert_eq!(identities[0].subject, "registration-closed-subject");
    assert_eq!(identities[0].email_snapshot.as_deref(), None);
    let users = user::Entity::find()
        .all(&state.db)
        .await
        .expect("users should query");
    assert_eq!(users.len(), 2);

    server.stop(true).await;
}

#[actix_web::test]
async fn manual_binding_respects_global_registration_setting_only_when_email_verification_would_create_user()
 {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("manual-registration-closed-subject");
    mock_provider.set_email("manual-registration-closed@example.com");

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions::mock(&mock_provider.issuer),
    )
    .await;
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::auth_runtime::AUTH_ALLOW_USER_REGISTRATION_KEY,
        "false",
    ));

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    let flow_token = oidc_email_required_flow(&resp);

    let resp =
        start_oidc_email_verification(&app, &flow_token, "manual-registration-closed@example.com")
            .await;
    assert_eq!(resp.status(), 403);

    let identities = external_auth_identity::Entity::find()
        .all(&state.db)
        .await
        .expect("identities should query");
    assert!(identities.is_empty());
    let users = user::Entity::find()
        .all(&state.db)
        .await
        .expect("users should query");
    assert_eq!(users.len(), 1);

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_auto_link_by_verified_email_ignores_global_registration_setting() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("registration-closed-auto-link-subject");
    mock_provider.set_email("auto-link-registration-closed@example.com");

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let linked_user_id = admin_create_user!(
        app,
        admin_token,
        "reg-auto-link",
        "auto-link-registration-closed@example.com",
        "password123"
    );
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions {
            auto_provision_enabled: true,
            auto_link_verified_email_enabled: true,
            ..TestOidcProviderOptions::mock(&mock_provider.issuer)
        },
    )
    .await;
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::auth_runtime::AUTH_ALLOW_USER_REGISTRATION_KEY,
        "false",
    ));

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/files").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers()
            .get("Location")
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:8080/files")
    );
    assert!(common::extract_cookie(&resp, "aster_access").is_some());

    let identities = external_auth_identity::Entity::find()
        .all(&state.db)
        .await
        .expect("identities should query");
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].user_id, linked_user_id);
    assert_eq!(
        identities[0].subject,
        "registration-closed-auto-link-subject"
    );

    server.stop(true).await;
}

#[actix_web::test]
async fn no_email_claim_can_register_after_local_email_verification() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("fallback-provision-subject");
    mock_provider.clear_email();

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions::mock(&mock_provider.issuer),
    )
    .await;

    let state_value =
        start_oidc_login(&app, &mock_provider, &provider_key, "/settings/security").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    let flow_token = oidc_email_required_flow(&resp);

    assert_start_oidc_email_verification_ok(&app, &flow_token, "fallback-provision@example.com")
        .await;
    let token = latest_oidc_email_verification_token(&state).await;
    let resp = confirm_oidc_email_verification(&app, &token).await;
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers()
            .get("Location")
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:8080/settings/security")
    );
    assert!(common::extract_cookie(&resp, "aster_access").is_some());
    assert!(common::extract_cookie(&resp, "aster_refresh").is_some());

    let user = user::Entity::find()
        .filter(user::Column::Email.eq("fallback-provision@example.com"))
        .one(&state.db)
        .await
        .expect("user should query")
        .expect("OIDC verified email should create user");
    assert!(user.email_verified_at.is_some());
    let identities = external_auth_identity::Entity::find()
        .all(&state.db)
        .await
        .expect("identities should query");
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].user_id, user.id);
    assert_eq!(identities[0].subject, "fallback-provision-subject");
    assert_eq!(
        identities[0].email_snapshot.as_deref(),
        Some("fallback-provision@example.com")
    );

    server.stop(true).await;
}

#[actix_web::test]
async fn no_email_claim_falls_back_to_local_email_verification_for_existing_user_link() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("fallback-link-subject");
    mock_provider.clear_email();

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let linked_user_id = admin_create_user!(
        app,
        admin_token,
        "fb-link-user",
        "fallback-link@example.com",
        "password123"
    );
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions {
            auto_link_verified_email_enabled: true,
            ..TestOidcProviderOptions::mock(&mock_provider.issuer)
        },
    )
    .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/files").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    let flow_token = oidc_email_required_flow(&resp);

    assert_start_oidc_email_verification_ok(&app, &flow_token, "fallback-link@example.com").await;
    let token = latest_oidc_email_verification_token(&state).await;
    let resp = confirm_oidc_email_verification(&app, &token).await;
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers()
            .get("Location")
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:8080/files")
    );
    assert!(common::extract_cookie(&resp, "aster_access").is_some());

    let identities = external_auth_identity::Entity::find()
        .all(&state.db)
        .await
        .expect("identities should query");
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].user_id, linked_user_id);
    assert_eq!(identities[0].subject, "fallback-link-subject");

    server.stop(true).await;
}

#[actix_web::test]
async fn manual_email_verification_can_link_existing_user_without_auto_link_enabled() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("manual-email-link-subject");
    mock_provider.clear_email();

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let linked_user_id = admin_create_user!(
        app,
        admin_token,
        "manual-mail-link",
        "manual-email-link@example.com",
        "password123"
    );
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions::mock(&mock_provider.issuer),
    )
    .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/files").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    let flow_token = oidc_email_required_flow(&resp);

    assert_start_oidc_email_verification_ok(&app, &flow_token, "manual-email-link@example.com")
        .await;
    let token = latest_oidc_email_verification_token(&state).await;
    let resp = confirm_oidc_email_verification(&app, &token).await;
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers()
            .get("Location")
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:8080/files")
    );
    assert!(common::extract_cookie(&resp, "aster_access").is_some());

    let identities = external_auth_identity::Entity::find()
        .all(&state.db)
        .await
        .expect("identities should query");
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].user_id, linked_user_id);
    assert_eq!(identities[0].subject, "manual-email-link-subject");
    assert_eq!(
        identities[0].email_snapshot.as_deref(),
        Some("manual-email-link@example.com")
    );

    server.stop(true).await;
}

#[actix_web::test]
async fn no_email_claim_can_link_after_local_password_login() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("fallback-password-link-subject");
    mock_provider.clear_email();

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let linked_user_id = admin_create_user!(
        app,
        admin_token,
        "pwd-link-user",
        "password-link@example.com",
        "password123"
    );
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions::mock(&mock_provider.issuer),
    )
    .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/files").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    let flow_token = oidc_email_required_flow(&resp);

    let resp = link_oidc_with_password(&app, &flow_token, "pwd-link-user", "password123").await;
    assert_eq!(resp.status(), 200);
    assert!(common::extract_cookie(&resp, "aster_access").is_some());
    assert!(common::extract_cookie(&resp, "aster_refresh").is_some());
    assert!(common::extract_cookie(&resp, "aster_csrf").is_some());
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"]["expires_in"].as_u64().is_some());

    let identities = external_auth_identity::Entity::find()
        .all(&state.db)
        .await
        .expect("identities should query");
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].user_id, linked_user_id);
    assert_eq!(identities[0].subject, "fallback-password-link-subject");
    assert_eq!(identities[0].email_snapshot.as_deref(), None);

    let resp = link_oidc_with_password(&app, &flow_token, "pwd-link-user", "password123").await;
    assert_eq!(resp.status(), 400);

    server.stop(true).await;
}

#[actix_web::test]
async fn oidc_password_link_rejects_wrong_password_without_sending_email() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("fallback-password-link-wrong-subject");
    mock_provider.clear_email();

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    admin_create_user!(
        app,
        admin_token,
        "pwd-link-wrong",
        "password-link-wrong@example.com",
        "password123"
    );
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions::mock(&mock_provider.issuer),
    )
    .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    let flow_token = oidc_email_required_flow(&resp);

    let resp = link_oidc_with_password(&app, &flow_token, "pwd-link-wrong", "wrong-password").await;
    assert_eq!(resp.status(), 401);
    assert!(common::extract_cookie(&resp, "aster_access").is_none());

    let identities = external_auth_identity::Entity::find()
        .all(&state.db)
        .await
        .expect("identities should query");
    assert!(identities.is_empty());
    let email_flows = external_auth_email_verification_flow::Entity::find()
        .all(&state.db)
        .await
        .expect("email verification flows should query");
    assert_eq!(email_flows.len(), 1);
    assert!(email_flows[0].verification_token_hash.is_none());
    assert!(email_flows[0].consumed_at.is_none());

    server.stop(true).await;
}

#[actix_web::test]
async fn oidc_email_verification_respects_global_registration_setting() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("fallback-registration-closed-subject");
    mock_provider.clear_email();

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions::mock(&mock_provider.issuer),
    )
    .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    let flow_token = oidc_email_required_flow(&resp);
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::auth_runtime::AUTH_ALLOW_USER_REGISTRATION_KEY,
        "false",
    ));

    let resp = start_oidc_email_verification(
        &app,
        &flow_token,
        "registration-closed-fallback@example.com",
    )
    .await;
    assert_eq!(resp.status(), 403);

    let users = user::Entity::find()
        .all(&state.db)
        .await
        .expect("users should query");
    assert_eq!(users.len(), 1);
    let identities = external_auth_identity::Entity::find()
        .all(&state.db)
        .await
        .expect("identities should query");
    assert!(identities.is_empty());

    server.stop(true).await;
}

#[actix_web::test]
async fn oidc_email_verification_enforces_entered_email_domain() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("fallback-domain-subject");
    mock_provider.clear_email();

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions {
            auto_provision_enabled: true,
            ..TestOidcProviderOptions::mock(&mock_provider.issuer)
        },
    )
    .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    let flow_token = oidc_email_required_flow(&resp);

    let resp = start_oidc_email_verification(&app, &flow_token, "user@example.org").await;
    assert_eq!(resp.status(), 403);
    let users = user::Entity::find()
        .all(&state.db)
        .await
        .expect("users should query");
    assert_eq!(users.len(), 1);

    server.stop(true).await;
}

#[actix_web::test]
async fn oidc_email_verification_rejects_replay() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("fallback-replay-subject");
    mock_provider.clear_email();

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions {
            auto_provision_enabled: true,
            ..TestOidcProviderOptions::mock(&mock_provider.issuer)
        },
    )
    .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    let flow_token = oidc_email_required_flow(&resp);
    assert_start_oidc_email_verification_ok(&app, &flow_token, "fallback-replay@example.com").await;
    let token = latest_oidc_email_verification_token(&state).await;

    let resp = confirm_oidc_email_verification(&app, &token).await;
    assert_eq!(resp.status(), 302);
    assert!(common::extract_cookie(&resp, "aster_access").is_some());
    let resp = confirm_oidc_email_verification(&app, &token).await;
    assert_eq!(resp.status(), 302);
    let location = resp
        .headers()
        .get("Location")
        .and_then(|value| value.to_str().ok())
        .expect("replay redirect should have location");
    assert_eq!(
        location,
        "http://localhost:8080/login?external_auth=email_verification_invalid"
    );
    assert!(common::extract_cookie(&resp, "aster_access").is_none());
    let identities = external_auth_identity::Entity::find()
        .all(&state.db)
        .await
        .expect("identities should query");
    assert_eq!(identities.len(), 1);

    server.stop(true).await;
}

#[actix_web::test]
async fn oidc_email_verification_rejects_expired_pending_flow() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("fallback-expired-subject");
    mock_provider.clear_email();

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions {
            auto_provision_enabled: true,
            ..TestOidcProviderOptions::mock(&mock_provider.issuer)
        },
    )
    .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    let flow_token = oidc_email_required_flow(&resp);
    let mut flow = external_auth_email_verification_flow::Entity::find()
        .one(&state.db)
        .await
        .expect("flow should query")
        .expect("flow should exist")
        .into_active_model();
    flow.expires_at = Set(Utc::now() - Duration::minutes(1));
    flow.update(&state.db).await.expect("flow should update");

    let resp =
        start_oidc_email_verification(&app, &flow_token, "fallback-expired@example.com").await;
    assert_eq!(resp.status(), 400);

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_rejects_flow_after_provider_disabled() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let created =
        create_external_auth_provider(&app, &admin_token, &mock_provider.issuer, true, true).await;
    let provider_key = created_provider_key(&created);
    let provider_id = created["data"]["id"]
        .as_i64()
        .expect("provider id should be returned");
    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;

    let req = test::TestRequest::patch()
        .uri(&format!(
            "/api/v1/admin/external-auth/providers/{provider_id}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({ "enabled": false }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    assert_oidc_error_redirect(&resp);
    let identities = external_auth_identity::Entity::find()
        .all(&state.db)
        .await
        .expect("identities should query");
    assert!(identities.is_empty());

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_rejects_audience_mismatch() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_audience("wrong-client-id");

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key =
        create_external_auth_provider_key(&app, &admin_token, &mock_provider.issuer, true, true)
            .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    assert_oidc_error_redirect(&resp);

    let identities = external_auth_identity::Entity::find()
        .all(&state.db)
        .await
        .expect("identities should query");
    assert!(identities.is_empty());

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_rejects_oversized_subject_before_db_write() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject(&"s".repeat(256));

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key =
        create_external_auth_provider_key(&app, &admin_token, &mock_provider.issuer, true, true)
            .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    assert_oidc_error_redirect(&resp);

    let identities = external_auth_identity::Entity::find()
        .all(&state.db)
        .await
        .expect("identities should query");
    assert!(identities.is_empty());

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_rejects_nonce_mismatch() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key =
        create_external_auth_provider_key(&app, &admin_token, &mock_provider.issuer, true, true)
            .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    mock_provider.set_nonce_override(Some("wrong-nonce".to_string()));
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    assert_oidc_error_redirect(&resp);

    let identities = external_auth_identity::Entity::find()
        .all(&state.db)
        .await
        .expect("identities should query");
    assert!(identities.is_empty());

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_rejects_provider_key_mismatch() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key =
        create_external_auth_provider_key(&app, &admin_token, &mock_provider.issuer, true, true)
            .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oidc_callback(&app, "other", &state_value).await;
    assert_oidc_error_redirect(&resp);

    let identities = external_auth_identity::Entity::find()
        .all(&state.db)
        .await
        .expect("identities should query");
    assert!(identities.is_empty());

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_enforces_allowed_domains() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("domain-subject");
    mock_provider.set_email("domain-user@example.com");

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions {
            auto_provision_enabled: true,
            allowed_domains: vec!["example.org".to_string()],
            ..TestOidcProviderOptions::mock(&mock_provider.issuer)
        },
    )
    .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    assert_oidc_error_redirect(&resp);

    let identities = external_auth_identity::Entity::find()
        .all(&state.db)
        .await
        .expect("identities should query");
    assert!(identities.is_empty());
    let users = user::Entity::find()
        .all(&state.db)
        .await
        .expect("users should query");
    assert_eq!(users.len(), 1);

    server.stop(true).await;
}

#[actix_web::test]
async fn oidc_links_can_be_listed_and_deleted_after_login() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("links-subject");
    mock_provider.set_email("links-user@example.com");

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key =
        create_external_auth_provider_key(&app, &admin_token, &mock_provider.issuer, true, true)
            .await;

    let state_value =
        start_oidc_login(&app, &mock_provider, &provider_key, "/settings/security").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    assert_eq!(resp.status(), 302);
    let access_token =
        common::extract_cookie(&resp, "aster_access").expect("access cookie should be set");

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/external-auth/links")
        .insert_header(("Cookie", common::access_cookie_header(&access_token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let links = body["data"]
        .as_array()
        .expect("links response should be an array");
    assert_eq!(links.len(), 1);
    assert_eq!(links[0]["provider_key"], provider_key);
    assert_eq!(links[0]["subject"], "links-subject");
    let link_id = links[0]["id"].as_i64().expect("link id should exist");

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/auth/external-auth/links/{link_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&access_token)))
        .insert_header(common::csrf_header_for(&access_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/external-auth/links")
        .insert_header(("Cookie", common::access_cookie_header(&access_token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 0);

    let identities = external_auth_identity::Entity::find()
        .all(&state.db)
        .await
        .expect("identities should query");
    assert!(identities.is_empty());

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_rejects_issuer_mismatch_after_id_token_verification() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key =
        create_external_auth_provider_key(&app, &admin_token, &mock_provider.issuer, true, true)
            .await;
    mock_provider.set_issuer_override(Some("http://evil.example.test".to_string()));

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/auth/external-auth/oidc/{provider_key}/start"
        ))
        .insert_header(("Origin", TEST_BROWSER_ORIGIN))
        .set_json(serde_json::json!({ "return_path": "/" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    reqwest::get(body["data"]["authorization_url"].as_str().unwrap())
        .await
        .expect("mock authorize request should succeed");
    let state_value = mock_provider.last_authorize_request().state;

    let callback = format!(
        "/api/v1/auth/external-auth/oidc/{provider_key}/callback?code=mock-code&state={state_value}"
    );
    let req = test::TestRequest::get().uri(&callback).to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 302);
    let location = resp
        .headers()
        .get("Location")
        .and_then(|value| value.to_str().ok())
        .unwrap();
    assert!(location.starts_with("http://localhost:8080/login?external_auth=error"));
    assert!(common::extract_cookie(&resp, "aster_access").is_none());

    let identities = external_auth_identity::Entity::find()
        .all(&state.db)
        .await
        .expect("identities should query");
    assert!(identities.is_empty());

    server.stop(true).await;
}

#[actix_web::test]
async fn external_auth_identity_lookup_uses_namespace_subject_not_provider_id() {
    let state = common::setup().await;
    let provider_a = external_auth_provider_repo::create(
        &state.db,
        external_auth_provider_model("a", "http://issuer.example.test", true),
    )
    .await
    .expect("provider a should create");
    let provider_b = external_auth_provider_repo::create(
        &state.db,
        external_auth_provider_model("b", "http://issuer.example.test", true),
    )
    .await
    .expect("provider b should create");
    let (admin_token, _) = {
        let app = create_test_app!(state.clone());
        register_and_login!(app)
    };
    let claims = aster_drive::services::auth_service::verify_token(
        &admin_token,
        &state.config.auth.jwt_secret,
    )
    .expect("admin token should verify");

    external_auth_identity_repo::create_identity(
        &state.db,
        external_auth_identity_repo::CreateExternalAuthIdentityInput {
            user_id: claims.user_id,
            provider_id: provider_a.id,
            identity_namespace: provider_a
                .issuer_url
                .clone()
                .expect("issuer url should exist"),
            subject: "shared-subject".to_string(),
            email_snapshot: Some("a@example.com".to_string()),
            display_name_snapshot: Some("Provider A".to_string()),
            now: Utc::now(),
        },
    )
    .await
    .expect("identity should create");

    let found = external_auth_identity_repo::find_by_identity_namespace_subject(
        &state.db,
        provider_b
            .issuer_url
            .as_deref()
            .expect("issuer url should exist"),
        "shared-subject",
    )
    .await
    .expect("identity lookup should succeed")
    .expect("identity should be found by identity namespace+subject");
    assert_eq!(found.provider_id, provider_a.id);

    let duplicate = external_auth_identity_repo::create_identity(
        &state.db,
        external_auth_identity_repo::CreateExternalAuthIdentityInput {
            user_id: claims.user_id,
            provider_id: provider_b.id,
            identity_namespace: provider_b
                .issuer_url
                .clone()
                .expect("issuer url should exist"),
            subject: "shared-subject".to_string(),
            email_snapshot: Some("b@example.com".to_string()),
            display_name_snapshot: Some("Provider B".to_string()),
            now: Utc::now(),
        },
    )
    .await;
    assert!(duplicate.is_err());
}

#[actix_web::test]
async fn cleanup_expired_flows_removes_only_expired_rows() {
    let state = common::setup().await;
    let provider = external_auth_provider_repo::create(
        &state.db,
        external_auth_provider_model("cleanup", "http://cleanup.example.test", true),
    )
    .await
    .expect("provider should create");
    let now = Utc::now();
    for (state_hash, expires_at) in [
        ("expired", now - Duration::minutes(1)),
        ("active", now + Duration::minutes(5)),
    ] {
        external_auth_login_flow_repo::create(
            &state.db,
            external_auth_login_flow::ActiveModel {
                provider_id: Set(provider.id),
                state_hash: Set(state_hash.to_string()),
                nonce: Set(Some(format!("{state_hash}-nonce"))),
                pkce_verifier: Set(Some(format!("{state_hash}-verifier"))),
                redirect_uri: Set("http://localhost/callback".to_string()),
                return_path: Set(Some("/".to_string())),
                created_at: Set(now),
                expires_at: Set(expires_at),
                consumed_at: Set(None),
                ..Default::default()
            },
        )
        .await
        .expect("flow should create");
    }
    for (flow_token_hash, expires_at) in [
        ("expired-email-flow", now - Duration::minutes(1)),
        ("active-email-flow", now + Duration::minutes(5)),
    ] {
        external_auth_email_verification_flow::ActiveModel {
            provider_id: Set(provider.id),
            identity_namespace: Set("http://cleanup.example.test".to_string()),
            subject: Set(format!("{flow_token_hash}-subject")),
            target_email: Set(None),
            display_name_snapshot: Set(None),
            preferred_username_snapshot: Set(None),
            return_path: Set(Some("/".to_string())),
            flow_token_hash: Set(flow_token_hash.to_string()),
            verification_token_hash: Set(None),
            email_requested_at: Set(None),
            created_at: Set(now),
            expires_at: Set(expires_at),
            consumed_at: Set(None),
            ..Default::default()
        }
        .insert(&state.db)
        .await
        .expect("email verification flow should create");
    }

    let removed = external_auth_service::cleanup_expired_flows(&state)
        .await
        .expect("cleanup should succeed");
    assert_eq!(removed, 2);
    let flows = external_auth_login_flow::Entity::find()
        .all(&state.db)
        .await
        .expect("flows should query");
    assert_eq!(flows.len(), 1);
    assert_eq!(flows[0].state_hash, "active");
    let email_flows = external_auth_email_verification_flow::Entity::find()
        .all(&state.db)
        .await
        .expect("email verification flows should query");
    assert_eq!(email_flows.len(), 1);
    assert_eq!(email_flows[0].flow_token_hash, "active-email-flow");
}

/// Dex 容器端到端 smoke：真实 discovery/JWKS/auth-code/token 交换链路。
///
#[actix_web::test]
async fn dex_container_authorization_code_login_e2e() {
    use testcontainers::{
        GenericImage, ImageExt,
        core::{IntoContainerPort, WaitFor},
        runners::AsyncRunner,
    };

    let (dex_port, listener) = reserve_localhost_port();
    let dex_issuer = format!("http://127.0.0.1:{dex_port}");

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions {
            auto_provision_enabled: true,
            ..TestOidcProviderOptions::mock(&dex_issuer)
        },
    )
    .await;
    let config = dex_config(&dex_issuer, &provider_key);
    drop(listener);

    let _container = GenericImage::new("ghcr.io/dexidp/dex", DEX_TEST_IMAGE_TAG)
        .with_wait_for(WaitFor::message_on_either_std("listening on"))
        .with_mapped_port(dex_port, 5556.tcp())
        .with_copy_to("/etc/dex/config.asterdrive-test.yaml", config.into_bytes())
        .with_cmd(["dex", "serve", "/etc/dex/config.asterdrive-test.yaml"])
        .start()
        .await
        .expect("failed to start Dex OIDC container");
    wait_for_dex_discovery(&dex_issuer).await;

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/auth/external-auth/oidc/{provider_key}/start"
        ))
        .insert_header(("Origin", TEST_BROWSER_ORIGIN))
        .set_json(serde_json::json!({ "return_path": "/settings/security" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let authorization_url = body["data"]["authorization_url"]
        .as_str()
        .expect("authorization url should be returned");

    let callback_url =
        complete_dex_password_login(&dex_issuer, &provider_key, authorization_url).await;
    let parsed_callback = reqwest::Url::parse(&callback_url).expect("callback URL should parse");
    let callback_path_and_query = parsed_callback[url::Position::BeforePath..].to_string();
    let req = test::TestRequest::get()
        .uri(&callback_path_and_query)
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers()
            .get("Location")
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:8080/settings/security")
    );
    assert!(common::extract_cookie(&resp, "aster_access").is_some());
    assert!(common::extract_cookie(&resp, "aster_refresh").is_some());
    assert!(common::extract_cookie(&resp, "aster_csrf").is_some());

    let identities = external_auth_identity::Entity::find()
        .all(&state.db)
        .await
        .expect("identities should query");
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].identity_namespace, dex_issuer);
    assert_eq!(identities[0].subject, DEX_TEST_USER_SUBJECT);
    assert_eq!(
        identities[0].email_snapshot.as_deref(),
        Some(DEX_TEST_USER_EMAIL)
    );
}

use actix_web::{App, HttpRequest, HttpResponse, HttpServer, Responder, web};
use aster_drive::utils::OUTBOUND_HTTP_USER_AGENT;
use base64::Engine as _;
use serde::Deserialize;
use std::sync::{Arc, Mutex};

use super::{TEST_CLIENT_ID, TEST_CLIENT_SECRET};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenAuthObservation {
    Basic,
    Post,
    None,
}

#[derive(Clone)]
pub struct MockOAuth2Provider {
    pub base_url: String,
    authorization_requests: Arc<Mutex<Vec<AuthorizeRequest>>>,
    expected_token_auth: Arc<Mutex<TokenAuthObservation>>,
    token_auth_observations: Arc<Mutex<Vec<TokenAuthObservation>>>,
    profile_subject: Arc<Mutex<Option<String>>>,
    profile_email: Arc<Mutex<Option<String>>>,
    profile_email_verified: Arc<Mutex<Option<bool>>>,
    github_emails: Arc<Mutex<Vec<GitHubEmailEntry>>>,
    github_emails_status: Arc<Mutex<actix_web::http::StatusCode>>,
    qq_openid: Arc<Mutex<String>>,
    qq_openid_client_id: Arc<Mutex<String>>,
    qq_userinfo_ret: Arc<Mutex<i64>>,
    qq_userinfo_msg: Arc<Mutex<String>>,
    qq_nickname: Arc<Mutex<String>>,
}

#[derive(Clone, Debug)]
pub struct GitHubEmailEntry {
    pub email: String,
    pub primary: bool,
    pub verified: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AuthorizeRequest {
    pub response_type: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub scope: Option<String>,
    pub state: String,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
    pub nonce: Option<String>,
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

#[derive(Debug, Deserialize)]
struct QqTokenRequest {
    grant_type: String,
    client_id: String,
    client_secret: Option<String>,
    code: String,
    redirect_uri: String,
    code_verifier: Option<String>,
    fmt: String,
}

#[derive(Debug, Deserialize)]
struct QqOpenIdRequest {
    access_token: String,
    fmt: String,
}

#[derive(Debug, Deserialize)]
struct QqUserInfoRequest {
    access_token: String,
    oauth_consumer_key: String,
    openid: String,
}

impl MockOAuth2Provider {
    fn new() -> Self {
        Self {
            base_url: String::new(),
            authorization_requests: Arc::new(Mutex::new(Vec::new())),
            expected_token_auth: Arc::new(Mutex::new(TokenAuthObservation::Post)),
            token_auth_observations: Arc::new(Mutex::new(Vec::new())),
            profile_subject: Arc::new(Mutex::new(Some("oauth2-subject-1".to_string()))),
            profile_email: Arc::new(Mutex::new(Some("oauth2-user@example.com".to_string()))),
            profile_email_verified: Arc::new(Mutex::new(Some(true))),
            github_emails: Arc::new(Mutex::new(vec![GitHubEmailEntry {
                email: "github-primary@example.com".to_string(),
                primary: true,
                verified: true,
            }])),
            github_emails_status: Arc::new(Mutex::new(actix_web::http::StatusCode::OK)),
            qq_openid: Arc::new(Mutex::new("qq-openid-1".to_string())),
            qq_openid_client_id: Arc::new(Mutex::new(TEST_CLIENT_ID.to_string())),
            qq_userinfo_ret: Arc::new(Mutex::new(0)),
            qq_userinfo_msg: Arc::new(Mutex::new(String::new())),
            qq_nickname: Arc::new(Mutex::new("QQ Test User".to_string())),
        }
    }

    fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    pub fn last_authorize_request(&self) -> AuthorizeRequest {
        self.authorization_requests
            .lock()
            .expect("authorize requests lock should not be poisoned")
            .last()
            .expect("authorization request should be recorded")
            .clone()
    }

    pub fn token_auth_observations(&self) -> Vec<TokenAuthObservation> {
        self.token_auth_observations
            .lock()
            .expect("token auth observations lock should not be poisoned")
            .clone()
    }

    pub fn set_expected_token_auth(&self, expected: TokenAuthObservation) {
        *self
            .expected_token_auth
            .lock()
            .expect("expected token auth lock should not be poisoned") = expected;
    }

    pub fn set_subject(&self, subject: Option<&str>) {
        *self
            .profile_subject
            .lock()
            .expect("subject lock should not be poisoned") = subject.map(str::to_string);
    }

    pub fn set_email(&self, email: Option<&str>) {
        *self
            .profile_email
            .lock()
            .expect("email lock should not be poisoned") = email.map(str::to_string);
    }

    pub fn set_email_verified(&self, verified: Option<bool>) {
        *self
            .profile_email_verified
            .lock()
            .expect("email verified lock should not be poisoned") = verified;
    }

    pub fn set_github_emails(&self, emails: Vec<GitHubEmailEntry>) {
        *self
            .github_emails
            .lock()
            .expect("GitHub emails lock should not be poisoned") = emails;
    }

    pub fn set_github_emails_status(&self, status: actix_web::http::StatusCode) {
        *self
            .github_emails_status
            .lock()
            .expect("GitHub emails status lock should not be poisoned") = status;
    }

    pub fn set_qq_openid(&self, openid: &str) {
        *self
            .qq_openid
            .lock()
            .expect("QQ openid lock should not be poisoned") = openid.to_string();
    }

    pub fn set_qq_openid_client_id(&self, client_id: &str) {
        *self
            .qq_openid_client_id
            .lock()
            .expect("QQ openid client id lock should not be poisoned") = client_id.to_string();
    }

    pub fn set_qq_userinfo_error(&self, ret: i64, msg: &str) {
        *self
            .qq_userinfo_ret
            .lock()
            .expect("QQ userinfo ret lock should not be poisoned") = ret;
        *self
            .qq_userinfo_msg
            .lock()
            .expect("QQ userinfo msg lock should not be poisoned") = msg.to_string();
    }

    fn userinfo_payload(&self) -> serde_json::Value {
        let subject = self
            .profile_subject
            .lock()
            .expect("subject lock should not be poisoned")
            .clone();
        let email = self
            .profile_email
            .lock()
            .expect("email lock should not be poisoned")
            .clone();
        let email_verified = *self
            .profile_email_verified
            .lock()
            .expect("email verified lock should not be poisoned");
        let mut payload = serde_json::json!({
            "login": "oauth2test",
            "name": "OAuth2 Test User"
        });
        if let Some(subject) = subject {
            payload["id"] = serde_json::json!(subject);
        }
        if let Some(email) = email {
            payload["email"] = serde_json::json!(email);
        }
        if let Some(email_verified) = email_verified {
            payload["email_verified"] = serde_json::json!(email_verified);
        }
        payload
    }

    fn github_emails_payload(&self) -> serde_json::Value {
        let emails = self
            .github_emails
            .lock()
            .expect("GitHub emails lock should not be poisoned")
            .clone();
        serde_json::Value::Array(
            emails
                .into_iter()
                .map(|entry| {
                    serde_json::json!({
                        "email": entry.email,
                        "primary": entry.primary,
                        "verified": entry.verified
                    })
                })
                .collect(),
        )
    }
}

pub async fn start_mock_oauth2_provider() -> (MockOAuth2Provider, actix_web::dev::ServerHandle) {
    let seed = MockOAuth2Provider::new();
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("listener should bind");
    let addr = listener
        .local_addr()
        .expect("listener address should exist");
    let provider = seed.with_base_url(format!(
        "http://127.0.0.1:{addr_port}",
        addr_port = addr.port()
    ));
    let app_provider = provider.clone();
    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(app_provider.clone()))
            .route("/authorize", web::get().to(mock_authorize))
            .route("/token", web::post().to(mock_token))
            .route("/userinfo", web::get().to(mock_userinfo))
            .route("/user", web::get().to(mock_userinfo))
            .route("/user/emails", web::get().to(mock_github_emails))
            .route("/qq/token", web::get().to(mock_qq_token))
            .route("/qq/me", web::get().to(mock_qq_openid))
            .route("/qq/get_user_info", web::get().to(mock_qq_userinfo))
    })
    .listen(listener)
    .expect("mock OAuth2 server should listen")
    .run();
    let handle = server.handle();
    tokio::spawn(server);
    (provider, handle)
}

async fn mock_authorize(
    provider: web::Data<MockOAuth2Provider>,
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
    provider: web::Data<MockOAuth2Provider>,
    req: HttpRequest,
    form: web::Form<TokenRequest>,
) -> impl Responder {
    let request = form.into_inner();
    assert_eq!(request.grant_type, "authorization_code");
    assert_eq!(request.code, "mock-code");
    assert!(!request.redirect_uri.is_empty());
    assert!(
        request
            .code_verifier
            .as_deref()
            .is_some_and(|value| !value.is_empty()),
        "PKCE code_verifier should be sent to token endpoint"
    );

    let auth_observation = token_auth_observation(&req, &request);
    provider
        .token_auth_observations
        .lock()
        .expect("token auth observations lock should not be poisoned")
        .push(auth_observation);

    let expected_auth = *provider
        .expected_token_auth
        .lock()
        .expect("expected token auth lock should not be poisoned");
    if auth_observation != expected_auth {
        return HttpResponse::Unauthorized().json(serde_json::json!({
            "error": "invalid_client"
        }));
    }
    match expected_auth {
        TokenAuthObservation::Basic => {
            assert_eq!(
                basic_credentials(&req),
                Some((TEST_CLIENT_ID.to_string(), TEST_CLIENT_SECRET.to_string()))
            );
            assert_eq!(request.client_id, None);
            assert_eq!(request.client_secret, None);
        }
        TokenAuthObservation::Post => {
            assert_eq!(request.client_id.as_deref(), Some(TEST_CLIENT_ID));
            assert_eq!(request.client_secret.as_deref(), Some(TEST_CLIENT_SECRET));
        }
        TokenAuthObservation::None => {
            assert_eq!(request.client_id.as_deref(), Some(TEST_CLIENT_ID));
            assert_eq!(request.client_secret, None);
        }
    }

    HttpResponse::Ok().json(serde_json::json!({
        "access_token": "mock-access-token",
        "token_type": "Bearer",
        "expires_in": 300
    }))
}

fn token_auth_observation(req: &HttpRequest, form: &TokenRequest) -> TokenAuthObservation {
    if basic_credentials(req).is_some() {
        return TokenAuthObservation::Basic;
    }
    if form.client_secret.is_some() {
        return TokenAuthObservation::Post;
    }
    TokenAuthObservation::None
}

fn basic_credentials(req: &HttpRequest) -> Option<(String, String)> {
    let header = req
        .headers()
        .get("Authorization")
        .and_then(|value| value.to_str().ok())?;
    let encoded = header.strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    let (client_id, client_secret) = decoded.split_once(':')?;
    Some((client_id.to_string(), client_secret.to_string()))
}

async fn mock_userinfo(
    provider: web::Data<MockOAuth2Provider>,
    req: HttpRequest,
) -> impl Responder {
    let auth = req
        .headers()
        .get("Authorization")
        .and_then(|value| value.to_str().ok());
    assert_eq!(auth, Some("Bearer mock-access-token"));
    HttpResponse::Ok().json(provider.userinfo_payload())
}

async fn mock_github_emails(
    provider: web::Data<MockOAuth2Provider>,
    req: HttpRequest,
) -> impl Responder {
    let auth = req
        .headers()
        .get("Authorization")
        .and_then(|value| value.to_str().ok());
    assert_eq!(auth, Some("Bearer mock-access-token"));
    let user_agent = req
        .headers()
        .get("User-Agent")
        .and_then(|value| value.to_str().ok());
    // GitHub rejects API requests without a User-Agent; keep the mock strict so
    // the provider cannot regress silently.
    assert_eq!(user_agent, Some(OUTBOUND_HTTP_USER_AGENT));
    let status = *provider
        .github_emails_status
        .lock()
        .expect("GitHub emails status lock should not be poisoned");
    if !status.is_success() {
        return HttpResponse::build(status).json(serde_json::json!({
            "message": "mock GitHub emails error"
        }));
    }
    HttpResponse::Ok().json(provider.github_emails_payload())
}

async fn mock_qq_token(
    provider: web::Data<MockOAuth2Provider>,
    query: web::Query<QqTokenRequest>,
) -> impl Responder {
    let request = query.into_inner();
    assert_eq!(request.grant_type, "authorization_code");
    assert_eq!(request.client_id, TEST_CLIENT_ID);
    assert_eq!(request.client_secret.as_deref(), Some(TEST_CLIENT_SECRET));
    assert_eq!(request.code, "mock-code");
    assert!(!request.redirect_uri.is_empty());
    assert!(
        request
            .code_verifier
            .as_deref()
            .is_some_and(|value| !value.is_empty()),
        "QQ token request should include PKCE code_verifier"
    );
    assert_eq!(request.fmt, "json");
    provider
        .token_auth_observations
        .lock()
        .expect("token auth observations lock should not be poisoned")
        .push(TokenAuthObservation::Post);
    HttpResponse::Ok().json(serde_json::json!({
        "access_token": "mock-qq-access-token",
        "expires_in": "7776000"
    }))
}

async fn mock_qq_openid(
    provider: web::Data<MockOAuth2Provider>,
    query: web::Query<QqOpenIdRequest>,
) -> impl Responder {
    let request = query.into_inner();
    assert_eq!(request.access_token, "mock-qq-access-token");
    assert_eq!(request.fmt, "json");
    HttpResponse::Ok().json(serde_json::json!({
        "client_id": provider
            .qq_openid_client_id
            .lock()
            .expect("QQ openid client id lock should not be poisoned")
            .clone(),
        "openid": provider
            .qq_openid
            .lock()
            .expect("QQ openid lock should not be poisoned")
            .clone()
    }))
}

async fn mock_qq_userinfo(
    provider: web::Data<MockOAuth2Provider>,
    query: web::Query<QqUserInfoRequest>,
) -> impl Responder {
    let request = query.into_inner();
    assert_eq!(request.access_token, "mock-qq-access-token");
    assert_eq!(request.oauth_consumer_key, TEST_CLIENT_ID);
    assert_eq!(
        request.openid,
        provider
            .qq_openid
            .lock()
            .expect("QQ openid lock should not be poisoned")
            .as_str()
    );
    let ret = *provider
        .qq_userinfo_ret
        .lock()
        .expect("QQ userinfo ret lock should not be poisoned");
    HttpResponse::Ok().json(serde_json::json!({
        "ret": ret,
        "msg": provider
            .qq_userinfo_msg
            .lock()
            .expect("QQ userinfo msg lock should not be poisoned")
            .clone(),
        "nickname": provider
            .qq_nickname
            .lock()
            .expect("QQ nickname lock should not be poisoned")
            .clone(),
        "figureurl_qq_2": "https://q.qlogo.cn/qqapp/100000001/qq-openid-1/100"
    }))
}

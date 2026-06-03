use actix_web::{App, HttpResponse, HttpServer, Responder, web};
use base64::Engine as _;
use chrono::{Duration, Utc};
use jsonwebtoken::{
    Algorithm, EncodingKey, Header,
    jwk::{
        AlgorithmParameters, CommonParameters, Jwk, JwkSet, KeyAlgorithm, PublicKeyUse,
        RSAKeyParameters,
    },
};
use rand_08::rngs::OsRng;
use ring::signature::{KeyPair, RsaKeyPair, RsaPublicKeyComponents};
use rsa::{RsaPrivateKey, pkcs1::EncodeRsaPrivateKey};
use serde::Deserialize;
use std::sync::{Arc, Mutex};

use super::TEST_CLIENT_ID;

const TEST_KID: &str = "aster-test-kid";

#[derive(Clone)]
struct StaticRsaKey {
    private_der: Vec<u8>,
    modulus: Vec<u8>,
    exponent: Vec<u8>,
}

#[derive(Clone)]
pub struct MockOidcProvider {
    pub issuer: String,
    key: Arc<StaticRsaKey>,
    authorization_requests: Arc<Mutex<Vec<AuthorizeRequest>>>,
    token_subject: Arc<Mutex<String>>,
    token_email: Arc<Mutex<Option<String>>>,
    token_email_verified: Arc<Mutex<Option<serde_json::Value>>>,
    token_audience: Arc<Mutex<String>>,
    token_nonce_override: Arc<Mutex<Option<String>>>,
    token_issuer_override: Arc<Mutex<Option<String>>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AuthorizeRequest {
    pub response_type: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub scope: Option<String>,
    pub state: String,
    pub nonce: String,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
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
        let key = StaticRsaKey::generate();
        Self {
            issuer: String::new(),
            key: Arc::new(key),
            authorization_requests: Arc::new(Mutex::new(Vec::new())),
            token_subject: Arc::new(Mutex::new("oidc-subject-1".to_string())),
            token_email: Arc::new(Mutex::new(Some("oidc-user@example.com".to_string()))),
            token_email_verified: Arc::new(Mutex::new(Some(serde_json::json!(true)))),
            token_audience: Arc::new(Mutex::new(TEST_CLIENT_ID.to_string())),
            token_nonce_override: Arc::new(Mutex::new(None)),
            token_issuer_override: Arc::new(Mutex::new(None)),
        }
    }

    fn with_issuer(mut self, issuer: String) -> Self {
        self.issuer = issuer;
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

    pub fn set_issuer_override(&self, issuer: Option<String>) {
        *self
            .token_issuer_override
            .lock()
            .expect("issuer override lock should not be poisoned") = issuer;
    }

    pub fn set_subject(&self, subject: &str) {
        *self
            .token_subject
            .lock()
            .expect("subject lock should not be poisoned") = subject.to_string();
    }

    pub fn set_email(&self, email: &str) {
        *self
            .token_email
            .lock()
            .expect("email lock should not be poisoned") = Some(email.to_string());
    }

    pub fn clear_email(&self) {
        *self
            .token_email
            .lock()
            .expect("email lock should not be poisoned") = None;
    }

    pub fn set_email_verified(&self, verified: bool) {
        *self
            .token_email_verified
            .lock()
            .expect("email verified lock should not be poisoned") =
            Some(serde_json::json!(verified));
    }

    pub fn set_email_verified_claim(&self, value: serde_json::Value) {
        *self
            .token_email_verified
            .lock()
            .expect("email verified lock should not be poisoned") = Some(value);
    }

    pub fn clear_email_verified_claim(&self) {
        *self
            .token_email_verified
            .lock()
            .expect("email verified lock should not be poisoned") = None;
    }

    pub fn set_audience(&self, audience: &str) {
        *self
            .token_audience
            .lock()
            .expect("audience lock should not be poisoned") = audience.to_string();
    }

    pub fn set_nonce_override(&self, nonce: Option<String>) {
        *self
            .token_nonce_override
            .lock()
            .expect("nonce override lock should not be poisoned") = nonce;
    }

    fn public_jwk(&self) -> Jwk {
        Jwk {
            common: CommonParameters {
                public_key_use: Some(PublicKeyUse::Signature),
                key_algorithm: Some(KeyAlgorithm::RS256),
                key_id: Some(TEST_KID.to_string()),
                ..Default::default()
            },
            algorithm: AlgorithmParameters::RSA(RSAKeyParameters {
                n: base64_url(&self.key.modulus),
                e: base64_url(&self.key.exponent),
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
        let email_verified = self
            .token_email_verified
            .lock()
            .expect("email verified lock should not be poisoned")
            .clone();
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
            if let Some(email_verified) = email_verified {
                claims["email_verified"] = email_verified;
            }
        }
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(TEST_KID.to_string());
        jsonwebtoken::encode(
            &header,
            &claims,
            &EncodingKey::from_rsa_der(&self.key.private_der),
        )
        .expect("id_token should sign")
    }
}

impl StaticRsaKey {
    fn generate() -> Self {
        let private_der = generate_test_rsa_private_der();
        let key_pair = RsaKeyPair::from_der(&private_der).expect("RSA private key should parse");
        let public = RsaPublicKeyComponents::<Vec<u8>>::from(key_pair.public());
        Self {
            private_der,
            modulus: public.n,
            exponent: public.e,
        }
    }
}

fn generate_test_rsa_private_der() -> Vec<u8> {
    let mut rng = OsRng;
    let key = RsaPrivateKey::new(&mut rng, 2048).expect("RSA test key should generate");
    key.to_pkcs1_der()
        .expect("RSA test key should encode")
        .as_bytes()
        .to_vec()
}

fn base64_url(bytes: impl AsRef<[u8]>) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub async fn start_mock_external_auth_provider() -> (MockOidcProvider, actix_web::dev::ServerHandle)
{
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

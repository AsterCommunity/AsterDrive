//! 通用 OAuth2 外部认证 provider driver。

use async_trait::async_trait;
use base64::Engine as _;
use rand::RngExt;
use reqwest::header;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use url::Url;

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::external_auth::driver::{
    ExternalAuthAuthorizationStart, ExternalAuthCallback, ExternalAuthProfile,
    ExternalAuthProviderConfig, ExternalAuthProviderDescriptor, ExternalAuthProviderDriver,
    ExternalAuthProviderTestCheck, ExternalAuthProviderTestResult,
};
use crate::external_auth::url::{has_http_scheme, parse_url};
use crate::services::auth::local;
use crate::types::{ExternalAuthProtocol, ExternalAuthProviderKind};
use crate::utils::{OUTBOUND_HTTP_USER_AGENT, id};

const OAUTH2_DEFAULT_SCOPES: &str = "openid email profile";
const OAUTH2_NAMESPACE_MAX_LEN: usize = 512;
const OAUTH2_SUBJECT_MAX_LEN: usize = 255;
const OAUTH2_SNAPSHOT_MAX_LEN: usize = 255;
const TOKEN_ENDPOINT_TIMEOUT_SECS: u64 = 15;

#[derive(Clone, Copy)]
enum OAuth2TokenAuthMethod {
    ClientSecretPost,
    PublicClient,
}

#[derive(Clone, Copy)]
struct OAuth2TokenRequest<'a> {
    token_url: &'a str,
    provider: &'a ExternalAuthProviderConfig,
    code: &'a str,
    redirect_uri: &'a str,
    pkce_verifier: &'a str,
    auth_method: OAuth2TokenAuthMethod,
    client_secret: Option<&'a str>,
}

impl<'a> OAuth2TokenRequest<'a> {
    fn with_auth_method(self, auth_method: OAuth2TokenAuthMethod) -> Self {
        Self {
            auth_method,
            ..self
        }
    }
}

#[derive(Default)]
pub struct OAuth2ProviderDriver;

#[derive(Debug, Deserialize)]
struct OAuth2TokenResponse {
    access_token: String,
    #[serde(default)]
    token_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OAuth2ErrorResponse {
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

impl OAuth2ProviderDriver {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ExternalAuthProviderDriver for OAuth2ProviderDriver {
    fn kind(&self) -> ExternalAuthProviderKind {
        ExternalAuthProviderKind::GenericOAuth2
    }

    fn descriptor(&self) -> ExternalAuthProviderDescriptor {
        ExternalAuthProviderDescriptor {
            kind: ExternalAuthProviderKind::GenericOAuth2,
            protocol: ExternalAuthProtocol::OAuth2,
            display_name: "Generic OAuth2",
            description: "OAuth2 authorization-code sign-in using manually configured authorization, token and userinfo endpoints.",
            default_scopes: OAUTH2_DEFAULT_SCOPES,
            issuer_url_required: false,
            manual_endpoint_configuration_supported: true,
            authorization_url_required: true,
            token_url_required: true,
            userinfo_url_required: true,
            supports_discovery: false,
            supports_pkce: true,
            supports_email_verified_claim: true,
        }
    }

    async fn start_authorization(
        &self,
        provider: &ExternalAuthProviderConfig,
        redirect_uri: &str,
    ) -> Result<ExternalAuthAuthorizationStart> {
        let authorization_url = require_url(
            provider.authorization_url.as_deref(),
            "authorization_url",
            AsterError::config_error,
        )?;
        let mut authorization_url = validate_url(
            authorization_url,
            "authorization_url",
            AsterError::config_error,
        )?;
        let state = format!("oauth2_{}", id::new_short_token());
        let pkce_verifier = build_pkce_verifier();
        let pkce_challenge = build_pkce_challenge(&pkce_verifier);

        {
            let mut query = authorization_url.query_pairs_mut();
            query.append_pair("response_type", "code");
            query.append_pair("client_id", &provider.client_id);
            query.append_pair("redirect_uri", redirect_uri);
            query.append_pair("scope", provider.scopes.trim());
            query.append_pair("state", &state);
            query.append_pair("code_challenge", &pkce_challenge);
            query.append_pair("code_challenge_method", "S256");
        }

        Ok(ExternalAuthAuthorizationStart {
            authorization_url: authorization_url.to_string(),
            state,
            nonce: None,
            pkce_verifier: Some(pkce_verifier),
        })
    }

    async fn exchange_callback(
        &self,
        provider: &ExternalAuthProviderConfig,
        callback: ExternalAuthCallback,
    ) -> Result<ExternalAuthProfile> {
        let pkce_verifier = callback.pkce_verifier.ok_or_else(|| {
            AsterError::database_operation("stored OAuth2 PKCE verifier is missing")
        })?;
        let http_client = oauth2_http_client()?;
        let token = exchange_code_for_token(
            &http_client,
            provider,
            &callback.code,
            &callback.redirect_uri,
            &pkce_verifier,
        )
        .await?;
        let profile_json = fetch_userinfo(&http_client, provider, &token).await?;
        profile_from_userinfo(provider, &profile_json)
    }

    async fn test_provider(
        &self,
        provider: &ExternalAuthProviderConfig,
    ) -> Result<ExternalAuthProviderTestResult> {
        let authorization_url = require_url(
            provider.authorization_url.as_deref(),
            "authorization_url",
            AsterError::validation_error,
        )?;
        let token_url = require_url(
            provider.token_url.as_deref(),
            "token_url",
            AsterError::validation_error,
        )?;
        let userinfo_url = require_url(
            provider.userinfo_url.as_deref(),
            "userinfo_url",
            AsterError::validation_error,
        )?;
        validate_url(
            authorization_url,
            "authorization_url",
            AsterError::validation_error,
        )?;
        validate_url(token_url, "token_url", AsterError::validation_error)?;
        validate_url(userinfo_url, "userinfo_url", AsterError::validation_error)?;
        if provider.client_id.trim().is_empty() {
            return Err(AsterError::validation_error("client_id is required"));
        }

        Ok(ExternalAuthProviderTestResult {
            provider: self.descriptor().display_name.to_string(),
            issuer: provider.issuer_url.clone(),
            authorization_endpoint: Some(authorization_url.to_string()),
            token_endpoint: Some(token_url.to_string()),
            userinfo_endpoint: Some(userinfo_url.to_string()),
            jwks_key_count: None,
            checks: vec![
                ExternalAuthProviderTestCheck {
                    name: "manual_endpoints".to_string(),
                    success: true,
                    message: "OAuth2 authorization, token and userinfo endpoints are configured"
                        .to_string(),
                },
                ExternalAuthProviderTestCheck {
                    name: "authorization_code".to_string(),
                    success: true,
                    message:
                        "OAuth2 client credentials require a real authorization code to validate"
                            .to_string(),
                },
            ],
        })
    }
}

/// Builds the outbound HTTP client shared by Generic OAuth2 and specialized
/// OAuth2-backed providers such as GitHub.
///
/// GitHub rejects API calls without a User-Agent header, so keep the project
/// user agent on the shared client instead of setting it per request.
pub(super) fn oauth2_http_client() -> Result<reqwest::Client> {
    reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(TOKEN_ENDPOINT_TIMEOUT_SECS))
        .user_agent(OUTBOUND_HTTP_USER_AGENT)
        .build()
        .map_aster_err_ctx(
            "failed to build OAuth2 HTTP client",
            AsterError::internal_error,
        )
}

pub(super) fn require_url<'a>(
    value: Option<&'a str>,
    field: &str,
    error_fn: fn(String) -> AsterError,
) -> Result<&'a str> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| error_fn(format!("OAuth2 provider missing {field}")))
}

pub(super) fn validate_url(
    value: &str,
    field: &str,
    error_fn: fn(String) -> AsterError,
) -> Result<Url> {
    let parsed = parse_url(value, &format!("invalid OAuth2 {field}"), error_fn)?;
    if !has_http_scheme(&parsed) {
        return Err(error_fn(format!(
            "unsupported URL scheme for OAuth2 {field}, only http/https allowed"
        )));
    }
    Ok(parsed)
}

fn build_pkce_verifier() -> String {
    let mut bytes = [0_u8; 32];
    let mut rng = rand::rng();
    rng.fill(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn build_pkce_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

pub(super) async fn exchange_code_for_token(
    http_client: &reqwest::Client,
    provider: &ExternalAuthProviderConfig,
    code: &str,
    redirect_uri: &str,
    pkce_verifier: &str,
) -> Result<String> {
    let token_url = require_url(
        provider.token_url.as_deref(),
        "token_url",
        AsterError::config_error,
    )?;
    validate_url(token_url, "token_url", AsterError::config_error)?;

    let client_secret = provider
        .client_secret
        .as_deref()
        .map(str::trim)
        .filter(|secret| !secret.is_empty());
    let token_request = OAuth2TokenRequest {
        token_url,
        provider,
        code,
        redirect_uri,
        pkce_verifier,
        auth_method: OAuth2TokenAuthMethod::PublicClient,
        client_secret,
    };
    let response = if client_secret.is_some() {
        send_token_request(
            http_client,
            token_request.with_auth_method(OAuth2TokenAuthMethod::ClientSecretPost),
        )
        .await?
    } else {
        send_token_request(http_client, token_request).await?
    };
    if !response.status().is_success() {
        return Err(oauth2_endpoint_error(response, "OAuth2 token exchange").await);
    }
    let token_response = response
        .json::<OAuth2TokenResponse>()
        .await
        .map_aster_err_ctx(
            "OAuth2 token response is invalid",
            AsterError::auth_invalid_credentials,
        )?;
    if token_response.access_token.trim().is_empty() {
        return Err(AsterError::auth_invalid_credentials(
            "OAuth2 token response missing access_token",
        ));
    }
    if let Some(token_type) = token_response.token_type.as_deref()
        && !token_type.eq_ignore_ascii_case("bearer")
    {
        return Err(AsterError::auth_invalid_credentials(
            "OAuth2 token response returned unsupported token_type",
        ));
    }
    Ok(token_response.access_token)
}

async fn send_token_request(
    http_client: &reqwest::Client,
    token_request: OAuth2TokenRequest<'_>,
) -> Result<reqwest::Response> {
    let form = {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        serializer.append_pair("grant_type", "authorization_code");
        serializer.append_pair("code", token_request.code);
        serializer.append_pair("redirect_uri", token_request.redirect_uri);
        serializer.append_pair("code_verifier", token_request.pkce_verifier);
        match token_request.auth_method {
            OAuth2TokenAuthMethod::ClientSecretPost => {
                serializer.append_pair("client_id", &token_request.provider.client_id);
                if let Some(secret) = token_request.client_secret {
                    serializer.append_pair("client_secret", secret);
                }
            }
            OAuth2TokenAuthMethod::PublicClient => {
                serializer.append_pair("client_id", &token_request.provider.client_id);
            }
        }
        serializer.finish()
    };

    let request = http_client
        .post(token_request.token_url)
        .header(header::ACCEPT, "application/json")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(form);

    request.send().await.map_aster_err_ctx(
        "OAuth2 token exchange failed",
        AsterError::auth_invalid_credentials,
    )
}

pub(super) async fn oauth2_endpoint_error(
    response: reqwest::Response,
    context: &str,
) -> AsterError {
    let status = response.status();
    let www_authenticate = response
        .headers()
        .get(header::WWW_AUTHENTICATE)
        .and_then(|value| value.to_str().ok())
        .map(sanitize_error_fragment)
        .filter(|value| !value.is_empty());
    let provider_error = response.json::<OAuth2ErrorResponse>().await.ok();

    let mut details = Vec::new();
    if let Some(error) = provider_error
        .as_ref()
        .and_then(|body| body.error.as_deref())
        .map(sanitize_error_fragment)
        .filter(|error| !error.is_empty())
    {
        details.push(format!("error={error}"));
    }
    if let Some(description) = provider_error
        .as_ref()
        .and_then(|body| body.error_description.as_deref())
        .map(sanitize_error_fragment)
        .filter(|description| !description.is_empty())
    {
        details.push(format!("description={description}"));
    }
    if let Some(www_authenticate) = www_authenticate {
        details.push(format!("www-authenticate={www_authenticate}"));
    }

    if details.is_empty() {
        AsterError::auth_invalid_credentials(format!("{context} failed ({status})"))
    } else {
        AsterError::auth_invalid_credentials(format!(
            "{context} failed ({status}; {})",
            details.join("; ")
        ))
    }
}

fn sanitize_error_fragment(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !ch.is_control())
        .take(128)
        .collect::<String>()
        .trim()
        .to_string()
}

pub(super) async fn fetch_userinfo(
    http_client: &reqwest::Client,
    provider: &ExternalAuthProviderConfig,
    access_token: &str,
) -> Result<Value> {
    let userinfo_url = require_url(
        provider.userinfo_url.as_deref(),
        "userinfo_url",
        AsterError::config_error,
    )?;
    validate_url(userinfo_url, "userinfo_url", AsterError::config_error)?;
    let response = http_client
        .get(userinfo_url)
        .bearer_auth(access_token)
        .header(header::ACCEPT, "application/json")
        .send()
        .await
        .map_aster_err_ctx(
            "OAuth2 userinfo request failed",
            AsterError::auth_invalid_credentials,
        )?;
    if !response.status().is_success() {
        return Err(oauth2_endpoint_error(response, "OAuth2 userinfo request").await);
    }
    response.json::<Value>().await.map_aster_err_ctx(
        "OAuth2 userinfo response is invalid",
        AsterError::auth_invalid_credentials,
    )
}

pub(super) fn profile_from_userinfo(
    provider: &ExternalAuthProviderConfig,
    userinfo: &Value,
) -> Result<ExternalAuthProfile> {
    let subject_claim = provider.subject_claim.as_deref().unwrap_or("sub");
    let subject = extract_claim_string(userinfo, subject_claim)
        .or_else(|| {
            if subject_claim == "sub" {
                extract_claim_string(userinfo, "id")
            } else {
                None
            }
        })
        .ok_or_else(|| AsterError::auth_invalid_credentials("OAuth2 userinfo missing subject"))?;
    let subject = validate_required_claim(&subject, "OAuth2 subject", OAUTH2_SUBJECT_MAX_LEN)?;

    let email = extract_claim_string(userinfo, provider.email_claim.as_deref().unwrap_or("email"))
        .map(|email| email.trim().to_string())
        .filter(|email| !email.is_empty());
    if let Some(email) = email.as_deref() {
        local::validate_email(email)
            .map_err(|_| AsterError::auth_invalid_credentials("OAuth2 email claim is invalid"))?;
    }

    Ok(ExternalAuthProfile {
        identity_namespace: identity_namespace(provider)?,
        subject,
        email,
        email_verified: extract_claim_bool(
            userinfo,
            provider
                .email_verified_claim
                .as_deref()
                .unwrap_or("email_verified"),
        )
        .unwrap_or(false),
        display_name: normalize_optional_snapshot(extract_claim_string(
            userinfo,
            provider.display_name_claim.as_deref().unwrap_or("name"),
        )),
        preferred_username: normalize_optional_snapshot(extract_claim_string(
            userinfo,
            provider
                .username_claim
                .as_deref()
                .unwrap_or("preferred_username"),
        )),
    })
}

fn identity_namespace(provider: &ExternalAuthProviderConfig) -> Result<String> {
    if let Some(issuer) = provider
        .issuer_url
        .as_deref()
        .map(str::trim)
        .filter(|issuer| !issuer.is_empty())
    {
        return validate_required_claim(issuer, "OAuth2 issuer", OAUTH2_NAMESPACE_MAX_LEN);
    }
    let authorization_url = require_url(
        provider.authorization_url.as_deref(),
        "authorization_url",
        AsterError::config_error,
    )?;
    let parsed = validate_url(
        authorization_url,
        "authorization_url",
        AsterError::config_error,
    )?;
    let origin = parsed.origin().ascii_serialization();
    validate_required_claim(&origin, "OAuth2 origin", OAUTH2_NAMESPACE_MAX_LEN)
}

fn extract_claim_string(value: &Value, claim: &str) -> Option<String> {
    extract_claim_value(value, claim).and_then(|value| match value {
        Value::String(value) => Some(value.trim().to_string()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    })
}

fn extract_claim_bool(value: &Value, claim: &str) -> Option<bool> {
    extract_claim_value(value, claim).and_then(|value| match value {
        Value::Bool(value) => Some(*value),
        Value::String(value) if value.eq_ignore_ascii_case("true") => Some(true),
        Value::String(value) if value.eq_ignore_ascii_case("false") => Some(false),
        _ => None,
    })
}

fn extract_claim_value<'a>(value: &'a Value, claim: &str) -> Option<&'a Value> {
    let claim = claim.trim();
    if claim.is_empty() {
        return None;
    }
    if claim.starts_with('/') {
        return value.pointer(claim);
    }
    if let Some(found) = value.get(claim) {
        return Some(found);
    }
    claim
        .split('.')
        .try_fold(value, |current, segment| current.get(segment))
}

fn validate_required_claim(value: &str, field: &str, max_len: usize) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > max_len || value.chars().any(char::is_control) {
        return Err(AsterError::auth_invalid_credentials(format!(
            "{field} claim is invalid"
        )));
    }
    Ok(value.to_string())
}

fn truncate_to_utf8_boundary(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        return value.to_string();
    }
    let mut end = max_len;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

fn normalize_optional_snapshot(value: Option<String>) -> Option<String> {
    value
        .map(|value| {
            value
                .chars()
                .filter(|ch| !ch.is_control())
                .collect::<String>()
        })
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| truncate_to_utf8_boundary(&value, OAUTH2_SNAPSHOT_MAX_LEN))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> ExternalAuthProviderConfig {
        ExternalAuthProviderConfig {
            id: 1,
            key: "generic".to_string(),
            provider_kind: ExternalAuthProviderKind::GenericOAuth2,
            protocol: ExternalAuthProtocol::OAuth2,
            options: Default::default(),
            issuer_url: None,
            authorization_url: Some("https://id.example.com/oauth/authorize".to_string()),
            token_url: Some("https://id.example.com/oauth/token".to_string()),
            userinfo_url: Some("https://id.example.com/oauth/userinfo".to_string()),
            client_id: "client".to_string(),
            client_secret: None,
            scopes: OAUTH2_DEFAULT_SCOPES.to_string(),
            subject_claim: None,
            username_claim: None,
            display_name_claim: None,
            email_claim: None,
            email_verified_claim: None,
            groups_claim: None,
            avatar_url_claim: None,
        }
    }

    #[test]
    fn profile_supports_json_pointer_and_dotted_claims() {
        let mut provider = provider();
        provider.subject_claim = Some("/user/id".to_string());
        provider.username_claim = Some("user.login".to_string());
        provider.email_claim = Some("mail.primary".to_string());
        provider.email_verified_claim = Some("mail.verified".to_string());
        let userinfo = serde_json::json!({
            "user": { "id": 123, "login": "octo" },
            "mail": { "primary": "octo@example.com", "verified": "true" },
            "name": "Octo Cat"
        });

        let profile = profile_from_userinfo(&provider, &userinfo).expect("profile should parse");

        assert_eq!(profile.subject, "123");
        assert_eq!(profile.email.as_deref(), Some("octo@example.com"));
        assert!(profile.email_verified);
        assert_eq!(profile.preferred_username.as_deref(), Some("octo"));
    }

    #[test]
    fn profile_defaults_unverified_when_claim_is_missing() {
        let userinfo = serde_json::json!({
            "id": "github-1",
            "email": "user@example.com"
        });

        let profile = profile_from_userinfo(&provider(), &userinfo).expect("profile should parse");

        assert_eq!(profile.subject, "github-1");
        assert!(!profile.email_verified);
    }

    #[test]
    fn validate_url_rejects_non_http_schemes() {
        let err = validate_url("file:///tmp/token", "token_url", AsterError::config_error)
            .expect_err("non-http OAuth2 URL should be rejected");

        assert!(
            err.to_string()
                .contains("unsupported URL scheme for OAuth2 token_url")
        );
    }

    #[test]
    fn pkce_verifier_uses_valid_rfc7636_shape() {
        let verifier = build_pkce_verifier();

        assert!(verifier.len() >= 43);
        assert!(verifier.len() <= 128);
        assert!(
            verifier
                .chars()
                .all(|ch| { ch.is_ascii_alphanumeric() || matches!(ch, '-' | '.' | '_' | '~') })
        );
    }

    #[actix_web::test]
    async fn userinfo_error_includes_safe_provider_diagnostics() {
        use actix_web::{App, HttpResponse, HttpServer, web};

        async fn unauthorized_userinfo() -> HttpResponse {
            HttpResponse::Unauthorized()
                .append_header((
                    "WWW-Authenticate",
                    r#"Bearer error="insufficient_scope", error_description="missing openid""#,
                ))
                .json(serde_json::json!({
                    "error": "invalid_token",
                    "error_description": "missing openid scope"
                }))
        }

        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("listener should bind");
        let addr = listener
            .local_addr()
            .expect("listener address should exist");
        let server =
            HttpServer::new(|| App::new().route("/userinfo", web::get().to(unauthorized_userinfo)))
                .listen(listener)
                .expect("mock server should listen")
                .run();
        let handle = server.handle();
        tokio::spawn(server);

        let mut provider = provider();
        provider.userinfo_url = Some(format!("http://127.0.0.1:{}/userinfo", addr.port()));
        let http_client = oauth2_http_client().expect("HTTP client should build");

        let error = fetch_userinfo(&http_client, &provider, "opaque-access-token")
            .await
            .expect_err("userinfo should fail");
        let message = error.to_string();

        assert!(message.contains("OAuth2 userinfo request failed (401 Unauthorized"));
        assert!(message.contains("error=invalid_token"));
        assert!(message.contains("description=missing openid scope"));
        assert!(message.contains("www-authenticate=Bearer"));

        handle.stop(true).await;
    }
}

//! QQ 互联 OAuth2 外部认证 provider driver。

use async_trait::async_trait;
use reqwest::header;
use serde::Deserialize;

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::external_auth::driver::{
    ExternalAuthAuthorizationStart, ExternalAuthCallback, ExternalAuthProfile,
    ExternalAuthProviderConfig, ExternalAuthProviderDescriptor, ExternalAuthProviderDriver,
    ExternalAuthProviderTestCheck, ExternalAuthProviderTestResult,
};
use crate::types::{ExternalAuthProtocol, ExternalAuthProviderKind};

use super::oauth2::{
    OAuth2ProviderDriver, oauth2_endpoint_error, oauth2_http_client, validate_url,
};

const QQ_NAMESPACE_PREFIX: &str = "qq:";
const QQ_AUTHORIZATION_URL: &str = "https://graph.qq.com/oauth2.0/authorize";
const QQ_TOKEN_URL: &str = "https://graph.qq.com/oauth2.0/token";
const QQ_OPENID_URL: &str = "https://graph.qq.com/oauth2.0/me";
const QQ_USERINFO_URL: &str = "https://graph.qq.com/user/get_user_info";
const QQ_DEFAULT_SCOPES: &str = "get_user_info";
const QQ_OPENID_MAX_LEN: usize = 255;
const QQ_SNAPSHOT_MAX_LEN: usize = 255;

/// QQ Connect OAuth2 provider driver.
#[derive(Default)]
pub struct QqProviderDriver;

#[derive(Debug, Deserialize)]
struct QqTokenResponse {
    #[serde(default)]
    access_token: String,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
    #[serde(default)]
    msg: Option<String>,
}

#[derive(Debug, Deserialize)]
struct QqOpenIdResponse {
    #[serde(default)]
    client_id: String,
    #[serde(default)]
    openid: String,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct QqUserInfoResponse {
    ret: i64,
    #[serde(default)]
    msg: Option<String>,
    #[serde(default)]
    nickname: Option<String>,
}

impl QqProviderDriver {
    /// Creates a QQ Connect provider driver with fixed OAuth2 endpoints.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ExternalAuthProviderDriver for QqProviderDriver {
    fn kind(&self) -> ExternalAuthProviderKind {
        ExternalAuthProviderKind::Qq
    }

    fn descriptor(&self) -> ExternalAuthProviderDescriptor {
        ExternalAuthProviderDescriptor {
            kind: ExternalAuthProviderKind::Qq,
            protocol: ExternalAuthProtocol::OAuth2,
            display_name: "QQ",
            description: "QQ Connect OAuth2 sign-in using fixed token, openid and user info endpoints.",
            default_scopes: QQ_DEFAULT_SCOPES,
            issuer_url_required: false,
            manual_endpoint_configuration_supported: false,
            authorization_url_required: false,
            token_url_required: false,
            userinfo_url_required: false,
            supports_discovery: false,
            supports_pkce: true,
            supports_email_verified_claim: false,
        }
    }

    async fn start_authorization(
        &self,
        provider: &ExternalAuthProviderConfig,
        redirect_uri: &str,
    ) -> Result<ExternalAuthAuthorizationStart> {
        OAuth2ProviderDriver::new()
            .start_authorization(&qq_oauth2_config(provider), redirect_uri)
            .await
    }

    async fn exchange_callback(
        &self,
        provider: &ExternalAuthProviderConfig,
        callback: ExternalAuthCallback,
    ) -> Result<ExternalAuthProfile> {
        let provider = qq_oauth2_config(provider);
        let pkce_verifier = callback.pkce_verifier.ok_or_else(|| {
            AsterError::database_operation("stored QQ OAuth2 PKCE verifier is missing")
        })?;
        let http_client = oauth2_http_client()?;
        let access_token = exchange_qq_code_for_token(
            &http_client,
            &provider,
            &callback.code,
            &callback.redirect_uri,
            &pkce_verifier,
        )
        .await?;
        let openid = fetch_qq_openid(&http_client, &provider, &access_token).await?;
        let userinfo = fetch_qq_userinfo(&http_client, &provider, &access_token, &openid).await?;
        Ok(ExternalAuthProfile {
            identity_namespace: qq_identity_namespace(&provider)?,
            subject: validate_qq_openid(&openid)?,
            email: None,
            email_verified: false,
            display_name: normalize_optional_snapshot(userinfo.nickname),
            preferred_username: None,
        })
    }

    async fn test_provider(
        &self,
        provider: &ExternalAuthProviderConfig,
    ) -> Result<ExternalAuthProviderTestResult> {
        if provider.client_id.trim().is_empty() {
            return Err(AsterError::validation_error("client_id is required"));
        }
        let provider = qq_oauth2_config(provider);
        let authorization_url = provider
            .authorization_url
            .as_deref()
            .expect("QQ authorization URL should be set");
        let token_url = provider
            .token_url
            .as_deref()
            .expect("QQ token URL should be set");
        let userinfo_url = provider
            .userinfo_url
            .as_deref()
            .expect("QQ userinfo URL should be set");
        validate_url(
            authorization_url,
            "authorization_url",
            AsterError::validation_error,
        )?;
        validate_url(token_url, "token_url", AsterError::validation_error)?;
        validate_url(userinfo_url, "userinfo_url", AsterError::validation_error)?;
        validate_url(QQ_OPENID_URL, "openid_url", AsterError::validation_error)?;

        Ok(ExternalAuthProviderTestResult {
            provider: self.descriptor().display_name.to_string(),
            issuer: Some(qq_identity_namespace(&provider)?),
            authorization_endpoint: Some(authorization_url.to_string()),
            token_endpoint: Some(token_url.to_string()),
            userinfo_endpoint: Some(userinfo_url.to_string()),
            jwks_key_count: None,
            checks: vec![
                ExternalAuthProviderTestCheck {
                    name: "qq_endpoints".to_string(),
                    success: true,
                    message:
                        "QQ authorization, token, openid and userinfo endpoints are configured"
                            .to_string(),
                },
                ExternalAuthProviderTestCheck {
                    name: "qq_openid".to_string(),
                    success: true,
                    message: "QQ openid is fetched before get_user_info during sign-in".to_string(),
                },
            ],
        })
    }
}

fn qq_oauth2_config(provider: &ExternalAuthProviderConfig) -> ExternalAuthProviderConfig {
    let mut provider = provider.clone();
    provider.provider_kind = ExternalAuthProviderKind::Qq;
    provider.protocol = ExternalAuthProtocol::OAuth2;
    provider.issuer_url = Some(
        qq_identity_namespace(&provider)
            .unwrap_or_else(|_| format!("{QQ_NAMESPACE_PREFIX}{}", provider.client_id.trim())),
    );
    // Admin create/update rejects manual QQ endpoints through the descriptor.
    // Non-empty values are kept only for integration tests that inject a local
    // QQ-compatible mock server instead of calling the real QQ Connect API.
    provider.authorization_url = provider
        .authorization_url
        .filter(|value| !value.trim().is_empty())
        .or_else(|| Some(QQ_AUTHORIZATION_URL.to_string()));
    provider.token_url = provider
        .token_url
        .filter(|value| !value.trim().is_empty())
        .or_else(|| Some(QQ_TOKEN_URL.to_string()));
    provider.userinfo_url = provider
        .userinfo_url
        .filter(|value| !value.trim().is_empty())
        .or_else(|| Some(QQ_USERINFO_URL.to_string()));
    provider.scopes = if provider.scopes.trim().is_empty() {
        QQ_DEFAULT_SCOPES.to_string()
    } else {
        provider.scopes.trim().to_string()
    };
    provider.subject_claim = Some("openid".to_string());
    provider.username_claim = None;
    provider.display_name_claim = Some("nickname".to_string());
    provider.email_claim = None;
    provider.email_verified_claim = None;
    provider.avatar_url_claim = Some("figureurl_qq_2".to_string());
    provider
}

async fn exchange_qq_code_for_token(
    http_client: &reqwest::Client,
    provider: &ExternalAuthProviderConfig,
    code: &str,
    redirect_uri: &str,
    pkce_verifier: &str,
) -> Result<String> {
    let token_url = provider
        .token_url
        .as_deref()
        .ok_or_else(|| AsterError::config_error("QQ token URL is missing"))?;
    let mut token_url = validate_url(token_url, "token_url", AsterError::config_error)?;
    {
        let mut query = token_url.query_pairs_mut();
        query.append_pair("grant_type", "authorization_code");
        query.append_pair("client_id", &provider.client_id);
        if let Some(client_secret) = provider
            .client_secret
            .as_deref()
            .map(str::trim)
            .filter(|secret| !secret.is_empty())
        {
            query.append_pair("client_secret", client_secret);
        }
        query.append_pair("code", code);
        query.append_pair("redirect_uri", redirect_uri);
        // QQ Connect docs do not list PKCE, but authorization uses the shared
        // OAuth2 driver which sends a code_challenge, so keep the token request paired.
        query.append_pair("code_verifier", pkce_verifier);
        query.append_pair("fmt", "json");
    }
    let response = http_client
        .get(token_url)
        .header(header::ACCEPT, "application/json")
        .send()
        .await
        .map_aster_err_ctx(
            "QQ token exchange failed",
            AsterError::auth_invalid_credentials,
        )?;
    if !response.status().is_success() {
        return Err(oauth2_endpoint_error(response, "QQ token exchange").await);
    }
    let token_response = response.json::<QqTokenResponse>().await.map_aster_err_ctx(
        "QQ token response is invalid",
        AsterError::auth_invalid_credentials,
    )?;
    if token_response.access_token.trim().is_empty() {
        return Err(AsterError::auth_invalid_credentials(format!(
            "QQ token response missing access_token{}",
            qq_error_suffix(
                token_response.error.as_deref(),
                token_response
                    .error_description
                    .as_deref()
                    .or(token_response.msg.as_deref())
            )
        )));
    }
    Ok(token_response.access_token)
}

async fn fetch_qq_openid(
    http_client: &reqwest::Client,
    provider: &ExternalAuthProviderConfig,
    access_token: &str,
) -> Result<String> {
    let mut openid_url = qq_openid_url(provider)?;
    {
        let mut query = openid_url.query_pairs_mut();
        query.append_pair("access_token", access_token);
        query.append_pair("fmt", "json");
    }
    let response = http_client
        .get(openid_url)
        .header(header::ACCEPT, "application/json")
        .send()
        .await
        .map_aster_err_ctx(
            "QQ openid request failed",
            AsterError::auth_invalid_credentials,
        )?;
    if !response.status().is_success() {
        return Err(oauth2_endpoint_error(response, "QQ openid request").await);
    }
    let openid_response = response
        .json::<QqOpenIdResponse>()
        .await
        .map_aster_err_ctx(
            "QQ openid response is invalid",
            AsterError::auth_invalid_credentials,
        )?;
    if !openid_response.client_id.is_empty() && openid_response.client_id != provider.client_id {
        return Err(AsterError::auth_invalid_credentials(
            "QQ openid response client_id does not match provider",
        ));
    }
    if openid_response.openid.trim().is_empty() {
        return Err(AsterError::auth_invalid_credentials(format!(
            "QQ openid response missing openid{}",
            qq_error_suffix(
                openid_response.error.as_deref(),
                openid_response.error_description.as_deref()
            )
        )));
    }
    Ok(openid_response.openid)
}

async fn fetch_qq_userinfo(
    http_client: &reqwest::Client,
    provider: &ExternalAuthProviderConfig,
    access_token: &str,
    openid: &str,
) -> Result<QqUserInfoResponse> {
    let userinfo_url = provider
        .userinfo_url
        .as_deref()
        .ok_or_else(|| AsterError::config_error("QQ userinfo URL is missing"))?;
    let mut userinfo_url = validate_url(userinfo_url, "userinfo_url", AsterError::config_error)?;
    {
        let mut query = userinfo_url.query_pairs_mut();
        query.append_pair("access_token", access_token);
        query.append_pair("oauth_consumer_key", &provider.client_id);
        query.append_pair("openid", openid);
    }
    let response = http_client
        .get(userinfo_url)
        .header(header::ACCEPT, "application/json")
        .send()
        .await
        .map_aster_err_ctx(
            "QQ userinfo request failed",
            AsterError::auth_invalid_credentials,
        )?;
    if !response.status().is_success() {
        return Err(oauth2_endpoint_error(response, "QQ userinfo request").await);
    }
    let userinfo = response
        .json::<QqUserInfoResponse>()
        .await
        .map_aster_err_ctx(
            "QQ userinfo response is invalid",
            AsterError::auth_invalid_credentials,
        )?;
    if userinfo.ret != 0 {
        return Err(AsterError::auth_invalid_credentials(format!(
            "QQ userinfo request failed{}",
            qq_error_suffix(Some(&userinfo.ret.to_string()), userinfo.msg.as_deref())
        )));
    }
    Ok(userinfo)
}

fn qq_identity_namespace(provider: &ExternalAuthProviderConfig) -> Result<String> {
    let client_id = provider.client_id.trim();
    if client_id.is_empty() || client_id.chars().any(char::is_control) {
        return Err(AsterError::validation_error("QQ client_id is invalid"));
    }
    Ok(format!("{QQ_NAMESPACE_PREFIX}{client_id}"))
}

fn qq_openid_url(provider: &ExternalAuthProviderConfig) -> Result<reqwest::Url> {
    let token_url = provider
        .token_url
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(QQ_TOKEN_URL);
    if token_url == QQ_TOKEN_URL {
        return validate_url(QQ_OPENID_URL, "openid_url", AsterError::config_error);
    }
    let parsed = validate_url(token_url, "openid_url", AsterError::config_error)?;
    qq_openid_url_from_token_url(parsed)
}

fn qq_openid_url_from_token_url(mut token_url: reqwest::Url) -> Result<reqwest::Url> {
    {
        let mut paths = token_url
            .path_segments_mut()
            .map_err(|_| AsterError::config_error("invalid QQ token URL"))?;
        paths.pop_if_empty();
        paths.pop();
        paths.push("me");
    }
    token_url.set_query(None);
    token_url.set_fragment(None);
    Ok(token_url)
}

fn validate_qq_openid(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > QQ_OPENID_MAX_LEN || value.chars().any(char::is_control) {
        return Err(AsterError::auth_invalid_credentials(
            "QQ openid claim is invalid",
        ));
    }
    Ok(value.to_string())
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
        .map(|value| truncate_to_utf8_boundary(&value, QQ_SNAPSHOT_MAX_LEN))
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

fn qq_error_suffix(error: Option<&str>, description: Option<&str>) -> String {
    let mut parts = Vec::new();
    if let Some(error) = error
        .map(sanitize_qq_error)
        .filter(|value| !value.is_empty())
    {
        parts.push(format!("error={error}"));
    }
    if let Some(description) = description
        .map(sanitize_qq_error)
        .filter(|value| !value.is_empty())
    {
        parts.push(format!("description={description}"));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" ({})", parts.join("; "))
    }
}

fn sanitize_qq_error(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !ch.is_control())
        .take(128)
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> ExternalAuthProviderConfig {
        ExternalAuthProviderConfig {
            id: 1,
            key: "qq".to_string(),
            provider_kind: ExternalAuthProviderKind::Qq,
            protocol: ExternalAuthProtocol::OAuth2,
            issuer_url: Some("https://ignored.example.com".to_string()),
            authorization_url: Some("https://ignored.example.com/auth".to_string()),
            token_url: Some("https://ignored.example.com/token".to_string()),
            userinfo_url: Some("https://ignored.example.com/userinfo".to_string()),
            client_id: "100000001".to_string(),
            client_secret: Some("secret".to_string()),
            scopes: String::new(),
            subject_claim: Some("sub".to_string()),
            username_claim: Some("login".to_string()),
            display_name_claim: Some("name".to_string()),
            email_claim: Some("email".to_string()),
            email_verified_claim: Some("email_verified".to_string()),
            groups_claim: None,
            avatar_url_claim: None,
        }
    }

    #[test]
    fn qq_config_uses_fixed_endpoints_and_claim_semantics() {
        let mut provider = provider();
        provider.authorization_url = None;
        provider.token_url = None;
        provider.userinfo_url = None;
        let config = qq_oauth2_config(&provider);

        assert_eq!(config.provider_kind, ExternalAuthProviderKind::Qq);
        assert_eq!(config.protocol, ExternalAuthProtocol::OAuth2);
        assert_eq!(config.issuer_url.as_deref(), Some("qq:100000001"));
        assert_eq!(
            config.authorization_url.as_deref(),
            Some(QQ_AUTHORIZATION_URL)
        );
        assert_eq!(config.token_url.as_deref(), Some(QQ_TOKEN_URL));
        assert_eq!(config.userinfo_url.as_deref(), Some(QQ_USERINFO_URL));
        assert_eq!(config.scopes, QQ_DEFAULT_SCOPES);
        assert_eq!(config.subject_claim.as_deref(), Some("openid"));
        assert_eq!(config.username_claim, None);
        assert_eq!(config.display_name_claim.as_deref(), Some("nickname"));
        assert_eq!(config.email_claim, None);
        assert_eq!(config.email_verified_claim, None);
        assert_eq!(config.avatar_url_claim.as_deref(), Some("figureurl_qq_2"));
    }

    #[test]
    fn qq_identity_namespace_is_client_scoped() {
        let mut first = provider();
        first.client_id = "100000001".to_string();
        let mut second = provider();
        second.client_id = "200000002".to_string();

        assert_eq!(qq_identity_namespace(&first).unwrap(), "qq:100000001");
        assert_eq!(qq_identity_namespace(&second).unwrap(), "qq:200000002");
    }

    #[test]
    fn qq_openid_url_preserves_mock_path_prefix() {
        let openid_url = qq_openid_url_from_token_url(
            reqwest::Url::parse("http://127.0.0.1:3000/prefix/qq/token?fmt=json#fragment").unwrap(),
        )
        .unwrap();

        assert_eq!(openid_url.as_str(), "http://127.0.0.1:3000/prefix/qq/me");
    }

    #[test]
    fn qq_openid_validation_rejects_empty_control_and_long_values() {
        assert_eq!(validate_qq_openid(" openid-1 ").unwrap(), "openid-1");
        assert!(validate_qq_openid("").is_err());
        assert!(validate_qq_openid("open\nid").is_err());
        assert!(validate_qq_openid(&"a".repeat(QQ_OPENID_MAX_LEN + 1)).is_err());
    }
}

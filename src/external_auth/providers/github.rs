//! GitHub 外部认证 provider driver。

use async_trait::async_trait;
use reqwest::header;
use serde::Deserialize;

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::external_auth::driver::{
    ExternalAuthAuthorizationStart, ExternalAuthCallback, ExternalAuthProfile,
    ExternalAuthProviderConfig, ExternalAuthProviderDescriptor, ExternalAuthProviderDriver,
    ExternalAuthProviderTestCheck, ExternalAuthProviderTestResult,
};
use crate::services::auth_service;
use crate::types::{ExternalAuthProtocol, ExternalAuthProviderKind};

use super::oauth2::{
    OAuth2ProviderDriver, exchange_code_for_token, fetch_userinfo, oauth2_endpoint_error,
    oauth2_http_client, profile_from_userinfo, validate_url,
};

const GITHUB_AUTHORIZATION_URL: &str = "https://github.com/login/oauth/authorize";
const GITHUB_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const GITHUB_USERINFO_URL: &str = "https://api.github.com/user";
const GITHUB_DEFAULT_SCOPES: &str = "read:user user:email";
const GITHUB_EMAIL_IGNORED_CLAIM: &str = "__asterdrive_github_email_ignored__";
const GITHUB_EMAIL_VERIFIED_IGNORED_CLAIM: &str = "__asterdrive_github_email_verified_ignored__";

#[derive(Default)]
pub struct GitHubProviderDriver;

#[derive(Debug, Deserialize)]
struct GitHubEmail {
    email: String,
    #[serde(default)]
    primary: bool,
    #[serde(default)]
    verified: bool,
}

impl GitHubProviderDriver {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ExternalAuthProviderDriver for GitHubProviderDriver {
    fn kind(&self) -> ExternalAuthProviderKind {
        ExternalAuthProviderKind::GitHub
    }

    fn descriptor(&self) -> ExternalAuthProviderDescriptor {
        ExternalAuthProviderDescriptor {
            kind: ExternalAuthProviderKind::GitHub,
            protocol: ExternalAuthProtocol::OAuth2,
            display_name: "GitHub",
            description: "GitHub OAuth App sign-in with verified primary email fetched from the user emails API.",
            default_scopes: GITHUB_DEFAULT_SCOPES,
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
            .start_authorization(&github_oauth2_config(provider), redirect_uri)
            .await
    }

    async fn exchange_callback(
        &self,
        provider: &ExternalAuthProviderConfig,
        callback: ExternalAuthCallback,
    ) -> Result<ExternalAuthProfile> {
        let pkce_verifier = callback.pkce_verifier.ok_or_else(|| {
            AsterError::database_operation("stored GitHub OAuth2 PKCE verifier is missing")
        })?;
        let provider = github_oauth2_config(provider);
        let http_client = oauth2_http_client()?;
        let token = exchange_code_for_token(
            &http_client,
            &provider,
            &callback.code,
            &callback.redirect_uri,
            &pkce_verifier,
        )
        .await?;
        let userinfo = fetch_userinfo(&http_client, &provider, &token).await?;
        let mut profile = profile_from_userinfo(&provider, &userinfo)?;
        profile.email = fetch_verified_primary_email(&http_client, &provider, &token).await?;
        profile.email_verified = profile.email.is_some();
        Ok(profile)
    }

    async fn test_provider(
        &self,
        provider: &ExternalAuthProviderConfig,
    ) -> Result<ExternalAuthProviderTestResult> {
        if provider.client_id.trim().is_empty() {
            return Err(AsterError::validation_error("client_id is required"));
        }
        let provider = github_oauth2_config(provider);
        let authorization_url = provider
            .authorization_url
            .as_deref()
            .expect("GitHub authorization URL should be set");
        let token_url = provider
            .token_url
            .as_deref()
            .expect("GitHub token URL should be set");
        let userinfo_url = provider
            .userinfo_url
            .as_deref()
            .expect("GitHub userinfo URL should be set");
        validate_url(
            authorization_url,
            "authorization_url",
            AsterError::validation_error,
        )?;
        validate_url(token_url, "token_url", AsterError::validation_error)?;
        validate_url(userinfo_url, "userinfo_url", AsterError::validation_error)?;

        Ok(ExternalAuthProviderTestResult {
            provider: self.descriptor().display_name.to_string(),
            issuer: provider.issuer_url.clone(),
            authorization_endpoint: Some(authorization_url.to_string()),
            token_endpoint: Some(token_url.to_string()),
            userinfo_endpoint: Some(userinfo_url.to_string()),
            jwks_key_count: None,
            checks: vec![
                ExternalAuthProviderTestCheck {
                    name: "github_endpoints".to_string(),
                    success: true,
                    message:
                        "GitHub authorization, token, user and user emails endpoints are configured"
                            .to_string(),
                },
                ExternalAuthProviderTestCheck {
                    name: "verified_primary_email".to_string(),
                    success: true,
                    message:
                        "GitHub verified primary email is read from /user/emails during sign-in"
                            .to_string(),
                },
            ],
        })
    }
}

fn github_oauth2_config(provider: &ExternalAuthProviderConfig) -> ExternalAuthProviderConfig {
    let mut provider = provider.clone();
    provider.provider_kind = ExternalAuthProviderKind::GitHub;
    provider.protocol = ExternalAuthProtocol::OAuth2;
    provider.authorization_url = provider
        .authorization_url
        .filter(|value| !value.trim().is_empty())
        .or_else(|| Some(GITHUB_AUTHORIZATION_URL.to_string()));
    provider.token_url = provider
        .token_url
        .filter(|value| !value.trim().is_empty())
        .or_else(|| Some(GITHUB_TOKEN_URL.to_string()));
    provider.userinfo_url = provider
        .userinfo_url
        .filter(|value| !value.trim().is_empty())
        .or_else(|| Some(GITHUB_USERINFO_URL.to_string()));
    provider.scopes = if provider.scopes.trim().is_empty() {
        GITHUB_DEFAULT_SCOPES.to_string()
    } else {
        provider.scopes.trim().to_string()
    };
    provider.subject_claim = provider.subject_claim.or_else(|| Some("id".to_string()));
    provider.username_claim = provider
        .username_claim
        .or_else(|| Some("login".to_string()));
    provider.display_name_claim = provider
        .display_name_claim
        .or_else(|| Some("name".to_string()));
    provider.email_claim = Some(GITHUB_EMAIL_IGNORED_CLAIM.to_string());
    provider.email_verified_claim = Some(GITHUB_EMAIL_VERIFIED_IGNORED_CLAIM.to_string());
    provider
}

async fn fetch_verified_primary_email(
    http_client: &reqwest::Client,
    provider: &ExternalAuthProviderConfig,
    access_token: &str,
) -> Result<Option<String>> {
    let emails_url = github_emails_url(provider)?;
    let response = http_client
        .get(&emails_url)
        .bearer_auth(access_token)
        .header(header::ACCEPT, "application/json")
        .send()
        .await
        .map_aster_err_ctx(
            "GitHub user emails request failed",
            AsterError::auth_invalid_credentials,
        )?;
    if !response.status().is_success() {
        return Err(oauth2_endpoint_error(response, "GitHub user emails request").await);
    }

    let emails = response
        .json::<Vec<GitHubEmail>>()
        .await
        .map_aster_err_ctx(
            "GitHub user emails response is invalid",
            AsterError::auth_invalid_credentials,
        )?;
    select_verified_primary_email(emails)
}

fn github_emails_url(provider: &ExternalAuthProviderConfig) -> Result<String> {
    let userinfo_url = provider
        .userinfo_url
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(GITHUB_USERINFO_URL);
    let mut parsed = validate_url(userinfo_url, "userinfo_url", AsterError::config_error)?;
    let path = parsed.path().trim_end_matches('/');
    parsed.set_path(&format!("{path}/emails"));
    parsed.set_query(None);
    parsed.set_fragment(None);
    Ok(parsed.to_string())
}

fn select_verified_primary_email(emails: Vec<GitHubEmail>) -> Result<Option<String>> {
    let Some(email) = emails
        .into_iter()
        .find(|email| email.primary && email.verified)
        .map(|email| email.email.trim().to_string())
        .filter(|email| !email.is_empty())
    else {
        return Ok(None);
    };
    auth_service::validate_email(&email)
        .map_err(|_| AsterError::auth_invalid_credentials("GitHub primary email is invalid"))?;
    Ok(Some(email))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> ExternalAuthProviderConfig {
        ExternalAuthProviderConfig {
            id: 1,
            key: "github".to_string(),
            provider_kind: ExternalAuthProviderKind::GitHub,
            protocol: ExternalAuthProtocol::OAuth2,
            issuer_url: None,
            authorization_url: None,
            token_url: None,
            userinfo_url: None,
            client_id: "client".to_string(),
            client_secret: Some("secret".to_string()),
            scopes: String::new(),
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
    fn github_config_uses_fixed_defaults_and_claims() {
        let config = github_oauth2_config(&provider());

        assert_eq!(
            config.authorization_url.as_deref(),
            Some(GITHUB_AUTHORIZATION_URL)
        );
        assert_eq!(config.token_url.as_deref(), Some(GITHUB_TOKEN_URL));
        assert_eq!(config.userinfo_url.as_deref(), Some(GITHUB_USERINFO_URL));
        assert_eq!(config.scopes, GITHUB_DEFAULT_SCOPES);
        assert_eq!(config.subject_claim.as_deref(), Some("id"));
        assert_eq!(config.username_claim.as_deref(), Some("login"));
        assert_eq!(config.display_name_claim.as_deref(), Some("name"));
    }

    #[test]
    fn github_emails_url_is_derived_from_userinfo_url() {
        let mut config = github_oauth2_config(&provider());
        config.userinfo_url = Some("https://api.github.test/user?ignored=true".to_string());

        let emails_url = github_emails_url(&config).expect("emails URL should build");

        assert_eq!(emails_url, "https://api.github.test/user/emails");
    }

    #[test]
    fn verified_primary_email_selection_requires_primary_and_verified() {
        let selected = select_verified_primary_email(vec![
            GitHubEmail {
                email: "secondary@example.com".to_string(),
                primary: false,
                verified: true,
            },
            GitHubEmail {
                email: "primary-unverified@example.com".to_string(),
                primary: true,
                verified: false,
            },
            GitHubEmail {
                email: " github@example.com ".to_string(),
                primary: true,
                verified: true,
            },
        ])
        .expect("email selection should succeed");

        assert_eq!(selected.as_deref(), Some("github@example.com"));
    }

    #[test]
    fn verified_primary_email_selection_returns_none_when_missing() {
        let selected = select_verified_primary_email(vec![
            GitHubEmail {
                email: "secondary@example.com".to_string(),
                primary: false,
                verified: true,
            },
            GitHubEmail {
                email: "primary-unverified@example.com".to_string(),
                primary: true,
                verified: false,
            },
        ])
        .expect("missing verified primary email should not error");

        assert_eq!(selected, None);
    }

    #[test]
    fn verified_primary_email_selection_rejects_invalid_email() {
        let error = select_verified_primary_email(vec![GitHubEmail {
            email: "not-an-email".to_string(),
            primary: true,
            verified: true,
        }])
        .expect_err("invalid verified primary email should fail");

        assert!(
            error
                .to_string()
                .contains("GitHub primary email is invalid")
        );
    }
}

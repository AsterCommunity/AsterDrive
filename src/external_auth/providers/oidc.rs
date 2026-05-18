//! OIDC 外部认证 provider driver。

use std::borrow::Cow;

use async_trait::async_trait;
use openidconnect::core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata};
use openidconnect::reqwest;
use openidconnect::{
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, IssuerUrl, Nonce, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, Scope,
};
use openidconnect::{EndpointMaybeSet, EndpointNotSet, EndpointSet};

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::services::auth_service;
use crate::types::{ExternalAuthProtocol, ExternalAuthProviderKind};
use crate::utils::OUTBOUND_HTTP_USER_AGENT;

use super::super::driver::{
    ExternalAuthAuthorizationStart, ExternalAuthCallback, ExternalAuthProfile,
    ExternalAuthProviderConfig, ExternalAuthProviderDescriptor, ExternalAuthProviderDriver,
    ExternalAuthProviderTestCheck, ExternalAuthProviderTestResult,
};

const OIDC_ISSUER_MAX_LEN: usize = 512;
const OIDC_SUBJECT_MAX_LEN: usize = 255;
const OIDC_SNAPSHOT_MAX_LEN: usize = 255;

type OidcHttpClient = reqwest::Client;
type OidcClient = CoreClient<
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

#[derive(Default)]
pub struct OidcProviderDriver;

impl OidcProviderDriver {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ExternalAuthProviderDriver for OidcProviderDriver {
    fn kind(&self) -> ExternalAuthProviderKind {
        ExternalAuthProviderKind::Oidc
    }

    fn descriptor(&self) -> ExternalAuthProviderDescriptor {
        ExternalAuthProviderDescriptor {
            kind: ExternalAuthProviderKind::Oidc,
            protocol: ExternalAuthProtocol::Oidc,
            display_name: "OpenID Connect",
            description: "OpenID Connect authorization-code sign-in with discovery, PKCE, nonce and ID token validation.",
            default_scopes: "openid email profile",
            issuer_url_required: true,
            manual_endpoint_configuration_supported: false,
            authorization_url_required: false,
            token_url_required: false,
            userinfo_url_required: false,
            supports_discovery: true,
            supports_pkce: true,
            supports_email_verified_claim: true,
        }
    }

    async fn start_authorization(
        &self,
        provider: &ExternalAuthProviderConfig,
        redirect_uri: &str,
    ) -> Result<ExternalAuthAuthorizationStart> {
        let client = build_client(provider, redirect_uri).await?;
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        let mut request = client
            .authorize_url(
                CoreAuthenticationFlow::AuthorizationCode,
                CsrfToken::new_random,
                Nonce::new_random,
            )
            .set_pkce_challenge(pkce_challenge);

        for scope in provider.scopes.split_whitespace() {
            if scope != "openid" {
                request = request.add_scope(Scope::new(scope.to_string()));
            }
        }

        let (authorization_url, csrf_state, nonce) = request.url();
        Ok(ExternalAuthAuthorizationStart {
            authorization_url: authorization_url.to_string(),
            state: csrf_state.secret().clone(),
            nonce: Some(nonce.secret().clone()),
            pkce_verifier: Some(pkce_verifier.secret().clone()),
        })
    }

    async fn exchange_callback(
        &self,
        provider: &ExternalAuthProviderConfig,
        callback: ExternalAuthCallback,
    ) -> Result<ExternalAuthProfile> {
        let nonce = callback
            .nonce
            .ok_or_else(|| AsterError::database_operation("stored OIDC nonce is missing"))?;
        let pkce_verifier = callback.pkce_verifier.ok_or_else(|| {
            AsterError::database_operation("stored OIDC PKCE verifier is missing")
        })?;
        let client = build_client(provider, &callback.redirect_uri).await?;
        let http_client = oidc_http_client()?;
        let token_request = client
            .exchange_code(AuthorizationCode::new(callback.code))
            .map_aster_err_ctx(
                "OIDC provider metadata missing token endpoint",
                AsterError::config_error,
            )?;
        let token_response = token_request
            .set_pkce_verifier(PkceCodeVerifier::new(pkce_verifier))
            .set_redirect_uri(Cow::Owned(
                RedirectUrl::new(callback.redirect_uri.clone()).map_aster_err_ctx(
                    "invalid stored OIDC redirect URI",
                    AsterError::database_operation,
                )?,
            ))
            .request_async(&http_client)
            .await
            .map_aster_err_ctx(
                "OIDC token exchange failed",
                AsterError::auth_invalid_credentials,
            )?;

        let id_token = token_response.extra_fields().id_token().ok_or_else(|| {
            AsterError::auth_invalid_credentials("OIDC token response missing id_token")
        })?;
        let verifier = client.id_token_verifier();
        let nonce = Nonce::new(nonce);
        let claims = id_token.claims(&verifier, &nonce).map_aster_err_ctx(
            "OIDC ID token verification failed",
            AsterError::auth_invalid_credentials,
        )?;
        let profile = profile_from_id_token(claims)?;
        if profile.identity_namespace != provider.require_issuer_url()? {
            return Err(AsterError::auth_invalid_credentials(
                "OIDC issuer does not match configured provider",
            ));
        }
        Ok(profile)
    }

    async fn test_provider(
        &self,
        provider: &ExternalAuthProviderConfig,
    ) -> Result<ExternalAuthProviderTestResult> {
        let metadata = discover_provider(provider).await?;
        let token_endpoint = metadata.token_endpoint().ok_or_else(|| {
            AsterError::validation_error("OIDC discovery metadata missing token_endpoint")
        })?;
        let authorization_endpoint = metadata.authorization_endpoint().as_str().to_string();
        let token_endpoint = token_endpoint.as_str().to_string();
        let jwks_key_count = metadata.jwks().keys().len();
        Ok(ExternalAuthProviderTestResult {
            provider: self.descriptor().display_name.to_string(),
            issuer: Some(metadata.issuer().as_str().to_string()),
            authorization_endpoint: Some(authorization_endpoint),
            token_endpoint: Some(token_endpoint),
            userinfo_endpoint: metadata
                .userinfo_endpoint()
                .map(|url| url.as_str().to_string()),
            jwks_key_count: Some(jwks_key_count),
            checks: vec![
                ExternalAuthProviderTestCheck {
                    name: "discovery".to_string(),
                    success: true,
                    message: "OIDC discovery metadata was loaded".to_string(),
                },
                ExternalAuthProviderTestCheck {
                    name: "jwks".to_string(),
                    success: true,
                    message: format!("JWKS contains {jwks_key_count} key(s)"),
                },
            ],
        })
    }
}

fn oidc_http_client() -> Result<OidcHttpClient> {
    reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(15))
        .user_agent(OUTBOUND_HTTP_USER_AGENT)
        .build()
        .map_aster_err_ctx(
            "failed to build OIDC HTTP client",
            AsterError::internal_error,
        )
}

async fn build_client(
    provider: &ExternalAuthProviderConfig,
    redirect_uri: &str,
) -> Result<OidcClient> {
    let http_client = oidc_http_client()?;
    let issuer = IssuerUrl::new(provider.require_issuer_url()?.to_string())
        .map_aster_err_ctx("invalid OIDC issuer URL", AsterError::validation_error)?;
    let metadata = CoreProviderMetadata::discover_async(issuer, &http_client)
        .await
        .map_aster_err_ctx("OIDC discovery failed", AsterError::validation_error)?;
    let client_secret = provider
        .client_secret
        .clone()
        .filter(|secret| !secret.is_empty())
        .map(ClientSecret::new);
    let redirect_uri = RedirectUrl::new(redirect_uri.to_string())
        .map_aster_err_ctx("invalid OIDC redirect URI", AsterError::validation_error)?;
    Ok(CoreClient::from_provider_metadata(
        metadata,
        ClientId::new(provider.client_id.clone()),
        client_secret,
    )
    .set_redirect_uri(redirect_uri))
}

async fn discover_provider(provider: &ExternalAuthProviderConfig) -> Result<CoreProviderMetadata> {
    let http_client = oidc_http_client()?;
    let issuer = IssuerUrl::new(provider.require_issuer_url()?.to_string())
        .map_aster_err_ctx("invalid OIDC issuer URL", AsterError::validation_error)?;
    CoreProviderMetadata::discover_async(issuer, &http_client)
        .await
        .map_aster_err_ctx("OIDC discovery failed", AsterError::validation_error)
}

fn validate_oidc_required_claim(value: &str, field: &str, max_len: usize) -> Result<String> {
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
        .map(|value| truncate_to_utf8_boundary(&value, OIDC_SNAPSHOT_MAX_LEN))
}

fn profile_from_id_token(
    claims: &openidconnect::core::CoreIdTokenClaims,
) -> Result<ExternalAuthProfile> {
    let display_name = normalize_optional_snapshot(
        claims
            .name()
            .and_then(|claim| claim.get(None))
            .map(|name| name.as_str().to_string()),
    );
    let preferred_username = normalize_optional_snapshot(
        claims
            .preferred_username()
            .map(|username| username.as_str().to_string()),
    );
    let email = claims
        .email()
        .map(|email| email.as_str().trim().to_string())
        .filter(|email| !email.is_empty());
    if let Some(email) = email.as_deref() {
        auth_service::validate_email(email)
            .map_err(|_| AsterError::auth_invalid_credentials("OIDC email claim is invalid"))?;
    }

    Ok(ExternalAuthProfile {
        identity_namespace: validate_oidc_required_claim(
            claims.issuer().as_str(),
            "OIDC issuer",
            OIDC_ISSUER_MAX_LEN,
        )?,
        subject: validate_oidc_required_claim(
            claims.subject().as_str(),
            "OIDC subject",
            OIDC_SUBJECT_MAX_LEN,
        )?,
        email,
        email_verified: claims.email_verified().unwrap_or(false),
        display_name,
        preferred_username,
    })
}

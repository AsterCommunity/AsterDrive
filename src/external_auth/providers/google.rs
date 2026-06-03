//! Google 外部认证 provider driver。

use async_trait::async_trait;

use crate::errors::Result;
use crate::external_auth::driver::{
    ExternalAuthAuthorizationStart, ExternalAuthCallback, ExternalAuthProfile,
    ExternalAuthProviderConfig, ExternalAuthProviderDescriptor, ExternalAuthProviderDriver,
    ExternalAuthProviderTestResult,
};
use crate::types::{ExternalAuthProtocol, ExternalAuthProviderKind};

use super::oidc::OidcProviderDriver;

/// Google Accounts OIDC issuer advertised by the discovery document.
const GOOGLE_ISSUER_URL: &str = "https://accounts.google.com";
/// Minimal Google sign-in scopes. Google API / Drive access belongs to a
/// separate OAuth authorization feature and should not be added here.
const GOOGLE_DEFAULT_SCOPES: &str = "openid profile email";

/// Dedicated Google sign-in provider backed by the generic OIDC driver.
#[derive(Default)]
pub struct GoogleProviderDriver;

impl GoogleProviderDriver {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ExternalAuthProviderDriver for GoogleProviderDriver {
    fn kind(&self) -> ExternalAuthProviderKind {
        ExternalAuthProviderKind::Google
    }

    fn descriptor(&self) -> ExternalAuthProviderDescriptor {
        ExternalAuthProviderDescriptor {
            kind: ExternalAuthProviderKind::Google,
            protocol: ExternalAuthProtocol::Oidc,
            display_name: "Google",
            description: "Google OpenID Connect sign-in with fixed issuer and standard email_verified semantics.",
            default_scopes: GOOGLE_DEFAULT_SCOPES,
            issuer_url_required: false,
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
        OidcProviderDriver::new()
            .start_authorization(&google_oidc_config(provider), redirect_uri)
            .await
    }

    async fn exchange_callback(
        &self,
        provider: &ExternalAuthProviderConfig,
        callback: ExternalAuthCallback,
    ) -> Result<ExternalAuthProfile> {
        OidcProviderDriver::new()
            .exchange_callback(&google_oidc_config(provider), callback)
            .await
    }

    async fn test_provider(
        &self,
        provider: &ExternalAuthProviderConfig,
    ) -> Result<ExternalAuthProviderTestResult> {
        let mut result = OidcProviderDriver::new()
            .test_provider(&google_oidc_config(provider))
            .await?;
        result.provider = self.descriptor().display_name.to_string();
        Ok(result)
    }
}

/// Applies the Google preset while still allowing tests to inject a loopback issuer.
fn google_oidc_config(provider: &ExternalAuthProviderConfig) -> ExternalAuthProviderConfig {
    let mut provider = provider.clone();
    provider.provider_kind = ExternalAuthProviderKind::Google;
    provider.protocol = ExternalAuthProtocol::Oidc;
    provider.issuer_url = provider
        .issuer_url
        .filter(|value| !value.trim().is_empty())
        .or_else(|| Some(GOOGLE_ISSUER_URL.to_string()));
    provider.authorization_url = None;
    provider.token_url = None;
    provider.userinfo_url = None;
    provider.scopes = if provider.scopes.trim().is_empty() {
        GOOGLE_DEFAULT_SCOPES.to_string()
    } else {
        provider.scopes.trim().to_string()
    };
    provider.subject_claim = provider.subject_claim.or_else(|| Some("sub".to_string()));
    provider.display_name_claim = provider
        .display_name_claim
        .or_else(|| Some("name".to_string()));
    provider.email_claim = provider.email_claim.or_else(|| Some("email".to_string()));
    provider.email_verified_claim = provider
        .email_verified_claim
        .or_else(|| Some("email_verified".to_string()));
    provider.avatar_url_claim = provider
        .avatar_url_claim
        .or_else(|| Some("picture".to_string()));
    provider
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> ExternalAuthProviderConfig {
        ExternalAuthProviderConfig {
            id: 1,
            key: "google".to_string(),
            provider_kind: ExternalAuthProviderKind::Google,
            protocol: ExternalAuthProtocol::Oidc,
            issuer_url: None,
            authorization_url: Some("https://ignored.example.com/auth".to_string()),
            token_url: Some("https://ignored.example.com/token".to_string()),
            userinfo_url: Some("https://ignored.example.com/userinfo".to_string()),
            client_id: "client-id".to_string(),
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
    fn google_config_uses_fixed_defaults_and_claims() {
        let config = google_oidc_config(&provider());

        assert_eq!(config.provider_kind, ExternalAuthProviderKind::Google);
        assert_eq!(config.protocol, ExternalAuthProtocol::Oidc);
        assert_eq!(config.issuer_url.as_deref(), Some(GOOGLE_ISSUER_URL));
        assert_eq!(config.authorization_url, None);
        assert_eq!(config.token_url, None);
        assert_eq!(config.userinfo_url, None);
        assert_eq!(config.scopes, GOOGLE_DEFAULT_SCOPES);
        assert_eq!(config.subject_claim.as_deref(), Some("sub"));
        assert_eq!(config.display_name_claim.as_deref(), Some("name"));
        assert_eq!(config.email_claim.as_deref(), Some("email"));
        assert_eq!(
            config.email_verified_claim.as_deref(),
            Some("email_verified")
        );
        assert_eq!(config.avatar_url_claim.as_deref(), Some("picture"));
    }

    #[test]
    fn google_config_keeps_test_issuer_override() {
        let mut provider = provider();
        provider.issuer_url = Some("http://127.0.0.1:3000".to_string());

        let config = google_oidc_config(&provider);

        assert_eq!(config.issuer_url.as_deref(), Some("http://127.0.0.1:3000"));
    }
}

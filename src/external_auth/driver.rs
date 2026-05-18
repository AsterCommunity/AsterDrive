//! 外部认证 provider driver trait。

use crate::errors::{AsterError, Result};
use crate::types::{ExternalAuthProtocol, ExternalAuthProviderKind};
use async_trait::async_trait;
use serde::Serialize;
use std::fmt;

#[derive(Clone, Debug)]
pub struct ExternalAuthProviderDescriptor {
    pub kind: ExternalAuthProviderKind,
    pub protocol: ExternalAuthProtocol,
    pub display_name: &'static str,
    pub description: &'static str,
    pub default_scopes: &'static str,
    pub issuer_url_required: bool,
    pub manual_endpoint_configuration_supported: bool,
    pub authorization_url_required: bool,
    pub token_url_required: bool,
    pub userinfo_url_required: bool,
    pub supports_discovery: bool,
    pub supports_pkce: bool,
    pub supports_email_verified_claim: bool,
}

#[derive(Clone)]
pub struct ExternalAuthProviderConfig {
    pub id: i64,
    pub key: String,
    pub provider_kind: ExternalAuthProviderKind,
    pub protocol: ExternalAuthProtocol,
    pub issuer_url: Option<String>,
    pub authorization_url: Option<String>,
    pub token_url: Option<String>,
    pub userinfo_url: Option<String>,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub scopes: String,
    pub subject_claim: Option<String>,
    pub username_claim: Option<String>,
    pub display_name_claim: Option<String>,
    pub email_claim: Option<String>,
    pub email_verified_claim: Option<String>,
    pub groups_claim: Option<String>,
    pub avatar_url_claim: Option<String>,
}

impl fmt::Debug for ExternalAuthProviderConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExternalAuthProviderConfig")
            .field("id", &self.id)
            .field("key", &self.key)
            .field("provider_kind", &self.provider_kind)
            .field("protocol", &self.protocol)
            .field("issuer_url", &self.issuer_url)
            .field("authorization_url", &self.authorization_url)
            .field("token_url", &self.token_url)
            .field("userinfo_url", &self.userinfo_url)
            .field("client_id", &self.client_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "***REDACTED***"),
            )
            .field("scopes", &self.scopes)
            .field("subject_claim", &self.subject_claim)
            .field("username_claim", &self.username_claim)
            .field("display_name_claim", &self.display_name_claim)
            .field("email_claim", &self.email_claim)
            .field("email_verified_claim", &self.email_verified_claim)
            .field("groups_claim", &self.groups_claim)
            .field("avatar_url_claim", &self.avatar_url_claim)
            .finish()
    }
}

impl ExternalAuthProviderConfig {
    pub fn from_provider(provider: &crate::entities::external_auth_provider::Model) -> Self {
        Self {
            id: provider.id,
            key: provider.key.clone(),
            provider_kind: provider.provider_kind,
            protocol: provider.protocol,
            issuer_url: provider.issuer_url.clone(),
            authorization_url: provider.authorization_url.clone(),
            token_url: provider.token_url.clone(),
            userinfo_url: provider.userinfo_url.clone(),
            client_id: provider.client_id.clone(),
            client_secret: provider.client_secret.clone(),
            scopes: provider.scopes.clone(),
            subject_claim: provider.subject_claim.clone(),
            username_claim: provider.username_claim.clone(),
            display_name_claim: provider.display_name_claim.clone(),
            email_claim: provider.email_claim.clone(),
            email_verified_claim: provider.email_verified_claim.clone(),
            groups_claim: provider.groups_claim.clone(),
            avatar_url_claim: provider.avatar_url_claim.clone(),
        }
    }

    pub fn require_issuer_url(&self) -> Result<&str> {
        self.issuer_url
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                AsterError::validation_error("external auth provider missing issuer_url")
            })
    }
}

#[derive(Clone, Debug)]
pub struct ExternalAuthAuthorizationStart {
    pub authorization_url: String,
    pub state: String,
    pub nonce: Option<String>,
    pub pkce_verifier: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ExternalAuthCallback {
    pub code: String,
    pub nonce: Option<String>,
    pub pkce_verifier: Option<String>,
    pub redirect_uri: String,
}

#[derive(Clone, Debug)]
pub struct ExternalAuthProfile {
    pub identity_namespace: String,
    pub subject: String,
    pub email: Option<String>,
    pub email_verified: bool,
    pub display_name: Option<String>,
    pub preferred_username: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthProviderTestCheck {
    pub name: String,
    pub success: bool,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthProviderTestResult {
    pub provider: String,
    pub issuer: Option<String>,
    pub authorization_endpoint: Option<String>,
    pub token_endpoint: Option<String>,
    pub userinfo_endpoint: Option<String>,
    pub jwks_key_count: Option<usize>,
    pub checks: Vec<ExternalAuthProviderTestCheck>,
}

#[async_trait]
pub trait ExternalAuthProviderDriver: Send + Sync {
    fn kind(&self) -> ExternalAuthProviderKind;

    fn descriptor(&self) -> ExternalAuthProviderDescriptor;

    async fn start_authorization(
        &self,
        provider: &ExternalAuthProviderConfig,
        redirect_uri: &str,
    ) -> Result<ExternalAuthAuthorizationStart>;

    async fn exchange_callback(
        &self,
        provider: &ExternalAuthProviderConfig,
        callback: ExternalAuthCallback,
    ) -> Result<ExternalAuthProfile>;

    async fn test_provider(
        &self,
        provider: &ExternalAuthProviderConfig,
    ) -> Result<ExternalAuthProviderTestResult>;
}

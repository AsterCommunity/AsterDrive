//! Microsoft Entra ID / Microsoft Account 外部认证 provider driver。

use async_trait::async_trait;
use openidconnect::core::{
    CoreClient, CoreIdTokenVerifier, CoreJsonWebKeySet, CoreProviderMetadata,
};
use openidconnect::{ClientId, IssuerUrl, Nonce};

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::external_auth::driver::{
    ExternalAuthAuthorizationStart, ExternalAuthCallback, ExternalAuthProfile,
    ExternalAuthProviderConfig, ExternalAuthProviderDescriptor, ExternalAuthProviderDriver,
    ExternalAuthProviderTestCheck, ExternalAuthProviderTestResult,
};
use crate::types::{ExternalAuthProtocol, ExternalAuthProviderKind};

use super::oidc::{
    OidcClient, oidc_http_client, profile_from_id_token, start_authorization_with_oidc_client,
};

const MICROSOFT_DEFAULT_TENANT: &str = "common";
const MICROSOFT_LOGIN_HOST: &str = "login.microsoftonline.com";
const MICROSOFT_LOGIN_BASE: &str = "https://login.microsoftonline.com";
const MICROSOFT_DEFAULT_SCOPES: &str = "openid profile email";
const MICROSOFT_ACCOUNT_TENANT_ID: &str = "9188040d-6c67-4c5b-b112-36a304b66dad";
const MICROSOFT_TENANT_ID_LEN: usize = 36;
const MICROSOFT_TENANT_ID_HYPHEN_POSITIONS: [usize; 4] = [8, 13, 18, 23];

/// Dedicated Microsoft sign-in provider backed by the generic OIDC driver.
#[derive(Default)]
pub struct MicrosoftProviderDriver;

impl MicrosoftProviderDriver {
    /// Creates a Microsoft provider driver with fixed Microsoft OIDC defaults.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ExternalAuthProviderDriver for MicrosoftProviderDriver {
    fn kind(&self) -> ExternalAuthProviderKind {
        ExternalAuthProviderKind::Microsoft
    }

    fn descriptor(&self) -> ExternalAuthProviderDescriptor {
        ExternalAuthProviderDescriptor {
            kind: ExternalAuthProviderKind::Microsoft,
            protocol: ExternalAuthProtocol::Oidc,
            display_name: "Microsoft",
            description: "Microsoft Entra ID OpenID Connect sign-in with tenant-aware issuer handling.",
            default_scopes: MICROSOFT_DEFAULT_SCOPES,
            issuer_url_required: false,
            manual_endpoint_configuration_supported: false,
            authorization_url_required: false,
            token_url_required: false,
            userinfo_url_required: false,
            supports_discovery: true,
            supports_pkce: true,
            supports_email_verified_claim: false,
        }
    }

    async fn start_authorization(
        &self,
        provider: &ExternalAuthProviderConfig,
        redirect_uri: &str,
    ) -> Result<ExternalAuthAuthorizationStart> {
        let provider = microsoft_oidc_config(provider)?;
        let client = build_microsoft_client(&provider, redirect_uri).await?;
        start_authorization_with_oidc_client(&provider, client)
    }

    async fn exchange_callback(
        &self,
        provider: &ExternalAuthProviderConfig,
        callback: ExternalAuthCallback,
    ) -> Result<ExternalAuthProfile> {
        exchange_microsoft_callback(&microsoft_oidc_config(provider)?, callback).await
    }

    async fn test_provider(
        &self,
        provider: &ExternalAuthProviderConfig,
    ) -> Result<ExternalAuthProviderTestResult> {
        test_microsoft_provider(&microsoft_oidc_config(provider)?).await
    }
}

/// Normalizes admin input into the Microsoft OIDC issuer stored on the provider.
///
/// Empty input falls back to `common`. Non-URL input is interpreted as a tenant
/// selector (`common`, `organizations`, `consumers`, or a tenant UUID), while a
/// full URL must already point at `login.microsoftonline.com/{tenant}/v2.0`.
pub fn normalize_microsoft_tenant_or_issuer_url(value: Option<String>) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(Some(microsoft_issuer_url_for_tenant(
            MICROSOFT_DEFAULT_TENANT,
        )));
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(Some(microsoft_issuer_url_for_tenant(
            MICROSOFT_DEFAULT_TENANT,
        )));
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return normalize_microsoft_issuer_url(trimmed).map(Some);
    }
    validate_microsoft_tenant(trimmed)?;
    Ok(Some(microsoft_issuer_url_for_tenant(trimmed)))
}

/// Applies fixed Microsoft defaults before delegating to the generic OIDC driver.
///
/// Tests may still inject a loopback issuer so local mock OIDC servers can
/// exercise the callback path without pretending to be Microsoft.
fn microsoft_oidc_config(
    provider: &ExternalAuthProviderConfig,
) -> Result<ExternalAuthProviderConfig> {
    let mut provider = provider.clone();
    provider.provider_kind = ExternalAuthProviderKind::Microsoft;
    provider.protocol = ExternalAuthProtocol::Oidc;
    provider.issuer_url = normalize_microsoft_tenant_or_issuer_url(provider.issuer_url.clone())?;
    provider.authorization_url = None;
    provider.token_url = None;
    provider.userinfo_url = None;
    provider.scopes = if provider.scopes.trim().is_empty() {
        MICROSOFT_DEFAULT_SCOPES.to_string()
    } else {
        provider.scopes.trim().to_string()
    };
    provider.subject_claim = provider.subject_claim.or_else(|| Some("sub".to_string()));
    provider.display_name_claim = provider
        .display_name_claim
        .or_else(|| Some("name".to_string()));
    provider.email_claim = provider.email_claim.or_else(|| Some("email".to_string()));
    provider.email_verified_claim = None;
    provider.avatar_url_claim = None;
    Ok(provider)
}

async fn exchange_microsoft_callback(
    provider: &ExternalAuthProviderConfig,
    callback: ExternalAuthCallback,
) -> Result<ExternalAuthProfile> {
    let nonce = callback
        .nonce
        .ok_or_else(|| AsterError::database_operation("stored OIDC nonce is missing"))?;
    let pkce_verifier = callback
        .pkce_verifier
        .clone()
        .ok_or_else(|| AsterError::database_operation("stored OIDC PKCE verifier is missing"))?;
    let metadata = discover_microsoft_provider(provider).await?;
    let client = build_client_from_metadata(provider, &callback.redirect_uri, metadata.clone())?;
    let http_client = oidc_http_client()?;
    let token_request = client
        .exchange_code(openidconnect::AuthorizationCode::new(callback.code))
        .map_aster_err_ctx(
            "OIDC provider metadata missing token endpoint",
            AsterError::config_error,
        )?;
    let token_response = token_request
        .set_pkce_verifier(openidconnect::PkceCodeVerifier::new(pkce_verifier))
        .set_redirect_uri(std::borrow::Cow::Owned(
            openidconnect::RedirectUrl::new(callback.redirect_uri.clone()).map_aster_err_ctx(
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
    let verifier = CoreIdTokenVerifier::new_public_client(
        ClientId::new(provider.client_id.clone()),
        IssuerUrl::new(provider.require_issuer_url()?.to_string())
            .map_aster_err_ctx("invalid Microsoft issuer URL", AsterError::validation_error)?,
        metadata.jwks().clone(),
    )
    .require_issuer_match(false);
    let nonce = Nonce::new(nonce);
    let claims = id_token.claims(&verifier, &nonce).map_aster_err_ctx(
        "OIDC ID token verification failed",
        AsterError::auth_invalid_credentials,
    )?;
    let profile = profile_from_id_token(claims)?;
    validate_microsoft_token_issuer(provider.require_issuer_url()?, &profile.identity_namespace)?;
    Ok(profile)
}

async fn test_microsoft_provider(
    provider: &ExternalAuthProviderConfig,
) -> Result<ExternalAuthProviderTestResult> {
    let metadata = discover_microsoft_provider(provider).await?;
    let token_endpoint = metadata.token_endpoint().ok_or_else(|| {
        AsterError::validation_error("OIDC discovery metadata missing token_endpoint")
    })?;
    let jwks_key_count = metadata.jwks().keys().len();
    Ok(ExternalAuthProviderTestResult {
        provider: "Microsoft".to_string(),
        issuer: Some(metadata.issuer().as_str().to_string()),
        authorization_endpoint: Some(metadata.authorization_endpoint().as_str().to_string()),
        token_endpoint: Some(token_endpoint.as_str().to_string()),
        userinfo_endpoint: metadata
            .userinfo_endpoint()
            .map(|url| url.as_str().to_string()),
        jwks_key_count: Some(jwks_key_count),
        checks: vec![
            ExternalAuthProviderTestCheck {
                name: "discovery".to_string(),
                success: true,
                message: "Microsoft OIDC discovery metadata was loaded".to_string(),
            },
            ExternalAuthProviderTestCheck {
                name: "jwks".to_string(),
                success: true,
                message: format!("JWKS contains {jwks_key_count} key(s)"),
            },
        ],
    })
}

async fn build_microsoft_client(
    provider: &ExternalAuthProviderConfig,
    redirect_uri: &str,
) -> Result<OidcClient> {
    let metadata = discover_microsoft_provider(provider).await?;
    build_client_from_metadata(provider, redirect_uri, metadata)
}

fn build_client_from_metadata(
    provider: &ExternalAuthProviderConfig,
    redirect_uri: &str,
    metadata: CoreProviderMetadata,
) -> Result<OidcClient> {
    let client_secret = provider
        .client_secret
        .clone()
        .filter(|secret| !secret.is_empty())
        .map(openidconnect::ClientSecret::new);
    let redirect_uri = openidconnect::RedirectUrl::new(redirect_uri.to_string())
        .map_aster_err_ctx("invalid OIDC redirect URI", AsterError::validation_error)?;
    Ok(CoreClient::from_provider_metadata(
        metadata,
        ClientId::new(provider.client_id.clone()),
        client_secret,
    )
    .set_redirect_uri(redirect_uri))
}

async fn discover_microsoft_provider(
    provider: &ExternalAuthProviderConfig,
) -> Result<CoreProviderMetadata> {
    let configured_issuer = provider.require_issuer_url()?;
    let discovery_url = microsoft_discovery_url(configured_issuer)?;
    let http_client = oidc_http_client()?;
    let response = http_client
        .get(discovery_url.as_str())
        .send()
        .await
        .map_aster_err_ctx("OIDC discovery failed", AsterError::validation_error)?;
    let status = response.status();
    if !status.is_success() {
        return Err(AsterError::validation_error(format!(
            "OIDC discovery failed: HTTP status code {status} at {discovery_url}"
        )));
    }
    let body = response
        .bytes()
        .await
        .map_aster_err_ctx("OIDC discovery failed", AsterError::validation_error)?;
    let metadata: CoreProviderMetadata = serde_json::from_slice(&body).map_aster_err_ctx(
        "OIDC discovery metadata parse failed",
        AsterError::validation_error,
    )?;
    validate_microsoft_discovery_issuer(configured_issuer, metadata.issuer().as_str())?;
    let jwks = CoreJsonWebKeySet::fetch_async(metadata.jwks_uri(), &http_client)
        .await
        .map_aster_err_ctx("OIDC JWKS fetch failed", AsterError::validation_error)?;
    Ok(metadata.set_jwks(jwks))
}

fn microsoft_discovery_url(configured_issuer: &str) -> Result<reqwest::Url> {
    let issuer = reqwest::Url::parse(configured_issuer)
        .map_aster_err_ctx("invalid Microsoft issuer URL", AsterError::validation_error)?;
    if issuer.scheme() == "https" && issuer.host_str() == Some(MICROSOFT_LOGIN_HOST) {
        let tenant = microsoft_tenant_from_issuer_url(&issuer)?;
        return reqwest::Url::parse(&format!(
            "{MICROSOFT_LOGIN_BASE}/{tenant}/v2.0/.well-known/openid-configuration"
        ))
        .map_aster_err_ctx(
            "invalid Microsoft discovery URL",
            AsterError::validation_error,
        );
    }
    append_well_known_openid_configuration(issuer)
}

fn append_well_known_openid_configuration(mut issuer: reqwest::Url) -> Result<reqwest::Url> {
    {
        let mut paths = issuer
            .path_segments_mut()
            .map_err(|_| AsterError::validation_error("invalid Microsoft issuer URL"))?;
        paths.pop_if_empty();
        paths.push(".well-known");
        paths.push("openid-configuration");
    }
    issuer.set_query(None);
    issuer.set_fragment(None);
    Ok(issuer)
}

fn validate_microsoft_discovery_issuer(
    configured_issuer: &str,
    discovery_issuer: &str,
) -> Result<()> {
    if configured_issuer == discovery_issuer {
        return Ok(());
    }
    let configured = reqwest::Url::parse(configured_issuer).map_aster_err_ctx(
        "invalid configured Microsoft issuer URL",
        AsterError::validation_error,
    )?;
    if configured.scheme() == "http" && configured.host_str().is_some_and(is_loopback_host) {
        return Err(AsterError::validation_error(
            "Microsoft discovery issuer does not match configured provider",
        ));
    }
    if configured.scheme() != "https" || configured.host_str() != Some(MICROSOFT_LOGIN_HOST) {
        return Err(AsterError::validation_error(
            "Microsoft discovery issuer is not trusted",
        ));
    }
    let configured_tenant = microsoft_tenant_from_issuer_url(&configured)?;
    if !is_microsoft_multi_tenant_alias(configured_tenant) {
        return Err(AsterError::validation_error(
            "Microsoft discovery issuer does not match configured provider",
        ));
    }
    // Microsoft tenant-independent metadata returns a templated issuer instead
    // of the exact configured alias, for example common -> {tenantid}.
    let expected_template = microsoft_issuer_url_for_tenant("{tenantid}");
    if discovery_issuer == expected_template {
        return Ok(());
    }
    validate_microsoft_token_issuer(configured_issuer, discovery_issuer).map_err(|_| {
        AsterError::validation_error(
            "Microsoft discovery issuer does not match configured provider",
        )
    })
}

/// Builds the public Microsoft identity platform issuer for a tenant selector.
fn microsoft_issuer_url_for_tenant(tenant: &str) -> String {
    format!("{MICROSOFT_LOGIN_BASE}/{}/v2.0", tenant.trim())
}

/// Normalizes a full Microsoft issuer URL and rejects non-Microsoft hosts.
fn normalize_microsoft_issuer_url(value: &str) -> Result<String> {
    let normalized = value.trim_end_matches('/');
    let parsed = reqwest::Url::parse(normalized)
        .map_aster_err_ctx("invalid Microsoft issuer URL", AsterError::validation_error)?;
    if parsed.scheme() == "http" && parsed.host_str().is_some_and(is_loopback_host) {
        return Ok(normalized.to_string());
    }
    if parsed.scheme() != "https" || parsed.host_str() != Some(MICROSOFT_LOGIN_HOST) {
        return Err(AsterError::validation_error(
            "Microsoft issuer URL must use login.microsoftonline.com",
        ));
    }
    let tenant = microsoft_tenant_from_issuer_url(&parsed)?;
    Ok(microsoft_issuer_url_for_tenant(tenant))
}

/// Extracts and validates the tenant segment from a Microsoft issuer URL.
fn microsoft_tenant_from_issuer_url(parsed: &reqwest::Url) -> Result<&str> {
    let segments = parsed
        .path_segments()
        .ok_or_else(|| AsterError::validation_error("Microsoft issuer URL missing tenant"))?
        .collect::<Vec<_>>();
    if segments.len() != 2 || segments[1] != "v2.0" {
        return Err(AsterError::validation_error(
            "Microsoft issuer URL must end with /{tenant}/v2.0",
        ));
    }
    validate_microsoft_tenant(segments[0])?;
    Ok(segments[0])
}

/// Validates the issuer returned by the ID token against Microsoft tenant rules.
///
/// Concrete tenant issuers must match exactly. Multi-tenant aliases such as
/// `common`, `organizations`, and `consumers` may receive tokens issued by a
/// concrete tenant under `login.microsoftonline.com`, constrained by each
/// alias' account-type semantics.
fn validate_microsoft_token_issuer(configured_issuer: &str, token_issuer: &str) -> Result<()> {
    if configured_issuer == token_issuer {
        return Ok(());
    }
    let configured = reqwest::Url::parse(configured_issuer).map_aster_err_ctx(
        "invalid configured Microsoft issuer URL",
        AsterError::validation_error,
    )?;
    let token = reqwest::Url::parse(token_issuer).map_aster_err_ctx(
        "invalid Microsoft token issuer",
        AsterError::auth_invalid_credentials,
    )?;
    if configured.scheme() == "http" && configured.host_str().is_some_and(is_loopback_host) {
        return Err(AsterError::auth_invalid_credentials(
            "OIDC issuer does not match configured provider",
        ));
    }
    if configured.host_str() != Some(MICROSOFT_LOGIN_HOST)
        || token.host_str() != Some(MICROSOFT_LOGIN_HOST)
        || token.scheme() != "https"
    {
        return Err(AsterError::auth_invalid_credentials(
            "Microsoft token issuer is not trusted",
        ));
    }
    let configured_tenant = microsoft_tenant_from_issuer_url(&configured)?;
    let token_tenant = microsoft_tenant_from_issuer_url(&token)
        .map_err(|_| AsterError::auth_invalid_credentials("Microsoft token issuer is invalid"))?;
    if is_microsoft_token_tenant_allowed_for_configured_tenant(configured_tenant, token_tenant) {
        return Ok(());
    }
    Err(AsterError::auth_invalid_credentials(
        "OIDC issuer does not match configured provider",
    ))
}

fn is_microsoft_token_tenant_allowed_for_configured_tenant(
    configured_tenant: &str,
    token_tenant: &str,
) -> bool {
    if !is_microsoft_tenant_id(token_tenant) {
        return false;
    }
    match configured_tenant {
        "common" => true,
        "organizations" => !token_tenant.eq_ignore_ascii_case(MICROSOFT_ACCOUNT_TENANT_ID),
        "consumers" => token_tenant.eq_ignore_ascii_case(MICROSOFT_ACCOUNT_TENANT_ID),
        _ => false,
    }
}

/// Allows Microsoft multi-tenant aliases and UUID tenant IDs.
fn validate_microsoft_tenant(tenant: &str) -> Result<()> {
    if is_microsoft_multi_tenant_alias(tenant) || is_microsoft_tenant_id(tenant) {
        return Ok(());
    }
    Err(AsterError::validation_error(
        "Microsoft tenant must be common, organizations, consumers, or a tenant ID",
    ))
}

fn is_microsoft_multi_tenant_alias(value: &str) -> bool {
    matches!(value, "common" | "organizations" | "consumers")
}

fn is_microsoft_tenant_id(value: &str) -> bool {
    value.len() == MICROSOFT_TENANT_ID_LEN
        && value.chars().enumerate().all(|(index, ch)| {
            if MICROSOFT_TENANT_ID_HYPHEN_POSITIONS.contains(&index) {
                ch == '-'
            } else {
                ch.is_ascii_hexdigit()
            }
        })
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

#[cfg(test)]
mod tests {
    use super::*;

    const TENANT_ID: &str = "11111111-2222-3333-4444-555555555555";
    const OTHER_TENANT_ID: &str = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";

    fn provider() -> ExternalAuthProviderConfig {
        ExternalAuthProviderConfig {
            id: 1,
            key: "microsoft".to_string(),
            provider_kind: ExternalAuthProviderKind::Microsoft,
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
            email_verified_claim: Some("email_verified".to_string()),
            groups_claim: None,
            avatar_url_claim: Some("picture".to_string()),
        }
    }

    #[test]
    fn microsoft_config_uses_default_common_issuer_and_claims() {
        let config = microsoft_oidc_config(&provider()).unwrap();

        assert_eq!(config.provider_kind, ExternalAuthProviderKind::Microsoft);
        assert_eq!(config.protocol, ExternalAuthProtocol::Oidc);
        assert_eq!(
            config.issuer_url.as_deref(),
            Some("https://login.microsoftonline.com/common/v2.0")
        );
        assert_eq!(config.authorization_url, None);
        assert_eq!(config.token_url, None);
        assert_eq!(config.userinfo_url, None);
        assert_eq!(config.scopes, MICROSOFT_DEFAULT_SCOPES);
        assert_eq!(config.subject_claim.as_deref(), Some("sub"));
        assert_eq!(config.display_name_claim.as_deref(), Some("name"));
        assert_eq!(config.email_claim.as_deref(), Some("email"));
        assert_eq!(config.email_verified_claim, None);
        assert_eq!(config.avatar_url_claim, None);
    }

    #[test]
    fn microsoft_tenant_normalization_accepts_supported_tenants() {
        for tenant in ["common", "organizations", "consumers", TENANT_ID] {
            assert_eq!(
                normalize_microsoft_tenant_or_issuer_url(Some(tenant.to_string())).unwrap(),
                Some(format!("https://login.microsoftonline.com/{tenant}/v2.0"))
            );
        }
        assert_eq!(
            normalize_microsoft_tenant_or_issuer_url(Some(format!(
                "https://login.microsoftonline.com/{TENANT_ID}/v2.0/"
            )))
            .unwrap(),
            Some(format!(
                "https://login.microsoftonline.com/{TENANT_ID}/v2.0"
            ))
        );
        assert_eq!(
            normalize_microsoft_tenant_or_issuer_url(Some("http://127.0.0.1:3000".to_string()))
                .unwrap(),
            Some("http://127.0.0.1:3000".to_string())
        );
    }

    #[test]
    fn microsoft_tenant_normalization_rejects_unsupported_tenants() {
        for tenant in [
            "tenant.example.com",
            "https://login.example.com/common/v2.0",
            "https://login.microsoftonline.com/common",
            "common/v2.0",
        ] {
            assert!(
                normalize_microsoft_tenant_or_issuer_url(Some(tenant.to_string())).is_err(),
                "{tenant} should be rejected"
            );
        }
    }

    #[test]
    fn microsoft_config_rejects_invalid_issuer_instead_of_falling_back_to_common() {
        let mut provider = provider();
        provider.issuer_url = Some("https://login.example.com/common/v2.0".to_string());

        assert!(microsoft_oidc_config(&provider).is_err());
    }

    #[test]
    fn microsoft_discovery_url_keeps_the_v2_metadata_path() {
        assert_eq!(
            microsoft_discovery_url("https://login.microsoftonline.com/common/v2.0")
                .unwrap()
                .as_str(),
            "https://login.microsoftonline.com/common/v2.0/.well-known/openid-configuration"
        );
        assert_eq!(
            microsoft_discovery_url(&format!(
                "https://login.microsoftonline.com/{TENANT_ID}/v2.0"
            ))
            .unwrap()
            .as_str(),
            format!(
                "https://login.microsoftonline.com/{TENANT_ID}/v2.0/.well-known/openid-configuration"
            )
        );
        assert_eq!(
            microsoft_discovery_url("http://127.0.0.1:3000/mock/tenant/v2.0")
                .unwrap()
                .as_str(),
            "http://127.0.0.1:3000/mock/tenant/v2.0/.well-known/openid-configuration"
        );
    }

    #[test]
    fn microsoft_issuer_validation_accepts_multi_tenant_token_issuers() {
        for configured_tenant in ["common", "organizations"] {
            validate_microsoft_token_issuer(
                &format!("https://login.microsoftonline.com/{configured_tenant}/v2.0"),
                &format!("https://login.microsoftonline.com/{TENANT_ID}/v2.0"),
            )
            .expect("multi-tenant issuer should accept concrete tenant token issuer");
        }
        for configured_tenant in ["common", "consumers"] {
            validate_microsoft_token_issuer(
                &format!("https://login.microsoftonline.com/{configured_tenant}/v2.0"),
                &format!("https://login.microsoftonline.com/{MICROSOFT_ACCOUNT_TENANT_ID}/v2.0"),
            )
            .expect("Microsoft Account tenant issuer should match consumer-capable aliases");
        }
        validate_microsoft_token_issuer(
            &format!("https://login.microsoftonline.com/{TENANT_ID}/v2.0"),
            &format!("https://login.microsoftonline.com/{TENANT_ID}/v2.0"),
        )
        .expect("specific tenant should accept exact issuer");
    }

    #[test]
    fn microsoft_discovery_validation_accepts_multi_tenant_template_issuer() {
        for configured_tenant in ["common", "organizations", "consumers"] {
            validate_microsoft_discovery_issuer(
                &format!("https://login.microsoftonline.com/{configured_tenant}/v2.0"),
                "https://login.microsoftonline.com/{tenantid}/v2.0",
            )
            .expect("multi-tenant discovery should accept Microsoft template issuer");
        }
        validate_microsoft_discovery_issuer(
            &format!("https://login.microsoftonline.com/{TENANT_ID}/v2.0"),
            &format!("https://login.microsoftonline.com/{TENANT_ID}/v2.0"),
        )
        .expect("specific tenant discovery should accept exact issuer");
    }

    #[test]
    fn microsoft_discovery_validation_rejects_template_for_specific_tenant() {
        validate_microsoft_discovery_issuer(
            &format!("https://login.microsoftonline.com/{TENANT_ID}/v2.0"),
            "https://login.microsoftonline.com/{tenantid}/v2.0",
        )
        .expect_err("specific tenant discovery should not accept template issuer");
    }

    #[test]
    fn microsoft_issuer_validation_rejects_mismatches() {
        for (configured, token) in [
            (
                format!("https://login.microsoftonline.com/{TENANT_ID}/v2.0"),
                format!("https://login.microsoftonline.com/{OTHER_TENANT_ID}/v2.0"),
            ),
            (
                "https://login.microsoftonline.com/organizations/v2.0".to_string(),
                format!("https://login.microsoftonline.com/{MICROSOFT_ACCOUNT_TENANT_ID}/v2.0"),
            ),
            (
                "https://login.microsoftonline.com/consumers/v2.0".to_string(),
                format!("https://login.microsoftonline.com/{TENANT_ID}/v2.0"),
            ),
            (
                "https://login.microsoftonline.com/common/v2.0".to_string(),
                "https://login.example.com/11111111-2222-3333-4444-555555555555/v2.0".to_string(),
            ),
            (
                "http://127.0.0.1:3000".to_string(),
                "http://127.0.0.1:4000".to_string(),
            ),
        ] {
            assert!(
                validate_microsoft_token_issuer(&configured, &token).is_err(),
                "{configured} should reject {token}"
            );
        }
    }
}

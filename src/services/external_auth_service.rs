//! 外部认证登录、身份绑定和 provider 管理业务逻辑。

use base64::Engine as _;
use chrono::{Duration, Utc};
use sea_orm::{ActiveValue::Set, IntoActiveModel};
use serde::{Deserialize, Serialize};

use crate::api::pagination::{OffsetPage, load_offset_page};
use crate::config::{auth_runtime::RuntimeAuthPolicy, branding, site_url};
use crate::db::repository::{
    external_auth_email_verification_flow_repo, external_auth_identity_repo,
    external_auth_login_flow_repo, external_auth_provider_repo, user_repo,
};
use crate::entities::{
    external_auth_email_verification_flow, external_auth_identity, external_auth_login_flow,
    external_auth_provider, user,
};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::external_auth::{
    ExternalAuthCallback, ExternalAuthProfile, ExternalAuthProviderConfig, registry,
};
use crate::runtime::PrimaryAppState;
use crate::services::auth_service::{self, LoginResult};
use crate::services::{mail_outbox_service, mail_service, mail_template::MailTemplatePayload};
use crate::types::{ExternalAuthProtocol, ExternalAuthProviderKind, UserRole, UserStatus};
use crate::utils::{hash, id, numbers::u64_to_i64};

const DEFAULT_SCOPES: &str = "openid email profile";
const FLOW_TTL_SECS: u64 = 300;
const EMAIL_VERIFICATION_FLOW_TTL_SECS: u64 = 1_800;
const REDACTED_SECRET: &str = "***REDACTED***";
const EXTERNAL_AUTH_USER_PASSWORD_BYTES: usize = 48;
const EXTERNAL_AUTH_IDENTITY_NAMESPACE_MAX_LEN: usize = 512;
const EXTERNAL_AUTH_URL_MAX_LEN: usize = 2048;
const USERNAME_MAX_LEN: usize = 16;
const USERNAME_MIN_LEN: usize = 4;

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthPublicProvider {
    pub key: String,
    pub kind: ExternalAuthProviderKind,
    pub display_name: String,
    pub icon_url: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthProviderKindInfo {
    pub kind: ExternalAuthProviderKind,
    pub protocol: ExternalAuthProtocol,
    pub display_name: String,
    pub description: String,
    pub default_scopes: String,
    pub issuer_url_required: bool,
    pub manual_endpoint_configuration_supported: bool,
    pub authorization_url_required: bool,
    pub token_url_required: bool,
    pub userinfo_url_required: bool,
    pub supports_discovery: bool,
    pub supports_pkce: bool,
    pub supports_email_verified_claim: bool,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthStartLoginRequest {
    pub return_path: Option<String>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthStartLoginResponse {
    pub authorization_url: String,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthEmailVerificationStartRequest {
    pub flow_token: String,
    pub email: String,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthPasswordLinkRequest {
    pub flow_token: String,
    pub identifier: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthEmailVerificationStartResponse {
    pub message: String,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(utoipa::IntoParams, utoipa::ToSchema)
)]
pub struct ExternalAuthEmailVerificationConfirmQuery {
    pub token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(utoipa::IntoParams, utoipa::ToSchema)
)]
pub struct ExternalAuthCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthLinkInfo {
    pub id: i64,
    pub provider_id: i64,
    pub provider_key: String,
    pub provider_kind: ExternalAuthProviderKind,
    pub provider_display_name: String,
    pub issuer: String,
    pub subject: String,
    pub email_snapshot: Option<String>,
    pub display_name_snapshot: Option<String>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub last_login_at: Option<chrono::DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct AdminExternalAuthProviderInfo {
    pub id: i64,
    pub key: String,
    pub provider_kind: ExternalAuthProviderKind,
    pub protocol: ExternalAuthProtocol,
    pub display_name: String,
    pub icon_url: Option<String>,
    pub issuer_url: Option<String>,
    pub authorization_url: Option<String>,
    pub token_url: Option<String>,
    pub userinfo_url: Option<String>,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub client_secret_configured: bool,
    pub scopes: String,
    pub enabled: bool,
    pub auto_provision_enabled: bool,
    pub auto_link_verified_email_enabled: bool,
    pub require_email_verified: bool,
    pub subject_claim: Option<String>,
    pub username_claim: Option<String>,
    pub display_name_claim: Option<String>,
    pub email_claim: Option<String>,
    pub email_verified_claim: Option<String>,
    pub groups_claim: Option<String>,
    pub avatar_url_claim: Option<String>,
    pub allowed_domains: Vec<String>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct CreateExternalAuthProviderInput {
    pub provider_kind: ExternalAuthProviderKind,
    pub display_name: String,
    pub icon_url: Option<String>,
    pub issuer_url: Option<String>,
    pub authorization_url: Option<String>,
    pub token_url: Option<String>,
    pub userinfo_url: Option<String>,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub scopes: Option<String>,
    pub enabled: Option<bool>,
    pub auto_provision_enabled: Option<bool>,
    pub auto_link_verified_email_enabled: Option<bool>,
    pub require_email_verified: Option<bool>,
    pub subject_claim: Option<String>,
    pub username_claim: Option<String>,
    pub display_name_claim: Option<String>,
    pub email_claim: Option<String>,
    pub email_verified_claim: Option<String>,
    pub groups_claim: Option<String>,
    pub avatar_url_claim: Option<String>,
    pub allowed_domains: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct UpdateExternalAuthProviderInput {
    pub display_name: Option<String>,
    pub icon_url: Option<Option<String>>,
    pub issuer_url: Option<Option<String>>,
    pub authorization_url: Option<Option<String>>,
    pub token_url: Option<Option<String>>,
    pub userinfo_url: Option<Option<String>>,
    pub client_id: Option<String>,
    pub client_secret: Option<Option<String>>,
    pub scopes: Option<String>,
    pub enabled: Option<bool>,
    pub auto_provision_enabled: Option<bool>,
    pub auto_link_verified_email_enabled: Option<bool>,
    pub require_email_verified: Option<bool>,
    pub subject_claim: Option<Option<String>>,
    pub username_claim: Option<Option<String>>,
    pub display_name_claim: Option<Option<String>>,
    pub email_claim: Option<Option<String>>,
    pub email_verified_claim: Option<Option<String>>,
    pub groups_claim: Option<Option<String>>,
    pub avatar_url_claim: Option<Option<String>>,
    pub allowed_domains: Option<Option<Vec<String>>>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthProviderTestParamsInput {
    pub provider_kind: ExternalAuthProviderKind,
    pub issuer_url: Option<String>,
    pub authorization_url: Option<String>,
    pub token_url: Option<String>,
    pub userinfo_url: Option<String>,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub scopes: Option<String>,
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

#[derive(Clone, Debug, Serialize)]
pub struct ExternalAuthLoginAuditDetails<'a> {
    pub provider_key: &'a str,
    pub issuer: &'a str,
    pub subject: &'a str,
    pub linked: bool,
    pub auto_provisioned: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExternalAuthProviderAuditDetails<'a> {
    pub key: &'a str,
    pub icon_url: Option<&'a str>,
    pub issuer_url: Option<&'a str>,
    pub enabled: bool,
    pub auto_provision_enabled: bool,
    pub auto_link_verified_email_enabled: bool,
    pub require_email_verified: bool,
}

type ExternalAuthUserClaims = ExternalAuthProfile;

#[derive(Debug)]
struct ResolvedExternalAuthUser {
    user: user::Model,
    linked: bool,
    auto_provisioned: bool,
}

pub struct PendingExternalAuthEmailVerification {
    pub flow_token: String,
    pub return_path: String,
}

pub struct ExternalAuthEmailVerificationConfirmResult {
    pub login: LoginResult,
    pub return_path: String,
    pub provider_key: String,
    pub issuer: String,
    pub subject: String,
    pub linked: bool,
    pub auto_provisioned: bool,
}

pub struct ExternalAuthPasswordLinkResult {
    pub login: LoginResult,
    pub return_path: String,
    pub provider_key: String,
    pub issuer: String,
    pub subject: String,
    pub linked: bool,
    pub auto_provisioned: bool,
}

pub enum ExternalAuthCallbackOutcome {
    Login(ExternalAuthCallbackResult),
    EmailVerificationRequired(PendingExternalAuthEmailVerification),
}

pub struct ExternalAuthCallbackResult {
    pub login: LoginResult,
    pub return_path: String,
    pub provider_key: String,
    pub issuer: String,
    pub subject: String,
    pub linked: bool,
    pub auto_provisioned: bool,
}

fn descriptor_to_info(
    descriptor: crate::external_auth::ExternalAuthProviderDescriptor,
) -> ExternalAuthProviderKindInfo {
    ExternalAuthProviderKindInfo {
        kind: descriptor.kind,
        protocol: descriptor.protocol,
        display_name: descriptor.display_name.to_string(),
        description: descriptor.description.to_string(),
        default_scopes: descriptor.default_scopes.to_string(),
        issuer_url_required: descriptor.issuer_url_required,
        manual_endpoint_configuration_supported: descriptor.manual_endpoint_configuration_supported,
        authorization_url_required: descriptor.authorization_url_required,
        token_url_required: descriptor.token_url_required,
        userinfo_url_required: descriptor.userinfo_url_required,
        supports_discovery: descriptor.supports_discovery,
        supports_pkce: descriptor.supports_pkce,
        supports_email_verified_claim: descriptor.supports_email_verified_claim,
    }
}

fn provider_to_public(model: external_auth_provider::Model) -> ExternalAuthPublicProvider {
    ExternalAuthPublicProvider {
        key: model.key,
        kind: model.provider_kind,
        display_name: model.display_name,
        icon_url: model.icon_url,
    }
}

fn parse_allowed_domains(raw: Option<&str>) -> Result<Vec<String>> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str::<Vec<String>>(trimmed).map_aster_err_ctx(
        "failed to parse external auth allowed domains",
        AsterError::database_operation,
    )
}

fn provider_to_admin(
    model: external_auth_provider::Model,
) -> Result<AdminExternalAuthProviderInfo> {
    let allowed_domains = parse_allowed_domains(model.allowed_domains.as_deref())?;
    Ok(AdminExternalAuthProviderInfo {
        id: model.id,
        key: model.key,
        provider_kind: model.provider_kind,
        protocol: model.protocol,
        display_name: model.display_name,
        icon_url: model.icon_url,
        issuer_url: model.issuer_url,
        authorization_url: model.authorization_url,
        token_url: model.token_url,
        userinfo_url: model.userinfo_url,
        client_id: model.client_id,
        client_secret: model
            .client_secret
            .as_ref()
            .filter(|secret| !secret.is_empty())
            .map(|_| REDACTED_SECRET.to_string()),
        client_secret_configured: model
            .client_secret
            .as_ref()
            .is_some_and(|secret| !secret.is_empty()),
        scopes: model.scopes,
        enabled: model.enabled,
        auto_provision_enabled: model.auto_provision_enabled,
        auto_link_verified_email_enabled: model.auto_link_verified_email_enabled,
        require_email_verified: model.require_email_verified,
        subject_claim: model.subject_claim,
        username_claim: model.username_claim,
        display_name_claim: model.display_name_claim,
        email_claim: model.email_claim,
        email_verified_claim: model.email_verified_claim,
        groups_claim: model.groups_claim,
        avatar_url_claim: model.avatar_url_claim,
        allowed_domains,
        created_at: model.created_at,
        updated_at: model.updated_at,
    })
}

fn normalize_key(value: &str) -> Result<String> {
    let key = value.trim().to_ascii_lowercase();
    if key.len() < 2 || key.len() > 64 {
        return Err(AsterError::validation_error(
            "external auth provider key must be 2-64 characters",
        ));
    }
    if !key
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(AsterError::validation_error(
            "external auth provider key may only contain lowercase letters, numbers and hyphens",
        ));
    }
    if key.starts_with('-') || key.ends_with('-') {
        return Err(AsterError::validation_error(
            "external auth provider key cannot start or end with '-'",
        ));
    }
    Ok(key)
}

fn normalize_required(value: &str, field: &str, max_len: usize) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AsterError::validation_error(format!("{field} is required")));
    }
    if trimmed.len() > max_len {
        return Err(AsterError::validation_error(format!(
            "{field} exceeds {max_len} bytes"
        )));
    }
    Ok(trimmed.to_string())
}

fn normalize_optional_claim(value: Option<String>, field: &str) -> Result<Option<String>> {
    match value {
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else if trimmed.len() > 128 {
                Err(AsterError::validation_error(format!(
                    "{field} exceeds 128 bytes"
                )))
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        None => Ok(None),
    }
}

fn normalize_secret_create(value: Option<String>) -> Option<String> {
    value
        .map(|secret| secret.trim().to_string())
        .filter(|secret| !secret.is_empty() && secret != REDACTED_SECRET)
}

fn normalize_secret_update(
    value: Option<Option<String>>,
    existing: Option<String>,
) -> Option<String> {
    match value {
        None => existing,
        Some(None) => None,
        Some(Some(secret)) => {
            let trimmed = secret.trim();
            if trimmed.is_empty() {
                None
            } else if trimmed == REDACTED_SECRET {
                existing
            } else {
                Some(trimmed.to_string())
            }
        }
    }
}

fn normalize_scopes_with_default(
    value: Option<&str>,
    default_scopes: &str,
    protocol: ExternalAuthProtocol,
) -> Result<String> {
    let raw = value.unwrap_or(default_scopes);
    let mut scopes = Vec::new();
    for scope in raw.split_whitespace() {
        let scope = scope.trim();
        if scope.is_empty() || scopes.iter().any(|existing| existing == scope) {
            continue;
        }
        if scope.chars().any(char::is_control) || scope.len() > 128 {
            return Err(AsterError::validation_error("invalid external auth scope"));
        }
        scopes.push(scope.to_string());
    }
    if protocol == ExternalAuthProtocol::Oidc && !scopes.iter().any(|scope| scope == "openid") {
        scopes.insert(0, "openid".to_string());
    }
    Ok(scopes.join(" "))
}

fn normalize_scopes(value: Option<&str>, protocol: ExternalAuthProtocol) -> Result<String> {
    normalize_scopes_with_default(value, DEFAULT_SCOPES, protocol)
}

fn normalize_optional_url(
    value: Option<String>,
    field: &str,
    max_len: usize,
) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.len() > max_len {
        return Err(AsterError::validation_error(format!(
            "{field} exceeds {max_len} bytes"
        )));
    }
    let parse_context = format!("invalid external auth {field}");
    let parsed =
        url::Url::parse(trimmed).map_aster_err_ctx(&parse_context, AsterError::validation_error)?;
    match parsed.scheme() {
        "https" => {}
        "http" if parsed.host_str().is_some_and(is_loopback_host) => {}
        _ => {
            return Err(AsterError::validation_error(format!(
                "external auth {field} must use HTTPS, except localhost"
            )));
        }
    }
    if parsed.fragment().is_some() {
        return Err(AsterError::validation_error(format!(
            "external auth {field} cannot include fragment"
        )));
    }
    Ok(Some(trimmed.to_string()))
}

fn normalize_icon_url_input(value: Option<String>) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.len() > EXTERNAL_AUTH_URL_MAX_LEN {
        return Err(AsterError::validation_error(format!(
            "icon_url exceeds {EXTERNAL_AUTH_URL_MAX_LEN} bytes"
        )));
    }
    if trimmed.chars().any(char::is_whitespace) {
        return Err(AsterError::validation_error(
            "external auth icon_url cannot contain whitespace",
        ));
    }
    if trimmed.starts_with('/') && !trimmed.starts_with("//") {
        return Ok(Some(trimmed.to_string()));
    }
    let parsed = url::Url::parse(trimmed).map_aster_err_ctx(
        "invalid external auth icon_url",
        AsterError::validation_error,
    )?;
    match parsed.scheme() {
        "https" => {}
        "http" if parsed.host_str().is_some_and(is_loopback_host) => {}
        _ => {
            return Err(AsterError::validation_error(
                "external auth icon_url must be a root-relative path or HTTPS URL, except localhost",
            ));
        }
    }
    if parsed.fragment().is_some() {
        return Err(AsterError::validation_error(
            "external auth icon_url cannot include fragment",
        ));
    }
    Ok(Some(trimmed.to_string()))
}

fn normalize_issuer_url_input(value: Option<String>, required: bool) -> Result<Option<String>> {
    let Some(issuer) = normalize_optional_url(
        value,
        "issuer_url",
        EXTERNAL_AUTH_IDENTITY_NAMESPACE_MAX_LEN,
    )?
    else {
        if required {
            return Err(AsterError::validation_error("issuer_url is required"));
        }
        return Ok(None);
    };
    let parsed = url::Url::parse(&issuer).map_aster_err_ctx(
        "invalid external auth issuer_url",
        AsterError::validation_error,
    )?;
    if parsed.query().is_some() {
        return Err(AsterError::validation_error(
            "external auth issuer_url cannot include query or fragment",
        ));
    }
    Ok(Some(issuer.trim_end_matches('/').to_string()))
}

fn normalize_manual_endpoint_input(
    value: Option<String>,
    field: &str,
    required: bool,
    supported: bool,
) -> Result<Option<String>> {
    let endpoint = normalize_optional_url(value, field, EXTERNAL_AUTH_URL_MAX_LEN)?;
    if endpoint.is_some() && !supported {
        return Err(AsterError::validation_error(format!(
            "{field} is not supported for this external auth provider kind"
        )));
    }
    if endpoint.is_none() && required {
        return Err(AsterError::validation_error(format!("{field} is required")));
    }
    Ok(endpoint)
}

fn is_loopback_host(host: &str) -> bool {
    host == "localhost" || host == "127.0.0.1" || host == "::1"
}

fn normalize_allowed_domains(value: Option<Vec<String>>) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let mut domains = Vec::new();
    for raw in value {
        let domain = raw.trim().trim_start_matches('@').to_ascii_lowercase();
        if domain.is_empty() {
            continue;
        }
        if domain.len() > 253
            || !domain.contains('.')
            || domain
                .chars()
                .any(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.'))
        {
            return Err(AsterError::validation_error(format!(
                "invalid external auth allowed domain '{raw}'"
            )));
        }
        if !domains.contains(&domain) {
            domains.push(domain);
        }
    }
    if domains.is_empty() {
        return Ok(None);
    }
    serde_json::to_string(&domains).map(Some).map_aster_err_ctx(
        "failed to serialize external auth allowed domains",
        AsterError::internal_error,
    )
}

fn state_hash(state: &str) -> String {
    hash::sha256_hex(state.as_bytes())
}

fn token_hash(token: &str) -> String {
    hash::sha256_hex(token.as_bytes())
}

fn normalize_return_path(value: Option<&str>) -> Result<String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok("/".to_string());
    };
    if !value.starts_with('/') || value.starts_with("//") || value.contains('\\') {
        return Err(AsterError::validation_error(
            "invalid external auth return_path",
        ));
    }
    if value.len() > 2048 {
        return Err(AsterError::validation_error(
            "external auth return_path is too long",
        ));
    }
    Ok(value.to_string())
}

fn normalize_flow_token(value: &str) -> Result<String> {
    let token = value.trim();
    if token.is_empty() {
        return Err(AsterError::validation_error(
            "external auth flow_token is required",
        ));
    }
    if token.len() > 128 || token.chars().any(char::is_whitespace) {
        return Err(AsterError::validation_error(
            "invalid external auth flow_token",
        ));
    }
    Ok(token.to_string())
}

fn normalize_email_for_external_auth(value: &str) -> Result<String> {
    let email = value.trim().to_string();
    auth_service::validate_email(&email)?;
    Ok(email)
}

fn request_origin(req: &actix_web::HttpRequest) -> Result<(String, String)> {
    let conn = req.connection_info();
    let scheme = conn.scheme().to_string();
    let host = conn.host().to_string();
    if host.trim().is_empty() {
        return Err(AsterError::config_error(
            "cannot build external auth redirect URI without request host",
        ));
    }
    Ok((scheme, host))
}

fn callback_path(provider_kind: ExternalAuthProviderKind, provider_key: &str) -> String {
    format!(
        "/api/v1/auth/external-auth/{}/{provider_key}/callback",
        provider_kind.as_str()
    )
}

pub fn callback_redirect_uri(
    state: &PrimaryAppState,
    req: &actix_web::HttpRequest,
    provider_kind: ExternalAuthProviderKind,
    provider_key: &str,
) -> Result<String> {
    let (scheme, host) = request_origin(req)?;
    let path = callback_path(provider_kind, provider_key);
    let uri = site_url::public_app_url_for_request(&state.runtime_config, &path, &scheme, &host)
        .unwrap_or_else(|| format!("{scheme}://{host}{path}"));
    if uri.starts_with('/') {
        return Err(AsterError::config_error(
            "external auth callback redirect URI must be absolute; configure public_site_url",
        ));
    }
    Ok(uri)
}

fn email_domain_allowed(provider: &external_auth_provider::Model, email: &str) -> Result<bool> {
    let domains = parse_allowed_domains(provider.allowed_domains.as_deref())?;
    if domains.is_empty() {
        return Ok(true);
    }
    let Some((_, domain)) = email.rsplit_once('@') else {
        return Ok(false);
    };
    let domain = domain.to_ascii_lowercase();
    Ok(domains.iter().any(|allowed| allowed == &domain))
}

fn require_email_if_configured(
    provider: &external_auth_provider::Model,
    claims: &ExternalAuthUserClaims,
) -> Result<()> {
    if !provider.require_email_verified {
        return Ok(());
    }
    if claims.email.as_deref().is_none_or(str::is_empty) {
        return Err(AsterError::auth_forbidden(
            "external auth provider requires a verified email but no email claim was returned",
        ));
    }
    if !claims.email_verified {
        return Err(AsterError::auth_forbidden(
            "external auth provider requires verified email",
        ));
    }
    Ok(())
}

fn random_internal_password() -> String {
    let mut bytes = [0_u8; EXTERNAL_AUTH_USER_PASSWORD_BYTES];
    let mut rng = rand::rng();
    rand::RngExt::fill(&mut rng, &mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn sanitize_username_piece(value: &str) -> String {
    value
        .chars()
        .filter_map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                Some(c.to_ascii_lowercase())
            } else if c == '.' || c == ' ' {
                Some('-')
            } else {
                None
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

async fn unique_username<C: sea_orm::ConnectionTrait>(
    db: &C,
    claims: &ExternalAuthUserClaims,
) -> Result<String> {
    let mut base = claims
        .preferred_username
        .as_deref()
        .map(sanitize_username_piece)
        .filter(|value| value.len() >= USERNAME_MIN_LEN)
        .or_else(|| {
            claims
                .email
                .as_deref()
                .and_then(|email| email.split('@').next())
                .map(sanitize_username_piece)
                .filter(|value| value.len() >= USERNAME_MIN_LEN)
        })
        .unwrap_or_else(|| format!("oidc{}", &hash::sha256_hex(claims.subject.as_bytes())[..8]));

    if base.len() > USERNAME_MAX_LEN {
        base.truncate(USERNAME_MAX_LEN);
        base = base.trim_matches('-').to_string();
    }
    while base.len() < USERNAME_MIN_LEN {
        base.push('0');
    }

    if user_repo::find_by_username(db, &base).await?.is_none() {
        return Ok(base);
    }

    let stem_max = USERNAME_MAX_LEN.saturating_sub(5);
    let mut stem = base;
    if stem.len() > stem_max {
        stem.truncate(stem_max);
        stem = stem.trim_matches('-').to_string();
    }
    if stem.len() < USERNAME_MIN_LEN {
        stem = "oidc".to_string();
    }
    for index in 1..10_000 {
        let candidate = format!("{stem}-{index}");
        if candidate.len() > USERNAME_MAX_LEN {
            continue;
        }
        if user_repo::find_by_username(db, &candidate).await?.is_none() {
            return Ok(candidate);
        }
    }
    Err(AsterError::validation_error(
        "failed to allocate unique username for external auth user",
    ))
}

fn claims_with_verified_local_email(
    flow: &external_auth_email_verification_flow::Model,
    email: &str,
) -> ExternalAuthUserClaims {
    ExternalAuthUserClaims {
        identity_namespace: flow.identity_namespace.clone(),
        subject: flow.subject.clone(),
        email: Some(email.to_string()),
        email_verified: true,
        display_name: flow.display_name_snapshot.clone(),
        preferred_username: flow.preferred_username_snapshot.clone(),
    }
}

fn claims_without_provider_email(
    flow: &external_auth_email_verification_flow::Model,
) -> ExternalAuthUserClaims {
    ExternalAuthUserClaims {
        identity_namespace: flow.identity_namespace.clone(),
        subject: flow.subject.clone(),
        email: None,
        email_verified: false,
        display_name: flow.display_name_snapshot.clone(),
        preferred_username: flow.preferred_username_snapshot.clone(),
    }
}

async fn create_identity_for_claims<C: sea_orm::ConnectionTrait>(
    db: &C,
    user_id: i64,
    provider: &external_auth_provider::Model,
    claims: &ExternalAuthUserClaims,
    now: chrono::DateTime<Utc>,
) -> Result<external_auth_identity::Model> {
    external_auth_identity_repo::create_identity(
        db,
        external_auth_identity_repo::CreateExternalAuthIdentityInput {
            user_id,
            provider_id: provider.id,
            identity_namespace: claims.identity_namespace.clone(),
            subject: claims.subject.clone(),
            email_snapshot: claims.email.clone(),
            display_name_snapshot: claims.display_name.clone(),
            now,
        },
    )
    .await
}

fn external_auth_provider_config(
    provider: &external_auth_provider::Model,
) -> ExternalAuthProviderConfig {
    ExternalAuthProviderConfig::from_provider(provider)
}

fn external_auth_provider_config_from_test_params(
    input: ExternalAuthProviderTestParamsInput,
) -> Result<ExternalAuthProviderConfig> {
    let driver = registry::default_registry().get_driver(input.provider_kind)?;
    let descriptor = driver.descriptor();
    if descriptor.kind != input.provider_kind {
        return Err(AsterError::config_error(format!(
            "external auth provider driver '{}' returned descriptor for '{}'",
            input.provider_kind.as_str(),
            descriptor.kind.as_str()
        )));
    }
    Ok(ExternalAuthProviderConfig {
        id: 0,
        key: "draft".to_string(),
        provider_kind: input.provider_kind,
        protocol: descriptor.protocol,
        issuer_url: normalize_issuer_url_input(input.issuer_url, descriptor.issuer_url_required)?,
        authorization_url: normalize_manual_endpoint_input(
            input.authorization_url,
            "authorization_url",
            descriptor.authorization_url_required,
            descriptor.manual_endpoint_configuration_supported,
        )?,
        token_url: normalize_manual_endpoint_input(
            input.token_url,
            "token_url",
            descriptor.token_url_required,
            descriptor.manual_endpoint_configuration_supported,
        )?,
        userinfo_url: normalize_manual_endpoint_input(
            input.userinfo_url,
            "userinfo_url",
            descriptor.userinfo_url_required,
            descriptor.manual_endpoint_configuration_supported,
        )?,
        client_id: normalize_required(&input.client_id, "client_id", 512)?,
        client_secret: normalize_secret_create(input.client_secret),
        scopes: normalize_scopes_with_default(
            input.scopes.as_deref(),
            descriptor.default_scopes,
            descriptor.protocol,
        )?,
        subject_claim: None,
        username_claim: None,
        display_name_claim: None,
        email_claim: None,
        email_verified_claim: None,
        groups_claim: None,
        avatar_url_claim: None,
    })
}

fn map_driver_test_result(
    result: crate::external_auth::ExternalAuthProviderTestResult,
) -> ExternalAuthProviderTestResult {
    ExternalAuthProviderTestResult {
        provider: result.provider,
        issuer: result.issuer,
        authorization_endpoint: result.authorization_endpoint,
        token_endpoint: result.token_endpoint,
        userinfo_endpoint: result.userinfo_endpoint,
        jwks_key_count: result.jwks_key_count,
        checks: result
            .checks
            .into_iter()
            .map(|check| ExternalAuthProviderTestCheck {
                name: check.name,
                success: check.success,
                message: check.message,
            })
            .collect(),
    }
}

async fn link_external_auth_identity_to_authenticated_user<C: sea_orm::ConnectionTrait>(
    db: &C,
    provider: &external_auth_provider::Model,
    claims: &ExternalAuthUserClaims,
    user: user::Model,
    now: chrono::DateTime<Utc>,
) -> Result<ResolvedExternalAuthUser> {
    if let Some(identity) = external_auth_identity_repo::find_by_identity_namespace_subject(
        db,
        &claims.identity_namespace,
        &claims.subject,
    )
    .await?
    {
        if identity.user_id != user.id {
            return Err(AsterError::auth_forbidden(
                "external auth identity is already linked to another user",
            ));
        }
        external_auth_identity_repo::touch_login(
            db,
            identity.id,
            claims.email.as_deref(),
            claims.display_name.as_deref(),
            now,
        )
        .await?;
        return Ok(ResolvedExternalAuthUser {
            user,
            linked: false,
            auto_provisioned: false,
        });
    }

    create_identity_for_claims(db, user.id, provider, claims, now).await?;
    Ok(ResolvedExternalAuthUser {
        user,
        linked: true,
        auto_provisioned: false,
    })
}

async fn create_external_auth_user_and_identity(
    state: &PrimaryAppState,
    provider: &external_auth_provider::Model,
    claims: &ExternalAuthUserClaims,
    now: chrono::DateTime<Utc>,
) -> Result<ResolvedExternalAuthUser> {
    let auth_policy = RuntimeAuthPolicy::from_runtime_config(&state.runtime_config);
    if !auth_policy.allow_user_registration {
        return Err(AsterError::auth_forbidden(
            "new user registration is disabled",
        ));
    }

    let email = claims.email.as_deref().ok_or_else(|| {
        AsterError::auth_forbidden("external auth auto provisioning requires an email claim")
    })?;
    if (provider.require_email_verified || provider.auto_link_verified_email_enabled)
        && !claims.email_verified
    {
        return Err(AsterError::auth_forbidden(
            "external auth auto provisioning requires verified email",
        ));
    }
    if !email_domain_allowed(provider, email)? {
        return Err(AsterError::auth_forbidden(
            "external auth email domain is not allowed for this provider",
        ));
    }

    let txn = crate::db::transaction::begin(&state.db).await?;
    let result = async {
        if let Some(existing) = user_repo::find_by_email(&txn, email).await? {
            return Err(AsterError::validation_error(format!(
                "user email '{}' already exists but automatic email linking is disabled",
                existing.email
            )));
        }
        let username = unique_username(&txn, claims).await?;
        let password = random_internal_password();
        let user = auth_service::shared::create_user_with_role(
            &txn,
            state,
            auth_service::shared::CreateUserWithRoleInput {
                username: &username,
                email,
                password: &password,
                role: UserRole::User,
                status: UserStatus::Active,
                email_verified_at: claims.email_verified.then_some(now),
            },
        )
        .await?;
        create_identity_for_claims(&txn, user.id, provider, claims, now).await?;
        Ok(user)
    }
    .await;

    match result {
        Ok(user) => {
            crate::db::transaction::commit(txn).await?;
            Ok(ResolvedExternalAuthUser {
                user,
                linked: true,
                auto_provisioned: true,
            })
        }
        Err(err) => Err(err),
    }
}

async fn create_external_auth_user_and_identity_in_connection<C: sea_orm::ConnectionTrait>(
    db: &C,
    state: &PrimaryAppState,
    provider: &external_auth_provider::Model,
    claims: &ExternalAuthUserClaims,
    now: chrono::DateTime<Utc>,
) -> Result<user::Model> {
    let auth_policy = RuntimeAuthPolicy::from_runtime_config(&state.runtime_config);
    if !auth_policy.allow_user_registration {
        return Err(AsterError::auth_forbidden(
            "new user registration is disabled",
        ));
    }

    let email = claims.email.as_deref().ok_or_else(|| {
        AsterError::auth_forbidden("external auth auto provisioning requires an email claim")
    })?;
    if !email_domain_allowed(provider, email)? {
        return Err(AsterError::auth_forbidden(
            "external auth email domain is not allowed for this provider",
        ));
    }
    if user_repo::find_by_email(db, email).await?.is_some() {
        return Err(AsterError::validation_error(
            "user email already exists but automatic email linking is disabled",
        ));
    }

    let username = unique_username(db, claims).await?;
    let password = random_internal_password();
    let user = auth_service::shared::create_user_with_role(
        db,
        state,
        auth_service::shared::CreateUserWithRoleInput {
            username: &username,
            email,
            password: &password,
            role: UserRole::User,
            status: UserStatus::Active,
            email_verified_at: Some(now),
        },
    )
    .await?;
    create_identity_for_claims(db, user.id, provider, claims, now).await?;
    Ok(user)
}

async fn resolve_external_auth_user_with_verified_email<C: sea_orm::ConnectionTrait>(
    db: &C,
    state: &PrimaryAppState,
    provider: &external_auth_provider::Model,
    claims: &ExternalAuthUserClaims,
    now: chrono::DateTime<Utc>,
) -> Result<ResolvedExternalAuthUser> {
    let email = claims.email.as_deref().ok_or_else(|| {
        AsterError::auth_forbidden("external auth email verification requires an email")
    })?;
    if !email_domain_allowed(provider, email)? {
        return Err(AsterError::auth_forbidden(
            "external auth email domain is not allowed for this provider",
        ));
    }

    if let Some(identity) = external_auth_identity_repo::find_by_identity_namespace_subject(
        db,
        &claims.identity_namespace,
        &claims.subject,
    )
    .await?
    {
        external_auth_identity_repo::touch_login(
            db,
            identity.id,
            claims.email.as_deref(),
            claims.display_name.as_deref(),
            now,
        )
        .await?;
        let user = user_repo::find_by_id(db, identity.user_id).await?;
        if !user.status.is_active() {
            return Err(AsterError::auth_forbidden("account is disabled"));
        }
        return Ok(ResolvedExternalAuthUser {
            user,
            linked: false,
            auto_provisioned: false,
        });
    }

    if let Some(user) = user_repo::find_by_email(db, email).await? {
        if !user.status.is_active() {
            return Err(AsterError::auth_forbidden("account is disabled"));
        }
        if user.email_verified_at.is_none() {
            return Err(AsterError::auth_forbidden(
                "local account email is not verified",
            ));
        }
        create_identity_for_claims(db, user.id, provider, claims, now).await?;
        return Ok(ResolvedExternalAuthUser {
            user,
            linked: true,
            auto_provisioned: false,
        });
    }

    let user =
        create_external_auth_user_and_identity_in_connection(db, state, provider, claims, now)
            .await?;
    Ok(ResolvedExternalAuthUser {
        user,
        linked: true,
        auto_provisioned: true,
    })
}

async fn resolve_external_auth_user(
    state: &PrimaryAppState,
    provider: &external_auth_provider::Model,
    claims: &ExternalAuthUserClaims,
) -> Result<Option<ResolvedExternalAuthUser>> {
    let now = Utc::now();
    if let Some(identity) = external_auth_identity_repo::find_by_identity_namespace_subject(
        &state.db,
        &claims.identity_namespace,
        &claims.subject,
    )
    .await?
    {
        external_auth_identity_repo::touch_login(
            &state.db,
            identity.id,
            claims.email.as_deref(),
            claims.display_name.as_deref(),
            now,
        )
        .await?;
        let user = user_repo::find_by_id(&state.db, identity.user_id).await?;
        if !user.status.is_active() {
            return Err(AsterError::auth_forbidden("account is disabled"));
        }
        return Ok(Some(ResolvedExternalAuthUser {
            user,
            linked: false,
            auto_provisioned: false,
        }));
    }

    require_email_if_configured(provider, claims)?;
    if let Some(email) = claims.email.as_deref()
        && !email_domain_allowed(provider, email)?
    {
        return Err(AsterError::auth_forbidden(
            "external auth email domain is not allowed for this provider",
        ));
    }

    if provider.auto_link_verified_email_enabled
        && claims.email_verified
        && let Some(email) = claims.email.as_deref()
        && let Some(user) = user_repo::find_by_email(&state.db, email).await?
    {
        if !user.status.is_active() {
            return Err(AsterError::auth_forbidden("account is disabled"));
        }
        create_identity_for_claims(&state.db, user.id, provider, claims, now).await?;
        return Ok(Some(ResolvedExternalAuthUser {
            user,
            linked: true,
            auto_provisioned: false,
        }));
    }

    if provider.auto_provision_enabled {
        let auth_policy = RuntimeAuthPolicy::from_runtime_config(&state.runtime_config);
        let Some(email) = claims.email.as_deref().filter(|email| !email.is_empty()) else {
            return Ok(None);
        };
        if (provider.require_email_verified || provider.auto_link_verified_email_enabled)
            && !claims.email_verified
        {
            return Ok(None);
        }
        if !auth_policy.allow_user_registration {
            return Ok(None);
        }
        if user_repo::find_by_email(&state.db, email).await?.is_some() {
            return Ok(None);
        }
        return create_external_auth_user_and_identity(state, provider, claims, now)
            .await
            .map(Some);
    }

    Ok(None)
}

fn external_auth_claims_missing_email(claims: &ExternalAuthUserClaims) -> bool {
    claims.email.as_deref().is_none_or(str::is_empty)
}

fn format_mail_duration_seconds(total_secs: i64) -> String {
    let total_secs = total_secs.max(1);
    let (value, unit) = if total_secs >= 86_400 && total_secs % 86_400 == 0 {
        (total_secs / 86_400, "day")
    } else if total_secs >= 3_600 && total_secs % 3_600 == 0 {
        (total_secs / 3_600, "hour")
    } else if total_secs >= 60 {
        ((total_secs + 59) / 60, "minute")
    } else {
        (total_secs, "second")
    };
    let suffix = if value == 1 { "" } else { "s" };
    format!("{value} {unit}{suffix}")
}

async fn resolve_existing_external_auth_identity<C: sea_orm::ConnectionTrait>(
    db: &C,
    claims: &ExternalAuthUserClaims,
    now: chrono::DateTime<Utc>,
) -> Result<Option<ResolvedExternalAuthUser>> {
    let Some(identity) = external_auth_identity_repo::find_by_identity_namespace_subject(
        db,
        &claims.identity_namespace,
        &claims.subject,
    )
    .await?
    else {
        return Ok(None);
    };

    external_auth_identity_repo::touch_login(
        db,
        identity.id,
        claims.email.as_deref(),
        claims.display_name.as_deref(),
        now,
    )
    .await?;
    let user = user_repo::find_by_id(db, identity.user_id).await?;
    if !user.status.is_active() {
        return Err(AsterError::auth_forbidden("account is disabled"));
    }
    Ok(Some(ResolvedExternalAuthUser {
        user,
        linked: false,
        auto_provisioned: false,
    }))
}

async fn create_pending_email_verification_flow(
    state: &PrimaryAppState,
    provider: &external_auth_provider::Model,
    claims: &ExternalAuthUserClaims,
    return_path: Option<String>,
) -> Result<PendingExternalAuthEmailVerification> {
    let flow_token = format!("oev_{}", crate::utils::id::new_short_token());
    let now = Utc::now();
    let ttl = u64_to_i64(
        EMAIL_VERIFICATION_FLOW_TTL_SECS,
        "external auth email verification flow ttl",
    )?;
    external_auth_email_verification_flow_repo::create(
        &state.db,
        external_auth_email_verification_flow::ActiveModel {
            provider_id: Set(provider.id),
            identity_namespace: Set(claims.identity_namespace.clone()),
            subject: Set(claims.subject.clone()),
            target_email: Set(None),
            display_name_snapshot: Set(claims.display_name.clone()),
            preferred_username_snapshot: Set(claims.preferred_username.clone()),
            return_path: Set(return_path.clone()),
            flow_token_hash: Set(token_hash(&flow_token)),
            verification_token_hash: Set(None),
            email_requested_at: Set(None),
            created_at: Set(now),
            expires_at: Set(now + Duration::seconds(ttl)),
            consumed_at: Set(None),
            ..Default::default()
        },
    )
    .await?;

    Ok(PendingExternalAuthEmailVerification {
        flow_token,
        return_path: return_path.unwrap_or_else(|| "/".to_string()),
    })
}

pub async fn list_public_providers(
    state: &PrimaryAppState,
) -> Result<Vec<ExternalAuthPublicProvider>> {
    Ok(external_auth_provider_repo::find_enabled(&state.db)
        .await?
        .into_iter()
        .filter(|provider| registry::default_registry().contains(provider.provider_kind))
        .map(provider_to_public)
        .collect())
}

pub async fn list_public_providers_by_kind(
    state: &PrimaryAppState,
    provider_kind: ExternalAuthProviderKind,
) -> Result<Vec<ExternalAuthPublicProvider>> {
    Ok(
        external_auth_provider_repo::find_enabled_by_kind(&state.db, provider_kind)
            .await?
            .into_iter()
            .map(provider_to_public)
            .collect(),
    )
}

pub async fn start_login(
    state: &PrimaryAppState,
    req: &actix_web::HttpRequest,
    provider_kind: ExternalAuthProviderKind,
    provider_key: &str,
    return_path: Option<&str>,
) -> Result<ExternalAuthStartLoginResponse> {
    let provider_key = normalize_key(provider_key)?;
    let provider =
        external_auth_provider_repo::find_by_kind_key(&state.db, provider_kind, &provider_key)
            .await?
            .ok_or_else(|| {
                AsterError::record_not_found(format!(
                    "external auth provider '{}:{provider_key}'",
                    provider_kind.as_str()
                ))
            })?;
    if !provider.enabled {
        return Err(AsterError::auth_forbidden(
            "external auth provider is disabled",
        ));
    }

    let return_path = normalize_return_path(return_path)?;
    let redirect_uri = callback_redirect_uri(state, req, provider.provider_kind, &provider.key)?;
    let auth_start = registry::default_registry()
        .get_driver(provider.provider_kind)?
        .start_authorization(&external_auth_provider_config(&provider), &redirect_uri)
        .await?;
    let now = Utc::now();
    let ttl = u64_to_i64(FLOW_TTL_SECS, "external auth login flow ttl")?;
    let flow = external_auth_login_flow::ActiveModel {
        provider_id: Set(provider.id),
        state_hash: Set(state_hash(&auth_start.state)),
        nonce: Set(auth_start.nonce),
        pkce_verifier: Set(auth_start.pkce_verifier),
        redirect_uri: Set(redirect_uri),
        return_path: Set(Some(return_path)),
        created_at: Set(now),
        expires_at: Set(now + Duration::seconds(ttl)),
        consumed_at: Set(None),
        ..Default::default()
    };
    external_auth_login_flow_repo::create(&state.db, flow).await?;

    Ok(ExternalAuthStartLoginResponse {
        authorization_url: auth_start.authorization_url,
    })
}

pub async fn finish_callback(
    state: &PrimaryAppState,
    provider_kind: ExternalAuthProviderKind,
    provider_key: &str,
    query: &ExternalAuthCallbackQuery,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<ExternalAuthCallbackOutcome> {
    if let Some(error) = query.error.as_deref() {
        let description = query
            .error_description
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(error);
        return Err(AsterError::auth_invalid_credentials(format!(
            "external auth provider returned error: {description}"
        )));
    }
    let code = query.code.as_deref().ok_or_else(|| {
        AsterError::auth_invalid_credentials("external auth callback missing code")
    })?;
    let state_value = query.state.as_deref().ok_or_else(|| {
        AsterError::auth_invalid_credentials("external auth callback missing state")
    })?;

    let flow = external_auth_login_flow_repo::consume_by_state_hash(
        &state.db,
        &state_hash(state_value),
        Utc::now(),
    )
    .await?
    .ok_or_else(|| {
        AsterError::auth_invalid_credentials("external auth state is invalid or expired")
    })?;
    let provider = external_auth_provider_repo::find_by_id(&state.db, flow.provider_id).await?;
    if provider.provider_kind != provider_kind {
        return Err(AsterError::auth_invalid_credentials(
            "external auth callback provider kind does not match login flow",
        ));
    }
    let expected_key = normalize_key(provider_key)?;
    if provider.key != expected_key {
        return Err(AsterError::auth_invalid_credentials(
            "external auth callback provider does not match login flow",
        ));
    }
    if !provider.enabled {
        return Err(AsterError::auth_forbidden(
            "external auth provider is disabled",
        ));
    }

    let user_claims = registry::default_registry()
        .get_driver(provider.provider_kind)?
        .exchange_callback(
            &external_auth_provider_config(&provider),
            ExternalAuthCallback {
                code: code.to_string(),
                nonce: flow.nonce,
                pkce_verifier: flow.pkce_verifier,
                redirect_uri: flow.redirect_uri.clone(),
            },
        )
        .await?;

    if external_auth_claims_missing_email(&user_claims) {
        if let Some(resolved) =
            resolve_existing_external_auth_identity(&state.db, &user_claims, Utc::now()).await?
        {
            let (access_token, refresh_token) =
                auth_service::issue_tokens_for_user(state, &resolved.user, ip_address, user_agent)
                    .await?;
            return Ok(ExternalAuthCallbackOutcome::Login(
                ExternalAuthCallbackResult {
                    login: LoginResult {
                        access_token,
                        refresh_token,
                        user_id: resolved.user.id,
                    },
                    return_path: flow.return_path.unwrap_or_else(|| "/".to_string()),
                    provider_key: provider.key,
                    issuer: user_claims.identity_namespace,
                    subject: user_claims.subject,
                    linked: resolved.linked,
                    auto_provisioned: resolved.auto_provisioned,
                },
            ));
        }
        let pending = create_pending_email_verification_flow(
            state,
            &provider,
            &user_claims,
            flow.return_path.clone(),
        )
        .await?;
        return Ok(ExternalAuthCallbackOutcome::EmailVerificationRequired(
            pending,
        ));
    }

    let resolved = match resolve_external_auth_user(state, &provider, &user_claims).await? {
        Some(resolved) => resolved,
        None => {
            let pending = create_pending_email_verification_flow(
                state,
                &provider,
                &user_claims,
                flow.return_path.clone(),
            )
            .await?;
            return Ok(ExternalAuthCallbackOutcome::EmailVerificationRequired(
                pending,
            ));
        }
    };
    let (access_token, refresh_token) =
        auth_service::issue_tokens_for_user(state, &resolved.user, ip_address, user_agent).await?;

    Ok(ExternalAuthCallbackOutcome::Login(
        ExternalAuthCallbackResult {
            login: LoginResult {
                access_token,
                refresh_token,
                user_id: resolved.user.id,
            },
            return_path: flow.return_path.unwrap_or_else(|| "/".to_string()),
            provider_key: provider.key,
            issuer: user_claims.identity_namespace,
            subject: user_claims.subject,
            linked: resolved.linked,
            auto_provisioned: resolved.auto_provisioned,
        },
    ))
}

pub async fn start_email_verification(
    state: &PrimaryAppState,
    input: ExternalAuthEmailVerificationStartRequest,
) -> Result<ExternalAuthEmailVerificationStartResponse> {
    let flow_token = normalize_flow_token(&input.flow_token)?;
    let email = normalize_email_for_external_auth(&input.email)?;
    let now = Utc::now();
    let flow = external_auth_email_verification_flow_repo::find_active_by_flow_token_hash(
        &state.db,
        &token_hash(&flow_token),
        now,
    )
    .await?
    .ok_or_else(|| {
        AsterError::contact_verification_invalid("external auth email verification flow is invalid")
    })?;
    if flow.verification_token_hash.is_some() {
        return Err(AsterError::contact_verification_invalid(
            "external auth email verification request has already been started",
        ));
    }

    let provider = external_auth_provider_repo::find_by_id(&state.db, flow.provider_id).await?;
    if !provider.enabled {
        return Err(AsterError::auth_forbidden(
            "external auth provider is disabled",
        ));
    }
    if !email_domain_allowed(&provider, &email)? {
        return Err(AsterError::auth_forbidden(
            "external auth email domain is not allowed for this provider",
        ));
    }

    match user_repo::find_by_email(&state.db, &email).await? {
        Some(user) => {
            if !user.status.is_active() {
                return Err(AsterError::auth_forbidden("account is disabled"));
            }
            if user.email_verified_at.is_none() {
                return Err(AsterError::auth_forbidden(
                    "local account email is not verified",
                ));
            }
        }
        None => {
            let auth_policy = RuntimeAuthPolicy::from_runtime_config(&state.runtime_config);
            if !auth_policy.allow_user_registration {
                return Err(AsterError::auth_forbidden(
                    "new user registration is disabled",
                ));
            }
        }
    }

    let verification_token = mail_service::build_verification_token();
    let verification_token_hash = token_hash(&verification_token);
    let provider_name = provider.display_name.clone();
    let site_name = branding::title_or_default(&state.runtime_config);
    let expires_in = format_mail_duration_seconds((flow.expires_at - now).num_seconds());
    let txn = crate::db::transaction::begin(&state.db).await?;
    let result = async {
        external_auth_email_verification_flow_repo::update_email_request(
            &txn,
            flow,
            &email,
            &verification_token_hash,
            now,
        )
        .await?
        .then_some(())
        .ok_or_else(|| {
            AsterError::contact_verification_invalid(
                "external auth email verification request has already been started",
            )
        })?;
        mail_outbox_service::enqueue(
            &txn,
            &email,
            None,
            MailTemplatePayload::external_auth_email_verification(
                &email,
                &verification_token,
                &provider_name,
                &site_name,
                &expires_in,
            ),
        )
        .await?;
        Ok(())
    }
    .await;

    match result {
        Ok(()) => {
            crate::db::transaction::commit(txn).await?;
            Ok(ExternalAuthEmailVerificationStartResponse {
                message: "external auth email verification email sent".to_string(),
            })
        }
        Err(error) => Err(error),
    }
}

pub async fn link_with_password(
    state: &PrimaryAppState,
    input: ExternalAuthPasswordLinkRequest,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<ExternalAuthPasswordLinkResult> {
    let flow_token = normalize_flow_token(&input.flow_token)?;
    let identifier = input.identifier.trim();
    if identifier.is_empty() {
        return Err(AsterError::validation_error("identifier is required"));
    }
    if input.password.is_empty() {
        return Err(AsterError::validation_error("password is required"));
    }

    let now = Utc::now();
    let flow = external_auth_email_verification_flow_repo::find_active_by_flow_token_hash(
        &state.db,
        &token_hash(&flow_token),
        now,
    )
    .await?
    .ok_or_else(|| {
        AsterError::contact_verification_invalid("external auth email verification flow is invalid")
    })?;
    let provider = external_auth_provider_repo::find_by_id(&state.db, flow.provider_id).await?;
    if !provider.enabled {
        return Err(AsterError::auth_forbidden(
            "external auth provider is disabled",
        ));
    }

    let Some(user) = auth_service::shared::find_user_by_identifier(&state.db, identifier).await?
    else {
        return Err(AsterError::auth_invalid_credentials("invalid credentials"));
    };
    if !user.status.is_active() {
        return Err(AsterError::auth_forbidden("account is disabled"));
    }
    if !auth_service::is_email_verified(&user) {
        return Err(AsterError::auth_pending_activation(
            "account pending activation",
        ));
    }
    if !hash::verify_password(&input.password, &user.password_hash)? {
        return Err(AsterError::auth_invalid_credentials("invalid credentials"));
    }

    let claims = claims_without_provider_email(&flow);
    let txn = crate::db::transaction::begin(&state.db).await?;
    let result = async {
        let consumed =
            external_auth_email_verification_flow_repo::mark_consumed_if_unused(&txn, flow.id, now)
                .await?;
        if !consumed {
            return Err(AsterError::contact_verification_invalid(
                "external auth login flow has already been used",
            ));
        }
        link_external_auth_identity_to_authenticated_user(&txn, &provider, &claims, user, now).await
    }
    .await;

    let resolved = match result {
        Ok(resolved) => {
            crate::db::transaction::commit(txn).await?;
            resolved
        }
        Err(error) => return Err(error),
    };
    let (access_token, refresh_token) =
        auth_service::issue_tokens_for_user(state, &resolved.user, ip_address, user_agent).await?;

    Ok(ExternalAuthPasswordLinkResult {
        login: LoginResult {
            access_token,
            refresh_token,
            user_id: resolved.user.id,
        },
        return_path: flow.return_path.unwrap_or_else(|| "/".to_string()),
        provider_key: provider.key,
        issuer: claims.identity_namespace,
        subject: claims.subject,
        linked: resolved.linked,
        auto_provisioned: resolved.auto_provisioned,
    })
}

pub async fn confirm_email_verification(
    state: &PrimaryAppState,
    token: &str,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<ExternalAuthEmailVerificationConfirmResult> {
    let token = token.trim();
    if token.is_empty() {
        return Err(AsterError::contact_verification_invalid(
            "external auth email verification token is missing",
        ));
    }
    let now = Utc::now();
    let flow = external_auth_email_verification_flow_repo::find_active_by_verification_token_hash(
        &state.db,
        &token_hash(token),
        now,
    )
    .await?
    .ok_or_else(|| {
        AsterError::contact_verification_invalid("external auth email verification link is invalid")
    })?;
    let email = flow.target_email.clone().ok_or_else(|| {
        AsterError::contact_verification_invalid(
            "external auth email verification target is missing",
        )
    })?;
    let provider = external_auth_provider_repo::find_by_id(&state.db, flow.provider_id).await?;
    if !provider.enabled {
        return Err(AsterError::auth_forbidden(
            "external auth provider is disabled",
        ));
    }

    let claims = claims_with_verified_local_email(&flow, &email);
    let txn = crate::db::transaction::begin(&state.db).await?;
    let result = async {
        let consumed =
            external_auth_email_verification_flow_repo::mark_consumed_if_unused(&txn, flow.id, now)
                .await?;
        if !consumed {
            return Err(AsterError::contact_verification_invalid(
                "external auth email verification link has already been used",
            ));
        }
        resolve_external_auth_user_with_verified_email(&txn, state, &provider, &claims, now).await
    }
    .await;

    let resolved = match result {
        Ok(resolved) => {
            crate::db::transaction::commit(txn).await?;
            resolved
        }
        Err(error) => return Err(error),
    };
    let (access_token, refresh_token) =
        auth_service::issue_tokens_for_user(state, &resolved.user, ip_address, user_agent).await?;

    Ok(ExternalAuthEmailVerificationConfirmResult {
        login: LoginResult {
            access_token,
            refresh_token,
            user_id: resolved.user.id,
        },
        return_path: flow.return_path.unwrap_or_else(|| "/".to_string()),
        provider_key: provider.key,
        issuer: claims.identity_namespace,
        subject: claims.subject,
        linked: resolved.linked,
        auto_provisioned: resolved.auto_provisioned,
    })
}

pub async fn list_links(
    state: &PrimaryAppState,
    user_id: i64,
) -> Result<Vec<ExternalAuthLinkInfo>> {
    let identities = external_auth_identity_repo::list_for_user(&state.db, user_id).await?;
    let providers = external_auth_provider_repo::find_all(&state.db).await?;
    let provider_lookup = providers
        .into_iter()
        .map(|provider| (provider.id, provider))
        .collect::<std::collections::HashMap<_, _>>();
    Ok(identities
        .into_iter()
        .filter_map(|identity| {
            let provider = provider_lookup.get(&identity.provider_id)?;
            Some(link_to_info(identity, provider))
        })
        .collect())
}

fn link_to_info(
    identity: external_auth_identity::Model,
    provider: &external_auth_provider::Model,
) -> ExternalAuthLinkInfo {
    ExternalAuthLinkInfo {
        id: identity.id,
        provider_id: identity.provider_id,
        provider_key: provider.key.clone(),
        provider_kind: provider.provider_kind,
        provider_display_name: provider.display_name.clone(),
        issuer: identity.identity_namespace,
        subject: identity.subject,
        email_snapshot: identity.email_snapshot,
        display_name_snapshot: identity.display_name_snapshot,
        created_at: identity.created_at,
        updated_at: identity.updated_at,
        last_login_at: identity.last_login_at,
    }
}

pub async fn delete_link(state: &PrimaryAppState, user_id: i64, id: i64) -> Result<bool> {
    external_auth_identity_repo::delete_for_user(&state.db, id, user_id).await
}

pub async fn list_admin_providers(
    state: &PrimaryAppState,
    limit: u64,
    offset: u64,
) -> Result<OffsetPage<AdminExternalAuthProviderInfo>> {
    let page = load_offset_page(limit, offset, 100, |limit, offset| async move {
        external_auth_provider_repo::find_paginated(
            &state.db,
            limit,
            offset,
            registry::default_registry().supported_kinds(),
        )
        .await
    })
    .await?;
    let items = page
        .items
        .into_iter()
        .map(provider_to_admin)
        .collect::<Result<Vec<_>>>()?;
    Ok(OffsetPage::new(items, page.total, page.limit, page.offset))
}

pub fn list_provider_kinds() -> Vec<ExternalAuthProviderKindInfo> {
    registry::default_registry()
        .descriptors()
        .into_iter()
        .map(descriptor_to_info)
        .collect()
}

pub async fn get_admin_provider(
    state: &PrimaryAppState,
    id: i64,
) -> Result<AdminExternalAuthProviderInfo> {
    let provider = external_auth_provider_repo::find_by_id(&state.db, id).await?;
    if !registry::default_registry().contains(provider.provider_kind) {
        return Err(AsterError::record_not_found(format!(
            "external auth provider #{id}"
        )));
    }
    provider_to_admin(provider)
}

pub async fn create_provider(
    state: &PrimaryAppState,
    input: CreateExternalAuthProviderInput,
) -> Result<AdminExternalAuthProviderInfo> {
    let driver = registry::default_registry().get_driver(input.provider_kind)?;
    let descriptor = driver.descriptor();
    if descriptor.kind != input.provider_kind {
        return Err(AsterError::config_error(format!(
            "external auth provider driver '{}' returned descriptor for '{}'",
            input.provider_kind.as_str(),
            descriptor.kind.as_str()
        )));
    }
    let key = id::new_best_effort_uuid("external auth provider key", |candidate| {
        let db = &state.db;
        let provider_kind = input.provider_kind;
        async move {
            let candidate_key = candidate.to_string();
            external_auth_provider_repo::find_by_kind_key(db, provider_kind, &candidate_key)
                .await
                .map(|provider| provider.is_some())
        }
    })
    .await?
    .to_string();
    let display_name = normalize_required(&input.display_name, "display_name", 128)?;
    let icon_url = normalize_icon_url_input(input.icon_url)?;
    let issuer_url = normalize_issuer_url_input(input.issuer_url, descriptor.issuer_url_required)?;
    let authorization_url = normalize_manual_endpoint_input(
        input.authorization_url,
        "authorization_url",
        descriptor.authorization_url_required,
        descriptor.manual_endpoint_configuration_supported,
    )?;
    let token_url = normalize_manual_endpoint_input(
        input.token_url,
        "token_url",
        descriptor.token_url_required,
        descriptor.manual_endpoint_configuration_supported,
    )?;
    let userinfo_url = normalize_manual_endpoint_input(
        input.userinfo_url,
        "userinfo_url",
        descriptor.userinfo_url_required,
        descriptor.manual_endpoint_configuration_supported,
    )?;
    let client_id = normalize_required(&input.client_id, "client_id", 512)?;
    let scopes = normalize_scopes_with_default(
        input.scopes.as_deref(),
        descriptor.default_scopes,
        descriptor.protocol,
    )?;
    let allowed_domains = normalize_allowed_domains(input.allowed_domains)?;
    let now = Utc::now();
    let model = external_auth_provider::ActiveModel {
        key: Set(key),
        display_name: Set(display_name),
        icon_url: Set(icon_url),
        provider_kind: Set(input.provider_kind),
        protocol: Set(descriptor.protocol),
        issuer_url: Set(issuer_url),
        authorization_url: Set(authorization_url),
        token_url: Set(token_url),
        userinfo_url: Set(userinfo_url),
        client_id: Set(client_id),
        client_secret: Set(normalize_secret_create(input.client_secret)),
        scopes: Set(scopes),
        enabled: Set(input.enabled.unwrap_or(true)),
        auto_provision_enabled: Set(input.auto_provision_enabled.unwrap_or(false)),
        auto_link_verified_email_enabled: Set(input
            .auto_link_verified_email_enabled
            .unwrap_or(false)),
        require_email_verified: Set(input.require_email_verified.unwrap_or(true)),
        subject_claim: Set(normalize_optional_claim(
            input.subject_claim,
            "subject_claim",
        )?),
        username_claim: Set(normalize_optional_claim(
            input.username_claim,
            "username_claim",
        )?),
        display_name_claim: Set(normalize_optional_claim(
            input.display_name_claim,
            "display_name_claim",
        )?),
        email_claim: Set(normalize_optional_claim(input.email_claim, "email_claim")?),
        email_verified_claim: Set(normalize_optional_claim(
            input.email_verified_claim,
            "email_verified_claim",
        )?),
        groups_claim: Set(normalize_optional_claim(
            input.groups_claim,
            "groups_claim",
        )?),
        avatar_url_claim: Set(normalize_optional_claim(
            input.avatar_url_claim,
            "avatar_url_claim",
        )?),
        allowed_domains: Set(allowed_domains),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    let provider = external_auth_provider_repo::create(&state.db, model).await?;
    provider_to_admin(provider)
}

pub async fn update_provider(
    state: &PrimaryAppState,
    id: i64,
    input: UpdateExternalAuthProviderInput,
) -> Result<AdminExternalAuthProviderInfo> {
    let existing = external_auth_provider_repo::find_by_id(&state.db, id).await?;
    if !registry::default_registry().contains(existing.provider_kind) {
        return Err(AsterError::record_not_found(format!(
            "external auth provider #{id}"
        )));
    }
    let descriptor = registry::default_registry()
        .get_driver(existing.provider_kind)?
        .descriptor();
    let mut active = existing.clone().into_active_model();
    if let Some(display_name) = input.display_name {
        active.display_name = Set(normalize_required(&display_name, "display_name", 128)?);
    }
    if let Some(icon_url) = input.icon_url {
        active.icon_url = Set(normalize_icon_url_input(icon_url)?);
    }
    if let Some(issuer_url) = input.issuer_url {
        active.issuer_url = Set(normalize_issuer_url_input(
            issuer_url,
            descriptor.issuer_url_required,
        )?);
    }
    if let Some(authorization_url) = input.authorization_url {
        active.authorization_url = Set(normalize_manual_endpoint_input(
            authorization_url,
            "authorization_url",
            descriptor.authorization_url_required,
            descriptor.manual_endpoint_configuration_supported,
        )?);
    }
    if let Some(token_url) = input.token_url {
        active.token_url = Set(normalize_manual_endpoint_input(
            token_url,
            "token_url",
            descriptor.token_url_required,
            descriptor.manual_endpoint_configuration_supported,
        )?);
    }
    if let Some(userinfo_url) = input.userinfo_url {
        active.userinfo_url = Set(normalize_manual_endpoint_input(
            userinfo_url,
            "userinfo_url",
            descriptor.userinfo_url_required,
            descriptor.manual_endpoint_configuration_supported,
        )?);
    }
    if let Some(client_id) = input.client_id {
        active.client_id = Set(normalize_required(&client_id, "client_id", 512)?);
    }
    active.client_secret = Set(normalize_secret_update(
        input.client_secret,
        existing.client_secret.clone(),
    ));
    if let Some(scopes) = input.scopes {
        active.scopes = Set(normalize_scopes(Some(&scopes), existing.protocol)?);
    }
    if let Some(enabled) = input.enabled {
        active.enabled = Set(enabled);
    }
    if let Some(value) = input.auto_provision_enabled {
        active.auto_provision_enabled = Set(value);
    }
    if let Some(value) = input.auto_link_verified_email_enabled {
        active.auto_link_verified_email_enabled = Set(value);
    }
    if let Some(value) = input.require_email_verified {
        active.require_email_verified = Set(value);
    }
    if let Some(value) = input.subject_claim {
        active.subject_claim = Set(normalize_optional_claim(value, "subject_claim")?);
    }
    if let Some(value) = input.username_claim {
        active.username_claim = Set(normalize_optional_claim(value, "username_claim")?);
    }
    if let Some(value) = input.display_name_claim {
        active.display_name_claim = Set(normalize_optional_claim(value, "display_name_claim")?);
    }
    if let Some(value) = input.email_claim {
        active.email_claim = Set(normalize_optional_claim(value, "email_claim")?);
    }
    if let Some(value) = input.email_verified_claim {
        active.email_verified_claim = Set(normalize_optional_claim(value, "email_verified_claim")?);
    }
    if let Some(value) = input.groups_claim {
        active.groups_claim = Set(normalize_optional_claim(value, "groups_claim")?);
    }
    if let Some(value) = input.avatar_url_claim {
        active.avatar_url_claim = Set(normalize_optional_claim(value, "avatar_url_claim")?);
    }
    if let Some(value) = input.allowed_domains {
        active.allowed_domains = Set(normalize_allowed_domains(value)?);
    }
    active.updated_at = Set(Utc::now());

    let provider = external_auth_provider_repo::update(&state.db, active).await?;
    provider_to_admin(provider)
}

pub async fn delete_provider(state: &PrimaryAppState, id: i64) -> Result<()> {
    let provider = external_auth_provider_repo::find_by_id(&state.db, id).await?;
    if !registry::default_registry().contains(provider.provider_kind) {
        return Err(AsterError::record_not_found(format!(
            "external auth provider #{id}"
        )));
    }
    external_auth_provider_repo::delete(&state.db, id).await
}

pub async fn test_provider(
    state: &PrimaryAppState,
    id: i64,
) -> Result<ExternalAuthProviderTestResult> {
    let provider = external_auth_provider_repo::find_by_id(&state.db, id).await?;
    let result = registry::default_registry()
        .get_driver(provider.provider_kind)?
        .test_provider(&external_auth_provider_config(&provider))
        .await?;
    external_auth_provider_repo::touch_updated_at(&state.db, id, Utc::now()).await?;
    Ok(map_driver_test_result(result))
}

pub async fn test_provider_params(
    _state: &PrimaryAppState,
    input: ExternalAuthProviderTestParamsInput,
) -> Result<ExternalAuthProviderTestResult> {
    let provider = external_auth_provider_config_from_test_params(input)?;
    let result = registry::default_registry()
        .get_driver(provider.provider_kind)?
        .test_provider(&provider)
        .await?;
    Ok(map_driver_test_result(result))
}

pub async fn cleanup_expired_flows(state: &PrimaryAppState) -> Result<u64> {
    let now = Utc::now();
    let login_flows = external_auth_login_flow_repo::cleanup_expired(&state.db, now).await?;
    let email_flows =
        external_auth_email_verification_flow_repo::cleanup_expired(&state.db, now).await?;
    Ok(login_flows + email_flows)
}

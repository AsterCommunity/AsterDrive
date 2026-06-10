use crate::api::api_error_code::ApiErrorCode;
use crate::config::site_url;
use crate::entities::external_auth_provider;
use crate::errors::{AsterError, MapAsterErr, Result, validation_error_with_code};
use crate::external_auth::url::{is_https_or_loopback_http, parse_url};
use crate::runtime::SharedRuntimeState;
use crate::services::auth_service;
use crate::types::{ExternalAuthProtocol, ExternalAuthProviderKind, NullablePatch};
use crate::utils::hash;

use super::{
    DEFAULT_SCOPES, EXTERNAL_AUTH_IDENTITY_NAMESPACE_MAX_LEN, EXTERNAL_AUTH_URL_MAX_LEN,
    REDACTED_SECRET,
};

pub(super) fn parse_allowed_domains(raw: Option<&str>) -> Result<Vec<String>> {
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

pub(super) fn normalize_key(value: &str) -> Result<String> {
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

pub(super) fn normalize_required(value: &str, field: &str, max_len: usize) -> Result<String> {
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

pub(super) fn normalize_optional_claim(
    value: Option<String>,
    field: &str,
) -> Result<Option<String>> {
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

pub(super) fn normalize_secret_create(value: Option<String>) -> Option<String> {
    value
        .map(|secret| secret.trim().to_string())
        .filter(|secret| !secret.is_empty() && secret != REDACTED_SECRET)
}

pub(super) fn normalize_secret_update(
    value: NullablePatch<String>,
    existing: Option<String>,
) -> Option<String> {
    match value {
        NullablePatch::Absent => existing,
        NullablePatch::Null => None,
        NullablePatch::Value(secret) => {
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

pub(super) fn normalize_scopes_with_default(
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

pub(super) fn normalize_scopes(
    value: Option<&str>,
    protocol: ExternalAuthProtocol,
) -> Result<String> {
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
    let parsed = parse_url(trimmed, &parse_context, AsterError::validation_error)?;
    if !is_https_or_loopback_http(&parsed) {
        return Err(AsterError::validation_error(format!(
            "external auth {field} must use HTTPS, except localhost"
        )));
    }
    if parsed.fragment().is_some() {
        return Err(AsterError::validation_error(format!(
            "external auth {field} cannot include fragment"
        )));
    }
    Ok(Some(trimmed.to_string()))
}

pub(super) fn normalize_icon_url_input(value: Option<String>) -> Result<Option<String>> {
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
    let parsed = parse_url(
        trimmed,
        "invalid external auth icon_url",
        AsterError::validation_error,
    )?;
    if !is_https_or_loopback_http(&parsed) {
        return Err(AsterError::validation_error(
            "external auth icon_url must be a root-relative path or HTTPS URL, except localhost",
        ));
    }
    if parsed.fragment().is_some() {
        return Err(AsterError::validation_error(
            "external auth icon_url cannot include fragment",
        ));
    }
    Ok(Some(trimmed.to_string()))
}

pub(super) fn normalize_issuer_url_input(
    value: Option<String>,
    required: bool,
) -> Result<Option<String>> {
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
    let parsed = parse_url(
        &issuer,
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

pub(super) fn normalize_manual_endpoint_input(
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

pub(super) fn normalize_allowed_domains(value: Option<Vec<String>>) -> Result<Option<String>> {
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

pub(super) fn state_hash(state: &str) -> String {
    hash::sha256_hex(state.as_bytes())
}

pub(super) fn token_hash(token: &str) -> String {
    hash::sha256_hex(token.as_bytes())
}

pub(super) fn normalize_return_path(value: Option<&str>) -> Result<String> {
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

pub(super) fn normalize_flow_token(value: &str) -> Result<String> {
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

pub(super) fn normalize_email_for_external_auth(value: &str) -> Result<String> {
    let email = value.trim().to_string();
    auth_service::validate_email(&email)?;
    Ok(email)
}

fn callback_path(provider_kind: ExternalAuthProviderKind, provider_key: &str) -> String {
    format!(
        "/api/v1/auth/external-auth/{}/{provider_key}/callback",
        provider_kind.as_str()
    )
}

pub fn callback_redirect_uri(
    state: &impl SharedRuntimeState,
    req: &actix_web::HttpRequest,
    provider_kind: ExternalAuthProviderKind,
    provider_key: &str,
) -> Result<String> {
    let conn = req.connection_info();
    let scheme = conn.scheme();
    let host = conn.host();
    let path = callback_path(provider_kind, provider_key);
    let uri = site_url::public_app_url_for_request(state.runtime_config(), &path, scheme, host)
        .ok_or_else(|| {
            validation_error_with_code(
                ApiErrorCode::ExternalAuthCallbackRedirectUriRequired,
                "cannot build external auth callback redirect URI; configure public_site_url",
            )
        })?;
    if uri.starts_with('/') {
        return Err(validation_error_with_code(
            ApiErrorCode::ExternalAuthCallbackRedirectUriRequired,
            "external auth callback redirect URI must be absolute; configure public_site_url",
        ));
    }
    Ok(uri)
}

pub(super) fn email_domain_allowed(
    provider: &external_auth_provider::Model,
    email: &str,
) -> Result<bool> {
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

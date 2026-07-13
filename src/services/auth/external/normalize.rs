use crate::api::api_error_code::ApiErrorCode;
use crate::config::site_url;
use crate::errors::{Result, validation_error_with_code};
use crate::runtime::SharedRuntimeState;
use crate::services::auth::local;
use aster_forge_api::NullablePatch;
use aster_forge_external_auth::ExternalAuthProviderKind;

use super::REDACTED_SECRET;

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

pub(super) fn normalize_email_for_external_auth(value: &str) -> Result<String> {
    let email = value.trim().to_string();
    local::validate_email(&email)?;
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

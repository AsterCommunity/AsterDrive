use chrono::{Duration, Utc};
use sea_orm::ActiveValue::Set;

use crate::db::repository::{external_auth_login_flow_repo, external_auth_provider_repo};
use crate::entities::external_auth_login_flow;
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use aster_forge_external_auth::{
    ExternalAuthCallback, ExternalAuthProviderKind, default_registry,
    normalize as external_auth_normalize,
};
use aster_forge_utils::numbers::u64_to_i64;

use super::normalize::callback_redirect_uri;
use super::providers::external_auth_provider_config;
use super::resolution::{
    external_auth_claims_missing_email, resolve_existing_external_auth_identity,
    resolve_external_auth_user,
};
use super::verification::create_pending_email_verification_flow;
use super::{
    ExternalAuthCallbackOutcome, ExternalAuthCallbackQuery, ExternalAuthCallbackResult,
    ExternalAuthPrimaryLogin, ExternalAuthStartLoginResponse, FLOW_TTL_SECS,
};

pub async fn start_login(
    state: &impl SharedRuntimeState,
    req: &actix_web::HttpRequest,
    provider_kind: ExternalAuthProviderKind,
    provider_key: &str,
    return_path: Option<&str>,
) -> Result<ExternalAuthStartLoginResponse> {
    let provider_key = external_auth_normalize::normalize_provider_key(provider_key)?;
    let provider = external_auth_provider_repo::find_by_kind_key(
        state.writer_db(),
        provider_kind,
        &provider_key,
    )
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

    let return_path = external_auth_normalize::normalize_return_path(
        return_path,
        super::EXTERNAL_AUTH_URL_MAX_LEN,
    )?;
    let redirect_uri = callback_redirect_uri(state, req, provider.provider_kind, &provider.key)?;
    let runtime_provider = external_auth_provider_config(&provider);
    let auth_start = default_registry()
        .driver_for_provider(&runtime_provider)?
        .start_authorization(&runtime_provider, &redirect_uri)
        .await?;
    let now = Utc::now();
    let ttl = u64_to_i64(FLOW_TTL_SECS, "external auth login flow ttl")?;
    let flow = external_auth_login_flow::ActiveModel {
        provider_id: Set(provider.id),
        state_hash: Set(external_auth_normalize::state_hash(&auth_start.state)),
        nonce: Set(auth_start.nonce),
        pkce_verifier: Set(auth_start.pkce_verifier),
        redirect_uri: Set(redirect_uri),
        return_path: Set(Some(return_path)),
        created_at: Set(now),
        expires_at: Set(now + Duration::seconds(ttl)),
        consumed_at: Set(None),
        ..Default::default()
    };
    external_auth_login_flow_repo::create(state.writer_db(), flow).await?;

    Ok(ExternalAuthStartLoginResponse {
        authorization_url: auth_start.authorization_url,
    })
}

pub async fn finish_callback(
    state: &impl SharedRuntimeState,
    provider_kind: ExternalAuthProviderKind,
    provider_key: &str,
    query: &ExternalAuthCallbackQuery,
    _ip_address: Option<&str>,
    _user_agent: Option<&str>,
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
        state.writer_db(),
        &external_auth_normalize::state_hash(state_value),
        Utc::now(),
    )
    .await?
    .ok_or_else(|| {
        AsterError::auth_invalid_credentials("external auth state is invalid or expired")
    })?;
    let provider =
        external_auth_provider_repo::find_by_id(state.writer_db(), flow.provider_id).await?;
    if provider.provider_kind != provider_kind {
        return Err(AsterError::auth_invalid_credentials(
            "external auth callback provider kind does not match login flow",
        ));
    }
    let expected_key = external_auth_normalize::normalize_provider_key(provider_key)?;
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

    let runtime_provider = external_auth_provider_config(&provider);
    let user_claims = default_registry()
        .driver_for_provider(&runtime_provider)?
        .exchange_callback(
            &runtime_provider,
            ExternalAuthCallback {
                code: code.to_string(),
                nonce: flow.nonce,
                pkce_verifier: flow.pkce_verifier,
                redirect_uri: flow.redirect_uri.clone(),
            },
        )
        .await?;

    if external_auth_claims_missing_email(&user_claims) {
        // Existing bindings are keyed by issuer + subject, so they may sign in
        // even when the current callback cannot provide an email snapshot.
        if let Some(resolved) =
            resolve_existing_external_auth_identity(state.writer_db(), &user_claims, Utc::now())
                .await?
        {
            return Ok(ExternalAuthCallbackOutcome::Login(
                ExternalAuthCallbackResult {
                    primary_login: ExternalAuthPrimaryLogin {
                        user: resolved.user,
                        return_path: flow.return_path.unwrap_or_else(|| "/".to_string()),
                        provider_key: provider.key,
                        issuer: user_claims.identity_namespace,
                        subject: user_claims.subject,
                        linked: resolved.linked,
                        auto_provisioned: resolved.auto_provisioned,
                    },
                },
            ));
        }
        if provider.provider_kind == ExternalAuthProviderKind::GitHub
            && provider.require_email_verified
        {
            return Err(AsterError::auth_forbidden(
                "GitHub provider requires a verified primary email",
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

    Ok(ExternalAuthCallbackOutcome::Login(
        ExternalAuthCallbackResult {
            primary_login: ExternalAuthPrimaryLogin {
                user: resolved.user,
                return_path: flow.return_path.unwrap_or_else(|| "/".to_string()),
                provider_key: provider.key,
                issuer: user_claims.identity_namespace,
                subject: user_claims.subject,
                linked: resolved.linked,
                auto_provisioned: resolved.auto_provisioned,
            },
        },
    ))
}

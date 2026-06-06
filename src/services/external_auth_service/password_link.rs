use chrono::Utc;

use crate::db::repository::{
    external_auth_email_verification_flow_repo, external_auth_provider_repo,
};
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use crate::services::auth_service;
use crate::utils::hash;

use super::normalize::{normalize_flow_token, token_hash};
use super::resolution::{
    claims_without_provider_email, link_external_auth_identity_to_authenticated_user,
};
use super::{
    ExternalAuthPasswordLinkRequest, ExternalAuthPasswordLinkResult, ExternalAuthPrimaryLogin,
};

const DUMMY_PASSWORD_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRmb3JkdW1teQ$uLpdZ2ciOQUUMGrye7Tyvz/vZ/saqtJiqQBvovmG6ms";

pub async fn link_with_password(
    state: &impl SharedRuntimeState,
    input: ExternalAuthPasswordLinkRequest,
    _ip_address: Option<&str>,
    _user_agent: Option<&str>,
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
        state.writer_db(),
        &token_hash(&flow_token),
        now,
    )
    .await?
    .ok_or_else(|| {
        AsterError::contact_verification_invalid("external auth email verification flow is invalid")
    })?;
    let provider =
        external_auth_provider_repo::find_by_id(state.writer_db(), flow.provider_id).await?;
    if !provider.enabled {
        return Err(AsterError::auth_forbidden(
            "external auth provider is disabled",
        ));
    }

    let user = auth_service::shared::find_user_by_identifier(state.writer_db(), identifier).await?;
    let password_hash = user
        .as_ref()
        .map(|user| user.password_hash.as_str())
        .unwrap_or(DUMMY_PASSWORD_HASH);
    if !hash::verify_password(&input.password, password_hash)? {
        return Err(AsterError::auth_invalid_credentials("invalid credentials"));
    }
    let Some(user) = user else {
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

    let claims = claims_without_provider_email(&flow);
    let txn = crate::db::transaction::begin(state.writer_db()).await?;
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
    Ok(ExternalAuthPasswordLinkResult {
        primary_login: ExternalAuthPrimaryLogin {
            user: resolved.user,
            return_path: flow.return_path.unwrap_or_else(|| "/".to_string()),
            provider_key: provider.key,
            issuer: claims.identity_namespace,
            subject: claims.subject,
            linked: resolved.linked,
            auto_provisioned: resolved.auto_provisioned,
        },
    })
}

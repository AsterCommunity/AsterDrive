use chrono::Utc;

use crate::db::repository::{
    external_auth_email_verification_flow_repo, external_auth_provider_repo,
};
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use crate::services::auth::flow::{AuthFlowState, external_recovery_snapshot};
use crate::services::auth::local;
use aster_forge_external_auth::normalize as external_auth_normalize;

use super::{
    ExternalAuthPasswordLinkRequest, ExternalAuthPasswordLinkResult, ExternalAuthPrimaryLogin,
};

pub async fn link_with_password(
    state: &impl SharedRuntimeState,
    input: ExternalAuthPasswordLinkRequest,
    browser_proof: Option<&str>,
    _ip_address: Option<&str>,
    _user_agent: Option<&str>,
) -> Result<ExternalAuthPasswordLinkResult> {
    local::ensure_password_login_enabled(state)?;
    let flow_token = external_auth_normalize::normalize_flow_token(&input.flow_token, 128)?;
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
        &external_auth_normalize::token_hash(&flow_token),
        now,
    )
    .await?
    .ok_or_else(|| {
        AsterError::contact_verification_invalid("external auth email verification flow is invalid")
    })?;
    super::verification::ensure_recovery_browser(&flow, browser_proof)?;
    if external_recovery_snapshot(&flow, Utc::now()).state != AuthFlowState::RecoveryPending {
        return Err(AsterError::contact_verification_invalid(
            "external auth recovery flow is no longer active",
        ));
    }
    let provider =
        external_auth_provider_repo::find_by_id(state.writer_db(), flow.provider_id).await?;
    if !provider.enabled {
        return Err(AsterError::auth_forbidden(
            "external auth provider is disabled",
        ));
    }

    let user = local::shared::find_user_by_identifier(state.writer_db(), identifier).await?;
    let Some(user) = user else {
        let _ = state
            .runtime_config()
            .password_hash_runtime()
            .verify_password(&input.password, local::shared::DUMMY_PASSWORD_HASH)
            .await?;
        return Err(AsterError::auth_invalid_credentials("invalid credentials"));
    };
    if !local::verify_user_password(state, &user, &input.password).await? {
        return Err(AsterError::auth_invalid_credentials("invalid credentials"));
    }
    if !user.status.is_active() {
        return Err(AsterError::auth_forbidden("account is disabled"));
    }
    if !local::is_email_verified(&user) {
        return Err(AsterError::auth_pending_activation(
            "account pending activation",
        ));
    }

    // Password possession is only the first factor. Keep this unconsumed intent until the
    // account's required MFA succeeds; creating an identity here would bypass account security.
    Ok(ExternalAuthPasswordLinkResult {
        primary_login: ExternalAuthPrimaryLogin {
            user,
            return_path: flow.return_path.unwrap_or_else(|| "/".to_string()),
            provider_key: provider.key,
            issuer: flow.identity_namespace,
            subject: flow.subject,
            linked: false,
            auto_provisioned: false,
            pending_link: Some(super::PendingExternalLink {
                flow_id: flow.id,
                browser_pfoxum: browser_proof.unwrap_or_default().to_string(),
            }),
        },
    })
}

#[cfg(test)]
mod tests {
    use aster_forge_auth::PasswordHashRuntime;

    use crate::services::auth::local::shared::DUMMY_PASSWORD_HASH;

    #[tokio::test]
    async fn dummy_hash_tracks_and_verifies_with_the_production_argon2_profile() {
        assert!(DUMMY_PASSWORD_HASH.starts_with("$argon2id$v=19$m=65536,t=3,p=4$"));
        let verification = PasswordHashRuntime::new(1)
            .unwrap()
            .verify_password("asterdrive-external-auth-dummy", DUMMY_PASSWORD_HASH)
            .await
            .unwrap();
        assert!(verification.is_valid);
        assert!(!verification.needs_rehash);
    }
}

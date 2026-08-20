use chrono::Utc;

use crate::db::repository::{
    external_auth_email_verification_flow_repo, external_auth_provider_repo,
};
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use crate::services::auth::local;
use aster_forge_db::transaction;
use aster_forge_external_auth::normalize as external_auth_normalize;

use super::resolution::{
    claims_without_provider_email, link_external_auth_identity_to_authenticated_user,
};
use super::{
    ExternalAuthPasswordLinkRequest, ExternalAuthPasswordLinkResult, ExternalAuthPrimaryLogin,
};

const DUMMY_PASSWORD_HASH: &str = "$argon2id$v=19$m=65536,t=3,p=4$n0vXvx9kNno+7WMn3NjzQQ$HbceuAm7HxAF4IsSxoy8kD0+IYUK3T6broR+SRLVjrc";

pub async fn link_with_password(
    state: &impl SharedRuntimeState,
    input: ExternalAuthPasswordLinkRequest,
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
            .verify_password(&input.password, DUMMY_PASSWORD_HASH)
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

    let claims = claims_without_provider_email(&flow);
    let txn = transaction::begin(state.writer_db()).await?;
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
            transaction::commit(txn).await?;
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

#[cfg(test)]
mod tests {
    use crate::config::password_hash::PasswordHashRuntime;

    use super::DUMMY_PASSWORD_HASH;

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

//! 认证服务子模块：`password`。

use crate::api::subcode::ApiSubcode;
use crate::db::repository::user_repo;
use crate::errors::{AsterError, Result, auth_forbidden_with_subcode};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::utils::hash;

use super::session::{invalidate_auth_snapshot_cache, purge_all_auth_sessions_in_connection};
use super::shared::{find_user_by_identifier, update_password_in_connection};
use crate::services::mfa_service::{self, PrimaryLoginCompletion};

use super::{AuthUserInfo, is_email_verified};

pub async fn login(
    state: &PrimaryAppState,
    identifier: &str,
    password: &str,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<PrimaryLoginCompletion> {
    let identifier_kind = if identifier.trim().contains('@') {
        "email"
    } else {
        "username"
    };
    tracing::debug!(identifier_kind, "login attempt");

    let outcome = async {
        let Some(user) = find_user_by_identifier(state.writer_db(), identifier).await? else {
            tracing::debug!(identifier_kind, "login rejected: user not found");
            return Err(AsterError::auth_invalid_credentials("Invalid Credentials"));
        };

        if !user.status.is_active() {
            tracing::debug!(user_id = user.id, "login rejected: account disabled");
            return Err(auth_forbidden_with_subcode(
                ApiSubcode::AuthAccountDisabled,
                "account is disabled",
            ));
        }
        if !is_email_verified(&user) {
            tracing::debug!(
                user_id = user.id,
                "login rejected: account pending activation"
            );
            return Err(AsterError::auth_pending_activation(
                "account pending activation",
            ));
        }

        if !hash::verify_password(password, &user.password_hash)? {
            tracing::debug!(user_id = user.id, "login rejected: invalid password");
            return Err(AsterError::auth_invalid_credentials("Invalid Credentials"));
        }

        let completion = mfa_service::complete_primary_login_or_start_mfa(
            state,
            &user,
            crate::types::MfaFirstFactor::Password,
            None,
            ip_address,
            user_agent,
        )
        .await?;

        tracing::debug!(
            user_id = user.id,
            session_version = user.session_version,
            "login succeeded"
        );

        Ok(completion)
    }
    .await;

    record_login_metric(state, &outcome);
    outcome
}

fn record_login_metric(state: &impl SharedRuntimeState, result: &Result<PrimaryLoginCompletion>) {
    let (status, reason) = match result {
        Ok(_) => ("success", "ok"),
        Err(AsterError::AuthInvalidCredentials(_)) => ("failure", "invalid_credentials"),
        Err(AsterError::AuthForbidden(_)) => ("failure", "forbidden"),
        Err(AsterError::AuthPendingActivation(_)) => ("failure", "pending_activation"),
        Err(AsterError::RateLimited(_)) => ("failure", "rate_limited"),
        Err(_) => ("failure", "error"),
    };
    state.metrics().record_auth_event("login", status, reason);
}

pub async fn change_password(
    state: &impl SharedRuntimeState,
    user_id: i64,
    current_password: &str,
    new_password: &str,
) -> Result<AuthUserInfo> {
    tracing::debug!(user_id, "changing password");
    let user = user_repo::find_by_id(state.writer_db(), user_id).await?;

    if !user.status.is_active() {
        return Err(auth_forbidden_with_subcode(
            ApiSubcode::AuthAccountDisabled,
            "account is disabled",
        ));
    }

    if !hash::verify_password(current_password, &user.password_hash)? {
        return Err(AsterError::auth_invalid_credentials("wrong password"));
    }

    set_password(state, user.id, new_password).await
}

pub async fn set_password(
    state: &impl SharedRuntimeState,
    user_id: i64,
    new_password: &str,
) -> Result<AuthUserInfo> {
    tracing::debug!(user_id, "setting password");
    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    let result = async {
        let user = user_repo::find_by_id(&txn, user_id).await?;
        let updated = update_password_in_connection(&txn, user, new_password).await?;
        purge_all_auth_sessions_in_connection(&txn, updated.id).await?;
        Ok::<_, AsterError>(updated)
    }
    .await;
    let updated = match result {
        Ok(updated) => {
            crate::db::transaction::commit(txn).await?;
            updated
        }
        Err(error) => {
            crate::db::transaction::rollback(txn).await?;
            return Err(error);
        }
    };
    invalidate_auth_snapshot_cache(state, updated.id).await;
    tracing::debug!(
        user_id = updated.id,
        session_version = updated.session_version,
        "set password"
    );
    Ok(AuthUserInfo::from(updated))
}

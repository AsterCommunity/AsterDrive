//! 认证服务子模块：`password`。

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::user_repo;
use crate::errors::{AsterError, Result, auth_forbidden_with_code};
use crate::runtime::SharedRuntimeState;
use aster_forge_db::transaction;

use super::session::{invalidate_auth_snapshot_cache, purge_all_auth_sessions_in_connection};
use super::shared::{find_user_by_identifier, update_password_in_connection};
use super::validate_password;
use crate::services::auth::mfa::{self, PrimaryLoginCompletion};

use super::{AuthUserInfo, is_email_verified};

pub async fn login(
    state: &impl SharedRuntimeState,
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

    let mut failure_reason = None;
    let outcome = async {
        let Some(user) = find_user_by_identifier(state.writer_db(), identifier).await? else {
            tracing::debug!(identifier_kind, "login rejected: user not found");
            failure_reason = Some(LoginFailureReason::InvalidCredentials);
            return Err(AsterError::auth_invalid_credentials("Invalid Credentials"));
        };

        if !user.status.is_active() {
            tracing::debug!(user_id = user.id, "login rejected: account disabled");
            failure_reason = Some(LoginFailureReason::AccountDisabled);
            return Err(AsterError::auth_invalid_credentials("Invalid Credentials"));
        }
        if !is_email_verified(&user) {
            tracing::debug!(
                user_id = user.id,
                "login rejected: account pending activation"
            );
            failure_reason = Some(LoginFailureReason::PendingActivation);
            return Err(AsterError::auth_invalid_credentials("Invalid Credentials"));
        }

        if !verify_user_password(state, &user, password).await? {
            tracing::debug!(user_id = user.id, "login rejected: invalid password");
            failure_reason = Some(LoginFailureReason::InvalidCredentials);
            return Err(AsterError::auth_invalid_credentials("Invalid Credentials"));
        }

        let completion = mfa::complete_primary_login_or_start_mfa(
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

    record_login_metric(state, &outcome, failure_reason);
    outcome
}

#[derive(Debug, Clone, Copy)]
enum LoginFailureReason {
    InvalidCredentials,
    AccountDisabled,
    PendingActivation,
}

fn record_login_metric(
    state: &impl SharedRuntimeState,
    result: &Result<PrimaryLoginCompletion>,
    failure_reason: Option<LoginFailureReason>,
) {
    let (status, reason) = match result {
        Ok(_) => ("success", "ok"),
        Err(AsterError::AuthInvalidCredentials(_)) => (
            "failure",
            match failure_reason {
                Some(LoginFailureReason::AccountDisabled) => "account_disabled",
                Some(LoginFailureReason::PendingActivation) => "pending_activation",
                _ => "invalid_credentials",
            },
        ),
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
        return Err(auth_forbidden_with_code(
            ApiErrorCode::AuthAccountDisabled,
            "account is disabled",
        ));
    }

    if !state
        .runtime_config()
        .password_hash_runtime()
        .verify_password(current_password, &user.password_hash)
        .await?
        .is_valid
    {
        return Err(AsterError::auth_invalid_credentials("wrong password"));
    }

    if new_password == current_password {
        return Err(AsterError::validation_error(
            "new password must be different from current password",
        ));
    }

    set_password(state, user.id, new_password).await
}

pub async fn set_password(
    state: &impl SharedRuntimeState,
    user_id: i64,
    new_password: &str,
) -> Result<AuthUserInfo> {
    tracing::debug!(user_id, "setting password");
    validate_password(new_password)?;
    let new_password_hash = state
        .runtime_config()
        .password_hash_runtime()
        .hash_password(new_password)
        .await?;
    let txn = transaction::begin(state.writer_db()).await?;
    let result = async {
        let user = user_repo::find_by_id(&txn, user_id).await?;
        let was_forced = user.must_change_password;
        let updated = update_password_in_connection(&txn, user, new_password_hash).await?;
        purge_all_auth_sessions_in_connection(&txn, updated.id).await?;
        Ok::<_, AsterError>((updated, was_forced))
    }
    .await;
    let (updated, was_forced) = match result {
        Ok(updated) => {
            transaction::commit(txn).await?;
            updated
        }
        Err(error) => {
            transaction::rollback(txn).await?;
            return Err(error);
        }
    };
    invalidate_auth_snapshot_cache(state, updated.id).await;
    tracing::debug!(
        user_id = updated.id,
        session_version = updated.session_version,
        "set password"
    );
    if was_forced {
        tracing::info!(user_id = updated.id, "completed forced password change");
    }
    Ok(AuthUserInfo::from(updated))
}

pub(crate) async fn verify_user_password(
    state: &impl SharedRuntimeState,
    user: &crate::entities::user::Model,
    password: &str,
) -> Result<bool> {
    let verification = state
        .runtime_config()
        .password_hash_runtime()
        .verify_password(password, &user.password_hash)
        .await?;
    if !verification.is_valid {
        return Ok(false);
    }

    if verification.needs_rehash {
        upgrade_user_password_hash(state, user, password).await;
    }
    Ok(true)
}

async fn upgrade_user_password_hash(
    state: &impl SharedRuntimeState,
    user: &crate::entities::user::Model,
    password: &str,
) {
    let new_hash = match state
        .runtime_config()
        .password_hash_runtime()
        .hash_password(password)
        .await
    {
        Ok(hash) => hash,
        Err(error) => {
            tracing::warn!(
                user_id = user.id,
                "failed to generate upgraded user password hash: {error}"
            );
            return;
        }
    };

    match user_repo::update_password_hash_if_current(
        state.writer_db(),
        user.id,
        &user.password_hash,
        &new_hash,
    )
    .await
    {
        Ok(true) => tracing::info!(user_id = user.id, "upgraded user password hash policy"),
        Ok(false) => tracing::debug!(
            user_id = user.id,
            "skipped user password hash upgrade after concurrent credential change"
        ),
        Err(error) => tracing::warn!(
            user_id = user.id,
            "failed to persist upgraded user password hash: {error}"
        ),
    }
}

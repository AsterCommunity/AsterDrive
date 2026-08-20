//! 认证服务子模块：`registration`。

use aster_forge_db::transaction;
use chrono::Utc;

use crate::api::api_error_code::ApiErrorCode;
use crate::config::{
    auth_runtime::{RuntimeAuthPolicy, RuntimeContactVerificationPolicy},
    branding,
    local_email_policy::LocalEmailPolicy,
};
use crate::db::repository::system_initialization_repo;
use crate::errors::{Result, auth_forbidden_with_code, validation_error_with_code};
use crate::runtime::SharedRuntimeState;
use crate::services::{mail::outbox, mail::template::MailTemplatePayload};
use aster_drive_model::types::{UserRole, UserStatus, VerificationPurpose};

use super::shared::{
    CreateUserWithRoleInput, create_first_admin, create_user_with_role, find_user_by_identifier,
    hash_new_password, is_active_verification_request_error, issue_contact_verification_token,
    resend_allowed,
};
use super::{
    AuthUserInfo, UserAuditInfo, ensure_password_login_enabled, is_email_verified, user_audit_info,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegisterActivationResendOutcome {
    Sent(UserAuditInfo),
    EmailNotFound,
    AlreadyActive,
    AccountDisabled,
    Cooldown,
    EmailPolicyRejected,
}

impl RegisterActivationResendOutcome {
    pub fn metric_reason(&self) -> &'static str {
        match self {
            Self::Sent(_) => "pending_activation",
            Self::EmailNotFound => "email_not_found",
            Self::AlreadyActive => "already_active",
            Self::AccountDisabled => "account_disabled",
            Self::Cooldown => "rate_limited",
            Self::EmailPolicyRejected => "email_policy_rejected",
        }
    }

    pub fn metric_status(&self) -> &'static str {
        match self {
            Self::Sent(_) => "sent",
            Self::EmailNotFound
            | Self::AlreadyActive
            | Self::AccountDisabled
            | Self::Cooldown
            | Self::EmailPolicyRejected => "skipped",
        }
    }
}

pub async fn create_user_by_admin(
    state: &impl SharedRuntimeState,
    username: &str,
    email: &str,
    password: &str,
    must_change_password: bool,
) -> Result<AuthUserInfo> {
    let password_hash = hash_new_password(state, password).await?;
    let user = create_user_with_role(
        state.writer_db(),
        state,
        CreateUserWithRoleInput {
            username,
            email,
            password_hash: &password_hash,
            role: UserRole::User,
            status: UserStatus::Active,
            must_change_password,
            email_verified_at: Some(Utc::now()),
        },
    )
    .await?;
    if let Some(policy_group_id) = user.policy_group_id {
        state
            .policy_snapshot()
            .set_user_policy_group(user.id, policy_group_id);
        crate::services::ops::config::runtime::publish_user_policy_group_reload_after_commit(
            state,
            "create_by_admin",
            user.id,
        )
        .await;
    }
    Ok(AuthUserInfo::from(user))
}

pub async fn register(
    state: &impl SharedRuntimeState,
    username: &str,
    email: &str,
    password: &str,
) -> Result<AuthUserInfo> {
    crate::services::system_setup::require_ready(state.writer_db()).await?;
    ensure_password_login_enabled(state)?;

    let auth_policy = RuntimeAuthPolicy::from_runtime_config(state.runtime_config());
    tracing::debug!(
        registration_enabled = auth_policy.allow_user_registration,
        activation_enabled = auth_policy.register_activation_enabled,
        "registering user"
    );
    if !auth_policy.allow_user_registration {
        return Err(auth_forbidden_with_code(
            ApiErrorCode::AuthRegistrationDisabled,
            "new user registration is disabled",
        ));
    }

    LocalEmailPolicy::from_runtime_config(state.runtime_config()).check(email)?;
    let password_hash = hash_new_password(state, password).await?;

    let policy = RuntimeContactVerificationPolicy::from_runtime_config(state.runtime_config());
    let site_name = branding::title_or_default(state.runtime_config());
    let txn = transaction::begin(state.writer_db()).await?;
    let email_verified_at = (!auth_policy.register_activation_enabled).then_some(Utc::now());
    let user = create_user_with_role(
        &txn,
        state,
        CreateUserWithRoleInput {
            username,
            email,
            password_hash: &password_hash,
            role: UserRole::User,
            status: UserStatus::Active,
            must_change_password: false,
            email_verified_at,
        },
    )
    .await?;
    if auth_policy.register_activation_enabled {
        let token = issue_contact_verification_token(
            &txn,
            user.id,
            VerificationPurpose::RegisterActivation,
            &user.email,
            policy.register_activation_ttl_secs,
        )
        .await?;
        outbox::enqueue(
            &txn,
            &user.email,
            Some(&user.username),
            MailTemplatePayload::register_activation(&user.username, &token, &site_name),
        )
        .await?;
    }
    transaction::commit(txn).await?;
    if let Some(policy_group_id) = user.policy_group_id {
        state
            .policy_snapshot()
            .set_user_policy_group(user.id, policy_group_id);
        crate::services::ops::config::runtime::publish_user_policy_group_reload_after_commit(
            state, "register", user.id,
        )
        .await;
    }

    tracing::debug!(
        user_id = user.id,
        activation_enabled = auth_policy.register_activation_enabled,
        email_verified = user.email_verified_at.is_some(),
        "registered user"
    );
    Ok(AuthUserInfo::from(user))
}

pub async fn resend_register_activation(
    state: &impl SharedRuntimeState,
    identifier: &str,
) -> Result<RegisterActivationResendOutcome> {
    ensure_password_login_enabled(state)?;
    let Some(user) = find_user_by_identifier(state.writer_db(), identifier).await? else {
        return Ok(RegisterActivationResendOutcome::EmailNotFound);
    };

    if !user.status.is_active() {
        return Ok(RegisterActivationResendOutcome::AccountDisabled);
    }
    if is_email_verified(&user) {
        return Ok(RegisterActivationResendOutcome::AlreadyActive);
    }

    if let Err(error) =
        LocalEmailPolicy::from_runtime_config(state.runtime_config()).check_not_blocked(&user.email)
    {
        tracing::debug!(
            user_id = user.id,
            error = %error,
            "register activation resend skipped due to email policy"
        );
        return Ok(RegisterActivationResendOutcome::EmailPolicyRejected);
    }

    if !resend_allowed(
        state,
        state.writer_db(),
        user.id,
        VerificationPurpose::RegisterActivation,
    )
    .await?
    {
        tracing::debug!(
            user_id = user.id,
            "register activation resend skipped due to cooldown"
        );
        return Ok(RegisterActivationResendOutcome::Cooldown);
    }
    let policy = RuntimeContactVerificationPolicy::from_runtime_config(state.runtime_config());
    let site_name = branding::title_or_default(state.runtime_config());

    let txn = transaction::begin(state.writer_db()).await?;
    let token = match issue_contact_verification_token(
        &txn,
        user.id,
        VerificationPurpose::RegisterActivation,
        &user.email,
        policy.register_activation_ttl_secs,
    )
    .await
    {
        Ok(token) => token,
        Err(err) if is_active_verification_request_error(&err) => {
            return Ok(RegisterActivationResendOutcome::Cooldown);
        }
        Err(err) => return Err(err),
    };
    outbox::enqueue(
        &txn,
        &user.email,
        Some(&user.username),
        MailTemplatePayload::register_activation(&user.username, &token, &site_name),
    )
    .await?;
    transaction::commit(txn).await?;

    Ok(RegisterActivationResendOutcome::Sent(user_audit_info(
        &user,
    )))
}

pub async fn check_auth_state(
    state: &impl SharedRuntimeState,
) -> Result<crate::services::system_setup::SystemSetupStatus> {
    crate::services::system_setup::inspect(state.writer_db()).await
}

pub async fn setup(
    state: &impl SharedRuntimeState,
    username: &str,
    email: &str,
    password: &str,
) -> Result<AuthUserInfo> {
    tracing::debug!("running initial setup");
    ensure_initial_admin_setup_required(state.writer_db()).await?;
    let password_hash = hash_new_password(state, password).await?;
    let txn = transaction::begin(state.writer_db()).await?;
    system_initialization_repo::acquire_setup_lock(&txn).await?;
    ensure_initial_admin_setup_required(&txn).await?;

    let user = create_first_admin(&txn, state, username, email, &password_hash)
        .await
        .map(AuthUserInfo::from)?;
    transaction::commit(txn).await?;

    if let Some(policy_group_id) = user.policy_group_id {
        state
            .policy_snapshot()
            .set_user_policy_group(user.id, policy_group_id);
        crate::services::ops::config::runtime::publish_user_policy_group_reload_after_commit(
            state, "setup", user.id,
        )
        .await;
    }
    tracing::debug!(user_id = user.id, "completed initial setup");
    Ok(user)
}

async fn ensure_initial_admin_setup_required<C: sea_orm::ConnectionTrait>(db: &C) -> Result<()> {
    if crate::services::system_setup::state(db).await?
        == crate::services::system_setup::SystemSetupState::NeedsAdmin
    {
        Ok(())
    } else {
        Err(validation_error_with_code(
            ApiErrorCode::ValidationSystemAlreadyInitialized,
            "system already initialized",
        ))
    }
}

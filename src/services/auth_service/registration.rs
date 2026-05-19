//! 认证服务子模块：`registration`。

use chrono::Utc;

use crate::api::subcode::ApiSubcode;
use crate::config::{
    auth_runtime::{RuntimeAuthPolicy, RuntimeContactVerificationPolicy},
    branding,
};
use crate::db::repository::user_repo;
use crate::errors::{Result, auth_forbidden_with_subcode, validation_error_with_subcode};
use crate::runtime::PrimaryAppState;
use crate::services::{mail_outbox_service, mail_template::MailTemplatePayload};
use crate::types::{UserRole, UserStatus, VerificationPurpose};

use super::shared::{
    CreateUserWithRoleInput, create_first_admin, create_user_with_role, find_user_by_identifier,
    is_active_verification_request_error, issue_contact_verification_token, resend_allowed,
};
use super::{AuthUserInfo, UserAuditInfo, is_email_verified, user_audit_info};

pub async fn create_user_by_admin(
    state: &PrimaryAppState,
    username: &str,
    email: &str,
    password: &str,
) -> Result<AuthUserInfo> {
    let user = create_user_with_role(
        &state.db,
        state,
        CreateUserWithRoleInput {
            username,
            email,
            password,
            role: UserRole::User,
            status: UserStatus::Active,
            email_verified_at: Some(Utc::now()),
        },
    )
    .await?;
    if let Some(policy_group_id) = user.policy_group_id {
        state
            .policy_snapshot
            .set_user_policy_group(user.id, policy_group_id);
    }
    Ok(AuthUserInfo::from(user))
}

pub async fn register(
    state: &PrimaryAppState,
    username: &str,
    email: &str,
    password: &str,
) -> Result<AuthUserInfo> {
    let auth_policy = RuntimeAuthPolicy::from_runtime_config(&state.runtime_config);
    tracing::debug!(
        registration_enabled = auth_policy.allow_user_registration,
        activation_enabled = auth_policy.register_activation_enabled,
        "registering user"
    );
    if !auth_policy.allow_user_registration {
        return Err(auth_forbidden_with_subcode(
            ApiSubcode::AuthRegistrationDisabled,
            "new user registration is disabled",
        ));
    }

    if user_repo::count_all(&state.db).await? == 0 {
        return create_first_admin(state, username, email, password)
            .await
            .map(AuthUserInfo::from);
    }

    let policy = RuntimeContactVerificationPolicy::from_runtime_config(&state.runtime_config);
    let site_name = branding::title_or_default(&state.runtime_config);
    let txn = crate::db::transaction::begin(&state.db).await?;
    let email_verified_at = (!auth_policy.register_activation_enabled).then_some(Utc::now());
    let user = create_user_with_role(
        &txn,
        state,
        CreateUserWithRoleInput {
            username,
            email,
            password,
            role: UserRole::User,
            status: UserStatus::Active,
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
        mail_outbox_service::enqueue(
            &txn,
            &user.email,
            Some(&user.username),
            MailTemplatePayload::register_activation(&user.username, &token, &site_name),
        )
        .await?;
    }
    crate::db::transaction::commit(txn).await?;
    if let Some(policy_group_id) = user.policy_group_id {
        state
            .policy_snapshot
            .set_user_policy_group(user.id, policy_group_id);
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
    state: &PrimaryAppState,
    identifier: &str,
) -> Result<Option<UserAuditInfo>> {
    let Some(user) = find_user_by_identifier(&state.db, identifier).await? else {
        return Ok(None);
    };

    if !user.status.is_active() || is_email_verified(&user) {
        return Ok(None);
    }

    if !resend_allowed(
        state,
        &state.db,
        user.id,
        VerificationPurpose::RegisterActivation,
    )
    .await?
    {
        tracing::debug!(
            user_id = user.id,
            "register activation resend skipped due to cooldown"
        );
        return Ok(None);
    }
    let policy = RuntimeContactVerificationPolicy::from_runtime_config(&state.runtime_config);
    let site_name = branding::title_or_default(&state.runtime_config);

    let txn = crate::db::transaction::begin(&state.db).await?;
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
        Err(err) if is_active_verification_request_error(&err) => return Ok(None),
        Err(err) => return Err(err),
    };
    mail_outbox_service::enqueue(
        &txn,
        &user.email,
        Some(&user.username),
        MailTemplatePayload::register_activation(&user.username, &token, &site_name),
    )
    .await?;
    crate::db::transaction::commit(txn).await?;

    Ok(Some(user_audit_info(&user)))
}

pub async fn check_auth_state(state: &PrimaryAppState) -> Result<bool> {
    Ok(user_repo::count_all(&state.db).await? > 0)
}

pub async fn setup(
    state: &PrimaryAppState,
    username: &str,
    email: &str,
    password: &str,
) -> Result<AuthUserInfo> {
    tracing::debug!("running initial setup");
    if user_repo::count_all(&state.db).await? > 0 {
        return Err(validation_error_with_subcode(
            ApiSubcode::ValidationSystemAlreadyInitialized,
            "system already initialized",
        ));
    }
    let user = create_first_admin(state, username, email, password)
        .await
        .map(AuthUserInfo::from)?;
    tracing::debug!(user_id = user.id, "completed initial setup");
    Ok(user)
}

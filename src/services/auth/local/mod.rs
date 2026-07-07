//! 认证服务聚合入口。

mod cache;
mod contact_verification;
mod password;
mod registration;
mod session;
pub(crate) mod shared;
mod tokens;
mod validation;

use crate::errors::Result;
use crate::runtime::SharedRuntimeState;
use crate::services::audit_service::{self, AuditContext, AuditRequestInfo};
use crate::services::auth::mfa::PrimaryLoginCompletion;
use sea_orm::{ActiveValue, Set};
use serde::{Deserialize, Serialize};

use crate::entities::user;
use crate::types::{StoredUserConfig, TokenType, UserRole, UserStatus, VerificationPurpose};

pub use contact_verification::{
    cleanup_expired_contact_verification_tokens, confirm_contact_verification,
    confirm_password_reset, request_email_change, request_password_reset, resend_email_change,
};
pub use password::{change_password, login, set_password};
pub use registration::{
    RegisterActivationResendOutcome, check_auth_state, create_user_by_admin, register,
    resend_register_activation, setup,
};
pub use session::{
    cleanup_expired_auth_sessions, get_auth_snapshot, invalidate_auth_snapshot_cache,
    list_auth_sessions, revoke_auth_session, revoke_other_auth_sessions, revoke_user_sessions,
};
#[cfg(debug_assertions)]
pub use tokens::test_support;
pub use tokens::{
    authenticate_access_token, authenticate_refresh_token, issue_password_change_tokens_for_user,
    issue_tokens_for_session, issue_tokens_for_user, issue_tokens_for_user_in_connection,
    refresh_tokens, revoke_refresh_token, verify_token,
};
pub(crate) use validation::{validate_email, validate_password, validate_username};

const INITIAL_SESSION_VERSION: i64 = 1;
const ACTIVE_VERIFICATION_REQUEST_MESSAGE: &str = "a verification request is already active";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,
    pub user_id: i64,
    #[serde(default = "default_session_version")]
    pub session_version: i64,
    #[serde(default)]
    pub password_change: bool,
    #[serde(default)]
    pub jti: Option<String>,
    pub token_type: TokenType,
    pub exp: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub struct AuthSnapshot {
    pub status: UserStatus,
    pub role: UserRole,
    pub session_version: i64,
    pub must_change_password: bool,
}

#[derive(Debug)]
pub struct ContactVerificationConfirmResult {
    pub purpose: VerificationPurpose,
    pub user_id: i64,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserAuditInfo {
    pub id: i64,
    pub username: String,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct AuthSessionInfo {
    pub id: String,
    pub is_current: bool,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub last_seen_at: chrono::DateTime<chrono::Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthUserInfo {
    pub id: i64,
    pub username: String,
    pub email: String,
    pub role: UserRole,
    pub status: UserStatus,
    pub must_change_password: bool,
    pub session_version: i64,
    pub email_verified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub pending_email: Option<String>,
    pub storage_used: i64,
    pub storage_quota: i64,
    pub policy_group_id: Option<i64>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub config: Option<StoredUserConfig>,
}

impl From<user::Model> for AuthUserInfo {
    fn from(model: user::Model) -> Self {
        Self {
            id: model.id,
            username: model.username,
            email: model.email,
            role: model.role,
            status: model.status,
            must_change_password: model.must_change_password,
            session_version: model.session_version,
            email_verified_at: model.email_verified_at,
            pending_email: model.pending_email,
            storage_used: model.storage_used,
            storage_quota: model.storage_quota,
            policy_group_id: model.policy_group_id,
            created_at: model.created_at,
            updated_at: model.updated_at,
            config: model.config,
        }
    }
}

impl From<AuthUserInfo> for user::ActiveModel {
    fn from(info: AuthUserInfo) -> Self {
        Self {
            id: Set(info.id),
            username: Set(info.username),
            email: Set(info.email),
            password_hash: ActiveValue::NotSet,
            role: Set(info.role),
            status: Set(info.status),
            must_change_password: Set(info.must_change_password),
            session_version: Set(info.session_version),
            email_verified_at: Set(info.email_verified_at),
            pending_email: Set(info.pending_email),
            storage_used: Set(info.storage_used),
            storage_quota: Set(info.storage_quota),
            policy_group_id: Set(info.policy_group_id),
            created_at: Set(info.created_at),
            updated_at: Set(info.updated_at),
            config: Set(info.config),
        }
    }
}

#[derive(Debug)]
pub struct PasswordResetRequestResult {
    pub user: Option<UserAuditInfo>,
}

#[derive(Debug)]
pub struct LoginResult {
    pub access_token: String,
    pub refresh_token: String,
    pub user_id: i64,
    pub password_change_required: bool,
}

impl AuthSnapshot {
    fn from_user(user: &user::Model) -> Self {
        Self {
            status: user.status,
            role: user.role,
            session_version: user.session_version,
            must_change_password: user.must_change_password,
        }
    }
}

fn default_session_version() -> i64 {
    0
}

fn user_audit_info(user: &user::Model) -> UserAuditInfo {
    UserAuditInfo {
        id: user.id,
        username: user.username.clone(),
    }
}

pub fn is_email_verified(user: &user::Model) -> bool {
    user.email_verified_at.is_some()
}

// 审计包装收敛在聚合层，避免 registration/password/contact_verification 这些
// 纯业务子模块依赖 route 级副作用。
pub async fn setup_with_audit(
    state: &impl SharedRuntimeState,
    username: &str,
    email: &str,
    password: &str,
    request_info: &AuditRequestInfo,
) -> Result<AuthUserInfo> {
    let user = setup(state, username, email, password).await?;
    let audit_ctx = request_info.to_context(user.id);
    audit_service::log(
        state,
        &audit_ctx,
        audit_service::AuditAction::SystemSetup,
        audit_service::AuditEntityType::User,
        None,
        Some(&user.username),
        None,
    )
    .await;
    Ok(user)
}

pub async fn register_with_audit(
    state: &impl SharedRuntimeState,
    username: &str,
    email: &str,
    password: &str,
    request_info: &AuditRequestInfo,
) -> Result<AuthUserInfo> {
    let user = register(state, username, email, password).await?;
    let audit_ctx = request_info.to_context(user.id);
    audit_service::log(
        state,
        &audit_ctx,
        audit_service::AuditAction::UserRegister,
        audit_service::AuditEntityType::User,
        None,
        Some(&user.username),
        None,
    )
    .await;
    Ok(user)
}

pub async fn resend_register_activation_with_audit(
    state: &impl SharedRuntimeState,
    identifier: &str,
    request_info: &AuditRequestInfo,
) -> Result<Option<UserAuditInfo>> {
    let outcome = resend_register_activation(state, identifier).await?;
    state.metrics().record_auth_event(
        "register_activation_resend",
        outcome.metric_status(),
        outcome.metric_reason(),
    );
    if let RegisterActivationResendOutcome::Sent(user) = &outcome {
        let audit_ctx = request_info.to_context(user.id);
        audit_service::log(
            state,
            &audit_ctx,
            audit_service::AuditAction::UserResendRegistration,
            crate::services::audit_service::AuditEntityType::User,
            Some(user.id),
            Some(&user.username),
            None,
        )
        .await;
    }
    Ok(match outcome {
        RegisterActivationResendOutcome::Sent(user) => Some(user),
        RegisterActivationResendOutcome::EmailNotFound
        | RegisterActivationResendOutcome::AlreadyActive
        | RegisterActivationResendOutcome::AccountDisabled
        | RegisterActivationResendOutcome::Cooldown
        | RegisterActivationResendOutcome::EmailPolicyRejected => None,
    })
}

pub async fn confirm_contact_verification_with_audit(
    state: &impl SharedRuntimeState,
    token: &str,
    request_info: &AuditRequestInfo,
) -> Result<ContactVerificationConfirmResult> {
    let result = confirm_contact_verification(state, token).await?;
    let audit_ctx = request_info.to_context(result.user_id);
    let action = match result.purpose {
        VerificationPurpose::RegisterActivation => {
            audit_service::AuditAction::UserConfirmRegistration
        }
        VerificationPurpose::ContactChange => audit_service::AuditAction::UserConfirmEmailChange,
        VerificationPurpose::PasswordReset => audit_service::AuditAction::UserConfirmPasswordReset,
    };
    audit_service::log(
        state,
        &audit_ctx,
        action,
        crate::services::audit_service::AuditEntityType::User,
        Some(result.user_id),
        None,
        None,
    )
    .await;
    Ok(result)
}

pub async fn request_password_reset_with_audit(
    state: &impl SharedRuntimeState,
    email: &str,
    request_info: &AuditRequestInfo,
) -> Result<PasswordResetRequestResult> {
    let result = request_password_reset(state, email).await?;
    if let Some(user) = result.user.as_ref() {
        let audit_ctx = request_info.to_context(user.id);
        audit_service::log(
            state,
            &audit_ctx,
            audit_service::AuditAction::UserRequestPasswordReset,
            crate::services::audit_service::AuditEntityType::User,
            Some(user.id),
            Some(&user.username),
            None,
        )
        .await;
    }
    Ok(result)
}

pub async fn confirm_password_reset_with_audit(
    state: &impl SharedRuntimeState,
    token: &str,
    new_password: &str,
    request_info: &AuditRequestInfo,
) -> Result<AuthUserInfo> {
    let user = confirm_password_reset(state, token, new_password).await?;
    let audit_ctx = request_info.to_context(user.id);
    audit_service::log(
        state,
        &audit_ctx,
        audit_service::AuditAction::UserConfirmPasswordReset,
        crate::services::audit_service::AuditEntityType::User,
        Some(user.id),
        Some(&user.username),
        None,
    )
    .await;
    Ok(user)
}

pub async fn login_with_audit(
    state: &impl SharedRuntimeState,
    identifier: &str,
    password: &str,
    request_info: &AuditRequestInfo,
) -> Result<PrimaryLoginCompletion> {
    let result = login(
        state,
        identifier,
        password,
        request_info.ip_address.as_deref(),
        request_info.user_agent.as_deref(),
    )
    .await?;
    let (user_id, details) = match &result {
        PrimaryLoginCompletion::Authenticated(login) => (
            login.user_id,
            audit_service::details(audit_service::UserLoginAuditDetails {
                mfa_required: false,
                password_change_required: Some(login.password_change_required),
                available_methods: None,
            }),
        ),
        PrimaryLoginCompletion::MfaRequired(challenge) => (challenge.user_id, {
            let available_methods = challenge
                .methods
                .iter()
                .map(|method| method.as_str())
                .collect::<Vec<_>>();
            audit_service::details(audit_service::UserLoginAuditDetails {
                mfa_required: true,
                password_change_required: None,
                available_methods: Some(available_methods.as_slice()),
            })
        }),
    };
    let audit_ctx = request_info.to_context(user_id);
    audit_service::log_with_details(
        state,
        &audit_ctx,
        audit_service::AuditAction::UserLogin,
        audit_service::AuditEntityType::AuthSession,
        None,
        Some(identifier),
        || details.clone(),
    )
    .await;
    Ok(result)
}

pub async fn log_logout_for_token(
    state: &impl SharedRuntimeState,
    token: &str,
    request_info: &AuditRequestInfo,
) -> bool {
    let Ok(claims) = verify_token(token, &state.config().auth.jwt_secret) else {
        return false;
    };

    let audit_ctx = request_info.to_context(claims.user_id);
    audit_service::log(
        state,
        &audit_ctx,
        audit_service::AuditAction::UserLogout,
        audit_service::AuditEntityType::AuthSession,
        None,
        None,
        None,
    )
    .await;
    true
}

pub async fn change_password_with_audit(
    state: &impl SharedRuntimeState,
    user_id: i64,
    current_password: &str,
    new_password: &str,
    audit_ctx: &AuditContext,
) -> Result<AuthUserInfo> {
    let user = change_password(state, user_id, current_password, new_password).await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::UserChangePassword,
        audit_service::AuditEntityType::User,
        None,
        None,
        None,
    )
    .await;
    Ok(user)
}

pub async fn request_email_change_with_audit(
    state: &impl SharedRuntimeState,
    user_id: i64,
    new_email: &str,
    audit_ctx: &AuditContext,
) -> Result<AuthUserInfo> {
    let user = request_email_change(state, user_id, new_email).await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::UserRequestEmailChange,
        crate::services::audit_service::AuditEntityType::User,
        Some(user.id),
        Some(&user.username),
        None,
    )
    .await;
    Ok(user)
}

pub async fn resend_email_change_with_audit(
    state: &impl SharedRuntimeState,
    user_id: i64,
    audit_ctx: &AuditContext,
) -> Result<Option<UserAuditInfo>> {
    let user = resend_email_change(state, user_id).await?;
    if let Some(user) = user.as_ref() {
        audit_service::log(
            state,
            audit_ctx,
            audit_service::AuditAction::UserResendEmailChange,
            crate::services::audit_service::AuditEntityType::User,
            Some(user.id),
            Some(&user.username),
            None,
        )
        .await;
    }
    Ok(user)
}

pub async fn revoke_user_sessions_with_audit(
    state: &impl SharedRuntimeState,
    user_id: i64,
    audit_ctx: &AuditContext,
) -> Result<UserAuditInfo> {
    let user = revoke_user_sessions(state, user_id).await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::AdminRevokeUserSessions,
        crate::services::audit_service::AuditEntityType::User,
        Some(user.id),
        Some(&user.username),
        None,
    )
    .await;
    Ok(user)
}

pub async fn set_password_with_audit(
    state: &impl SharedRuntimeState,
    user_id: i64,
    new_password: &str,
    audit_ctx: &AuditContext,
) -> Result<AuthUserInfo> {
    let user = set_password(state, user_id, new_password).await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::AdminResetUserPassword,
        crate::services::audit_service::AuditEntityType::User,
        Some(user.id),
        Some(&user.username),
        None,
    )
    .await;
    Ok(user)
}

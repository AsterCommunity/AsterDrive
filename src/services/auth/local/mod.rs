//! 认证服务聚合入口。

mod contact_verification;
mod password;
mod registration;
mod session;
pub(crate) mod shared;
mod tokens;
mod validation;

use crate::config::auth_runtime::RuntimeAuthPolicy;
use crate::errors::Result;
use crate::runtime::SharedRuntimeState;
use crate::services::auth::mfa::PrimaryLoginCompletion;
use crate::services::ops::audit::{self, AuditContext, AuditRequestInfo};
use sea_orm::{ActiveValue, Set};
use serde::{Deserialize, Serialize};

use aster_drive_model::entities::user;
use aster_drive_model::types::{
    StoredUserConfig, TokenType, UserRole, UserStatus, VerificationPurpose,
};

pub use contact_verification::{
    cleanup_expired_contact_verification_tokens, confirm_contact_verification,
    confirm_password_reset, request_email_change, request_password_reset, resend_email_change,
};
pub(crate) use password::ensure_password_login_enabled;
pub(crate) use password::verify_user_password;
pub use password::{change_password, login, set_password};
pub use registration::{
    RegisterActivationResendOutcome, check_auth_state, create_user_by_admin, register,
    resend_register_activation, setup,
};
pub use session::{
    cleanup_expired_auth_sessions, get_current_auth_snapshot, list_auth_sessions,
    revoke_auth_session, revoke_other_auth_sessions, revoke_user_sessions,
};
pub use tokens::{
    authenticate_access_token, issue_tokens_for_session, issue_tokens_for_user, refresh_tokens,
    revoke_refresh_token, verify_token,
};
pub(crate) use validation::{validate_email, validate_password, validate_username};

/// Session version assigned to every freshly created account; bumped to invalidate sessions.
const INITIAL_SESSION_VERSION: i64 = 1;
/// User-facing message reused by all "another verification request is active" rejections.
const ACTIVE_VERIFICATION_REQUEST_MESSAGE: &str = "a verification request is already active";

/// Drive's request projection of Forge-verified claims, never the JWT wire format.
#[derive(Serialize, Deserialize, Clone)]
pub struct Claims {
    /// Stable Forge session identity, checked against the writer on every access authorization.
    pub sid: String,
    /// Canonical subject supplied by the verified JWT.
    pub sub: String,
    /// Drive integer account identity parsed only from `sub`.
    pub user_id: i64,
    /// Account generation checked against the writer on authenticated requests.
    pub session_version: i64,
    /// Product projection of Forge's explicit login restriction.
    pub password_change: bool,
    /// Refresh identity; absent for access credentials and omitted from Debug.
    pub jti: Option<String>,
    /// Verified credential purpose, separate from product authorization.
    pub token_type: TokenType,
    /// Exclusive verified expiry, retained for transport compatibility.
    pub exp: usize,
}

impl std::fmt::Debug for Claims {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Claims")
            .field("user_id", &self.user_id)
            .field("sid", &self.sid)
            .field("session_version", &self.session_version)
            .field("password_change", &self.password_change)
            .field("token_type", &self.token_type)
            .field("exp", &self.exp)
            .finish_non_exhaustive()
    }
}

/// Per-request account projection refreshed from the writer DB on every authorized call.
///
/// Not a JWT claim replay: [`Claims`] only carries the session reference, while authorization
/// decisions read this snapshot so disabled accounts and stale session versions fail closed.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub struct AuthSnapshot {
    /// Current account status; only the active state may authorize requests.
    pub status: UserStatus,
    /// Account role used for admin-route gating.
    pub role: UserRole,
    /// Account generation; must match the `session_version` claim or the token is stale.
    pub session_version: i64,
    /// Whether an administrator-forced password change is still outstanding.
    pub must_change_password: bool,
}

/// Whether login must steer into the forced password-change step for this account.
///
/// The flag only applies while password login is enabled; with password login disabled the
/// restriction is unenforceable and therefore not reported.
pub(crate) fn password_change_required_for_login(
    state: &impl SharedRuntimeState,
    user: &user::Model,
) -> bool {
    effective_password_change_required(password_login_enabled(state), user.must_change_password)
}

/// Same restriction as [`password_change_required_for_login`], evaluated on a token-refresh
/// [`AuthSnapshot`] instead of the login-time user row.
pub(crate) fn password_change_required_for_snapshot(
    state: &impl SharedRuntimeState,
    snapshot: AuthSnapshot,
) -> bool {
    effective_password_change_required(password_login_enabled(state), snapshot.must_change_password)
}

/// Shared rule behind both projections: an outstanding forced change counts only when password
/// login is enabled, otherwise the requirement cannot be satisfied and is suppressed.
fn effective_password_change_required(
    password_login_enabled: bool,
    must_change_password: bool,
) -> bool {
    password_login_enabled && must_change_password
}

/// Reads the current password-login policy switch from runtime configuration.
fn password_login_enabled(state: &impl SharedRuntimeState) -> bool {
    RuntimeAuthPolicy::from_runtime_config(state.runtime_config()).password_login_enabled
}

#[cfg(test)]
mod policy_tests {
    use super::effective_password_change_required;

    #[test]
    fn forced_password_change_is_ignored_when_password_login_is_disabled() {
        assert!(!effective_password_change_required(false, true));
        assert!(effective_password_change_required(true, true));
        assert!(!effective_password_change_required(true, false));
    }
}

/// Outcome of confirming a contact-verification token.
#[derive(Debug)]
pub struct ContactVerificationConfirmResult {
    /// Purpose the consumed token was issued for; drives follow-up behavior.
    pub purpose: VerificationPurpose,
    /// Account the token belonged to.
    pub user_id: i64,
    /// Address the token was issued against (the verified address after an email change).
    pub target: String,
}

/// Minimal account identity safe to embed in audit records and non-committal responses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserAuditInfo {
    /// Account primary key.
    pub id: i64,
    /// Account display name.
    pub username: String,
}

/// One browser session of the current account, as listed by the session-management UI.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct AuthSessionInfo {
    /// Session identifier (`sid` claim of its access tokens).
    pub id: String,
    /// Whether this row belongs to the session that issued the request.
    pub is_current: bool,
    /// Client IP recorded at session creation, if any.
    pub ip_address: Option<String>,
    /// Client user agent recorded at session creation, if any.
    pub user_agent: Option<String>,
    /// When the session was created.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last authenticated activity seen for this session.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub last_seen_at: chrono::DateTime<chrono::Utc>,
    /// Refresh-window deadline; the session cannot be refreshed past it.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// Full account profile returned to authenticated callers and admin user management.
///
/// Auth-related fields (email, verification, session version) always reflect the writer DB read
/// that produced this value; never reconstruct authorization decisions from a stale copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthUserInfo {
    /// Account primary key.
    pub id: i64,
    /// Unique account display name.
    pub username: String,
    /// Current email address; the pending change target lives in [`AuthUserInfo::pending_email`].
    pub email: String,
    /// Account role used for authorization.
    pub role: UserRole,
    /// Lifecycle status; only the active state may authenticate.
    pub status: UserStatus,
    /// Whether an administrator-forced password change is still outstanding.
    pub must_change_password: bool,
    /// Account generation; bumped whenever existing sessions must be invalidated.
    pub session_version: i64,
    /// When the current email was verified; `None` until account activation completes.
    pub email_verified_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Awaiting-change email address, set between requesting and confirming an email change.
    pub pending_email: Option<String>,
    /// Bytes currently attributed to the account.
    pub storage_used: i64,
    /// Byte cap enforced on uploads; negative values mean no explicit quota.
    pub storage_quota: i64,
    /// Policy group the account is bound to, if any.
    pub policy_group_id: Option<i64>,
    /// Account creation time.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last account mutation time.
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Per-account stored configuration, if present.
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

/// Outcome of a password-reset request, shaped to stay identical for existing and unknown emails.
#[derive(Debug)]
pub struct PasswordResetRequestResult {
    /// Auditable identity when the account exists and is reset-eligible; `None` otherwise so the
    /// response cannot enumerate accounts.
    pub user: Option<UserAuditInfo>,
}

/// Session material issued after a completed password login.
#[derive(Debug)]
pub struct LoginResult {
    /// Short-lived bearer token for API calls.
    pub access_token: String,
    /// Long-lived refresh token bound to the new session row.
    pub refresh_token: String,
    /// Authenticated account primary key.
    pub user_id: i64,
    /// Whether the client must complete a forced password change before further use.
    pub password_change_required: bool,
}

/// Builds a [`UserAuditInfo`] projection from a user row.
fn user_audit_info(user: &user::Model) -> UserAuditInfo {
    UserAuditInfo {
        id: user.id,
        username: user.username.clone(),
    }
}

/// Whether the account's current email address has completed verification.
pub fn is_email_verified(user: &user::Model) -> bool {
    user.email_verified_at.is_some()
}

// 审计包装收敛在聚合层，避免 registration/password/contact_verification 这些
// 纯业务子模块依赖 route 级副作用。
/// Creates the first administrator account and records a `SystemSetup` audit entry.
///
/// Thin aggregation wrapper: business rules live in [`setup`], audit side effects live here.
///
/// # Errors
/// Forwards [`setup`] failures; audit logging itself is best-effort and never fails the call.
pub async fn setup_with_audit(
    state: &impl SharedRuntimeState,
    username: &str,
    email: &str,
    password: &str,
    request_info: &AuditRequestInfo,
) -> Result<AuthUserInfo> {
    let user = setup(state, username, email, password).await?;
    let audit_ctx = request_info.to_context(user.id);
    audit::log(
        state,
        &audit_ctx,
        audit::AuditAction::SystemSetup,
        audit::AuditEntityType::User,
        None,
        Some(&user.username),
        None,
    )
    .await;
    Ok(user)
}

/// Self-service registration plus a `UserRegister` audit entry.
///
/// # Errors
/// Forwards [`register`] failures; audit logging itself is best-effort and never fails the call.
pub async fn register_with_audit(
    state: &impl SharedRuntimeState,
    username: &str,
    email: &str,
    password: &str,
    request_info: &AuditRequestInfo,
) -> Result<AuthUserInfo> {
    let user = register(state, username, email, password).await?;
    let audit_ctx = request_info.to_context(user.id);
    audit::log(
        state,
        &audit_ctx,
        audit::AuditAction::UserRegister,
        audit::AuditEntityType::User,
        None,
        Some(&user.username),
        None,
    )
    .await;
    Ok(user)
}

/// Re-sends a registration activation email, records metrics and a `UserResendRegistration`
/// audit entry when a mail was actually sent.
///
/// Skipped outcomes (`Ok(None)`) cover unknown emails, already-active accounts, disabled
/// accounts, cooldowns, and email-policy rejections; they are never surfaced as errors.
///
/// # Errors
/// Forwards persistence failures from [`resend_register_activation`].
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
        audit::log(
            state,
            &audit_ctx,
            audit::AuditAction::UserResendRegistration,
            crate::services::ops::audit::AuditEntityType::User,
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

/// Confirms a contact-verification token and logs a purpose-specific audit entry.
///
/// # Errors
/// Forwards [`confirm_contact_verification`] failures; audit logging is best-effort.
pub async fn confirm_contact_verification_with_audit(
    state: &impl SharedRuntimeState,
    token: &str,
    request_info: &AuditRequestInfo,
) -> Result<ContactVerificationConfirmResult> {
    let result = confirm_contact_verification(state, token).await?;
    let audit_ctx = request_info.to_context(result.user_id);
    let action = match result.purpose {
        VerificationPurpose::RegisterActivation => audit::AuditAction::UserConfirmRegistration,
        VerificationPurpose::ContactChange => audit::AuditAction::UserConfirmEmailChange,
        VerificationPurpose::PasswordReset => audit::AuditAction::UserConfirmPasswordReset,
    };
    audit::log(
        state,
        &audit_ctx,
        action,
        crate::services::ops::audit::AuditEntityType::User,
        Some(result.user_id),
        None,
        None,
    )
    .await;
    Ok(result)
}

/// Handles a password-reset request and audits the reset-eligible account, if any.
///
/// The audit entry fires only when [`PasswordResetRequestResult::user`] is `Some`, keeping the
/// "no such account" path free of account-specific records.
///
/// # Errors
/// Forwards [`request_password_reset`] failures; audit logging is best-effort.
pub async fn request_password_reset_with_audit(
    state: &impl SharedRuntimeState,
    email: &str,
    request_info: &AuditRequestInfo,
) -> Result<PasswordResetRequestResult> {
    let result = request_password_reset(state, email).await?;
    if let Some(user) = result.user.as_ref() {
        let audit_ctx = request_info.to_context(user.id);
        audit::log(
            state,
            &audit_ctx,
            audit::AuditAction::UserRequestPasswordReset,
            crate::services::ops::audit::AuditEntityType::User,
            Some(user.id),
            Some(&user.username),
            None,
        )
        .await;
    }
    Ok(result)
}

/// Consumes a password-reset token, rotates the password, and writes a `UserConfirmPasswordReset`
/// audit entry.
///
/// # Errors
/// Forwards [`confirm_password_reset`] failures; audit logging is best-effort.
pub async fn confirm_password_reset_with_audit(
    state: &impl SharedRuntimeState,
    token: &str,
    new_password: &str,
    request_info: &AuditRequestInfo,
) -> Result<AuthUserInfo> {
    let user = confirm_password_reset(state, token, new_password).await?;
    let audit_ctx = request_info.to_context(user.id);
    audit::log(
        state,
        &audit_ctx,
        audit::AuditAction::UserConfirmPasswordReset,
        crate::services::ops::audit::AuditEntityType::User,
        Some(user.id),
        Some(&user.username),
        None,
    )
    .await;
    Ok(user)
}

/// Verifies identifier + password, records a `UserLogin` audit entry with MFA details, and
/// returns either full session material or a pending MFA challenge.
///
/// The audit entry distinguishes direct authentication (no MFA, forced-change flag included)
/// from a started challenge (available second-factor methods listed).
///
/// # Errors
/// Forwards [`login`] failures; audit logging is best-effort.
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
            audit::details(audit::UserLoginAuditDetails {
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
            audit::details(audit::UserLoginAuditDetails {
                mfa_required: true,
                password_change_required: None,
                available_methods: Some(available_methods.as_slice()),
            })
        }),
    };
    let audit_ctx = request_info.to_context(user_id);
    audit::log_with_details(
        state,
        &audit_ctx,
        audit::AuditAction::UserLogin,
        audit::AuditEntityType::AuthSession,
        None,
        Some(identifier),
        || details.clone(),
    )
    .await;
    Ok(result)
}

/// Writes a `UserLogout` audit entry when the presented access token still verifies.
///
/// Returns `false` without logging for invalid, expired, or replayed tokens so logout of a
/// dead session stays silent rather than fabricating an audit trail.
pub async fn log_logout_for_token(
    state: &impl SharedRuntimeState,
    token: &str,
    request_info: &AuditRequestInfo,
) -> bool {
    let Ok(claims) = verify_token(token, &state.config().auth.jwt_secret) else {
        return false;
    };

    let audit_ctx = request_info.to_context(claims.user_id);
    audit::log(
        state,
        &audit_ctx,
        audit::AuditAction::UserLogout,
        audit::AuditEntityType::AuthSession,
        None,
        None,
        None,
    )
    .await;
    true
}

/// Authenticated self-service password change plus a `UserChangePassword` audit entry.
///
/// # Errors
/// Forwards [`change_password`] failures; audit logging is best-effort.
pub async fn change_password_with_audit(
    state: &impl SharedRuntimeState,
    claims: &Claims,
    current_password: &str,
    new_password: &str,
    audit_ctx: &AuditContext,
) -> Result<AuthUserInfo> {
    let user = change_password(state, claims, current_password, new_password).await?;
    audit::log(
        state,
        audit_ctx,
        audit::AuditAction::UserChangePassword,
        audit::AuditEntityType::User,
        None,
        None,
        None,
    )
    .await;
    Ok(user)
}

/// Starts an email change plus a `UserRequestEmailChange` audit entry.
///
/// # Errors
/// Forwards [`request_email_change`] failures; audit logging is best-effort.
pub async fn request_email_change_with_audit(
    state: &impl SharedRuntimeState,
    user_id: i64,
    new_email: &str,
    audit_ctx: &AuditContext,
) -> Result<AuthUserInfo> {
    let user = request_email_change(state, user_id, new_email).await?;
    audit::log(
        state,
        audit_ctx,
        audit::AuditAction::UserRequestEmailChange,
        crate::services::ops::audit::AuditEntityType::User,
        Some(user.id),
        Some(&user.username),
        None,
    )
    .await;
    Ok(user)
}

/// Re-sends an email-change confirmation plus a `UserResendEmailChange` audit entry when a mail
/// was actually sent.
///
/// # Errors
/// Forwards [`resend_email_change`] failures; audit logging is best-effort.
pub async fn resend_email_change_with_audit(
    state: &impl SharedRuntimeState,
    user_id: i64,
    audit_ctx: &AuditContext,
) -> Result<Option<UserAuditInfo>> {
    let user = resend_email_change(state, user_id).await?;
    if let Some(user) = user.as_ref() {
        audit::log(
            state,
            audit_ctx,
            audit::AuditAction::UserResendEmailChange,
            crate::services::ops::audit::AuditEntityType::User,
            Some(user.id),
            Some(&user.username),
            None,
        )
        .await;
    }
    Ok(user)
}

/// Administrator revoke-all-sessions plus an `AdminRevokeUserSessions` audit entry.
///
/// # Errors
/// Forwards [`revoke_user_sessions`] failures; audit logging is best-effort.
pub async fn revoke_user_sessions_with_audit(
    state: &impl SharedRuntimeState,
    user_id: i64,
    audit_ctx: &AuditContext,
) -> Result<UserAuditInfo> {
    let user = revoke_user_sessions(state, user_id).await?;
    audit::log(
        state,
        audit_ctx,
        audit::AuditAction::AdminRevokeUserSessions,
        crate::services::ops::audit::AuditEntityType::User,
        Some(user.id),
        Some(&user.username),
        None,
    )
    .await;
    Ok(user)
}

/// Administrator password reset plus an `AdminResetUserPassword` audit entry.
///
/// # Errors
/// Forwards [`set_password`] failures; audit logging is best-effort.
pub async fn set_password_with_audit(
    state: &impl SharedRuntimeState,
    user_id: i64,
    new_password: &str,
    audit_ctx: &AuditContext,
) -> Result<AuthUserInfo> {
    let user = set_password(state, user_id, new_password).await?;
    audit::log(
        state,
        audit_ctx,
        audit::AuditAction::AdminResetUserPassword,
        crate::services::ops::audit::AuditEntityType::User,
        Some(user.id),
        Some(&user.username),
        None,
    )
    .await;
    Ok(user)
}

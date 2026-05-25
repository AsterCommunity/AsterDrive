//! MFA 登录 flow。

use chrono::{Duration, Utc};
use sea_orm::ActiveValue::Set;
use serde::Serialize;

use crate::api::subcode::ApiSubcode;
use crate::db::repository::{
    mfa_factor_repo, mfa_login_flow_repo, mfa_totp_setup_flow_repo, user_repo,
};
use crate::entities::{mfa_login_flow, user};
use crate::errors::{AsterError, Result, auth_mfa_failed_with_subcode};
use crate::runtime::PrimaryAppState;
use crate::services::{audit_service, auth_service};
use crate::types::{MfaFactorMethod, MfaFirstFactor, MfaMethod};
use crate::utils::numbers::u64_to_i64;

use super::{MFA_LOGIN_FLOW_TTL_SECS, MFA_MAX_ATTEMPTS, crypto, recovery_codes, totp};

#[derive(Debug)]
pub enum PrimaryLoginCompletion {
    Authenticated(auth_service::LoginResult),
    MfaRequired(MfaChallengeStart),
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct MfaChallengeStart {
    #[serde(skip_serializing)]
    pub user_id: i64,
    pub flow_token: String,
    pub expires_in: u64,
    pub methods: Vec<MfaMethod>,
}

#[derive(Debug)]
pub struct MfaChallengeLoginResult {
    pub access_token: String,
    pub refresh_token: String,
    pub user_id: i64,
}

struct MfaChallengeAttempt {
    user_id: i64,
    result: Result<MfaChallengeLoginResult>,
}

pub async fn complete_primary_login_or_start_mfa(
    state: &PrimaryAppState,
    user: &user::Model,
    first_factor: MfaFirstFactor,
    return_path: Option<&str>,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<PrimaryLoginCompletion> {
    if mfa_factor_repo::find_totp_for_user(state.writer_db(), user.id)
        .await?
        .is_none()
    {
        let (access_token, refresh_token) =
            auth_service::issue_tokens_for_user(state, user, ip_address, user_agent).await?;
        return Ok(PrimaryLoginCompletion::Authenticated(
            auth_service::LoginResult {
                access_token,
                refresh_token,
                user_id: user.id,
            },
        ));
    }

    Ok(PrimaryLoginCompletion::MfaRequired(
        create_login_flow(
            state,
            user,
            first_factor,
            return_path,
            ip_address,
            user_agent,
        )
        .await?,
    ))
}

pub async fn create_login_flow(
    state: &PrimaryAppState,
    user: &user::Model,
    first_factor: MfaFirstFactor,
    return_path: Option<&str>,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<MfaChallengeStart> {
    let flow_token = format!("mfa_{}", crate::utils::id::new_short_token());
    let now = Utc::now();
    let ttl = u64_to_i64(MFA_LOGIN_FLOW_TTL_SECS, "mfa login flow ttl")?;
    mfa_login_flow_repo::create(
        state.writer_db(),
        mfa_login_flow::ActiveModel {
            flow_token_hash: Set(crypto::token_hash(&flow_token)),
            user_id: Set(user.id),
            user_session_version: Set(user.session_version),
            first_factor: Set(first_factor),
            return_path: Set(return_path.map(str::to_string)),
            ip_address: Set(ip_address.map(str::to_string)),
            user_agent: Set(user_agent.map(str::to_string)),
            attempt_count: Set(0),
            expires_at: Set(now + Duration::seconds(ttl)),
            consumed_at: Set(None),
            created_at: Set(now),
            ..Default::default()
        },
    )
    .await?;

    let mut methods = vec![MfaMethod::Totp];
    if crate::db::repository::mfa_recovery_code_repo::count_unused_for_user(
        state.writer_db(),
        user.id,
    )
    .await?
        > 0
    {
        methods.push(MfaMethod::RecoveryCode);
    }

    Ok(MfaChallengeStart {
        user_id: user.id,
        flow_token,
        expires_in: MFA_LOGIN_FLOW_TTL_SECS,
        methods,
    })
}

pub async fn cleanup_expired_flows(state: &PrimaryAppState) -> Result<u64> {
    let now = Utc::now();
    let login_flows = mfa_login_flow_repo::cleanup_expired(state.writer_db(), now).await?;
    let setup_flows = mfa_totp_setup_flow_repo::cleanup_expired(state.writer_db(), now).await?;
    Ok(login_flows + setup_flows)
}

pub async fn verify_challenge(
    state: &PrimaryAppState,
    flow_token: &str,
    method: MfaMethod,
    code: &str,
) -> Result<MfaChallengeLoginResult> {
    let normalized_flow_token = flow_token.trim();
    if normalized_flow_token.is_empty() {
        return Err(flow_invalid("missing MFA flow token"));
    }
    let now = Utc::now();
    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    let attempt = async {
        let flow = mfa_login_flow_repo::find_by_flow_token_hash(
            &txn,
            &crypto::token_hash(normalized_flow_token),
        )
        .await?
        .ok_or_else(|| flow_invalid("MFA flow is invalid"))?;
        ensure_flow_active(&flow, now)?;

        let user = user_repo::find_by_id(&txn, flow.user_id).await?;
        ensure_flow_user_valid(&user, &flow)?;
        let user_id = user.id;

        let verified = match method {
            MfaMethod::Totp if totp::looks_like_code(code) => {
                verify_totp(&txn, state, &user, code, now).await?
            }
            MfaMethod::Totp => false,
            MfaMethod::RecoveryCode if recovery_codes::looks_like_code(code) => {
                recovery_codes::verify_and_consume(&txn, user.id, code).await?
            }
            MfaMethod::RecoveryCode => false,
        };

        if !verified {
            let next_attempt_count = flow.attempt_count.saturating_add(1);
            let consume_at = (next_attempt_count >= MFA_MAX_ATTEMPTS).then_some(now);
            mfa_login_flow_repo::increment_attempts(&txn, flow.id, consume_at).await?;
            let error = if next_attempt_count >= MFA_MAX_ATTEMPTS {
                auth_mfa_failed_with_subcode(
                    ApiSubcode::AuthMfaAttemptsExceeded,
                    "MFA attempts exceeded",
                )
            } else {
                code_invalid()
            };
            return Ok::<_, AsterError>(MfaChallengeAttempt {
                user_id,
                result: Err(error),
            });
        }

        if !mfa_login_flow_repo::consume(&txn, flow.id, now).await? {
            return Err(flow_invalid("MFA flow has already been consumed"));
        }

        let (access_token, refresh_token) = auth_service::issue_tokens_for_user_in_connection(
            &txn,
            state,
            &user,
            flow.ip_address.as_deref(),
            flow.user_agent.as_deref(),
        )
        .await?;
        Ok::<_, AsterError>(MfaChallengeAttempt {
            user_id,
            result: Ok(MfaChallengeLoginResult {
                access_token,
                refresh_token,
                user_id,
            }),
        })
    }
    .await?;

    match attempt.result {
        Ok(result) => {
            crate::db::transaction::commit(txn).await?;
            let audit_ctx = audit_service::AuditContext {
                user_id: result.user_id,
                ip_address: None,
                user_agent: None,
            };
            audit_service::log(
                state,
                &audit_ctx,
                audit_service::AuditAction::UserMfaChallengeSuccess,
                audit_service::AuditEntityType::MfaFactor,
                None,
                None,
                None,
            )
            .await;
            Ok(result)
        }
        Err(error) => {
            if matches!(
                error.api_error_subcode(),
                Some(ApiSubcode::AuthMfaCodeInvalid | ApiSubcode::AuthMfaAttemptsExceeded)
            ) {
                crate::db::transaction::commit(txn).await?;
                let audit_ctx = audit_service::AuditContext {
                    user_id: attempt.user_id,
                    ip_address: None,
                    user_agent: None,
                };
                audit_service::log(
                    state,
                    &audit_ctx,
                    audit_service::AuditAction::UserMfaChallengeFailed,
                    audit_service::AuditEntityType::MfaFactor,
                    None,
                    None,
                    None,
                )
                .await;
            } else {
                crate::db::transaction::rollback(txn).await?;
            }
            Err(error)
        }
    }
}

async fn verify_totp<C: sea_orm::ConnectionTrait>(
    db: &C,
    state: &PrimaryAppState,
    user: &user::Model,
    code: &str,
    now: chrono::DateTime<Utc>,
) -> Result<bool> {
    let Some(factor) = mfa_factor_repo::find_totp_for_user(db, user.id).await? else {
        return Err(auth_mfa_failed_with_subcode(
            ApiSubcode::AuthMfaFactorRequired,
            "TOTP factor is not enabled",
        ));
    };
    let aad = crypto::factor_aad(user.id, MfaFactorMethod::Totp.as_str());
    let secret = crypto::decrypt_secret(
        &state.config.auth.mfa_secret_key,
        aad.as_bytes(),
        &factor.secret_ciphertext,
    )?;
    let verified = totp::verify_code(&secret, code, now)?;
    if verified {
        mfa_factor_repo::touch_last_used(db, factor.id, now).await?;
    }
    Ok(verified)
}

fn ensure_flow_active(flow: &mfa_login_flow::Model, now: chrono::DateTime<Utc>) -> Result<()> {
    if flow.consumed_at.is_some() {
        return Err(flow_invalid("MFA flow has already been consumed"));
    }
    if flow.expires_at <= now {
        return Err(auth_mfa_failed_with_subcode(
            ApiSubcode::AuthMfaFlowExpired,
            "MFA flow has expired",
        ));
    }
    if flow.attempt_count >= MFA_MAX_ATTEMPTS {
        return Err(auth_mfa_failed_with_subcode(
            ApiSubcode::AuthMfaAttemptsExceeded,
            "MFA attempts exceeded",
        ));
    }
    Ok(())
}

fn ensure_flow_user_valid(user: &user::Model, flow: &mfa_login_flow::Model) -> Result<()> {
    if !user.status.is_active() {
        return Err(AsterError::auth_forbidden("account is disabled"));
    }
    if !auth_service::is_email_verified(user) {
        return Err(AsterError::auth_pending_activation(
            "account pending activation",
        ));
    }
    if user.session_version != flow.user_session_version {
        return Err(flow_invalid("MFA flow session version is stale"));
    }
    Ok(())
}

fn code_invalid() -> AsterError {
    auth_mfa_failed_with_subcode(ApiSubcode::AuthMfaCodeInvalid, "invalid MFA code")
}

fn flow_invalid(message: impl Into<String>) -> AsterError {
    auth_mfa_failed_with_subcode(ApiSubcode::AuthMfaFlowInvalid, message)
}

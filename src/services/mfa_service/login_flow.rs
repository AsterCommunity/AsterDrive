//! MFA 登录 flow。

use chrono::{Duration, Utc};
use rand::RngExt;
use sea_orm::{ActiveValue::Set, ConnectionTrait};
use serde::Serialize;

use crate::api::subcode::ApiSubcode;
use crate::config::{
    auth_runtime::RuntimeEmailCodeLoginPolicy, branding, mail::RuntimeMailSettings,
};
use crate::db::repository::{
    mfa_email_code_repo, mfa_factor_repo, mfa_login_flow_repo, mfa_recovery_code_repo,
    mfa_totp_setup_flow_repo, user_repo,
};
use crate::entities::{mfa_email_code, mfa_login_flow, user};
use crate::errors::{AsterError, Result, auth_mfa_failed_with_subcode};
use crate::runtime::{MailRuntimeState, SharedRuntimeState};
use crate::services::{
    audit_service,
    audit_service::AuditRequestInfo,
    auth_service, mail_service,
    mail_template::{self, MailTemplatePayload},
};
use crate::types::{MfaFirstFactor, MfaMethod, MfaPersistentFactorMethod};
use crate::utils::hash;
use crate::utils::numbers::{i64_to_u64, u64_to_i64};

use super::{
    EMAIL_CODE_DIGITS, MFA_LOGIN_FLOW_TTL_SECS, MFA_MAX_ATTEMPTS, MfaEmailCodeSendResponse, crypto,
    recovery_codes, totp,
};

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
    state: &impl SharedRuntimeState,
    user: &user::Model,
    first_factor: MfaFirstFactor,
    return_path: Option<&str>,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<PrimaryLoginCompletion> {
    let methods = available_challenge_methods(state.writer_db(), state, user).await?;
    if methods.is_empty() {
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
            methods,
        )
        .await?,
    ))
}

pub async fn create_login_flow(
    state: &impl SharedRuntimeState,
    user: &user::Model,
    first_factor: MfaFirstFactor,
    return_path: Option<&str>,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
    methods: Vec<MfaMethod>,
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

    Ok(MfaChallengeStart {
        user_id: user.id,
        flow_token,
        expires_in: MFA_LOGIN_FLOW_TTL_SECS,
        methods,
    })
}

pub async fn cleanup_expired_flows(state: &impl SharedRuntimeState) -> Result<u64> {
    let now = Utc::now();
    let email_codes = mfa_email_code_repo::cleanup_expired(state.writer_db(), now).await?;
    let login_flows = mfa_login_flow_repo::cleanup_expired(state.writer_db(), now).await?;
    let setup_flows = mfa_totp_setup_flow_repo::cleanup_expired(state.writer_db(), now).await?;
    Ok(email_codes + login_flows + setup_flows)
}

pub async fn send_email_code(
    state: &impl MailRuntimeState,
    flow_token: &str,
    audit_info: &AuditRequestInfo,
) -> Result<MfaEmailCodeSendResponse> {
    let normalized_flow_token = flow_token.trim();
    if normalized_flow_token.is_empty() {
        return Err(flow_invalid("missing MFA flow token"));
    }

    let policy = RuntimeEmailCodeLoginPolicy::from_runtime_config(state.runtime_config());
    if !email_code_policy_ready(state, &policy) {
        return Err(AsterError::mail_not_configured(
            "email code login requires mail configuration and auth settings",
        ));
    }

    let now = Utc::now();
    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    let result = async {
        let flow = mfa_login_flow_repo::find_by_flow_token_hash(
            &txn,
            &crypto::token_hash(normalized_flow_token),
        )
        .await?
        .ok_or_else(|| flow_invalid("MFA flow is invalid"))?;
        ensure_flow_active(&flow, now)?;

        let user = user_repo::find_by_id(&txn, flow.user_id).await?;
        ensure_flow_user_valid(&user, &flow)?;
        let methods = available_challenge_methods(&txn, state, &user).await?;
        if !methods.contains(&MfaMethod::EmailCode) {
            return Err(auth_mfa_failed_with_subcode(
                ApiSubcode::AuthMfaFactorRequired,
                "email code MFA is not available for this login flow",
            ));
        }

        if let Some(latest) =
            mfa_email_code_repo::find_latest_unconsumed_for_user(&txn, user.id).await?
        {
            let cooldown = u64_to_i64(policy.resend_cooldown_secs, "email code resend cooldown")?;
            let allowed_at = latest.created_at + Duration::seconds(cooldown);
            if allowed_at > now {
                let remaining = (allowed_at - now).num_seconds().max(1);
                return Err(AsterError::rate_limited(format!(
                    "please wait {remaining} seconds before requesting another email code",
                )));
            }
        }

        mfa_email_code_repo::consume_active_for_user(&txn, user.id, now).await?;
        let remaining_flow_secs = i64_to_u64(
            (flow.expires_at - now).num_seconds().max(1),
            "remaining MFA flow lifetime",
        )?;
        let effective_expires_in = policy.ttl_secs.min(remaining_flow_secs);
        let ttl = u64_to_i64(effective_expires_in, "email code ttl")?;
        let code = generate_email_code();
        let code_hash = hash::hash_password(&code)?;
        let record = mfa_email_code_repo::create(
            &txn,
            mfa_email_code::ActiveModel {
                flow_id: Set(flow.id),
                user_id: Set(user.id),
                code_hash: Set(code_hash),
                expires_at: Set(now + Duration::seconds(ttl)),
                consumed_at: Set(None),
                created_at: Set(now),
                ..Default::default()
            },
        )
        .await?;
        Ok::<_, AsterError>((flow, user, record, effective_expires_in, code))
    }
    .await;

    let (flow, user, record, effective_expires_in, code) = match result {
        Ok(value) => {
            crate::db::transaction::commit(txn).await?;
            value
        }
        Err(error) => {
            crate::db::transaction::rollback(txn).await?;
            return Err(error);
        }
    };

    let payload = MailTemplatePayload::login_email_code(
        &user.username,
        &code,
        &branding::title_or_default(state.runtime_config()),
        &format_email_code_expires_in(effective_expires_in),
    );
    let stored = match payload.to_stored() {
        Ok(stored) => stored,
        Err(error) => {
            consume_email_code_after_mail_failure(state, record.id).await;
            return Err(error);
        }
    };
    let rendered =
        match mail_template::render(state.runtime_config(), payload.template_code(), &stored) {
            Ok(rendered) => rendered,
            Err(error) => {
                consume_email_code_after_mail_failure(state, record.id).await;
                return Err(error);
            }
        };
    if let Err(error) = mail_service::send_rendered(
        state,
        mail_service::MailRecipient {
            address: user.email.clone(),
            display_name: Some(user.username.clone()),
        },
        rendered,
    )
    .await
    {
        consume_email_code_after_mail_failure(state, record.id).await;
        return Err(error);
    }

    audit_service::log(
        state,
        &audit_info.to_context(user.id),
        audit_service::AuditAction::UserMfaEmailCodeSend,
        audit_service::AuditEntityType::MfaFactor,
        Some(flow.id),
        Some(MfaMethod::EmailCode.as_str()),
        None,
    )
    .await;

    Ok(MfaEmailCodeSendResponse {
        expires_in: effective_expires_in,
        resend_after: policy.resend_cooldown_secs,
    })
}

async fn consume_email_code_after_mail_failure(
    state: &impl SharedRuntimeState,
    record_id: i64,
) {
    if let Err(cleanup_error) =
        mfa_email_code_repo::consume(state.writer_db(), record_id, Utc::now()).await
    {
        tracing::warn!(
            mfa_email_code_id = record_id,
            error = %cleanup_error,
            "failed to consume email MFA code after mail delivery failure"
        );
    }
}

pub async fn verify_challenge(
    state: &impl SharedRuntimeState,
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
            MfaMethod::EmailCode if looks_like_email_code(code) => {
                match verify_email_code(&txn, state, &flow, &user, code, now).await {
                    Ok(verified) => verified,
                    Err(error)
                        if error.api_error_subcode()
                            == Some(ApiSubcode::AuthMfaEmailCodeExpired) =>
                    {
                        return Ok(MfaChallengeAttempt {
                            user_id,
                            result: Err(error),
                        });
                    }
                    Err(error) => return Err(error),
                }
            }
            MfaMethod::EmailCode => false,
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
                Some(
                    ApiSubcode::AuthMfaCodeInvalid
                        | ApiSubcode::AuthMfaAttemptsExceeded
                        | ApiSubcode::AuthMfaEmailCodeExpired
                )
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
    state: &impl SharedRuntimeState,
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
    let aad = crypto::factor_aad(user.id, MfaPersistentFactorMethod::Totp.as_str());
    let secret = crypto::decrypt_secret(
        &state.config().auth.mfa_secret_key,
        aad.as_bytes(),
        &factor.secret_ciphertext,
    )?;
    let verified = totp::verify_code(&secret, code, now)?;
    if verified {
        mfa_factor_repo::touch_last_used(db, factor.id, now).await?;
    }
    Ok(verified)
}

async fn verify_email_code<C: ConnectionTrait>(
    db: &C,
    state: &impl SharedRuntimeState,
    flow: &mfa_login_flow::Model,
    user: &user::Model,
    code: &str,
    now: chrono::DateTime<Utc>,
) -> Result<bool> {
    let code = code.trim();
    // 邮箱验证码是登录 challenge 方法，不是持久化 factor。
    // 每次校验前都重新确认策略可用，避免管理员关闭配置后旧 flow 继续使用 email code。
    let policy = RuntimeEmailCodeLoginPolicy::from_runtime_config(state.runtime_config());
    if !email_code_policy_ready(state, &policy) {
        return Err(auth_mfa_failed_with_subcode(
            ApiSubcode::AuthMfaFactorRequired,
            "email code MFA is not available",
        ));
    }
    let methods = available_challenge_methods(db, state, user).await?;
    if !methods.contains(&MfaMethod::EmailCode) {
        return Err(auth_mfa_failed_with_subcode(
            ApiSubcode::AuthMfaFactorRequired,
            "email code MFA is not available for this login flow",
        ));
    }

    let Some(record) =
        mfa_email_code_repo::find_latest_unconsumed_for_flow(db, flow.id, user.id).await?
    else {
        return Err(auth_mfa_failed_with_subcode(
            ApiSubcode::AuthMfaEmailCodeRequired,
            "email code has not been requested",
        ));
    };

    if record.expires_at <= now {
        mfa_email_code_repo::consume(db, record.id, now).await?;
        return Err(auth_mfa_failed_with_subcode(
            ApiSubcode::AuthMfaEmailCodeExpired,
            "email code has expired",
        ));
    }

    let verified = hash::verify_password(code, &record.code_hash)?;
    if verified {
        mfa_email_code_repo::consume(db, record.id, now).await?;
    }
    Ok(verified)
}

async fn available_challenge_methods<C: ConnectionTrait>(
    db: &C,
    state: &impl SharedRuntimeState,
    user: &user::Model,
) -> Result<Vec<MfaMethod>> {
    // Challenge method 是“这次登录可以怎么过第二步”，范围比持久化 factor 更宽。
    // TOTP 来自 `mfa_factors`；恢复码和邮箱验证码分别来自独立表/运行时策略。
    let totp_enabled = mfa_factor_repo::find_totp_for_user(db, user.id)
        .await?
        .is_some();
    let recovery_available = if totp_enabled {
        mfa_recovery_code_repo::count_unused_for_user(db, user.id).await? > 0
    } else {
        false
    };
    let policy = RuntimeEmailCodeLoginPolicy::from_runtime_config(state.runtime_config());
    let email_available = auth_service::is_email_verified(user)
        && email_code_policy_ready(state, &policy)
        && (!totp_enabled || policy.allow_totp_fallback);

    let mut methods = Vec::new();
    if totp_enabled {
        methods.push(MfaMethod::Totp);
        if recovery_available {
            methods.push(MfaMethod::RecoveryCode);
        }
    }
    if email_available {
        methods.push(MfaMethod::EmailCode);
    }
    Ok(methods)
}

fn email_code_policy_ready(
    state: &impl SharedRuntimeState,
    policy: &RuntimeEmailCodeLoginPolicy,
) -> bool {
    policy.enabled
        && RuntimeMailSettings::from_runtime_config(state.runtime_config()).is_ready_for_delivery()
}

fn generate_email_code() -> String {
    // rand::rng() returns ThreadRng, which implements CryptoRng and is seeded from the OS.
    let mut rng = rand::rng();
    (0..EMAIL_CODE_DIGITS)
        .map(|_| char::from(b'0' + rng.random_range(0_u8..10_u8)))
        .collect()
}

fn looks_like_email_code(code: &str) -> bool {
    let trimmed = code.trim();
    trimmed.len() == EMAIL_CODE_DIGITS && trimmed.bytes().all(|byte| byte.is_ascii_digit())
}

fn format_email_code_expires_in(ttl_secs: u64) -> String {
    if ttl_secs.is_multiple_of(60) {
        let minutes = ttl_secs / 60;
        if minutes == 1 {
            "1 minute".to_string()
        } else {
            format!("{minutes} minutes")
        }
    } else if ttl_secs == 1 {
        "1 second".to_string()
    } else {
        format!("{ttl_secs} seconds")
    }
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

//! MFA 登录 flow。

use aster_forge_db::transaction;
use chrono::{Duration, Utc};
use rand::RngExt;
use sea_orm::{ActiveValue::Set, ConnectionTrait};
use serde::Serialize;

use crate::api::api_error_code::ApiErrorCode;
use crate::config::{
    auth_runtime::{RuntimeAuthPolicy, RuntimeEmailCodeLoginPolicy},
    branding, mail,
};
use crate::db::repository::{
    mfa_email_code_repo, mfa_factor_repo, mfa_login_flow_repo, mfa_recovery_code_repo,
    mfa_totp_setup_flow_repo, user_repo,
};
use crate::errors::{AsterError, Result, auth_mfa_failed_with_code};
use crate::runtime::{MailRuntimeState, SharedRuntimeState};
use crate::services::auth::flow::{
    AuthFlowCommand, AuthFlowKind, AuthFlowSnapshot, AuthFlowState, AuthFlowTransitionError,
    new_primary_flow, plan_transition,
};
use crate::services::{
    auth::local,
    mail::audit as mail_audit,
    mail::sender,
    mail::template::{self, MailTemplatePayload},
    ops::audit,
    ops::audit::AuditRequestInfo,
};
use aster_drive_model::entities::{mfa_email_code, mfa_login_flow, user};
use aster_drive_model::types::{MfaFirstFactor, MfaMethod, MfaPersistentFactorMethod};
use aster_forge_utils::numbers::{i64_to_u64, u64_to_i64};

use super::{
    EMAIL_CODE_DIGITS, MFA_LOGIN_FLOW_TTL_SECS, MFA_MAX_ATTEMPTS, MfaEmailCodeSendResponse, crypto,
    recovery_codes, totp,
};

#[derive(Debug)]
pub enum PrimaryLoginCompletion {
    Authenticated(local::LoginResult),
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
    pub password_change_required: bool,
}

struct MfaChallengeAttempt {
    user_id: i64,
    flow_id: Option<i64>,
    attempt_count: Option<i32>,
    result: Result<MfaChallengeLoginResult>,
}

enum PreparedEmailCodeVerification {
    Missing,
    Expired {
        record_id: i64,
    },
    Current {
        record_id: i64,
        code_hash: String,
        is_valid: bool,
    },
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
        let password_change_required = local::password_change_required_for_login(state, user);
        let (access_token, refresh_token) = if password_change_required {
            local::issue_password_change_tokens_for_user(state, user, ip_address, user_agent)
                .await?
        } else {
            local::issue_tokens_for_user(state, user, ip_address, user_agent).await?
        };
        return Ok(PrimaryLoginCompletion::Authenticated(local::LoginResult {
            access_token,
            refresh_token,
            user_id: user.id,
            password_change_required,
        }));
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

fn ensure_first_factor_allowed(
    first_factor: MfaFirstFactor,
    password_login_enabled: bool,
) -> Result<()> {
    if first_factor != MfaFirstFactor::Password || password_login_enabled {
        return Ok(());
    }

    Err(crate::errors::auth_forbidden_with_code(
        ApiErrorCode::AuthPasswordLoginDisabled,
        "password login is disabled by administrator policy",
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
    let flow_token = format!("mfa_{}", aster_forge_utils::id::new_short_token());
    let now = Utc::now();
    let ttl = u64_to_i64(MFA_LOGIN_FLOW_TTL_SECS, "mfa login flow ttl")?;
    let primary_kind = match first_factor {
        MfaFirstFactor::Password => AuthFlowKind::PasswordLogin,
        MfaFirstFactor::ExternalAuth => AuthFlowKind::ExternalLogin,
    };
    let primary = new_primary_flow(
        primary_kind,
        format!("primary:{flow_token}"),
        now + Duration::seconds(ttl),
        now,
    )
    .map_err(|_| AsterError::auth_invalid_credentials("primary login flow is invalid"))?;
    plan_transition(
        &primary,
        AuthFlowCommand::Transition {
            expected_revision: primary.revision,
            to: AuthFlowState::SecondFactorPending,
        },
        now,
    )
    .map_err(|_| {
        AsterError::auth_invalid_credentials("primary login flow transition is invalid")
    })?;
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

    let flow_token_hash = crypto::token_hash(normalized_flow_token);
    validate_email_code_send(
        state.writer_db(),
        state,
        &flow_token_hash,
        &policy,
        Utc::now(),
    )
    .await?;

    let code = generate_email_code();
    let code_hash = state
        .runtime_config()
        .password_hash_runtime()
        .hash_password(&code)
        .await?;

    let now = Utc::now();
    let txn = transaction::begin(state.writer_db()).await?;
    let result = async {
        let (flow, user, effective_expires_in) =
            validate_email_code_send(&txn, state, &flow_token_hash, &policy, now).await?;

        mfa_email_code_repo::consume_active_for_user(&txn, user.id, now).await?;
        let ttl = u64_to_i64(effective_expires_in, "email code ttl")?;
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
        Ok::<_, AsterError>((flow, user, record, effective_expires_in))
    }
    .await;

    let (flow, user, record, effective_expires_in) = match result {
        Ok(value) => {
            transaction::commit(txn).await?;
            value
        }
        Err(error) => {
            transaction::rollback(txn).await?;
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
    let rendered = match template::render(state.runtime_config(), payload.template_code(), &stored)
    {
        Ok(rendered) => rendered,
        Err(error) => {
            consume_email_code_after_mail_failure(state, record.id).await;
            return Err(error);
        }
    };
    let template_code = payload.template_code();
    let subject = rendered.subject.clone();
    if let Err(error) = sender::send_rendered(
        state,
        aster_forge_mail::MailRecipient {
            address: user.email.clone(),
            display_name: Some(user.username.clone()),
        },
        rendered,
    )
    .await
    {
        let error_message = error.to_string();
        consume_email_code_after_mail_failure(state, record.id).await;
        mail_audit::log_delivery_failed_with_db(
            state.writer_db(),
            state.runtime_config(),
            mail_audit::MailAuditInput {
                actor_user_id: user.id,
                ip_address: audit_info.ip_address.as_deref(),
                user_agent: audit_info.user_agent.as_deref(),
                to_address: &user.email,
                to_name: Some(&user.username),
                template_code: template_code.as_str(),
                subject: Some(&subject),
                outbox_id: None,
                attempt_count: None,
                error: Some(&error_message),
            },
        )
        .await;
        return Err(error);
    }

    mail_audit::log_send(
        state,
        mail_audit::MailAuditInput {
            actor_user_id: user.id,
            ip_address: audit_info.ip_address.as_deref(),
            user_agent: audit_info.user_agent.as_deref(),
            to_address: &user.email,
            to_name: Some(&user.username),
            template_code: template_code.as_str(),
            subject: Some(&subject),
            outbox_id: None,
            attempt_count: None,
            error: None,
        },
    )
    .await;

    audit::log(
        state,
        &audit_info.to_context(user.id),
        audit::AuditAction::UserMfaEmailCodeSend,
        audit::AuditEntityType::MfaFactor,
        Some(flow.id),
        Some(MfaMethod::EmailCode.as_str()),
        audit::details(audit::MfaEmailCodeAuditDetails {
            method: MfaMethod::EmailCode,
            flow_id: flow.id,
            expires_in: effective_expires_in,
            resend_after: policy.resend_cooldown_secs,
        }),
    )
    .await;

    Ok(MfaEmailCodeSendResponse {
        expires_in: effective_expires_in,
        resend_after: policy.resend_cooldown_secs,
    })
}

async fn consume_email_code_after_mail_failure(state: &impl SharedRuntimeState, record_id: i64) {
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

async fn validate_email_code_send<C: ConnectionTrait>(
    db: &C,
    state: &impl SharedRuntimeState,
    flow_token_hash: &str,
    policy: &RuntimeEmailCodeLoginPolicy,
    now: chrono::DateTime<Utc>,
) -> Result<(mfa_login_flow::Model, user::Model, u64)> {
    let flow = mfa_login_flow_repo::find_by_flow_token_hash(db, flow_token_hash)
        .await?
        .ok_or_else(|| flow_invalid("MFA flow is invalid"))?;
    ensure_flow_active(&flow, now)?;

    let user = user_repo::find_by_id(db, flow.user_id).await?;
    ensure_flow_user_valid(&user, &flow)?;
    let methods = available_challenge_methods(db, state, &user).await?;
    if !methods.contains(&MfaMethod::EmailCode) {
        return Err(auth_mfa_failed_with_code(
            ApiErrorCode::AuthMfaFactorRequired,
            "email code MFA is not available for this login flow",
        ));
    }

    if let Some(latest) = mfa_email_code_repo::find_latest_unconsumed_for_user(db, user.id).await? {
        let cooldown = u64_to_i64(policy.resend_cooldown_secs, "email code resend cooldown")?;
        let allowed_at = latest.created_at + Duration::seconds(cooldown);
        if allowed_at > now {
            let remaining = (allowed_at - now).num_seconds().max(1);
            return Err(AsterError::rate_limited(format!(
                "please wait {remaining} seconds before requesting another email code",
            )));
        }
    }

    let remaining_flow_secs = i64_to_u64(
        (flow.expires_at - now).num_seconds().max(1),
        "remaining MFA flow lifetime",
    )?;
    let effective_expires_in = policy.ttl_secs.min(remaining_flow_secs);
    Ok((flow, user, effective_expires_in))
}

pub async fn verify_challenge(
    state: &impl SharedRuntimeState,
    flow_token: &str,
    method: MfaMethod,
    code: &str,
    audit_info: &AuditRequestInfo,
) -> Result<MfaChallengeLoginResult> {
    let normalized_flow_token = flow_token.trim();
    if normalized_flow_token.is_empty() {
        return Err(flow_invalid("missing MFA flow token"));
    }

    let flow_token_hash = crypto::token_hash(normalized_flow_token);
    let preflight_now = Utc::now();
    let preflight_flow =
        load_active_flow(state.writer_db(), &flow_token_hash, preflight_now).await?;
    ensure_first_factor_allowed(
        preflight_flow.first_factor,
        RuntimeAuthPolicy::from_runtime_config(state.runtime_config()).password_login_enabled,
    )?;
    let preflight_user = user_repo::find_by_id(state.writer_db(), preflight_flow.user_id).await?;
    ensure_flow_user_valid(&preflight_user, &preflight_flow)?;
    let prepared_recovery =
        if method == MfaMethod::RecoveryCode && recovery_codes::looks_like_code(code) {
            recovery_codes::verify(state, state.writer_db(), preflight_user.id, code).await?
        } else {
            None
        };
    let prepared_email = if method == MfaMethod::EmailCode && looks_like_email_code(code) {
        prepare_email_code_verification(
            state.writer_db(),
            state,
            &preflight_flow,
            &preflight_user,
            code,
            preflight_now,
        )
        .await?
    } else {
        PreparedEmailCodeVerification::Missing
    };

    let now = Utc::now();
    let txn = transaction::begin(state.writer_db()).await?;
    let attempt = async {
        let flow = load_active_flow(&txn, &flow_token_hash, now).await?;
        ensure_first_factor_allowed(
            flow.first_factor,
            RuntimeAuthPolicy::from_runtime_config(state.runtime_config()).password_login_enabled,
        )?;
        let user = user_repo::find_by_id(&txn, flow.user_id).await?;
        ensure_flow_user_valid(&user, &flow)?;
        let user_id = user.id;

        let verified = match method {
            MfaMethod::Totp if totp::looks_like_code(code) => {
                verify_totp(&txn, state, &user, code, now).await?
            }
            MfaMethod::Totp => false,
            MfaMethod::RecoveryCode if recovery_codes::looks_like_code(code) => {
                if let Some(verified) = prepared_recovery.as_ref() {
                    recovery_codes::consume_verified(&txn, user.id, verified).await?
                } else {
                    false
                }
            }
            MfaMethod::RecoveryCode => false,
            MfaMethod::EmailCode if looks_like_email_code(code) => {
                match apply_email_code_verification(&txn, state, &flow, &user, &prepared_email, now)
                    .await
                {
                    Ok(verified) => verified,
                    Err(error)
                        if error.api_error_code_override()
                            == Some(ApiErrorCode::AuthMfaEmailCodeExpired) =>
                    {
                        return Ok(MfaChallengeAttempt {
                            user_id,
                            flow_id: Some(flow.id),
                            attempt_count: Some(flow.attempt_count),
                            result: Err(error),
                        });
                    }
                    Err(error) => return Err(error),
                }
            }
            MfaMethod::EmailCode => false,
        };

        if !verified {
            let transition = plan_transition(
                &mfa_flow_snapshot(&flow),
                AuthFlowCommand::RecordFailure {
                    expected_revision: 0,
                },
                now,
            )
            .map_err(map_mfa_flow_transition_error)?;
            let next_attempt_count = i32::try_from(transition.attempt_count)
                .map_err(|_| AsterError::internal_error("MFA attempt count overflow"))?;
            let consume_at = (transition.state == AuthFlowState::Failed).then_some(now);
            mfa_login_flow_repo::increment_attempts(&txn, flow.id, consume_at).await?;
            let error = if next_attempt_count >= MFA_MAX_ATTEMPTS {
                auth_mfa_failed_with_code(
                    ApiErrorCode::AuthMfaAttemptsExceeded,
                    "MFA attempts exceeded",
                )
            } else {
                code_invalid()
            };
            return Ok::<_, AsterError>(MfaChallengeAttempt {
                user_id,
                flow_id: Some(flow.id),
                attempt_count: Some(next_attempt_count),
                result: Err(error),
            });
        }

        if !mfa_login_flow_repo::consume(&txn, flow.id, now).await? {
            return Err(flow_invalid("MFA flow has already been consumed"));
        }

        let password_change_required = local::password_change_required_for_login(state, &user);
        let (access_token, refresh_token) = if password_change_required {
            local::issue_password_change_tokens_for_user(
                state,
                &user,
                flow.ip_address.as_deref(),
                flow.user_agent.as_deref(),
            )
            .await?
        } else {
            local::issue_tokens_for_user_in_connection(
                &txn,
                state,
                &user,
                flow.ip_address.as_deref(),
                flow.user_agent.as_deref(),
            )
            .await?
        };
        Ok::<_, AsterError>(MfaChallengeAttempt {
            user_id,
            flow_id: Some(flow.id),
            attempt_count: Some(flow.attempt_count),
            result: Ok(MfaChallengeLoginResult {
                access_token,
                refresh_token,
                user_id,
                password_change_required,
            }),
        })
    }
    .await?;

    match attempt.result {
        Ok(result) => {
            transaction::commit(txn).await?;
            let audit_ctx = audit_info.to_context(result.user_id);
            let details = audit::details(audit::MfaChallengeAuditDetails {
                method,
                flow_id: attempt.flow_id,
                attempt_count: attempt.attempt_count,
                password_change_required: Some(result.password_change_required),
                failure_reason: None,
            });
            audit::log_with_details(
                state,
                &audit_ctx,
                audit::AuditAction::UserMfaChallengeSuccess,
                audit::AuditEntityType::MfaFactor,
                None,
                Some(method.as_str()),
                || details.clone(),
            )
            .await;
            Ok(result)
        }
        Err(error) => {
            if matches!(
                error.api_error_code_override(),
                Some(
                    ApiErrorCode::AuthMfaCodeInvalid
                        | ApiErrorCode::AuthMfaAttemptsExceeded
                        | ApiErrorCode::AuthMfaEmailCodeExpired
                )
            ) {
                transaction::commit(txn).await?;
                let audit_ctx = audit_info.to_context(attempt.user_id);
                let failure_reason = error
                    .api_error_code_override()
                    .map(|code| code.as_str())
                    .unwrap_or("mfa_failed");
                let details = audit::details(audit::MfaChallengeAuditDetails {
                    method,
                    flow_id: attempt.flow_id,
                    attempt_count: attempt.attempt_count,
                    password_change_required: None,
                    failure_reason: Some(failure_reason),
                });
                audit::log_with_details(
                    state,
                    &audit_ctx,
                    audit::AuditAction::UserMfaChallengeFailed,
                    audit::AuditEntityType::MfaFactor,
                    None,
                    Some(method.as_str()),
                    || details.clone(),
                )
                .await;
            } else {
                transaction::rollback(txn).await?;
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
        return Err(auth_mfa_failed_with_code(
            ApiErrorCode::AuthMfaFactorRequired,
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

async fn load_active_flow<C: ConnectionTrait>(
    db: &C,
    flow_token_hash: &str,
    now: chrono::DateTime<Utc>,
) -> Result<mfa_login_flow::Model> {
    let flow = mfa_login_flow_repo::find_by_flow_token_hash(db, flow_token_hash)
        .await?
        .ok_or_else(|| flow_invalid("MFA flow is invalid"))?;
    ensure_flow_active(&flow, now)?;
    Ok(flow)
}

async fn prepare_email_code_verification<C: ConnectionTrait>(
    db: &C,
    state: &impl SharedRuntimeState,
    flow: &mfa_login_flow::Model,
    user: &user::Model,
    code: &str,
    now: chrono::DateTime<Utc>,
) -> Result<PreparedEmailCodeVerification> {
    let code = code.trim();
    ensure_email_code_method_available(db, state, user).await?;

    let Some(record) =
        mfa_email_code_repo::find_latest_unconsumed_for_flow(db, flow.id, user.id).await?
    else {
        return Ok(PreparedEmailCodeVerification::Missing);
    };

    if record.expires_at <= now {
        return Ok(PreparedEmailCodeVerification::Expired {
            record_id: record.id,
        });
    }

    let is_valid = state
        .runtime_config()
        .password_hash_runtime()
        .verify_password(code, &record.code_hash)
        .await?
        .is_valid;

    Ok(PreparedEmailCodeVerification::Current {
        record_id: record.id,
        code_hash: record.code_hash,
        is_valid,
    })
}

async fn apply_email_code_verification<C: ConnectionTrait>(
    db: &C,
    state: &impl SharedRuntimeState,
    flow: &mfa_login_flow::Model,
    user: &user::Model,
    prepared: &PreparedEmailCodeVerification,
    now: chrono::DateTime<Utc>,
) -> Result<bool> {
    ensure_email_code_method_available(db, state, user).await?;

    let Some(record) =
        mfa_email_code_repo::find_latest_unconsumed_for_flow(db, flow.id, user.id).await?
    else {
        return Err(auth_mfa_failed_with_code(
            ApiErrorCode::AuthMfaEmailCodeRequired,
            "email code has not been requested",
        ));
    };

    if record.expires_at <= now {
        mfa_email_code_repo::consume(db, record.id, now).await?;
        return Err(auth_mfa_failed_with_code(
            ApiErrorCode::AuthMfaEmailCodeExpired,
            "email code has expired",
        ));
    }

    match prepared {
        PreparedEmailCodeVerification::Missing => Ok(false),
        PreparedEmailCodeVerification::Expired { record_id } => {
            if record.id == *record_id {
                mfa_email_code_repo::consume(db, record.id, now).await?;
                Err(auth_mfa_failed_with_code(
                    ApiErrorCode::AuthMfaEmailCodeExpired,
                    "email code has expired",
                ))
            } else {
                Ok(false)
            }
        }
        PreparedEmailCodeVerification::Current {
            record_id,
            code_hash,
            is_valid,
        } if record.id == *record_id && record.code_hash == *code_hash => {
            if *is_valid {
                mfa_email_code_repo::consume(db, record.id, now).await
            } else {
                Ok(false)
            }
        }
        PreparedEmailCodeVerification::Current { .. } => Ok(false),
    }
}

async fn ensure_email_code_method_available<C: ConnectionTrait>(
    db: &C,
    state: &impl SharedRuntimeState,
    user: &user::Model,
) -> Result<()> {
    // 邮箱验证码是登录 challenge 方法，不是持久化 factor。
    // 每次校验前都重新确认策略可用，避免管理员关闭配置后旧 flow 继续使用 email code。
    let policy = RuntimeEmailCodeLoginPolicy::from_runtime_config(state.runtime_config());
    if !email_code_policy_ready(state, &policy) {
        return Err(auth_mfa_failed_with_code(
            ApiErrorCode::AuthMfaFactorRequired,
            "email code MFA is not available",
        ));
    }
    let methods = available_challenge_methods(db, state, user).await?;
    if !methods.contains(&MfaMethod::EmailCode) {
        return Err(auth_mfa_failed_with_code(
            ApiErrorCode::AuthMfaFactorRequired,
            "email code MFA is not available for this login flow",
        ));
    }
    Ok(())
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
    let email_available = local::is_email_verified(user)
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
    policy.enabled && mail::runtime_mail_settings(state.runtime_config()).is_ready_for_delivery()
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
    plan_transition(
        &mfa_flow_snapshot(flow),
        AuthFlowCommand::Transition {
            expected_revision: 0,
            to: AuthFlowState::Authenticated,
        },
        now,
    )
    .map(|_| ())
    .map_err(map_mfa_flow_transition_error)
}

fn mfa_flow_snapshot(flow: &mfa_login_flow::Model) -> AuthFlowSnapshot {
    AuthFlowSnapshot {
        flow_id: format!("mfa-login:{}", flow.id),
        kind: AuthFlowKind::MfaLogin,
        state: if flow.consumed_at.is_some() {
            AuthFlowState::Consumed
        } else {
            AuthFlowState::SecondFactorPending
        },
        revision: 0,
        attempt_count: u32::try_from(flow.attempt_count).unwrap_or(u32::MAX),
        max_attempts: Some(u32::try_from(MFA_MAX_ATTEMPTS).expect("positive MFA attempt limit")),
        expires_at: flow.expires_at,
    }
}

fn map_mfa_flow_transition_error(error: AuthFlowTransitionError) -> AsterError {
    match error {
        AuthFlowTransitionError::Expired => {
            auth_mfa_failed_with_code(ApiErrorCode::AuthMfaFlowExpired, "MFA flow has expired")
        }
        AuthFlowTransitionError::AttemptBudgetExhausted
        | AuthFlowTransitionError::Terminal(AuthFlowState::Failed) => auth_mfa_failed_with_code(
            ApiErrorCode::AuthMfaAttemptsExceeded,
            "MFA attempts exceeded",
        ),
        AuthFlowTransitionError::Terminal(_) => flow_invalid("MFA flow has already been consumed"),
        AuthFlowTransitionError::IllegalTransition { .. }
        | AuthFlowTransitionError::RevisionConflict { .. } => {
            flow_invalid("MFA flow transition is invalid")
        }
    }
}

fn ensure_flow_user_valid(user: &user::Model, flow: &mfa_login_flow::Model) -> Result<()> {
    if !user.status.is_active() {
        return Err(AsterError::auth_forbidden("account is disabled"));
    }
    if !local::is_email_verified(user) {
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
    auth_mfa_failed_with_code(ApiErrorCode::AuthMfaCodeInvalid, "invalid MFA code")
}

fn flow_invalid(message: impl Into<String>) -> AsterError {
    auth_mfa_failed_with_code(ApiErrorCode::AuthMfaFlowInvalid, message)
}

#[cfg(test)]
mod policy_tests {
    use super::ensure_first_factor_allowed;
    use crate::api::api_error_code::ApiErrorCode;
    use aster_drive_model::types::MfaFirstFactor;

    #[test]
    fn password_first_factor_is_rejected_when_disabled() {
        let error = ensure_first_factor_allowed(MfaFirstFactor::Password, false)
            .expect_err("password MFA flow must be rejected after policy is disabled");
        assert_eq!(
            error.api_error_code(),
            ApiErrorCode::AuthPasswordLoginDisabled
        );
    }

    #[test]
    fn external_first_factor_remains_allowed_when_password_is_disabled() {
        assert!(ensure_first_factor_allowed(MfaFirstFactor::ExternalAuth, false).is_ok());
    }
}

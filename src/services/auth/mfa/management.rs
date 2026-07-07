//! MFA 自助管理与管理员重置。

use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, IntoActiveModel};
use serde::{Deserialize, Serialize};

use crate::api::api_error_code::ApiErrorCode;
use crate::config::branding;
use crate::db::repository::{
    mfa_email_code_repo, mfa_factor_repo, mfa_login_flow_repo, mfa_recovery_code_repo,
    mfa_totp_setup_flow_repo, user_repo,
};
use crate::entities::{mfa_factor, mfa_totp_setup_flow};
use crate::errors::{AsterError, Result, auth_forbidden_with_code, auth_mfa_failed_with_code};
use crate::runtime::SharedRuntimeState;
use crate::services::{auth::local, ops::audit};
use crate::types::MfaPersistentFactorMethod;
use crate::utils::numbers::u64_to_i64;

use super::{
    MFA_SETUP_FLOW_TTL_SECS, crypto, now_utc, persistent_factor_method_label, recovery_codes, totp,
};

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct MfaStatus {
    pub enabled: bool,
    pub factors: Vec<MfaFactorInfo>,
    pub recovery_codes_remaining: u64,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct MfaFactorInfo {
    pub id: i64,
    /// 只返回长期持久化 factor。恢复码和邮箱验证码不会出现在 MFA factor 列表里。
    pub method: MfaPersistentFactorMethod,
    pub name: String,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub enabled_at: chrono::DateTime<Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub last_used_at: Option<chrono::DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct TotpSetupStartResponse {
    pub flow_token: String,
    pub expires_in: u64,
    pub secret: String,
    pub otpauth_uri: String,
}

#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct TotpSetupFinishRequest {
    pub flow_token: String,
    pub code: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct TotpSetupFinishResponse {
    pub factor: MfaFactorInfo,
    pub recovery_codes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct MfaSensitiveActionRequest {
    pub code: Option<String>,
}

pub async fn get_status(state: &impl SharedRuntimeState, user_id: i64) -> Result<MfaStatus> {
    let factors = mfa_factor_repo::list_for_user(state.writer_db(), user_id)
        .await?
        .into_iter()
        .map(factor_info)
        .collect::<Vec<_>>();
    let recovery_codes_remaining =
        mfa_recovery_code_repo::count_unused_for_user(state.writer_db(), user_id).await?;
    Ok(MfaStatus {
        enabled: !factors.is_empty(),
        factors,
        recovery_codes_remaining,
    })
}

pub async fn start_totp_setup(
    state: &impl SharedRuntimeState,
    user_id: i64,
) -> Result<TotpSetupStartResponse> {
    let user = user_repo::find_by_id(state.writer_db(), user_id).await?;
    if !user.status.is_active() {
        return Err(AsterError::auth_forbidden("account is disabled"));
    }
    if mfa_factor_repo::find_totp_for_user(state.writer_db(), user_id)
        .await?
        .is_some()
    {
        return Err(auth_forbidden_with_code(
            ApiErrorCode::AuthMfaFactorAlreadyExists,
            "TOTP MFA is already enabled",
        ));
    }

    let secret = totp::generate_secret();
    let secret_base32 = totp::encode_secret(&secret);
    let flow_token = format!("mfs_{}", crate::utils::id::new_short_token());
    let aad = crypto::setup_flow_aad(user_id);
    let encrypted =
        crypto::encrypt_secret(&state.config().auth.mfa_secret_key, aad.as_bytes(), &secret)?;
    let now = now_utc();
    let ttl = u64_to_i64(MFA_SETUP_FLOW_TTL_SECS, "mfa setup flow ttl")?;
    mfa_totp_setup_flow_repo::create(
        state.writer_db(),
        mfa_totp_setup_flow::ActiveModel {
            flow_token_hash: Set(crypto::token_hash(&flow_token)),
            user_id: Set(user_id),
            secret_ciphertext: Set(encrypted),
            secret_version: Set(1),
            expires_at: Set(now + Duration::seconds(ttl)),
            consumed_at: Set(None),
            created_at: Set(now),
            ..Default::default()
        },
    )
    .await?;

    let issuer = branding::title_or_default(state.runtime_config());
    let account = if user.email.is_empty() {
        user.username.as_str()
    } else {
        user.email.as_str()
    };
    Ok(TotpSetupStartResponse {
        flow_token,
        expires_in: MFA_SETUP_FLOW_TTL_SECS,
        otpauth_uri: totp::otpauth_uri(&secret_base32, &issuer, account),
        secret: secret_base32,
    })
}

pub async fn verify_totp_setup(
    state: &impl SharedRuntimeState,
    user_id: i64,
    input: TotpSetupFinishRequest,
    audit_ctx: &audit::AuditContext,
) -> Result<TotpSetupFinishResponse> {
    let now = now_utc();
    let flow_token_hash = crypto::token_hash(input.flow_token.trim());
    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    let result = async {
        let user = user_repo::find_by_id(&txn, user_id).await?;
        ensure_user_can_manage_mfa(&user)?;
        if mfa_factor_repo::find_totp_for_user(&txn, user_id)
            .await?
            .is_some()
        {
            return Err(auth_forbidden_with_code(
                ApiErrorCode::AuthMfaFactorAlreadyExists,
                "TOTP MFA is already enabled",
            ));
        }
        let flow =
            mfa_totp_setup_flow_repo::find_active_by_flow_token_hash(&txn, &flow_token_hash, now)
                .await?
                .ok_or_else(|| AsterError::auth_token_invalid("TOTP setup flow is invalid"))?;
        if flow.user_id != user_id {
            return Err(AsterError::auth_forbidden(
                "TOTP setup flow does not belong to user",
            ));
        }
        let aad = crypto::setup_flow_aad(user_id);
        let secret = crypto::decrypt_secret(
            &state.config().auth.mfa_secret_key,
            aad.as_bytes(),
            &flow.secret_ciphertext,
        )?;
        if !totp::verify_code(&secret, &input.code, now)? {
            return Err(auth_mfa_failed_with_code(
                ApiErrorCode::AuthMfaCodeInvalid,
                "invalid TOTP code",
            ));
        }
        if !mfa_totp_setup_flow_repo::consume(&txn, flow.id, now).await? {
            return Err(AsterError::auth_token_invalid(
                "TOTP setup flow has already been consumed",
            ));
        }

        let factor_aad = crypto::factor_aad(user_id, MfaPersistentFactorMethod::Totp.as_str());
        let encrypted_secret = crypto::encrypt_secret(
            &state.config().auth.mfa_secret_key,
            factor_aad.as_bytes(),
            &secret,
        )?;
        let name = input
            .name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Authenticator app")
            .to_string();
        let factor = mfa_factor_repo::create(
            &txn,
            mfa_factor::ActiveModel {
                user_id: Set(user_id),
                method: Set(MfaPersistentFactorMethod::Totp),
                name: Set(name),
                secret_ciphertext: Set(encrypted_secret),
                secret_version: Set(1),
                enabled_at: Set(now),
                last_used_at: Set(None),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .await?;
        let recovery_codes = recovery_codes::replace_for_user(&txn, user_id).await?;
        Ok::<_, AsterError>((factor_info(factor), recovery_codes))
    }
    .await;

    match result {
        Ok((factor, recovery_codes)) => {
            crate::db::transaction::commit(txn).await?;
            audit::log(
                state,
                audit_ctx,
                audit::AuditAction::UserMfaEnable,
                audit::AuditEntityType::MfaFactor,
                Some(factor.id),
                Some(&factor.name),
                audit::details(audit::UserMfaManageAuditDetails {
                    method: factor.method,
                    factor_id: Some(factor.id),
                    factor_name: Some(&factor.name),
                    factor_count: None,
                    recovery_code_count: Some(recovery_codes.len()),
                }),
            )
            .await;
            Ok(TotpSetupFinishResponse {
                factor,
                recovery_codes,
            })
        }
        Err(error) => {
            crate::db::transaction::rollback(txn).await?;
            Err(error)
        }
    }
}

pub async fn delete_factor(
    state: &impl SharedRuntimeState,
    user_id: i64,
    factor_id: i64,
    input: MfaSensitiveActionRequest,
    audit_ctx: &audit::AuditContext,
) -> Result<bool> {
    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    let result = async {
        let user = user_repo::find_by_id(&txn, user_id).await?;
        ensure_user_can_manage_mfa(&user)?;
        verify_sensitive_mfa_code(&txn, state, user_id, input.code.as_deref()).await?;
        let factor = mfa_factor_repo::find_by_id_for_user(&txn, factor_id, user_id)
            .await?
            .ok_or_else(|| AsterError::record_not_found(format!("MFA factor #{factor_id}")))?;
        mfa_recovery_code_repo::delete_all_for_user(&txn, user_id).await?;
        mfa_email_code_repo::delete_all_for_user(&txn, user_id).await?;
        mfa_login_flow_repo::delete_all_for_user(&txn, user_id).await?;
        mfa_totp_setup_flow_repo::delete_all_for_user(&txn, user_id).await?;
        let deleted = mfa_factor_repo::delete_for_user(&txn, factor_id, user_id).await?;
        Ok::<_, AsterError>((deleted, factor_info(factor)))
    }
    .await;
    match result {
        Ok((deleted, factor)) => {
            crate::db::transaction::commit(txn).await?;
            if deleted {
                audit::log_with_details(
                    state,
                    audit_ctx,
                    audit::AuditAction::UserMfaDisable,
                    audit::AuditEntityType::MfaFactor,
                    Some(factor_id),
                    Some(&factor.name),
                    || {
                        audit::details(audit::UserMfaManageAuditDetails {
                            method: factor.method,
                            factor_id: Some(factor.id),
                            factor_name: Some(&factor.name),
                            factor_count: None,
                            recovery_code_count: None,
                        })
                    },
                )
                .await;
            }
            Ok(deleted)
        }
        Err(error) => {
            crate::db::transaction::rollback(txn).await?;
            Err(error)
        }
    }
}

pub async fn regenerate_recovery_codes(
    state: &impl SharedRuntimeState,
    user_id: i64,
    input: MfaSensitiveActionRequest,
    audit_ctx: &audit::AuditContext,
) -> Result<Vec<String>> {
    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    let result = async {
        let user = user_repo::find_by_id(&txn, user_id).await?;
        ensure_user_can_manage_mfa(&user)?;
        verify_sensitive_mfa_code(&txn, state, user_id, input.code.as_deref()).await?;
        recovery_codes::replace_for_user(&txn, user_id).await
    }
    .await;
    match result {
        Ok(codes) => {
            crate::db::transaction::commit(txn).await?;
            audit::log(
                state,
                audit_ctx,
                audit::AuditAction::UserMfaRecoveryCodesRegenerate,
                audit::AuditEntityType::MfaFactor,
                None,
                None,
                audit::details(audit::UserMfaManageAuditDetails {
                    method: MfaPersistentFactorMethod::Totp,
                    factor_id: None,
                    factor_name: None,
                    factor_count: None,
                    recovery_code_count: Some(codes.len()),
                }),
            )
            .await;
            Ok(codes)
        }
        Err(error) => {
            crate::db::transaction::rollback(txn).await?;
            Err(error)
        }
    }
}

pub async fn reset_user_mfa(
    state: &impl SharedRuntimeState,
    user_id: i64,
    audit_ctx: &audit::AuditContext,
) -> Result<()> {
    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    let result = async {
        let user = user_repo::find_by_id(&txn, user_id).await?;
        let next_session_version = user.session_version.saturating_add(1);
        let factor_count = mfa_factor_repo::list_for_user(&txn, user_id).await?.len();
        let recovery_code_count = mfa_recovery_code_repo::list_unused_for_user(&txn, user_id)
            .await?
            .len();
        mfa_factor_repo::delete_all_for_user(&txn, user_id).await?;
        mfa_recovery_code_repo::delete_all_for_user(&txn, user_id).await?;
        mfa_email_code_repo::delete_all_for_user(&txn, user_id).await?;
        mfa_login_flow_repo::delete_all_for_user(&txn, user_id).await?;
        mfa_totp_setup_flow_repo::delete_all_for_user(&txn, user_id).await?;
        crate::db::repository::auth_session_repo::delete_all_for_user(&txn, user_id).await?;
        let username = user.username.clone();
        let mut active = user.into_active_model();
        active.session_version = Set(next_session_version);
        active.updated_at = Set(now_utc());
        let updated = active.update(&txn).await.map_err(AsterError::from)?;
        Ok::<_, AsterError>((updated.id, username, factor_count, recovery_code_count))
    }
    .await;
    match result {
        Ok((updated_user_id, username, factor_count, recovery_code_count)) => {
            crate::db::transaction::commit(txn).await?;
            local::invalidate_auth_snapshot_cache(state, updated_user_id).await;
            audit::log(
                state,
                audit_ctx,
                audit::AuditAction::AdminResetUserMfa,
                audit::AuditEntityType::MfaFactor,
                None,
                Some(&username),
                audit::details(audit::UserMfaManageAuditDetails {
                    method: MfaPersistentFactorMethod::Totp,
                    factor_id: None,
                    factor_name: None,
                    factor_count: Some(factor_count),
                    recovery_code_count: Some(recovery_code_count),
                }),
            )
            .await;
            Ok(())
        }
        Err(error) => {
            crate::db::transaction::rollback(txn).await?;
            Err(error)
        }
    }
}

async fn verify_sensitive_mfa_code<C: sea_orm::ConnectionTrait>(
    db: &C,
    state: &impl SharedRuntimeState,
    user_id: i64,
    code: Option<&str>,
) -> Result<()> {
    let Some(code) = code.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err(auth_mfa_failed_with_code(
            ApiErrorCode::AuthMfaCodeInvalid,
            "MFA code is required",
        ));
    };
    if looks_like_totp_code(code)
        && let Some(factor) = mfa_factor_repo::find_totp_for_user(db, user_id).await?
    {
        let aad = crypto::factor_aad(user_id, persistent_factor_method_label(factor.method));
        let secret = crypto::decrypt_secret(
            &state.config().auth.mfa_secret_key,
            aad.as_bytes(),
            &factor.secret_ciphertext,
        )?;
        if totp::verify_code(&secret, code, now_utc())? {
            return Ok(());
        }
    }
    if looks_like_recovery_code(code)
        && recovery_codes::verify_and_consume(db, user_id, code).await?
    {
        return Ok(());
    }
    Err(auth_mfa_failed_with_code(
        ApiErrorCode::AuthMfaCodeInvalid,
        "invalid MFA code",
    ))
}

fn looks_like_totp_code(code: &str) -> bool {
    totp::looks_like_code(code)
}

fn looks_like_recovery_code(code: &str) -> bool {
    recovery_codes::looks_like_code(code)
}

fn ensure_user_can_manage_mfa(user: &crate::entities::user::Model) -> Result<()> {
    if !user.status.is_active() {
        return Err(AsterError::auth_forbidden("account is disabled"));
    }
    if !local::is_email_verified(user) {
        return Err(AsterError::auth_pending_activation(
            "account pending activation",
        ));
    }
    Ok(())
}

fn factor_info(model: mfa_factor::Model) -> MfaFactorInfo {
    MfaFactorInfo {
        id: model.id,
        method: model.method,
        name: model.name,
        enabled_at: model.enabled_at,
        last_used_at: model.last_used_at,
    }
}

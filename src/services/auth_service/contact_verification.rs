//! 认证服务子模块：`contact_verification`。

use chrono::Utc;
use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};

use crate::config::branding;
use crate::db::repository::{contact_verification_token_repo, user_repo};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::PrimaryAppState;
use crate::services::{mail_outbox_service, mail_template::MailTemplatePayload};
use crate::types::VerificationPurpose;
use crate::utils::hash;

use super::session::invalidate_auth_snapshot_cache;
use super::shared::{
    ensure_email_available, ensure_resend_allowed, is_active_verification_request_error,
    issue_contact_verification_token, map_user_email_db_err, password_reset_request_allowed,
    resend_allowed, update_password_in_connection,
};
use super::validation::{normalize_email, validate_password};
use super::{
    AuthUserInfo, ContactVerificationConfirmResult, PasswordResetRequestResult, UserAuditInfo,
    is_email_verified, user_audit_info,
};

pub async fn request_email_change(
    state: &PrimaryAppState,
    user_id: i64,
    new_email: &str,
) -> Result<AuthUserInfo> {
    tracing::debug!(user_id, "requesting email change");
    let normalized_email = normalize_email(new_email)?;
    let existing = user_repo::find_by_id(&state.db, user_id).await?;

    if !existing.status.is_active() {
        return Err(AsterError::auth_forbidden("account is disabled"));
    }
    if !is_email_verified(&existing) {
        return Err(AsterError::auth_pending_activation(
            "account must be activated before changing email",
        ));
    }
    if existing.email == normalized_email {
        return Err(AsterError::validation_error(
            "new email must be different from current email",
        ));
    }

    ensure_email_available(&state.db, &normalized_email, Some(existing.id)).await?;
    if existing.pending_email.as_deref() == Some(normalized_email.as_str()) {
        ensure_resend_allowed(
            state,
            &state.db,
            existing.id,
            VerificationPurpose::ContactChange,
        )
        .await?;
    }

    let policy = crate::config::auth_runtime::RuntimeContactVerificationPolicy::from_runtime_config(
        &state.runtime_config,
    );
    let site_name = branding::title_or_default(&state.runtime_config);
    let txn = crate::db::transaction::begin(&state.db).await?;
    let mut active = existing.into_active_model();
    active.pending_email = Set(Some(normalized_email.clone()));
    active.updated_at = Set(Utc::now());
    let updated = active.update(&txn).await.map_err(map_user_email_db_err)?;
    let token = issue_contact_verification_token(
        &txn,
        updated.id,
        VerificationPurpose::ContactChange,
        &normalized_email,
        policy.contact_change_ttl_secs,
    )
    .await?;
    mail_outbox_service::enqueue(
        &txn,
        &normalized_email,
        Some(&updated.username),
        MailTemplatePayload::contact_change_confirmation(&updated.username, &token, &site_name),
    )
    .await?;
    crate::db::transaction::commit(txn).await?;

    tracing::debug!(
        user_id = updated.id,
        has_pending_email = updated.pending_email.is_some(),
        "requested email change"
    );
    Ok(AuthUserInfo::from(updated))
}

pub async fn resend_email_change(
    state: &PrimaryAppState,
    user_id: i64,
) -> Result<Option<UserAuditInfo>> {
    let user = user_repo::find_by_id(&state.db, user_id).await?;
    let pending_email = user
        .pending_email
        .clone()
        .ok_or_else(|| AsterError::validation_error("no pending email change request"))?;

    if !user.status.is_active() {
        return Err(AsterError::auth_forbidden("account is disabled"));
    }
    if !is_email_verified(&user) {
        return Err(AsterError::auth_pending_activation(
            "account must be activated before changing email",
        ));
    }

    ensure_email_available(&state.db, &pending_email, Some(user.id)).await?;
    if !resend_allowed(
        state,
        &state.db,
        user.id,
        VerificationPurpose::ContactChange,
    )
    .await?
    {
        tracing::debug!(
            user_id = user.id,
            "email change resend skipped due to cooldown"
        );
        return Ok(None);
    }
    let policy = crate::config::auth_runtime::RuntimeContactVerificationPolicy::from_runtime_config(
        &state.runtime_config,
    );
    let site_name = branding::title_or_default(&state.runtime_config);

    let txn = crate::db::transaction::begin(&state.db).await?;
    let token = match issue_contact_verification_token(
        &txn,
        user.id,
        VerificationPurpose::ContactChange,
        &pending_email,
        policy.contact_change_ttl_secs,
    )
    .await
    {
        Ok(token) => token,
        Err(err) if is_active_verification_request_error(&err) => return Ok(None),
        Err(err) => return Err(err),
    };
    mail_outbox_service::enqueue(
        &txn,
        &pending_email,
        Some(&user.username),
        MailTemplatePayload::contact_change_confirmation(&user.username, &token, &site_name),
    )
    .await?;
    crate::db::transaction::commit(txn).await?;

    Ok(Some(user_audit_info(&user)))
}

pub async fn request_password_reset(
    state: &PrimaryAppState,
    email: &str,
) -> Result<PasswordResetRequestResult> {
    tracing::debug!("requesting password reset");
    let normalized_email = normalize_email(email)?;
    let Some(user) = user_repo::find_by_email(&state.db, &normalized_email).await? else {
        return Ok(PasswordResetRequestResult { user: None });
    };

    if !user.status.is_active() || !is_email_verified(&user) {
        return Ok(PasswordResetRequestResult { user: None });
    }

    if !password_reset_request_allowed(state, &state.db, user.id).await? {
        tracing::debug!(
            user_id = user.id,
            "password reset request skipped due to cooldown"
        );
        return Ok(PasswordResetRequestResult {
            user: Some(user_audit_info(&user)),
        });
    }

    let policy = crate::config::auth_runtime::RuntimeContactVerificationPolicy::from_runtime_config(
        &state.runtime_config,
    );
    let site_name = branding::title_or_default(&state.runtime_config);
    let txn = crate::db::transaction::begin(&state.db).await?;
    let token = match issue_contact_verification_token(
        &txn,
        user.id,
        VerificationPurpose::PasswordReset,
        &user.email,
        policy.password_reset_ttl_secs,
    )
    .await
    {
        Ok(token) => token,
        Err(err) if is_active_verification_request_error(&err) => {
            return Ok(PasswordResetRequestResult {
                user: Some(user_audit_info(&user)),
            });
        }
        Err(err) => return Err(err),
    };
    mail_outbox_service::enqueue(
        &txn,
        &user.email,
        Some(&user.username),
        MailTemplatePayload::password_reset(&user.username, &token, &site_name),
    )
    .await?;
    crate::db::transaction::commit(txn).await?;

    tracing::debug!(user_id = user.id, "enqueued password reset");
    Ok(PasswordResetRequestResult {
        user: Some(user_audit_info(&user)),
    })
}

pub async fn confirm_password_reset(
    state: &PrimaryAppState,
    token: &str,
    new_password: &str,
) -> Result<AuthUserInfo> {
    tracing::debug!("confirming password reset");
    validate_password(new_password)?;

    let token_hash = hash::sha256_hex(token.as_bytes());
    let record = contact_verification_token_repo::find_by_token_hash(&state.db, &token_hash)
        .await?
        .ok_or_else(|| {
            AsterError::contact_verification_invalid("password reset link is invalid")
        })?;

    if record.purpose != VerificationPurpose::PasswordReset {
        return Err(AsterError::contact_verification_invalid(
            "password reset link is invalid",
        ));
    }
    if record.consumed_at.is_some() {
        return Err(AsterError::contact_verification_invalid(
            "password reset link has already been used",
        ));
    }
    if record.expires_at <= Utc::now() {
        return Err(AsterError::contact_verification_expired(
            "password reset link has expired",
        ));
    }

    let txn = crate::db::transaction::begin(&state.db).await?;
    let existing_user = user_repo::find_by_id(&txn, record.user_id).await?;
    if !existing_user.status.is_active() {
        return Err(AsterError::auth_forbidden("account is disabled"));
    }
    if !is_email_verified(&existing_user) || existing_user.email != record.target {
        return Err(AsterError::contact_verification_invalid(
            "password reset request no longer exists",
        ));
    }

    let consumed =
        contact_verification_token_repo::mark_consumed_if_unused(&txn, record.id).await?;
    if !consumed {
        return Err(AsterError::contact_verification_invalid(
            "password reset link has already been used",
        ));
    }

    let updated = update_password_in_connection(&txn, existing_user, new_password).await?;
    let site_name = branding::title_or_default(&state.runtime_config);
    mail_outbox_service::enqueue(
        &txn,
        &updated.email,
        Some(&updated.username),
        MailTemplatePayload::password_reset_notice(&updated.username, &site_name),
    )
    .await?;
    crate::db::transaction::commit(txn).await?;
    invalidate_auth_snapshot_cache(state, updated.id).await;
    tracing::debug!(
        user_id = updated.id,
        session_version = updated.session_version,
        "confirmed password reset"
    );
    Ok(AuthUserInfo::from(updated))
}

pub async fn confirm_contact_verification(
    state: &PrimaryAppState,
    token: &str,
) -> Result<ContactVerificationConfirmResult> {
    tracing::debug!("confirming contact verification");
    let token_hash = hash::sha256_hex(token.as_bytes());
    let record = contact_verification_token_repo::find_by_token_hash(&state.db, &token_hash)
        .await?
        .ok_or_else(|| {
            AsterError::contact_verification_invalid("contact verification link is invalid")
        })?;

    if record.consumed_at.is_some() {
        return Err(AsterError::contact_verification_invalid(
            "contact verification link has already been used",
        ));
    }
    if record.expires_at <= Utc::now() {
        return Err(AsterError::contact_verification_expired(
            "contact verification link has expired",
        ));
    }

    let target = record.target.clone();
    let purpose = record.purpose;
    let user_id = record.user_id;
    tracing::debug!(user_id, purpose = ?purpose, "loaded contact verification record");

    let txn = crate::db::transaction::begin(&state.db).await?;
    let existing_user = user_repo::find_by_id(&txn, user_id).await?;
    if !existing_user.status.is_active() {
        return Err(AsterError::auth_forbidden("account is disabled"));
    }
    let username = existing_user.username.clone();
    let site_name = branding::title_or_default(&state.runtime_config);
    let previous_email = (purpose == VerificationPurpose::ContactChange
        && existing_user.email != target)
        .then(|| existing_user.email.clone());
    if purpose == VerificationPurpose::PasswordReset {
        return Err(AsterError::contact_verification_invalid(
            "password reset token cannot be confirmed from this endpoint",
        ));
    }

    let consumed =
        contact_verification_token_repo::mark_consumed_if_unused(&txn, record.id).await?;
    if !consumed {
        return Err(AsterError::contact_verification_invalid(
            "contact verification link has already been used",
        ));
    }

    let now = Utc::now();
    match purpose {
        VerificationPurpose::RegisterActivation => {
            if existing_user.email != target {
                return Err(AsterError::contact_verification_invalid(
                    "contact verification target mismatch",
                ));
            }

            if !is_email_verified(&existing_user) {
                let mut active = existing_user.into_active_model();
                active.email_verified_at = Set(Some(now));
                active.updated_at = Set(now);
                active
                    .update(&txn)
                    .await
                    .map_aster_err(AsterError::database_operation)?;
            }
        }
        VerificationPurpose::ContactChange => {
            if existing_user.email != target
                && existing_user.pending_email.as_deref() != Some(target.as_str())
            {
                return Err(AsterError::contact_verification_invalid(
                    "contact change request no longer exists",
                ));
            }

            ensure_email_available(&txn, &target, Some(existing_user.id)).await?;

            if existing_user.email != target {
                let mut active = existing_user.into_active_model();
                active.email = Set(target.clone());
                active.pending_email = Set(None);
                active.email_verified_at = Set(Some(now));
                active.updated_at = Set(now);
                active.update(&txn).await.map_err(map_user_email_db_err)?;
                if let Some(previous_email) = previous_email.as_deref() {
                    mail_outbox_service::enqueue(
                        &txn,
                        previous_email,
                        Some(&username),
                        MailTemplatePayload::contact_change_notice(
                            &username,
                            previous_email,
                            &target,
                            &site_name,
                        ),
                    )
                    .await?;
                }
            }
        }
        VerificationPurpose::PasswordReset => {
            return Err(AsterError::contact_verification_invalid(
                "password reset token cannot be confirmed from this endpoint",
            ));
        }
    }
    crate::db::transaction::commit(txn).await?;

    tracing::debug!(user_id, purpose = ?purpose, "confirmed contact verification");
    Ok(ContactVerificationConfirmResult {
        purpose,
        user_id,
        target,
    })
}

pub async fn cleanup_expired_contact_verification_tokens(state: &PrimaryAppState) -> Result<u64> {
    contact_verification_token_repo::delete_expired(&state.db).await
}

use chrono::{Duration, Utc};
use sea_orm::ActiveValue::Set;

use crate::api::subcode::ApiSubcode;
use crate::config::{auth_runtime::RuntimeAuthPolicy, branding};
use crate::db::repository::{
    external_auth_email_verification_flow_repo, external_auth_provider_repo, user_repo,
};
use crate::entities::{external_auth_email_verification_flow, external_auth_provider};
use crate::errors::{AsterError, Result, auth_forbidden_with_subcode};
use crate::runtime::SharedRuntimeState;
use crate::services::{mail_outbox_service, mail_service, mail_template::MailTemplatePayload};
use crate::utils::numbers::u64_to_i64;

use super::normalize::{
    email_domain_allowed, normalize_email_for_external_auth, normalize_flow_token, token_hash,
};
use super::resolution::{
    ExternalAuthUserClaims, claims_with_verified_local_email,
    resolve_external_auth_user_with_verified_email,
};
use super::{
    EMAIL_VERIFICATION_FLOW_TTL_SECS, ExternalAuthEmailVerificationConfirmResult,
    ExternalAuthEmailVerificationStartRequest, ExternalAuthEmailVerificationStartResponse,
    ExternalAuthPrimaryLogin, PendingExternalAuthEmailVerification,
};

fn format_mail_duration_seconds(total_secs: i64) -> String {
    let total_secs = total_secs.max(1);
    let (value, unit) = if total_secs >= 86_400 && total_secs % 86_400 == 0 {
        (total_secs / 86_400, "day")
    } else if total_secs >= 3_600 && total_secs % 3_600 == 0 {
        (total_secs / 3_600, "hour")
    } else if total_secs >= 60 {
        ((total_secs + 59) / 60, "minute")
    } else {
        (total_secs, "second")
    };
    let suffix = if value == 1 { "" } else { "s" };
    format!("{value} {unit}{suffix}")
}

pub(super) async fn create_pending_email_verification_flow(
    state: &impl SharedRuntimeState,
    provider: &external_auth_provider::Model,
    claims: &ExternalAuthUserClaims,
    return_path: Option<String>,
) -> Result<PendingExternalAuthEmailVerification> {
    let flow_token = format!("oev_{}", crate::utils::id::new_short_token());
    let now = Utc::now();
    let ttl = u64_to_i64(
        EMAIL_VERIFICATION_FLOW_TTL_SECS,
        "external auth email verification flow ttl",
    )?;
    external_auth_email_verification_flow_repo::create(
        state.writer_db(),
        external_auth_email_verification_flow::ActiveModel {
            provider_id: Set(provider.id),
            identity_namespace: Set(claims.identity_namespace.clone()),
            subject: Set(claims.subject.clone()),
            target_email: Set(None),
            display_name_snapshot: Set(claims.display_name.clone()),
            preferred_username_snapshot: Set(claims.preferred_username.clone()),
            return_path: Set(return_path.clone()),
            flow_token_hash: Set(token_hash(&flow_token)),
            verification_token_hash: Set(None),
            email_requested_at: Set(None),
            created_at: Set(now),
            expires_at: Set(now + Duration::seconds(ttl)),
            consumed_at: Set(None),
            ..Default::default()
        },
    )
    .await?;

    Ok(PendingExternalAuthEmailVerification {
        flow_token,
        return_path: return_path.unwrap_or_else(|| "/".to_string()),
    })
}

pub async fn start_email_verification(
    state: &impl SharedRuntimeState,
    input: ExternalAuthEmailVerificationStartRequest,
) -> Result<ExternalAuthEmailVerificationStartResponse> {
    let flow_token = normalize_flow_token(&input.flow_token)?;
    let email = normalize_email_for_external_auth(&input.email)?;
    let now = Utc::now();
    let flow = external_auth_email_verification_flow_repo::find_active_by_flow_token_hash(
        state.writer_db(),
        &token_hash(&flow_token),
        now,
    )
    .await?
    .ok_or_else(|| {
        AsterError::contact_verification_invalid("external auth email verification flow is invalid")
    })?;
    if flow.verification_token_hash.is_some() {
        return Err(AsterError::contact_verification_invalid(
            "external auth email verification request has already been started",
        ));
    }

    let provider =
        external_auth_provider_repo::find_by_id(state.writer_db(), flow.provider_id).await?;
    if !provider.enabled {
        return Err(AsterError::auth_forbidden(
            "external auth provider is disabled",
        ));
    }
    if !email_domain_allowed(&provider, &email)? {
        return Err(AsterError::auth_forbidden(
            "external auth email domain is not allowed for this provider",
        ));
    }

    match user_repo::find_by_email(state.writer_db(), &email).await? {
        Some(user) => {
            if !user.status.is_active() {
                return Err(AsterError::auth_forbidden("account is disabled"));
            }
            if user.email_verified_at.is_none() {
                return Err(AsterError::auth_forbidden(
                    "local account email is not verified",
                ));
            }
        }
        None => {
            let auth_policy = RuntimeAuthPolicy::from_runtime_config(state.runtime_config());
            if !auth_policy.allow_user_registration {
                return Err(auth_forbidden_with_subcode(
                    ApiSubcode::AuthRegistrationDisabled,
                    "new user registration is disabled",
                ));
            }
        }
    }

    let verification_token = mail_service::build_verification_token();
    let verification_token_hash = token_hash(&verification_token);
    let provider_name = provider.display_name.clone();
    let site_name = branding::title_or_default(state.runtime_config());
    let expires_in = format_mail_duration_seconds((flow.expires_at - now).num_seconds());
    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    let result = async {
        external_auth_email_verification_flow_repo::update_email_request(
            &txn,
            flow,
            &email,
            &verification_token_hash,
            now,
        )
        .await?
        .then_some(())
        .ok_or_else(|| {
            AsterError::contact_verification_invalid(
                "external auth email verification request has already been started",
            )
        })?;
        mail_outbox_service::enqueue(
            &txn,
            &email,
            None,
            MailTemplatePayload::external_auth_email_verification(
                &email,
                &verification_token,
                &provider_name,
                &site_name,
                &expires_in,
            ),
        )
        .await?;
        Ok(())
    }
    .await;

    match result {
        Ok(()) => {
            crate::db::transaction::commit(txn).await?;
            Ok(ExternalAuthEmailVerificationStartResponse {
                message: "external auth email verification email sent".to_string(),
            })
        }
        Err(error) => Err(error),
    }
}

pub async fn confirm_email_verification(
    state: &impl SharedRuntimeState,
    token: &str,
    _ip_address: Option<&str>,
    _user_agent: Option<&str>,
) -> Result<ExternalAuthEmailVerificationConfirmResult> {
    let token = token.trim();
    if token.is_empty() {
        return Err(AsterError::contact_verification_invalid(
            "external auth email verification token is missing",
        ));
    }
    let now = Utc::now();
    let flow = external_auth_email_verification_flow_repo::find_active_by_verification_token_hash(
        state.writer_db(),
        &token_hash(token),
        now,
    )
    .await?
    .ok_or_else(|| {
        AsterError::contact_verification_invalid("external auth email verification link is invalid")
    })?;
    let email = flow.target_email.clone().ok_or_else(|| {
        AsterError::contact_verification_invalid(
            "external auth email verification target is missing",
        )
    })?;
    let provider =
        external_auth_provider_repo::find_by_id(state.writer_db(), flow.provider_id).await?;
    if !provider.enabled {
        return Err(AsterError::auth_forbidden(
            "external auth provider is disabled",
        ));
    }

    let claims = claims_with_verified_local_email(&flow, &email);
    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    let result = async {
        let consumed =
            external_auth_email_verification_flow_repo::mark_consumed_if_unused(&txn, flow.id, now)
                .await?;
        if !consumed {
            return Err(AsterError::contact_verification_invalid(
                "external auth email verification link has already been used",
            ));
        }
        resolve_external_auth_user_with_verified_email(&txn, state, &provider, &claims, now).await
    }
    .await;

    let resolved = match result {
        Ok(resolved) => {
            crate::db::transaction::commit(txn).await?;
            if resolved.auto_provisioned
                && let Some(policy_group_id) = resolved.user.policy_group_id
            {
                state
                    .policy_snapshot()
                    .set_user_policy_group(resolved.user.id, policy_group_id);
            }
            resolved
        }
        Err(error) => return Err(error),
    };
    Ok(ExternalAuthEmailVerificationConfirmResult {
        primary_login: ExternalAuthPrimaryLogin {
            user: resolved.user,
            return_path: flow.return_path.unwrap_or_else(|| "/".to_string()),
            provider_key: provider.key,
            issuer: claims.identity_namespace,
            subject: claims.subject,
            linked: resolved.linked,
            auto_provisioned: resolved.auto_provisioned,
        },
    })
}

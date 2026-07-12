//! 用户邀请注册服务。

use aster_forge_db::transaction;
use chrono::{DateTime, Duration, Utc};
use sea_orm::{ActiveValue::Set, ConnectionTrait};
use serde::Serialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::api::api_error_code::ApiErrorCode;
use crate::config::{auth_runtime, branding, local_email_policy::LocalEmailPolicy, site_url};
use crate::db::repository::{user_invitation_repo, user_repo};
use crate::entities::{user, user_invitation};
use crate::errors::{AsterError, Result, validation_error_with_code};
use crate::runtime::{MailRuntimeState, SharedRuntimeState};
use crate::services::{
    auth::local::shared::{CreateUserWithRoleInput, create_user_with_role},
    mail::outbox,
    mail::template::MailTemplatePayload,
};
use crate::types::{UserInvitationStatus, UserRole, UserStatus};
use crate::utils::{hash, id, numbers::u64_to_i64};
use aster_forge_api::OffsetPage;

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AdminUserInvitationInfo {
    pub id: i64,
    pub email: String,
    pub status: UserInvitationStatus,
    pub invited_by: i64,
    pub accepted_user_id: Option<i64>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: DateTime<Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTime<Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTime<Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub accepted_at: Option<DateTime<Utc>>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub revoked_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invitation_url: Option<String>,
    pub mail_queued: bool,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PublicUserInvitationInfo {
    pub email: String,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: DateTime<Utc>,
}

pub async fn create_invitation(
    state: &impl MailRuntimeState,
    email: &str,
    invited_by: i64,
) -> Result<AdminUserInvitationInfo> {
    let email = crate::utils::email::normalize_email(email)?;
    LocalEmailPolicy::from_runtime_config(state.runtime_config()).check(&email)?;

    let token = id::new_short_token();
    let token_hash = hash::sha256_hex(token.as_bytes());
    let now = Utc::now();
    let invitation_ttl_secs = u64_to_i64(
        auth_runtime::user_invitation_ttl_secs(state.runtime_config()),
        "user invitation ttl",
    )?;
    let expires_at = now + Duration::seconds(invitation_ttl_secs);
    let invitation_url = invitation_url(state.runtime_config(), &token);
    let expires_in = format_mail_duration_seconds(invitation_ttl_secs);
    let site_name = branding::title_or_default(state.runtime_config());

    let txn = transaction::begin(state.writer_db()).await?;
    let result = async {
        ensure_email_available(&txn, &email).await?;
        for existing in user_invitation_repo::find_pending_by_email(&txn, &email).await? {
            user_invitation_repo::mark_revoked_if_pending(&txn, existing.id).await?;
        }

        let invitation = user_invitation_repo::create(
            &txn,
            user_invitation::ActiveModel {
                email: Set(email.clone()),
                token_hash: Set(token_hash),
                status: Set(UserInvitationStatus::Pending),
                invited_by: Set(invited_by),
                accepted_user_id: Set(None),
                expires_at: Set(expires_at),
                created_at: Set(now),
                updated_at: Set(now),
                accepted_at: Set(None),
                revoked_at: Set(None),
                ..Default::default()
            },
        )
        .await?;

        outbox::enqueue(
            &txn,
            &email,
            None,
            MailTemplatePayload::user_invitation(&email, &invitation_url, &site_name, &expires_in),
        )
        .await?;

        Ok::<_, AsterError>(invitation)
    }
    .await;
    let invitation = match result {
        Ok(invitation) => {
            transaction::commit(txn).await?;
            invitation
        }
        Err(error) => {
            transaction::rollback(txn).await?;
            return Err(error);
        }
    };

    Ok(to_admin_info(invitation, Some(invitation_url), true))
}

pub async fn list_invitations(
    state: &impl SharedRuntimeState,
    limit: u64,
    offset: u64,
) -> Result<OffsetPage<AdminUserInvitationInfo>> {
    let (items, total) = user_invitation_repo::list(state.writer_db(), limit, offset).await?;
    let mut infos = Vec::with_capacity(items.len());
    for item in items {
        let invitation = invitation_list_view(item);
        infos.push(to_admin_info(invitation, None, false));
    }
    Ok(OffsetPage::new(infos, total, limit, offset))
}

pub async fn revoke_invitation(
    state: &impl SharedRuntimeState,
    id: i64,
) -> Result<AdminUserInvitationInfo> {
    let invitation = user_invitation_repo::find_by_id(state.writer_db(), id).await?;
    let invitation = refresh_expired_status(state.writer_db(), invitation).await?;
    if invitation.status != UserInvitationStatus::Pending {
        return Err(invitation_status_error(invitation.status));
    }

    if !user_invitation_repo::mark_revoked_if_pending(state.writer_db(), invitation.id).await? {
        return Err(current_invitation_status_error(state.writer_db(), invitation.id).await);
    }

    let invitation = user_invitation_repo::find_by_id(state.writer_db(), id).await?;
    Ok(to_admin_info(invitation, None, false))
}

pub async fn verify_public_invitation(
    state: &impl SharedRuntimeState,
    token: &str,
) -> Result<PublicUserInvitationInfo> {
    let invitation = find_valid_invitation_by_token(state.writer_db(), token).await?;
    Ok(PublicUserInvitationInfo {
        email: invitation.email,
        expires_at: invitation.expires_at,
    })
}

pub async fn accept_invitation(
    state: &impl SharedRuntimeState,
    token: &str,
    username: &str,
    password: &str,
) -> Result<user::Model> {
    let token_hash = invitation_token_hash(token)?;
    let txn = transaction::begin(state.writer_db()).await?;
    let result = async {
        let Some(invitation) = user_invitation_repo::find_by_token_hash(&txn, &token_hash).await?
        else {
            return Err(invitation_invalid_error());
        };
        let invitation = refresh_expired_status(&txn, invitation).await?;
        ensure_invitation_pending(&invitation)?;
        ensure_invitation_not_expired(&txn, &invitation).await?;
        LocalEmailPolicy::from_runtime_config(state.runtime_config()).check(&invitation.email)?;
        ensure_email_available(&txn, &invitation.email).await?;

        let user = create_user_with_role(
            &txn,
            state,
            CreateUserWithRoleInput {
                username,
                email: &invitation.email,
                password,
                role: UserRole::User,
                status: UserStatus::Active,
                must_change_password: false,
                email_verified_at: Some(Utc::now()),
            },
        )
        .await?;
        if !user_invitation_repo::mark_accepted_if_pending(&txn, invitation.id, user.id).await? {
            return Err(current_invitation_status_error(&txn, invitation.id).await);
        }
        Ok::<_, AsterError>(user)
    }
    .await;
    let user = match result {
        Ok(user) => {
            transaction::commit(txn).await?;
            user
        }
        Err(error) => {
            transaction::rollback(txn).await?;
            return Err(error);
        }
    };

    if let Some(policy_group_id) = user.policy_group_id {
        state
            .policy_snapshot()
            .set_user_policy_group(user.id, policy_group_id);
    }
    Ok(user)
}

async fn find_valid_invitation_by_token<C: ConnectionTrait>(
    db: &C,
    token: &str,
) -> Result<user_invitation::Model> {
    let token_hash = invitation_token_hash(token)?;
    let Some(invitation) = user_invitation_repo::find_by_token_hash(db, &token_hash).await? else {
        return Err(invitation_invalid_error());
    };
    let invitation = refresh_expired_status(db, invitation).await?;
    ensure_invitation_pending(&invitation)?;
    ensure_invitation_not_expired(db, &invitation).await?;
    Ok(invitation)
}

async fn refresh_expired_status<C: ConnectionTrait>(
    db: &C,
    mut invitation: user_invitation::Model,
) -> Result<user_invitation::Model> {
    if invitation.status == UserInvitationStatus::Pending && invitation.expires_at <= Utc::now() {
        if !user_invitation_repo::mark_expired_if_pending(db, invitation.id).await? {
            return user_invitation_repo::find_by_id(db, invitation.id).await;
        }
        invitation.status = UserInvitationStatus::Expired;
        invitation.updated_at = Utc::now();
    }
    Ok(invitation)
}

fn invitation_list_view(mut invitation: user_invitation::Model) -> user_invitation::Model {
    if invitation.status == UserInvitationStatus::Pending && invitation.expires_at <= Utc::now() {
        invitation.status = UserInvitationStatus::Expired;
    }
    invitation
}

async fn ensure_invitation_not_expired<C: ConnectionTrait>(
    db: &C,
    invitation: &user_invitation::Model,
) -> Result<()> {
    if invitation.expires_at > Utc::now() {
        return Ok(());
    }
    if !user_invitation_repo::mark_expired_if_pending(db, invitation.id).await? {
        return Err(current_invitation_status_error(db, invitation.id).await);
    }
    Err(invitation_status_error(UserInvitationStatus::Expired))
}

fn ensure_invitation_pending(invitation: &user_invitation::Model) -> Result<()> {
    if invitation.status.is_pending() {
        Ok(())
    } else {
        Err(invitation_status_error(invitation.status))
    }
}

async fn ensure_email_available<C: ConnectionTrait>(db: &C, email: &str) -> Result<()> {
    if user_repo::find_by_email(db, email).await?.is_some()
        || user_repo::find_by_pending_email(db, email).await?.is_some()
    {
        return Err(validation_error_with_code(
            ApiErrorCode::AuthEmailExists,
            "email already exists",
        ));
    }
    Ok(())
}

fn invitation_token_hash(token: &str) -> Result<String> {
    let token = token.trim();
    if token.is_empty() {
        return Err(invitation_invalid_error());
    }
    Ok(hash::sha256_hex(token.as_bytes()))
}

fn invitation_url(runtime_config: &crate::config::RuntimeConfig, token: &str) -> String {
    site_url::public_app_url_or_path(runtime_config, &format!("/invite/{token}"))
}

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

fn to_admin_info(
    invitation: user_invitation::Model,
    invitation_url: Option<String>,
    mail_queued: bool,
) -> AdminUserInvitationInfo {
    AdminUserInvitationInfo {
        id: invitation.id,
        email: invitation.email,
        status: invitation.status,
        invited_by: invitation.invited_by,
        accepted_user_id: invitation.accepted_user_id,
        expires_at: invitation.expires_at,
        created_at: invitation.created_at,
        updated_at: invitation.updated_at,
        accepted_at: invitation.accepted_at,
        revoked_at: invitation.revoked_at,
        invitation_url,
        mail_queued,
    }
}

fn invitation_invalid_error() -> AsterError {
    validation_error_with_code(
        ApiErrorCode::AuthInvitationInvalid,
        "invitation token is invalid",
    )
}

fn invitation_status_error(status: UserInvitationStatus) -> AsterError {
    let (code, message) = match status {
        UserInvitationStatus::Pending => (
            ApiErrorCode::AuthInvitationInvalid,
            "invitation is not usable",
        ),
        UserInvitationStatus::Accepted => (
            ApiErrorCode::AuthInvitationAccepted,
            "invitation has already been accepted",
        ),
        UserInvitationStatus::Expired => (
            ApiErrorCode::AuthInvitationExpired,
            "invitation has expired",
        ),
        UserInvitationStatus::Revoked => (
            ApiErrorCode::AuthInvitationRevoked,
            "invitation has been revoked",
        ),
    };
    validation_error_with_code(code, message)
}

async fn current_invitation_status_error<C: ConnectionTrait>(db: &C, id: i64) -> AsterError {
    match user_invitation_repo::find_by_id(db, id).await {
        Ok(invitation) => invitation_status_error(invitation.status),
        Err(error) => error,
    }
}

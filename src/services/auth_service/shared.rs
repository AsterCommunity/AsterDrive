//! 认证服务子模块：`shared`。

use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ConnectionTrait, DbErr, EntityTrait, IntoActiveModel, Iterable, Set, SqlErr,
    TryInsertResult, sea_query::OnConflict,
};

use crate::api::subcode::ApiSubcode;
use crate::config::auth_runtime::RuntimeContactVerificationPolicy;
use crate::db::repository::{contact_verification_token_repo, user_repo};
use crate::entities::{contact_verification_token, user};
use crate::errors::{AsterError, MapAsterErr, Result, validation_error_with_subcode};
use crate::runtime::SharedRuntimeState;
use crate::services::mail_service;
use crate::types::{UserRole, UserStatus, VerificationChannel, VerificationPurpose};
use crate::utils::hash;
use crate::utils::numbers::u64_to_i64;

use super::validation::{normalize_email, normalize_username, validate_password};
use super::{ACTIVE_VERIFICATION_REQUEST_MESSAGE, INITIAL_SESSION_VERSION};

fn is_unique_conflict_db_err(err: &DbErr) -> bool {
    matches!(err.sql_err(), Some(SqlErr::UniqueConstraintViolation(_)))
}

fn username_exists_error() -> AsterError {
    validation_error_with_subcode(ApiSubcode::AuthUsernameExists, "username already exists")
}

fn email_exists_error() -> AsterError {
    validation_error_with_subcode(ApiSubcode::AuthEmailExists, "email already exists")
}

fn identifier_exists_error() -> AsterError {
    validation_error_with_subcode(
        ApiSubcode::AuthIdentifierExists,
        "username or email already exists",
    )
}

async fn map_user_create_db_err<C: ConnectionTrait>(
    db: &C,
    err: DbErr,
    username: &str,
    email: &str,
) -> AsterError {
    if !is_unique_conflict_db_err(&err) {
        return AsterError::from(err);
    }

    map_user_create_conflict(db, username, email).await
}

async fn map_user_create_conflict<C: ConnectionTrait>(
    db: &C,
    username: &str,
    email: &str,
) -> AsterError {
    if user_repo::find_by_username(db, username)
        .await
        .ok()
        .flatten()
        .is_some()
    {
        return username_exists_error();
    }

    if user_repo::find_by_email(db, email)
        .await
        .ok()
        .flatten()
        .is_some()
        || user_repo::find_by_pending_email(db, email)
            .await
            .ok()
            .flatten()
            .is_some()
    {
        return email_exists_error();
    }

    identifier_exists_error()
}

async fn insert_user_with_conflict_marker<C: ConnectionTrait>(
    db: &C,
    model: user::ActiveModel,
) -> std::result::Result<Option<user::Model>, DbErr> {
    let mut on_conflict = OnConflict::new();
    on_conflict.do_nothing_on(user::PrimaryKey::iter());

    match user::Entity::insert(model)
        .on_conflict(on_conflict.to_owned())
        .try_insert()
        .exec(db)
        .await?
    {
        TryInsertResult::Inserted(result) => {
            let user = user::Entity::find_by_id(result.last_insert_id)
                .one(db)
                .await?
                .ok_or_else(|| {
                    DbErr::RecordNotFound(format!(
                        "inserted user #{} could not be reloaded",
                        result.last_insert_id
                    ))
                })?;
            Ok(Some(user))
        }
        TryInsertResult::Conflicted => Ok(None),
        TryInsertResult::Empty => Err(DbErr::RecordNotInserted),
    }
}

pub(super) fn map_user_email_db_err(err: DbErr) -> AsterError {
    if is_unique_conflict_db_err(&err) {
        email_exists_error()
    } else {
        AsterError::from(err)
    }
}

async fn map_contact_token_create_db_err<C: ConnectionTrait>(
    db: &C,
    err: DbErr,
    user_id: i64,
    purpose: VerificationPurpose,
) -> AsterError {
    if !is_unique_conflict_db_err(&err) {
        return AsterError::from(err);
    }

    if contact_verification_token_repo::find_latest_active_for_user(
        db,
        user_id,
        VerificationChannel::Email,
        purpose,
    )
    .await
    .ok()
    .flatten()
    .is_some()
    {
        return AsterError::rate_limited(ACTIVE_VERIFICATION_REQUEST_MESSAGE);
    }

    AsterError::from(err)
}

pub(super) fn is_active_verification_request_error(err: &AsterError) -> bool {
    matches!(err, AsterError::RateLimited(message) if message == ACTIVE_VERIFICATION_REQUEST_MESSAGE)
}

pub(super) async fn ensure_email_available<C: ConnectionTrait>(
    db: &C,
    email: &str,
    exclude_user_id: Option<i64>,
) -> Result<()> {
    if let Some(existing) = user_repo::find_by_email(db, email).await?
        && Some(existing.id) != exclude_user_id
    {
        return Err(email_exists_error());
    }

    if let Some(existing) = user_repo::find_by_pending_email(db, email).await?
        && Some(existing.id) != exclude_user_id
    {
        return Err(email_exists_error());
    }

    Ok(())
}

pub(crate) struct CreateUserWithRoleInput<'a> {
    pub username: &'a str,
    pub email: &'a str,
    pub password: &'a str,
    pub role: UserRole,
    pub status: UserStatus,
    pub email_verified_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub(crate) async fn create_user_with_role<C: ConnectionTrait>(
    db: &C,
    state: &impl SharedRuntimeState,
    input: CreateUserWithRoleInput<'_>,
) -> Result<user::Model> {
    let CreateUserWithRoleInput {
        username,
        email,
        password,
        role,
        status,
        email_verified_at,
    } = input;
    let username = normalize_username(username)?;
    let email = normalize_email(email)?;
    validate_password(password)?;

    if user_repo::find_by_username(db, &username).await?.is_some() {
        return Err(username_exists_error());
    }
    ensure_email_available(db, &email, None).await?;

    let password_hash = hash::hash_password(password)?;
    let now = Utc::now();

    let default_quota = state
        .runtime_config()
        .get_i64("default_storage_quota")
        .unwrap_or_else(|| {
            if let Some(raw) = state.runtime_config().get("default_storage_quota") {
                tracing::warn!("invalid default_storage_quota value '{}', using 0", raw);
            }
            0
        });
    let default_policy_group_id = state
        .policy_snapshot()
        .system_default_policy_group()
        .map(|group| group.id)
        .ok_or_else(|| {
            AsterError::storage_policy_not_found(
                "no system default storage policy group configured",
            )
        })?;

    let username_for_err = username.clone();
    let email_for_err = email.clone();
    let model = user::ActiveModel {
        username: Set(username),
        email: Set(email),
        password_hash: Set(password_hash),
        role: Set(role),
        status: Set(status),
        session_version: Set(INITIAL_SESSION_VERSION),
        email_verified_at: Set(email_verified_at),
        pending_email: Set(None),
        storage_used: Set(0),
        storage_quota: Set(default_quota),
        policy_group_id: Set(Some(default_policy_group_id)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    let user = match insert_user_with_conflict_marker(db, model).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return Err(map_user_create_conflict(db, &username_for_err, &email_for_err).await);
        }
        Err(err) => {
            return Err(map_user_create_db_err(db, err, &username_for_err, &email_for_err).await);
        }
    };

    Ok(user)
}

pub(super) async fn create_first_admin(
    state: &impl SharedRuntimeState,
    username: &str,
    email: &str,
    password: &str,
) -> Result<user::Model> {
    tracing::info!("first user registered — granting admin role to '{username}'");
    create_user_with_role(
        state.writer_db(),
        state,
        CreateUserWithRoleInput {
            username,
            email,
            password,
            role: UserRole::Admin,
            status: UserStatus::Active,
            email_verified_at: Some(Utc::now()),
        },
    )
    .await
    .inspect(|user| {
        if let Some(policy_group_id) = user.policy_group_id {
            state
                .policy_snapshot()
                .set_user_policy_group(user.id, policy_group_id);
        }
    })
}

pub(super) async fn issue_contact_verification_token<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    purpose: VerificationPurpose,
    target: &str,
    ttl_secs: u64,
) -> Result<String> {
    let now = Utc::now();
    let token = mail_service::build_verification_token();
    let token_hash = hash::sha256_hex(token.as_bytes());

    contact_verification_token_repo::delete_active_for_user(
        db,
        user_id,
        VerificationChannel::Email,
        purpose,
    )
    .await?;

    match (contact_verification_token::ActiveModel {
        user_id: Set(user_id),
        channel: Set(VerificationChannel::Email),
        purpose: Set(purpose),
        target: Set(target.to_string()),
        token_hash: Set(token_hash),
        expires_at: Set(now + Duration::seconds(u64_to_i64(ttl_secs, "contact verification ttl")?)),
        consumed_at: Set(None),
        created_at: Set(now),
        ..Default::default()
    })
    .insert(db)
    .await
    {
        Ok(_) => {}
        Err(err) => {
            return Err(map_contact_token_create_db_err(db, err, user_id, purpose).await);
        }
    }

    Ok(token)
}

pub(super) async fn ensure_resend_allowed<C: ConnectionTrait>(
    state: &impl SharedRuntimeState,
    db: &C,
    user_id: i64,
    purpose: VerificationPurpose,
) -> Result<()> {
    if resend_allowed(state, db, user_id, purpose).await? {
        return Ok(());
    }

    let policy = RuntimeContactVerificationPolicy::from_runtime_config(state.runtime_config());
    let remaining = policy.resend_cooldown_secs.max(1);
    Err(AsterError::rate_limited(format!(
        "please wait {remaining} seconds before resending",
    )))
}

pub(super) async fn resend_allowed<C: ConnectionTrait>(
    state: &impl SharedRuntimeState,
    db: &C,
    user_id: i64,
    purpose: VerificationPurpose,
) -> Result<bool> {
    let policy = RuntimeContactVerificationPolicy::from_runtime_config(state.runtime_config());
    let Some(latest) = contact_verification_token_repo::find_latest_active_for_user(
        db,
        user_id,
        VerificationChannel::Email,
        purpose,
    )
    .await?
    else {
        return Ok(true);
    };

    let allowed_at = latest.created_at
        + Duration::seconds(u64_to_i64(
            policy.resend_cooldown_secs,
            "contact verification resend cooldown",
        )?);
    Ok(allowed_at <= Utc::now())
}

pub(super) async fn password_reset_request_allowed<C: ConnectionTrait>(
    state: &impl SharedRuntimeState,
    db: &C,
    user_id: i64,
) -> Result<bool> {
    let policy = RuntimeContactVerificationPolicy::from_runtime_config(state.runtime_config());
    let Some(latest) = contact_verification_token_repo::find_latest_active_for_user(
        db,
        user_id,
        VerificationChannel::Email,
        VerificationPurpose::PasswordReset,
    )
    .await?
    else {
        return Ok(true);
    };

    let allowed_at = latest.created_at
        + Duration::seconds(u64_to_i64(
            policy.password_reset_request_cooldown_secs,
            "password reset request cooldown",
        )?);
    Ok(allowed_at <= Utc::now())
}

pub(super) async fn update_password_in_connection<C: ConnectionTrait>(
    db: &C,
    user: user::Model,
    new_password: &str,
) -> Result<user::Model> {
    validate_password(new_password)?;

    let next_session_version = user.session_version.saturating_add(1);
    let mut active = user.into_active_model();
    active.password_hash = Set(hash::hash_password(new_password)?);
    active.session_version = Set(next_session_version);
    active.updated_at = Set(Utc::now());
    active
        .update(db)
        .await
        .map_aster_err(AsterError::database_operation)
}

pub(crate) async fn find_user_by_identifier<C: ConnectionTrait>(
    db: &C,
    identifier: &str,
) -> Result<Option<user::Model>> {
    let normalized = identifier.trim();
    if normalized.contains('@') {
        user_repo::find_by_email(db, normalized).await
    } else {
        user_repo::find_by_username(db, normalized).await
    }
}

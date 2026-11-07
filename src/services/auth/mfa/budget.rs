//! Account-level MFA budgets. All mutations run under the writer account lock.
use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::mfa_attempt_budget_repo;
use crate::errors::{AsterError, Result, auth_mfa_failed_with_code};
use aster_drive_model::entities::mfa_attempt_budget;
use chrono::{DateTime, Duration, Utc};
use sea_orm::ConnectionTrait;

pub(super) const FAILURE_LIMIT: i32 = 5;
pub(super) const WINDOW_SECONDS: i64 = 15 * 60;
pub(super) const COOLDOWN_SECONDS: i64 = 15 * 60;

#[derive(Clone, Copy)]
pub(super) enum Purpose {
    Login,
    Sensitive,
}

impl Purpose {
    fn as_str(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::Sensitive => "sensitive",
        }
    }
}

fn exhausted() -> AsterError {
    auth_mfa_failed_with_code(
        ApiErrorCode::AuthMfaAttemptsExceeded,
        "MFA account attempts exhausted; retry after the temporary cooldown",
    )
}

pub(super) async fn ensure_available<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    purpose: Purpose,
    now: DateTime<Utc>,
) -> Result<()> {
    match mfa_attempt_budget_repo::find(db, user_id, purpose.as_str()).await? {
        Some(model) if model.blocked_until.is_some_and(|until| until > now) => Err(exhausted()),
        _ => Ok(()),
    }
}

fn fresh(user_id: i64, purpose: &str, now: DateTime<Utc>) -> mfa_attempt_budget::Model {
    mfa_attempt_budget::Model {
        user_id,
        purpose: purpose.to_string(),
        failure_count: 0,
        window_started_at: now,
        blocked_until: None,
        updated_at: now,
    }
}

pub(super) async fn record_failure<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    purpose: Purpose,
    now: DateTime<Utc>,
) -> Result<bool> {
    let model = mfa_attempt_budget_repo::find(db, user_id, purpose.as_str()).await?;
    let mut model = match model {
        Some(model) if model.blocked_until.is_some_and(|until| until > now) => {
            return Err(exhausted());
        }
        Some(model)
            if model.blocked_until.is_none()
                && now < model.window_started_at + Duration::seconds(WINDOW_SECONDS) =>
        {
            model
        }
        _ => fresh(user_id, purpose.as_str(), now),
    };
    model.failure_count = model.failure_count.saturating_add(1);
    let blocked = model.failure_count >= FAILURE_LIMIT;
    model.blocked_until = blocked.then_some(now + Duration::seconds(COOLDOWN_SECONDS));
    model.updated_at = now;
    mfa_attempt_budget_repo::save(db, model).await?;
    Ok(blocked)
}

pub(super) async fn reset<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    purpose: Purpose,
    now: DateTime<Utc>,
) -> Result<()> {
    mfa_attempt_budget_repo::save(db, fresh(user_id, purpose.as_str(), now)).await
}

pub(super) async fn ensure_email_send_available<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    now: DateTime<Utc>,
) -> Result<()> {
    match mfa_attempt_budget_repo::find(db, user_id, "email_send").await? {
        Some(model) if model.blocked_until.is_some_and(|until| until > now) => Err(
            AsterError::rate_limited("please wait before requesting another email code"),
        ),
        _ => Ok(()),
    }
}

pub(super) async fn reserve_email_send<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    now: DateTime<Utc>,
    cooldown_seconds: i64,
) -> Result<()> {
    ensure_email_send_available(db, user_id, now).await?;
    let mut model = fresh(user_id, "email_send", now);
    model.blocked_until = Some(now + Duration::seconds(cooldown_seconds));
    mfa_attempt_budget_repo::save(db, model).await
}

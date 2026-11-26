//! 仓储模块：`contact_verification_token_repo`。

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::contact_verification_token::{
    self, Entity as ContactVerificationToken,
};
use aster_drive_model::types::{VerificationChannel, VerificationPurpose};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, IntoActiveModel, QueryFilter, QueryOrder, sea_query::Expr,
};

/// Inserts a new contact verification token row from the given active model.
pub async fn create<C: ConnectionTrait>(
    db: &C,
    model: contact_verification_token::ActiveModel,
) -> Result<contact_verification_token::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

/// Resolves a verification token by the hash of its raw value, regardless of
/// consumption or expiry. Returns `None` when no token carries the hash.
pub async fn find_by_token_hash(
    db: &DatabaseConnection,
    token_hash: &str,
) -> Result<Option<contact_verification_token::Model>> {
    ContactVerificationToken::find()
        .filter(contact_verification_token::Column::TokenHash.eq(token_hash))
        .one(db)
        .await
        .map_err(AsterError::from)
}

/// Returns the user's most recent unconsumed, unexpired token for the given
/// channel and purpose; used for resend throttling.
pub async fn find_latest_active_for_user<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    channel: VerificationChannel,
    purpose: VerificationPurpose,
) -> Result<Option<contact_verification_token::Model>> {
    ContactVerificationToken::find()
        .filter(contact_verification_token::Column::UserId.eq(user_id))
        .filter(contact_verification_token::Column::Channel.eq(channel))
        .filter(contact_verification_token::Column::Purpose.eq(purpose))
        .filter(contact_verification_token::Column::ConsumedAt.is_null())
        .filter(contact_verification_token::Column::ExpiresAt.gt(Utc::now()))
        .order_by_desc(contact_verification_token::Column::CreatedAt)
        .one(db)
        .await
        .map_err(AsterError::from)
}

/// Removes the user's unconsumed tokens for one channel and purpose,
/// superseding them with a fresh token.
pub async fn delete_active_for_user<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    channel: VerificationChannel,
    purpose: VerificationPurpose,
) -> Result<()> {
    ContactVerificationToken::delete_many()
        .filter(contact_verification_token::Column::UserId.eq(user_id))
        .filter(contact_verification_token::Column::Channel.eq(channel))
        .filter(contact_verification_token::Column::Purpose.eq(purpose))
        .filter(contact_verification_token::Column::ConsumedAt.is_null())
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

/// Marks the given token consumed at the current time and persists the change.
pub async fn mark_consumed<C: ConnectionTrait>(
    db: &C,
    token: contact_verification_token::Model,
) -> Result<contact_verification_token::Model> {
    let mut active = token.into_active_model();
    active.consumed_at = Set(Some(Utc::now()));
    active.update(db).await.map_err(AsterError::from)
}

/// One-shot consumption guard: marks the token consumed only when it is still
/// unused and unexpired at execution time. Returns `false` when the token is
/// absent, already consumed, or expired, so a token can never verify twice.
pub async fn mark_consumed_if_unused<C: ConnectionTrait>(db: &C, token_id: i64) -> Result<bool> {
    let execution_now = Utc::now();
    let result = ContactVerificationToken::update_many()
        .col_expr(
            contact_verification_token::Column::ConsumedAt,
            Expr::value(Some(execution_now)),
        )
        .filter(contact_verification_token::Column::Id.eq(token_id))
        .filter(contact_verification_token::Column::ConsumedAt.is_null())
        .filter(contact_verification_token::Column::ExpiresAt.gt(execution_now))
        .exec(db)
        .await
        .map_err(AsterError::from)?;

    Ok(result.rows_affected == 1)
}

/// Removes tokens that expired before the current time, returning the number
/// of deleted rows.
pub async fn delete_expired(db: &DatabaseConnection) -> Result<u64> {
    let result = ContactVerificationToken::delete_many()
        .filter(contact_verification_token::Column::ExpiresAt.lt(Utc::now()))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

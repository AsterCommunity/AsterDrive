//! One-time state and replay reservations use database uniqueness/CAS, never cache fallback.
use crate::errors::{AsterError, Result};
use aster_drive_model::entities::auth_once_state::{self, Entity};
use chrono::{DateTime, Utc};
use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set, SqlErr,
};
use sha2::{Digest, Sha256};

fn key_hash(key: &str) -> String {
    hex::encode(Sha256::digest(key.as_bytes()))
}

/// Immutable inserts prevent a consumed challenge or nonce being recreated by cache recovery.
pub async fn reserve<C: ConnectionTrait>(
    db: &C,
    key: &str,
    payload: &str,
    expires_at: DateTime<Utc>,
) -> Result<bool> {
    if payload.len() > 256 * 1024 || key.len() > 1024 {
        return Err(AsterError::validation_error(
            "one-time authentication state exceeds its budget",
        ));
    }
    // Bound expiry collection; normal auth traffic steadily retires old state.
    let expired = Entity::find()
        .select_only()
        .column(auth_once_state::Column::KeyHash)
        .filter(auth_once_state::Column::ExpiresAt.lt(Utc::now()))
        .order_by_asc(auth_once_state::Column::ExpiresAt)
        .limit(128)
        .into_tuple::<String>()
        .all(db)
        .await
        .map_err(AsterError::from)?;
    if !expired.is_empty() {
        Entity::delete_many()
            .filter(auth_once_state::Column::KeyHash.is_in(expired))
            .filter(auth_once_state::Column::ExpiresAt.lt(Utc::now()))
            .exec(db)
            .await
            .map_err(AsterError::from)?;
    }
    match Entity::insert(auth_once_state::ActiveModel {
        key_hash: Set(key_hash(key)),
        payload: Set(payload.to_owned()),
        expires_at: Set(expires_at),
    })
    .exec(db)
    .await
    {
        Ok(_) => Ok(true),
        // Do not infer uniqueness from ON DUPLICATE KEY affected-row counts:
        // MySQL CLIENT_FOUND_ROWS can report success for its no-op update.
        Err(error) if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) => {
            Ok(false)
        }
        Err(error) => Err(AsterError::from(error)),
    }
}

/// Consume first; only the delete winner receives the payload. A failed DB operation fails closed.
pub async fn consume<C: ConnectionTrait>(db: &C, key: &str) -> Result<Option<String>> {
    let hash = key_hash(key);
    let row = match Entity::find_by_id(&hash)
        .one(db)
        .await
        .map_err(AsterError::from)?
    {
        Some(row) => row,
        None => return Ok(None),
    };
    let deleted = Entity::delete_many()
        .filter(auth_once_state::Column::KeyHash.eq(hash))
        .filter(auth_once_state::Column::ExpiresAt.eq(row.expires_at))
        .exec(db)
        .await
        .map_err(AsterError::from)?
        .rows_affected;
    Ok((deleted == 1 && row.expires_at > Utc::now()).then_some(row.payload))
}

//! Writer-owned one-time authentication state; never an independently fallible cache copy.
use sea_orm::entity::prelude::*;

/// Writer-owned one-time authentication state (challenge, nonce, replay reservation).
///
/// Rows are inserted immutably and consumed by deletion; database uniqueness decides
/// single use, never a cache.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "auth_once_states")]
pub struct Model {
    /// Hex SHA-256 of the caller-supplied one-time key; the plaintext key is never stored.
    #[sea_orm(primary_key, auto_increment = false)]
    pub key_hash: String,
    /// Opaque payload returned to the single consumer that wins the delete, typically an
    /// encoded challenge or signed request context.
    pub payload: String,
    /// Absolute expiry instant; expired rows are garbage-collected opportunistically.
    pub expires_at: DateTimeUtc,
}

/// No database-level relations are declared for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

//! SeaORM 实体定义：`contact_verification_token`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::{VerificationChannel, VerificationPurpose};

/// Single-use contact verification token (email or phone) bound to one account and purpose.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(table_name = "contact_verification_tokens")]
pub struct Model {
    /// Auto-incrementing surrogate key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Account being verified; references `users.id`.
    pub user_id: i64,
    /// Delivery channel the token is sent over.
    pub channel: VerificationChannel,
    /// Workflow the token unlocks; one token serves exactly one purpose.
    pub purpose: VerificationPurpose,
    /// Contact address the token was sent to (email address or phone number).
    pub target: String,
    /// Hex hash of the delivered token; the plaintext token is never stored. Unique column.
    pub token_hash: String,
    /// Absolute expiry instant; expired tokens are never accepted.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: DateTimeUtc,
    /// UTC instant the token was consumed; `None` while still unused.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub consumed_at: Option<DateTimeUtc>,
    /// UTC instant the token was issued.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
}

/// Declared database relations for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// The account being verified, matched on `user_id` to `users.id`.
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id"
    )]
    User,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

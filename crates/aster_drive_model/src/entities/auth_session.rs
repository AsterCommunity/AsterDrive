//! SeaORM 实体定义：`auth_session`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Live authentication session owning one refresh-token rotation chain.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(table_name = "auth_sessions")]
pub struct Model {
    /// Session identifier (UUID), handed out inside access/refresh tokens.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// Owning account; references `users.id`.
    pub user_id: i64,
    /// `jti` claim of the refresh token that is currently valid for this session; rotation
    /// swaps it via a compare-and-swap on the previous value.
    pub current_refresh_jti: String,
    /// `jti` that was current before the latest rotation; kept so a replayed old refresh
    /// token can be recognized and treated as session theft.
    pub previous_refresh_jti: Option<String>,
    /// Absolute expiry of the refresh-token chain; access tokens never outlive this window.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub refresh_expires_at: DateTimeUtc,
    /// Client IP observed at creation or the latest rotation; `None` when unknown.
    pub ip_address: Option<String>,
    /// Client `User-Agent` observed at creation or the latest rotation; `None` when unknown.
    pub user_agent: Option<String>,
    /// UTC instant the session was created.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    /// UTC instant of the latest authenticated activity; only ever moves forward.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub last_seen_at: DateTimeUtc,
    /// UTC instant the session was revoked; `None` while the session is still active.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub revoked_at: Option<DateTimeUtc>,
}

/// Declared database relations for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// The owning user account, matched on `user_id` to `users.id`.
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

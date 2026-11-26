//! SeaORM 实体定义：`wopi_session`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Short-lived WOPI access session issued to a client app for one file,
/// bound to the acting user, the app registration and the user's session
/// version at issue time.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(table_name = "wopi_sessions")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Hash of the opaque WOPI access token handed to the client; the raw
    /// token is never stored.
    pub token_hash: String,
    /// User on whose behalf the app acts for this session.
    pub actor_user_id: i64,
    /// Snapshot of the user's session version at issue time; bumping the
    /// user's version invalidates this session.
    pub session_version: i64,
    /// Team scope of the file when the session was created; `None` for
    /// personal-scope files.
    pub team_id: Option<i64>,
    /// File the session grants access to.
    pub file_id: i64,
    /// Registered WOPI client app key (from app discovery config) identifying
    /// which integration holds this session.
    pub app_key: String,
    /// Time after which the session token is no longer accepted (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: DateTimeUtc,
    /// Row creation timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// The acting user (FK `actor_user_id`).
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::ActorUserId",
        to = "super::user::Column::Id"
    )]
    User,
    /// The file this session grants access to (FK `file_id`).
    #[sea_orm(
        belongs_to = "super::file::Entity",
        from = "Column::FileId",
        to = "super::file::Column::Id"
    )]
    File,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl Related<super::file::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::File.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

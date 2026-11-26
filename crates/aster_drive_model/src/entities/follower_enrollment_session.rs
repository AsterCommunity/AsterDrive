//! SeaORM 实体定义：`follower_enrollment_session`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// One-shot enrollment handshake between this primary and a follower remote
/// node: the follower redeems the enrollment token, then confirms with the ack
/// token. Only one completed session per node is meaningful; pending sessions
/// are invalidated when a new one is issued.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(table_name = "follower_enrollment_sessions")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Remote node being enrolled; one node may hold several sessions but at
    /// most one completed one.
    pub managed_follower_id: i64,
    /// Hash of the enrollment token presented by the follower; lookup key for
    /// the redeem step.
    pub token_hash: String,
    /// Hash of the follow-up token the follower presents to acknowledge
    /// enrollment completion.
    pub ack_token_hash: String,
    /// Absolute deadline after which the token can no longer be redeemed (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: DateTimeUtc,
    /// When the follower redeemed the enrollment token; `None` until then.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub redeemed_at: Option<DateTimeUtc>,
    /// When the follower confirmed enrollment completion; `None` until then.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub acked_at: Option<DateTimeUtc>,
    /// When the session was superseded by a newer one; `None` while usable.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub invalidated_at: Option<DateTimeUtc>,
    /// Row creation timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// The remote node being enrolled (FK `managed_follower_id`).
    #[sea_orm(
        belongs_to = "super::managed_follower::Entity",
        from = "Column::ManagedFollowerId",
        to = "super::managed_follower::Column::Id"
    )]
    ManagedFollower,
}

impl Related<super::managed_follower::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ManagedFollower.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

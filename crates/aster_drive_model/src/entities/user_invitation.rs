//! SeaORM 实体定义：`user_invitations`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::UserInvitationStatus;

/// Instance user invitation row: a single-use signup link for one email
/// address, tracked from issuance until acceptance/revocation/expiry.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(table_name = "user_invitations")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Invited email address; one pending invitation per address.
    pub email: String,
    /// Hash of the invitation token; the raw token only exists in the
    /// invitation link and is never stored.
    pub token_hash: String,
    /// Current lifecycle state of the invitation.
    pub status: UserInvitationStatus,
    /// Admin user who issued the invitation.
    pub invited_by: i64,
    /// User account created by accepting this invitation; `None` until
    /// accepted.
    pub accepted_user_id: Option<i64>,
    /// Time after which the invitation can no longer be accepted (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: DateTimeUtc,
    /// Row creation timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    /// Row last-update timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTimeUtc,
    /// When the invitation was accepted; `None` until then.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub accepted_at: Option<DateTimeUtc>,
    /// When the invitation was revoked by an admin; `None` unless revoked.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub revoked_at: Option<DateTimeUtc>,
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// The admin who issued the invitation (FK `invited_by`).
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::InvitedBy",
        to = "super::user::Column::Id"
    )]
    InvitedByUser,
    /// The user created by accepting this invitation (FK `accepted_user_id`).
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::AcceptedUserId",
        to = "super::user::Column::Id"
    )]
    AcceptedUser,
}

impl ActiveModelBehavior for ActiveModel {}

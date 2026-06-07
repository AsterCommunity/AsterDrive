//! SeaORM 实体定义：`user_invitations`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::UserInvitationStatus;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(table_name = "user_invitations")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub email: String,
    pub token_hash: String,
    pub status: UserInvitationStatus,
    pub invited_by: i64,
    pub accepted_user_id: Option<i64>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: DateTimeUtc,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTimeUtc,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub accepted_at: Option<DateTimeUtc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub revoked_at: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::InvitedBy",
        to = "super::user::Column::Id"
    )]
    InvitedByUser,
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::AcceptedUserId",
        to = "super::user::Column::Id"
    )]
    AcceptedUser,
}

impl ActiveModelBehavior for ActiveModel {}

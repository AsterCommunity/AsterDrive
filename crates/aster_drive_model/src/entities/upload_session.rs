//! SeaORM 实体定义：`upload_session`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::{UploadSessionKind, UploadSessionStatus};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), schema(as = UploadSession))]
#[sea_orm(table_name = "upload_sessions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String, // UUID
    pub user_id: i64,
    pub team_id: Option<i64>,
    /// Browser/frontend instance that initiated the session.
    /// Used only for recoverable-session visibility; auth still uses user/team scope.
    pub frontend_client_id: Option<String>,
    pub filename: String,
    pub total_size: i64,
    pub chunk_size: i64,
    pub total_chunks: i32,
    pub received_count: i32,
    pub folder_id: Option<i64>,
    pub policy_id: i64,
    pub status: UploadSessionStatus,
    /// Explicit data-plane kind selected and persisted during upload initialization.
    pub session_kind: UploadSessionKind,
    /// Driver-agnostic temporary object key used by object/presigned multipart upload flows.
    pub object_temp_key: Option<String>,
    /// Driver-agnostic multipart upload id; empty for direct/stream upload transports.
    pub object_multipart_id: Option<String>,
    /// Encrypted provider-native upload session metadata. The upload URL is a temporary secret.
    #[serde(skip_serializing)]
    pub provider_session_ciphertext: Option<String>,
    /// 上传完成后关联的文件 ID（用于幂等重试）
    pub file_id: Option<i64>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: DateTimeUtc,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id"
    )]
    User,
    #[sea_orm(
        belongs_to = "super::team::Entity",
        from = "Column::TeamId",
        to = "super::team::Column::Id"
    )]
    Team,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl Related<super::team::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Team.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

//! SeaORM 实体定义：`upload_session`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::{UploadSessionKind, UploadSessionStatus};

/// Resumable upload session: tracks progress and placement decisions from
/// initialization until the file is finalized or the session fails/expires.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), schema(as = UploadSession))]
#[sea_orm(table_name = "upload_sessions")]
pub struct Model {
    /// Primary key; UUID string generated at session creation.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// Owning user; auth scope for the session.
    pub user_id: i64,
    /// Team scope when uploading into a team workspace; `None` for
    /// personal-scope uploads.
    pub team_id: Option<i64>,
    /// Browser/frontend instance that initiated the session.
    /// Used only for recoverable-session visibility; auth still uses user/team scope.
    pub frontend_client_id: Option<String>,
    /// Target file name as supplied by the client.
    pub filename: String,
    /// MIME type resolved and frozen during upload initialization.
    pub mime_type: String,
    /// Expected total upload size in bytes.
    pub total_size: i64,
    /// Chunk size in bytes for chunked/multipart transports; `0` for
    /// non-chunked stream and single-shot transports.
    pub chunk_size: i64,
    /// Expected number of chunks for chunked transports; `0` for non-chunked
    /// transports.
    pub total_chunks: i32,
    /// Number of chunks (or bytes advanced for relay flows) already received;
    /// used to resume and to gate out-of-order part submissions.
    pub received_count: i32,
    /// Destination folder; `None` means the user/team root.
    pub folder_id: Option<i64>,
    /// Storage policy selected during upload initialization.
    pub policy_id: i64,
    /// Placement profile/rule revision selected during upload initialization.
    pub placement_profile_id: i64,
    /// Placement rule chosen for this session; folder-policy overrides select
    /// a policy without a placement rule, so this is `None` there.
    pub placement_rule_id: Option<i64>,
    /// Placement topology revision captured at initialization; detects
    /// topology changes between session creation and completion.
    pub placement_revision: i64,
    /// Serialized `UploadExecutionPreference` fixed at initialization
    /// (`automatic` or `force_server_stream`).
    pub placement_execution_preference: String,
    /// Current lifecycle state of the session.
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
    /// File created by this session once completed (used for idempotent
    /// retries); `None` until finalization.
    pub file_id: Option<i64>,
    /// Row creation timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    /// Time after which the session is considered stale and becomes
    /// recoverable/cleanable (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: DateTimeUtc,
    /// Row last-update timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTimeUtc,
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// Owning user (FK `user_id`).
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id"
    )]
    User,
    /// Team scope when uploading into a team workspace (FK `team_id`).
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

//! 上传 session 数据面、状态与调度契约类型。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Persisted data plane for an upload session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
#[serde(rename_all = "snake_case")]
pub enum UploadSessionKind {
    /// Single streaming request written directly by the server.
    #[sea_orm(string_value = "stream")]
    Stream,
    /// Chunked upload staged at byte offsets in local temp storage.
    #[sea_orm(string_value = "offset_staging")]
    OffsetStaging,
    /// Chunked upload staged sequentially in local temp storage (SFTP plans).
    #[sea_orm(string_value = "stream_staging")]
    StreamStaging,
    /// Server relays chunks into a provider's multipart upload.
    #[sea_orm(string_value = "provider_relay_multipart")]
    ProviderRelayMultipart,
    /// Browser uploads the whole object to one provider presigned URL.
    #[sea_orm(string_value = "provider_presigned_single")]
    ProviderPresignedSingle,
    /// Browser uploads parts to provider presigned part URLs.
    #[sea_orm(string_value = "provider_presigned_multipart")]
    ProviderPresignedMultipart,
    /// Primary relays chunks into a remote node's multipart upload.
    #[sea_orm(string_value = "remote_relay_multipart")]
    RemoteRelayMultipart,
    /// Browser uploads the whole object to one remote node presigned URL.
    #[sea_orm(string_value = "remote_presigned_single")]
    RemotePresignedSingle,
    /// Browser uploads parts to remote node presigned part URLs.
    #[sea_orm(string_value = "remote_presigned_multipart")]
    RemotePresignedMultipart,
    /// Browser drives a provider-native resumable session directly.
    #[sea_orm(string_value = "provider_direct_resumable")]
    ProviderDirectResumable,
    /// Server relays chunks through a provider-native resumable session.
    #[sea_orm(string_value = "provider_relay_resumable")]
    ProviderRelayResumable,
}

/// Ordering constraint the client must follow when uploading chunks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum UploadChunkOrdering {
    /// Chunks may arrive in any order and in parallel.
    Unordered,
    /// Chunks must arrive strictly in sequence.
    Sequential,
}

/// Chunk scheduling contract derived from the session data plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct UploadScheduling {
    /// Ordering the client must apply to chunk uploads.
    pub chunk_ordering: UploadChunkOrdering,
    /// Maximum number of chunks the client may upload concurrently.
    pub max_chunk_concurrency: i32,
}

impl UploadSessionKind {
    /// Returns the snake_case kind name persisted in
    /// `upload_sessions.session_kind`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stream => "stream",
            Self::OffsetStaging => "offset_staging",
            Self::StreamStaging => "stream_staging",
            Self::ProviderRelayMultipart => "provider_relay_multipart",
            Self::ProviderPresignedSingle => "provider_presigned_single",
            Self::ProviderPresignedMultipart => "provider_presigned_multipart",
            Self::RemoteRelayMultipart => "remote_relay_multipart",
            Self::RemotePresignedSingle => "remote_presigned_single",
            Self::RemotePresignedMultipart => "remote_presigned_multipart",
            Self::ProviderDirectResumable => "provider_direct_resumable",
            Self::ProviderRelayResumable => "provider_relay_resumable",
        }
    }
}

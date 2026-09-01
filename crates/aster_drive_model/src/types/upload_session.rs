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
    #[sea_orm(string_value = "stream")]
    Stream,
    #[sea_orm(string_value = "offset_staging")]
    OffsetStaging,
    #[sea_orm(string_value = "stream_staging")]
    StreamStaging,
    #[sea_orm(string_value = "provider_relay_multipart")]
    ProviderRelayMultipart,
    #[sea_orm(string_value = "provider_presigned_single")]
    ProviderPresignedSingle,
    #[sea_orm(string_value = "provider_presigned_multipart")]
    ProviderPresignedMultipart,
    #[sea_orm(string_value = "remote_relay_multipart")]
    RemoteRelayMultipart,
    #[sea_orm(string_value = "remote_presigned_single")]
    RemotePresignedSingle,
    #[sea_orm(string_value = "remote_presigned_multipart")]
    RemotePresignedMultipart,
    #[sea_orm(string_value = "provider_direct_resumable")]
    ProviderDirectResumable,
    #[sea_orm(string_value = "provider_relay_resumable")]
    ProviderRelayResumable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum UploadChunkOrdering {
    Unordered,
    Sequential,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct UploadScheduling {
    pub chunk_ordering: UploadChunkOrdering,
    pub max_chunk_concurrency: i32,
}

impl UploadSessionKind {
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

//! 存储策略传输策略、远端节点传输与媒体处理器类型。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// 上传 session 状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "lowercase")]
pub enum UploadSessionStatus {
    /// Session is accepting client uploads.
    #[sea_orm(string_value = "uploading")]
    Uploading,
    /// All input arrived and the final object is being assembled.
    #[sea_orm(string_value = "assembling")]
    Assembling,
    /// Upload was finalized into a persisted blob.
    #[sea_orm(string_value = "completed")]
    Completed,
    /// Upload failed and will not complete.
    #[sea_orm(string_value = "failed")]
    Failed,
    /// Presigned URLs were issued and the client is uploading directly.
    #[sea_orm(string_value = "presigned")]
    Presigned,
}

/// Upload data plane fixed by upload initialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum UploadTransport {
    /// One streaming request body handled server-side.
    Stream,
    /// Chunked transfer with per-chunk progress tracking.
    Chunked,
    /// Client uploads the whole object through one presigned URL.
    Presigned,
    /// Client uploads parts through presigned part URLs.
    PresignedMultipart,
    /// Client drives a provider-native resumable session.
    ProviderResumable,
}

impl UploadTransport {
    /// Returns the snake_case transport name used by plan resolution.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stream => "stream",
            Self::Chunked => "chunked",
            Self::Presigned => "presigned",
            Self::PresignedMultipart => "presigned_multipart",
            Self::ProviderResumable => "provider_resumable",
        }
    }
}

/// Object-storage upload transfer strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ObjectStorageUploadStrategy {
    /// 服务端将请求体直接中继到对象存储，不落本地临时文件
    RelayStream,
    /// 浏览器直传对象存储
    Presigned,
}

/// Object-storage download transfer strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ObjectStorageDownloadStrategy {
    /// 服务端从对象存储拉流后回传给客户端
    RelayStream,
    /// 服务端完成鉴权后重定向到对象存储 presigned GET URL
    Presigned,
}

/// Remote 上传传输策略（存储策略 options JSON）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum RemoteUploadStrategy {
    /// 主控节点直接把完整请求体流式中继到从节点
    RelayStream,
    /// 浏览器通过 presigned URL 直接把对象写到从节点
    Presigned,
}

/// Remote 下载传输策略（存储策略 options JSON）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum RemoteDownloadStrategy {
    /// 主控节点从从节点拉流后回传给客户端
    RelayStream,
    /// 主控节点完成鉴权后重定向到从节点 presigned GET URL
    Presigned,
}

/// Provider-native resumable upload transfer strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProviderResumableUploadStrategy {
    /// AsterDrive receives the browser upload and the provider driver owns its resumable session.
    ServerRelay,
    /// The browser uploads directly to a provider-issued preauthenticated session URL.
    FrontendDirect,
}

/// Provider-native download transfer strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProviderDownloadStrategy {
    /// AsterDrive follows the provider download URL and relays the response body.
    ServerRelay,
    /// AsterDrive redirects the browser to a provider-issued preauthenticated URL.
    FrontendDirect,
}

/// Provider-native download filename policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProviderDownloadFilenameMode {
    /// Prefer the provider's stored filename so direct downloads remain available.
    ProviderNative,
    /// Require the provider filename to match AsterDrive's current filename.
    StrictCurrent,
}

/// Remote node transport mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum RemoteNodeTransportMode {
    /// Primary calls the node directly at its configured base URL.
    #[sea_orm(string_value = "direct")]
    #[default]
    Direct,
    /// Node dials back to the primary over an outbound tunnel.
    #[sea_orm(string_value = "reverse_tunnel")]
    ReverseTunnel,
    /// Use the base URL when configured, otherwise the reverse tunnel.
    #[sea_orm(string_value = "auto")]
    Auto,
}

/// Transport selected by the primary after resolving a remote node's mode and base URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ResolvedRemoteTransport {
    /// Primary reaches the node at its base URL.
    #[sea_orm(string_value = "direct")]
    Direct,
    /// Primary reaches the node through its reverse tunnel connection.
    #[sea_orm(string_value = "reverse_tunnel")]
    #[default]
    ReverseTunnel,
}

impl ResolvedRemoteTransport {
    /// Returns the snake_case transport name shared by storage and API values.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::ReverseTunnel => "reverse_tunnel",
        }
    }

    /// Returns `true` when traffic must flow through the reverse tunnel.
    pub const fn uses_reverse_tunnel(self) -> bool {
        matches!(self, Self::ReverseTunnel)
    }
}

impl RemoteNodeTransportMode {
    /// Returns the snake_case mode name shared by storage and API values.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::ReverseTunnel => "reverse_tunnel",
            Self::Auto => "auto",
        }
    }

    /// Returns `true` when this mode cannot operate without a base URL.
    pub const fn requires_direct_base_url(self) -> bool {
        matches!(self, Self::Direct)
    }

    /// Returns whether the mode resolves to the reverse tunnel for `base_url`.
    pub fn resolves_to_reverse_tunnel(self, base_url: &str) -> bool {
        self.resolve(base_url).uses_reverse_tunnel()
    }

    /// Resolves the mode against the node's configured base URL; `Auto` falls
    /// back to the reverse tunnel when the base URL is missing or blank.
    pub fn resolve(self, base_url: &str) -> ResolvedRemoteTransport {
        match self {
            Self::Direct => ResolvedRemoteTransport::Direct,
            Self::ReverseTunnel => ResolvedRemoteTransport::ReverseTunnel,
            Self::Auto if base_url.trim().is_empty() => ResolvedRemoteTransport::ReverseTunnel,
            Self::Auto => ResolvedRemoteTransport::Direct,
        }
    }
}

/// 统一媒体处理器类型（system_config / storage_policy.options）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum MediaProcessorKind {
    /// In-process Rust image decoder.
    Images,
    /// In-process audio tag reader.
    Lofty,
    /// External `vips` CLI for image previews.
    VipsCli,
    /// External `ffmpeg` CLI for transcode / preview work.
    FfmpegCli,
    /// External `ffprobe` CLI for video metadata extraction.
    FfprobeCli,
    /// Storage connector's own media handling.
    StorageNative,
}

impl MediaProcessorKind {
    /// Returns the snake_case processor name stored in configuration and used
    /// as the descriptor key.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Images => "images",
            Self::Lofty => "lofty",
            Self::VipsCli => "vips_cli",
            Self::FfmpegCli => "ffmpeg_cli",
            Self::FfprobeCli => "ffprobe_cli",
            Self::StorageNative => "storage_native",
        }
    }
}

/// Raw JSON array stored in `storage_policies.allowed_types`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, DeriveValueType)]
pub struct StoredStoragePolicyAllowedTypes(pub String);

impl StoredStoragePolicyAllowedTypes {
    /// Canonical serialized form of an empty allowed-types list.
    pub const EMPTY_JSON: &str = "[]";

    /// Returns a wrapper holding the canonical empty JSON array.
    pub fn empty() -> Self {
        Self(Self::EMPTY_JSON.to_string())
    }
}

impl AsRef<str> for StoredStoragePolicyAllowedTypes {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for StoredStoragePolicyAllowedTypes {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<StoredStoragePolicyAllowedTypes> for String {
    fn from(value: StoredStoragePolicyAllowedTypes) -> Self {
        value.0
    }
}

/// Opaque versioned storage configuration stored in
/// `storage_policies.storage_config`.
///
/// The envelope contains separately owned connector and core behavior
/// sections. The model crate deliberately keeps both sections opaque.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, DeriveValueType)]
pub struct StoredStoragePolicyConfig(pub String);

impl AsRef<str> for StoredStoragePolicyConfig {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for StoredStoragePolicyConfig {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<StoredStoragePolicyConfig> for String {
    fn from(value: StoredStoragePolicyConfig) -> Self {
        value.0
    }
}

/// Parses the stored allowed-types JSON array, returning an empty list and
/// logging a warning when the payload is malformed or non-empty garbage.
pub fn parse_storage_policy_allowed_types(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_else(|error| {
        if !raw.is_empty() && raw != StoredStoragePolicyAllowedTypes::EMPTY_JSON {
            tracing::warn!("invalid storage policy allowed_types JSON '{raw}': {error}");
        }
        Vec::new()
    })
}

/// Serializes an allowed-types list into its stored JSON form.
pub fn serialize_storage_policy_allowed_types(
    allowed_types: &[String],
) -> std::result::Result<StoredStoragePolicyAllowedTypes, serde_json::Error> {
    serde_json::to_string(allowed_types).map(StoredStoragePolicyAllowedTypes)
}

/// Minimum part size in bytes that S3-compatible object storage accepts for
/// multipart uploads (5 MiB).
pub const OBJECT_MULTIPART_MIN_PART_SIZE: i64 = 5 * 1024 * 1024;

/// Clamps a configured multipart part size up to
/// [`OBJECT_MULTIPART_MIN_PART_SIZE`]; non-positive values fall back to the
/// minimum.
pub fn effective_object_multipart_chunk_size(configured: i64) -> i64 {
    if configured <= 0 {
        OBJECT_MULTIPART_MIN_PART_SIZE
    } else {
        configured.max(OBJECT_MULTIPART_MIN_PART_SIZE)
    }
}

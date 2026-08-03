use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// 存储驱动类型
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum, Serialize, Deserialize,
)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    schema(rename_all = "snake_case")
)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
#[serde(rename_all = "snake_case")]
pub enum DriverType {
    #[sea_orm(string_value = "local")]
    Local,
    #[sea_orm(string_value = "s3")]
    S3,
    #[sea_orm(string_value = "sftp")]
    Sftp,
    #[sea_orm(string_value = "azure_blob")]
    AzureBlob,
    #[sea_orm(string_value = "tencent_cos")]
    TencentCos,
    #[sea_orm(string_value = "remote")]
    Remote,
    #[sea_orm(string_value = "onedrive")]
    OneDrive,
}

impl DriverType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::S3 => "s3",
            Self::Sftp => "sftp",
            Self::AzureBlob => "azure_blob",
            Self::TencentCos => "tencent_cos",
            Self::Remote => "remote",
            Self::OneDrive => "onedrive",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "local" => Some(Self::Local),
            "s3" => Some(Self::S3),
            "sftp" => Some(Self::Sftp),
            "azure_blob" => Some(Self::AzureBlob),
            "tencent_cos" => Some(Self::TencentCos),
            "remote" => Some(Self::Remote),
            "onedrive" => Some(Self::OneDrive),
            _ => None,
        }
    }
}

impl std::str::FromStr for DriverType {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value).ok_or(())
    }
}

impl AsRef<str> for DriverType {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// 上传 session 状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "lowercase")]
pub enum UploadSessionStatus {
    #[sea_orm(string_value = "uploading")]
    Uploading,
    #[sea_orm(string_value = "assembling")]
    Assembling,
    #[sea_orm(string_value = "completed")]
    Completed,
    #[sea_orm(string_value = "failed")]
    Failed,
    #[sea_orm(string_value = "presigned")]
    Presigned,
}

/// 上传模式（不存 DB，仅 API 响应用）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum UploadMode {
    Direct,
    Chunked,
    Presigned,
    PresignedMultipart,
    ProviderResumable,
}

impl UploadMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
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

/// Microsoft Graph Drive location mode for OneDrive storage policies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum OneDriveAccountMode {
    Personal,
    WorkOrSchool,
    SharepointSite,
    GroupDrive,
}

impl OneDriveAccountMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Personal => "personal",
            Self::WorkOrSchool => "work_or_school",
            Self::SharepointSite => "sharepoint_site",
            Self::GroupDrive => "group_drive",
        }
    }
}

/// Remote node transport mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum RemoteNodeTransportMode {
    #[sea_orm(string_value = "direct")]
    #[default]
    Direct,
    #[sea_orm(string_value = "reverse_tunnel")]
    ReverseTunnel,
    #[sea_orm(string_value = "auto")]
    Auto,
}

impl RemoteNodeTransportMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::ReverseTunnel => "reverse_tunnel",
            Self::Auto => "auto",
        }
    }

    pub const fn requires_direct_base_url(self) -> bool {
        matches!(self, Self::Direct)
    }

    pub fn resolves_to_reverse_tunnel(self, base_url: &str) -> bool {
        match self {
            Self::Direct => false,
            Self::ReverseTunnel => true,
            Self::Auto => base_url.trim().is_empty(),
        }
    }
}

/// 统一媒体处理器类型（system_config / storage_policy.options）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum MediaProcessorKind {
    Images,
    Lofty,
    VipsCli,
    FfmpegCli,
    FfprobeCli,
    StorageNative,
}

impl MediaProcessorKind {
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
    pub const EMPTY_JSON: &str = "[]";

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

pub fn parse_storage_policy_allowed_types(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_else(|error| {
        if !raw.is_empty() && raw != StoredStoragePolicyAllowedTypes::EMPTY_JSON {
            tracing::warn!("invalid storage policy allowed_types JSON '{raw}': {error}");
        }
        Vec::new()
    })
}

pub fn serialize_storage_policy_allowed_types(
    allowed_types: &[String],
) -> std::result::Result<StoredStoragePolicyAllowedTypes, serde_json::Error> {
    serde_json::to_string(allowed_types).map(StoredStoragePolicyAllowedTypes)
}

pub const OBJECT_MULTIPART_MIN_PART_SIZE: i64 = 5 * 1024 * 1024;

pub fn effective_object_multipart_chunk_size(configured: i64) -> i64 {
    if configured <= 0 {
        OBJECT_MULTIPART_MIN_PART_SIZE
    } else {
        configured.max(OBJECT_MULTIPART_MIN_PART_SIZE)
    }
}
#[cfg(test)]
mod tests {
    use crate::types::{MicrosoftGraphCloud, RemoteDownloadStrategy, RemoteUploadStrategy};
    use validator::Validate;

    use super::{
        DriverType, MediaProcessorKind, ObjectStorageDownloadStrategy, ObjectStorageUploadStrategy,
        OneDriveAccountMode, ProviderDownloadFilenameMode, ProviderDownloadStrategy,
        ProviderResumableUploadStrategy, StoragePolicyOptions, parse_storage_policy_options,
        serialize_storage_policy_options,
    };
    use std::time::Duration;

    #[test]
    fn object_storage_strategy_defaults_to_relay_stream() {
        let options = StoragePolicyOptions::default();
        assert_eq!(
            options.effective_object_storage_upload_strategy(),
            ObjectStorageUploadStrategy::RelayStream
        );
    }

    #[test]
    fn driver_type_wire_values_use_snake_case() {
        let json = serde_json::to_string(&DriverType::TencentCos).unwrap();
        assert_eq!(json, r#""tencent_cos""#);
        assert_eq!(DriverType::TencentCos.as_str(), "tencent_cos");
        assert_eq!(
            serde_json::to_string(&DriverType::AzureBlob).unwrap(),
            r#""azure_blob""#
        );
        assert_eq!(DriverType::AzureBlob.as_str(), "azure_blob");

        let parsed: DriverType = serde_json::from_str(r#""tencent_cos""#).unwrap();
        assert_eq!(parsed, DriverType::TencentCos);
        let parsed: DriverType = serde_json::from_str(r#""azure_blob""#).unwrap();
        assert_eq!(parsed, DriverType::AzureBlob);

        assert!(serde_json::from_str::<DriverType>(r#""tencentcos""#).is_err());
        assert!(serde_json::from_str::<DriverType>(r#""tencentCos""#).is_err());
        assert!(serde_json::from_str::<DriverType>(r#""tencent-cos""#).is_err());
        assert!(serde_json::from_str::<DriverType>(r#""azureBlob""#).is_err());
    }

    #[test]
    fn explicit_object_storage_upload_strategy_maps_to_presigned() {
        let options =
            parse_storage_policy_options(r#"{"object_storage_upload_strategy":"presigned"}"#);
        assert_eq!(
            options.effective_object_storage_upload_strategy(),
            ObjectStorageUploadStrategy::Presigned
        );
    }

    #[test]
    fn legacy_s3_upload_strategy_alias_maps_to_object_storage_upload_strategy() {
        let options = parse_storage_policy_options(r#"{"s3_upload_strategy":"presigned"}"#);
        assert_eq!(
            options.object_storage_upload_strategy,
            Some(ObjectStorageUploadStrategy::Presigned)
        );
    }

    #[test]
    fn object_storage_download_strategy_defaults_to_relay_stream() {
        let options = StoragePolicyOptions::default();
        assert_eq!(
            options.effective_object_storage_download_strategy(),
            ObjectStorageDownloadStrategy::RelayStream
        );
    }

    #[test]
    fn explicit_object_storage_download_strategy_maps_to_presigned() {
        let options =
            parse_storage_policy_options(r#"{"object_storage_download_strategy":"presigned"}"#);
        assert_eq!(
            options.effective_object_storage_download_strategy(),
            ObjectStorageDownloadStrategy::Presigned
        );
    }

    #[test]
    fn legacy_s3_download_strategy_alias_maps_to_object_storage_download_strategy() {
        let options = parse_storage_policy_options(r#"{"s3_download_strategy":"presigned"}"#);
        assert_eq!(
            options.object_storage_download_strategy,
            Some(ObjectStorageDownloadStrategy::Presigned)
        );
    }

    #[test]
    fn s3_path_style_defaults_to_enabled_and_can_be_disabled() {
        let options = parse_storage_policy_options("{}");
        assert!(options.effective_s3_path_style());

        let options = parse_storage_policy_options(r#"{"s3_path_style":false}"#);
        assert!(!options.effective_s3_path_style());
    }

    #[test]
    fn s3_region_defaults_to_auto_and_preserves_configured_value() {
        let options = parse_storage_policy_options("{}");
        assert_eq!(options.effective_s3_region(), "auto");

        let options = parse_storage_policy_options(r#"{"s3_region":" us-east-1 "}"#);
        assert_eq!(options.s3_region.as_deref(), Some("us-east-1"));
        assert_eq!(options.effective_s3_region(), "us-east-1");
        assert_eq!(
            serialize_storage_policy_options(&options)
                .expect("S3 region should serialize")
                .as_ref(),
            r#"{"s3_region":"us-east-1"}"#
        );
    }

    #[test]
    fn blank_s3_region_normalizes_to_default() {
        let options = parse_storage_policy_options(r#"{"s3_region":"  "}"#);

        assert_eq!(options.s3_region, None);
        assert_eq!(options.effective_s3_region(), "auto");
    }

    #[test]
    fn s3_region_validation_rejects_credential_scope_separators() {
        for region in ["us east 1", "us-east-1/extra", "us-east-1\0", "华东-1"] {
            let error = StoragePolicyOptions {
                s3_region: Some(region.to_string()),
                ..Default::default()
            }
            .validate()
            .expect_err("invalid S3 region should fail");

            assert!(error.to_string().contains("s3_region must be"), "{error}");
        }
    }

    #[test]
    fn s3_region_validation_accepts_provider_specific_printable_values_and_max_length() {
        for region in [
            "auto",
            "us-east-1",
            "us-west-004",
            "RegionOne",
            "custom.region_1",
        ] {
            StoragePolicyOptions {
                s3_region: Some(region.to_string()),
                ..Default::default()
            }
            .validate()
            .unwrap_or_else(|error| panic!("provider region '{region}' should be valid: {error}"));
        }

        StoragePolicyOptions {
            s3_region: Some(format!(" {} ", "r".repeat(128))),
            ..Default::default()
        }
        .validate()
        .expect("trimmed 128-byte S3 region should be valid");

        StoragePolicyOptions {
            s3_region: Some("   ".to_string()),
            ..Default::default()
        }
        .validate()
        .expect("blank S3 region should be treated as unconfigured");

        let error = StoragePolicyOptions {
            s3_region: Some("r".repeat(129)),
            ..Default::default()
        }
        .validate()
        .expect_err("129-byte S3 region should fail");
        assert!(error.to_string().contains("s3_region must be"), "{error}");
    }

    #[test]
    fn remote_download_strategy_defaults_to_relay_stream() {
        let options = StoragePolicyOptions::default();
        assert_eq!(
            options.effective_remote_download_strategy(),
            RemoteDownloadStrategy::RelayStream
        );
    }

    #[test]
    fn explicit_remote_presigned_download_strategy_maps_to_presigned() {
        let options = parse_storage_policy_options(r#"{"remote_download_strategy":"presigned"}"#);
        assert_eq!(
            options.effective_remote_download_strategy(),
            RemoteDownloadStrategy::Presigned
        );
    }

    #[test]
    fn explicit_thumbnail_processor_maps_to_media_processor_kind() {
        let options = parse_storage_policy_options(
            r#"{"storage_native_processing_enabled":true,"thumbnail_processor":"storage_native"}"#,
        );
        assert_eq!(
            options.thumbnail_processor,
            Some(MediaProcessorKind::StorageNative)
        );
        assert!(options.storage_native_processing_enabled());
    }

    #[test]
    fn thumbnail_extensions_are_normalized_on_parse() {
        let options = parse_storage_policy_options(
            r#"{"storage_native_processing_enabled":true,"thumbnail_processor":"storage_native","thumbnail_extensions":[" .PNG ","png",".Jpg","","  "]}"#,
        );
        assert_eq!(
            options.thumbnail_extensions,
            vec!["png".to_string(), "jpg".to_string()]
        );
    }

    #[test]
    fn thumbnail_processor_validation_rejects_non_storage_native_values() {
        let options = parse_storage_policy_options(r#"{"thumbnail_processor":"vips_cli"}"#);
        let error = options.validate().unwrap_err();
        assert!(
            error
                .to_string()
                .contains("thumbnail_processor only supports")
        );
    }

    #[test]
    fn storage_native_thumbnail_requires_extensions() {
        let options = parse_storage_policy_options(
            r#"{"storage_native_processing_enabled":true,"thumbnail_processor":"storage_native"}"#,
        );
        let error = options.validate().unwrap_err();
        assert!(
            error
                .to_string()
                .contains("thumbnail_extensions is required")
        );
    }

    #[test]
    fn storage_native_thumbnail_rejects_explicit_disabled_processing_switch() {
        let options = parse_storage_policy_options(
            r#"{"storage_native_processing_enabled":false,"thumbnail_processor":"storage_native","thumbnail_extensions":["png"]}"#,
        );
        let error = options.validate().unwrap_err();
        assert!(
            error
                .to_string()
                .contains("storage_native_processing_enabled cannot be explicitly disabled")
        );
    }

    #[test]
    fn storage_native_thumbnail_accepts_explicit_enabled_processing_switch() {
        let options = parse_storage_policy_options(
            r#"{"storage_native_processing_enabled":true,"thumbnail_processor":"storage_native","thumbnail_extensions":["png"]}"#,
        );

        options
            .validate()
            .expect("explicitly enabled storage-native thumbnail options should be valid");
        assert!(options.storage_native_processing_enabled());
        assert!(options.uses_storage_native_thumbnail());
    }

    #[test]
    fn storage_native_thumbnail_preserves_legacy_missing_processing_switch() {
        let options = parse_storage_policy_options(
            r#"{"thumbnail_processor":"storage_native","thumbnail_extensions":["png"]}"#,
        );

        options
            .validate()
            .expect("legacy storage-native thumbnail options should remain valid");
        assert!(options.storage_native_processing_enabled());
        assert!(options.uses_storage_native_thumbnail());
    }

    #[test]
    fn thumbnail_extensions_require_storage_native_processor() {
        let options = parse_storage_policy_options(r#"{"thumbnail_extensions":["png"]}"#);
        let error = options.validate().unwrap_err();
        assert!(
            error
                .to_string()
                .contains("thumbnail_extensions requires thumbnail_processor")
        );
    }

    #[test]
    fn storage_native_media_metadata_requires_processing_switch() {
        let options =
            parse_storage_policy_options(r#"{"storage_native_media_metadata_enabled":true}"#);
        let error = options.validate().unwrap_err();
        assert!(
            error
                .to_string()
                .contains("storage_native_processing_enabled is required")
        );
    }

    #[test]
    fn storage_native_media_metadata_allows_empty_extensions() {
        let options = parse_storage_policy_options(
            r#"{"storage_native_processing_enabled":true,"storage_native_media_metadata_enabled":true}"#,
        );
        options
            .validate()
            .expect("empty suffix list should be allowed");
        assert!(options.uses_storage_native_media_metadata());
        assert!(!options.storage_native_media_metadata_matches_file_name("clip.mp4"));
    }

    #[test]
    fn media_metadata_extensions_require_storage_native_media_metadata() {
        let options = parse_storage_policy_options(
            r#"{"storage_native_processing_enabled":true,"media_metadata_extensions":["mp4"]}"#,
        );
        let error = options.validate().unwrap_err();
        assert!(
            error
                .to_string()
                .contains("media_metadata_extensions requires")
        );
    }

    #[test]
    fn storage_native_media_metadata_matches_file_name_by_extension() {
        let options = parse_storage_policy_options(
            r#"{"storage_native_processing_enabled":true,"storage_native_media_metadata_enabled":true,"media_metadata_extensions":[" .MP4 ","mp4",".Mov","","  "]}"#,
        );
        assert_eq!(
            options.media_metadata_extensions,
            vec!["mp4".to_string(), "mov".to_string()]
        );
        assert!(options.storage_native_media_metadata_matches_file_name("clip.MP4"));
        assert!(options.storage_native_media_metadata_matches_file_name("movie.mov"));
        assert!(!options.storage_native_media_metadata_matches_file_name("cover.png"));
    }

    #[test]
    fn storage_native_thumbnail_matches_file_name_by_extension() {
        let options = parse_storage_policy_options(
            r#"{"storage_native_processing_enabled":true,"thumbnail_processor":"storage_native","thumbnail_extensions":["png","heic"]}"#,
        );
        assert!(options.storage_native_thumbnail_matches_file_name("cover.PNG"));
        assert!(options.storage_native_thumbnail_matches_file_name("photo.heic"));
        assert!(!options.storage_native_thumbnail_matches_file_name("clip.mp4"));
        assert!(!options.storage_native_thumbnail_matches_file_name("README"));
    }

    #[test]
    fn removed_proxy_tempfile_strategy_falls_back_to_relay_stream() {
        let options = parse_storage_policy_options(r#"{"s3_upload_strategy":"proxy_tempfile"}"#);
        assert_eq!(
            options.effective_object_storage_upload_strategy(),
            ObjectStorageUploadStrategy::RelayStream
        );
    }

    #[test]
    fn s3_timeouts_default_to_safe_values() {
        let options = StoragePolicyOptions::default();
        assert_eq!(
            options.effective_s3_connect_timeout(),
            Duration::from_secs(5)
        );
        assert_eq!(options.effective_s3_read_timeout(), Duration::from_secs(30));
        assert_eq!(
            options.effective_s3_operation_timeout(),
            Duration::from_secs(60 * 60)
        );
    }

    #[test]
    fn explicit_s3_timeouts_override_defaults() {
        let options = parse_storage_policy_options(
            r#"{"s3_connect_timeout_secs":9,"s3_read_timeout_secs":45,"s3_operation_timeout_secs":1200}"#,
        );
        assert_eq!(
            options.effective_s3_connect_timeout(),
            Duration::from_secs(9)
        );
        assert_eq!(options.effective_s3_read_timeout(), Duration::from_secs(45));
        assert_eq!(
            options.effective_s3_operation_timeout(),
            Duration::from_secs(1200)
        );
    }

    #[test]
    fn zero_s3_timeouts_fall_back_to_safe_defaults() {
        let options = parse_storage_policy_options(
            r#"{"s3_connect_timeout_secs":0,"s3_read_timeout_secs":0,"s3_operation_timeout_secs":0}"#,
        );
        assert_eq!(
            options.effective_s3_connect_timeout(),
            Duration::from_secs(5)
        );
        assert_eq!(options.effective_s3_read_timeout(), Duration::from_secs(30));
        assert_eq!(
            options.effective_s3_operation_timeout(),
            Duration::from_secs(60 * 60)
        );
    }

    #[test]
    fn serialize_storage_policy_options_omits_default_fields() {
        let json = serde_json::to_string(&StoragePolicyOptions::default()).unwrap();
        assert_eq!(json, "{}");

        let json = serde_json::to_string(&StoragePolicyOptions {
            object_storage_upload_strategy: Some(ObjectStorageUploadStrategy::Presigned),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(json, r#"{"object_storage_upload_strategy":"presigned"}"#);

        let json = serde_json::to_string(&StoragePolicyOptions {
            object_storage_download_strategy: Some(ObjectStorageDownloadStrategy::Presigned),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(json, r#"{"object_storage_download_strategy":"presigned"}"#);

        let json = serde_json::to_string(&StoragePolicyOptions {
            s3_path_style: Some(false),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(json, r#"{"s3_path_style":false}"#);

        let json = serde_json::to_string(&StoragePolicyOptions {
            remote_download_strategy: Some(RemoteDownloadStrategy::Presigned),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(json, r#"{"remote_download_strategy":"presigned"}"#);

        let json = serde_json::to_string(&StoragePolicyOptions {
            remote_upload_strategy: Some(RemoteUploadStrategy::Presigned),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(json, r#"{"remote_upload_strategy":"presigned"}"#);

        let json = String::from(
            serialize_storage_policy_options(&StoragePolicyOptions {
                storage_native_processing_enabled: Some(true),
                thumbnail_processor: Some(MediaProcessorKind::StorageNative),
                thumbnail_extensions: vec![".PNG".to_string(), "png".to_string()],
                storage_native_media_metadata_enabled: Some(true),
                media_metadata_extensions: vec![".MP4".to_string(), "mp4".to_string()],
                ..Default::default()
            })
            .unwrap(),
        );
        assert_eq!(
            json,
            r#"{"thumbnail_processor":"storage_native","thumbnail_extensions":["png"],"storage_native_processing_enabled":true,"storage_native_media_metadata_enabled":true,"media_metadata_extensions":["mp4"]}"#
        );

        let json = serde_json::to_string(&StoragePolicyOptions {
            s3_operation_timeout_secs: Some(600),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(json, r#"{"s3_operation_timeout_secs":600}"#);

        let json = String::from(
            serialize_storage_policy_options(&StoragePolicyOptions {
                sftp_host_key_fingerprint: Some("  SHA256:abc123  ".to_string()),
                ..Default::default()
            })
            .unwrap(),
        );
        assert_eq!(json, r#"{"sftp_host_key_fingerprint":"SHA256:abc123"}"#);
    }

    #[test]
    fn remote_upload_strategy_defaults_to_relay_stream() {
        let options = parse_storage_policy_options("{}");
        assert_eq!(
            options.effective_remote_upload_strategy(),
            RemoteUploadStrategy::RelayStream
        );
    }

    #[test]
    fn provider_resumable_upload_strategy_defaults_to_server_relay() {
        let options = parse_storage_policy_options("{}");
        assert_eq!(
            options.effective_provider_resumable_upload_strategy(),
            ProviderResumableUploadStrategy::ServerRelay
        );

        let direct = parse_storage_policy_options(
            r#"{"provider_resumable_upload_strategy":"frontend_direct"}"#,
        );
        assert_eq!(
            direct.effective_provider_resumable_upload_strategy(),
            ProviderResumableUploadStrategy::FrontendDirect
        );
    }

    #[test]
    fn provider_download_strategy_defaults_to_server_relay() {
        let options = parse_storage_policy_options("{}");
        assert_eq!(
            options.effective_provider_download_strategy(),
            ProviderDownloadStrategy::ServerRelay
        );

        let direct =
            parse_storage_policy_options(r#"{"provider_download_strategy":"frontend_direct"}"#);
        assert_eq!(
            direct.effective_provider_download_strategy(),
            ProviderDownloadStrategy::FrontendDirect
        );
    }

    #[test]
    fn provider_download_filename_mode_defaults_to_provider_native() {
        let options = StoragePolicyOptions::default();
        assert_eq!(
            options.effective_provider_download_filename_mode(),
            ProviderDownloadFilenameMode::ProviderNative
        );
    }

    #[test]
    fn provider_download_filename_mode_parses_strict_current() {
        let options =
            parse_storage_policy_options(r#"{"provider_download_filename_mode":"strict_current"}"#);
        assert_eq!(
            options.effective_provider_download_filename_mode(),
            ProviderDownloadFilenameMode::StrictCurrent
        );
    }

    #[test]
    fn provider_download_strategy_serializes_canonical_literal_and_rejects_unknown_values() {
        let json = serde_json::to_string(&StoragePolicyOptions {
            provider_download_strategy: Some(ProviderDownloadStrategy::FrontendDirect),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(json, r#"{"provider_download_strategy":"frontend_direct"}"#);

        let invalid = parse_storage_policy_options(r#"{"provider_download_strategy":"presigned"}"#);
        assert_eq!(
            invalid.effective_provider_download_strategy(),
            ProviderDownloadStrategy::ServerRelay
        );
    }

    #[test]
    fn invalid_remote_upload_strategy_falls_back_to_default() {
        let options = parse_storage_policy_options(r#"{"remote_upload_strategy":"chunked"}"#);
        assert_eq!(
            options.effective_remote_upload_strategy(),
            RemoteUploadStrategy::RelayStream
        );
    }

    #[test]
    fn serialize_remote_presigned_strategy_uses_canonical_literal() {
        let json = serde_json::to_string(&StoragePolicyOptions {
            remote_upload_strategy: Some(RemoteUploadStrategy::Presigned),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(json, r#"{"remote_upload_strategy":"presigned"}"#);
    }

    #[test]
    fn onedrive_options_normalize_blank_tenant_and_resolve_defaults() {
        let options = parse_storage_policy_options(
            r#"{"onedrive_account_mode":"work_or_school","onedrive_tenant":"  ","onedrive_drive_id":" drive ","onedrive_root_item_id":" root "}"#,
        );

        assert_eq!(
            options.effective_onedrive_cloud(),
            MicrosoftGraphCloud::Global
        );
        assert_eq!(options.effective_onedrive_tenant(), "common");
        assert_eq!(options.onedrive_drive_id.as_deref(), Some("drive"));
        assert_eq!(options.onedrive_root_item_id.as_deref(), Some("root"));
    }

    #[test]
    fn onedrive_options_default_drive_and_root_are_optional() {
        StoragePolicyOptions {
            onedrive_account_mode: Some(OneDriveAccountMode::WorkOrSchool),
            ..Default::default()
        }
        .validate()
        .expect("work or school OneDrive should resolve the default drive during authorization");
    }

    #[test]
    fn onedrive_options_require_account_mode() {
        let error = StoragePolicyOptions {
            onedrive_drive_id: Some("drive".to_string()),
            ..Default::default()
        }
        .validate()
        .expect_err("missing account mode should fail");

        assert!(
            error
                .to_string()
                .contains("onedrive_account_mode is required"),
            "{error}"
        );
    }

    #[test]
    fn onedrive_group_mode_requires_group_id() {
        let error = StoragePolicyOptions {
            onedrive_account_mode: Some(OneDriveAccountMode::GroupDrive),
            ..Default::default()
        }
        .validate()
        .expect_err("group drive without group id should fail");

        assert!(
            error.to_string().contains("onedrive_group_id is required"),
            "{error}"
        );
    }

    #[test]
    fn onedrive_modes_reject_other_mode_target_ids() {
        let error = StoragePolicyOptions {
            onedrive_account_mode: Some(OneDriveAccountMode::SharepointSite),
            onedrive_site_id: Some("site".to_string()),
            onedrive_group_id: Some("group".to_string()),
            ..Default::default()
        }
        .validate()
        .expect_err("sharepoint site mode should reject group id");

        assert!(
            error
                .to_string()
                .contains("onedrive_group_id is only valid"),
            "{error}"
        );

        let error = StoragePolicyOptions {
            onedrive_account_mode: Some(OneDriveAccountMode::GroupDrive),
            onedrive_site_id: Some("site".to_string()),
            onedrive_group_id: Some("group".to_string()),
            ..Default::default()
        }
        .validate()
        .expect_err("group drive mode should reject site id");

        assert!(
            error.to_string().contains("onedrive_site_id is only valid"),
            "{error}"
        );
    }

    #[test]
    fn onedrive_personal_mode_rejects_china_cloud() {
        let error = StoragePolicyOptions {
            onedrive_cloud: Some(MicrosoftGraphCloud::China),
            onedrive_account_mode: Some(OneDriveAccountMode::Personal),
            ..Default::default()
        }
        .validate()
        .expect_err("personal Microsoft accounts must use global Graph");

        assert!(
            error.to_string().contains("global Microsoft Graph cloud"),
            "{error}"
        );
    }
}

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;
use validator::{Validate, ValidationError};

/// 存储驱动类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
#[serde(rename_all = "lowercase")]
pub enum DriverType {
    #[sea_orm(string_value = "local")]
    Local,
    #[sea_orm(string_value = "s3")]
    S3,
    #[sea_orm(string_value = "remote")]
    Remote,
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
}

/// S3 上传传输策略（存储策略 options JSON）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum S3UploadStrategy {
    /// 服务端将请求体直接中继到 S3，不落本地临时文件
    RelayStream,
    /// 浏览器直传 S3 / MinIO
    Presigned,
}

/// S3 下载传输策略（存储策略 options JSON）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum S3DownloadStrategy {
    /// 服务端从 S3 拉流后回传给客户端
    RelayStream,
    /// 服务端完成鉴权后重定向到 S3 presigned GET URL
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

/// Raw JSON object stored in `storage_policies.options`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, DeriveValueType)]
pub struct StoredStoragePolicyOptions(pub String);

impl StoredStoragePolicyOptions {
    pub const EMPTY_JSON: &str = "{}";

    pub fn empty() -> Self {
        Self(Self::EMPTY_JSON.to_string())
    }
}

impl AsRef<str> for StoredStoragePolicyOptions {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for StoredStoragePolicyOptions {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<StoredStoragePolicyOptions> for String {
    fn from(value: StoredStoragePolicyOptions) -> Self {
        value.0
    }
}

const DEFAULT_S3_CONNECT_TIMEOUT_SECS: u64 = 5;
const DEFAULT_S3_READ_TIMEOUT_SECS: u64 = 30;
const DEFAULT_S3_OPERATION_TIMEOUT_SECS: u64 = 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq, Validate)]
#[validate(schema(function = "validate_storage_policy_options"))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s3_upload_strategy: Option<S3UploadStrategy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s3_download_strategy: Option<S3DownloadStrategy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_download_strategy: Option<RemoteDownloadStrategy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_upload_strategy: Option<RemoteUploadStrategy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[validate(custom(function = "validate_storage_policy_thumbnail_processor"))]
    pub thumbnail_processor: Option<MediaProcessorKind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub thumbnail_extensions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_dedup: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[validate(range(min = 1, message = "s3_connect_timeout_secs must be greater than 0"))]
    pub s3_connect_timeout_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[validate(range(min = 1, message = "s3_read_timeout_secs must be greater than 0"))]
    pub s3_read_timeout_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[validate(range(min = 1, message = "s3_operation_timeout_secs must be greater than 0"))]
    pub s3_operation_timeout_secs: Option<u64>,
}

impl StoragePolicyOptions {
    pub fn effective_s3_upload_strategy(&self) -> S3UploadStrategy {
        self.s3_upload_strategy
            .unwrap_or(S3UploadStrategy::RelayStream)
    }

    pub fn effective_s3_download_strategy(&self) -> S3DownloadStrategy {
        self.s3_download_strategy
            .unwrap_or(S3DownloadStrategy::RelayStream)
    }

    pub fn effective_remote_download_strategy(&self) -> RemoteDownloadStrategy {
        self.remote_download_strategy
            .unwrap_or(RemoteDownloadStrategy::RelayStream)
    }

    pub fn effective_remote_upload_strategy(&self) -> RemoteUploadStrategy {
        self.remote_upload_strategy
            .unwrap_or(RemoteUploadStrategy::RelayStream)
    }

    pub fn uses_storage_native_thumbnail(&self) -> bool {
        self.thumbnail_processor == Some(MediaProcessorKind::StorageNative)
    }

    pub fn normalize_in_place(&mut self) {
        self.thumbnail_extensions =
            normalize_storage_policy_thumbnail_extensions(&self.thumbnail_extensions);
    }

    pub fn normalized(mut self) -> Self {
        self.normalize_in_place();
        self
    }

    pub fn storage_native_thumbnail_matches_file_name(&self, file_name: &str) -> bool {
        if !self.uses_storage_native_thumbnail() {
            return false;
        }

        file_extension_suffix(file_name)
            .map(|extension| {
                self.thumbnail_extensions
                    .iter()
                    .any(|candidate| candidate == &extension)
            })
            .unwrap_or(false)
    }

    pub fn effective_s3_connect_timeout(&self) -> Duration {
        Duration::from_secs(
            self.s3_connect_timeout_secs
                .filter(|secs| *secs > 0)
                .unwrap_or(DEFAULT_S3_CONNECT_TIMEOUT_SECS),
        )
    }

    pub fn effective_s3_read_timeout(&self) -> Duration {
        Duration::from_secs(
            self.s3_read_timeout_secs
                .filter(|secs| *secs > 0)
                .unwrap_or(DEFAULT_S3_READ_TIMEOUT_SECS),
        )
    }

    pub fn effective_s3_operation_timeout(&self) -> Duration {
        Duration::from_secs(
            self.s3_operation_timeout_secs
                .filter(|secs| *secs > 0)
                .unwrap_or(DEFAULT_S3_OPERATION_TIMEOUT_SECS),
        )
    }
}

fn validate_storage_policy_thumbnail_processor(
    value: &MediaProcessorKind,
) -> std::result::Result<(), ValidationError> {
    if *value != MediaProcessorKind::StorageNative {
        let mut error = ValidationError::new("invalid");
        error.message = Some("thumbnail_processor only supports 'storage_native'".into());
        return Err(error);
    }

    Ok(())
}

fn normalize_storage_policy_thumbnail_extensions(values: &[String]) -> Vec<String> {
    let mut normalized = Vec::new();
    for value in values {
        let Some(extension) = normalize_thumbnail_extension(value) else {
            continue;
        };
        if !normalized.iter().any(|candidate| candidate == &extension) {
            normalized.push(extension);
        }
    }
    normalized
}

fn normalize_thumbnail_extension(value: &str) -> Option<String> {
    let normalized = value.trim().trim_start_matches('.').to_ascii_lowercase();
    (!normalized.is_empty()).then_some(normalized)
}

#[inline]
fn has_normalizable_thumbnail_extension(values: &[String]) -> bool {
    values
        .iter()
        .any(|value| !value.trim().trim_start_matches('.').is_empty())
}

fn file_extension_suffix(file_name: &str) -> Option<String> {
    Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .and_then(normalize_thumbnail_extension)
}

fn validate_storage_policy_options(
    value: &StoragePolicyOptions,
) -> std::result::Result<(), ValidationError> {
    let uses_storage_native_thumbnail = value.uses_storage_native_thumbnail();
    let has_thumbnail_extensions =
        has_normalizable_thumbnail_extension(&value.thumbnail_extensions);

    if uses_storage_native_thumbnail && !has_thumbnail_extensions {
        let mut error = ValidationError::new("invalid");
        error.message = Some(
            "thumbnail_extensions is required when thumbnail_processor is 'storage_native'".into(),
        );
        return Err(error);
    }

    if !uses_storage_native_thumbnail && has_thumbnail_extensions {
        let mut error = ValidationError::new("invalid");
        error.message =
            Some("thumbnail_extensions requires thumbnail_processor 'storage_native'".into());
        return Err(error);
    }

    Ok(())
}

pub fn parse_storage_policy_options(options: &str) -> StoragePolicyOptions {
    let mut parsed = serde_json::from_str(options).unwrap_or_else(|e| {
        if !options.is_empty() && options != "{}" {
            tracing::warn!("invalid storage policy options JSON '{options}': {e}");
        }
        StoragePolicyOptions::default()
    });
    parsed.normalize_in_place();
    parsed
}

pub fn serialize_storage_policy_options(
    options: &StoragePolicyOptions,
) -> std::result::Result<StoredStoragePolicyOptions, serde_json::Error> {
    serde_json::to_string(&options.clone().normalized()).map(StoredStoragePolicyOptions)
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

pub const S3_MULTIPART_MIN_PART_SIZE: i64 = 5 * 1024 * 1024;

pub fn effective_s3_multipart_chunk_size(configured: i64) -> i64 {
    if configured <= 0 {
        S3_MULTIPART_MIN_PART_SIZE
    } else {
        configured.max(S3_MULTIPART_MIN_PART_SIZE)
    }
}
#[cfg(test)]
mod tests {
    use crate::types::{RemoteDownloadStrategy, RemoteUploadStrategy};
    use validator::Validate;

    use super::{
        MediaProcessorKind, S3DownloadStrategy, S3UploadStrategy, StoragePolicyOptions,
        parse_storage_policy_options, serialize_storage_policy_options,
    };
    use std::time::Duration;

    #[test]
    fn s3_strategy_defaults_to_relay_stream() {
        let options = StoragePolicyOptions::default();
        assert_eq!(
            options.effective_s3_upload_strategy(),
            S3UploadStrategy::RelayStream
        );
    }

    #[test]
    fn explicit_presigned_strategy_maps_to_presigned() {
        let options = parse_storage_policy_options(r#"{"s3_upload_strategy":"presigned"}"#);
        assert_eq!(
            options.effective_s3_upload_strategy(),
            S3UploadStrategy::Presigned
        );
    }

    #[test]
    fn s3_download_strategy_defaults_to_relay_stream() {
        let options = StoragePolicyOptions::default();
        assert_eq!(
            options.effective_s3_download_strategy(),
            S3DownloadStrategy::RelayStream
        );
    }

    #[test]
    fn explicit_presigned_download_strategy_maps_to_presigned() {
        let options = parse_storage_policy_options(r#"{"s3_download_strategy":"presigned"}"#);
        assert_eq!(
            options.effective_s3_download_strategy(),
            S3DownloadStrategy::Presigned
        );
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
        let options = parse_storage_policy_options(r#"{"thumbnail_processor":"storage_native"}"#);
        assert_eq!(
            options.thumbnail_processor,
            Some(MediaProcessorKind::StorageNative)
        );
    }

    #[test]
    fn thumbnail_extensions_are_normalized_on_parse() {
        let options = parse_storage_policy_options(
            r#"{"thumbnail_processor":"storage_native","thumbnail_extensions":[" .PNG ","png",".Jpg","","  "]}"#,
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
        let options = parse_storage_policy_options(r#"{"thumbnail_processor":"storage_native"}"#);
        let error = options.validate().unwrap_err();
        assert!(
            error
                .to_string()
                .contains("thumbnail_extensions is required")
        );
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
    fn storage_native_thumbnail_matches_file_name_by_extension() {
        let options = parse_storage_policy_options(
            r#"{"thumbnail_processor":"storage_native","thumbnail_extensions":["png","heic"]}"#,
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
            options.effective_s3_upload_strategy(),
            S3UploadStrategy::RelayStream
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
            s3_upload_strategy: Some(S3UploadStrategy::Presigned),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(json, r#"{"s3_upload_strategy":"presigned"}"#);

        let json = serde_json::to_string(&StoragePolicyOptions {
            s3_download_strategy: Some(S3DownloadStrategy::Presigned),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(json, r#"{"s3_download_strategy":"presigned"}"#);

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
                thumbnail_processor: Some(MediaProcessorKind::StorageNative),
                thumbnail_extensions: vec![".PNG".to_string(), "png".to_string()],
                ..Default::default()
            })
            .unwrap(),
        );
        assert_eq!(
            json,
            r#"{"thumbnail_processor":"storage_native","thumbnail_extensions":["png"]}"#
        );

        let json = serde_json::to_string(&StoragePolicyOptions {
            s3_operation_timeout_secs: Some(600),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(json, r#"{"s3_operation_timeout_secs":600}"#);
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
}

use crate::api::response::ApiErrorInfo;
use crate::errors::Result;
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::types::{
    DriverType, RemoteDownloadStrategy, RemoteUploadStrategy, StoragePolicyOptions,
};
use serde::{Deserialize, Serialize};
use std::fmt;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

pub const INTERNAL_STORAGE_PROTOCOL_VERSION: u16 = 2;
pub const INTERNAL_STORAGE_MIN_SUPPORTED_PROTOCOL_VERSION: u16 = 2;
pub const INTERNAL_STORAGE_PROTOCOL_VERSION_LABEL: &str = "v2";
pub const INTERNAL_STORAGE_MIN_SUPPORTED_PROTOCOL_VERSION_LABEL: &str = "v2";
pub const REMOTE_BROWSER_PRESIGNED_CORS_ALLOWED_HEADERS: &str = "content-type, range";
pub const REMOTE_BROWSER_PRESIGNED_CORS_GET_EXPOSE_HEADERS: &str = "Accept-Ranges, Cache-Control, Content-Disposition, Content-Length, Content-Range, Content-Type, ETag";
pub const REMOTE_BROWSER_PRESIGNED_CORS_PUT_EXPOSE_HEADERS: &str = "ETag";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteStorageCapabilities {
    #[serde(default)]
    pub protocol_version: String,
    #[serde(default)]
    pub min_supported_protocol_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_version: Option<String>,
    #[serde(default)]
    pub features: RemoteStorageFeatureFlags,
    #[serde(default)]
    pub browser_cors: RemoteStorageBrowserCorsContract,
    #[serde(default)]
    pub limits: RemoteStorageProtocolLimits,
    #[serde(default)]
    pub supports_list: bool,
    #[serde(default)]
    pub supports_range_read: bool,
    #[serde(default)]
    pub supports_stream_upload: bool,
}

impl Default for RemoteStorageCapabilities {
    fn default() -> Self {
        Self::current()
    }
}

impl RemoteStorageCapabilities {
    pub fn current() -> Self {
        Self {
            protocol_version: INTERNAL_STORAGE_PROTOCOL_VERSION_LABEL.to_string(),
            min_supported_protocol_version: INTERNAL_STORAGE_MIN_SUPPORTED_PROTOCOL_VERSION_LABEL
                .to_string(),
            server_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            features: RemoteStorageFeatureFlags::current(),
            browser_cors: RemoteStorageBrowserCorsContract::current(),
            limits: RemoteStorageProtocolLimits::default(),
            supports_list: true,
            supports_range_read: true,
            supports_stream_upload: true,
        }
    }

    pub fn unknown() -> Self {
        Self {
            protocol_version: "unknown".to_string(),
            min_supported_protocol_version: "unknown".to_string(),
            server_version: None,
            features: RemoteStorageFeatureFlags::default(),
            browser_cors: RemoteStorageBrowserCorsContract::default(),
            limits: RemoteStorageProtocolLimits::default(),
            supports_list: false,
            supports_range_read: false,
            supports_stream_upload: false,
        }
    }

    pub fn from_stored_json(raw: &str) -> Self {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed == "{}" {
            return Self::unknown();
        }

        serde_json::from_str(trimmed).unwrap_or_else(|error| {
            tracing::warn!("invalid remote storage capabilities JSON '{raw}': {error}");
            Self::unknown()
        })
    }

    pub fn validate_protocol(&self, context: &str) -> Result<()> {
        let remote_max = parse_protocol_version(&self.protocol_version).ok_or_else(|| {
            protocol_error(
                context,
                format!(
                    "remote discovery has invalid protocol_version '{}'",
                    self.protocol_version
                ),
            )
        })?;
        let remote_min = if self.min_supported_protocol_version.trim().is_empty() {
            remote_max
        } else {
            parse_protocol_version(&self.min_supported_protocol_version).ok_or_else(|| {
                protocol_error(
                    context,
                    format!(
                        "remote discovery has invalid min_supported_protocol_version '{}'",
                        self.min_supported_protocol_version
                    ),
                )
            })?
        };

        if remote_min > remote_max {
            return Err(protocol_error(
                context,
                format!(
                    "remote discovery declares inverted protocol range {}-{}",
                    version_label(remote_min),
                    version_label(remote_max)
                ),
            ));
        }

        if remote_max < INTERNAL_STORAGE_MIN_SUPPORTED_PROTOCOL_VERSION
            || remote_min > INTERNAL_STORAGE_PROTOCOL_VERSION
        {
            return Err(protocol_error(
                context,
                format!(
                    "local supports {}-{}, remote declares {}-{}",
                    INTERNAL_STORAGE_MIN_SUPPORTED_PROTOCOL_VERSION_LABEL,
                    INTERNAL_STORAGE_PROTOCOL_VERSION_LABEL,
                    version_label(remote_min),
                    version_label(remote_max)
                ),
            ));
        }

        Ok(())
    }

    pub fn validate_for_remote_policy(
        &self,
        remote_node_id: i64,
        policy_id: i64,
        options: &StoragePolicyOptions,
    ) -> Result<()> {
        let context =
            format!("remote storage policy #{policy_id} on remote node #{remote_node_id}");
        self.validate_protocol(&context)?;
        self.ensure_features(&context, &self.base_policy_required_features())?;

        if options.effective_remote_download_strategy() == RemoteDownloadStrategy::Presigned {
            self.ensure_browser_presigned_cors(
                &context,
                &["range"],
                &["Accept-Ranges", "Content-Range", "Content-Length"],
            )?;
        }

        if options.effective_remote_upload_strategy() == RemoteUploadStrategy::Presigned {
            self.ensure_browser_presigned_cors(&context, &["content-type"], &["ETag"])?;
        }

        Ok(())
    }

    pub fn validate_for_binding(
        &self,
        remote_node_id: i64,
        remote_node_name: &str,
        policy_requirements: &[(i64, &StoragePolicyOptions)],
    ) -> Result<()> {
        let context =
            format!("remote node #{remote_node_id} ('{remote_node_name}') binding reload");
        self.validate_protocol(&context)?;
        if policy_requirements.is_empty() {
            return Ok(());
        }

        self.ensure_features(&context, &self.base_policy_required_features())?;
        for (policy_id, options) in policy_requirements {
            if options.effective_remote_download_strategy() == RemoteDownloadStrategy::Presigned {
                self.ensure_browser_presigned_cors(
                    &format!("{context}; policy #{policy_id} requires remote presigned download"),
                    &["range"],
                    &["Accept-Ranges", "Content-Range", "Content-Length"],
                )?;
            }
            if options.effective_remote_upload_strategy() == RemoteUploadStrategy::Presigned {
                self.ensure_browser_presigned_cors(
                    &format!("{context}; policy #{policy_id} requires remote presigned upload"),
                    &["content-type"],
                    &["ETag"],
                )?;
            }
        }

        Ok(())
    }

    fn ensure_features(&self, context: &str, required: &[(&'static str, bool)]) -> Result<()> {
        let missing = required
            .iter()
            .filter_map(|(name, supported)| (!*supported).then_some(*name))
            .collect::<Vec<_>>();
        if missing.is_empty() {
            return Ok(());
        }

        Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!(
                "{context}: remote internal storage protocol is missing required feature(s): {}; remote declared features: {:?}",
                missing.join(", "),
                self.features
            ),
        ))
    }

    fn base_policy_required_features(&self) -> Vec<(&'static str, bool)> {
        vec![
            ("object_get", self.features.object_get),
            ("object_head", self.features.object_head),
            ("object_put", self.features.object_put),
            ("object_delete", self.features.object_delete),
            ("metadata", self.features.metadata),
            ("range_get", self.features.range_get),
            ("accept_ranges_header", self.features.accept_ranges_header),
            ("list", self.features.list),
            ("compose", self.features.compose),
        ]
    }

    fn ensure_browser_presigned_cors(
        &self,
        context: &str,
        required_allowed_headers: &[&str],
        required_exposed_headers: &[&str],
    ) -> Result<()> {
        self.ensure_features(
            context,
            &[(
                "browser_presigned_cors",
                self.features.browser_presigned_cors,
            )],
        )?;

        let missing_allowed = required_allowed_headers
            .iter()
            .filter(|header| !contains_header(&self.browser_cors.allowed_headers, header))
            .copied()
            .collect::<Vec<_>>();
        let missing_exposed = required_exposed_headers
            .iter()
            .filter(|header| !contains_header(&self.browser_cors.exposed_headers, header))
            .copied()
            .collect::<Vec<_>>();

        if missing_allowed.is_empty() && missing_exposed.is_empty() {
            return Ok(());
        }

        let mut details = Vec::new();
        if !missing_allowed.is_empty() {
            details.push(format!(
                "allowed_headers missing {}",
                missing_allowed.join(", ")
            ));
        }
        if !missing_exposed.is_empty() {
            details.push(format!(
                "exposed_headers missing {}",
                missing_exposed.join(", ")
            ));
        }

        Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!(
                "{context}: remote internal storage browser CORS contract is incomplete: {}; allowed_headers={:?}; exposed_headers={:?}",
                details.join("; "),
                self.browser_cors.allowed_headers,
                self.browser_cors.exposed_headers
            ),
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteStorageFeatureFlags {
    #[serde(default)]
    pub object_get: bool,
    #[serde(default)]
    pub object_head: bool,
    #[serde(default)]
    pub object_put: bool,
    #[serde(default)]
    pub object_delete: bool,
    #[serde(default)]
    pub list: bool,
    #[serde(default)]
    pub range_get: bool,
    #[serde(default)]
    pub accept_ranges_header: bool,
    #[serde(default)]
    pub browser_presigned_cors: bool,
    #[serde(default)]
    pub compose: bool,
    #[serde(default)]
    pub metadata: bool,
}

impl RemoteStorageFeatureFlags {
    pub fn current() -> Self {
        Self {
            object_get: true,
            object_head: true,
            object_put: true,
            object_delete: true,
            list: true,
            range_get: true,
            accept_ranges_header: true,
            browser_presigned_cors: true,
            compose: true,
            metadata: true,
        }
    }
}

impl Default for RemoteStorageFeatureFlags {
    fn default() -> Self {
        Self {
            object_get: false,
            object_head: false,
            object_put: false,
            object_delete: false,
            list: false,
            range_get: false,
            accept_ranges_header: false,
            browser_presigned_cors: false,
            compose: false,
            metadata: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteStorageBrowserCorsContract {
    #[serde(default)]
    pub allowed_headers: Vec<String>,
    #[serde(default)]
    pub exposed_headers: Vec<String>,
}

impl RemoteStorageBrowserCorsContract {
    pub fn current() -> Self {
        Self {
            allowed_headers: csv_header_values(REMOTE_BROWSER_PRESIGNED_CORS_ALLOWED_HEADERS),
            exposed_headers: csv_header_values(REMOTE_BROWSER_PRESIGNED_CORS_GET_EXPOSE_HEADERS),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteStorageProtocolLimits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_ingress_size: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compose_max_parts: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compose_max_object_size: Option<i64>,
}

fn protocol_error(context: &str, detail: String) -> crate::errors::AsterError {
    storage_driver_error(
        StorageErrorKind::Misconfigured,
        format!("{context}: remote internal storage protocol incompatible: {detail}"),
    )
}

fn parse_protocol_version(value: &str) -> Option<u16> {
    value
        .trim()
        .strip_prefix('v')
        .or_else(|| value.trim().strip_prefix('V'))
        .unwrap_or_else(|| value.trim())
        .parse::<u16>()
        .ok()
}

fn version_label(version: u16) -> String {
    format!("v{version}")
}

fn csv_header_values(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn contains_header(headers: &[String], expected: &str) -> bool {
    headers
        .iter()
        .any(|header| header.eq_ignore_ascii_case(expected))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct RemoteStorageListResponse {
    pub items: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteStorageObjectMetadata {
    pub size: u64,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteBindingSyncRequest {
    pub name: String,
    pub is_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteIngressProfileInfo {
    pub profile_key: String,
    pub name: String,
    pub driver_type: DriverType,
    pub endpoint: String,
    pub bucket: String,
    pub base_path: String,
    pub max_file_size: i64,
    pub is_default: bool,
    pub desired_revision: i64,
    pub applied_revision: i64,
    pub last_error: String,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "driver_type", rename_all = "lowercase")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum RemoteCreateIngressProfileRequest {
    Local(RemoteCreateLocalIngressProfileRequest),
    S3(RemoteCreateS3IngressProfileRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteCreateLocalIngressProfileRequest {
    pub name: String,
    pub base_path: String,
    pub max_file_size: i64,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteCreateS3IngressProfileRequest {
    pub name: String,
    pub endpoint: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub base_path: String,
    pub max_file_size: i64,
    #[serde(default)]
    pub is_default: bool,
}

impl fmt::Debug for RemoteCreateS3IngressProfileRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RemoteCreateS3IngressProfileRequest")
            .field("name", &self.name)
            .field("endpoint", &self.endpoint)
            .field("bucket", &self.bucket)
            .field("access_key", &"<redacted>")
            .field("secret_key", &"<redacted>")
            .field("base_path", &self.base_path)
            .field("max_file_size", &self.max_file_size)
            .field("is_default", &self.is_default)
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteUpdateIngressProfileRequest {
    pub name: Option<String>,
    pub driver_type: Option<DriverType>,
    pub endpoint: Option<String>,
    pub bucket: Option<String>,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
    pub base_path: Option<String>,
    pub max_file_size: Option<i64>,
    pub is_default: Option<bool>,
}

impl fmt::Debug for RemoteUpdateIngressProfileRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RemoteUpdateIngressProfileRequest")
            .field("name", &self.name)
            .field("driver_type", &self.driver_type)
            .field("endpoint", &self.endpoint)
            .field("bucket", &self.bucket)
            .field(
                "access_key",
                &self.access_key.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "secret_key",
                &self.secret_key.as_ref().map(|_| "<redacted>"),
            )
            .field("base_path", &self.base_path)
            .field("max_file_size", &self.max_file_size)
            .field("is_default", &self.is_default)
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteStorageComposeRequest {
    pub target_key: String,
    pub part_keys: Vec<String>,
    pub expected_size: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteStorageComposeResponse {
    pub bytes_written: u64,
}

#[derive(Debug, Deserialize)]
pub(super) struct ApiEnvelope<T> {
    pub(super) code: i32,
    pub(super) msg: String,
    pub(super) data: Option<T>,
    pub(super) error: Option<ApiErrorInfo>,
}

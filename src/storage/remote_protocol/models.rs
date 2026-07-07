use crate::api::api_error_code::ApiErrorCode;
use crate::errors::Result;
use crate::storage::StorageCapacityInfo;
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::types::DriverType;
use serde::{Deserialize, Serialize};
use std::fmt;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

pub const INTERNAL_STORAGE_PROTOCOL_VERSION: u16 = 5;
pub const INTERNAL_STORAGE_MIN_SUPPORTED_PROTOCOL_VERSION: u16 = 4;
pub const INTERNAL_STORAGE_PROTOCOL_VERSION_LABEL: &str = "v5";
pub const INTERNAL_STORAGE_MIN_SUPPORTED_PROTOCOL_VERSION_LABEL: &str = "v4";
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
    // TODO(remote-storage-target): this wire field remains `managed_ingress`
    // for internal protocol v4/v5 compatibility. Keep the Rust payload shape
    // target-named, but do not rename the serialized field until the primary /
    // follower protocol can negotiate a successor capability key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub managed_ingress: Option<RemoteStorageTargetCapabilities>,
    #[serde(default)]
    pub supports_list: bool,
    #[serde(default)]
    pub supports_range_read: bool,
    #[serde(default)]
    pub supports_stream_upload: bool,
    #[serde(default)]
    pub supports_capacity: bool,
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
            managed_ingress: Some(RemoteStorageTargetCapabilities::default()),
            supports_list: true,
            supports_range_read: true,
            supports_stream_upload: true,
            supports_capacity: true,
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
            managed_ingress: None,
            supports_list: false,
            supports_range_read: false,
            supports_stream_upload: false,
            supports_capacity: false,
        }
    }

    pub fn with_remote_storage_target_driver_types(
        mut self,
        driver_types: Vec<DriverType>,
    ) -> Self {
        self.managed_ingress = Some(RemoteStorageTargetCapabilities::from_known_driver_types(
            driver_types,
        ));
        self
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
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteStorageTargetCapabilities {
    pub enabled: bool,
    #[serde(default)]
    pub driver_types: Vec<RemoteStorageTargetDriverType>,
}

impl RemoteStorageTargetCapabilities {
    pub fn from_known_driver_types(driver_types: Vec<DriverType>) -> Self {
        Self {
            enabled: !driver_types.is_empty(),
            driver_types: driver_types
                .into_iter()
                .map(RemoteStorageTargetDriverType::from_known_driver_type)
                .collect(),
        }
    }

    pub fn supports_known_driver(&self, driver_type: DriverType) -> bool {
        self.enabled
            && self
                .driver_types
                .iter()
                .any(|candidate| candidate.matches_known_driver(driver_type))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteStorageTargetDriverType(String);

impl RemoteStorageTargetDriverType {
    pub fn from_known_driver_type(driver_type: DriverType) -> Self {
        Self(driver_type.as_str().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn as_known_driver_type(&self) -> Option<DriverType> {
        self.0.parse().ok()
    }

    pub fn matches_known_driver(&self, driver_type: DriverType) -> bool {
        self.as_str() == driver_type.as_str()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[derive(Default)]
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
            exposed_headers: csv_header_values_union(&[
                REMOTE_BROWSER_PRESIGNED_CORS_GET_EXPOSE_HEADERS,
                REMOTE_BROWSER_PRESIGNED_CORS_PUT_EXPOSE_HEADERS,
            ]),
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

fn csv_header_values_union(raw_values: &[&str]) -> Vec<String> {
    raw_values
        .iter()
        .flat_map(|raw| csv_header_values(raw))
        .fold(Vec::new(), |mut headers, header| {
            if !headers
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(&header))
            {
                headers.push(header);
            }
            headers
        })
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct RemoteStorageListResponse {
    pub items: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteStorageCapacityResponse {
    pub capacity: StorageCapacityInfo,
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
pub struct RemoteStorageTargetInfo {
    pub target_key: String,
    pub name: String,
    pub driver_type: DriverType,
    pub endpoint: String,
    pub bucket: String,
    pub base_path: String,
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
pub enum RemoteCreateStorageTargetRequest {
    Local(RemoteCreateLocalStorageTargetRequest),
    S3(RemoteCreateS3StorageTargetRequest),
}

impl RemoteCreateStorageTargetRequest {
    pub fn driver_type(&self) -> DriverType {
        match self {
            Self::Local(_) => DriverType::Local,
            Self::S3(_) => DriverType::S3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteCreateLocalStorageTargetRequest {
    pub name: String,
    pub base_path: String,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteCreateS3StorageTargetRequest {
    pub name: String,
    pub endpoint: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub base_path: String,
    #[serde(default)]
    pub is_default: bool,
}

impl fmt::Debug for RemoteCreateS3StorageTargetRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RemoteCreateS3StorageTargetRequest")
            .field("name", &self.name)
            .field("endpoint", &self.endpoint)
            .field("bucket", &self.bucket)
            .field("access_key", &"<redacted>")
            .field("secret_key", &"<redacted>")
            .field("base_path", &self.base_path)
            .field("is_default", &self.is_default)
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteUpdateStorageTargetRequest {
    pub name: Option<String>,
    pub driver_type: Option<DriverType>,
    pub endpoint: Option<String>,
    pub bucket: Option<String>,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
    pub base_path: Option<String>,
    pub is_default: Option<bool>,
}

impl fmt::Debug for RemoteUpdateStorageTargetRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RemoteUpdateStorageTargetRequest")
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
    pub(super) code: ApiErrorCode,
    pub(super) msg: String,
    pub(super) data: Option<T>,
}

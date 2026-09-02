use crate::api::api_error_code::ApiErrorCode;
use crate::errors::Result;
use aster_drive_model::types::{RemoteStorageTargetDriverKind, ResolvedRemoteTransport};
use aster_drive_storage::StorageCapacityInfo;
use aster_drive_storage::StorageErrorKind;
use serde::{Deserialize, Serialize};
use std::fmt;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

pub const INTERNAL_STORAGE_PROTOCOL_VERSION: u16 = 6;
pub const INTERNAL_STORAGE_MIN_SUPPORTED_PROTOCOL_VERSION: u16 = 6;
pub const INTERNAL_STORAGE_PROTOCOL_VERSION_LABEL: &str = "v6";
pub const INTERNAL_STORAGE_MIN_SUPPORTED_PROTOCOL_VERSION_LABEL: &str = "v6";
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
    #[serde(
        default,
        rename = "remote_storage_target",
        skip_serializing_if = "Option::is_none"
    )]
    pub remote_storage_target: Option<RemoteStorageTargetCapabilities>,
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
            remote_storage_target: Some(RemoteStorageTargetCapabilities::default()),
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
            remote_storage_target: None,
            supports_list: false,
            supports_range_read: false,
            supports_stream_upload: false,
            supports_capacity: false,
        }
    }

    pub fn with_remote_storage_target_driver_types(
        mut self,
        driver_types: Vec<RemoteStorageTargetDriverKind>,
    ) -> Self {
        self.remote_storage_target = Some(
            RemoteStorageTargetCapabilities::from_known_driver_types(driver_types),
        );
        self
    }

    pub fn with_remote_storage_target_connector_ids(mut self, connector_ids: Vec<String>) -> Self {
        self.remote_storage_target = Some(RemoteStorageTargetCapabilities {
            enabled: !connector_ids.is_empty(),
            driver_types: Vec::new(),
            connector_ids,
        });
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
    /// V6 connector identities. `driver_types` is retained in the Rust model
    /// only for decoding old fixtures and will be removed with the 0.7.0
    /// cleanup once all protocol snapshots are connector-id based.
    #[serde(default)]
    pub connector_ids: Vec<String>,
}

impl RemoteStorageTargetCapabilities {
    pub fn from_known_driver_types(driver_types: Vec<RemoteStorageTargetDriverKind>) -> Self {
        Self {
            enabled: !driver_types.is_empty(),
            driver_types: driver_types
                .into_iter()
                .map(RemoteStorageTargetDriverType::from_known_driver_type)
                .collect(),
            connector_ids: Vec::new(),
        }
    }

    pub fn with_connector_ids(mut self, connector_ids: Vec<String>) -> Self {
        self.connector_ids = connector_ids;
        self
    }

    /// Returns whether a connector identity is advertised by the follower.
    /// V6 peers use connector IDs as the source of truth; the legacy
    /// `driver_types` list is consulted only when no IDs are present so old
    /// snapshots remain readable during the migration window.
    pub fn supports_connector_id(&self, connector_id: &str) -> bool {
        if !self.enabled {
            return false;
        }
        if !self.connector_ids.is_empty() {
            return self.connector_ids.iter().any(|candidate| {
                candidate == connector_id
                    || candidate
                        .rsplit_once('.')
                        .map(|(_, suffix)| {
                            connector_id
                                .rsplit_once('.')
                                .map(|(_, wanted)| suffix.replace('-', "_") == wanted)
                                .unwrap_or(false)
                        })
                        .unwrap_or(false)
            });
        }
        connector_id
            .rsplit_once('.')
            .and_then(|(_, suffix)| suffix.parse::<RemoteStorageTargetDriverKind>().ok())
            .is_some_and(|driver| self.supports_known_driver_legacy(driver))
    }

    fn supports_known_driver_legacy(&self, driver_type: RemoteStorageTargetDriverKind) -> bool {
        self.driver_types
            .iter()
            .any(|candidate| candidate.matches_known_driver(driver_type))
    }

    pub fn supports_known_driver(&self, driver_type: RemoteStorageTargetDriverKind) -> bool {
        let connector_id = format!("asterdrive.storage.{}", driver_type.as_str());
        self.supports_connector_id(&connector_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteStorageTargetDriverType(String);

impl RemoteStorageTargetDriverType {
    pub fn from_known_driver_type(driver_type: RemoteStorageTargetDriverKind) -> Self {
        Self(driver_type.as_str().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn as_known_driver_type(&self) -> Option<RemoteStorageTargetDriverKind> {
        self.0.parse().ok()
    }

    pub fn matches_known_driver(&self, driver_type: RemoteStorageTargetDriverKind) -> bool {
        self.as_str() == driver_type.as_str()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[derive(Default)]
pub struct RemoteStorageFeatureFlags {
    #[serde(default)]
    pub binding_state_pull: bool,
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
            binding_state_pull: true,
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
    crate::errors::storage_driver_error(
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<u64>,
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
    #[serde(default)]
    pub resolved_transport: ResolvedRemoteTransport,
    #[serde(default = "default_binding_revision")]
    pub desired_revision: i64,
}

const fn default_binding_revision() -> i64 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteBindingDesiredState {
    pub name: String,
    pub is_enabled: bool,
    pub resolved_transport: ResolvedRemoteTransport,
    pub desired_revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteStorageTargetInfo {
    pub target_key: String,
    pub name: String,
    /// Stable connector identity. Optional only for decoding pre-V6 fixtures;
    /// all V6 responses populate it.
    #[serde(default)]
    pub connector_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connector_config: Option<aster_drive_storage::ConnectorConfigEnvelope>,
    /// Legacy flattened presentation field retained for in-process migration
    /// and old fixtures only. It is deliberately omitted from V6 wire output.
    #[serde(skip)]
    pub endpoint: String,
    #[serde(skip)]
    pub bucket: String,
    #[serde(skip)]
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
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteCreateStorageTargetRequest {
    pub name: String,
    pub connector_config: aster_drive_storage::ConnectorConfigEnvelope<serde_json::Value>,
    #[serde(default)]
    pub credential: Option<serde_json::Value>,
    #[serde(default)]
    pub is_default: bool,
}

impl RemoteCreateStorageTargetRequest {
    pub fn connector_id(&self) -> &aster_drive_storage::ConnectorId {
        &self.connector_config.connector_id
    }
}

#[derive(Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteUpdateStorageTargetRequest {
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connector_config: Option<aster_drive_storage::ConnectorConfigEnvelope<serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<serde_json::Value>,
    /// Legacy 0.5.0 fields retained only for in-process migration fixtures.
    /// TODO(remote-storage-target-0.7.0): remove these members entirely.
    pub driver_type: Option<RemoteStorageTargetDriverKind>,
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
            .field("connector_config", &self.connector_config)
            .field(
                "credential",
                &self.credential.as_ref().map(|_| "<redacted>"),
            )
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
pub(crate) struct ApiEnvelope<T> {
    pub(crate) code: ApiErrorCode,
    pub(crate) msg: String,
    pub(crate) data: Option<T>,
}

#[cfg(test)]
mod tests {
    use super::{RemoteBindingSyncRequest, RemoteStorageFeatureFlags};

    #[test]
    fn binding_sync_defaults_legacy_payloads_to_reverse_tunnel_state() {
        let request: RemoteBindingSyncRequest = serde_json::from_value(serde_json::json!({
            "name": "legacy-primary",
            "is_enabled": true
        }))
        .expect("legacy binding sync payload should remain readable");

        assert_eq!(
            request.resolved_transport,
            aster_drive_model::types::ResolvedRemoteTransport::ReverseTunnel
        );
        assert_eq!(request.desired_revision, 1);
    }

    #[test]
    fn binding_state_pull_requires_an_explicit_current_capability() {
        let legacy: RemoteStorageFeatureFlags =
            serde_json::from_value(serde_json::json!({})).expect("legacy features should decode");
        assert!(!legacy.binding_state_pull);
        assert!(RemoteStorageFeatureFlags::current().binding_state_pull);
    }
}

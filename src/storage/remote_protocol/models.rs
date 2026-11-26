use crate::api::api_error_code::ApiErrorCode;
use crate::errors::Result;
use crate::storage::StorageConnectionInput;
use aster_drive_model::types::ResolvedRemoteTransport;
use aster_drive_storage::StorageCapacityInfo;
use aster_drive_storage::StorageErrorKind;
use serde::{Deserialize, Serialize};
use std::fmt;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Highest internal storage protocol version this build speaks.
pub const INTERNAL_STORAGE_PROTOCOL_VERSION: u16 = 7;
/// Oldest remote protocol version this build interoperates with.
pub const INTERNAL_STORAGE_MIN_SUPPORTED_PROTOCOL_VERSION: u16 = 7;
/// Wire label for [`INTERNAL_STORAGE_PROTOCOL_VERSION`].
pub const INTERNAL_STORAGE_PROTOCOL_VERSION_LABEL: &str = "v7";
/// Wire label for [`INTERNAL_STORAGE_MIN_SUPPORTED_PROTOCOL_VERSION`].
pub const INTERNAL_STORAGE_MIN_SUPPORTED_PROTOCOL_VERSION_LABEL: &str = "v7";
/// `Access-Control-Allow-Headers` value advertised for browser presigned storage requests.
pub const REMOTE_BROWSER_PRESIGNED_CORS_ALLOWED_HEADERS: &str = "content-type, range";
/// `Access-Control-Expose-Headers` value applied to presigned download responses.
pub const REMOTE_BROWSER_PRESIGNED_CORS_GET_EXPOSE_HEADERS: &str = "Accept-Ranges, Cache-Control, Content-Disposition, Content-Length, Content-Range, Content-Type, ETag";
/// `Access-Control-Expose-Headers` value applied to presigned upload responses.
pub const REMOTE_BROWSER_PRESIGNED_CORS_PUT_EXPOSE_HEADERS: &str = "ETag";

/// Capability snapshot advertised by the remote node's internal storage
/// protocol. All fields decode with defaults so older followers can read newer
/// payloads; use [`Self::validate_protocol`] to check version compatibility.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteStorageCapabilities {
    /// Highest protocol version the remote supports, e.g. `"v7"`.
    #[serde(default)]
    pub protocol_version: String,
    /// Oldest protocol version the remote still accepts; empty means it equals `protocol_version`.
    #[serde(default)]
    pub min_supported_protocol_version: String,
    /// Remote build version string, when reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_version: Option<String>,
    /// Per-operation feature switches.
    #[serde(default)]
    pub features: RemoteStorageFeatureFlags,
    /// CORS contract applied to browser presigned requests.
    #[serde(default)]
    pub browser_cors: RemoteStorageBrowserCorsContract,
    /// Ingress and compose size limits enforced by the remote.
    #[serde(default)]
    pub limits: RemoteStorageProtocolLimits,
    /// Storage-target subsystem capabilities, absent on remotes that predate it.
    #[serde(
        default,
        rename = "remote_storage_target",
        skip_serializing_if = "Option::is_none"
    )]
    pub remote_storage_target: Option<RemoteStorageTargetCapabilities>,
    /// Whether the remote serves paginated object listing.
    #[serde(default)]
    pub supports_list: bool,
    /// Whether the remote serves ranged object reads.
    #[serde(default)]
    pub supports_range_read: bool,
    /// Whether the remote accepts streamed uploads.
    #[serde(default)]
    pub supports_stream_upload: bool,
    /// Whether the remote reports storage capacity.
    #[serde(default)]
    pub supports_capacity: bool,
    /// Per-target runtime capabilities reported by remotes that support the storage-target subsystem.
    #[serde(default)]
    pub target_runtime_capabilities: Vec<RemoteStorageTargetRuntimeCapabilities>,
}

impl Default for RemoteStorageCapabilities {
    fn default() -> Self {
        Self::current()
    }
}

impl RemoteStorageCapabilities {
    /// Capability snapshot describing this build's own protocol surface.
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
            target_runtime_capabilities: Vec::new(),
        }
    }

    /// Placeholder for a missing or unparsable capability payload: no version
    /// declared and every feature flag off.
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
            target_runtime_capabilities: Vec::new(),
        }
    }

    /// Advertises the storage-target subsystem with the given connector ids;
    /// an empty list disables the subsystem.
    pub fn with_remote_storage_target_connector_ids(mut self, connector_ids: Vec<String>) -> Self {
        self.remote_storage_target = Some(RemoteStorageTargetCapabilities {
            enabled: !connector_ids.is_empty(),
            connector_ids,
        });
        self
    }

    /// Parses a persisted capability JSON snapshot; empty, `{}` or invalid
    /// input falls back to [`Self::unknown`] instead of erroring.
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

    /// Checks the remote's declared protocol range against the locally
    /// supported range; errors tagged with `context` on malformed or
    /// non-overlapping declarations.
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

/// Capability snapshot of the connector actually bound to one remote target.
/// The primary must prefer this target-scoped value over Remote's generic
/// protocol limits when planning storage migration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteStorageTargetRuntimeCapabilities {
    /// Storage target these capabilities describe.
    pub target_key: String,
    /// Connector currently bound to the target.
    pub connector_id: String,
    /// Target config revision the remote has applied; planning must not
    /// assume capabilities from a newer revision.
    pub applied_revision: i64,
    /// The bound connector serves efficient ranged reads.
    #[serde(default)]
    pub range_read: bool,
    /// The bound connector accepts streamed uploads.
    #[serde(default)]
    pub stream_upload: bool,
    /// Multipart upload constraints, when the connector is multipart-capable.
    #[serde(default)]
    pub multipart: Option<RemoteMultipartCapabilities>,
}

/// Multipart upload constraints of the connector bound to a remote target,
/// reported so the primary can plan part sizes within remote limits.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteMultipartCapabilities {
    /// Minimum part size in bytes accepted by the remote connector.
    pub min_part_size: u64,
    /// Maximum part size in bytes; `None` means unbounded.
    pub max_part_size: Option<u64>,
    /// Maximum number of parts per upload.
    pub max_parts: u64,
    /// The connector consumes streaming readers natively for uploads.
    pub native_reader_upload: bool,
    /// Present only in buffered upload mode: the largest upload the connector
    /// accepts in bytes; `None` when the connector streams natively.
    pub buffered_reader_max_size: Option<u64>,
}

/// Whether the remote exposes the storage-target subsystem and which connector
/// types it enables there.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteStorageTargetCapabilities {
    /// Whether any storage target is exposed over the protocol.
    pub enabled: bool,
    /// Connector ids the remote accepts for target creation.
    #[serde(default)]
    pub connector_ids: Vec<String>,
}

impl RemoteStorageTargetCapabilities {
    /// Replaces the advertised connector ids.
    pub fn with_connector_ids(mut self, connector_ids: Vec<String>) -> Self {
        self.connector_ids = connector_ids;
        self
    }

    /// Reports whether the subsystem is enabled and lists `connector_id`.
    pub fn supports_connector_id(&self, connector_id: &str) -> bool {
        self.enabled
            && self
                .connector_ids
                .iter()
                .any(|candidate| candidate == connector_id)
    }
}

/// Per-operation feature switches for the internal storage protocol. All
/// flags default to `false` so legacy payloads degrade to "unsupported".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[derive(Default)]
pub struct RemoteStorageFeatureFlags {
    /// The remote can pull binding desired state from the primary control plane.
    #[serde(default)]
    pub binding_state_pull: bool,
    /// Object GET is served.
    #[serde(default)]
    pub object_get: bool,
    /// Object HEAD is served.
    #[serde(default)]
    pub object_head: bool,
    /// Object PUT is served.
    #[serde(default)]
    pub object_put: bool,
    /// Object DELETE is served.
    #[serde(default)]
    pub object_delete: bool,
    /// Paginated object listing is served.
    #[serde(default)]
    pub list: bool,
    /// Ranged object GET is served.
    #[serde(default)]
    pub range_get: bool,
    /// Ranged responses carry `Accept-Ranges`.
    #[serde(default)]
    pub accept_ranges_header: bool,
    /// Browser presigned requests get the advertised CORS contract.
    #[serde(default)]
    pub browser_presigned_cors: bool,
    /// Server-side object composition is served.
    #[serde(default)]
    pub compose: bool,
    /// Object metadata endpoint is served.
    #[serde(default)]
    pub metadata: bool,
}

impl RemoteStorageFeatureFlags {
    /// Feature flags advertised by this build: every currently defined operation.
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

/// CORS contract applied to browser presigned storage requests on the remote.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteStorageBrowserCorsContract {
    /// Header names the browser may send on preflighted requests.
    #[serde(default)]
    pub allowed_headers: Vec<String>,
    /// Header names exposed to browser scripts on responses.
    #[serde(default)]
    pub exposed_headers: Vec<String>,
}

impl RemoteStorageBrowserCorsContract {
    /// Contract advertised by this build, derived from the module CORS constants.
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

/// Size limits the remote enforces; `None` means the limit is not declared.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteStorageProtocolLimits {
    /// Maximum accepted upload size in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_ingress_size: Option<i64>,
    /// Maximum part count for server-side compose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compose_max_parts: Option<i32>,
    /// Maximum composed object size in bytes.
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

/// One page of the remote's object listing.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct RemoteStorageListResponse {
    /// Object keys in this page, ascending by key.
    pub items: Vec<String>,
    /// Cursor to pass to the next page; `None` marks the last page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Response payload of the capacity endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteStorageCapacityResponse {
    /// Observed capacity, byte fields as reported by the connector.
    pub capacity: StorageCapacityInfo,
}

/// Object metadata reported by the remote.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteStorageObjectMetadata {
    /// Object size in bytes.
    pub size: u64,
    /// Stored content type, when the connector keeps one.
    pub content_type: Option<String>,
}

/// Desired binding state pushed by the primary to a remote follower.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteBindingSyncRequest {
    /// Binding name as configured on the primary.
    pub name: String,
    /// Whether the binding should serve traffic on the follower.
    pub is_enabled: bool,
    /// Transport the follower must accept for this binding; legacy payloads
    /// without this field default to reverse tunnel.
    #[serde(default)]
    pub resolved_transport: ResolvedRemoteTransport,
    /// Monotonic desired-state revision; legacy payloads default to `1`.
    #[serde(default = "default_binding_revision")]
    pub desired_revision: i64,
}

const fn default_binding_revision() -> i64 {
    1
}

/// Binding desired state as persisted by the follower's binding registry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteBindingDesiredState {
    /// Binding name as configured on the primary.
    pub name: String,
    /// Whether the binding serves traffic on the follower.
    pub is_enabled: bool,
    /// Transport the follower must accept for this binding.
    pub resolved_transport: ResolvedRemoteTransport,
    /// Monotonic desired-state revision.
    pub desired_revision: i64,
}

/// Follower-side view of one storage target, as returned by target endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteStorageTargetInfo {
    /// Follower-side identifier of the target.
    pub target_key: String,
    /// Display name of the target.
    pub name: String,
    /// Connector backing the target.
    pub connector_id: String,
    /// Connector connection configuration in effect.
    pub connector_config: aster_drive_storage::ConnectorConfigEnvelope,
    /// Desired-state revision the follower knows about.
    pub desired_revision: i64,
    /// Latest revision the follower applied successfully.
    pub applied_revision: i64,
    /// Last apply failure message; empty when healthy.
    pub last_error: String,
    /// When the target was created (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the target last changed (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Connector catalog a remote offers for target creation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteStorageTargetConnectorCatalog {
    /// Connector descriptors describing configurable fields.
    pub descriptors: Vec<aster_drive_storage::StorageConnectorDescriptor>,
    /// Localized display strings for the descriptors.
    pub localizations: aster_drive_storage::StorageConnectorLocalizationCatalog,
}

/// Request to create one storage target on the follower.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteCreateStorageTargetRequest {
    /// Display name for the new target.
    pub name: String,
    /// Connection input (connector id plus connector-specific configuration).
    pub connection: StorageConnectionInput,
}

impl RemoteCreateStorageTargetRequest {
    /// Connector id of the requested connection.
    pub fn connector_id(&self) -> &aster_drive_storage::ConnectorId {
        &self.connection.connector_config.connector_id
    }
}

/// Partial update of a follower-side storage target; `None` fields are left
/// unchanged. Debug output redacts `connection` so secrets never reach logs.
#[derive(Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteUpdateStorageTargetRequest {
    /// New display name, when changing it.
    pub name: Option<String>,
    /// Replacement connection input, when rotating connector or configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<StorageConnectionInput>,
}

impl fmt::Debug for RemoteUpdateStorageTargetRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RemoteUpdateStorageTargetRequest")
            .field("name", &self.name)
            .field(
                "connection",
                &self.connection.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

/// Server-side compose request: concatenate part objects into one object.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteStorageComposeRequest {
    /// Storage target the composed object belongs to.
    pub target_key: String,
    /// Part object keys, concatenated in the given order.
    pub part_keys: Vec<String>,
    /// Caller-declared composed size in bytes; the remote validates it against
    /// ingress limits before writing.
    pub expected_size: i64,
}

/// Result of a successful server-side compose.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteStorageComposeResponse {
    /// Size of the composed object in bytes.
    pub bytes_written: u64,
}

/// Envelope every internal storage JSON response is wrapped in: control-plane
/// callers match on the embedded [`ApiErrorCode`] before trusting `data`.
pub(crate) type ApiEnvelope<T> = aster_forge_api::response::ApiResponse<
    T,
    ApiErrorCode,
    crate::api::response::ApiErrorDiagnostic,
>;

#[cfg(test)]
mod tests {
    use super::{
        RemoteBindingSyncRequest, RemoteMultipartCapabilities, RemoteStorageCapabilities,
        RemoteStorageFeatureFlags, RemoteStorageTargetRuntimeCapabilities,
    };

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

    #[test]
    fn target_runtime_capabilities_round_trip_without_breaking_legacy_payloads() {
        let mut capabilities = RemoteStorageCapabilities::current();
        capabilities.target_runtime_capabilities = vec![RemoteStorageTargetRuntimeCapabilities {
            target_key: "target-s3".to_string(),
            connector_id: "asterdrive.storage.s3".to_string(),
            applied_revision: 4,
            range_read: true,
            stream_upload: true,
            multipart: Some(RemoteMultipartCapabilities {
                min_part_size: 5 * 1024 * 1024,
                max_part_size: Some(5 * 1024 * 1024 * 1024),
                max_parts: 10_000,
                native_reader_upload: true,
                buffered_reader_max_size: None,
            }),
        }];
        let encoded = serde_json::to_vec(&capabilities).expect("capabilities should encode");
        let decoded: RemoteStorageCapabilities =
            serde_json::from_slice(&encoded).expect("capabilities should decode");
        assert_eq!(decoded, capabilities);

        let legacy: RemoteStorageCapabilities = serde_json::from_value(serde_json::json!({
            "protocol_version": "v6",
            "min_supported_protocol_version": "v6"
        }))
        .expect("legacy capabilities should decode");
        assert!(legacy.target_runtime_capabilities.is_empty());
    }
}

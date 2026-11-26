//! 存储策略服务子模块：`models`。

use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use super::placement::{
    PlacementMatcher, PlacementSelectionMode, PlacementUnavailableBehavior,
    StorageAdmissionConstraints, UploadExecutionPreference,
};
use crate::api::api_error_code::ApiErrorCode;
use crate::api::response::ApiErrorDiagnostic;
use aster_drive_model::entities::storage_policy;
use aster_drive_model::types::parse_storage_policy_allowed_types;
use aster_drive_storage::{
    ConnectorConfigEnvelope, ConnectorId, StorageConnectorActionId, StorageConnectorPromotionId,
    StoragePolicyBehaviorConfig, StoragePolicyConfigEnvelope,
};

/// Sanitized, admin-facing explanation of a storage failure.
///
/// Mirrors the API error diagnostic of the originating error so policy
/// endpoints can surface it without leaking provider credentials.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyDiagnostic {
    /// Stable API error code carried by the originating error.
    pub api_code: ApiErrorCode,
    /// Failure classification, such as a storage error kind or `connector_validation`.
    pub kind: String,
    /// Sanitized provider message, safe to display to administrators.
    pub message: String,
    /// Whether retrying the failed operation may succeed.
    pub retryable: bool,
    /// Connector field the failure is attributed to, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// Connector field scope the field belongs to, such as `connector_config`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

impl StoragePolicyDiagnostic {
    /// Projects a storage error into a diagnostic.
    ///
    /// Returns `None` for errors that carry no API diagnostic projection.
    pub fn from_error(error: &crate::errors::AsterError) -> Option<Self> {
        ApiErrorDiagnostic::from_error(error).map(|diagnostic| Self {
            api_code: error.api_error_code(),
            kind: diagnostic.kind,
            message: diagnostic.message,
            retryable: error.api_error_retryable(),
            field: diagnostic.field,
            scope: diagnostic.scope,
        })
    }
}

impl From<StoragePolicyDiagnostic> for ApiErrorDiagnostic {
    fn from(value: StoragePolicyDiagnostic) -> Self {
        Self {
            kind: value.kind,
            message: value.message,
            field: value.field,
            scope: value.scope,
        }
    }
}

/// Compact policy reference embedded in placement target views.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicySummaryInfo {
    /// Storage policy id.
    pub id: i64,
    /// Storage policy display name.
    pub name: String,
    /// Connector id backing the policy.
    pub connector_id: String,
}

/// Admin view of a storage policy group with its placement configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyGroupInfo {
    /// Policy group id.
    pub id: i64,
    /// Group display name.
    pub name: String,
    /// Optional free-form group description.
    pub description: String,
    /// Disabled groups never route new uploads.
    pub is_enabled: bool,
    /// Whether this is the system default group.
    pub is_default: bool,
    /// Creation timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last modification timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Admission constraints evaluated before rule matching.
    pub admission: StorageAdmissionConstraints,
    /// Upload execution preference for uploads routed through this group.
    pub execution_preference: UploadExecutionPreference,
    /// Bumped on every profile mutation; identifies the compiled placement snapshot entry.
    pub routing_revision: i64,
    /// Placement rules with their candidate targets.
    pub rules: Vec<StoragePlacementRuleInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePlacementRuleInfo {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub priority: i32,
    pub is_enabled: bool,
    pub matcher: PlacementMatcher,
    pub selection_mode: PlacementSelectionMode,
    pub unavailable_behavior: PlacementUnavailableBehavior,
    pub targets: Vec<StoragePlacementTargetInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePlacementTargetInfo {
    pub id: i64,
    pub policy_id: i64,
    pub weight: i32,
    pub accepting_new_writes: bool,
    pub stable_order: i32,
    pub policy: StoragePolicySummaryInfo,
}

/// Placement target payload inside a rule of a create or update request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePlacementTargetInput {
    /// Storage policy this target routes to.
    pub policy_id: i64,
    /// Weighted-random selection weight; must be positive.
    pub weight: i32,
    /// When false the target is draining and skipped for new writes.
    #[serde(default = "default_true")]
    pub accepting_new_writes: bool,
    /// Deterministic tiebreak order for first-available selection; non-negative.
    #[serde(default)]
    pub stable_order: i32,
}

/// Placement rule payload for policy group create and update requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePlacementRuleInput {
    /// Rule display name.
    pub name: String,
    /// Optional rule description.
    pub description: Option<String>,
    /// Unique positive evaluation priority within the group.
    pub priority: i32,
    /// Disabled rules are skipped during resolution.
    #[serde(default = "default_true")]
    pub is_enabled: bool,
    /// File predicate deciding whether the rule applies.
    pub matcher: PlacementMatcher,
    /// How one of the eligible targets is picked.
    #[serde(default)]
    pub selection_mode: PlacementSelectionMode,
    /// What happens when the rule has no eligible target.
    #[serde(default)]
    pub unavailable_behavior: PlacementUnavailableBehavior,
    /// Candidate targets; must reference distinct policies.
    pub targets: Vec<StoragePlacementTargetInput>,
}

/// Decoded storage policy as returned by the admin API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicy {
    /// Storage policy id.
    pub id: i64,
    /// Policy display name.
    pub name: String,
    /// Connector id backing this policy.
    pub connector_id: String,
    /// Schema-versioned connector config values.
    pub connector_config: ConnectorConfigEnvelope,
    /// Connector-defined behavior config values.
    pub behavior: StoragePolicyBehaviorConfig,
    /// Per-upload size limit in bytes; `0` means unlimited.
    pub max_file_size: i64,
    /// Extension allowlist decoded from the persisted JSON column; empty allows all types.
    pub allowed_types: Vec<String>,
    /// Whether this is the system default policy.
    pub is_default: bool,
    /// Upload chunk size in bytes.
    pub chunk_size: i64,
    /// Creation timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last modification timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Local blob accounting combined with a live backend capacity probe.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyCapacityInfo {
    /// Storage policy id.
    pub policy_id: i64,
    /// Connector id backing the policy.
    pub connector_id: String,
    /// Number of blobs referencing this policy.
    pub blob_count: i64,
    /// Total blob size in bytes.
    pub blob_total_bytes: i64,
    /// Backend-reported capacity; may be unsupported or unavailable.
    pub capacity: aster_drive_storage::StorageCapacityInfo,
    /// Sanitized probe failure, when backend capacity could not be read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<StoragePolicyDiagnostic>,
}

/// Result of a connector action executed through a storage policy.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyActionResult {
    /// Whether the action completed without a connector error.
    pub ok: bool,
    /// Id of the executed action.
    pub action_id: StorageConnectorActionId,
    /// Connector-defined output payload; omitted when the action returns none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<crate::storage::StorageConnectorActionOutput>,
    /// Sanitized failure info; present only when the action failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<StoragePolicyDiagnostic>,
}

impl From<crate::storage::StorageConnectorActionResult> for StoragePolicyActionResult {
    fn from(value: crate::storage::StorageConnectorActionResult) -> Self {
        Self {
            ok: true,
            action_id: value.action_id,
            output: value.output,
            diagnostic: None,
        }
    }
}

impl TryFrom<storage_policy::Model> for StoragePolicy {
    type Error = crate::errors::AsterError;

    fn try_from(model: storage_policy::Model) -> Result<Self, Self::Error> {
        let storage_config: StoragePolicyConfigEnvelope =
            serde_json::from_str(model.storage_config.as_ref()).map_err(|error| {
                crate::errors::AsterError::database_operation(format!(
                    "storage policy {} has invalid storage_config: {error}",
                    model.id
                ))
            })?;
        if storage_config.connector.connector_id.as_str() != model.connector_id {
            return Err(crate::errors::AsterError::database_operation(format!(
                "storage policy {} connector id does not match storage_config",
                model.id
            )));
        }
        Ok(Self {
            id: model.id,
            name: model.name,
            connector_id: model.connector_id,
            connector_config: ConnectorConfigEnvelope::new(
                storage_config.connector.connector_id,
                storage_config.connector.schema_version,
                serde_json::from_value(storage_config.connector.values).map_err(|error| {
                    crate::errors::AsterError::database_operation(format!(
                        "storage policy {} connector config must be a JSON object: {error}",
                        model.id
                    ))
                })?,
            ),
            behavior: storage_config.behavior.values,
            max_file_size: model.max_file_size,
            allowed_types: parse_storage_policy_allowed_types(model.allowed_types.as_ref()),
            is_default: model.is_default,
            chunk_size: model.chunk_size,
            created_at: model.created_at,
            updated_at: model.updated_at,
        })
    }
}

/// Counts produced by migrating policy group assignments.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PolicyGroupAssignmentMigrationResult {
    /// Group the assignments moved from.
    pub source_group_id: i64,
    /// Group the assignments moved to.
    pub target_group_id: i64,
    /// Number of user assignments moved.
    pub affected_users: u64,
    /// Number of team assignments moved.
    pub affected_teams: u64,
    /// Total assignments moved (`affected_users + affected_teams`).
    pub migrated_assignments: u64,
}

/// Payload for creating a storage policy.
#[derive(Debug, Clone)]
pub struct CreateStoragePolicyInput {
    /// Policy display name.
    pub name: String,
    /// Connector config, optional credential and behavior normalization input.
    pub connection: crate::storage::StoragePolicyConnectionInput,
    /// Per-upload size limit in bytes; must be non-negative, `0` means unlimited.
    pub max_file_size: i64,
    /// Upload chunk size in bytes; defaults to 5 MiB when omitted.
    pub chunk_size: Option<i64>,
    /// Whether the new policy becomes the system default.
    pub is_default: bool,
    /// Extension allowlist; `None` or empty allows all types.
    pub allowed_types: Option<Vec<String>>,
}

/// Partial storage policy patch; `None` fields keep their current value.
#[derive(Debug, Clone, Default)]
pub struct UpdateStoragePolicyInput {
    /// Replacement policy name.
    pub name: Option<String>,
    /// Replacement connector config; the connector id itself cannot change.
    pub connector_config: Option<ConnectorConfigEnvelope>,
    /// Replacement behavior config values.
    pub behavior: Option<StoragePolicyBehaviorConfig>,
    /// Replacement credential; when omitted the saved credential is reconciled automatically.
    pub credential: Option<crate::storage::StorageConnectorCredentialInput>,
    /// Replacement per-upload size limit in bytes; must be non-negative, `0` means unlimited.
    pub max_file_size: Option<i64>,
    /// Replacement upload chunk size in bytes.
    pub chunk_size: Option<i64>,
    /// Request the policy to become the system default.
    pub is_default: Option<bool>,
    /// Replacement extension allowlist; `None` keeps the current list, empty allows all types.
    pub allowed_types: Option<Vec<String>>,
}

/// Request to migrate an existing policy onto another connector.
#[derive(Debug, Clone)]
pub struct PromoteStoragePolicyConnectorInput {
    /// Connector the policy should be migrated to.
    pub target_connector_id: ConnectorId,
    /// Connector-declared promotion plan to execute.
    pub promotion_id: StorageConnectorPromotionId,
}

/// Payload for creating a storage policy group.
#[derive(Debug, Clone)]
pub struct CreateStoragePolicyGroupInput {
    /// Group display name.
    pub name: String,
    /// Optional group description.
    pub description: Option<String>,
    /// Disabled groups never route uploads; a default group must be enabled.
    pub is_enabled: bool,
    /// Whether the new group becomes the system default group.
    pub is_default: bool,
    /// Admission constraints; defaults apply when omitted.
    pub admission: Option<StorageAdmissionConstraints>,
    /// Upload execution preference; defaults apply when omitted.
    pub execution_preference: Option<UploadExecutionPreference>,
    /// Placement rules; required and must not be empty.
    pub rules: Option<Vec<StoragePlacementRuleInput>>,
}

/// Partial policy group patch; `None` fields keep their current value.
#[derive(Debug, Clone, Default)]
pub struct UpdateStoragePolicyGroupInput {
    /// Replacement group name.
    pub name: Option<String>,
    /// Replacement group description.
    pub description: Option<String>,
    /// Replacement enabled flag; the default group cannot be disabled.
    pub is_enabled: Option<bool>,
    /// Replacement default flag; transferring the default takes the assignment lock.
    pub is_default: Option<bool>,
    /// Replacement admission constraints.
    pub admission: Option<StorageAdmissionConstraints>,
    /// Replacement upload execution preference.
    pub execution_preference: Option<UploadExecutionPreference>,
    /// Replacement placement rules; when present they replace all existing rules.
    pub rules: Option<Vec<StoragePlacementRuleInput>>,
}

const fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::{StoragePolicyActionResult, StoragePolicyDiagnostic};
    use crate::api::api_error_code::ApiErrorCode;
    use aster_drive_storage::error::storage_driver_error;
    use aster_drive_storage::{StorageConnectorActionId, StorageErrorKind};

    #[test]
    fn storage_policy_diagnostic_sanitizes_admin_storage_details() {
        let error = crate::errors::AsterError::from(storage_driver_error(
            StorageErrorKind::Permission,
            "Azure Blob failed for https://acct.blob.core.windows.net/file?sig=topsecret AccountKey=supersecret;EndpointSuffix=core.windows.net",
        ));

        let diagnostic =
            StoragePolicyDiagnostic::from_error(&error).expect("storage errors are diagnostic");

        assert_eq!(diagnostic.api_code, ApiErrorCode::StoragePermission);
        assert_eq!(diagnostic.kind, "permission");
        assert!(diagnostic.message.contains("sig=[redacted]"));
        assert!(diagnostic.message.contains("AccountKey=[redacted]"));
        assert!(!diagnostic.message.contains("topsecret"));
        assert!(!diagnostic.message.contains("supersecret"));
        assert!(!diagnostic.retryable);
    }

    #[test]
    fn storage_policy_diagnostic_marks_retryable_storage_errors() {
        let error = crate::errors::AsterError::from(storage_driver_error(
            StorageErrorKind::Transient,
            "provider timed out",
        ));

        let diagnostic =
            StoragePolicyDiagnostic::from_error(&error).expect("storage errors are diagnostic");

        assert_eq!(diagnostic.api_code, ApiErrorCode::StorageTransient);
        assert_eq!(diagnostic.kind, "transient");
        assert_eq!(diagnostic.message, "provider timed out");
        assert!(diagnostic.retryable);
    }

    #[test]
    fn storage_policy_diagnostic_ignores_non_storage_errors() {
        let error = crate::errors::AsterError::validation_error("bad request");

        assert!(StoragePolicyDiagnostic::from_error(&error).is_none());
    }

    #[test]
    fn storage_policy_action_result_preserves_plugin_id_and_omits_empty_output() {
        let empty_payload = StoragePolicyActionResult {
            ok: true,
            action_id: StorageConnectorActionId::declared("plugin.verify_namespace"),
            output: None,
            diagnostic: None,
        };

        let value = serde_json::to_value(empty_payload).expect("serialize empty payload");

        assert_eq!(value["ok"], true);
        assert_eq!(value["action_id"], "plugin.verify_namespace");
        assert!(value.get("output").is_none());
        assert!(value.get("diagnostic").is_none());
    }

    #[test]
    fn storage_policy_action_result_serializes_connector_owned_nested_output() {
        let connector_result = crate::storage::StorageConnectorActionResult::with_output(
            StorageConnectorActionId::declared("plugin.inspect_remote_state"),
            serde_json::json!({
                "request_id": "req-1",
                "summary": {
                    "changed": true,
                    "preserved_items": 2
                },
                "warnings": ["remote value retained"]
            }),
        )
        .expect("connector action output should serialize");
        let result = StoragePolicyActionResult::from(connector_result);

        let value = serde_json::to_value(result).expect("serialize action result");

        assert_eq!(value["ok"], true);
        assert_eq!(value["action_id"], "plugin.inspect_remote_state");
        assert_eq!(value["output"]["request_id"], "req-1");
        assert_eq!(value["output"]["summary"]["changed"], true);
        assert_eq!(value["output"]["summary"]["preserved_items"], 2);
        assert_eq!(
            value["output"]["warnings"],
            serde_json::json!(["remote value retained"])
        );
        assert!(value.get("diagnostic").is_none());
    }
}

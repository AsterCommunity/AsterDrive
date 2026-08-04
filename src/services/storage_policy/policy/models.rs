//! 存储策略服务子模块：`models`。

use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::api::api_error_code::ApiErrorCode;
use crate::api::response::ApiErrorDiagnostic;
use aster_drive_model::entities::storage_policy;
use aster_drive_model::types::parse_storage_policy_allowed_types;
use aster_drive_storage::{
    ConnectorConfigEnvelope, StorageConnectorActionId, StoragePolicyBehaviorConfig,
    StoragePolicyConfigEnvelope,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyDiagnostic {
    pub api_code: ApiErrorCode,
    pub kind: String,
    pub message: String,
    pub retryable: bool,
}

impl StoragePolicyDiagnostic {
    pub fn from_error(error: &crate::errors::AsterError) -> Option<Self> {
        ApiErrorDiagnostic::from_error(error).map(|diagnostic| Self {
            api_code: error.api_error_code(),
            kind: diagnostic.kind,
            message: diagnostic.message,
            retryable: error.api_error_retryable(),
        })
    }
}

impl From<StoragePolicyDiagnostic> for ApiErrorDiagnostic {
    fn from(value: StoragePolicyDiagnostic) -> Self {
        Self {
            kind: value.kind,
            message: value.message,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicySummaryInfo {
    pub id: i64,
    pub name: String,
    pub connector_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyGroupItemInfo {
    pub id: i64,
    pub policy_id: i64,
    pub priority: i32,
    pub min_file_size: i64,
    pub max_file_size: i64,
    pub policy: StoragePolicySummaryInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyGroupInfo {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub is_enabled: bool,
    pub is_default: bool,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub items: Vec<StoragePolicyGroupItemInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyGroupItemInput {
    pub policy_id: i64,
    pub priority: i32,
    pub min_file_size: i64,
    pub max_file_size: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicy {
    pub id: i64,
    pub name: String,
    pub connector_id: String,
    pub connector_config: ConnectorConfigEnvelope,
    pub behavior: StoragePolicyBehaviorConfig,
    pub max_file_size: i64,
    pub allowed_types: Vec<String>,
    pub is_default: bool,
    pub chunk_size: i64,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyCapacityInfo {
    pub policy_id: i64,
    pub connector_id: String,
    pub blob_count: i64,
    pub blob_total_bytes: i64,
    pub capacity: aster_drive_storage::StorageCapacityInfo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<StoragePolicyDiagnostic>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyActionResult {
    pub ok: bool,
    pub action_id: StorageConnectorActionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<crate::storage::StorageConnectorActionOutput>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PolicyGroupAssignmentMigrationResult {
    pub source_group_id: i64,
    pub target_group_id: i64,
    pub affected_users: u64,
    pub affected_teams: u64,
    pub migrated_assignments: u64,
}

#[derive(Debug, Clone)]
pub struct CreateStoragePolicyInput {
    pub name: String,
    pub connection: crate::storage::StorageConnectorConnectionInput,
    pub max_file_size: i64,
    pub chunk_size: Option<i64>,
    pub is_default: bool,
    pub allowed_types: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateStoragePolicyInput {
    pub name: Option<String>,
    pub connector_config: Option<ConnectorConfigEnvelope>,
    pub behavior: Option<StoragePolicyBehaviorConfig>,
    pub credential: Option<crate::storage::StorageConnectorCredentialInput>,
    pub max_file_size: Option<i64>,
    pub chunk_size: Option<i64>,
    pub is_default: Option<bool>,
    pub allowed_types: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct CreateStoragePolicyGroupInput {
    pub name: String,
    pub description: Option<String>,
    pub is_enabled: bool,
    pub is_default: bool,
    pub items: Vec<StoragePolicyGroupItemInput>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateStoragePolicyGroupInput {
    pub name: Option<String>,
    pub description: Option<String>,
    pub is_enabled: Option<bool>,
    pub is_default: Option<bool>,
    pub items: Option<Vec<StoragePolicyGroupItemInput>>,
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

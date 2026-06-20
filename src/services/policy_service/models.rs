//! 存储策略服务子模块：`models`。

use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::api::api_error_code::ApiErrorCode;
use crate::entities::storage_policy;
use crate::errors::sanitize_storage_driver_client_message;
use crate::storage::error::storage_driver_error_display_message;
use crate::types::{
    DriverType, StoragePolicyOptions, parse_storage_policy_allowed_types,
    parse_storage_policy_options,
};

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyDiagnostic {
    pub api_code: ApiErrorCode,
    pub kind: String,
    pub message: String,
    pub retryable: bool,
}

impl StoragePolicyDiagnostic {
    pub fn from_error(error: &crate::errors::AsterError) -> Option<Self> {
        let kind = error.storage_error_kind()?;
        Some(Self {
            api_code: error.api_error_code(),
            kind: kind.as_str().to_string(),
            message: storage_policy_diagnostic_message(error),
            retryable: error.api_error_info().retryable,
        })
    }
}

fn storage_policy_diagnostic_message(error: &crate::errors::AsterError) -> String {
    sanitize_storage_driver_client_message(storage_driver_error_display_message(error.message()))
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyProbeResult {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<StoragePolicyDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicySummaryInfo {
    pub id: i64,
    pub name: String,
    pub driver_type: DriverType,
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
    pub driver_type: DriverType,
    pub endpoint: String,
    pub bucket: String,
    pub base_path: String,
    pub remote_node_id: Option<i64>,
    pub max_file_size: i64,
    pub allowed_types: Vec<String>,
    pub options: StoragePolicyOptions,
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
    pub driver_type: DriverType,
    pub blob_count: i64,
    pub blob_total_bytes: i64,
    pub capacity: crate::storage::StorageCapacityInfo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<StoragePolicyDiagnostic>,
}

pub type StoragePolicyActionType = crate::storage::StoragePolicyExecutableAction;
pub type ExecuteSavedStoragePolicyActionInput =
    crate::storage::ExecuteSavedStorageConnectorActionInput;
pub type ExecuteDraftStoragePolicyActionInput =
    crate::storage::ExecuteDraftStorageConnectorActionInput;
pub type StoragePolicyConnectionInput = crate::storage::StorageConnectorConnectionInput;
pub type TencentCosCorsConfigResult = crate::storage::TencentCosCorsConfigResult;

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyActionResult {
    pub ok: bool,
    pub action: StoragePolicyActionType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tencent_cos_cors: Option<TencentCosCorsConfigResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<StoragePolicyDiagnostic>,
}

impl From<crate::storage::StorageConnectorActionResult> for StoragePolicyActionResult {
    fn from(value: crate::storage::StorageConnectorActionResult) -> Self {
        Self {
            ok: true,
            action: value.action,
            tencent_cos_cors: value.tencent_cos_cors,
            diagnostic: None,
        }
    }
}

impl From<storage_policy::Model> for StoragePolicy {
    fn from(model: storage_policy::Model) -> Self {
        Self {
            id: model.id,
            name: model.name,
            driver_type: model.driver_type,
            endpoint: model.endpoint,
            bucket: model.bucket,
            base_path: model.base_path,
            remote_node_id: model.remote_node_id,
            max_file_size: model.max_file_size,
            allowed_types: parse_storage_policy_allowed_types(model.allowed_types.as_ref()),
            options: parse_storage_policy_options(model.options.as_ref()),
            is_default: model.is_default,
            chunk_size: model.chunk_size,
            created_at: model.created_at,
            updated_at: model.updated_at,
        }
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
pub struct ConfigureTencentCosCorsInput {
    pub connection: StoragePolicyConnectionInput,
}

#[derive(Debug, Clone)]
pub struct CreateStoragePolicyInput {
    pub name: String,
    pub connection: StoragePolicyConnectionInput,
    pub max_file_size: i64,
    pub chunk_size: Option<i64>,
    pub is_default: bool,
    pub allowed_types: Option<Vec<String>>,
    pub options: Option<StoragePolicyOptions>,
    pub application_config: crate::storage::StorageConnectorApplicationConfigInput,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateStoragePolicyInput {
    pub name: Option<String>,
    pub endpoint: Option<String>,
    pub bucket: Option<String>,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
    pub base_path: Option<String>,
    pub remote_node_id: Option<i64>,
    pub max_file_size: Option<i64>,
    pub chunk_size: Option<i64>,
    pub is_default: Option<bool>,
    pub allowed_types: Option<Vec<String>>,
    pub options: Option<StoragePolicyOptions>,
    pub application_config: crate::storage::StorageConnectorApplicationConfigInput,
}

#[derive(Debug, Clone)]
pub struct PromoteS3CompatiblePolicyDriverInput {
    pub target_driver_type: DriverType,
    pub endpoint: String,
    pub bucket: String,
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
    use super::{
        StoragePolicyActionResult, StoragePolicyActionType, StoragePolicyDiagnostic,
        TencentCosCorsConfigResult,
    };
    use crate::api::api_error_code::ApiErrorCode;
    use crate::storage::StorageErrorKind;
    use crate::storage::error::storage_driver_error;

    #[test]
    fn storage_policy_diagnostic_sanitizes_admin_storage_details() {
        let error = storage_driver_error(
            StorageErrorKind::Permission,
            "Azure Blob failed for https://acct.blob.core.windows.net/file?sig=topsecret AccountKey=supersecret;EndpointSuffix=core.windows.net",
        );

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
        let error = storage_driver_error(StorageErrorKind::Transient, "provider timed out");

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
    fn storage_policy_action_type_uses_stable_snake_case_wire_value() {
        let action = StoragePolicyActionType::ConfigureTencentCosCors;

        assert_eq!(action.as_str(), "configure_tencent_cos_cors");
        assert!(action.mutates_remote_state());
        assert_eq!(
            serde_json::to_string(&action).expect("serialize action"),
            "\"configure_tencent_cos_cors\""
        );
        assert_eq!(
            serde_json::from_str::<StoragePolicyActionType>("\"configure_tencent_cos_cors\"")
                .expect("deserialize action"),
            action
        );
    }

    #[test]
    fn storage_policy_action_result_omits_unrelated_payloads() {
        let empty_payload = StoragePolicyActionResult {
            ok: true,
            action: StoragePolicyActionType::ConfigureTencentCosCors,
            tencent_cos_cors: None,
            diagnostic: None,
        };

        let value = serde_json::to_value(empty_payload).expect("serialize empty payload");

        assert_eq!(value["ok"], true);
        assert_eq!(value["action"], "configure_tencent_cos_cors");
        assert!(value.get("tencent_cos_cors").is_none());
        assert!(value.get("diagnostic").is_none());
    }

    #[test]
    fn storage_policy_action_result_serializes_tencent_cos_cors_payload() {
        let result = StoragePolicyActionResult {
            ok: true,
            action: StoragePolicyActionType::ConfigureTencentCosCors,
            tencent_cos_cors: Some(TencentCosCorsConfigResult {
                rule_id: "asterdrive-presigned-access".to_string(),
                allowed_origins: vec![
                    "https://drive.example.com".to_string(),
                    "https://admin.example.com".to_string(),
                ],
                request_id: Some("req-1".to_string()),
                preserved_rule_count: 2,
                replaced_existing_rule: true,
                response_vary: true,
            }),
            diagnostic: None,
        };

        let value = serde_json::to_value(result).expect("serialize COS payload");

        assert_eq!(value["action"], "configure_tencent_cos_cors");
        assert_eq!(
            value["tencent_cos_cors"]["rule_id"],
            "asterdrive-presigned-access"
        );
        assert_eq!(
            value["tencent_cos_cors"]["allowed_origins"],
            serde_json::json!(["https://drive.example.com", "https://admin.example.com"])
        );
        assert_eq!(value["tencent_cos_cors"]["request_id"], "req-1");
        assert_eq!(value["tencent_cos_cors"]["preserved_rule_count"], 2);
        assert_eq!(value["tencent_cos_cors"]["replaced_existing_rule"], true);
        assert_eq!(value["tencent_cos_cors"]["response_vary"], true);
    }

    #[test]
    fn storage_policy_action_result_serializes_diagnostic_failure_payload() {
        let diagnostic = StoragePolicyDiagnostic {
            api_code: ApiErrorCode::StorageMisconfigured,
            kind: "misconfigured".to_string(),
            message: "connection test failed: local base path is not a directory".to_string(),
            retryable: false,
        };
        let result = StoragePolicyActionResult {
            ok: false,
            action: StoragePolicyActionType::ConfigureTencentCosCors,
            tencent_cos_cors: None,
            diagnostic: Some(diagnostic),
        };

        let value = serde_json::to_value(result).expect("serialize diagnostic payload");

        assert_eq!(value["ok"], false);
        assert_eq!(value["action"], "configure_tencent_cos_cors");
        assert_eq!(value["diagnostic"]["api_code"], "storage.misconfigured");
        assert_eq!(value["diagnostic"]["kind"], "misconfigured");
        assert_eq!(
            value["diagnostic"]["message"],
            "connection test failed: local base path is not a directory"
        );
        assert_eq!(value["diagnostic"]["retryable"], false);
        assert!(value.get("tencent_cos_cors").is_none());
    }
}

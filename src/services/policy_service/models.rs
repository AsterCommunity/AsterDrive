//! 存储策略服务子模块：`models`。

use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::entities::storage_policy;
use crate::types::{
    DriverType, StoragePolicyOptions, parse_storage_policy_allowed_types,
    parse_storage_policy_options,
};

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
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TencentCosCorsConfigResult {
    pub rule_id: String,
    pub allowed_origins: Vec<String>,
    pub request_id: Option<String>,
    pub preserved_rule_count: usize,
    pub replaced_existing_rule: bool,
    pub response_vary: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StoragePolicyActionType {
    ConfigureTencentCosCors,
}

impl StoragePolicyActionType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ConfigureTencentCosCors => "configure_tencent_cos_cors",
        }
    }

    pub const fn mutates_remote_state(self) -> bool {
        match self {
            Self::ConfigureTencentCosCors => true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExecuteSavedStoragePolicyActionInput {
    pub action: StoragePolicyActionType,
}

#[derive(Debug, Clone)]
pub struct ExecuteDraftStoragePolicyActionInput {
    pub action: StoragePolicyActionType,
    pub policy_id: Option<i64>,
    pub connection: StoragePolicyConnectionInput,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyActionResult {
    pub action: StoragePolicyActionType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tencent_cos_cors: Option<TencentCosCorsConfigResult>,
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
pub struct StoragePolicyConnectionInput {
    pub driver_type: DriverType,
    pub endpoint: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub base_path: String,
    pub remote_node_id: Option<i64>,
    pub options: StoragePolicyOptions,
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
    use super::{StoragePolicyActionResult, StoragePolicyActionType, TencentCosCorsConfigResult};

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
            action: StoragePolicyActionType::ConfigureTencentCosCors,
            tencent_cos_cors: None,
        };

        let value = serde_json::to_value(empty_payload).expect("serialize empty payload");

        assert_eq!(value["action"], "configure_tencent_cos_cors");
        assert!(value.get("tencent_cos_cors").is_none());
    }

    #[test]
    fn storage_policy_action_result_serializes_tencent_cos_cors_payload() {
        let result = StoragePolicyActionResult {
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
}

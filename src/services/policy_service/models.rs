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

impl From<storage_policy::Model> for StoragePolicy {
    fn from(model: storage_policy::Model) -> Self {
        let mut options = parse_storage_policy_options(model.options.as_ref());
        options.google_drive_refresh_token = options
            .google_drive_refresh_token
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
            .then(|| "__configured__".to_string());

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
            options,
            is_default: model.is_default,
            chunk_size: model.chunk_size,
            created_at: model.created_at,
            updated_at: model.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PolicyGroupUserMigrationResult {
    pub source_group_id: i64,
    pub target_group_id: i64,
    pub affected_users: u64,
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

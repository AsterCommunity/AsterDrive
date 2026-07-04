//! SeaORM 实体定义：`storage_policy`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::{DriverType, StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions};

#[derive(Clone, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), schema(as = StoragePolicy))]
#[sea_orm(table_name = "storage_policies")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub name: String,
    pub driver_type: DriverType,
    pub endpoint: String,
    pub bucket: String,
    #[serde(skip_serializing)]
    pub access_key: String,
    #[serde(skip_serializing)]
    pub secret_key: String,
    pub base_path: String,
    pub remote_node_id: Option<i64>,
    pub remote_storage_target_key: Option<String>,
    pub max_file_size: i64, // 0 = unlimited
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub allowed_types: StoredStoragePolicyAllowedTypes, // JSON array
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub options: StoredStoragePolicyOptions, // JSON object
    pub is_default: bool,
    pub chunk_size: i64, // 0 = single upload, >0 = chunk size in bytes
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTimeUtc,
}

impl fmt::Debug for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Model")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("driver_type", &self.driver_type)
            .field("endpoint", &self.endpoint)
            .field("bucket", &self.bucket)
            .field("access_key", &"***REDACTED***")
            .field("secret_key", &"***REDACTED***")
            .field("base_path", &self.base_path)
            .field("remote_node_id", &self.remote_node_id)
            .field("remote_storage_target_key", &self.remote_storage_target_key)
            .field("max_file_size", &self.max_file_size)
            .field("allowed_types", &self.allowed_types)
            .field("options", &self.options)
            .field("is_default", &self.is_default)
            .field("chunk_size", &self.chunk_size)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::storage_policy_authorization_flow::Entity")]
    StoragePolicyAuthorizationFlows,
    #[sea_orm(has_many = "super::storage_policy_credential::Entity")]
    StoragePolicyCredentials,
    #[sea_orm(has_many = "super::storage_policy_group_item::Entity")]
    StoragePolicyGroupItems,
    #[sea_orm(has_many = "super::file_blob::Entity")]
    FileBlobs,
    #[sea_orm(has_many = "super::folder::Entity")]
    Folders,
    #[sea_orm(
        belongs_to = "super::managed_follower::Entity",
        from = "Column::RemoteNodeId",
        to = "super::managed_follower::Column::Id"
    )]
    ManagedFollower,
}

impl Related<super::storage_policy_authorization_flow::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::StoragePolicyAuthorizationFlows.def()
    }
}

impl Related<super::storage_policy_credential::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::StoragePolicyCredentials.def()
    }
}

impl Related<super::storage_policy_group_item::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::StoragePolicyGroupItems.def()
    }
}

impl Related<super::file_blob::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::FileBlobs.def()
    }
}

impl Related<super::folder::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Folders.def()
    }
}

impl Related<super::managed_follower::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ManagedFollower.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_storage_policy_credentials() {
        let now = chrono::Utc::now();
        let model = Model {
            id: 1,
            name: "storage".to_string(),
            driver_type: DriverType::S3,
            endpoint: "https://s3.example.test".to_string(),
            bucket: "bucket".to_string(),
            access_key: "plain-access-key".to_string(),
            secret_key: "plain-secret-key".to_string(),
            base_path: "base".to_string(),
            remote_node_id: None,
            remote_storage_target_key: None,
            max_file_size: 0,
            allowed_types: StoredStoragePolicyAllowedTypes::from("[]".to_string()),
            options: StoredStoragePolicyOptions::from("{}".to_string()),
            is_default: false,
            chunk_size: 0,
            created_at: now,
            updated_at: now,
        };

        let debug = format!("{model:?}");
        assert!(debug.contains(r#"access_key: "***REDACTED***""#));
        assert!(debug.contains(r#"secret_key: "***REDACTED***""#));
        assert!(!debug.contains("plain-access-key"));
        assert!(!debug.contains("plain-secret-key"));
    }
}

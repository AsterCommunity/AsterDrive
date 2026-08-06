//! SeaORM 实体定义：`storage_policy`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::{StoredStoragePolicyAllowedTypes, StoredStoragePolicyConfig};

#[derive(Clone, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), schema(as = StoragePolicy))]
#[sea_orm(table_name = "storage_policies")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub name: String,
    pub connector_id: String,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub storage_config: StoredStoragePolicyConfig,
    pub max_file_size: i64, // 0 = unlimited
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub allowed_types: StoredStoragePolicyAllowedTypes, // JSON array
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
            .field("connector_id", &self.connector_id)
            .field("storage_config", &self.storage_config)
            .field("max_file_size", &self.max_file_size)
            .field("allowed_types", &self.allowed_types)
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
    #[sea_orm(has_one = "super::storage_policy_connector_credential::Entity")]
    StoragePolicyConnectorCredential,
    #[sea_orm(has_many = "super::storage_policy_group_item::Entity")]
    StoragePolicyGroupItems,
    #[sea_orm(has_many = "super::file_blob::Entity")]
    FileBlobs,
    #[sea_orm(has_many = "super::folder::Entity")]
    Folders,
}

impl Related<super::storage_policy_authorization_flow::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::StoragePolicyAuthorizationFlows.def()
    }
}

impl Related<super::storage_policy_connector_credential::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::StoragePolicyConnectorCredential.def()
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

impl ActiveModelBehavior for ActiveModel {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_includes_connector_identity_without_legacy_config_fields() {
        let now = chrono::Utc::now();
        let model = Model {
            id: 1,
            name: "storage".to_string(),
            connector_id: "asterdrive.storage.s3".to_string(),
            storage_config: StoredStoragePolicyConfig::from(
                r#"{"format_version":1,"connector":{"format_version":1,"connector_id":"asterdrive.storage.s3","schema_version":1,"values":{}},"behavior":{"format_version":1,"schema_version":1,"values":{}}}"#
                    .to_string(),
            ),
            max_file_size: 0,
            allowed_types: StoredStoragePolicyAllowedTypes::from("[]".to_string()),
            is_default: false,
            chunk_size: 0,
            created_at: now,
            updated_at: now,
        };

        let debug = format!("{model:?}");
        assert!(debug.contains(r#"connector_id: "asterdrive.storage.s3""#));
        assert!(!debug.contains("driver_type"));
        assert!(!debug.contains("access_key"));
        assert!(!debug.contains("options"));
    }
}

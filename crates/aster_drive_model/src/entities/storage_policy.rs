//! SeaORM 实体定义：`storage_policy`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::{StoredStoragePolicyAllowedTypes, StoredStoragePolicyConfig};

/// Storage policy row: a named connector binding plus its storage and upload
/// behavior configuration. Policies are routed to users/teams through policy
/// groups and placement rules.
#[derive(Clone, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), schema(as = StoragePolicy))]
#[sea_orm(table_name = "storage_policies")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Human-readable policy name shown in admin UIs.
    pub name: String,
    /// Registered storage connector id owning this policy's data plane, e.g.
    /// `asterdrive.storage.s3`; selects the driver implementation.
    pub connector_id: String,
    /// Opaque versioned connector/behavior configuration (JSON); parsed only
    /// by the connector registered under `connector_id`.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub storage_config: StoredStoragePolicyConfig,
    /// Maximum size of a single stored file in bytes; `0` = unlimited.
    pub max_file_size: i64,
    /// Legacy file-type allowlist serialized as a JSON array (`[]` = no
    /// entries); kept for compatibility, admission is now governed by the
    /// placement profile's admission config.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub allowed_types: StoredStoragePolicyAllowedTypes,
    /// Whether this policy is the instance-wide default storage policy.
    pub is_default: bool,
    /// Chunked-upload chunk size in bytes; `0` = single-shot upload, `>0` =
    /// upload in chunks of this many bytes.
    pub chunk_size: i64,
    /// Row creation timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    /// Row last-update timestamp (UTC).
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

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// Authorization flows started against this policy.
    #[sea_orm(has_many = "super::storage_policy_authorization_flow::Entity")]
    StoragePolicyAuthorizationFlows,
    /// The connector-owned encrypted credential attached to this policy.
    #[sea_orm(has_one = "super::storage_policy_connector_credential::Entity")]
    StoragePolicyConnectorCredential,
    /// File blobs currently stored under this policy.
    #[sea_orm(has_many = "super::file_blob::Entity")]
    FileBlobs,
    /// Folders routed to this policy.
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

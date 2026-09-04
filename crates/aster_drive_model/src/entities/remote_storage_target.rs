//! SeaORM 实体定义：`remote_storage_target`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

#[derive(Clone, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(table_name = "remote_storage_targets")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub master_binding_id: i64,
    pub target_key: String,
    pub name: String,
    pub connector_id: Option<String>,
    pub connector_config: Option<String>,
    /// TODO(remote-storage-target-0.7.0): remove the flattened compatibility
    /// columns after the supported 0.5.0 upgrade window. Runtime code never
    /// reads them; new rows write empty values only for the old NOT NULL schema.
    pub driver_type: String,
    pub endpoint: String,
    pub bucket: String,
    #[serde(skip_serializing)]
    pub access_key: String,
    #[serde(skip_serializing)]
    pub secret_key: String,
    pub base_path: String,
    pub is_default: bool,
    pub desired_revision: i64,
    pub applied_revision: i64,
    pub last_error: String,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTimeUtc,
}

impl fmt::Debug for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Model")
            .field("id", &self.id)
            .field("master_binding_id", &self.master_binding_id)
            .field("target_key", &self.target_key)
            .field("name", &self.name)
            .field("connector_id", &self.connector_id)
            .field("connector_config", &self.connector_config)
            .field("is_default", &self.is_default)
            .field("desired_revision", &self.desired_revision)
            .field("applied_revision", &self.applied_revision)
            .field("last_error", &self.last_error)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::master_binding::Entity",
        from = "Column::MasterBindingId",
        to = "super::master_binding::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    MasterBinding,
}

impl Related<super::master_binding::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::MasterBinding.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_uses_only_connector_owned_storage_fields() {
        let now = chrono::Utc::now();
        let model = Model {
            id: 1,
            master_binding_id: 2,
            target_key: "profile".to_string(),
            name: "ingress".to_string(),
            connector_id: Some("asterdrive.storage.s3".to_string()),
            connector_config: Some(r#"{"format_version":1}"#.to_string()),
            driver_type: String::new(),
            endpoint: String::new(),
            bucket: String::new(),
            access_key: String::new(),
            secret_key: String::new(),
            base_path: String::new(),
            is_default: false,
            desired_revision: 1,
            applied_revision: 1,
            last_error: String::new(),
            created_at: now,
            updated_at: now,
        };

        let debug = format!("{model:?}");
        assert!(debug.contains(r#"connector_id: Some("asterdrive.storage.s3")"#));
        assert!(!debug.contains("driver_type"));
        assert!(!debug.contains("access_key"));
    }
}

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

    /// Verify debug uses only connector owned storage fields.
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

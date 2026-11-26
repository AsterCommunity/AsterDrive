//! SeaORM 实体定义：`remote_storage_target`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Connector-owned storage target declared by a follower for one master
/// binding: the connector and its configuration under a stable `target_key`,
/// plus the binding-control revision counters and last apply error.
#[derive(Clone, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(table_name = "remote_storage_targets")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Master binding this target belongs to.
    pub master_binding_id: i64,
    /// Stable logical name the master uses to address this target in requests;
    /// unique per binding.
    pub target_key: String,
    /// Administrative display name shown in admin UIs.
    pub name: String,
    /// Registered storage connector id owning this target's data plane, e.g.
    /// `asterdrive.storage.s3`; `None` while the target is declared but not
    /// yet configured.
    pub connector_id: Option<String>,
    /// Opaque versioned connector configuration (JSON); parsed only by the
    /// connector registered under `connector_id`. `None` until configured.
    pub connector_config: Option<String>,
    /// Latest target configuration revision published by the master; bumped on
    /// every master-side change.
    pub desired_revision: i64,
    /// Latest configuration revision this follower has applied locally.
    pub applied_revision: i64,
    /// Error from the latest failed apply or probe attempt; empty when healthy.
    pub last_error: String,
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

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// Master binding owning this target (FK `master_binding_id`, cascade on
    /// delete).
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

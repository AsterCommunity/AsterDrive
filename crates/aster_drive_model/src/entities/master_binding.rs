//! SeaORM 实体定义：`master_binding`。

use crate::types::ResolvedRemoteTransport;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Binding row created on a follower node when it enrolls against a master
/// primary: holds the master's control-plane credentials, the local storage
/// namespace all provider object keys must live under, and the binding-control
/// revision counters between the two sides.
#[derive(Clone, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(table_name = "master_bindings")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Administrative display name of the master connection.
    pub name: String,
    /// Control-plane base URL of the master this follower enrolls against.
    pub master_url: String,
    /// Credential used to authenticate control-plane requests to the master.
    pub access_key: String,
    /// Secret paired with `access_key`; never serialize or log this value.
    #[serde(skip_serializing)]
    pub secret_key: String,
    /// Provider object-key prefix allocated for this binding (`mb_<token>`);
    /// all content served for the master is stored under it. Unique.
    pub storage_namespace: String,
    /// Whether control-plane requests and storage operations from this master
    /// are accepted.
    pub is_enabled: bool,
    /// Transport actually used to reach the master after configuration
    /// resolution.
    pub resolved_transport: ResolvedRemoteTransport,
    /// Latest binding-control revision published by the master; bumped on
    /// every master-side configuration change.
    pub desired_revision: i64,
    /// Latest binding-control revision this follower has applied locally.
    pub applied_revision: i64,
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
            .field("master_url", &self.master_url)
            .field("access_key", &"***REDACTED***")
            .field("secret_key", &"***REDACTED***")
            .field("storage_namespace", &self.storage_namespace)
            .field("is_enabled", &self.is_enabled)
            .field("resolved_transport", &self.resolved_transport)
            .field("desired_revision", &self.desired_revision)
            .field("applied_revision", &self.applied_revision)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// Remote storage targets configured under this binding.
    #[sea_orm(has_many = "super::remote_storage_target::Entity")]
    RemoteStorageTargets,
}

impl Related<super::remote_storage_target::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::RemoteStorageTargets.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_master_binding_credentials() {
        let now = chrono::Utc::now();
        let model = Model {
            id: 1,
            name: "master".to_string(),
            master_url: "https://master.example.test".to_string(),
            access_key: "plain-access-key".to_string(),
            secret_key: "plain-secret-key".to_string(),
            storage_namespace: "namespace".to_string(),
            is_enabled: true,
            resolved_transport: ResolvedRemoteTransport::ReverseTunnel,
            desired_revision: 1,
            applied_revision: 1,
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

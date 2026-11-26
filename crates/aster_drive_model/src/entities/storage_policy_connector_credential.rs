//! Encrypted connector-owned credential state for storage policies.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Connector-owned encrypted credential state bound to a storage policy.
/// The plaintext never leaves the connector; core only stores ciphertext.
#[derive(Clone, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "storage_policy_connector_credentials")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Owning storage policy (unique per policy).
    pub policy_id: i64,
    /// Connector id whose driver owns and interprets the ciphertext; must
    /// match the policy's connector.
    pub connector_id: String,
    /// Credential payload schema version understood by the owning connector.
    pub schema_version: i32,
    /// Monotonic compare-and-swap revision used by refreshable credentials.
    pub revision: i64,
    /// Connector-encrypted credential material; never serialized to API
    /// responses and redacted in debug output.
    #[serde(skip_serializing)]
    pub ciphertext: String,
    /// Row creation timestamp (UTC).
    pub created_at: DateTimeUtc,
    /// Row last-update timestamp (UTC).
    pub updated_at: DateTimeUtc,
}

impl fmt::Debug for Model {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Model")
            .field("id", &self.id)
            .field("policy_id", &self.policy_id)
            .field("connector_id", &self.connector_id)
            .field("schema_version", &self.schema_version)
            .field("revision", &self.revision)
            .field("ciphertext", &"***REDACTED***")
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// The storage policy this credential belongs to (FK `policy_id`).
    #[sea_orm(
        belongs_to = "super::storage_policy::Entity",
        from = "Column::PolicyId",
        to = "super::storage_policy::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    StoragePolicy,
}

impl Related<super::storage_policy::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::StoragePolicy.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_connector_credential_ciphertext() {
        let now = chrono::Utc::now();
        let model = Model {
            id: 1,
            policy_id: 2,
            connector_id: "asterdrive.storage.s3".to_string(),
            schema_version: 1,
            revision: 3,
            ciphertext: "encrypted-secret".to_string(),
            created_at: now,
            updated_at: now,
        };

        let debug = format!("{model:?}");
        assert!(debug.contains(r#"ciphertext: "***REDACTED***""#));
        assert!(!debug.contains("encrypted-secret"));
    }
}

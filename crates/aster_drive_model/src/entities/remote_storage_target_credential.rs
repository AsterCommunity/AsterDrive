//! Encrypted connector credential for one follower-side storage target.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "remote_storage_target_credentials")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique)]
    pub target_id: i64,
    pub connector_id: String,
    pub schema_version: i32,
    pub revision: i64,
    #[serde(skip_serializing)]
    pub ciphertext: String,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

impl fmt::Debug for Model {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Model")
            .field("id", &self.id)
            .field("target_id", &self.target_id)
            .field("connector_id", &self.connector_id)
            .field("schema_version", &self.schema_version)
            .field("revision", &self.revision)
            .field("ciphertext", &"***REDACTED***")
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::remote_storage_target::Entity",
        from = "Column::TargetId",
        to = "super::remote_storage_target::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    RemoteStorageTarget,
}

impl Related<super::remote_storage_target::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::RemoteStorageTarget.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_remote_target_credential_ciphertext() {
        let now = chrono::Utc::now();
        let model = Model {
            id: 1,
            target_id: 2,
            connector_id: "asterdrive.remote-target.s3".to_string(),
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

//! Encrypted connector credentials for follower remote storage targets.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Encrypted connector credential attached to one follower remote storage
/// target. A target holds at most one credential row; each upsert bumps the
/// revision counter so consumers can detect rotation.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "remote_storage_target_credentials")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Storage target these credentials unlock; unique.
    pub target_id: i64,
    /// Connector id the encrypted payload is addressed to; the payload can
    /// only be decrypted/parsed by the matching connector.
    pub connector_id: String,
    /// Version of the connector's credential payload schema.
    pub schema_version: i32,
    /// Rotation counter incremented on every credential upsert; starts at `1`.
    pub revision: i64,
    /// Connector-encrypted credential payload; never serialize or log this
    /// value.
    #[serde(skip_serializing)]
    pub ciphertext: String,
    /// Row creation timestamp (UTC).
    pub created_at: DateTimeUtc,
    /// Last credential upsert timestamp (UTC).
    pub updated_at: DateTimeUtc,
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// The storage target these credentials unlock (FK `target_id`, cascade on
    /// delete).
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

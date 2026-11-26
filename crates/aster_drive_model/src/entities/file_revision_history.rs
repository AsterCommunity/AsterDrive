//! Canonical revision history owned by one file identity.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Revision ledger for one file identity: a monotonically numbered revision
/// sequence plus the DeltaV control state. Exactly one active history exists
/// per live file; deleting the file retires the history instead of dropping it.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "file_revision_histories")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Public hyphenated UUID (36 chars) addressing this history from WebDAV; unique.
    pub public_id: String,
    /// Owning file; unique. `None` once the history is retired and detached
    /// from its file row.
    pub file_id: Option<i64>,
    /// Revision currently pointed at by the file content; `None` only before
    /// the first revision is appended or after retirement.
    pub current_revision_id: Option<i64>,
    /// Sequence number the next appended revision will receive; starts at `2`
    /// because the initial revision consumes `1`. Incremented after each append.
    pub next_sequence: i64,
    /// When DeltaV version control was enabled for this file; `None` keeps the
    /// history in plain-revision mode.
    pub deltav_controlled_at: Option<DateTimeUtc>,
    /// Revision that was current when DeltaV control was enabled; `None` while
    /// DeltaV control is off.
    pub deltav_root_revision_id: Option<i64>,
    /// Row creation timestamp (UTC).
    pub created_at: DateTimeUtc,
    /// When the owning file was deleted and this history was detached
    /// (`file_id` and `current_revision_id` reset to `NULL`).
    pub retired_at: Option<DateTimeUtc>,
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// The file owning this history (FK `file_id`, delete restricted).
    #[sea_orm(
        belongs_to = "super::file::Entity",
        from = "Column::FileId",
        to = "super::file::Column::Id"
    )]
    File,
    /// All revisions recorded in this ledger.
    #[sea_orm(has_many = "super::file_revision::Entity")]
    Revisions,
}

impl Related<super::file::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::File.def()
    }
}

impl Related<super::file_revision::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Revisions.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

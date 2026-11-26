//! Immutable content and metadata snapshot in a file revision history.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Immutable content and metadata snapshot in a file revision history.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "file_revisions")]
pub struct Model {
    /// Auto-incrementing surrogate key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Public UUID handed out in API responses; stable external identity of the revision.
    pub public_id: String,
    /// Revision chain the snapshot belongs to; references `file_revision_histories.id`.
    pub history_id: i64,
    /// 1-based position in the history chain, assigned monotonically per history.
    pub sequence: i64,
    /// Revision this one directly succeeds; `None` for the first revision in a history.
    pub predecessor_revision_id: Option<i64>,
    /// Content blob backing the snapshot; references `file_blobs.id`. `None` after the
    /// content was purged while the ledger entry is retained.
    pub blob_id: Option<i64>,
    /// Logical content size in bytes, kept even after the blob is purged.
    pub logical_size: i64,
    /// MIME type of the snapshot content; `None` when not recorded.
    pub mime_type: Option<String>,
    /// Entity tag exposed for conditional requests (If-Match/If-None-Match).
    pub etag: String,
    /// Lowercase hex SHA-256 of the snapshot content; `None` when not recorded or the
    /// blob was purged.
    pub content_sha256: Option<String>,
    /// Account that produced the snapshot; references `users.id`. `None` when the creator
    /// was deleted or the revision predates attribution.
    pub creator_user_id: Option<i64>,
    /// Creator display name snapshotted at creation so history survives account deletion.
    pub creator_display_name: Option<String>,
    /// Optional user-supplied note attached to the snapshot.
    pub comment: Option<String>,
    /// Transition that produced the snapshot: `create`, `overwrite`, `restore`, `copy`,
    /// or `property_change`.
    pub reason: String,
    /// UTC instant the snapshot was recorded.
    pub created_at: DateTimeUtc,
    /// UTC instant the revision was removed from the visible ledger — tombstoned, or
    /// its whole history retired alongside the file; `None` while it remains live history.
    pub retired_at: Option<DateTimeUtc>,
    /// UTC instant the snapshot's blob was physically purged (`blob_id` cleared at the
    /// same time); `None` while the content still exists.
    pub purged_at: Option<DateTimeUtc>,
}

/// Declared database relations for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// Revision chain the snapshot belongs to, matched on `history_id` to
    /// `file_revision_histories.id`.
    #[sea_orm(
        belongs_to = "super::file_revision_history::Entity",
        from = "Column::HistoryId",
        to = "super::file_revision_history::Column::Id"
    )]
    History,
    /// Content blob backing the snapshot, matched on `blob_id` to `file_blobs.id`.
    #[sea_orm(
        belongs_to = "super::file_blob::Entity",
        from = "Column::BlobId",
        to = "super::file_blob::Column::Id"
    )]
    FileBlob,
    /// Account that produced the snapshot, matched on `creator_user_id` to `users.id`.
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::CreatorUserId",
        to = "super::user::Column::Id"
    )]
    Creator,
    /// Namespaced properties attached to this revision snapshot.
    #[sea_orm(has_many = "super::file_revision_property::Entity")]
    Properties,
}

impl Related<super::file_revision_history::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::History.def()
    }
}

impl Related<super::file_blob::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::FileBlob.def()
    }
}

impl Related<super::file_revision_property::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Properties.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

//! SeaORM 实体定义：`blob_media_metadata`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::{MediaMetadataKind, MediaMetadataStatus, StoredMediaMetadataPayload};

/// Per-blob extracted media metadata cache; one row per blob, keyed by content hash.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(table_name = "blob_media_metadata")]
pub struct Model {
    /// Auto-incrementing surrogate key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Blob described by this row; references `file_blobs.id`. Unique: metadata is a
    /// per-blob cache, so a blob carries at most one row.
    #[sea_orm(unique)]
    pub blob_id: i64,
    /// Content hash of the blob, denormalized from `file_blobs.hash` for integrity checks.
    pub blob_hash: String,
    /// Media family the parser ran for.
    pub kind: MediaMetadataKind,
    /// Extraction outcome.
    pub status: MediaMetadataStatus,
    /// Raw JSON extracted metadata; `None` unless `status` is `ready`.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub metadata_json: Option<StoredMediaMetadataPayload>,
    /// Failure reason when extraction failed or the format is unsupported; otherwise `None`.
    pub error_message: Option<String>,
    /// Identifier of the parser implementation that produced (or attempted) this row.
    pub parser: String,
    /// Version tag of the parser; changed versions invalidate cached rows for re-extraction.
    pub parser_version: String,
    /// UTC instant the row was created.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    /// UTC instant of the latest extraction attempt.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTimeUtc,
}

/// Declared database relations for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// The backing blob, matched on `blob_id` to `file_blobs.id`.
    #[sea_orm(
        belongs_to = "super::file_blob::Entity",
        from = "Column::BlobId",
        to = "super::file_blob::Column::Id"
    )]
    FileBlob,
}

impl Related<super::file_blob::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::FileBlob.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

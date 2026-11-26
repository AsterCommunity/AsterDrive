//! Blob-level media metadata repository.

use chrono::{DateTime, Utc};
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set, sea_query::OnConflict,
};

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::blob_media_metadata::{self, Entity as BlobMediaMetadata};
use aster_drive_model::types::{
    MediaMetadataKind, MediaMetadataStatus, StoredMediaMetadataPayload,
};

/// Loads the media metadata record for a blob. Returns `None` when the blob
/// has no metadata row yet.
pub async fn find_by_blob_id(
    db: &DatabaseConnection,
    blob_id: i64,
) -> Result<Option<blob_media_metadata::Model>> {
    BlobMediaMetadata::find()
        .filter(blob_media_metadata::Column::BlobId.eq(blob_id))
        .one(db)
        .await
        .map_err(AsterError::from)
}

/// Full payload for one blob media metadata row, used by the upsert.
pub struct MediaMetadataRecordInput<'a> {
    /// Blob the record is keyed on; one record per blob.
    pub blob_id: i64,
    /// Content hash of the blob, refreshed on every upsert.
    pub blob_hash: &'a str,
    /// Metadata category produced by the parse attempt.
    pub kind: MediaMetadataKind,
    /// Parse outcome (success, unsupported, failure and so on).
    pub status: MediaMetadataStatus,
    /// Parsed payload; `None` for records without extractable metadata.
    pub metadata_json: Option<&'a StoredMediaMetadataPayload>,
    /// Human-readable failure detail; `None` on success.
    pub error_message: Option<&'a str>,
    /// Name of the parser implementation that produced the record.
    pub parser: &'a str,
    /// Version of the parser implementation, for cache invalidation.
    pub parser_version: &'a str,
    /// Timestamp written to both `created_at` (on insert) and `updated_at`.
    pub now: DateTime<Utc>,
}

/// Inserts the blob's metadata record, or replaces all of its fields (except
/// `created_at`) when a record already exists for the blob. Returns the reloaded
/// row; errors if it cannot be reloaded afterwards.
pub async fn upsert_record(
    db: &DatabaseConnection,
    input: MediaMetadataRecordInput<'_>,
) -> Result<blob_media_metadata::Model> {
    BlobMediaMetadata::insert(blob_media_metadata::ActiveModel {
        blob_id: Set(input.blob_id),
        blob_hash: Set(input.blob_hash.to_string()),
        kind: Set(input.kind),
        status: Set(input.status),
        metadata_json: Set(input.metadata_json.cloned()),
        error_message: Set(input.error_message.map(str::to_string)),
        parser: Set(input.parser.to_string()),
        parser_version: Set(input.parser_version.to_string()),
        created_at: Set(input.now),
        updated_at: Set(input.now),
        ..Default::default()
    })
    .on_conflict(
        OnConflict::column(blob_media_metadata::Column::BlobId)
            .update_columns([
                blob_media_metadata::Column::BlobHash,
                blob_media_metadata::Column::Kind,
                blob_media_metadata::Column::Status,
                blob_media_metadata::Column::MetadataJson,
                blob_media_metadata::Column::ErrorMessage,
                blob_media_metadata::Column::Parser,
                blob_media_metadata::Column::ParserVersion,
                blob_media_metadata::Column::UpdatedAt,
            ])
            .to_owned(),
    )
    .exec(db)
    .await
    .map_err(AsterError::from)?;

    find_by_blob_id(db, input.blob_id).await?.ok_or_else(|| {
        AsterError::record_not_found(format!("media metadata for blob #{}", input.blob_id))
    })
}

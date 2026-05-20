//! Blob-level media metadata repository.

use chrono::{DateTime, Utc};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set, sea_query::OnConflict};

use crate::entities::blob_media_metadata::{self, Entity as BlobMediaMetadata};
use crate::errors::{AsterError, Result};
use crate::types::{MediaMetadataKind, MediaMetadataStatus, StoredMediaMetadataPayload};

pub async fn find_by_blob_id<C: ConnectionTrait>(
    db: &C,
    blob_id: i64,
) -> Result<Option<blob_media_metadata::Model>> {
    BlobMediaMetadata::find()
        .filter(blob_media_metadata::Column::BlobId.eq(blob_id))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub struct MediaMetadataRecordInput<'a> {
    pub blob_id: i64,
    pub blob_hash: &'a str,
    pub kind: MediaMetadataKind,
    pub status: MediaMetadataStatus,
    pub metadata_json: Option<&'a StoredMediaMetadataPayload>,
    pub error_message: Option<&'a str>,
    pub parser: &'a str,
    pub parser_version: &'a str,
    pub now: DateTime<Utc>,
}

pub async fn upsert_record<C: ConnectionTrait>(
    db: &C,
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

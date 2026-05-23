use crate::api::subcode::ApiSubcode;
use crate::db::repository::property_repo;
use crate::entities::{file, file_blob};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::PrimaryAppState;
use crate::types::EntityType;

use super::model::{ArchiveRawManifest, CachedArchiveRawManifest, CachedArchiveRawManifestRef};
use super::{
    ArchivePreviewLimits, CACHE_NAMESPACE, ENTITY_PROPERTY_VALUE_MAX_BYTES, FORMAT_ZIP,
    RAW_CACHE_SCHEMA_VERSION, ZIP_RAW_MANIFEST_CACHE_NAME, archive_preview_validation_error,
};

pub(super) async fn load_cached_raw_manifest(
    state: &PrimaryAppState,
    source_file: &file::Model,
    blob: &file_blob::Model,
    limits: &ArchivePreviewLimits,
) -> Result<Option<ArchiveRawManifest>> {
    let Some(prop) = property_repo::find_by_key(
        state.reader_db(),
        EntityType::File,
        source_file.id,
        CACHE_NAMESPACE,
        ZIP_RAW_MANIFEST_CACHE_NAME,
    )
    .await?
    else {
        return Ok(None);
    };

    let Some(value) = prop.value else {
        return Ok(None);
    };
    let cached = match serde_json::from_str::<CachedArchiveRawManifest>(&value) {
        Ok(cached) => cached,
        Err(error) => {
            tracing::warn!(
                file_id = source_file.id,
                property_id = prop.id,
                "failed to parse archive preview cache: {error}"
            );
            return Ok(None);
        }
    };

    if cached.schema_version == RAW_CACHE_SCHEMA_VERSION
        && cached.source_blob_id == blob.id
        && cached.source_hash == blob.hash
        && cached.limit_signature == limits.raw_signature
        && cached.manifest.schema_version == RAW_CACHE_SCHEMA_VERSION
        && cached.manifest.format == FORMAT_ZIP
    {
        return Ok(Some(cached.manifest));
    }

    Ok(None)
}

pub(crate) async fn store_cached_manifest(
    state: &PrimaryAppState,
    source_file: &file::Model,
    blob: &file_blob::Model,
    limits: &ArchivePreviewLimits,
    manifest: &ArchiveRawManifest,
) -> Result<()> {
    let serialized =
        serialize_cached_raw_manifest(blob.id, &blob.hash, &limits.raw_signature, manifest)?;
    if serialized.len() > ENTITY_PROPERTY_VALUE_MAX_BYTES {
        return Err(archive_preview_validation_error(
            ApiSubcode::ArchivePreviewManifestTooLarge,
            format!(
                "archive preview manifest for file #{} exceeds entity property limit {} bytes",
                source_file.id, ENTITY_PROPERTY_VALUE_MAX_BYTES
            ),
        ));
    }

    property_repo::upsert(
        state.writer_db(),
        EntityType::File,
        source_file.id,
        CACHE_NAMESPACE,
        ZIP_RAW_MANIFEST_CACHE_NAME,
        Some(&serialized),
    )
    .await?;
    Ok(())
}

pub(super) fn fit_raw_manifest_to_cache_limit(
    file_id: i64,
    source_blob_id: i64,
    source_hash: &str,
    limit_signature: &str,
    manifest: ArchiveRawManifest,
) -> Result<ArchiveRawManifest> {
    if serialized_cached_raw_manifest_len(source_blob_id, source_hash, limit_signature, &manifest)?
        <= ENTITY_PROPERTY_VALUE_MAX_BYTES
    {
        return Ok(manifest);
    }

    let mut base = manifest;
    let original_entries = std::mem::take(&mut base.entries);
    let mut low = 0_usize;
    let mut high = original_entries.len();
    let mut best_entry_count = None;

    while low <= high {
        let mid = low + (high - low) / 2;
        let mut candidate = base.clone();
        candidate.entries = original_entries[..mid].to_vec();

        if serialized_cached_raw_manifest_len(
            source_blob_id,
            source_hash,
            limit_signature,
            &candidate,
        )? <= ENTITY_PROPERTY_VALUE_MAX_BYTES
        {
            best_entry_count = Some(mid);
            low = mid.saturating_add(1);
        } else if mid == 0 {
            break;
        } else {
            high = mid - 1;
        }
    }

    if let Some(entry_count) = best_entry_count {
        base.entries = original_entries[..entry_count].to_vec();
        return Ok(base);
    }

    Err(archive_preview_validation_error(
        ApiSubcode::ArchivePreviewManifestTooLarge,
        format!(
            "archive raw preview manifest for file #{file_id} exceeds entity property limit {ENTITY_PROPERTY_VALUE_MAX_BYTES} bytes"
        ),
    ))
}

fn serialized_cached_raw_manifest_len(
    source_blob_id: i64,
    source_hash: &str,
    limit_signature: &str,
    manifest: &ArchiveRawManifest,
) -> Result<usize> {
    serde_json::to_vec(&CachedArchiveRawManifestRef {
        schema_version: RAW_CACHE_SCHEMA_VERSION,
        source_blob_id,
        source_hash,
        limit_signature,
        manifest,
    })
    .map(|bytes| bytes.len())
    .map_aster_err_ctx(
        "serialize archive preview cache",
        AsterError::internal_error,
    )
}

pub(super) fn serialize_cached_raw_manifest(
    source_blob_id: i64,
    source_hash: &str,
    limit_signature: &str,
    manifest: &ArchiveRawManifest,
) -> Result<String> {
    serde_json::to_string(&CachedArchiveRawManifestRef {
        schema_version: RAW_CACHE_SCHEMA_VERSION,
        source_blob_id,
        source_hash,
        limit_signature,
        manifest,
    })
    .map_aster_err_ctx(
        "serialize archive preview cache",
        AsterError::internal_error,
    )
}

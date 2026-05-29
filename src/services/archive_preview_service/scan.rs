use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use base64::Engine as _;
use chrono::Utc;

use crate::api::subcode::ApiSubcode;
use crate::entities::{file, file_blob};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::services::archive_service::format::ArchiveFormat;
use crate::services::archive_service::range_reader::StorageRangeReader;
use crate::services::archive_service::scan::{
    ArchiveRawScanEntry, ArchiveScanEntryKind, ArchiveScanLimits, ArchiveScanNamePolicy,
};
use crate::services::archive_service::seven_zip_scan::{
    open_seven_zip_streaming_archive, scan_seven_zip_archive_raw,
};
use crate::services::archive_service::zip_scan::{
    build_zip_scan_result_from_raw_entries, scan_zip_archive_raw,
};
use crate::storage::StorageDriver;

use super::cache::fit_raw_manifest_to_cache_limit;
use super::model::{
    ArchivePreviewEntry, ArchivePreviewEntryKind, ArchivePreviewExtractCompatibility,
    ArchivePreviewManifest, ArchiveRawEntry, ArchiveRawManifest,
};
use super::{
    ArchivePreviewLimits, CACHE_SCHEMA_VERSION, ENTITY_PROPERTY_VALUE_MAX_BYTES,
    RAW_CACHE_SCHEMA_VERSION, archive_preview_validation_error, map_archive_open_error,
    map_archive_preview_scan_error,
};

pub(crate) async fn scan_manifest_from_temp(
    source_file: &file::Model,
    blob: &file_blob::Model,
    temp_path: &Path,
    limits: &ArchivePreviewLimits,
) -> Result<ArchiveRawManifest> {
    let path = temp_path.to_path_buf();
    scan_manifest_with_reader(
        source_file.id,
        blob.id,
        blob.hash.clone(),
        crate::utils::numbers::i64_to_u64(source_file.size, "source archive size")?,
        limits,
        move || {
            let file = std::fs::File::open(&path).map_aster_err_ctx(
                "open archive preview temp file",
                AsterError::storage_driver_error,
            )?;
            Ok(file)
        },
    )
    .await
}

pub(crate) async fn scan_manifest_from_storage_range(
    source_file: &file::Model,
    blob: &file_blob::Model,
    driver: Arc<dyn StorageDriver>,
    limits: &ArchivePreviewLimits,
) -> Result<ArchiveRawManifest> {
    let source_size = crate::utils::numbers::i64_to_u64(source_file.size, "source archive size")?;
    let storage_path = blob.storage_path.clone();
    let handle = tokio::runtime::Handle::current();
    scan_manifest_with_reader(
        source_file.id,
        blob.id,
        blob.hash.clone(),
        source_size,
        limits,
        move || {
            Ok(StorageRangeReader::new(
                driver,
                storage_path,
                source_size,
                handle,
            ))
        },
    )
    .await
}

async fn scan_manifest_with_reader<R, F>(
    source_file_id: i64,
    source_blob_id: i64,
    source_hash: String,
    source_archive_size: u64,
    limits: &ArchivePreviewLimits,
    make_reader: F,
) -> Result<ArchiveRawManifest>
where
    R: std::io::Read + std::io::Seek + Send + 'static,
    F: FnOnce() -> Result<R> + Send + 'static,
{
    let scan_limits = limits.scan_limits;
    let archive_format = limits.archive_format;
    let raw_signature = limits.raw_signature.clone();
    let deadline =
        Instant::now().checked_add(std::time::Duration::from_secs(limits.max_duration_secs));
    let generated_at = Utc::now().to_rfc3339();
    let manifest_source_hash = source_hash.clone();

    let manifest = tokio::task::spawn_blocking(move || {
        let reader = make_reader()?;
        let scanned = match archive_format {
            ArchiveFormat::Zip => {
                let mut archive = zip::ZipArchive::new(reader).map_err(map_archive_open_error)?;
                scan_zip_archive_raw(&mut archive, scan_limits, deadline)
                    .map_err(map_archive_preview_scan_error)?
            }
            ArchiveFormat::SevenZip => {
                let archive = open_seven_zip_streaming_archive(reader, scan_limits)
                    .map_err(map_seven_zip_preview_open_error)?;
                scan_seven_zip_archive_raw(&archive, scan_limits, source_archive_size, deadline)
                    .map_err(map_archive_preview_scan_error)?
            }
        };
        let entries = scanned
            .entries
            .into_iter()
            .map(raw_entry_to_cache_entry)
            .collect::<Result<Vec<_>>>()?;

        Ok::<_, AsterError>(ArchiveRawManifest {
            schema_version: RAW_CACHE_SCHEMA_VERSION,
            format: archive_format.as_str().to_string(),
            source_blob_id,
            source_hash: manifest_source_hash,
            generated_at,
            entry_count: crate::utils::numbers::u64_to_i64(
                scanned.entry_count,
                "archive preview entry count",
            )?,
            file_count: crate::utils::numbers::u64_to_i64(
                scanned.file_count,
                "archive preview file count",
            )?,
            directory_count: crate::utils::numbers::u64_to_i64(
                scanned.directory_count,
                "archive preview directory count",
            )?,
            total_uncompressed_size: scanned.total_uncompressed_bytes,
            entries,
        })
    })
    .await
    .map_err(|error| {
        AsterError::internal_error(format!("archive preview worker failed: {error}"))
    })??;

    fit_raw_manifest_to_cache_limit(
        source_file_id,
        source_blob_id,
        &source_hash,
        &raw_signature,
        manifest,
    )
}

fn raw_entry_to_cache_entry(entry: ArchiveRawScanEntry) -> Result<ArchiveRawEntry> {
    Ok(ArchiveRawEntry {
        index: entry.index,
        raw_name: base64::engine::general_purpose::STANDARD.encode(entry.raw_name),
        display_name: entry.display_name,
        zip_utf8: entry.zip_utf8,
        kind: match entry.kind {
            ArchiveScanEntryKind::File => ArchivePreviewEntryKind::File,
            ArchiveScanEntryKind::Directory => ArchivePreviewEntryKind::Directory,
        },
        size: entry.size,
        compressed_size: entry.compressed_size,
        modified_at: entry.modified_at,
    })
}

fn cache_entry_to_raw_scan_entry(entry: &ArchiveRawEntry) -> Result<ArchiveRawScanEntry> {
    let raw_name = base64::engine::general_purpose::STANDARD
        .decode(&entry.raw_name)
        .map_aster_err_ctx("decode archive raw entry name", AsterError::internal_error)?;
    Ok(ArchiveRawScanEntry {
        index: entry.index,
        raw_name,
        display_name: entry.display_name.clone(),
        zip_utf8: entry.zip_utf8,
        kind: match entry.kind {
            ArchivePreviewEntryKind::File => ArchiveScanEntryKind::File,
            ArchivePreviewEntryKind::Directory => ArchiveScanEntryKind::Directory,
        },
        size: entry.size,
        compressed_size: entry.compressed_size,
        modified_at: entry.modified_at.clone(),
    })
}

pub(super) fn build_manifest_from_raw(
    source_file_id: i64,
    raw_manifest: &ArchiveRawManifest,
    limits: &ArchivePreviewLimits,
) -> Result<ArchivePreviewManifest> {
    let raw_entries = raw_manifest
        .entries
        .iter()
        .map(cache_entry_to_raw_scan_entry)
        .collect::<Result<Vec<_>>>()?;
    let scan_limits = cached_raw_display_scan_limits(raw_manifest, &raw_entries, limits)?;
    let scanned = build_zip_scan_result_from_raw_entries(
        &raw_entries,
        scan_limits,
        None,
        limits.filename_encoding,
        ArchiveScanNamePolicy::PreviewDisplayName,
        |_| Ok(()),
    )
    .map_err(map_archive_preview_scan_error)?;
    let entry_count = max_i64_u64_count(
        raw_manifest.entry_count,
        scanned.entry_count,
        "archive preview entry count",
    )?;
    let file_count = max_i64_u64_count(
        raw_manifest.file_count,
        scanned.file_count,
        "archive preview file count",
    )?;
    let directory_count = max_i64_u64_count(
        raw_manifest.directory_count,
        scanned.directory_count,
        "archive preview directory count",
    )?;
    let total_uncompressed_size = raw_manifest
        .total_uncompressed_size
        .max(scanned.total_uncompressed_bytes);
    let extract_compatibility = if raw_manifest.entries_truncated() {
        ArchivePreviewExtractCompatibility::unsupported(
            super::model::ArchivePreviewExtractUnsupportedReason::UnsupportedEntryNames,
        )
    } else {
        ArchivePreviewExtractCompatibility::from_scan_extract_compatible(scanned.extract_compatible)
    };
    let entries = scanned
        .entries
        .into_iter()
        .map(|entry| ArchivePreviewEntry {
            path: entry.path,
            name: entry.name,
            parent: entry.parent,
            kind: match entry.kind {
                ArchiveScanEntryKind::File => ArchivePreviewEntryKind::File,
                ArchiveScanEntryKind::Directory => ArchivePreviewEntryKind::Directory,
            },
            size: entry.size,
            compressed_size: entry.compressed_size,
            modified_at: entry.modified_at,
        })
        .collect();

    let manifest = ArchivePreviewManifest {
        schema_version: CACHE_SCHEMA_VERSION,
        format: raw_manifest.format.clone(),
        source_blob_id: raw_manifest.source_blob_id,
        source_hash: raw_manifest.source_hash.clone(),
        generated_at: raw_manifest.generated_at.clone(),
        entry_count,
        file_count,
        directory_count,
        total_uncompressed_size,
        truncated: raw_manifest.entries_truncated(),
        extract_compatibility,
        entries,
    };

    fit_manifest_to_limit(source_file_id, manifest, limits.max_manifest_bytes)
}

fn map_seven_zip_preview_open_error(error: AsterError) -> AsterError {
    if matches!(error, AsterError::ValidationError(_))
        && error
            .message()
            .to_ascii_lowercase()
            .contains("invalid 7z archive")
    {
        return archive_preview_validation_error(
            ApiSubcode::ArchivePreviewInvalidZip,
            "invalid 7z archive",
        );
    }
    map_archive_preview_scan_error(error)
}

fn cached_raw_display_scan_limits(
    raw_manifest: &ArchiveRawManifest,
    raw_entries: &[ArchiveRawScanEntry],
    limits: &ArchivePreviewLimits,
) -> Result<ArchiveScanLimits> {
    let mut scan_limits = limits.scan_limits;
    let cached_entry_count =
        crate::utils::numbers::usize_to_u64(raw_entries.len(), "archive cached raw entry count")?;
    let stored_entry_count =
        crate::utils::numbers::i64_to_u64(raw_manifest.entry_count, "archive preview entry count")?;
    let stored_file_count =
        crate::utils::numbers::i64_to_u64(raw_manifest.file_count, "archive preview file count")?;

    scan_limits.max_entries = scan_limits
        .max_entries
        .max(cached_entry_count)
        .max(stored_entry_count);
    scan_limits.max_files = scan_limits
        .max_files
        .max(cached_entry_count)
        .max(stored_file_count);
    scan_limits.max_directories = u64::MAX;
    scan_limits.max_uncompressed_bytes = scan_limits
        .max_uncompressed_bytes
        .max(raw_manifest.total_uncompressed_size);
    scan_limits.max_compression_ratio = u64::MAX;
    scan_limits.max_entry_compression_ratio = u64::MAX;

    Ok(scan_limits)
}

fn max_i64_u64_count(stored: i64, scanned: u64, value_name: &str) -> Result<i64> {
    Ok(stored.max(crate::utils::numbers::u64_to_i64(scanned, value_name)?))
}

fn fit_manifest_to_limit(
    file_id: i64,
    manifest: ArchivePreviewManifest,
    max_manifest_bytes: usize,
) -> Result<ArchivePreviewManifest> {
    if manifest_fits_limits(&manifest, max_manifest_bytes)? {
        return Ok(manifest);
    }

    let mut base = manifest;
    let original_entries = std::mem::take(&mut base.entries);
    base.truncated = true;
    let mut low = 0_usize;
    let mut high = original_entries.len();
    let mut best_entry_count = None;

    while low <= high {
        let mid = low + (high - low) / 2;
        let mut candidate = base.clone();
        candidate.entries = original_entries[..mid].to_vec();

        if manifest_fits_limits(&candidate, max_manifest_bytes)? {
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
            "archive preview manifest for file #{file_id} exceeds server limit {max_manifest_bytes} bytes or entity property limit {ENTITY_PROPERTY_VALUE_MAX_BYTES} bytes"
        ),
    ))
}

fn manifest_fits_limits(
    manifest: &ArchivePreviewManifest,
    max_manifest_bytes: usize,
) -> Result<bool> {
    if serialized_manifest_len(manifest)? > max_manifest_bytes {
        return Ok(false);
    }
    Ok(true)
}

fn serialized_manifest_len(manifest: &ArchivePreviewManifest) -> Result<usize> {
    serde_json::to_vec(manifest)
        .map(|bytes| bytes.len())
        .map_aster_err_ctx(
            "serialize archive preview manifest",
            AsterError::internal_error,
        )
}

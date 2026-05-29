//! ZIP 中央目录扫描与安全校验。

use std::collections::HashSet;
use std::io::{Read, Seek};
use std::path::Path;
use std::time::Instant;

use zip::HasZipMetadata;

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::types::ArchiveFilenameEncoding;

mod encoding;
mod entry;

use super::path::{
    ensure_archive_entry_path_not_conflicting, insert_directory_path_with_limit,
    normalize_archive_entry_path, validate_archive_entry_compression_ratio,
    validate_archive_entry_path_limits, validate_total_archive_compression_ratio,
};
use super::scan::{
    ArchiveRawScanEntry, ArchiveRawScanResult, ArchiveScanEntryKind, ArchiveScanLimits,
    ArchiveScanNamePolicy, ArchiveScanResult, ensure_archive_scan_deadline,
};
use encoding::{decode_zip_entry_name, decode_zip_entry_name_parts};
use entry::{
    build_scan_entry, build_scan_entry_from_parts, format_zip_datetime, map_zip_entry_error,
    validate_zip_entry_supported,
};

pub(crate) fn scan_zip_archive<R, F>(
    archive: &mut zip::ZipArchive<R>,
    limits: ArchiveScanLimits,
    deadline: Option<Instant>,
    filename_encoding: ArchiveFilenameEncoding,
    name_policy: ArchiveScanNamePolicy,
    mut ensure_file_size_allowed: F,
) -> Result<ArchiveScanResult>
where
    R: Read + Seek,
    F: FnMut(i64) -> Result<()>,
{
    let entry_count = crate::utils::numbers::usize_to_u64(archive.len(), "archive entry count")?;
    if entry_count > limits.max_entries {
        return Err(AsterError::validation_error(format!(
            "archive contains {} entries, exceeds server limit {}",
            entry_count, limits.max_entries
        )));
    }

    let mut total_uncompressed_bytes = 0_i64;
    let mut total_compressed_bytes = 0_u64;
    let mut file_count = 0_u64;
    let mut extract_compatible = true;
    let mut seen_paths = HashSet::new();
    let mut directory_paths = HashSet::new();
    let mut file_paths = HashSet::new();
    let mut entries = Vec::with_capacity(archive.len());

    for index in 0..archive.len() {
        ensure_archive_scan_deadline(deadline)?;
        let entry = archive.by_index_raw(index).map_err(map_zip_entry_error)?;
        let decoded_name = decode_zip_entry_name(&entry, filename_encoding)?;
        validate_zip_entry_supported(&entry, &decoded_name)?;
        if matches!(name_policy, ArchiveScanNamePolicy::PreviewDisplayName)
            && normalize_archive_entry_path(&decoded_name, ArchiveScanNamePolicy::StrictAsterName)
                .is_err()
        {
            extract_compatible = false;
        }
        let relative_path = normalize_archive_entry_path(&decoded_name, name_policy)?;
        validate_archive_entry_path_limits(&relative_path, limits)?;
        ensure_archive_entry_path_not_conflicting(
            &relative_path,
            entry.is_dir(),
            &mut seen_paths,
            &directory_paths,
            &file_paths,
        )?;

        if entry.is_dir() {
            insert_directory_path_with_limit(&relative_path, &mut directory_paths, limits)?;
            entries.push(build_scan_entry(
                index,
                relative_path,
                ArchiveScanEntryKind::Directory,
                0,
                0,
                entry.last_modified(),
            )?);
            continue;
        }

        if let Some(parent) = relative_path.parent() {
            insert_directory_path_with_limit(parent, &mut directory_paths, limits)?;
        }
        file_count = file_count
            .checked_add(1)
            .ok_or_else(|| AsterError::internal_error("archive file count overflow"))?;
        if file_count > limits.max_files {
            return Err(AsterError::validation_error(format!(
                "archive contains {} files, exceeds server limit {}",
                file_count, limits.max_files
            )));
        }

        let entry_size = crate::utils::numbers::u64_to_i64(entry.size(), "archive entry size")?;
        ensure_file_size_allowed(entry_size)?;
        validate_archive_entry_compression_ratio(
            entry.size(),
            entry.compressed_size(),
            limits.max_entry_compression_ratio,
            &relative_path,
        )?;
        total_uncompressed_bytes = total_uncompressed_bytes
            .checked_add(entry_size)
            .ok_or_else(|| AsterError::internal_error("archive extract size overflow"))?;
        if total_uncompressed_bytes > limits.max_uncompressed_bytes {
            return Err(AsterError::validation_error(format!(
                "archive uncompressed size {} exceeds server limit {}",
                total_uncompressed_bytes, limits.max_uncompressed_bytes
            )));
        }
        total_compressed_bytes = total_compressed_bytes
            .checked_add(entry.compressed_size())
            .ok_or_else(|| AsterError::internal_error("archive compressed size overflow"))?;
        file_paths.insert(relative_path.clone());
        entries.push(build_scan_entry(
            index,
            relative_path,
            ArchiveScanEntryKind::File,
            entry_size,
            crate::utils::numbers::u64_to_i64(
                entry.compressed_size(),
                "archive entry compressed size",
            )?,
            entry.last_modified(),
        )?);
    }

    validate_total_archive_compression_ratio(
        total_uncompressed_bytes,
        total_compressed_bytes,
        limits.max_compression_ratio,
    )?;

    Ok(ArchiveScanResult {
        entry_count,
        file_count,
        directory_count: directory_paths.len().try_into().map_aster_err_with(|| {
            AsterError::internal_error("directory count exceeds u64 range")
        })?,
        total_uncompressed_bytes,
        extract_compatible,
        entries,
    })
}

pub(crate) fn scan_zip_archive_raw<R>(
    archive: &mut zip::ZipArchive<R>,
    limits: ArchiveScanLimits,
    deadline: Option<Instant>,
) -> Result<ArchiveRawScanResult>
where
    R: Read + Seek,
{
    let entry_count = crate::utils::numbers::usize_to_u64(archive.len(), "archive entry count")?;
    if entry_count > limits.max_entries {
        return Err(AsterError::validation_error(format!(
            "archive contains {} entries, exceeds server limit {}",
            entry_count, limits.max_entries
        )));
    }

    let mut total_uncompressed_bytes = 0_i64;
    let mut total_compressed_bytes = 0_u64;
    let mut file_count = 0_u64;
    let mut directory_count = 0_u64;
    let mut entries = Vec::with_capacity(archive.len());

    for index in 0..archive.len() {
        ensure_archive_scan_deadline(deadline)?;
        let entry = archive.by_index_raw(index).map_err(map_zip_entry_error)?;
        validate_zip_entry_supported(&entry, entry.name())?;
        let raw_name = entry.name_raw().to_vec();
        let display_name = entry.name().to_string();
        let raw_name_utf8 = entry.get_metadata().is_utf8;

        if entry.is_dir() {
            directory_count = directory_count
                .checked_add(1)
                .ok_or_else(|| AsterError::internal_error("archive directory count overflow"))?;
            if directory_count > limits.max_directories {
                return Err(AsterError::validation_error(format!(
                    "archive contains {} directories, exceeds server limit {}",
                    directory_count, limits.max_directories
                )));
            }
            entries.push(ArchiveRawScanEntry {
                index,
                raw_name,
                display_name,
                raw_name_utf8,
                kind: ArchiveScanEntryKind::Directory,
                size: 0,
                compressed_size: 0,
                modified_at: entry.last_modified().and_then(format_zip_datetime),
            });
            continue;
        }

        file_count = file_count
            .checked_add(1)
            .ok_or_else(|| AsterError::internal_error("archive file count overflow"))?;
        if file_count > limits.max_files {
            return Err(AsterError::validation_error(format!(
                "archive contains {} files, exceeds server limit {}",
                file_count, limits.max_files
            )));
        }

        let entry_size = crate::utils::numbers::u64_to_i64(entry.size(), "archive entry size")?;
        total_uncompressed_bytes = total_uncompressed_bytes
            .checked_add(entry_size)
            .ok_or_else(|| AsterError::internal_error("archive extract size overflow"))?;
        if total_uncompressed_bytes > limits.max_uncompressed_bytes {
            return Err(AsterError::validation_error(format!(
                "archive uncompressed size {} exceeds server limit {}",
                total_uncompressed_bytes, limits.max_uncompressed_bytes
            )));
        }
        total_compressed_bytes = total_compressed_bytes
            .checked_add(entry.compressed_size())
            .ok_or_else(|| AsterError::internal_error("archive compressed size overflow"))?;
        validate_archive_entry_compression_ratio(
            entry.size(),
            entry.compressed_size(),
            limits.max_entry_compression_ratio,
            Path::new(entry.name()),
        )?;
        entries.push(ArchiveRawScanEntry {
            index,
            raw_name,
            display_name,
            raw_name_utf8,
            kind: ArchiveScanEntryKind::File,
            size: entry_size,
            compressed_size: crate::utils::numbers::u64_to_i64(
                entry.compressed_size(),
                "archive entry compressed size",
            )?,
            modified_at: entry.last_modified().and_then(format_zip_datetime),
        });
    }

    validate_total_archive_compression_ratio(
        total_uncompressed_bytes,
        total_compressed_bytes,
        limits.max_compression_ratio,
    )?;

    Ok(ArchiveRawScanResult {
        entry_count,
        file_count,
        directory_count,
        total_uncompressed_bytes,
        total_compressed_base: total_compressed_bytes,
        entries,
    })
}

pub(crate) fn build_zip_scan_result_from_raw_entries<F>(
    raw_entries: &[ArchiveRawScanEntry],
    limits: ArchiveScanLimits,
    deadline: Option<Instant>,
    filename_encoding: ArchiveFilenameEncoding,
    name_policy: ArchiveScanNamePolicy,
    mut ensure_file_size_allowed: F,
) -> Result<ArchiveScanResult>
where
    F: FnMut(i64) -> Result<()>,
{
    let entry_count =
        crate::utils::numbers::usize_to_u64(raw_entries.len(), "archive entry count")?;
    if entry_count > limits.max_entries {
        return Err(AsterError::validation_error(format!(
            "archive contains {} entries, exceeds server limit {}",
            entry_count, limits.max_entries
        )));
    }

    let mut total_uncompressed_bytes = 0_i64;
    let mut total_compressed_bytes = 0_u64;
    let mut file_count = 0_u64;
    let mut extract_compatible = true;
    let mut seen_paths = HashSet::new();
    let mut directory_paths = HashSet::new();
    let mut file_paths = HashSet::new();
    let mut entries = Vec::with_capacity(raw_entries.len());

    for raw_entry in raw_entries {
        ensure_archive_scan_deadline(deadline)?;
        let decoded_name = decode_zip_entry_name_parts(
            &raw_entry.raw_name,
            &raw_entry.display_name,
            raw_entry.raw_name_utf8,
            filename_encoding,
        )?;
        if matches!(name_policy, ArchiveScanNamePolicy::PreviewDisplayName)
            && normalize_archive_entry_path(&decoded_name, ArchiveScanNamePolicy::StrictAsterName)
                .is_err()
        {
            extract_compatible = false;
        }
        let relative_path = normalize_archive_entry_path(&decoded_name, name_policy)?;
        validate_archive_entry_path_limits(&relative_path, limits)?;
        ensure_archive_entry_path_not_conflicting(
            &relative_path,
            raw_entry.kind.is_dir(),
            &mut seen_paths,
            &directory_paths,
            &file_paths,
        )?;

        if raw_entry.kind.is_dir() {
            insert_directory_path_with_limit(&relative_path, &mut directory_paths, limits)?;
            entries.push(build_scan_entry_from_parts(
                raw_entry.index,
                relative_path,
                ArchiveScanEntryKind::Directory,
                0,
                0,
                raw_entry.modified_at.clone(),
            )?);
            continue;
        }

        if let Some(parent) = relative_path.parent() {
            insert_directory_path_with_limit(parent, &mut directory_paths, limits)?;
        }
        file_count = file_count
            .checked_add(1)
            .ok_or_else(|| AsterError::internal_error("archive file count overflow"))?;
        if file_count > limits.max_files {
            return Err(AsterError::validation_error(format!(
                "archive contains {} files, exceeds server limit {}",
                file_count, limits.max_files
            )));
        }
        ensure_file_size_allowed(raw_entry.size)?;
        let entry_size_u64 =
            crate::utils::numbers::i64_to_u64(raw_entry.size, "archive entry size")?;
        let compressed_size_u64 = crate::utils::numbers::i64_to_u64(
            raw_entry.compressed_size,
            "archive entry compressed size",
        )?;
        validate_archive_entry_compression_ratio(
            entry_size_u64,
            compressed_size_u64,
            limits.max_entry_compression_ratio,
            &relative_path,
        )?;
        total_uncompressed_bytes = total_uncompressed_bytes
            .checked_add(raw_entry.size)
            .ok_or_else(|| AsterError::internal_error("archive extract size overflow"))?;
        if total_uncompressed_bytes > limits.max_uncompressed_bytes {
            return Err(AsterError::validation_error(format!(
                "archive uncompressed size {} exceeds server limit {}",
                total_uncompressed_bytes, limits.max_uncompressed_bytes
            )));
        }
        total_compressed_bytes = total_compressed_bytes
            .checked_add(compressed_size_u64)
            .ok_or_else(|| AsterError::internal_error("archive compressed size overflow"))?;
        file_paths.insert(relative_path.clone());
        entries.push(build_scan_entry_from_parts(
            raw_entry.index,
            relative_path,
            ArchiveScanEntryKind::File,
            raw_entry.size,
            raw_entry.compressed_size,
            raw_entry.modified_at.clone(),
        )?);
    }

    validate_total_archive_compression_ratio(
        total_uncompressed_bytes,
        total_compressed_bytes,
        limits.max_compression_ratio,
    )?;

    Ok(ArchiveScanResult {
        entry_count,
        file_count,
        directory_count: directory_paths.len().try_into().map_aster_err_with(|| {
            AsterError::internal_error("directory count exceeds u64 range")
        })?,
        total_uncompressed_bytes,
        extract_compatible,
        entries,
    })
}

#[cfg(test)]
mod tests;

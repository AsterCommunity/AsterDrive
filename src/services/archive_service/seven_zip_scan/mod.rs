//! 7z archive scanning and safety validation.

use std::collections::HashSet;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use chrono::{DateTime, Utc};
use zesven::{StreamingArchive, StreamingConfig};

use crate::errors::{AsterError, MapAsterErr, Result};

use super::path::{
    ensure_archive_entry_path_not_conflicting, insert_directory_path_with_limit,
    normalize_archive_entry_path, validate_archive_entry_compression_ratio,
    validate_archive_entry_path_limits, validate_total_archive_compression_ratio,
};
use super::scan::{
    ArchiveRawScanEntry, ArchiveRawScanResult, ArchiveScanEntry, ArchiveScanEntryKind,
    ArchiveScanLimits, ArchiveScanNamePolicy, ArchiveScanResult, ensure_archive_scan_deadline,
};

const UNIX_FILE_TYPE_MASK: u32 = 0o170000;
const UNIX_REGULAR_FILE_MODE: u32 = 0o100000;
const UNIX_DIRECTORY_MODE: u32 = 0o040000;

pub(crate) fn seven_zip_streaming_config(limits: ArchiveScanLimits) -> Result<StreamingConfig> {
    let max_entries =
        crate::utils::numbers::u64_to_usize(limits.max_entries, "archive entry count")?;
    let max_ratio: u32 = limits
        .max_entry_compression_ratio
        .min(limits.max_compression_ratio)
        .try_into()
        .map_aster_err_with(|| {
            AsterError::validation_error("archive compression ratio limit exceeds 7z parser range")
        })?;
    Ok(StreamingConfig::low_memory()
        .max_entries(max_entries)
        .max_compression_ratio(max_ratio)
        .disable_decoder_pool())
}

pub(crate) fn scan_seven_zip_archive<R, F>(
    archive: &StreamingArchive<R>,
    limits: ArchiveScanLimits,
    source_archive_size: u64,
    deadline: Option<Instant>,
    name_policy: ArchiveScanNamePolicy,
    mut ensure_file_size_allowed: F,
) -> Result<ArchiveScanResult>
where
    R: Read + Seek + Send,
    F: FnMut(i64) -> Result<()>,
{
    ensure_no_skipped_entries(archive)?;
    let entries = archive.entries_list();
    let entry_count = crate::utils::numbers::usize_to_u64(entries.len(), "archive entry count")?;
    if entry_count > limits.max_entries {
        return Err(AsterError::validation_error(format!(
            "archive contains {} entries, exceeds server limit {}",
            entry_count, limits.max_entries
        )));
    }

    let mut total_uncompressed_bytes = 0_i64;
    let mut file_count = 0_u64;
    let mut extract_compatible = true;
    let mut seen_paths = HashSet::new();
    let mut directory_paths = HashSet::new();
    let mut file_paths = HashSet::new();
    let mut scanned_entries = Vec::with_capacity(entries.len());

    for (index, entry) in entries.iter().enumerate() {
        ensure_archive_scan_deadline(deadline)?;
        validate_seven_zip_entry_supported(entry)?;
        let entry_path = entry.path.as_str();
        if matches!(name_policy, ArchiveScanNamePolicy::PreviewDisplayName)
            && normalize_archive_entry_path(entry_path, ArchiveScanNamePolicy::StrictAsterName)
                .is_err()
        {
            extract_compatible = false;
        }
        let relative_path = normalize_archive_entry_path(entry_path, name_policy)?;
        validate_archive_entry_path_limits(&relative_path, limits)?;
        ensure_archive_entry_path_not_conflicting(
            &relative_path,
            entry.is_directory,
            &mut seen_paths,
            &directory_paths,
            &file_paths,
        )?;

        if entry.is_directory {
            insert_directory_path_with_limit(&relative_path, &mut directory_paths, limits)?;
            scanned_entries.push(build_scan_entry(
                index,
                relative_path,
                ArchiveScanEntryKind::Directory,
                0,
                0,
                entry.modified(),
            )?);
            continue;
        }

        if let Some(parent) = relative_path.parent()
            && !parent.as_os_str().is_empty()
        {
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

        let entry_size = crate::utils::numbers::u64_to_i64(entry.size, "archive entry size")?;
        ensure_file_size_allowed(entry_size)?;
        validate_archive_entry_compression_ratio(
            entry.size,
            source_archive_size,
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

        file_paths.insert(relative_path.clone());
        scanned_entries.push(build_scan_entry(
            index,
            relative_path,
            ArchiveScanEntryKind::File,
            entry_size,
            compressed_size_for_manifest(source_archive_size)?,
            entry.modified(),
        )?);
    }

    validate_total_archive_compression_ratio(
        total_uncompressed_bytes,
        source_archive_size,
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
        entries: scanned_entries,
    })
}

pub(crate) fn scan_seven_zip_archive_raw<R>(
    archive: &StreamingArchive<R>,
    limits: ArchiveScanLimits,
    source_archive_size: u64,
    deadline: Option<Instant>,
) -> Result<ArchiveRawScanResult>
where
    R: Read + Seek + Send,
{
    ensure_no_skipped_entries(archive)?;
    let entries = archive.entries_list();
    let entry_count = crate::utils::numbers::usize_to_u64(entries.len(), "archive entry count")?;
    if entry_count > limits.max_entries {
        return Err(AsterError::validation_error(format!(
            "archive contains {} entries, exceeds server limit {}",
            entry_count, limits.max_entries
        )));
    }

    let mut total_uncompressed_bytes = 0_i64;
    let mut file_count = 0_u64;
    let mut directory_count = 0_u64;
    let mut raw_entries = Vec::with_capacity(entries.len());

    for (index, entry) in entries.iter().enumerate() {
        ensure_archive_scan_deadline(deadline)?;
        validate_seven_zip_entry_supported(entry)?;
        let entry_path = entry.path.as_str();
        let raw_name = entry_path.as_bytes().to_vec();
        let display_name = entry_path.to_string();

        if entry.is_directory {
            directory_count = directory_count
                .checked_add(1)
                .ok_or_else(|| AsterError::internal_error("archive directory count overflow"))?;
            if directory_count > limits.max_directories {
                return Err(AsterError::validation_error(format!(
                    "archive contains {} directories, exceeds server limit {}",
                    directory_count, limits.max_directories
                )));
            }
            raw_entries.push(ArchiveRawScanEntry {
                index,
                raw_name,
                display_name,
                zip_utf8: true,
                kind: ArchiveScanEntryKind::Directory,
                size: 0,
                compressed_size: 0,
                modified_at: entry.modified().map(format_system_time),
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

        let entry_size = crate::utils::numbers::u64_to_i64(entry.size, "archive entry size")?;
        validate_archive_entry_compression_ratio(
            entry.size,
            source_archive_size,
            limits.max_entry_compression_ratio,
            Path::new(entry_path),
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
        raw_entries.push(ArchiveRawScanEntry {
            index,
            raw_name,
            display_name,
            zip_utf8: true,
            kind: ArchiveScanEntryKind::File,
            size: entry_size,
            compressed_size: compressed_size_for_manifest(source_archive_size)?,
            modified_at: entry.modified().map(format_system_time),
        });
    }

    validate_total_archive_compression_ratio(
        total_uncompressed_bytes,
        source_archive_size,
        limits.max_compression_ratio,
    )?;

    Ok(ArchiveRawScanResult {
        entry_count,
        file_count,
        directory_count,
        total_uncompressed_bytes,
        entries: raw_entries,
    })
}

pub(crate) fn open_seven_zip_streaming_archive<R>(
    reader: R,
    limits: ArchiveScanLimits,
) -> Result<StreamingArchive<R>>
where
    R: Read + Seek + Send,
{
    StreamingArchive::open_with_config(reader, seven_zip_streaming_config(limits)?)
        .map_err(map_seven_zip_open_error)
}

pub(crate) fn map_seven_zip_open_error(error: zesven::Error) -> AsterError {
    map_seven_zip_error(error, "invalid 7z archive")
}

pub(crate) fn map_seven_zip_entry_error(error: zesven::Error) -> AsterError {
    map_seven_zip_error(error, "invalid 7z archive entry")
}

fn map_seven_zip_error(error: zesven::Error, fallback: &'static str) -> AsterError {
    if let zesven::Error::Io(ref io_error) = error
        && let Some(source) = io_error
            .get_ref()
            .and_then(|source| source.downcast_ref::<AsterError>())
    {
        return source.clone();
    }

    match error {
        zesven::Error::ResourceLimitExceeded(message) => AsterError::validation_error(message),
        zesven::Error::UnsupportedMethod { method_id } => AsterError::validation_error(format!(
            "archive uses unsupported 7z compression method {method_id:#x}"
        )),
        zesven::Error::UnsupportedFeature { feature } => {
            AsterError::validation_error(format!("archive uses unsupported 7z feature {feature}"))
        }
        zesven::Error::InvalidArchivePath(message) => {
            AsterError::validation_error(format!("archive entry contains unsafe path: {message}"))
        }
        zesven::Error::WrongPassword { .. }
        | zesven::Error::CryptoError(_)
        | zesven::Error::PasswordRequired => {
            AsterError::validation_error("encrypted 7z archives are not supported")
        }
        _ => AsterError::validation_error(fallback),
    }
}

fn ensure_no_skipped_entries<R>(archive: &StreamingArchive<R>) -> Result<()>
where
    R: Read + Seek + Send,
{
    if let Some(skipped) = archive.skipped_entries().first() {
        let path = skipped
            .raw_path
            .as_ref()
            .and_then(|raw| std::str::from_utf8(raw).ok())
            .unwrap_or("<invalid path>");
        return Err(AsterError::validation_error(format!(
            "archive entry '{}' contains unsafe path",
            path
        )));
    }
    Ok(())
}

fn validate_seven_zip_entry_supported(entry: &zesven::Entry) -> Result<()> {
    let entry_name = entry.path.as_str();
    if entry.is_encrypted {
        return Err(AsterError::validation_error(format!(
            "archive entry '{}' is encrypted; encrypted 7z entries are not supported",
            entry_name
        )));
    }
    if entry.is_symlink {
        return Err(AsterError::validation_error(format!(
            "archive entry '{}' is a symbolic link; symbolic links are not supported",
            entry_name
        )));
    }
    if entry.is_anti {
        return Err(AsterError::validation_error(format!(
            "archive entry '{}' is an anti-item; anti-items are not supported",
            entry_name
        )));
    }
    if let Some(mode) = entry.unix_mode() {
        let file_type = mode & UNIX_FILE_TYPE_MASK;
        if file_type != 0 && file_type != UNIX_REGULAR_FILE_MODE && file_type != UNIX_DIRECTORY_MODE
        {
            return Err(AsterError::validation_error(format!(
                "archive entry '{}' is a special file; only regular files and directories are supported",
                entry_name
            )));
        }
    }
    if !entry.is_file() && !entry.is_directory {
        return Err(AsterError::validation_error(format!(
            "archive entry '{}' is not a regular file or directory",
            entry_name
        )));
    }
    Ok(())
}

fn build_scan_entry(
    index: usize,
    relative_path: PathBuf,
    kind: ArchiveScanEntryKind,
    size: i64,
    compressed_size: i64,
    modified_at: Option<SystemTime>,
) -> Result<ArchiveScanEntry> {
    let path = relative_path.to_string_lossy().to_string();
    let name = relative_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AsterError::validation_error("archive entry name must be valid UTF-8"))?
        .to_string();
    let parent = relative_path.parent().and_then(|parent| {
        (!parent.as_os_str().is_empty()).then(|| parent.to_string_lossy().to_string())
    });

    Ok(ArchiveScanEntry {
        index,
        relative_path,
        path,
        name,
        parent,
        kind,
        size,
        compressed_size,
        modified_at: modified_at.map(format_system_time),
    })
}

fn compressed_size_for_manifest(source_archive_size: u64) -> Result<i64> {
    crate::utils::numbers::u64_to_i64(source_archive_size, "source archive size")
}

fn format_system_time(value: SystemTime) -> String {
    let datetime: DateTime<Utc> = value.into();
    datetime.to_rfc3339()
}

#[cfg(test)]
mod tests;

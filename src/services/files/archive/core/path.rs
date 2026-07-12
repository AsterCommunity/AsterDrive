use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::errors::{AsterError, Result};

use super::scan::{ArchiveScanLimits, ArchiveScanNamePolicy};

pub(crate) fn validate_archive_entry_path_limits(
    relative_path: &Path,
    limits: ArchiveScanLimits,
) -> Result<()> {
    let depth = crate::utils::numbers::usize_to_u64(
        relative_path.components().count(),
        "archive entry path depth",
    )?;
    if depth > limits.max_depth {
        return Err(AsterError::validation_error(format!(
            "archive entry '{}' depth {} exceeds server limit {}",
            relative_path.display(),
            depth,
            limits.max_depth
        )));
    }

    let path_bytes = crate::utils::numbers::usize_to_u64(
        relative_path.to_string_lossy().len(),
        "archive entry path bytes",
    )?;
    if path_bytes > limits.max_path_bytes {
        return Err(AsterError::validation_error(format!(
            "archive entry '{}' path length {} exceeds server limit {}",
            relative_path.display(),
            path_bytes,
            limits.max_path_bytes
        )));
    }

    Ok(())
}

pub(crate) fn ensure_archive_entry_path_not_conflicting(
    relative_path: &Path,
    is_dir: bool,
    seen_paths: &mut HashSet<PathBuf>,
    directory_paths: &HashSet<PathBuf>,
    file_paths: &HashSet<PathBuf>,
) -> Result<()> {
    if !seen_paths.insert(relative_path.to_path_buf()) {
        return Err(AsterError::validation_error(format!(
            "archive contains duplicate entry path '{}'",
            relative_path.display()
        )));
    }

    if is_dir {
        for ancestor in relative_path.ancestors().skip(1) {
            if ancestor.as_os_str().is_empty() {
                break;
            }
            if file_paths.contains(ancestor) {
                return Err(AsterError::validation_error(format!(
                    "archive directory '{}' is inside file entry '{}'",
                    relative_path.display(),
                    ancestor.display()
                )));
            }
        }
        if file_paths.contains(relative_path) {
            return Err(AsterError::validation_error(format!(
                "archive directory '{}' conflicts with file '{}'",
                relative_path.display(),
                relative_path.display()
            )));
        }
        return Ok(());
    }

    if directory_paths.contains(relative_path) {
        return Err(AsterError::validation_error(format!(
            "archive file '{}' conflicts with a directory of the same path",
            relative_path.display()
        )));
    }
    for ancestor in relative_path.ancestors().skip(1) {
        if ancestor.as_os_str().is_empty() {
            break;
        }
        if file_paths.contains(ancestor) {
            return Err(AsterError::validation_error(format!(
                "archive file '{}' is inside file entry '{}'",
                relative_path.display(),
                ancestor.display()
            )));
        }
    }

    Ok(())
}

pub(crate) fn insert_directory_path_with_limit(
    path: &Path,
    directory_paths: &mut HashSet<PathBuf>,
    limits: ArchiveScanLimits,
) -> Result<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        if let std::path::Component::Normal(name) = component {
            current.push(name);
            if directory_paths.insert(current.clone()) {
                let count = crate::utils::numbers::usize_to_u64(
                    directory_paths.len(),
                    "archive directory count",
                )?;
                if count > limits.max_directories {
                    return Err(AsterError::validation_error(format!(
                        "archive contains {} directories, exceeds server limit {}",
                        count, limits.max_directories
                    )));
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_archive_entry_compression_ratio(
    uncompressed_size: u64,
    compressed_size: u64,
    max_ratio: u64,
    relative_path: &Path,
) -> Result<()> {
    if uncompressed_size == 0 {
        return Ok(());
    }
    if compressed_size == 0 {
        return Err(AsterError::validation_error(format!(
            "archive entry '{}' has zero compressed bytes for {} uncompressed bytes",
            relative_path.display(),
            uncompressed_size
        )));
    }
    if compression_ratio_exceeds(uncompressed_size, compressed_size, max_ratio)? {
        return Err(AsterError::validation_error(format!(
            "archive entry '{}' compression ratio exceeds server limit {}",
            relative_path.display(),
            max_ratio
        )));
    }
    Ok(())
}

pub(crate) fn validate_total_archive_compression_ratio(
    total_uncompressed_bytes: i64,
    total_compressed_bytes: u64,
    max_ratio: u64,
) -> Result<()> {
    if total_uncompressed_bytes <= 0 {
        return Ok(());
    }
    if total_compressed_bytes == 0 {
        return Err(AsterError::validation_error(
            "archive has zero compressed file bytes for non-empty contents",
        ));
    }
    let total_uncompressed_bytes =
        crate::utils::numbers::i64_to_u64(total_uncompressed_bytes, "archive uncompressed size")?;
    if compression_ratio_exceeds(total_uncompressed_bytes, total_compressed_bytes, max_ratio)? {
        return Err(AsterError::validation_error(format!(
            "archive total compression ratio exceeds server limit {}",
            max_ratio
        )));
    }
    Ok(())
}

fn compression_ratio_exceeds(
    uncompressed_size: u64,
    compressed_size: u64,
    max_ratio: u64,
) -> Result<bool> {
    let allowed = u128::from(compressed_size)
        .checked_mul(u128::from(max_ratio))
        .ok_or_else(|| AsterError::internal_error("archive compression ratio overflow"))?;
    Ok(u128::from(uncompressed_size) > allowed)
}

pub(crate) fn normalize_archive_entry_path(
    path: &str,
    name_policy: ArchiveScanNamePolicy,
) -> Result<PathBuf> {
    if path.contains('\0')
        || path.starts_with('/')
        || path.starts_with('\\')
        || has_windows_drive_prefix(path)
    {
        return Err(AsterError::validation_error(format!(
            "archive entry '{}' contains unsafe path",
            path
        )));
    }

    let mut normalized = PathBuf::new();
    for component in path.split(['/', '\\']) {
        match component {
            "" | "." => {}
            ".." => {
                if !normalized.pop() {
                    return Err(AsterError::validation_error(format!(
                        "archive entry '{}' contains unsafe path",
                        path
                    )));
                }
            }
            name => {
                let name = normalize_archive_entry_name(name, name_policy)?;
                normalized.push(name);
            }
        }
    }

    if normalized.as_os_str().is_empty() {
        return Err(AsterError::validation_error(
            "archive entry path cannot be empty",
        ));
    }
    Ok(normalized)
}

fn normalize_archive_entry_name(name: &str, name_policy: ArchiveScanNamePolicy) -> Result<String> {
    match name_policy {
        ArchiveScanNamePolicy::StrictAsterName => Ok(
            aster_forge_validation::filename::normalize_validate_name(name)?,
        ),
        ArchiveScanNamePolicy::PreviewDisplayName => normalize_preview_entry_name(name),
    }
}

fn normalize_preview_entry_name(name: &str) -> Result<String> {
    let normalized = aster_forge_validation::filename::normalize_name(name);
    if normalized.is_empty() {
        return Err(AsterError::validation_error(
            "archive entry path cannot contain empty names",
        ));
    }
    if normalized.chars().any(|c| c.is_ascii_control()) {
        return Err(AsterError::validation_error(
            "archive entry name contains control characters",
        ));
    }
    Ok(normalized)
}

fn has_windows_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

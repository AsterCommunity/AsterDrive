//! ZIP 中央目录扫描与安全校验。

use std::collections::HashSet;
use std::io::{Read, Seek};
use std::path::{Component, Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::errors::{AsterError, MapAsterErr, Result};

const UNIX_FILE_TYPE_MASK: u32 = 0o170000;
const UNIX_REGULAR_FILE_MODE: u32 = 0o100000;
const UNIX_DIRECTORY_MODE: u32 = 0o040000;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ZipScanLimits {
    pub(crate) max_uncompressed_bytes: i64,
    pub(crate) max_entries: u64,
    pub(crate) max_files: u64,
    pub(crate) max_directories: u64,
    pub(crate) max_depth: u64,
    pub(crate) max_path_bytes: u64,
    pub(crate) max_compression_ratio: u64,
    pub(crate) max_entry_compression_ratio: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub(crate) enum ZipScanEntryKind {
    File,
    Directory,
}

impl ZipScanEntryKind {
    pub(crate) fn is_dir(self) -> bool {
        matches!(self, Self::Directory)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ZipScanEntry {
    pub(crate) index: usize,
    pub(crate) relative_path: PathBuf,
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) parent: Option<String>,
    pub(crate) kind: ZipScanEntryKind,
    pub(crate) size: i64,
    pub(crate) compressed_size: i64,
    pub(crate) modified_at: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ZipScanResult {
    pub(crate) entry_count: u64,
    pub(crate) file_count: u64,
    pub(crate) directory_count: u64,
    pub(crate) total_uncompressed_bytes: i64,
    pub(crate) entries: Vec<ZipScanEntry>,
}

pub(crate) fn scan_zip_archive<R, F>(
    archive: &mut zip::ZipArchive<R>,
    limits: ZipScanLimits,
    deadline: Option<Instant>,
    mut ensure_file_size_allowed: F,
) -> Result<ZipScanResult>
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
    let mut seen_paths = HashSet::new();
    let mut directory_paths = HashSet::new();
    let mut file_paths = HashSet::new();
    let mut entries = Vec::with_capacity(archive.len());

    for index in 0..archive.len() {
        ensure_zip_scan_deadline(deadline)?;
        let entry = archive
            .by_index_raw(index)
            .map_aster_err_with(|| AsterError::validation_error("invalid zip archive entry"))?;
        validate_zip_entry_supported(&entry)?;
        let enclosed_path = entry.enclosed_name().ok_or_else(|| {
            AsterError::validation_error(format!(
                "archive entry '{}' contains unsafe path",
                entry.name()
            ))
        })?;
        let relative_path = normalize_archive_entry_path(&enclosed_path)?;
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
                ZipScanEntryKind::Directory,
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
            ZipScanEntryKind::File,
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

    Ok(ZipScanResult {
        entry_count,
        file_count,
        directory_count: directory_paths.len().try_into().map_aster_err_with(|| {
            AsterError::internal_error("directory count exceeds u64 range")
        })?,
        total_uncompressed_bytes,
        entries,
    })
}

pub(crate) fn ensure_zip_scan_deadline(deadline: Option<Instant>) -> Result<()> {
    if let Some(deadline) = deadline
        && Instant::now() > deadline
    {
        return Err(AsterError::validation_error(
            "archive scan exceeded server time limit",
        ));
    }
    Ok(())
}

fn build_scan_entry(
    index: usize,
    relative_path: PathBuf,
    kind: ZipScanEntryKind,
    size: i64,
    compressed_size: i64,
    modified_at: Option<zip::DateTime>,
) -> Result<ZipScanEntry> {
    let path = relative_path.to_string_lossy().to_string();
    let name = relative_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AsterError::validation_error("archive entry name must be valid UTF-8"))?
        .to_string();
    let parent = relative_path.parent().and_then(|parent| {
        (!parent.as_os_str().is_empty()).then(|| parent.to_string_lossy().to_string())
    });

    Ok(ZipScanEntry {
        index,
        relative_path,
        path,
        name,
        parent,
        kind,
        size,
        compressed_size,
        modified_at: modified_at.and_then(format_zip_datetime),
    })
}

fn format_zip_datetime(datetime: zip::DateTime) -> Option<String> {
    datetime.is_valid().then(|| {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            datetime.year(),
            datetime.month(),
            datetime.day(),
            datetime.hour(),
            datetime.minute(),
            datetime.second()
        )
    })
}

fn validate_zip_entry_supported<R: Read>(entry: &zip::read::ZipFile<'_, R>) -> Result<()> {
    if entry.encrypted() {
        return Err(AsterError::validation_error(format!(
            "archive entry '{}' is encrypted; encrypted ZIP entries are not supported",
            entry.name()
        )));
    }
    if entry.is_symlink() {
        return Err(AsterError::validation_error(format!(
            "archive entry '{}' is a symbolic link; symbolic links are not supported",
            entry.name()
        )));
    }
    if let Some(mode) = entry.unix_mode() {
        let file_type = mode & UNIX_FILE_TYPE_MASK;
        if file_type != 0 && file_type != UNIX_REGULAR_FILE_MODE && file_type != UNIX_DIRECTORY_MODE
        {
            return Err(AsterError::validation_error(format!(
                "archive entry '{}' is a special file; only regular files and directories are supported",
                entry.name()
            )));
        }
    }
    if !entry.is_file() && !entry.is_dir() {
        return Err(AsterError::validation_error(format!(
            "archive entry '{}' is not a regular file or directory",
            entry.name()
        )));
    }
    match entry.compression() {
        zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated => Ok(()),
        method => Err(AsterError::validation_error(format!(
            "archive entry '{}' uses unsupported compression method {method:?}",
            entry.name()
        ))),
    }
}

fn validate_archive_entry_path_limits(relative_path: &Path, limits: ZipScanLimits) -> Result<()> {
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

fn ensure_archive_entry_path_not_conflicting(
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
        for file_path in file_paths {
            if file_path == relative_path {
                return Err(AsterError::validation_error(format!(
                    "archive directory '{}' conflicts with file '{}'",
                    relative_path.display(),
                    file_path.display()
                )));
            }
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

fn insert_directory_path_with_limit(
    path: &Path,
    directory_paths: &mut HashSet<PathBuf>,
    limits: ZipScanLimits,
) -> Result<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        if let Component::Normal(name) = component {
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

fn validate_archive_entry_compression_ratio(
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

fn validate_total_archive_compression_ratio(
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

fn normalize_archive_entry_path(path: &Path) -> Result<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(name) => {
                let name = name.to_str().ok_or_else(|| {
                    AsterError::validation_error("archive entry name must be valid UTF-8")
                })?;
                crate::utils::validate_name(name)?;
                normalized.push(name);
            }
            _ => {
                return Err(AsterError::validation_error(format!(
                    "archive entry '{}' contains invalid path component",
                    path.display()
                )));
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

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use super::*;

    fn scan_limits() -> ZipScanLimits {
        ZipScanLimits {
            max_uncompressed_bytes: 1024 * 1024,
            max_entries: 100,
            max_files: 100,
            max_directories: 100,
            max_depth: 16,
            max_path_bytes: 4096,
            max_compression_ratio: 100,
            max_entry_compression_ratio: 100,
        }
    }

    fn create_stored_zip_bytes(entries: &[(&str, Option<&[u8]>)]) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        for (path, content) in entries {
            match content {
                Some(bytes) => {
                    zip.start_file(*path, options)
                        .expect("zip entry should start");
                    zip.write_all(bytes).expect("zip entry should be writable");
                }
                None => {
                    zip.add_directory(*path, options)
                        .expect("zip directory should be writable");
                }
            }
        }

        zip.finish().expect("zip writer should finish").into_inner()
    }

    fn scan_error_for(entries: &[(&str, Option<&[u8]>)]) -> String {
        let bytes = create_stored_zip_bytes(entries);
        let cursor = Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor).expect("zip should open");

        scan_zip_archive(&mut archive, scan_limits(), None, |_| Ok(()))
            .expect_err("scan should reject archive")
            .message()
            .to_string()
    }

    #[test]
    fn scan_allows_explicit_parent_directory_after_child_file() {
        let bytes = create_stored_zip_bytes(&[
            ("prefix/child.txt", Some(b"payload".as_slice())),
            ("prefix/", None),
        ]);
        let cursor = Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor).expect("zip should open");

        let result = scan_zip_archive(&mut archive, scan_limits(), None, |_| Ok(()))
            .expect("parent directory after child file should be valid");

        assert_eq!(result.file_count, 1);
        assert_eq!(result.directory_count, 1);
        assert_eq!(result.entries.len(), 2);
    }

    #[test]
    fn scan_rejects_duplicate_paths_and_file_ancestors() {
        let duplicate = scan_error_for(&[
            ("dup/", None),
            ("dup", Some(b"same-normalized-path".as_slice())),
        ]);
        assert!(duplicate.contains("duplicate entry path 'dup'"));

        let child_file = scan_error_for(&[
            ("prefix", Some(b"not-a-directory".as_slice())),
            ("prefix/child.txt", Some(b"child".as_slice())),
        ]);
        assert!(
            child_file.contains("archive file 'prefix/child.txt' is inside file entry 'prefix'")
        );

        let child_directory = scan_error_for(&[
            ("prefix", Some(b"not-a-directory".as_slice())),
            ("prefix/child/", None),
        ]);
        assert!(
            child_directory
                .contains("archive directory 'prefix/child' is inside file entry 'prefix'")
        );
    }

    #[test]
    fn scan_rejects_implicit_directory_limit_overflow() {
        let bytes = create_stored_zip_bytes(&[("a/b/c.txt", Some(b"nested".as_slice()))]);
        let cursor = Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor).expect("zip should open");
        let mut limits = scan_limits();
        limits.max_directories = 1;

        let error = scan_zip_archive(&mut archive, limits, None, |_| Ok(()))
            .expect_err("implicit directories should count toward directory limit");

        assert!(
            error
                .message()
                .contains("directories, exceeds server limit 1")
        );
    }

    #[test]
    fn scan_deadline_rejects_expired_deadline() {
        let error =
            ensure_zip_scan_deadline(Some(Instant::now() - std::time::Duration::from_secs(1)))
                .expect_err("expired deadline should reject scan");

        assert_eq!(error.message(), "archive scan exceeded server time limit");
    }
}

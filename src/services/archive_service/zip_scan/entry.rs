use std::io::Read;
use std::path::PathBuf;

use crate::errors::{AsterError, Result};

use super::super::scan::{ArchiveScanEntry, ArchiveScanEntryKind};

const UNIX_FILE_TYPE_MASK: u32 = 0o170000;
const UNIX_REGULAR_FILE_MODE: u32 = 0o100000;
const UNIX_DIRECTORY_MODE: u32 = 0o040000;

pub(super) fn map_zip_entry_error(error: zip::result::ZipError) -> AsterError {
    if let zip::result::ZipError::Io(io_error) = error
        && let Some(source) = io_error
            .get_ref()
            .and_then(|source| source.downcast_ref::<AsterError>())
    {
        return source.clone();
    }

    AsterError::validation_error("invalid zip archive entry")
}

pub(super) fn build_scan_entry(
    index: usize,
    relative_path: PathBuf,
    kind: ArchiveScanEntryKind,
    size: i64,
    compressed_size: i64,
    modified_at: Option<zip::DateTime>,
) -> Result<ArchiveScanEntry> {
    build_scan_entry_from_parts(
        index,
        relative_path,
        kind,
        size,
        compressed_size,
        modified_at.and_then(format_zip_datetime),
    )
}

pub(super) fn build_scan_entry_from_parts(
    index: usize,
    relative_path: PathBuf,
    kind: ArchiveScanEntryKind,
    size: i64,
    compressed_size: i64,
    modified_at: Option<String>,
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
        modified_at,
    })
}

pub(super) fn format_zip_datetime(datetime: zip::DateTime) -> Option<String> {
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

pub(super) fn validate_zip_entry_supported<R: Read>(
    entry: &zip::read::ZipFile<'_, R>,
    entry_name: &str,
) -> Result<()> {
    if entry.encrypted() {
        return Err(AsterError::validation_error(format!(
            "archive entry '{}' is encrypted; encrypted ZIP entries are not supported",
            entry_name
        )));
    }
    if entry.is_symlink() {
        return Err(AsterError::validation_error(format!(
            "archive entry '{}' is a symbolic link; symbolic links are not supported",
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
    if !entry.is_file() && !entry.is_dir() {
        return Err(AsterError::validation_error(format!(
            "archive entry '{}' is not a regular file or directory",
            entry_name
        )));
    }
    match entry.compression() {
        zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated => Ok(()),
        method => Err(AsterError::validation_error(format!(
            "archive entry '{}' uses unsupported compression method {method:?}",
            entry_name
        ))),
    }
}

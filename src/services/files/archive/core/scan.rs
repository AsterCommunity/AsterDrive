use std::path::PathBuf;
use std::time::Instant;

use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::errors::{AsterError, Result};

#[derive(Debug, Clone, Copy)]
pub(crate) struct ArchiveScanLimits {
    pub(crate) max_uncompressed_bytes: i64,
    pub(crate) max_entries: u64,
    pub(crate) max_files: u64,
    pub(crate) max_directories: u64,
    pub(crate) max_depth: u64,
    pub(crate) max_path_bytes: u64,
    pub(crate) max_compression_ratio: u64,
    pub(crate) max_entry_compression_ratio: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArchiveScanNamePolicy {
    StrictAsterName,
    PreviewDisplayName,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub(crate) enum ArchiveScanEntryKind {
    File,
    Directory,
}

impl ArchiveScanEntryKind {
    pub(crate) fn is_dir(self) -> bool {
        matches!(self, Self::Directory)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ArchiveScanEntry {
    pub(crate) index: usize,
    pub(crate) relative_path: PathBuf,
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) parent: Option<String>,
    pub(crate) kind: ArchiveScanEntryKind,
    pub(crate) size: i64,
    pub(crate) compressed_size: i64,
    pub(crate) modified_at: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ArchiveScanResult {
    pub(crate) entry_count: u64,
    pub(crate) file_count: u64,
    pub(crate) directory_count: u64,
    pub(crate) total_uncompressed_bytes: i64,
    pub(crate) extract_compatible: bool,
    pub(crate) entries: Vec<ArchiveScanEntry>,
}

#[derive(Debug, Clone)]
pub(crate) struct ArchiveRawScanEntry {
    pub(crate) index: usize,
    pub(crate) raw_name: Vec<u8>,
    pub(crate) display_name: String,
    pub(crate) raw_name_utf8: bool,
    pub(crate) kind: ArchiveScanEntryKind,
    pub(crate) size: i64,
    pub(crate) compressed_size: i64,
    pub(crate) modified_at: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ArchiveRawScanResult {
    pub(crate) entry_count: u64,
    pub(crate) file_count: u64,
    pub(crate) directory_count: u64,
    pub(crate) total_uncompressed_bytes: i64,
    pub(crate) total_compressed_base: u64,
    pub(crate) entries: Vec<ArchiveRawScanEntry>,
}

pub(crate) fn ensure_archive_scan_deadline(deadline: Option<Instant>) -> Result<()> {
    if let Some(deadline) = deadline
        && Instant::now() > deadline
    {
        return Err(AsterError::validation_error(
            "archive scan exceeded server time limit",
        ));
    }
    Ok(())
}

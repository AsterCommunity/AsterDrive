use std::path::PathBuf;

use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ZipScanNamePolicy {
    StrictAsterName,
    PreviewDisplayName,
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
    pub(crate) extract_compatible: bool,
    pub(crate) entries: Vec<ZipScanEntry>,
}

#[derive(Debug, Clone)]
pub(crate) struct ZipRawScanEntry {
    pub(crate) index: usize,
    pub(crate) raw_name: Vec<u8>,
    pub(crate) display_name: String,
    pub(crate) zip_utf8: bool,
    pub(crate) kind: ZipScanEntryKind,
    pub(crate) size: i64,
    pub(crate) compressed_size: i64,
    pub(crate) modified_at: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ZipRawScanResult {
    pub(crate) entry_count: u64,
    pub(crate) file_count: u64,
    pub(crate) directory_count: u64,
    pub(crate) total_uncompressed_bytes: i64,
    pub(crate) entries: Vec<ZipRawScanEntry>,
}

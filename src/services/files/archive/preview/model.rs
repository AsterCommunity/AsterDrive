use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Read-only preview manifest of an archive file, served to clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ArchivePreviewManifest {
    /// Preview manifest schema version; bumped when the shape changes.
    pub schema_version: u32,
    /// Archive format identifier the manifest was produced with, e.g. `zip`.
    pub format: String,
    /// Blob id of the archive the manifest was scanned from.
    pub source_blob_id: i64,
    /// Content hash of the source blob; part of the cache identity.
    pub source_hash: String,
    /// RFC 3339 timestamp of when the scan was performed.
    pub generated_at: String,
    /// Total number of entries in the archive; may exceed `entries.len()`
    /// when the listing was truncated.
    pub entry_count: i64,
    /// Number of file entries in the archive.
    pub file_count: i64,
    /// Number of directory entries in the archive.
    pub directory_count: i64,
    /// Sum of uncompressed entry sizes in bytes; keeps the larger of the
    /// stored and re-scanned values.
    pub total_uncompressed_size: i64,
    /// True when the scan hit the entry limit and `entries` is incomplete.
    pub truncated: bool,
    /// Whether the archive is considered safe to extract server-side.
    pub extract_compatibility: ArchivePreviewExtractCompatibility,
    /// Preview entries actually listed, capped by the preview limits.
    pub entries: Vec<ArchivePreviewEntry>,
}

/// Single listed entry of an archive preview.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ArchivePreviewEntry {
    /// Display path of the entry inside the archive, root-relative.
    pub path: String,
    /// Final path component of the entry.
    pub name: String,
    /// Path of the parent directory; empty at the archive root.
    pub parent: Option<String>,
    /// Whether this entry is a file or a directory.
    pub kind: ArchivePreviewEntryKind,
    /// Uncompressed size in bytes.
    pub size: i64,
    /// Compressed size in bytes as stored in the archive.
    pub compressed_size: i64,
    /// Entry modification timestamp from the archive metadata, if present.
    pub modified_at: Option<String>,
}

/// Kind of an archive preview entry.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum ArchivePreviewEntryKind {
    /// Regular file entry.
    File,
    /// Directory entry.
    Directory,
}

/// Whether server-side extraction of the archive is allowed.
///
/// Extraction is rejected when the scan found entry names that the storage
/// layer cannot safely represent, or when the preview listing was truncated
/// so not all entry names were verified.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ArchivePreviewExtractCompatibility {
    /// True when the archive passed all extract safety checks.
    pub supported: bool,
    /// Reason extraction is unsupported; always `None` when `supported`.
    pub reason: Option<ArchivePreviewExtractUnsupportedReason>,
}

impl ArchivePreviewExtractCompatibility {
    /// Compatibility verdict for an archive that passed all safety checks.
    pub(super) fn supported() -> Self {
        Self {
            supported: true,
            reason: None,
        }
    }

    pub(super) fn unsupported(reason: ArchivePreviewExtractUnsupportedReason) -> Self {
        Self {
            supported: false,
            reason: Some(reason),
        }
    }

    pub(super) fn from_scan_extract_compatible(extract_compatible: bool) -> Self {
        if extract_compatible {
            Self::supported()
        } else {
            Self::unsupported(ArchivePreviewExtractUnsupportedReason::UnsupportedEntryNames)
        }
    }
}

/// Why an archive was rejected for server-side extraction.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum ArchivePreviewExtractUnsupportedReason {
    /// At least one entry name cannot be mapped to a safe storage path, or
    /// the preview listing was truncated so names were not fully verified.
    UnsupportedEntryNames,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct CachedArchiveRawManifest {
    pub(super) schema_version: u32,
    pub(super) source_blob_id: i64,
    pub(super) source_hash: String,
    pub(super) limit_signature: String,
    pub(super) manifest: ArchiveRawManifest,
}

#[derive(Debug, Serialize)]
pub(super) struct CachedArchiveRawManifestRef<'a> {
    pub(super) schema_version: u32,
    pub(super) source_blob_id: i64,
    pub(super) source_hash: &'a str,
    pub(super) limit_signature: &'a str,
    pub(super) manifest: &'a ArchiveRawManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ArchiveRawManifest {
    pub(super) schema_version: u32,
    pub(super) format: String,
    pub(super) source_blob_id: i64,
    pub(super) source_hash: String,
    pub(super) generated_at: String,
    pub(crate) entry_count: i64,
    pub(crate) file_count: i64,
    pub(crate) directory_count: i64,
    pub(super) total_uncompressed_size: i64,
    #[serde(default)]
    pub(super) total_compressed_base: u64,
    pub(super) entries: Vec<ArchiveRawEntry>,
}

impl ArchiveRawManifest {
    pub(crate) fn entries_truncated(&self) -> bool {
        i64::try_from(self.entries.len()).is_ok_and(|entry_len| entry_len < self.entry_count)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ArchiveRawEntry {
    pub(super) index: usize,
    pub(super) raw_name: String,
    pub(super) display_name: String,
    #[serde(default, alias = "zip_utf8")]
    pub(super) raw_name_utf8: bool,
    pub(super) kind: ArchivePreviewEntryKind,
    pub(super) size: i64,
    pub(super) compressed_size: i64,
    pub(super) modified_at: Option<String>,
}

use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ArchivePreviewManifest {
    pub schema_version: u32,
    pub format: String,
    pub source_blob_id: i64,
    pub source_hash: String,
    pub generated_at: String,
    pub entry_count: i64,
    pub file_count: i64,
    pub directory_count: i64,
    pub total_uncompressed_size: i64,
    pub truncated: bool,
    pub extract_compatibility: ArchivePreviewExtractCompatibility,
    pub entries: Vec<ArchivePreviewEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ArchivePreviewEntry {
    pub path: String,
    pub name: String,
    pub parent: Option<String>,
    pub kind: ArchivePreviewEntryKind,
    pub size: i64,
    pub compressed_size: i64,
    pub modified_at: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum ArchivePreviewEntryKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ArchivePreviewExtractCompatibility {
    pub supported: bool,
    pub reason: Option<ArchivePreviewExtractUnsupportedReason>,
}

impl ArchivePreviewExtractCompatibility {
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum ArchivePreviewExtractUnsupportedReason {
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

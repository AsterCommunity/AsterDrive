use crate::entities::file;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArchiveFormat {
    Zip,
    SevenZip,
}

impl ArchiveFormat {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::SevenZip => "7z",
        }
    }

    pub(crate) const fn raw_manifest_cache_name(self) -> &'static str {
        match self {
            Self::Zip => "zip_raw_manifest.v1",
            Self::SevenZip => "7z_raw_manifest.v1",
        }
    }

    pub(crate) const fn temp_file_name(self) -> &'static str {
        match self {
            Self::Zip => "source.zip",
            Self::SevenZip => "source.7z",
        }
    }

    pub(crate) fn strip_extension(self, name: &str) -> Option<&str> {
        let extension = match self {
            Self::Zip => ".zip",
            Self::SevenZip => ".7z",
        };
        if ends_with_ignore_ascii_case(name, extension) && name.len() > extension.len() {
            Some(&name[..name.len() - extension.len()])
        } else {
            None
        }
    }
}

pub(crate) fn detect_archive_extract_format(source_file: &file::Model) -> Option<ArchiveFormat> {
    // Extraction intentionally trusts only the stored filename extension; client-supplied MIME
    // types are easier to spoof and should not widen the executable archive surface.
    if ends_with_ignore_ascii_case(&source_file.name, ".zip") {
        return Some(ArchiveFormat::Zip);
    }
    if ends_with_ignore_ascii_case(&source_file.name, ".7z") {
        return Some(ArchiveFormat::SevenZip);
    }
    None
}

pub(crate) fn detect_archive_preview_format(source_file: &file::Model) -> Option<ArchiveFormat> {
    let mime = source_file.mime_type.to_ascii_lowercase();
    if ends_with_ignore_ascii_case(&source_file.name, ".zip") {
        return Some(ArchiveFormat::Zip);
    }
    if ends_with_ignore_ascii_case(&source_file.name, ".7z") {
        return Some(ArchiveFormat::SevenZip);
    }
    if matches!(
        mime.as_str(),
        "application/zip" | "application/x-zip-compressed"
    ) {
        return Some(ArchiveFormat::Zip);
    }
    if matches!(
        mime.as_str(),
        "application/x-7z" | "application/x-7z-compressed"
    ) {
        return Some(ArchiveFormat::SevenZip);
    }
    None
}

fn ends_with_ignore_ascii_case(value: &str, suffix: &str) -> bool {
    value
        .get(value.len().saturating_sub(suffix.len())..)
        .is_some_and(|tail| tail.eq_ignore_ascii_case(suffix))
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    fn source_file(name: &str, mime_type: &str) -> file::Model {
        file::Model {
            id: 1,
            name: name.to_string(),
            folder_id: None,
            team_id: None,
            blob_id: 1,
            size: 1,
            owner_user_id: Some(1),
            created_by_user_id: Some(1),
            created_by_username: "tester".to_string(),
            mime_type: mime_type.to_string(),
            extension: String::new(),
            compound_extension: None,
            file_category: crate::types::FileCategory::Archive,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deleted_at: None,
            is_locked: false,
        }
    }

    #[test]
    fn detect_archive_preview_format_prefers_extension_over_mime() {
        assert_eq!(
            detect_archive_preview_format(&source_file("bundle.7z", "application/zip")),
            Some(ArchiveFormat::SevenZip)
        );
        assert_eq!(
            detect_archive_preview_format(&source_file(
                "bundle.zip",
                "application/x-7z-compressed"
            )),
            Some(ArchiveFormat::Zip)
        );
    }
}

use crate::entities::file;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArchiveFormat {
    Zip,
}

impl ArchiveFormat {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Zip => "zip",
        }
    }

    pub(crate) const fn raw_manifest_cache_name(self) -> &'static str {
        match self {
            Self::Zip => "zip_raw_manifest.v2",
        }
    }

    pub(crate) const fn temp_file_name(self) -> &'static str {
        match self {
            Self::Zip => "source.zip",
        }
    }

    pub(crate) fn strip_extension(self, name: &str) -> Option<&str> {
        let extension = match self {
            Self::Zip => ".zip",
        };
        if ends_with_ignore_ascii_case(name, extension) && name.len() > extension.len() {
            Some(&name[..name.len() - extension.len()])
        } else {
            None
        }
    }
}

pub(crate) fn detect_supported_archive_format(source_file: &file::Model) -> Option<ArchiveFormat> {
    // Archive actions intentionally trust only the stored filename extension; client-supplied MIME
    // types are easier to spoof and should not widen the executable archive surface.
    if ends_with_ignore_ascii_case(&source_file.name, ".zip") {
        return Some(ArchiveFormat::Zip);
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
    fn archive_format_detection_uses_same_extension_only_rules_for_preview_and_extract() {
        let zip_by_name = source_file("bundle.zip", "application/x-7z-compressed");
        let zip_by_mime_only = source_file("bundle.bin", "application/zip");
        let seven_zip_with_zip_mime = source_file("bundle.7z", "application/zip");

        assert_eq!(
            detect_supported_archive_format(&zip_by_name),
            Some(ArchiveFormat::Zip)
        );
        assert_eq!(detect_supported_archive_format(&zip_by_mime_only), None);
        assert_eq!(
            detect_supported_archive_format(&seven_zip_with_zip_mime),
            None
        );
    }
}

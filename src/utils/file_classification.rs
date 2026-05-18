//! File extension parsing and category classification.

use std::str::FromStr;

use crate::errors::{AsterError, Result};
use crate::types::FileCategory;

pub const MAX_EXTENSION_LEN: usize = 32;
pub const MAX_EXTENSION_FILTERS: usize = 32;

const COMPOUND_EXTENSIONS: &[&str] = &[
    "tar.gz", "tar.bz2", "tar.xz", "tar.zst", "tar.br", "tar.lz", "tar.lzma", "tar.lzo",
];

const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff", "svg", "ico", "avif", "heic",
    "heif", "raw", "cr2", "nef", "orf", "rw2",
];

const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mov", "avi", "mkv", "webm", "flv", "wmv", "mpeg", "mpg", "3gp", "ts", "m2ts",
    "ogv",
];

const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "wav", "flac", "aac", "m4a", "ogg", "oga", "opus", "wma", "aiff", "alac", "mid", "midi",
];

const DOCUMENT_EXTENSIONS: &[&str] = &[
    "pdf", "txt", "md", "markdown", "rtf", "doc", "docx", "odt", "pages", "epub", "tex",
];

const SPREADSHEET_EXTENSIONS: &[&str] = &["xls", "xlsx", "ods", "csv", "tsv", "numbers"];

const PRESENTATION_EXTENSIONS: &[&str] = &["ppt", "pptx", "odp", "key"];

const ARCHIVE_EXTENSIONS: &[&str] = &[
    "zip", "rar", "7z", "tar", "gz", "bz2", "xz", "zst", "br", "tgz", "tbz", "tbz2", "txz", "lz",
    "lzma", "lzo", "cab", "iso", "dmg",
];

const CODE_EXTENSIONS: &[&str] = &[
    "rs",
    "ts",
    "tsx",
    "js",
    "jsx",
    "mjs",
    "cjs",
    "json",
    "jsonc",
    "yaml",
    "yml",
    "toml",
    "xml",
    "html",
    "htm",
    "css",
    "scss",
    "sass",
    "less",
    "sql",
    "sh",
    "bash",
    "zsh",
    "fish",
    "ps1",
    "py",
    "rb",
    "go",
    "java",
    "kt",
    "kts",
    "swift",
    "c",
    "h",
    "cpp",
    "cc",
    "cxx",
    "hpp",
    "cs",
    "php",
    "lua",
    "dart",
    "vue",
    "svelte",
    "lock",
    "ini",
    "conf",
    "dockerfile",
    "makefile",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileClassification {
    pub extension: String,
    pub compound_extension: Option<String>,
    pub category: FileCategory,
}

pub fn classify_file(name: &str, mime_type: &str) -> FileClassification {
    let extension = extension_from_name(name).unwrap_or_default();
    let compound_extension = compound_extension_from_name(name);
    let category =
        classify_extension_and_mime(&extension, compound_extension.as_deref(), mime_type);

    FileClassification {
        extension,
        compound_extension,
        category,
    }
}

pub fn normalize_extension_filter(raw: &str) -> Result<String> {
    let normalized = raw.trim().trim_start_matches('.').to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(AsterError::validation_error(
            "extensions must not contain empty values",
        ));
    }
    if normalized.len() > MAX_EXTENSION_LEN {
        return Err(AsterError::validation_error(format!(
            "extensions must be at most {MAX_EXTENSION_LEN} characters"
        )));
    }
    if normalized.starts_with('.')
        || normalized.ends_with('.')
        || normalized.contains("..")
        || !normalized.chars().all(|ch| {
            ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-' || ch == '+'
        })
    {
        return Err(AsterError::validation_error(
            "extensions may only contain letters, numbers, dot, underscore, plus, or hyphen",
        ));
    }

    Ok(normalized)
}

pub fn parse_extension_filters(raw: &str) -> Result<Vec<String>> {
    let mut extensions = Vec::new();
    for part in raw.split(',') {
        let extension = normalize_extension_filter(part)?;
        if !extensions.iter().any(|candidate| candidate == &extension) {
            extensions.push(extension);
        }
        if extensions.len() > MAX_EXTENSION_FILTERS {
            return Err(AsterError::validation_error(format!(
                "extensions supports at most {MAX_EXTENSION_FILTERS} values"
            )));
        }
    }

    Ok(extensions)
}

pub fn parse_file_category(raw: &str) -> Result<FileCategory> {
    FileCategory::from_str(raw.trim()).map_err(|()| {
        AsterError::validation_error(
            "category must be one of: image, video, audio, document, spreadsheet, presentation, archive, code, other",
        )
    })
}

pub fn extension_from_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    let dot = trimmed.rfind('.')?;
    if dot == 0 || dot + 1 >= trimmed.len() {
        return None;
    }
    let extension = &trimmed[dot + 1..];
    if extension.is_empty() {
        return None;
    }
    Some(extension.to_ascii_lowercase())
}

pub fn compound_extension_from_name(name: &str) -> Option<String> {
    let normalized = name.trim().to_ascii_lowercase();
    COMPOUND_EXTENSIONS
        .iter()
        .find(|extension| normalized.ends_with(&format!(".{extension}")))
        .map(|extension| (*extension).to_string())
}

fn classify_extension_and_mime(
    extension: &str,
    compound_extension: Option<&str>,
    mime_type: &str,
) -> FileCategory {
    if compound_extension.is_some() || contains(ARCHIVE_EXTENSIONS, extension) {
        return FileCategory::Archive;
    }
    if contains(SPREADSHEET_EXTENSIONS, extension) {
        return FileCategory::Spreadsheet;
    }
    if contains(PRESENTATION_EXTENSIONS, extension) {
        return FileCategory::Presentation;
    }
    if contains(IMAGE_EXTENSIONS, extension) {
        return FileCategory::Image;
    }
    if contains(VIDEO_EXTENSIONS, extension) {
        return FileCategory::Video;
    }
    if contains(AUDIO_EXTENSIONS, extension) {
        return FileCategory::Audio;
    }
    if contains(DOCUMENT_EXTENSIONS, extension) {
        return FileCategory::Document;
    }
    if contains(CODE_EXTENSIONS, extension) {
        return FileCategory::Code;
    }

    classify_mime(mime_type)
}

fn classify_mime(mime_type: &str) -> FileCategory {
    let mime = mime_type.trim().to_ascii_lowercase();
    if mime.starts_with("image/") {
        FileCategory::Image
    } else if mime.starts_with("video/") {
        FileCategory::Video
    } else if mime.starts_with("audio/") {
        FileCategory::Audio
    } else if mime == "application/pdf" || mime.starts_with("text/") {
        FileCategory::Document
    } else if mime.contains("spreadsheet") || mime.contains("excel") || mime.ends_with("/csv") {
        FileCategory::Spreadsheet
    } else if mime.contains("presentation") || mime.contains("powerpoint") {
        FileCategory::Presentation
    } else if mime.contains("zip")
        || mime.contains("compressed")
        || mime.contains("x-tar")
        || mime.contains("x-7z")
        || mime.contains("x-rar")
    {
        FileCategory::Archive
    } else if mime.contains("json") || mime.contains("xml") {
        FileCategory::Code
    } else {
        FileCategory::Other
    }
}

const fn contains(values: &[&str], needle: &str) -> bool {
    let mut index = 0;
    while index < values.len() {
        if values[index].len() == needle.len() {
            let a = values[index].as_bytes();
            let b = needle.as_bytes();
            let mut byte_index = 0;
            let mut equal = true;
            while byte_index < a.len() {
                if a[byte_index] != b[byte_index] {
                    equal = false;
                    break;
                }
                byte_index += 1;
            }
            if equal {
                return true;
            }
        }
        index += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_extensions_and_compound_extensions() {
        assert_eq!(extension_from_name("backup.tar.gz").as_deref(), Some("gz"));
        assert_eq!(
            compound_extension_from_name("backup.TAR.GZ").as_deref(),
            Some("tar.gz")
        );
        assert_eq!(extension_from_name(".gitignore"), None);
        assert_eq!(extension_from_name("README"), None);
    }

    #[test]
    fn classifies_with_fixed_priority() {
        let csv = classify_file("data.csv", "text/csv");
        assert_eq!(csv.category, FileCategory::Spreadsheet);

        let markdown = classify_file("README.md", "text/markdown");
        assert_eq!(markdown.category, FileCategory::Document);

        let archive = classify_file("backup.tar.gz", "application/gzip");
        assert_eq!(archive.category, FileCategory::Archive);
        assert_eq!(archive.compound_extension.as_deref(), Some("tar.gz"));
    }

    #[test]
    fn normalizes_extension_filters() {
        assert_eq!(
            parse_extension_filters(" .PDF,docx,pdf ").unwrap(),
            vec!["pdf", "docx"]
        );
        assert!(parse_extension_filters("pdf,,docx").is_err());
        assert!(parse_extension_filters("../pdf").is_err());
    }
}

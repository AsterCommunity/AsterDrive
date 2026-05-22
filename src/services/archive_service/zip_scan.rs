//! ZIP 中央目录扫描与安全校验。

use std::collections::HashSet;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::time::Instant;

use encoding_rs::{BIG5, EUC_KR, Encoding, GB18030, SHIFT_JIS, WINDOWS_1252};
use oem_cp::{
    code_table::{DECODING_TABLE_CP437, DECODING_TABLE_CP850},
    decode_string_complete_table,
};
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;
use zip::HasZipMetadata;

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::types::ArchiveFilenameEncoding;

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
    filename_encoding: ArchiveFilenameEncoding,
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
        let entry = archive.by_index_raw(index).map_err(map_zip_entry_error)?;
        let decoded_name = decode_zip_entry_name(&entry, filename_encoding)?;
        validate_zip_entry_supported(&entry, &decoded_name)?;
        let relative_path = normalize_archive_entry_path(&decoded_name)?;
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

fn map_zip_entry_error(error: zip::result::ZipError) -> AsterError {
    if let zip::result::ZipError::Io(io_error) = error
        && let Some(source) = io_error
            .get_ref()
            .and_then(|source| source.downcast_ref::<AsterError>())
    {
        return source.clone();
    }

    AsterError::validation_error("invalid zip archive entry")
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

fn validate_zip_entry_supported<R: Read>(
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

fn decode_zip_entry_name<R: Read>(
    entry: &zip::read::ZipFile<'_, R>,
    filename_encoding: ArchiveFilenameEncoding,
) -> Result<String> {
    let raw = entry.name_raw();
    match filename_encoding {
        ArchiveFilenameEncoding::Auto => decode_zip_entry_name_auto(entry),
        ArchiveFilenameEncoding::Utf8 => decode_zip_entry_name_utf8(raw, entry.name()),
        ArchiveFilenameEncoding::Gb18030 => decode_gb18030(raw).ok_or_else(|| {
            AsterError::validation_error(format!(
                "archive entry '{}' filename is not valid GB18030",
                entry.name()
            ))
        }),
        ArchiveFilenameEncoding::Cp437 => Ok(decode_string_complete_table(
            raw,
            &DECODING_TABLE_CP437,
        )),
        ArchiveFilenameEncoding::Cp850 => Ok(decode_string_complete_table(
            raw,
            &DECODING_TABLE_CP850,
        )),
        ArchiveFilenameEncoding::ShiftJis => decode_zip_entry_name_legacy_encoding(
            raw,
            entry.name(),
            SHIFT_JIS,
            "Shift_JIS",
        ),
        ArchiveFilenameEncoding::Big5 => {
            decode_zip_entry_name_legacy_encoding(raw, entry.name(), BIG5, "Big5")
        }
        ArchiveFilenameEncoding::EucKr => {
            decode_zip_entry_name_legacy_encoding(raw, entry.name(), EUC_KR, "EUC-KR")
        }
        ArchiveFilenameEncoding::Windows1252 => decode_zip_entry_name_legacy_encoding(
            raw,
            entry.name(),
            WINDOWS_1252,
            "Windows-1252",
        ),
    }
}

fn decode_zip_entry_name_auto<R: Read + ?Sized>(
    entry: &zip::read::ZipFile<'_, R>,
) -> Result<String> {
    let raw = entry.name_raw();
    if entry.get_metadata().is_utf8 {
        return decode_zip_entry_name_utf8(raw, entry.name());
    }

    if let Ok(name) = std::str::from_utf8(raw) {
        return Ok(name.to_string());
    }

    if raw.iter().any(|byte| *byte >= 0x80)
        && let Some(name) = decode_gb18030(raw)
        && contains_gb18030_cjk_signal(&name)
    {
        return Ok(name);
    }

    Ok(entry.name().to_string())
}

fn decode_zip_entry_name_utf8(raw: &[u8], display_name: &str) -> Result<String> {
    std::str::from_utf8(raw)
        .map(|value| value.to_string())
        .map_err(|_| {
            AsterError::validation_error(format!(
                "archive entry '{}' filename is not valid UTF-8",
                display_name
            ))
        })
}

fn decode_gb18030(raw: &[u8]) -> Option<String> {
    GB18030
        .decode_without_bom_handling_and_without_replacement(raw)
        .map(|value| value.into_owned())
}

fn decode_zip_entry_name_legacy_encoding(
    raw: &[u8],
    display_name: &str,
    encoding: &'static Encoding,
    encoding_label: &str,
) -> Result<String> {
    encoding
        .decode_without_bom_handling_and_without_replacement(raw)
        .map(|value| value.into_owned())
        .ok_or_else(|| {
            AsterError::validation_error(format!(
                "archive entry '{}' filename is not valid {}",
                display_name, encoding_label
            ))
        })
}

fn contains_gb18030_cjk_signal(value: &str) -> bool {
    value.chars().any(|ch| {
        matches!(
            ch,
            '\u{2e80}'..='\u{2eff}'
                | '\u{3000}'..='\u{303f}'
                | '\u{3400}'..='\u{4dbf}'
                | '\u{4e00}'..='\u{9fff}'
                | '\u{f900}'..='\u{faff}'
                | '\u{ff00}'..='\u{ffef}'
        )
    })
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

fn insert_directory_path_with_limit(
    path: &Path,
    directory_paths: &mut HashSet<PathBuf>,
    limits: ZipScanLimits,
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

fn normalize_archive_entry_path(path: &str) -> Result<PathBuf> {
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
                let name = crate::utils::normalize_validate_name(name)?;
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

fn has_windows_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use encoding_rs::{BIG5, EUC_KR, SHIFT_JIS, WINDOWS_1252};
    use oem_cp::{code_table::ENCODING_TABLE_CP850, encode_string_checked};

    use super::*;
    use zip::HasZipMetadata;

    const ZIP_UTF8_NAME_FLAG: u16 = 0x0800;
    const ZIP_UNICODE_PATH_EXTRA_FIELD: u16 = 0x7075;

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

    fn create_stored_zip_bytes_with_raw_name(
        decoded_name: &str,
        raw_name: &[u8],
        content: &[u8],
    ) -> Vec<u8> {
        assert_eq!(
            decoded_name.len(),
            raw_name.len(),
            "test helper patches names in place and requires equal byte lengths"
        );
        let mut bytes = create_stored_zip_bytes(&[(decoded_name, Some(content))]);
        patch_zip_entry_raw_name(&mut bytes, decoded_name.as_bytes(), raw_name, false);
        bytes
    }

    fn create_stored_zip_bytes_with_raw_name_and_utf8_flag(
        decoded_name: &str,
        raw_name: &[u8],
        content: &[u8],
    ) -> Vec<u8> {
        assert_eq!(
            decoded_name.len(),
            raw_name.len(),
            "test helper patches names in place and requires equal byte lengths"
        );
        let mut bytes = create_stored_zip_bytes(&[(decoded_name, Some(content))]);
        patch_zip_entry_raw_name(&mut bytes, decoded_name.as_bytes(), raw_name, true);
        bytes
    }

    fn create_stored_zip_bytes_with_variable_raw_name(raw_name: &[u8], content: &[u8]) -> Vec<u8> {
        let content_crc = crc32(content);
        let compressed_size: u32 = content.len().try_into().expect("test content fits u32");
        let uncompressed_size = compressed_size;
        let name_len: u16 = raw_name.len().try_into().expect("test filename fits u16");

        let mut bytes = Vec::new();
        push_u32(&mut bytes, 0x0403_4b50);
        push_u16(&mut bytes, 10);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u32(&mut bytes, content_crc);
        push_u32(&mut bytes, compressed_size);
        push_u32(&mut bytes, uncompressed_size);
        push_u16(&mut bytes, name_len);
        push_u16(&mut bytes, 0);
        bytes.extend_from_slice(raw_name);
        bytes.extend_from_slice(content);

        let central_directory_offset: u32 = bytes
            .len()
            .try_into()
            .expect("test central directory offset fits u32");
        push_u32(&mut bytes, 0x0201_4b50);
        push_u16(&mut bytes, 20);
        push_u16(&mut bytes, 10);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u32(&mut bytes, content_crc);
        push_u32(&mut bytes, compressed_size);
        push_u32(&mut bytes, uncompressed_size);
        push_u16(&mut bytes, name_len);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, 0);
        bytes.extend_from_slice(raw_name);

        let central_directory_size: u32 = (bytes.len()
            - usize::try_from(central_directory_offset)
                .expect("test central directory offset fits usize"))
        .try_into()
        .expect("test central directory size fits u32");
        push_u32(&mut bytes, 0x0605_4b50);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 1);
        push_u16(&mut bytes, 1);
        push_u32(&mut bytes, central_directory_size);
        push_u32(&mut bytes, central_directory_offset);
        push_u16(&mut bytes, 0);

        bytes
    }

    fn create_stored_zip_bytes_with_unicode_path_extra_field(
        raw_name: &[u8],
        unicode_name: &str,
        content: &[u8],
    ) -> Vec<u8> {
        let extra = zip_extra_field(
            ZIP_UNICODE_PATH_EXTRA_FIELD,
            &unicode_path_extra_field_payload(raw_name, unicode_name.as_bytes()),
        );
        let content_crc = crc32(content);
        let compressed_size: u32 = content.len().try_into().expect("test content fits u32");
        let uncompressed_size = compressed_size;
        let name_len: u16 = raw_name.len().try_into().expect("test filename fits u16");
        let extra_len: u16 = extra.len().try_into().expect("test extra field fits u16");

        let mut bytes = Vec::new();
        push_u32(&mut bytes, 0x0403_4b50);
        push_u16(&mut bytes, 10);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u32(&mut bytes, content_crc);
        push_u32(&mut bytes, compressed_size);
        push_u32(&mut bytes, uncompressed_size);
        push_u16(&mut bytes, name_len);
        push_u16(&mut bytes, extra_len);
        bytes.extend_from_slice(raw_name);
        bytes.extend_from_slice(&extra);
        bytes.extend_from_slice(content);

        let central_directory_offset: u32 = bytes
            .len()
            .try_into()
            .expect("test central directory offset fits u32");
        push_u32(&mut bytes, 0x0201_4b50);
        push_u16(&mut bytes, 20);
        push_u16(&mut bytes, 10);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u32(&mut bytes, content_crc);
        push_u32(&mut bytes, compressed_size);
        push_u32(&mut bytes, uncompressed_size);
        push_u16(&mut bytes, name_len);
        push_u16(&mut bytes, extra_len);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, 0);
        bytes.extend_from_slice(raw_name);
        bytes.extend_from_slice(&extra);

        let central_directory_size: u32 = (bytes.len()
            - usize::try_from(central_directory_offset)
                .expect("test central directory offset fits usize"))
        .try_into()
        .expect("test central directory size fits u32");
        push_u32(&mut bytes, 0x0605_4b50);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 1);
        push_u16(&mut bytes, 1);
        push_u32(&mut bytes, central_directory_size);
        push_u32(&mut bytes, central_directory_offset);
        push_u16(&mut bytes, 0);

        bytes
    }

    fn unicode_path_extra_field_payload(original_raw_name: &[u8], unicode_name: &[u8]) -> Vec<u8> {
        let mut payload = Vec::with_capacity(1 + 4 + unicode_name.len());
        payload.push(1);
        payload.extend_from_slice(&crc32(original_raw_name).to_le_bytes());
        payload.extend_from_slice(unicode_name);
        payload
    }

    fn zip_extra_field(field_id: u16, payload: &[u8]) -> Vec<u8> {
        let mut extra = Vec::with_capacity(4 + payload.len());
        push_u16(&mut extra, field_id);
        push_u16(
            &mut extra,
            payload
                .len()
                .try_into()
                .expect("test extra field payload fits u16"),
        );
        extra.extend_from_slice(payload);
        extra
    }

    fn push_u16(bytes: &mut Vec<u8>, value: u16) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn push_u32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xffff_ffff_u32;
        for byte in bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                let mask = 0_u32.wrapping_sub(crc & 1);
                crc = (crc >> 1) ^ (0xedb8_8320 & mask);
            }
        }
        !crc
    }

    fn patch_zip_entry_raw_name(
        bytes: &mut [u8],
        placeholder_name: &[u8],
        raw_name: &[u8],
        set_utf8_flag: bool,
    ) {
        patch_zip_entry_raw_name_in_header(
            bytes,
            &[0x50, 0x4b, 0x03, 0x04],
            6,
            26,
            30,
            placeholder_name,
            raw_name,
            set_utf8_flag,
        );
        patch_zip_entry_raw_name_in_header(
            bytes,
            &[0x50, 0x4b, 0x01, 0x02],
            8,
            28,
            46,
            placeholder_name,
            raw_name,
            set_utf8_flag,
        );
    }

    fn patch_zip_entry_raw_name_in_header(
        bytes: &mut [u8],
        signature: &[u8; 4],
        flag_offset: usize,
        name_len_offset: usize,
        name_offset: usize,
        placeholder_name: &[u8],
        raw_name: &[u8],
        set_utf8_flag: bool,
    ) {
        let mut patched = false;
        for index in 0..bytes.len().saturating_sub(signature.len()) {
            if !bytes[index..].starts_with(signature) || index + name_offset > bytes.len() {
                continue;
            }
            let name_len = u16::from_le_bytes([
                bytes[index + name_len_offset],
                bytes[index + name_len_offset + 1],
            ]) as usize;
            let name_start = index + name_offset;
            let name_end = name_start + name_len;
            if name_end > bytes.len() || &bytes[name_start..name_end] != placeholder_name {
                continue;
            }

            assert_eq!(name_len, raw_name.len());
            bytes[name_start..name_end].copy_from_slice(raw_name);
            let flags =
                u16::from_le_bytes([bytes[index + flag_offset], bytes[index + flag_offset + 1]]);
            let flags = if set_utf8_flag {
                flags | ZIP_UTF8_NAME_FLAG
            } else {
                flags & !ZIP_UTF8_NAME_FLAG
            };
            bytes[index + flag_offset..index + flag_offset + 2]
                .copy_from_slice(&flags.to_le_bytes());
            patched = true;
            break;
        }

        assert!(patched, "zip entry header should be patched");
    }

    fn scan_error_with_encoding(
        bytes: Vec<u8>,
        filename_encoding: ArchiveFilenameEncoding,
    ) -> String {
        scan_entries_with_encoding(bytes, filename_encoding)
            .expect_err("scan should reject archive")
            .message()
            .to_string()
    }

    fn scan_entries_with_encoding(
        bytes: Vec<u8>,
        filename_encoding: ArchiveFilenameEncoding,
    ) -> Result<Vec<ZipScanEntry>> {
        let cursor = Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor).expect("zip should open");

        scan_zip_archive(&mut archive, scan_limits(), None, filename_encoding, |_| {
            Ok(())
        })
        .map(|result| result.entries)
    }

    fn scan_error_for(entries: &[(&str, Option<&[u8]>)]) -> String {
        let bytes = create_stored_zip_bytes(entries);
        let cursor = Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor).expect("zip should open");

        scan_zip_archive(
            &mut archive,
            scan_limits(),
            None,
            ArchiveFilenameEncoding::Auto,
            |_| Ok(()),
        )
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

        let result = scan_zip_archive(
            &mut archive,
            scan_limits(),
            None,
            ArchiveFilenameEncoding::Auto,
            |_| Ok(()),
        )
        .expect("parent directory after child file should be valid");

        assert_eq!(result.file_count, 1);
        assert_eq!(result.directory_count, 1);
        assert_eq!(result.entries.len(), 2);
    }

    #[test]
    fn scan_decodes_utf8_chinese_paths() {
        let entries = scan_entries_with_encoding(
            create_stored_zip_bytes(&[("测试/文件.txt", Some(b"payload".as_slice()))]),
            ArchiveFilenameEncoding::Auto,
        )
        .expect("UTF-8 Chinese path should scan");

        assert_eq!(entries[0].path, "测试/文件.txt");
        assert_eq!(entries[0].name, "文件.txt");
        assert_eq!(entries[0].parent.as_deref(), Some("测试"));
    }

    #[test]
    fn scan_auto_rejects_invalid_raw_utf8_when_zip_utf8_flag_is_set() {
        let bytes = create_stored_zip_bytes_with_raw_name_and_utf8_flag(
            "aaaa.txt",
            b"\x82ber.txt",
            b"payload",
        );
        let error = scan_error_with_encoding(bytes, ArchiveFilenameEncoding::Auto);

        assert!(error.contains("filename is not valid UTF-8"));
    }

    #[test]
    fn scan_auto_uses_zip_unicode_path_extra_field_before_heuristics() {
        let bytes = create_stored_zip_bytes_with_unicode_path_extra_field(
            b"rawname.txt",
            "测试/文件.txt",
            b"payload",
        );
        let entries = scan_entries_with_encoding(bytes.clone(), ArchiveFilenameEncoding::Auto)
            .expect("Unicode path extra field should scan in auto mode");

        assert_eq!(entries[0].path, "测试/文件.txt");
        assert_eq!(entries[0].name, "文件.txt");
        assert_eq!(entries[0].parent.as_deref(), Some("测试"));

        let mut archive =
            zip::ZipArchive::new(Cursor::new(bytes)).expect("zip with Unicode path should open");
        let entry = archive.by_index_raw(0).expect("zip entry should open");
        assert!(
            entry.get_metadata().is_utf8,
            "zip crate should mark names from Unicode path extra fields as UTF-8"
        );
        assert_eq!(entry.name_raw(), "测试/文件.txt".as_bytes());
    }

    #[test]
    fn scan_auto_decodes_gb18030_chinese_paths_without_utf8_flag() {
        let bytes = create_stored_zip_bytes_with_raw_name(
            "aaaaaaaaa.txt",
            b"\xb2\xe2\xca\xd4/\xce\xc4\xbc\xfe.txt",
            b"payload",
        );
        let entries = scan_entries_with_encoding(bytes, ArchiveFilenameEncoding::Auto)
            .expect("GB18030 Chinese path should scan in auto mode");

        assert_eq!(entries[0].path, "测试/文件.txt");
        assert_eq!(entries[0].name, "文件.txt");
        assert_eq!(entries[0].parent.as_deref(), Some("测试"));
    }

    #[test]
    fn scan_forced_gb18030_decodes_legacy_chinese_paths() {
        let bytes = create_stored_zip_bytes_with_raw_name(
            "aaaaaaaaa.txt",
            b"\xb2\xe2\xca\xd4/\xce\xc4\xbc\xfe.txt",
            b"payload",
        );
        let entries = scan_entries_with_encoding(bytes, ArchiveFilenameEncoding::Gb18030)
            .expect("GB18030 Chinese path should scan when forced");

        assert_eq!(entries[0].path, "测试/文件.txt");
    }

    #[test]
    fn scan_forced_cp437_keeps_zip_default_decoding() {
        let bytes = create_stored_zip_bytes_with_raw_name("aaaa.txt", b"\x82ber.txt", b"payload");
        let entries = scan_entries_with_encoding(bytes, ArchiveFilenameEncoding::Cp437)
            .expect("CP437 path should scan when forced");

        assert_eq!(entries[0].path, "éber.txt");
    }

    #[test]
    fn scan_forced_cp437_decodes_raw_name_even_with_utf8_flag() {
        let bytes = create_stored_zip_bytes_with_raw_name_and_utf8_flag(
            "aaaa.txt",
            b"\x82ber.txt",
            b"payload",
        );
        let entries = scan_entries_with_encoding(bytes, ArchiveFilenameEncoding::Cp437)
            .expect("CP437 path should scan from raw bytes when forced");

        assert_eq!(entries[0].path, "éber.txt");
    }

    #[test]
    fn scan_forced_cp850_decodes_legacy_latin_paths() {
        let raw_name =
            encode_string_checked("über.txt", &ENCODING_TABLE_CP850).expect("name fits CP850");
        let bytes = create_stored_zip_bytes_with_variable_raw_name(&raw_name, b"payload");
        let entries = scan_entries_with_encoding(bytes, ArchiveFilenameEncoding::Cp850)
            .expect("CP850 path should scan when forced");

        assert_eq!(entries[0].path, "über.txt");
    }

    #[test]
    fn scan_forced_shift_jis_decodes_legacy_japanese_paths() {
        let (raw_name, _, had_errors) = SHIFT_JIS.encode("日本語.txt");
        assert!(!had_errors);
        let bytes = create_stored_zip_bytes_with_variable_raw_name(&raw_name, b"payload");
        let entries = scan_entries_with_encoding(bytes, ArchiveFilenameEncoding::ShiftJis)
            .expect("Shift_JIS path should scan when forced");

        assert_eq!(entries[0].path, "日本語.txt");
    }

    #[test]
    fn scan_forced_big5_decodes_legacy_traditional_chinese_paths() {
        let (raw_name, _, had_errors) = BIG5.encode("繁體.txt");
        assert!(!had_errors);
        let bytes = create_stored_zip_bytes_with_variable_raw_name(&raw_name, b"payload");
        let entries = scan_entries_with_encoding(bytes, ArchiveFilenameEncoding::Big5)
            .expect("Big5 path should scan when forced");

        assert_eq!(entries[0].path, "繁體.txt");
    }

    #[test]
    fn scan_forced_euc_kr_decodes_legacy_korean_paths() {
        let (raw_name, _, had_errors) = EUC_KR.encode("한국어.txt");
        assert!(!had_errors);
        let bytes = create_stored_zip_bytes_with_variable_raw_name(&raw_name, b"payload");
        let entries = scan_entries_with_encoding(bytes, ArchiveFilenameEncoding::EucKr)
            .expect("EUC-KR path should scan when forced");

        assert_eq!(entries[0].path, "한국어.txt");
    }

    #[test]
    fn scan_forced_windows_1252_decodes_legacy_western_paths() {
        let (raw_name, _, had_errors) = WINDOWS_1252.encode("café.txt");
        assert!(!had_errors);
        let bytes = create_stored_zip_bytes_with_variable_raw_name(&raw_name, b"payload");
        let entries = scan_entries_with_encoding(bytes, ArchiveFilenameEncoding::Windows1252)
            .expect("Windows-1252 path should scan when forced");

        assert_eq!(entries[0].path, "café.txt");
    }

    #[test]
    fn scan_forced_utf8_rejects_invalid_raw_names() {
        let bytes = create_stored_zip_bytes_with_raw_name("aaaa.txt", b"\x82ber.txt", b"payload");
        let error = scan_entries_with_encoding(bytes, ArchiveFilenameEncoding::Utf8)
            .expect_err("invalid UTF-8 raw path should be rejected");

        assert!(error.message().contains("filename is not valid UTF-8"));
    }

    #[test]
    fn normalize_archive_entry_path_rejects_unsafe_boundaries() {
        for path in [
            "/absolute.txt",
            "\\absolute.txt",
            "C:/absolute.txt",
            "C:\\absolute.txt",
            "c:relative.txt",
            "safe\0bad.txt",
            "../escape.txt",
            "a/../../escape.txt",
        ] {
            let error = normalize_archive_entry_path(path)
                .expect_err("unsafe archive path should be rejected");
            assert!(
                error.message().contains("unsafe path"),
                "path {path:?} should use the archive unsafe path error, got: {}",
                error.message()
            );
        }
    }

    #[test]
    fn normalize_archive_entry_path_keeps_valid_relative_boundaries() {
        assert_eq!(
            normalize_archive_entry_path("folder/C/file.txt")
                .expect("plain relative path should be valid"),
            PathBuf::from("folder").join("C").join("file.txt")
        );
        assert_eq!(
            normalize_archive_entry_path("folder/../safe.txt")
                .expect("contained parent traversal should normalize safely"),
            PathBuf::from("safe.txt")
        );
        assert_eq!(
            normalize_archive_entry_path("./folder//file.txt")
                .expect("current and empty path components should be ignored"),
            PathBuf::from("folder").join("file.txt")
        );
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
    fn scan_rejects_unicode_normalized_duplicate_paths() {
        let duplicate = scan_error_for(&[
            ("caf\u{00e9}.txt", Some(b"nfc".as_slice())),
            ("cafe\u{0301}.txt", Some(b"nfd".as_slice())),
        ]);

        assert!(duplicate.contains("duplicate entry path"));
    }

    #[test]
    fn scan_rejects_implicit_directory_limit_overflow() {
        let bytes = create_stored_zip_bytes(&[("a/b/c.txt", Some(b"nested".as_slice()))]);
        let cursor = Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor).expect("zip should open");
        let mut limits = scan_limits();
        limits.max_directories = 1;

        let error = scan_zip_archive(
            &mut archive,
            limits,
            None,
            ArchiveFilenameEncoding::Auto,
            |_| Ok(()),
        )
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

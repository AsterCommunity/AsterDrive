use std::io::Cursor;

use super::*;
use crate::services::archive_service::test_utils::crc32;

fn scan_limits() -> ArchiveScanLimits {
    ArchiveScanLimits {
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

fn create_7z_bytes(entries: &[(&str, Option<&[u8]>)], solid: bool) -> Vec<u8> {
    let cursor = Cursor::new(Vec::new());
    let options = if solid {
        zesven::WriteOptions::new().solid()
    } else {
        zesven::WriteOptions::new()
    };
    let mut writer = zesven::Writer::create(cursor)
        .expect("7z writer should start")
        .options(options);

    for (path, content) in entries {
        match content {
            Some(bytes) => writer
                .add_bytes(
                    zesven::ArchivePath::new(path).expect("7z file path should be valid"),
                    bytes,
                )
                .expect("7z file entry should be writable"),
            None => writer
                .add_directory(
                    zesven::ArchivePath::new(path.trim_end_matches('/'))
                        .expect("7z directory path should be valid"),
                    zesven::write::EntryMeta::directory(),
                )
                .expect("7z directory entry should be writable"),
        }
    }

    let (_, cursor) = writer.finish_into_inner().expect("7z writer should finish");
    cursor.into_inner()
}

fn create_7z_bytes_with_anti_item(path: &str) -> Vec<u8> {
    let cursor = Cursor::new(Vec::new());
    let mut writer = zesven::Writer::create(cursor).expect("7z writer should start");
    writer
        .add_anti_item(zesven::ArchivePath::new(path).expect("7z anti path should be valid"))
        .expect("7z anti item should be writable");

    let (_, cursor) = writer.finish_into_inner().expect("7z writer should finish");
    cursor.into_inner()
}

fn create_7z_bytes_with_patched_name(original_name: &str, patched_name: &str) -> Vec<u8> {
    assert_eq!(
        original_name.encode_utf16().count(),
        patched_name.encode_utf16().count(),
        "patched 7z name must keep the UTF-16 length unchanged"
    );
    let mut bytes = create_7z_bytes(&[(original_name, Some(b"payload".as_slice()))], false);
    let original = utf16le_null_terminated(original_name);
    let patched = utf16le_null_terminated(patched_name);
    let offset = bytes
        .windows(original.len())
        .position(|window| window == original.as_slice())
        .expect("7z encoded file name should be present");
    bytes[offset..offset + patched.len()].copy_from_slice(&patched);
    refresh_7z_header_checksums(&mut bytes);
    bytes
}

fn refresh_7z_header_checksums(bytes: &mut [u8]) {
    let next_header_offset = read_u64_le(bytes, 12);
    let next_header_size = read_u64_le(bytes, 20);
    let next_header_start: usize = (32 + next_header_offset)
        .try_into()
        .expect("test 7z next header offset should fit usize");
    let next_header_size: usize = next_header_size
        .try_into()
        .expect("test 7z next header size should fit usize");
    let next_header_crc = crc32(&bytes[next_header_start..next_header_start + next_header_size]);
    bytes[28..32].copy_from_slice(&next_header_crc.to_le_bytes());

    let start_header_crc = crc32(&bytes[12..32]);
    bytes[8..12].copy_from_slice(&start_header_crc.to_le_bytes());
}

fn read_u64_le(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("test 7z header should contain u64"),
    )
}

fn utf16le_null_terminated(value: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    for code_unit in value.encode_utf16() {
        bytes.extend_from_slice(&code_unit.to_le_bytes());
    }
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes
}

fn scan_7z(bytes: Vec<u8>, limits: ArchiveScanLimits) -> Result<ArchiveScanResult> {
    let source_archive_size =
        crate::utils::numbers::usize_to_u64(bytes.len(), "test 7z archive size")?;
    let archive = open_seven_zip_streaming_archive(Cursor::new(bytes), limits)?;
    scan_seven_zip_archive(
        &archive,
        limits,
        source_archive_size,
        None,
        ArchiveScanNamePolicy::StrictAsterName,
        |_| Ok(()),
    )
}

fn scan_7z_with_source_size(
    bytes: Vec<u8>,
    limits: ArchiveScanLimits,
    source_archive_size: u64,
) -> Result<ArchiveScanResult> {
    scan_7z_with_open_limits(bytes, limits, limits, source_archive_size)
}

fn scan_7z_with_open_limits(
    bytes: Vec<u8>,
    open_limits: ArchiveScanLimits,
    scan_limits: ArchiveScanLimits,
    source_archive_size: u64,
) -> Result<ArchiveScanResult> {
    let archive = open_seven_zip_streaming_archive(Cursor::new(bytes), open_limits)?;
    scan_seven_zip_archive(
        &archive,
        scan_limits,
        source_archive_size,
        None,
        ArchiveScanNamePolicy::StrictAsterName,
        |_| Ok(()),
    )
}

fn scan_7z_error(entries: &[(&str, Option<&[u8]>)]) -> String {
    let bytes = create_7z_bytes(entries, false);
    scan_7z(bytes, scan_limits())
        .expect_err("7z scan should reject archive")
        .message()
        .to_string()
}

#[test]
fn scan_seven_zip_root_file_does_not_count_empty_parent_directory() {
    let bytes = create_7z_bytes(&[("note.txt", Some(b"root file"))], false);
    let result = scan_7z(bytes, scan_limits()).expect("7z scan should succeed");

    assert_eq!(result.file_count, 1);
    assert_eq!(result.directory_count, 0);
    assert_eq!(result.entries[0].parent, None);
}

#[test]
fn scan_seven_zip_rejects_too_many_implicit_directories() {
    let bytes = create_7z_bytes(
        &[
            ("one/file.txt", Some(b"one")),
            ("two/file.txt", Some(b"two")),
        ],
        false,
    );
    let mut limits = scan_limits();
    limits.max_directories = 1;

    let error = scan_7z(bytes, limits)
        .expect_err("7z scan should reject directory limit")
        .message()
        .to_string();
    assert!(error.contains("directories"));
}

#[test]
fn scan_seven_zip_rejects_too_many_entries() {
    let bytes = create_7z_bytes(
        &[
            ("first.txt", Some(b"first")),
            ("second.txt", Some(b"second")),
        ],
        false,
    );
    let mut limits = scan_limits();
    limits.max_entries = 1;

    let error = scan_7z_with_open_limits(bytes, scan_limits(), limits, 1024)
        .expect_err("7z scan should reject entry limit")
        .message()
        .to_string();
    assert!(
        error.contains("entries") || error.contains("entry count"),
        "unexpected entry limit error: {error}"
    );
}

#[test]
fn scan_seven_zip_rejects_too_many_files() {
    let bytes = create_7z_bytes(
        &[
            ("first.txt", Some(b"first")),
            ("second.txt", Some(b"second")),
        ],
        false,
    );
    let mut limits = scan_limits();
    limits.max_files = 1;

    let error = scan_7z(bytes, limits)
        .expect_err("7z scan should reject file limit")
        .message()
        .to_string();
    assert!(error.contains("files"));
}

#[test]
fn scan_seven_zip_rejects_uncompressed_size_limit() {
    let payload = vec![b'a'; 2048];
    let bytes = create_7z_bytes(&[("payload.txt", Some(&payload))], false);
    let mut limits = scan_limits();
    limits.max_uncompressed_bytes = 1024;

    let error = scan_7z(bytes, limits)
        .expect_err("7z scan should reject uncompressed size limit")
        .message()
        .to_string();
    assert!(error.contains("uncompressed size"));
}

#[test]
fn scan_seven_zip_rejects_file_directory_conflicts() {
    let error = scan_7z_error(&[("prefix", Some(b"file")), ("prefix/child", Some(b"child"))]);

    assert!(error.contains("inside file entry"));
}

#[test]
fn scan_seven_zip_rejects_skipped_entries_from_path_traversal() {
    let bytes = create_7z_bytes_with_patched_name("safe-path.txt", "../escape.txt");
    let error = scan_7z(bytes, scan_limits())
        .expect_err("7z scan should reject skipped unsafe entries")
        .message()
        .to_string();

    assert!(
        error.contains("unsafe path") || error.contains("path traversal"),
        "unexpected unsafe path error: {error}"
    );
}

#[test]
fn scan_seven_zip_marks_display_only_names_extract_incompatible() {
    let bytes = create_7z_bytes(&[("folder/name:with-colon.txt", Some(b"display"))], false);
    let source_archive_size =
        crate::utils::numbers::usize_to_u64(bytes.len(), "test 7z archive size")
            .expect("test 7z archive size should fit u64");
    let archive = open_seven_zip_streaming_archive(Cursor::new(bytes), scan_limits())
        .expect("7z archive should open");
    let result = scan_seven_zip_archive(
        &archive,
        scan_limits(),
        source_archive_size,
        None,
        ArchiveScanNamePolicy::PreviewDisplayName,
        |_| Ok(()),
    )
    .expect("display-only scan should succeed");

    assert!(!result.extract_compatible);
}

#[test]
fn scan_seven_zip_rejects_anti_items() {
    let bytes = create_7z_bytes_with_anti_item("deleted.txt");
    let error = scan_7z(bytes, scan_limits())
        .expect_err("7z scan should reject anti item")
        .message()
        .to_string();

    assert!(error.contains("anti-item"));
}

#[test]
fn seven_zip_open_error_maps_password_failures_to_validation_error() {
    let error = map_seven_zip_open_error(zesven::Error::PasswordRequired);

    assert_eq!(error.message(), "encrypted 7z archives are not supported");
}

#[test]
fn scan_seven_zip_rejects_total_compression_ratio() {
    let payload = vec![b'a'; 4096];
    let bytes = create_7z_bytes(&[("payload.txt", Some(&payload))], false);
    let open_limits = scan_limits();
    let mut limits = scan_limits();
    limits.max_entry_compression_ratio = u64::MAX;
    limits.max_compression_ratio = 1;

    let error = scan_7z_with_open_limits(bytes, open_limits, limits, 1024)
        .expect_err("7z scan should reject total compression ratio")
        .message()
        .to_string();
    assert!(error.contains("total compression ratio"));
}

#[test]
fn scan_seven_zip_rejects_high_entry_compression_ratio() {
    let payload = vec![b'a'; 4096];
    let bytes = create_7z_bytes(&[("payload.txt", Some(&payload))], false);
    let mut limits = scan_limits();
    limits.max_entry_compression_ratio = 1;

    let error = scan_7z(bytes, limits)
        .expect_err("7z scan should reject high entry ratio")
        .message()
        .to_string();
    assert!(error.contains("compression ratio"));
}

#[test]
fn scan_seven_zip_rejects_entry_compression_ratio_against_source_size() {
    let payload = vec![b'a'; 4096];
    let bytes = create_7z_bytes(&[("payload.txt", Some(&payload))], false);
    let mut limits = scan_limits();
    limits.max_entry_compression_ratio = 1;
    limits.max_compression_ratio = u64::MAX;

    let error = scan_7z_with_source_size(bytes, limits, 1024)
        .expect_err("7z scan should reject entry compression ratio")
        .message()
        .to_string();
    assert!(error.contains("compression ratio"));
}

#[test]
fn scan_seven_zip_accepts_solid_archive() {
    let bytes = create_7z_bytes(
        &[
            ("first.txt", Some(b"first")),
            ("second.txt", Some(b"second")),
        ],
        true,
    );
    let archive = open_seven_zip_streaming_archive(Cursor::new(bytes), scan_limits())
        .expect("solid 7z archive should open");

    assert!(archive.is_solid());
    let source_archive_size = 1024;
    let result = scan_seven_zip_archive(
        &archive,
        scan_limits(),
        source_archive_size,
        None,
        ArchiveScanNamePolicy::StrictAsterName,
        |_| Ok(()),
    )
    .expect("solid 7z scan should succeed");
    assert_eq!(result.file_count, 2);
}

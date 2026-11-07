use std::io::{Cursor, Write};
use std::path::PathBuf;

use encoding_rs::{BIG5, EUC_KR, SHIFT_JIS, WINDOWS_1252};
use oem_cp::{code_table::ENCODING_TABLE_CP850, encode_string_checked};

use crate::services::files::archive::core::path::normalize_archive_entry_path;
use crate::services::files::archive::core::scan::ArchiveScanEntry;
use crate::services::files::archive::core::test_utils::{
    crc32, create_single_file_zip_with_raw_name, push_u16, push_u32,
};

use super::*;
use zip::HasZipMetadata;

const ZIP_UTF8_NAME_FLAG: u16 = 0x0800;
const ZIP_UNICODE_PATH_EXTRA_FIELD: u16 = 0x7075;
const ZIP_ENCRYPTED_FLAG: u16 = 0x0001;

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

#[test]
fn metadata_open_rejects_declared_entry_budget_before_reading_entries() {
    use std::io::{Read, Seek, SeekFrom};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    struct CountedReader {
        inner: Cursor<Vec<u8>>,
        read: Arc<AtomicUsize>,
    }
    impl Read for CountedReader {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            let read = self.inner.read(bytes)?;
            self.read.fetch_add(read, Ordering::Relaxed);
            Ok(read)
        }
    }
    impl Seek for CountedReader {
        fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
            self.inner.seek(position)
        }
    }

    let owned_names = (0..101).map(|i| format!("entry-{i}")).collect::<Vec<_>>();
    let entries = owned_names
        .iter()
        .map(|name| (name.as_str(), Some(b"".as_slice())))
        .collect::<Vec<_>>();
    let bytes = create_stored_zip_bytes(&entries);
    let size = bytes.len();
    let read = Arc::new(AtomicUsize::new(0));
    let mut archive = open_zip_archive(
        CountedReader {
            inner: Cursor::new(bytes),
            read: read.clone(),
        },
        None,
    )
    .expect("footer should open");
    let constructor_read = read.load(Ordering::Relaxed);
    assert!(
        constructor_read < size,
        "opening must not consume a full entry index"
    );
    assert_eq!(archive.len(), 101);
    let error = scan_zip_archive_raw(&mut archive, scan_limits(), None)
        .expect_err("entry admission must precede iteration");
    assert!(error.message().contains("101 entries"));
    assert_eq!(
        read.load(Ordering::Relaxed),
        constructor_read,
        "over-budget entry counts are rejected without entry parsing"
    );
}

#[test]
fn metadata_entry_budget_counts_duplicate_names_before_deduplication() {
    let mut bytes = create_stored_zip_bytes(&[("first", Some(b"one")), ("other", Some(b"two"))]);
    patch_zip_entry_raw_name(&mut bytes, b"other", b"first", true);
    let mut archive = open_zip_archive(Cursor::new(bytes), None).unwrap();
    assert_eq!(
        archive.len(),
        2,
        "raw central records must not be deduplicated before admission"
    );
    let mut limits = scan_limits();
    limits.max_entries = 1;
    let error = scan_zip_archive_raw(&mut archive, limits, None)
        .expect_err("duplicate names still consume the entry budget");
    assert!(error.message().contains("2 entries"));
}

fn stored_zip_as_zip64(mut bytes: Vec<u8>, sector_size: usize) -> Vec<u8> {
    let eocd = bytes.split_off(bytes.len() - 22);
    let count = u16::from_le_bytes([eocd[10], eocd[11]]) as u64;
    let directory_size = u32::from_le_bytes(eocd[12..16].try_into().expect("size field")) as u64;
    let directory_offset =
        u32::from_le_bytes(eocd[16..20].try_into().expect("offset field")) as u64;
    let zip64_offset = bytes.len() as u64;
    bytes.extend_from_slice(b"PK\x06\x06");
    bytes.extend_from_slice(&(44 + sector_size as u64).to_le_bytes());
    bytes.extend_from_slice(&45u16.to_le_bytes());
    bytes.extend_from_slice(&45u16.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&count.to_le_bytes());
    bytes.extend_from_slice(&count.to_le_bytes());
    bytes.extend_from_slice(&directory_size.to_le_bytes());
    bytes.extend_from_slice(&directory_offset.to_le_bytes());
    bytes.resize(bytes.len() + sector_size, 0);
    bytes.extend_from_slice(b"PK\x06\x07");
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&zip64_offset.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(b"PK\x05\x06");
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&u16::MAX.to_le_bytes());
    bytes.extend_from_slice(&u16::MAX.to_le_bytes());
    bytes.extend_from_slice(&u32::MAX.to_le_bytes());
    bytes.extend_from_slice(&u32::MAX.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes
}

#[test]
fn metadata_open_preserves_zip64_and_streamed_deflate_extraction() {
    for method in [
        zip::CompressionMethod::Stored,
        zip::CompressionMethod::Deflated,
    ] {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file(
                "folder/hello.txt",
                zip::write::SimpleFileOptions::default().compression_method(method),
            )
            .unwrap();
        writer.write_all(b"hello ZIP64").unwrap();
        let bytes = stored_zip_as_zip64(writer.finish().unwrap().into_inner(), 128);
        let mut archive =
            open_zip_archive(Cursor::new(bytes), None).expect("bounded ZIP64 should open");
        let manifest = scan_zip_archive(
            &mut archive,
            scan_limits(),
            None,
            ArchiveFilenameEncoding::Auto,
            ArchiveScanNamePolicy::StrictAsterName,
            |_| Ok(()),
        )
        .expect("ZIP64 should scan");
        assert_eq!(manifest.entry_count, 1);
        let entries = archive.files().unwrap();
        entries.reader_mut().finish_metadata();
        let descriptor = entries.next().unwrap().unwrap();
        let mut entry = descriptor
            .with_reader(entries.reader_mut(), zip::ZipReadOptions::default())
            .unwrap();
        let mut output = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut output).unwrap();
        assert_eq!(output, b"hello ZIP64");
    }
}

#[test]
fn metadata_open_rejects_zip64_sector_before_large_allocation() {
    use std::io::{self, Read, Seek, SeekFrom};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    struct SparseArchive {
        header: Vec<u8>,
        footer: Vec<u8>,
        footer_start: u64,
        position: u64,
        read_bytes: Arc<AtomicUsize>,
    }
    impl Read for SparseArchive {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            let size = self.footer_start + self.footer.len() as u64;
            let length = output
                .len()
                .min(usize::try_from(size.saturating_sub(self.position)).unwrap_or(usize::MAX));
            let output = &mut output[..length];
            output.fill(0);
            for (offset, bytes) in [(0, &self.header), (self.footer_start, &self.footer)] {
                let start = self.position.max(offset);
                let end = (self.position + length as u64).min(offset + bytes.len() as u64);
                if start < end {
                    output[usize::try_from(start - self.position).unwrap()
                        ..usize::try_from(end - self.position).unwrap()]
                        .copy_from_slice(
                            &bytes[usize::try_from(start - offset).unwrap()
                                ..usize::try_from(end - offset).unwrap()],
                        );
                }
            }
            self.position += length as u64;
            self.read_bytes.fetch_add(length, Ordering::Relaxed);
            Ok(length)
        }
    }
    impl Seek for SparseArchive {
        fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
            let position = match position {
                SeekFrom::Start(position) => i128::from(position),
                SeekFrom::End(delta) => {
                    i128::from(self.footer_start + self.footer.len() as u64) + i128::from(delta)
                }
                SeekFrom::Current(delta) => i128::from(self.position) + i128::from(delta),
            };
            self.position = u64::try_from(position)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid seek"))?;
            Ok(self.position)
        }
    }

    let bytes = stored_zip_as_zip64(create_stored_zip_bytes(&[("small.txt", Some(b"body"))]), 0);
    let zip64_start = bytes
        .windows(4)
        .position(|window| window == b"PK\x06\x06")
        .unwrap();
    let mut header = bytes[..zip64_start + 56].to_vec();
    let declared_record_size = 1024 * 1024 * 1024u64;
    header[zip64_start + 4..zip64_start + 12].copy_from_slice(&declared_record_size.to_le_bytes());
    let footer = bytes[zip64_start + 56..].to_vec();
    let read_bytes = Arc::new(AtomicUsize::new(0));
    let reader = SparseArchive {
        header,
        footer,
        footer_start: zip64_start as u64 + 12 + declared_record_size,
        position: 0,
        read_bytes: read_bytes.clone(),
    };
    let error = open_zip_archive(reader, None)
        .err()
        .expect("oversized ZIP64 sector must be rejected");
    assert!(error.to_string().contains("budget"), "{error}");
    assert!(
        read_bytes.load(Ordering::Relaxed) < 4096,
        "must reject from fixed headers without reading or allocating the sector"
    );
}

fn create_stored_zip_bytes(entries: &[(&str, Option<&[u8]>)]) -> Vec<u8> {
    let cursor = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(cursor);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

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
    create_single_file_zip_with_raw_name(raw_name, content)
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

fn patch_zip_entry_raw_name(
    bytes: &mut [u8],
    placeholder_name: &[u8],
    raw_name: &[u8],
    set_utf8_flag: bool,
) {
    patch_zip_entry_raw_name_in_header(
        bytes,
        ZipHeaderNameLayout {
            signature: &[0x50, 0x4b, 0x03, 0x04],
            flag_offset: 6,
            name_len_offset: 26,
            name_offset: 30,
        },
        placeholder_name,
        raw_name,
        set_utf8_flag,
    );
    patch_zip_entry_raw_name_in_header(
        bytes,
        ZipHeaderNameLayout {
            signature: &[0x50, 0x4b, 0x01, 0x02],
            flag_offset: 8,
            name_len_offset: 28,
            name_offset: 46,
        },
        placeholder_name,
        raw_name,
        set_utf8_flag,
    );
}

struct ZipHeaderNameLayout {
    signature: &'static [u8; 4],
    flag_offset: usize,
    name_len_offset: usize,
    name_offset: usize,
}

fn patch_zip_entry_raw_name_in_header(
    bytes: &mut [u8],
    layout: ZipHeaderNameLayout,
    placeholder_name: &[u8],
    raw_name: &[u8],
    set_utf8_flag: bool,
) {
    let mut patched = false;
    for index in 0..bytes.len().saturating_sub(layout.signature.len()) {
        if !bytes[index..].starts_with(layout.signature) || index + layout.name_offset > bytes.len()
        {
            continue;
        }
        let name_len = u16::from_le_bytes([
            bytes[index + layout.name_len_offset],
            bytes[index + layout.name_len_offset + 1],
        ]) as usize;
        let name_start = index + layout.name_offset;
        let name_end = name_start + name_len;
        if name_end > bytes.len() || &bytes[name_start..name_end] != placeholder_name {
            continue;
        }

        assert_eq!(name_len, raw_name.len());
        bytes[name_start..name_end].copy_from_slice(raw_name);
        let flags = u16::from_le_bytes([
            bytes[index + layout.flag_offset],
            bytes[index + layout.flag_offset + 1],
        ]);
        let flags = if set_utf8_flag {
            flags | ZIP_UTF8_NAME_FLAG
        } else {
            flags & !ZIP_UTF8_NAME_FLAG
        };
        bytes[index + layout.flag_offset..index + layout.flag_offset + 2]
            .copy_from_slice(&flags.to_le_bytes());
        patched = true;
        break;
    }

    assert!(patched, "zip entry header should be patched");
}

fn patch_zip_central_external_attrs(bytes: &mut [u8], path: &str, external_attrs: u32) {
    let path = path.as_bytes();
    let central_signature = [0x50, 0x4B, 0x01, 0x02];
    let mut patched = false;

    for index in 0..bytes.len().saturating_sub(central_signature.len()) {
        if !bytes[index..].starts_with(&central_signature) {
            continue;
        }
        let name_len = u16::from_le_bytes([bytes[index + 28], bytes[index + 29]]) as usize;
        let name_start = index + 46;
        let name_end = name_start + name_len;
        if name_end <= bytes.len() && &bytes[name_start..name_end] == path {
            bytes[index + 38..index + 42].copy_from_slice(&external_attrs.to_le_bytes());
            bytes[index + 5] = 3;
            patched = true;
            break;
        }
    }

    assert!(patched, "central directory entry should exist");
}

fn find_zip_header(
    bytes: &[u8],
    path: &str,
    signature: &[u8; 4],
    name_len_offset: usize,
    name_offset: usize,
) -> usize {
    let path = path.as_bytes();
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
        if name_end <= bytes.len() && &bytes[name_start..name_end] == path {
            return index;
        }
    }

    panic!("zip entry header should exist");
}

fn patch_zip_entry_general_purpose_flag(bytes: &mut [u8], path: &str, flag_mask: u16) {
    let local_header = find_zip_header(bytes, path, &[0x50, 0x4B, 0x03, 0x04], 26, 30);
    let local_flags = u16::from_le_bytes([bytes[local_header + 6], bytes[local_header + 7]]);
    bytes[local_header + 6..local_header + 8]
        .copy_from_slice(&(local_flags | flag_mask).to_le_bytes());

    let central_header = find_zip_header(bytes, path, &[0x50, 0x4B, 0x01, 0x02], 28, 46);
    let central_flags = u16::from_le_bytes([bytes[central_header + 8], bytes[central_header + 9]]);
    bytes[central_header + 8..central_header + 10]
        .copy_from_slice(&(central_flags | flag_mask).to_le_bytes());
}

fn create_symlink_zip_bytes(path: &str, target: &str) -> Vec<u8> {
    let mut bytes = create_stored_zip_bytes(&[(path, Some(target.as_bytes()))]);
    patch_zip_central_external_attrs(&mut bytes, path, 0o120777_u32 << 16);
    bytes
}

fn create_special_file_zip_bytes(path: &str) -> Vec<u8> {
    let mut bytes = create_stored_zip_bytes(&[(path, Some(b""))]);
    patch_zip_central_external_attrs(&mut bytes, path, 0o060666_u32 << 16);
    bytes
}

fn create_encrypted_flag_zip_bytes(path: &str, content: &[u8]) -> Vec<u8> {
    let mut bytes = create_stored_zip_bytes(&[(path, Some(content))]);
    patch_zip_entry_general_purpose_flag(&mut bytes, path, ZIP_ENCRYPTED_FLAG);
    bytes
}

fn scan_zip(bytes: Vec<u8>, limits: ArchiveScanLimits) -> Result<ArchiveScanResult> {
    let cursor = Cursor::new(bytes);
    let mut archive = open_zip_archive(cursor, None).expect("zip should open");

    scan_zip_archive(
        &mut archive,
        limits,
        None,
        ArchiveFilenameEncoding::Auto,
        ArchiveScanNamePolicy::StrictAsterName,
        |_| Ok(()),
    )
}

fn scan_zip_raw(bytes: Vec<u8>, limits: ArchiveScanLimits) -> Result<ArchiveRawScanResult> {
    let cursor = Cursor::new(bytes);
    let mut archive = open_zip_archive(cursor, None).expect("zip should open");

    scan_zip_archive_raw(&mut archive, limits, None)
}

fn scan_error_with_encoding(bytes: Vec<u8>, filename_encoding: ArchiveFilenameEncoding) -> String {
    scan_entries_with_encoding(bytes, filename_encoding)
        .expect_err("scan should reject archive")
        .message()
        .to_string()
}

fn scan_entries_with_encoding(
    bytes: Vec<u8>,
    filename_encoding: ArchiveFilenameEncoding,
) -> Result<Vec<ArchiveScanEntry>> {
    let cursor = Cursor::new(bytes);
    let mut archive = open_zip_archive(cursor, None).expect("zip should open");

    scan_zip_archive(
        &mut archive,
        scan_limits(),
        None,
        filename_encoding,
        ArchiveScanNamePolicy::StrictAsterName,
        |_| Ok(()),
    )
    .map(|result| result.entries)
}

fn scan_preview_entries_with_encoding(
    bytes: Vec<u8>,
    filename_encoding: ArchiveFilenameEncoding,
) -> Result<Vec<ArchiveScanEntry>> {
    let cursor = Cursor::new(bytes);
    let mut archive = open_zip_archive(cursor, None).expect("zip should open");

    scan_zip_archive(
        &mut archive,
        scan_limits(),
        None,
        filename_encoding,
        ArchiveScanNamePolicy::PreviewDisplayName,
        |_| Ok(()),
    )
    .map(|result| result.entries)
}

fn scan_error_for(entries: &[(&str, Option<&[u8]>)]) -> String {
    let bytes = create_stored_zip_bytes(entries);
    let cursor = Cursor::new(bytes);
    let mut archive = open_zip_archive(cursor, None).expect("zip should open");

    scan_zip_archive(
        &mut archive,
        scan_limits(),
        None,
        ArchiveFilenameEncoding::Auto,
        ArchiveScanNamePolicy::StrictAsterName,
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
    let mut archive = open_zip_archive(cursor, None).expect("zip should open");

    let result = scan_zip_archive(
        &mut archive,
        scan_limits(),
        None,
        ArchiveFilenameEncoding::Auto,
        ArchiveScanNamePolicy::StrictAsterName,
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
    let bytes =
        create_stored_zip_bytes_with_raw_name_and_utf8_flag("aaaa.txt", b"\x82ber.txt", b"payload");
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
        open_zip_archive(Cursor::new(bytes), None).expect("zip with Unicode path should open");
    let entry = archive
        .files()
        .expect("zip iterator should open")
        .next()
        .expect("zip entry should exist")
        .expect("zip entry should open");
    assert!(
        entry.get_metadata().flags.as_u16() & super::encoding::ZIP_UTF8_FLAG != 0,
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
    let bytes =
        create_stored_zip_bytes_with_raw_name_and_utf8_flag("aaaa.txt", b"\x82ber.txt", b"payload");
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
        r"\\?\C:\../escape.txt",
        r"\\?\C:\..\escape.txt",
        r"\\server\share\escape.txt",
    ] {
        let error = normalize_archive_entry_path(path, ArchiveScanNamePolicy::StrictAsterName)
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
        normalize_archive_entry_path("folder/C/file.txt", ArchiveScanNamePolicy::StrictAsterName)
            .expect("plain relative path should be valid"),
        PathBuf::from("folder").join("C").join("file.txt")
    );
    assert_eq!(
        normalize_archive_entry_path("folder/../safe.txt", ArchiveScanNamePolicy::StrictAsterName)
            .expect("contained parent traversal should normalize safely"),
        PathBuf::from("safe.txt")
    );
    assert_eq!(
        normalize_archive_entry_path("./folder//file.txt", ArchiveScanNamePolicy::StrictAsterName)
            .expect("current and empty path components should be ignored"),
        PathBuf::from("folder").join("file.txt")
    );
}

#[test]
fn preview_name_policy_allows_display_names_for_preview_only() {
    let bytes =
        create_stored_zip_bytes(&[("folder/name:with-colon.txt", Some(b"payload".as_slice()))]);

    let strict_error = scan_entries_with_encoding(bytes.clone(), ArchiveFilenameEncoding::Auto)
        .expect_err("strict extract scan should reject colon in path segment");
    assert!(strict_error.message().contains("forbidden character ':'"));

    let entries = scan_preview_entries_with_encoding(bytes, ArchiveFilenameEncoding::Auto)
        .expect("preview scan should allow display-only names");

    assert_eq!(entries[0].path, "folder/name:with-colon.txt");
    assert_eq!(entries[0].name, "name:with-colon.txt");
    assert_eq!(entries[0].parent.as_deref(), Some("folder"));
}

#[test]
fn scan_rejects_encrypted_entries() {
    let bytes = create_encrypted_flag_zip_bytes("secret.txt", b"secret");
    let error = scan_zip(bytes, scan_limits())
        .expect_err("encrypted ZIP entry should be rejected")
        .message()
        .to_string();

    assert!(error.contains("encrypted"));
}

#[test]
fn scan_rejects_symlink_entries() {
    // Reject links before extraction regardless of whether their targets are external or
    // contained. This does not depend on zip's built-in extract/path containment behavior.
    for target in [
        "../target",
        "/outside/target",
        r"C:\outside\target",
        "inside.txt",
    ] {
        let bytes = create_symlink_zip_bytes("link.txt", target);
        let error = scan_zip(bytes, scan_limits())
            .expect_err("ZIP symlink entry should be rejected")
            .message()
            .to_string();
        assert!(
            error.contains("symbolic link"),
            "target {target:?}: {error}"
        );
    }
}

#[test]
fn scan_rejects_special_file_entries() {
    let bytes = create_special_file_zip_bytes("device");
    let error = scan_zip(bytes, scan_limits())
        .expect_err("ZIP special file entry should be rejected")
        .message()
        .to_string();

    assert!(error.contains("special file"));
}

#[test]
fn scan_zip_raw_rejects_encrypted_entries() {
    let bytes = create_encrypted_flag_zip_bytes("secret.txt", b"secret");
    let error = scan_zip_raw(bytes, scan_limits())
        .expect_err("raw ZIP scan should reject encrypted entries")
        .message()
        .to_string();

    assert!(error.contains("encrypted"));
}

#[test]
fn scan_zip_raw_returns_manifest_and_replay_rejects_path_traversal() {
    let bytes = create_stored_zip_bytes(&[("safe/file.txt", Some(b"payload".as_slice()))]);
    let raw = scan_zip_raw(bytes, scan_limits()).expect("raw ZIP scan should succeed");

    assert_eq!(raw.entry_count, 1);
    assert_eq!(raw.file_count, 1);
    assert_eq!(raw.total_uncompressed_bytes, 7);
    assert_eq!(raw.total_compressed_base, 7);
    assert_eq!(raw.entries[0].display_name, "safe/file.txt");
    assert_eq!(raw.entries[0].raw_name, b"safe/file.txt");

    let mut tampered_entries = raw.entries;
    tampered_entries[0].raw_name = b"../escape.txt".to_vec();
    tampered_entries[0].display_name = "../escape.txt".to_string();
    let error = build_zip_scan_result_from_raw_entries(
        &tampered_entries,
        scan_limits(),
        None,
        ArchiveFilenameEncoding::Auto,
        ArchiveScanNamePolicy::StrictAsterName,
        |_| Ok(()),
    )
    .expect_err("raw ZIP replay should reject unsafe paths");

    assert!(error.message().contains("unsafe path"));
}

#[test]
fn build_zip_scan_result_rejects_raw_replay_entry_limit() {
    let raw_entries = vec![
        ArchiveRawScanEntry {
            index: 0,
            raw_name: b"first.txt".to_vec(),
            display_name: "first.txt".to_string(),
            raw_name_utf8: true,
            kind: ArchiveScanEntryKind::File,
            size: 1,
            compressed_size: 1,
            modified_at: None,
        },
        ArchiveRawScanEntry {
            index: 1,
            raw_name: b"second.txt".to_vec(),
            display_name: "second.txt".to_string(),
            raw_name_utf8: true,
            kind: ArchiveScanEntryKind::File,
            size: 1,
            compressed_size: 1,
            modified_at: None,
        },
    ];
    let mut limits = scan_limits();
    limits.max_entries = 1;

    let error = build_zip_scan_result_from_raw_entries(
        &raw_entries,
        limits,
        None,
        ArchiveFilenameEncoding::Auto,
        ArchiveScanNamePolicy::StrictAsterName,
        |_| Ok(()),
    )
    .expect_err("raw ZIP replay should reject entry limit");

    assert!(error.message().contains("entries"));
}

#[test]
fn build_zip_scan_result_rejects_raw_replay_extracted_size_overflow() {
    let raw_entries = vec![ArchiveRawScanEntry {
        index: 0,
        raw_name: b"payload.txt".to_vec(),
        display_name: "payload.txt".to_string(),
        raw_name_utf8: true,
        kind: ArchiveScanEntryKind::File,
        size: 11,
        compressed_size: 11,
        modified_at: None,
    }];
    let mut limits = scan_limits();
    limits.max_uncompressed_bytes = 10;

    let error = build_zip_scan_result_from_raw_entries(
        &raw_entries,
        limits,
        None,
        ArchiveFilenameEncoding::Auto,
        ArchiveScanNamePolicy::StrictAsterName,
        |_| Ok(()),
    )
    .expect_err("raw ZIP replay should reject extracted size above preflight limit");

    assert!(error.message().contains("uncompressed size"));
}

#[test]
fn build_zip_scan_result_rejects_raw_replay_total_compression_ratio() {
    let raw_entries = vec![ArchiveRawScanEntry {
        index: 0,
        raw_name: b"payload.txt".to_vec(),
        display_name: "payload.txt".to_string(),
        raw_name_utf8: true,
        kind: ArchiveScanEntryKind::File,
        size: 20,
        compressed_size: 10,
        modified_at: None,
    }];
    let mut limits = scan_limits();
    limits.max_entry_compression_ratio = u64::MAX;
    limits.max_compression_ratio = 1;

    let error = build_zip_scan_result_from_raw_entries(
        &raw_entries,
        limits,
        None,
        ArchiveFilenameEncoding::Auto,
        ArchiveScanNamePolicy::StrictAsterName,
        |_| Ok(()),
    )
    .expect_err("raw ZIP replay should reject total compression ratio");

    assert!(error.message().contains("total compression ratio"));
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
    assert!(child_file.contains("archive file 'prefix/child.txt' is inside file entry 'prefix'"));

    let child_directory = scan_error_for(&[
        ("prefix", Some(b"not-a-directory".as_slice())),
        ("prefix/child/", None),
    ]);
    assert!(
        child_directory.contains("archive directory 'prefix/child' is inside file entry 'prefix'")
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
    let mut archive = open_zip_archive(cursor, None).expect("zip should open");
    let mut limits = scan_limits();
    limits.max_directories = 1;

    let error = scan_zip_archive(
        &mut archive,
        limits,
        None,
        ArchiveFilenameEncoding::Auto,
        ArchiveScanNamePolicy::StrictAsterName,
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
        ensure_archive_scan_deadline(Some(Instant::now() - std::time::Duration::from_secs(1)))
            .expect_err("expired deadline should reject scan");

    assert_eq!(error.message(), "archive scan exceeded server time limit");
}

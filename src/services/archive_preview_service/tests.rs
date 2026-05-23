use async_trait::async_trait;
use base64::Engine as _;
use chrono::Utc;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::AsyncWriteExt;

use super::cache::{fit_raw_manifest_to_cache_limit, serialize_cached_raw_manifest};
use super::model::{ArchiveRawEntry, ArchiveRawManifest};
use super::scan::build_manifest_from_raw;
use super::*;
use crate::entities::system_config;
use crate::services::archive_service::test_utils::create_single_file_zip_with_raw_name;
use crate::storage::BlobMetadata;
use crate::storage::StorageDriver;
use crate::types::{SystemConfigSource, SystemConfigValueType};

struct PreviewMemoryRangeDriver {
    data: Vec<u8>,
    range_calls: AtomicUsize,
    stream_calls: AtomicUsize,
}

impl PreviewMemoryRangeDriver {
    fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            range_calls: AtomicUsize::new(0),
            stream_calls: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl StorageDriver for PreviewMemoryRangeDriver {
    async fn put(&self, _path: &str, _data: &[u8]) -> Result<String> {
        Ok("memory".to_string())
    }

    async fn get(&self, _path: &str) -> Result<Vec<u8>> {
        Ok(self.data.clone())
    }

    async fn get_stream(
        &self,
        _path: &str,
    ) -> Result<Box<dyn tokio::io::AsyncRead + Unpin + Send>> {
        self.stream_calls.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(std::io::Cursor::new(self.data.clone())))
    }

    async fn get_range(
        &self,
        _path: &str,
        offset: u64,
        length: Option<u64>,
    ) -> Result<Box<dyn tokio::io::AsyncRead + Unpin + Send>> {
        self.range_calls.fetch_add(1, Ordering::SeqCst);
        let start =
            crate::utils::numbers::u64_to_usize(offset, "preview memory range start offset")?;
        let end = length
            .map(|len| {
                offset
                    .checked_add(len)
                    .ok_or_else(|| AsterError::internal_error("preview memory range end overflow"))
            })
            .transpose()?
            .map(|end| crate::utils::numbers::u64_to_usize(end, "preview memory range end offset"))
            .transpose()?
            .unwrap_or(self.data.len())
            .min(self.data.len());
        let bytes = if start >= self.data.len() {
            Vec::new()
        } else {
            self.data[start..end].to_vec()
        };
        Ok(Box::new(std::io::Cursor::new(bytes)))
    }

    fn supports_efficient_range(&self) -> bool {
        true
    }

    async fn delete(&self, _path: &str) -> Result<()> {
        Ok(())
    }

    async fn exists(&self, _path: &str) -> Result<bool> {
        Ok(true)
    }

    async fn metadata(&self, _path: &str) -> Result<BlobMetadata> {
        Ok(BlobMetadata {
            size: crate::utils::numbers::usize_to_u64(
                self.data.len(),
                "preview memory driver data length",
            )?,
            content_type: None,
        })
    }
}

fn failed_task_subcode(message: &str) -> Option<ApiSubcode> {
    map_failed_task_error(Some(message)).api_error_subcode()
}

fn preview_test_limits() -> ArchivePreviewLimits {
    let raw_signature = "raw-test".to_string();
    ArchivePreviewLimits {
        max_source_bytes: 1024 * 1024,
        max_manifest_bytes: 64 * 1024,
        max_duration_secs: 10,
        scan_limits: ZipScanLimits {
            max_uncompressed_bytes: 1024 * 1024,
            max_entries: 100,
            max_files: 100,
            max_directories: 100,
            max_depth: 16,
            max_path_bytes: 4096,
            max_compression_ratio: 100,
            max_entry_compression_ratio: 100,
        },
        raw_signature: raw_signature.clone(),
        task_signature: format!("{raw_signature};entries=100;files=100;dirs=100"),
        filename_encoding: ArchiveFilenameEncoding::Auto,
    }
}

fn preview_test_limits_with_encoding(
    filename_encoding: ArchiveFilenameEncoding,
) -> ArchivePreviewLimits {
    let mut limits = preview_test_limits();
    limits.filename_encoding = filename_encoding;
    limits
}

fn preview_test_file(size: i64) -> file::Model {
    let now = Utc::now();
    file::Model {
        id: 7,
        name: "bundle.zip".to_string(),
        folder_id: None,
        team_id: None,
        blob_id: 9,
        size,
        owner_user_id: Some(1),
        created_by_user_id: Some(1),
        created_by_username: "tester".to_string(),
        mime_type: "application/zip".to_string(),
        extension: "zip".to_string(),
        compound_extension: None,
        file_category: crate::types::FileCategory::Archive,
        created_at: now,
        updated_at: now,
        deleted_at: None,
        is_locked: false,
    }
}

fn preview_test_blob(size: i64) -> file_blob::Model {
    let now = Utc::now();
    file_blob::Model {
        id: 9,
        hash: "hash".to_string(),
        size,
        policy_id: 1,
        storage_path: "blob.zip".to_string(),
        thumbnail_path: None,
        thumbnail_processor: None,
        thumbnail_version: None,
        ref_count: 1,
        created_at: now,
        updated_at: now,
    }
}

fn apply_runtime_config_value(
    runtime_config: &crate::config::RuntimeConfig,
    key: &str,
    value: &str,
) {
    runtime_config.apply(system_config::Model {
        id: 1,
        key: key.to_string(),
        value: value.to_string(),
        value_type: SystemConfigValueType::String,
        requires_restart: false,
        is_sensitive: false,
        source: SystemConfigSource::System,
        namespace: String::new(),
        category: "test".to_string(),
        description: "test".to_string(),
        updated_at: Utc::now(),
        updated_by: Some(1),
    });
}

fn preview_test_raw_manifest() -> ArchiveRawManifest {
    ArchiveRawManifest {
        schema_version: RAW_CACHE_SCHEMA_VERSION,
        format: FORMAT_ZIP.to_string(),
        source_blob_id: 9,
        source_hash: "hash".to_string(),
        generated_at: "2026-01-02T03:04:05Z".to_string(),
        entry_count: 1,
        file_count: 1,
        directory_count: 0,
        total_uncompressed_size: 5,
        entries: vec![ArchiveRawEntry {
            index: 0,
            raw_name: base64::engine::general_purpose::STANDARD.encode(b"readme.txt"),
            display_name: "readme.txt".to_string(),
            zip_utf8: false,
            kind: ArchivePreviewEntryKind::File,
            size: 5,
            compressed_size: 5,
            modified_at: None,
        }],
    }
}

#[test]
fn map_failed_task_error_reads_persisted_subcode_without_text_matching() {
    let stored = crate::errors::encode_api_error_subcode_message(
        ApiSubcode::ArchivePreviewInvalidZip,
        "worker changed this wording".to_string(),
    );

    let error = map_failed_task_error(Some(&stored));

    assert_eq!(
        error.api_error_subcode(),
        Some(ApiSubcode::ArchivePreviewInvalidZip)
    );
    assert_eq!(error.message(), "invalid zip archive");
}

#[test]
fn serialized_cache_uses_current_raw_schema_and_signature() {
    let serialized =
        serialize_cached_raw_manifest(9, "hash", "raw-limits", &preview_test_raw_manifest())
            .expect("cache should serialize");
    let value: serde_json::Value =
        serde_json::from_str(&serialized).expect("cache should parse as JSON");

    assert_eq!(RAW_CACHE_SCHEMA_VERSION, 1);
    assert_eq!(ZIP_RAW_MANIFEST_CACHE_NAME, "zip_raw_manifest.v1");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["limit_signature"], "raw-limits");
    assert!(value.get("filename_encoding").is_none());
    assert_eq!(value["manifest"]["schema_version"], 1);
    assert_eq!(
        value["manifest"]["entries"][0]["raw_name"],
        "cmVhZG1lLnR4dA=="
    );
}

#[test]
fn raw_signature_ignores_display_encoding() {
    let runtime_config = crate::config::RuntimeConfig::default();
    let auto =
        ArchivePreviewLimits::from_runtime_config(&runtime_config, ArchiveFilenameEncoding::Auto)
            .expect("auto limits should build");
    let gb18030 = ArchivePreviewLimits::from_runtime_config(
        &runtime_config,
        ArchiveFilenameEncoding::Gb18030,
    )
    .expect("GB18030 limits should build");

    assert_eq!(auto.raw_signature, gb18030.raw_signature);
    assert!(!auto.raw_signature.contains("filename_encoding"));
}

#[test]
fn raw_signature_ignores_display_count_limits_but_tracks_source_safety_limits() {
    let runtime_config = crate::config::RuntimeConfig::default();
    let baseline =
        ArchivePreviewLimits::from_runtime_config(&runtime_config, ArchiveFilenameEncoding::Auto)
            .expect("baseline limits should build");

    apply_runtime_config_value(
        &runtime_config,
        crate::config::definitions::ARCHIVE_PREVIEW_MAX_ENTRIES_KEY,
        "1",
    );
    let reduced_count =
        ArchivePreviewLimits::from_runtime_config(&runtime_config, ArchiveFilenameEncoding::Auto)
            .expect("reduced count limits should build");
    assert_eq!(baseline.raw_signature, reduced_count.raw_signature);
    assert_ne!(baseline.task_signature, reduced_count.task_signature);

    apply_runtime_config_value(
        &runtime_config,
        crate::config::definitions::ARCHIVE_PREVIEW_MAX_SOURCE_BYTES_KEY,
        "1",
    );
    let reduced_source =
        ArchivePreviewLimits::from_runtime_config(&runtime_config, ArchiveFilenameEncoding::Auto)
            .expect("reduced source limits should build");
    assert_ne!(baseline.raw_signature, reduced_source.raw_signature);
}

#[tokio::test(flavor = "multi_thread")]
async fn manifest_marks_preview_only_names_as_not_extract_compatible() {
    let bytes = {
        let cursor = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file("docs/name:with-colon.txt", options)
            .expect("file should start");
        zip.write_all(b"hello").expect("file should write");
        zip.finish().expect("zip should finish").into_inner()
    };
    let source_size =
        crate::utils::numbers::usize_to_i64(bytes.len(), "preview compatibility test zip size")
            .expect("test zip size should fit i64");
    let source_file = preview_test_file(source_size);
    let blob = preview_test_blob(source_size);
    let driver = Arc::new(PreviewMemoryRangeDriver::new(bytes));

    let raw_manifest =
        scan_manifest_from_storage_range(&source_file, &blob, driver, &preview_test_limits())
            .await
            .expect("preview scan should allow display-only names");
    let manifest = build_manifest_from_raw(source_file.id, &raw_manifest, &preview_test_limits())
        .expect("raw manifest should derive preview manifest");

    assert_eq!(manifest.entries[0].path, "docs/name:with-colon.txt");
    assert_eq!(
        manifest.extract_compatibility,
        ArchivePreviewExtractCompatibility::unsupported(
            ArchivePreviewExtractUnsupportedReason::UnsupportedEntryNames,
        )
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn raw_manifest_can_be_redecoded_without_rescanning_storage() {
    let raw_name = b"\xb2\xe2\xca\xd4.txt";
    let bytes = create_single_file_zip_with_raw_name(raw_name, b"hello");
    let source_size =
        crate::utils::numbers::usize_to_i64(bytes.len(), "preview recode test zip size")
            .expect("test zip size should fit i64");
    let source_file = preview_test_file(source_size);
    let blob = preview_test_blob(source_size);
    let driver = Arc::new(PreviewMemoryRangeDriver::new(bytes));

    let raw_manifest = scan_manifest_from_storage_range(
        &source_file,
        &blob,
        driver.clone(),
        &preview_test_limits(),
    )
    .await
    .expect("raw scan should succeed");
    assert!(driver.range_calls.load(Ordering::SeqCst) > 0);
    let range_calls_after_scan = driver.range_calls.load(Ordering::SeqCst);

    let auto_manifest = build_manifest_from_raw(
        source_file.id,
        &raw_manifest,
        &preview_test_limits_with_encoding(ArchiveFilenameEncoding::Auto),
    )
    .expect("auto manifest should build from raw cache");
    let gb18030_manifest = build_manifest_from_raw(
        source_file.id,
        &raw_manifest,
        &preview_test_limits_with_encoding(ArchiveFilenameEncoding::Gb18030),
    )
    .expect("GB18030 manifest should build from raw cache");

    assert_eq!(auto_manifest.entries[0].path, "测试.txt");
    assert_eq!(gb18030_manifest.entries[0].path, "测试.txt");
    assert_eq!(
        driver.range_calls.load(Ordering::SeqCst),
        range_calls_after_scan
    );
}

#[test]
fn raw_manifest_preserves_utf8_flag_validation_on_redecode() {
    let raw_manifest = ArchiveRawManifest {
        entries: vec![ArchiveRawEntry {
            index: 0,
            raw_name: base64::engine::general_purpose::STANDARD.encode(b"\x82ber.txt"),
            display_name: "�ber.txt".to_string(),
            zip_utf8: true,
            kind: ArchivePreviewEntryKind::File,
            size: 5,
            compressed_size: 5,
            modified_at: None,
        }],
        ..preview_test_raw_manifest()
    };

    let error = build_manifest_from_raw(7, &raw_manifest, &preview_test_limits())
        .expect_err("UTF-8 flagged invalid raw names should still fail from raw cache");

    assert_eq!(
        error.api_error_subcode(),
        Some(ApiSubcode::ArchivePreviewRejected)
    );
    assert!(error.message().contains("filename is not valid UTF-8"));
}

#[test]
fn raw_manifest_cache_can_be_redecoded_after_count_limits_are_lowered() {
    let mut raw_manifest = preview_test_raw_manifest();
    raw_manifest.entry_count = 2;
    raw_manifest.file_count = 2;
    raw_manifest.entries.push(ArchiveRawEntry {
        index: 1,
        raw_name: base64::engine::general_purpose::STANDARD.encode(b"second.txt"),
        display_name: "second.txt".to_string(),
        zip_utf8: false,
        kind: ArchivePreviewEntryKind::File,
        size: 5,
        compressed_size: 5,
        modified_at: None,
    });

    let mut limits = preview_test_limits();
    limits.scan_limits.max_entries = 1;
    limits.scan_limits.max_files = 1;

    let manifest = build_manifest_from_raw(7, &raw_manifest, &limits)
        .expect("cached raw manifest should survive stricter count limits");

    assert_eq!(manifest.entry_count, 2);
    assert_eq!(manifest.file_count, 2);
    assert_eq!(manifest.entries.len(), 2);
    assert!(!manifest.truncated);
}

#[test]
fn raw_manifest_cache_trims_entries_to_property_limit() {
    let mut raw_manifest = preview_test_raw_manifest();
    raw_manifest.entry_count = 200;
    raw_manifest.file_count = 200;
    raw_manifest.entries = (0..200)
        .map(|index| ArchiveRawEntry {
            index,
            raw_name: base64::engine::general_purpose::STANDARD.encode(format!(
                "very-long-cache-entry-{index:04}-{}.txt",
                "x".repeat(512)
            )),
            display_name: format!("very-long-cache-entry-{index:04}.txt"),
            zip_utf8: false,
            kind: ArchivePreviewEntryKind::File,
            size: 1,
            compressed_size: 1,
            modified_at: None,
        })
        .collect();

    let fitted = fit_raw_manifest_to_cache_limit(7, 9, "hash", "raw-limits", raw_manifest)
        .expect("oversized raw cache should be trimmed instead of rejected");
    let serialized = serialize_cached_raw_manifest(9, "hash", "raw-limits", &fitted)
        .expect("trimmed cache should serialize");

    assert!(fitted.entries_truncated());
    assert_eq!(fitted.entry_count, 200);
    assert!(serialized.len() <= ENTITY_PROPERTY_VALUE_MAX_BYTES);
}

#[test]
fn manifest_from_truncated_raw_cache_keeps_totals_and_marks_truncated() {
    let mut raw_manifest = preview_test_raw_manifest();
    raw_manifest.entry_count = 2;
    raw_manifest.file_count = 2;

    let manifest = build_manifest_from_raw(7, &raw_manifest, &preview_test_limits())
        .expect("truncated raw cache should still produce a preview manifest");

    assert_eq!(manifest.entry_count, 2);
    assert_eq!(manifest.file_count, 2);
    assert_eq!(manifest.entries.len(), 1);
    assert!(manifest.truncated);
    assert_eq!(
        manifest.extract_compatibility,
        ArchivePreviewExtractCompatibility::unsupported(
            ArchivePreviewExtractUnsupportedReason::UnsupportedEntryNames,
        )
    );
}

#[test]
fn map_failed_task_error_preserves_archive_preview_subcodes() {
    assert_eq!(
        failed_task_subcode("archive preview currently supports .zip files only"),
        Some(ApiSubcode::ArchivePreviewUnsupportedType)
    );
    assert_eq!(
        failed_task_subcode("source archive size 135064658 exceeds archive preview limit 67108864"),
        Some(ApiSubcode::ArchivePreviewSourceTooLarge)
    );
    assert_eq!(
        failed_task_subcode("invalid zip archive"),
        Some(ApiSubcode::ArchivePreviewInvalidZip)
    );
    assert_eq!(
        failed_task_subcode("archive preview manifest for file #1 exceeds server limit 64 bytes"),
        Some(ApiSubcode::ArchivePreviewManifestTooLarge)
    );
    assert_eq!(
        failed_task_subcode("source archive size mismatch: declared 3 bytes, downloaded 2 bytes"),
        Some(ApiSubcode::ArchivePreviewSourceSizeMismatch)
    );
    assert_eq!(
        failed_task_subcode("archive contains 2 entries, exceeds server limit 1"),
        Some(ApiSubcode::ArchivePreviewRejected)
    );
}

#[test]
fn map_failed_task_error_falls_back_to_unavailable_when_unknown() {
    let error = map_failed_task_error(Some("worker disappeared"));

    assert_eq!(error.code(), "E006");
    assert_eq!(
        error.message(),
        "archive preview is unavailable for this file"
    );
}

#[tokio::test]
async fn bounded_copy_accepts_exact_size_and_preserves_bytes() {
    let (mut writer, mut reader) = tokio::io::duplex(16);
    let producer = tokio::spawn(async move {
        writer
            .write_all(b"zip")
            .await
            .expect("write should succeed");
    });
    let mut output = Vec::new();

    let copied = copy_async_reader_to_writer_with_expected_size(
        &mut reader,
        &mut output,
        3,
        "source archive",
    )
    .await
    .expect("exact-size stream should copy");

    producer.await.expect("producer should finish");
    assert_eq!(copied, 3);
    assert_eq!(output, b"zip");
}

#[tokio::test]
async fn bounded_copy_rejects_short_and_long_streams() {
    let mut short_reader = tokio::io::empty();
    let mut short_output = Vec::new();
    let short_error = copy_async_reader_to_writer_with_expected_size(
        &mut short_reader,
        &mut short_output,
        1,
        "source archive",
    )
    .await
    .expect_err("short stream should fail");
    assert_eq!(
        short_error.api_error_subcode(),
        Some(ApiSubcode::ArchivePreviewSourceSizeMismatch)
    );
    assert!(short_error.message().contains("downloaded 0 bytes"));

    let (mut writer, mut reader) = tokio::io::duplex(16);
    let producer = tokio::spawn(async move {
        writer
            .write_all(b"too-long")
            .await
            .expect("write should succeed");
    });
    let mut long_output = Vec::new();
    let long_error = copy_async_reader_to_writer_with_expected_size(
        &mut reader,
        &mut long_output,
        3,
        "source archive",
    )
    .await
    .expect_err("long stream should fail");

    producer.await.expect("producer should finish");
    assert_eq!(
        long_error.api_error_subcode(),
        Some(ApiSubcode::ArchivePreviewSourceSizeMismatch)
    );
    assert!(
        long_error
            .message()
            .contains("expands beyond declared size")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn range_manifest_scan_uses_get_range_without_full_stream() {
    let bytes = {
        let cursor = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.add_directory("docs/", options)
            .expect("directory should be added");
        zip.start_file("docs/readme.txt", options)
            .expect("file should start");
        zip.write_all(b"hello").expect("file should write");
        zip.finish().expect("zip should finish").into_inner()
    };
    let source_size =
        crate::utils::numbers::usize_to_i64(bytes.len(), "preview range test zip size")
            .expect("test zip size should fit i64");
    let source_file = preview_test_file(source_size);
    let blob = preview_test_blob(source_size);
    let driver = Arc::new(PreviewMemoryRangeDriver::new(bytes));

    let manifest = scan_manifest_from_storage_range(
        &source_file,
        &blob,
        driver.clone(),
        &preview_test_limits(),
    )
    .await
    .expect("range manifest scan should succeed");

    assert_eq!(manifest.entry_count, 2);
    assert_eq!(manifest.file_count, 1);
    assert_eq!(manifest.directory_count, 1);
    assert_eq!(driver.stream_calls.load(Ordering::SeqCst), 0);
    assert!(driver.range_calls.load(Ordering::SeqCst) > 0);
}

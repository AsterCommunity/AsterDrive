//! Multipart storage driver default reader integration tests.

use aster_drive::errors::Result;
use aster_drive::storage::{MultipartStorageDriver, UploadedMultipartPart};
use async_trait::async_trait;
use std::sync::Mutex;
use std::time::Duration;

struct CapturingMultipartDriver {
    uploaded: Mutex<Vec<u8>>,
}

impl CapturingMultipartDriver {
    fn new() -> Self {
        Self {
            uploaded: Mutex::new(Vec::new()),
        }
    }

    fn uploaded(&self) -> Vec<u8> {
        self.uploaded
            .lock()
            .expect("uploaded lock should not poison")
            .clone()
    }
}

#[async_trait]
impl MultipartStorageDriver for CapturingMultipartDriver {
    async fn create_multipart_upload(&self, _path: &str) -> Result<String> {
        panic!("not used")
    }

    async fn presigned_upload_part_url(
        &self,
        _path: &str,
        _upload_id: &str,
        _part_number: i32,
        _expires: Duration,
    ) -> Result<String> {
        panic!("not used")
    }

    async fn complete_multipart_upload(
        &self,
        _path: &str,
        _upload_id: &str,
        _parts: Vec<(i32, String)>,
    ) -> Result<()> {
        panic!("not used")
    }

    async fn upload_multipart_part(
        &self,
        _path: &str,
        _upload_id: &str,
        _part_number: i32,
        data: &[u8],
    ) -> Result<String> {
        *self
            .uploaded
            .lock()
            .expect("uploaded lock should not poison") = data.to_vec();
        Ok("\"etag\"".to_string())
    }

    async fn abort_multipart_upload(&self, _path: &str, _upload_id: &str) -> Result<()> {
        panic!("not used")
    }

    async fn list_uploaded_part_details(
        &self,
        _path: &str,
        _upload_id: &str,
    ) -> Result<Vec<UploadedMultipartPart>> {
        panic!("not used")
    }
}

#[tokio::test]
async fn default_reader_upload_accepts_exact_size_stream() {
    let driver = CapturingMultipartDriver::new();

    let etag = driver
        .upload_multipart_part_reader(
            "path",
            "upload",
            1,
            Box::new(std::io::Cursor::new(b"abcd".to_vec())),
            4,
        )
        .await
        .expect("exact stream should upload");

    assert_eq!(etag, "\"etag\"");
    assert_eq!(driver.uploaded(), b"abcd".to_vec());
}

#[tokio::test]
async fn default_reader_upload_rejects_stream_larger_than_size() {
    let driver = CapturingMultipartDriver::new();

    let error = driver
        .upload_multipart_part_reader(
            "path",
            "upload",
            1,
            Box::new(std::io::Cursor::new(b"abcde".to_vec())),
            4,
        )
        .await
        .expect_err("oversized stream should fail");

    assert_eq!(error.code(), "E031");
    assert!(
        error
            .message()
            .contains("multipart part stream exceeds expected size 4")
    );
    assert!(driver.uploaded().is_empty());
}

#[tokio::test]
async fn default_reader_upload_rejects_stream_shorter_than_size() {
    let driver = CapturingMultipartDriver::new();

    let error = driver
        .upload_multipart_part_reader(
            "path",
            "upload",
            1,
            Box::new(std::io::Cursor::new(b"abc".to_vec())),
            4,
        )
        .await
        .expect_err("short stream should fail");

    assert_eq!(error.code(), "E031");
    assert!(
        error
            .message()
            .contains("multipart part stream ended before expected size 4")
    );
    assert!(driver.uploaded().is_empty());
}

#[tokio::test]
async fn default_reader_upload_enforces_zero_size_boundary() {
    let empty_driver = CapturingMultipartDriver::new();
    empty_driver
        .upload_multipart_part_reader(
            "path",
            "upload",
            1,
            Box::new(std::io::Cursor::new(Vec::new())),
            0,
        )
        .await
        .expect("empty stream should upload for zero size");
    assert!(empty_driver.uploaded().is_empty());

    let extra_driver = CapturingMultipartDriver::new();
    let error = extra_driver
        .upload_multipart_part_reader(
            "path",
            "upload",
            1,
            Box::new(std::io::Cursor::new(b"x".to_vec())),
            0,
        )
        .await
        .expect_err("zero-size stream with data should fail");

    assert_eq!(error.code(), "E031");
    assert!(
        error
            .message()
            .contains("multipart part stream exceeds expected size 0")
    );
    assert!(extra_driver.uploaded().is_empty());
}

#[tokio::test]
async fn default_reader_upload_rejects_negative_size_before_upload() {
    let driver = CapturingMultipartDriver::new();

    let error = driver
        .upload_multipart_part_reader(
            "path",
            "upload",
            1,
            Box::new(std::io::Cursor::new(Vec::new())),
            -1,
        )
        .await
        .expect_err("negative size should fail");

    assert_eq!(error.code(), "E004");
    assert!(driver.uploaded().is_empty());
}

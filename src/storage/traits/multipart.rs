//! multipart upload 抽象层。
//!
//! Multipart upload 常见于对象存储直传场景；本地存储不支持。
//! 将其隔离在 `MultipartStorageDriver` 子 trait 中，避免 `StorageDriver` trait
//! 被 upload_id / part_number / ETag 等直传语义污染。

use crate::errors::{AsterError, MapAsterErr, Result};
use async_trait::async_trait;
use bytes::Bytes;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};

const DEFAULT_MULTIPART_READER_BUFFER_SIZE: usize = 64 * 1024;

/// Multipart upload 支持。
///
/// 调用方通过 `driver.as_multipart()` 获取引用。
/// **调用方必须确保 session 携带了 multipart 关联标识**，否则不应该调用此方法。
#[async_trait]
pub trait MultipartStorageDriver: Send + Sync {
    /// 创建 multipart upload，返回 provider 端的 upload_id
    async fn create_multipart_upload(&self, path: &str) -> Result<String>;

    /// 为指定 part 生成 presigned PUT URL
    async fn presigned_upload_part_url(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        expires: Duration,
    ) -> Result<String>;

    /// 完成 multipart upload（parts: Vec<(part_number, etag)>）
    async fn complete_multipart_upload(
        &self,
        path: &str,
        upload_id: &str,
        parts: Vec<(i32, String)>,
    ) -> Result<()>;

    /// 服务端直接上传一个 multipart part，返回该 part 的 ETag
    async fn upload_multipart_part(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        data: &[u8],
    ) -> Result<String>;

    /// 服务端直接上传一个 multipart part，接收拥有所有权的 Bytes。
    ///
    /// HTTP relay 上传入口已经拿到 `web::Bytes`，S3 等驱动可以覆盖该方法直接构造
    /// provider body，避免先退回 `&[u8]` 再复制成 `Vec<u8>`。
    async fn upload_multipart_part_bytes(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        data: Bytes,
    ) -> Result<String> {
        self.upload_multipart_part(path, upload_id, part_number, &data)
            .await
    }

    /// 服务端直接流式上传一个 multipart part，返回该 part 的 ETag。
    async fn upload_multipart_part_reader(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> Result<String> {
        let mut reader = reader;
        let expected_size = crate::utils::numbers::bytes_to_usize(size, "multipart part size")?;
        let mut data = Vec::with_capacity(expected_size);
        let mut buffer = vec![0u8; DEFAULT_MULTIPART_READER_BUFFER_SIZE.min(expected_size.max(1))];

        while data.len() < expected_size {
            let remaining = expected_size - data.len();
            let read_len = buffer.len().min(remaining);
            let read = reader
                .read(&mut buffer[..read_len])
                .await
                .map_aster_err_ctx(
                    "read multipart part stream",
                    AsterError::storage_driver_error,
                )?;
            if read == 0 {
                break;
            }
            data.extend_from_slice(&buffer[..read]);
        }

        if data.len() < expected_size {
            return Err(AsterError::storage_driver_error(format!(
                "read multipart part stream: multipart part stream ended before expected size {size}"
            )));
        }

        let mut extra = [0u8; 1];
        let extra_read = reader.read(&mut extra).await.map_aster_err_ctx(
            "read multipart part stream",
            AsterError::storage_driver_error,
        )?;
        if extra_read > 0 {
            return Err(AsterError::storage_driver_error(format!(
                "read multipart part stream: multipart part stream exceeds expected size {size}"
            )));
        }

        self.upload_multipart_part(path, upload_id, part_number, &data)
            .await
    }

    /// 取消 multipart upload（清理已上传的 parts）
    async fn abort_multipart_upload(&self, path: &str, upload_id: &str) -> Result<()>;

    /// 列出已上传的 parts（返回 part numbers，用于断点续传进度查询）
    async fn list_uploaded_parts(&self, path: &str, upload_id: &str) -> Result<Vec<i32>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct CapturingMultipartDriver {
        uploaded: Mutex<Vec<u8>>,
    }

    impl CapturingMultipartDriver {
        fn new() -> Self {
            Self {
                uploaded: Mutex::new(Vec::new()),
            }
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

        async fn list_uploaded_parts(&self, _path: &str, _upload_id: &str) -> Result<Vec<i32>> {
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
        assert_eq!(
            *driver
                .uploaded
                .lock()
                .expect("uploaded lock should not poison"),
            b"abcd".to_vec()
        );
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
        assert!(
            driver
                .uploaded
                .lock()
                .expect("uploaded lock should not poison")
                .is_empty()
        );
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
        assert!(
            driver
                .uploaded
                .lock()
                .expect("uploaded lock should not poison")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn default_reader_upload_accepts_empty_stream_for_zero_size() {
        let driver = CapturingMultipartDriver::new();

        let etag = driver
            .upload_multipart_part_reader(
                "path",
                "upload",
                1,
                Box::new(std::io::Cursor::new(Vec::new())),
                0,
            )
            .await
            .expect("empty stream should upload for zero size");

        assert_eq!(etag, "\"etag\"");
        assert!(
            driver
                .uploaded
                .lock()
                .expect("uploaded lock should not poison")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn default_reader_upload_rejects_extra_byte_for_zero_size() {
        let driver = CapturingMultipartDriver::new();

        let error = driver
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
        assert!(
            driver
                .uploaded
                .lock()
                .expect("uploaded lock should not poison")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn default_reader_upload_rejects_negative_size_before_reading() {
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
        assert!(
            driver
                .uploaded
                .lock()
                .expect("uploaded lock should not poison")
                .is_empty()
        );
    }
}

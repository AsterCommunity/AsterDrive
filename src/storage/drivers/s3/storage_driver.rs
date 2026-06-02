use async_trait::async_trait;
use aws_sdk_s3::primitives::ByteStream;
use tokio::io::AsyncRead;

use crate::errors::{MapAsterErr, Result};
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::storage::traits::driver::{BlobMetadata, StorageDriver};
use crate::storage::traits::extensions::{
    ListStorageDriver, PresignedStorageDriver, StorageCapacityInfo, StreamUploadDriver,
};
use crate::storage::traits::multipart::MultipartStorageDriver;
use crate::utils::numbers;

use super::S3Driver;

// =============================================================================
// StorageDriver 核心 trait
// =============================================================================

#[async_trait]
impl StorageDriver for S3Driver {
    async fn put(&self, path: &str, data: &[u8]) -> Result<String> {
        let key = self.full_key(path);
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(ByteStream::from(data.to_vec()))
            .send()
            .await
            .map_err(|err| Self::map_sdk_error("S3 put failed", err))?;
        Ok(path.to_string())
    }

    async fn get(&self, path: &str) -> Result<Vec<u8>> {
        let key = self.full_key(path);
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|err| Self::map_sdk_error("S3 get failed", err))?;

        let bytes = resp
            .body
            .collect()
            .await
            .map_aster_err_ctx("S3 read body failed", |message| {
                storage_driver_error(StorageErrorKind::Transient, message)
            })?
            .into_bytes();

        Ok(bytes.to_vec())
    }

    async fn get_stream(&self, path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        let key = self.full_key(path);
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|err| Self::map_sdk_error("S3 get_stream failed", err))?;

        Ok(Box::new(resp.body.into_async_read()))
    }

    async fn get_range(
        &self,
        path: &str,
        offset: u64,
        length: Option<u64>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        let key = self.full_key(path);
        // HTTP Range 规范使用闭区间 [start, end]
        let range = match length {
            Some(len) if len > 0 => format!("bytes={}-{}", offset, offset + len - 1),
            Some(_) => {
                // 0 长度：直接返回空流，避免给 S3 发 "bytes=X-(X-1)" 这种非法 range
                return Ok(Box::new(tokio::io::empty()));
            }
            None => format!("bytes={offset}-"),
        };
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&key)
            .range(range)
            .send()
            .await
            .map_err(|err| Self::map_sdk_error("S3 get_range failed", err))?;

        Ok(Box::new(resp.body.into_async_read()))
    }

    fn supports_efficient_range(&self) -> bool {
        true
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let key = self.full_key(path);
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|err| Self::map_sdk_error("S3 delete failed", err))?;
        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let key = self.full_key(path);
        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => {
                if e.as_service_error().map(|svc_err| svc_err.is_not_found()) == Some(true) {
                    Ok(false)
                } else {
                    Err(Self::map_sdk_error("S3 exists check failed", e))
                }
            }
        }
    }

    async fn metadata(&self, path: &str) -> Result<BlobMetadata> {
        let key = self.full_key(path);
        let resp = self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|err| Self::map_sdk_error("S3 head failed", err))?;

        let size = resp
            .content_length
            .map(|value| numbers::i64_to_u64(value, "S3 content_length"))
            .transpose()
            .map_err(Self::rewrap_message_as_storage_error)?
            .unwrap_or(0);

        Ok(BlobMetadata {
            size,
            content_type: resp.content_type,
        })
    }

    async fn copy_object(&self, src_path: &str, dest_path: &str) -> Result<String> {
        let src_key = self.full_key(src_path);
        let dest_key = self.full_key(dest_path);
        // CopySource 形如 "{bucket}/{key}"，bucket 与 key 中的特殊字符（空格、中文、
        // `+`、`#` 等）必须做 percent-encoding，否则 SigV4 会拒绝签名。
        // 复用 aws-smithy-http 的 httpLabel Greedy 编码器，与 SDK 内部对 S3 key
        // 的编码策略保持一致（保留 `/` 作为分隔符）。
        use aws_smithy_http::label::{EncodingStrategy, fmt_string};
        let copy_source = format!(
            "{}/{}",
            fmt_string(&self.bucket, EncodingStrategy::Greedy),
            fmt_string(&src_key, EncodingStrategy::Greedy),
        );

        self.client
            .copy_object()
            .bucket(&self.bucket)
            .copy_source(&copy_source)
            .key(&dest_key)
            .send()
            .await
            .map_err(|err| Self::map_sdk_error("S3 copy_object failed", err))?;

        Ok(dest_path.to_string())
    }

    async fn capacity_info(&self) -> Result<StorageCapacityInfo> {
        Err(storage_driver_error(
            StorageErrorKind::Unsupported,
            "S3-compatible storage does not expose standardized bucket capacity information",
        ))
    }

    fn as_presigned(&self) -> Option<&dyn PresignedStorageDriver> {
        Some(self)
    }

    fn as_list(&self) -> Option<&dyn ListStorageDriver> {
        Some(self)
    }

    fn as_stream_upload(&self) -> Option<&dyn StreamUploadDriver> {
        Some(self)
    }

    fn as_multipart(&self) -> Option<&dyn MultipartStorageDriver> {
        Some(self)
    }
}

use async_trait::async_trait;
use aws_sdk_s3::primitives::ByteStream;
use tokio::io::AsyncRead;

use aster_drive_storage::traits::driver::{BlobMetadata, StorageDriver};
use aster_drive_storage::traits::extensions::StorageCapacityInfo;
use aster_drive_storage::{MapStorageErr, StorageErrorKind, storage_driver_error};
use aster_forge_utils::numbers;

use super::S3Driver;

const S3_SINGLE_COPY_LIMIT: u64 = 5 * 1024 * 1024 * 1024;
const S3_COPY_PART_SIZE: u64 = 5 * 1024 * 1024 * 1024;
const S3_COPY_MAX_PARTS: u64 = 10_000;

// =============================================================================
// StorageDriver 核心 trait
// =============================================================================

#[async_trait]
impl StorageDriver for S3Driver {
    async fn put(&self, path: &str, data: &[u8]) -> aster_drive_storage::Result<String> {
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

    async fn get(&self, path: &str) -> aster_drive_storage::Result<Vec<u8>> {
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
            .map_storage_err_ctx(StorageErrorKind::Transient, "S3 read body failed")?
            .into_bytes();

        Ok(bytes.to_vec())
    }

    async fn get_stream(
        &self,
        path: &str,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
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
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
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

    fn max_single_put_size(&self) -> Option<u64> {
        Some(5 * 1024 * 1024 * 1024)
    }

    async fn delete(&self, path: &str) -> aster_drive_storage::Result<()> {
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

    async fn exists(&self, path: &str) -> aster_drive_storage::Result<bool> {
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

    async fn metadata(&self, path: &str) -> aster_drive_storage::Result<BlobMetadata> {
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
            .map_storage_err(StorageErrorKind::Misconfigured)?
            .unwrap_or(0);

        Ok(BlobMetadata {
            size,
            content_type: resp.content_type,
        })
    }

    async fn copy_object(
        &self,
        src_path: &str,
        dest_path: &str,
    ) -> aster_drive_storage::Result<String> {
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
        let source_metadata = self.metadata(src_path).await?;
        let size = source_metadata.size;
        if size <= S3_SINGLE_COPY_LIMIT {
            self.client
                .copy_object()
                .bucket(&self.bucket)
                .copy_source(&copy_source)
                .key(&dest_key)
                .send()
                .await
                .map_err(|err| Self::map_sdk_error("S3 copy_object failed", err))?;
            return Ok(dest_path.to_string());
        }

        let mut create = self
            .client
            .create_multipart_upload()
            .bucket(&self.bucket)
            .key(&dest_key);
        if let Some(content_type) = source_metadata.content_type.as_deref() {
            create = create.content_type(content_type);
        }
        let upload_id = create
            .send()
            .await
            .map_err(|err| Self::map_sdk_error("S3 create multipart copy failed", err))?
            .upload_id()
            .map(str::to_owned)
            .ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Unknown,
                    "S3 multipart copy missing upload_id",
                )
            })?;
        let result = self
            .multipart_copy(&copy_source, &dest_key, &upload_id, size)
            .await;
        if let Err(error) = result {
            if let Err(abort_error) = self
                .client
                .abort_multipart_upload()
                .bucket(&self.bucket)
                .key(&dest_key)
                .upload_id(&upload_id)
                .send()
                .await
                .map_err(|err| Self::map_sdk_error("S3 abort multipart copy failed", err))
            {
                tracing::warn!(
                    destination = dest_path,
                    "failed to abort S3 multipart copy: {abort_error}"
                );
            }
            return Err(error);
        }
        Ok(dest_path.to_string())
    }

    async fn capacity_info(&self) -> aster_drive_storage::Result<StorageCapacityInfo> {
        Err(storage_driver_error(
            StorageErrorKind::Unsupported,
            "S3-compatible storage does not expose standardized bucket capacity information",
        ))
    }

    fn extensions(&self) -> aster_drive_storage::traits::StorageDriverExtensions<'_> {
        aster_drive_storage::traits::StorageDriverExtensions {
            direct_download: Some(self),
            presigned_upload: Some(self),
            list: Some(self),
            stream_upload: Some(self),
            multipart: Some(self),
            ..Default::default()
        }
    }
}

impl S3Driver {
    async fn multipart_copy(
        &self,
        copy_source: &str,
        dest_key: &str,
        upload_id: &str,
        size: u64,
    ) -> aster_drive_storage::Result<()> {
        use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};

        let ranges = multipart_copy_ranges(size)?;
        let mut completed = Vec::with_capacity(ranges.len());
        for (index, (start, end)) in ranges.into_iter().enumerate() {
            let part_number = i32::try_from(index + 1).map_err(|_| {
                storage_driver_error(
                    StorageErrorKind::Precondition,
                    "S3 multipart copy part number exceeds i32",
                )
            })?;
            let response = self
                .client
                .upload_part_copy()
                .bucket(&self.bucket)
                .key(dest_key)
                .upload_id(upload_id)
                .part_number(part_number)
                .copy_source(copy_source)
                .copy_source_range(format!("bytes={start}-{end}"))
                .send()
                .await
                .map_err(|err| Self::map_sdk_error("S3 upload part copy failed", err))?;
            let etag = response
                .copy_part_result()
                .and_then(|part| part.e_tag())
                .ok_or_else(|| {
                    storage_driver_error(
                        StorageErrorKind::Unknown,
                        "S3 multipart copy missing part ETag",
                    )
                })?;
            completed.push(
                CompletedPart::builder()
                    .part_number(part_number)
                    .e_tag(Self::normalize_multipart_etag(etag))
                    .build(),
            );
        }
        self.client
            .complete_multipart_upload()
            .bucket(&self.bucket)
            .key(dest_key)
            .upload_id(upload_id)
            .multipart_upload(
                CompletedMultipartUpload::builder()
                    .set_parts(Some(completed))
                    .build(),
            )
            .send()
            .await
            .map_err(|err| Self::map_sdk_error("S3 complete multipart copy failed", err))?;
        Ok(())
    }
}

fn multipart_copy_ranges(size: u64) -> aster_drive_storage::Result<Vec<(u64, u64)>> {
    if size <= S3_SINGLE_COPY_LIMIT {
        return Err(storage_driver_error(
            StorageErrorKind::Precondition,
            "S3 multipart copy planner requires an object larger than the single-copy limit",
        ));
    }
    let part_count = size.div_ceil(S3_COPY_PART_SIZE);
    if part_count > S3_COPY_MAX_PARTS {
        return Err(storage_driver_error(
            StorageErrorKind::Precondition,
            "S3 multipart copy exceeds the 10,000-part limit",
        ));
    }
    let capacity = usize::try_from(part_count).map_err(|_| {
        storage_driver_error(
            StorageErrorKind::Precondition,
            "S3 part count exceeds usize",
        )
    })?;
    let mut ranges = Vec::with_capacity(capacity);
    let mut start = 0_u64;
    while start < size {
        let end = start
            .saturating_add(S3_COPY_PART_SIZE)
            .saturating_sub(1)
            .min(size - 1);
        ranges.push((start, end));
        start = end + 1;
    }
    Ok(ranges)
}

#[cfg(test)]
mod copy_tests {
    use super::{
        S3_COPY_MAX_PARTS, S3_COPY_PART_SIZE, S3_SINGLE_COPY_LIMIT, multipart_copy_ranges,
    };

    #[test]
    fn multipart_copy_ranges_cover_boundaries_without_gaps_or_overlap() {
        assert!(multipart_copy_ranges(S3_SINGLE_COPY_LIMIT).is_err());
        assert_eq!(
            multipart_copy_ranges(S3_SINGLE_COPY_LIMIT + 1).unwrap(),
            vec![
                (0, S3_COPY_PART_SIZE - 1),
                (S3_COPY_PART_SIZE, S3_COPY_PART_SIZE)
            ]
        );

        let size = S3_COPY_PART_SIZE * 2 + 7;
        assert_eq!(
            multipart_copy_ranges(size).unwrap(),
            vec![
                (0, S3_COPY_PART_SIZE - 1),
                (S3_COPY_PART_SIZE, S3_COPY_PART_SIZE * 2 - 1),
                (S3_COPY_PART_SIZE * 2, size - 1),
            ]
        );
        assert!(multipart_copy_ranges(S3_COPY_PART_SIZE * (S3_COPY_MAX_PARTS + 1)).is_err());
    }
}

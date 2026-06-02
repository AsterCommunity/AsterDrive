use std::time::Duration;

use async_trait::async_trait;
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::primitives::ByteStream;
use bytes::Bytes;
use tokio::io::AsyncRead;

use crate::errors::{MapAsterErr, Result};
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::storage::traits::multipart::MultipartStorageDriver;

use super::S3Driver;
use super::presigned::clamp_presign_ttl;

// =============================================================================
// MultipartStorageDriver 扩展
// =============================================================================

#[async_trait]
impl MultipartStorageDriver for S3Driver {
    async fn create_multipart_upload(&self, path: &str) -> Result<String> {
        let key = self.full_key(path);
        let resp = self
            .client
            .create_multipart_upload()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|err| Self::map_sdk_error("S3 create_multipart_upload failed", err))?;

        resp.upload_id().map(|s| s.to_string()).ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Unknown,
                "S3 multipart upload: missing upload_id",
            )
        })
    }

    async fn presigned_upload_part_url(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        expires: Duration,
    ) -> Result<String> {
        let key = self.full_key(path);
        let presign_config = PresigningConfig::builder()
            .expires_in(clamp_presign_ttl(expires, "S3 presigned_upload_part_url"))
            .build()
            .map_aster_err_ctx("presign config", |message| {
                storage_driver_error(StorageErrorKind::Misconfigured, message)
            })?;

        let url = self
            .client
            .upload_part()
            .bucket(&self.bucket)
            .key(&key)
            .upload_id(upload_id)
            .part_number(part_number)
            .presigned(presign_config)
            .await
            .map_aster_err_ctx("S3 presigned upload_part failed", |message| {
                storage_driver_error(StorageErrorKind::Misconfigured, message)
            })?;

        Ok(url.uri().to_string())
    }

    async fn complete_multipart_upload(
        &self,
        path: &str,
        upload_id: &str,
        parts: Vec<(i32, String)>,
    ) -> Result<()> {
        use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};

        let completed_parts: Vec<CompletedPart> = parts
            .into_iter()
            .map(|(num, etag)| {
                CompletedPart::builder()
                    .part_number(num)
                    .e_tag(Self::normalize_multipart_etag(&etag))
                    .build()
            })
            .collect();

        let key = self.full_key(path);
        self.client
            .complete_multipart_upload()
            .bucket(&self.bucket)
            .key(&key)
            .upload_id(upload_id)
            .multipart_upload(
                CompletedMultipartUpload::builder()
                    .set_parts(Some(completed_parts))
                    .build(),
            )
            .send()
            .await
            .map_err(|err| Self::map_sdk_error("S3 complete_multipart_upload failed", err))?;

        Ok(())
    }

    async fn upload_multipart_part(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        data: &[u8],
    ) -> Result<String> {
        self.upload_multipart_part_bytes(path, upload_id, part_number, Bytes::copy_from_slice(data))
            .await
    }

    async fn upload_multipart_part_bytes(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        data: Bytes,
    ) -> Result<String> {
        let key = self.full_key(path);
        let resp = self
            .client
            .upload_part()
            .bucket(&self.bucket)
            .key(&key)
            .upload_id(upload_id)
            .part_number(part_number)
            .body(ByteStream::from(data))
            .send()
            .await
            .map_err(|err| Self::map_sdk_error("S3 upload_part failed", err))?;

        resp.e_tag().map(str::to_string).ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Unknown,
                "S3 multipart upload: missing ETag",
            )
        })
    }

    async fn upload_multipart_part_reader(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> Result<String> {
        let key = self.full_key(path);
        let content_length = crate::utils::numbers::i64_to_u64(size, "S3 multipart part size")?;
        let body = ByteStream::from_body_1_x(super::stream_upload::SizedReaderBody::new(
            reader,
            content_length,
        ));
        let resp = self
            .client
            .upload_part()
            .bucket(&self.bucket)
            .key(&key)
            .upload_id(upload_id)
            .part_number(part_number)
            .content_length(size)
            .body(body)
            .send()
            .await
            .map_err(|err| Self::map_sdk_error("S3 upload_part failed", err))?;

        resp.e_tag().map(str::to_string).ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Unknown,
                "S3 multipart upload: missing ETag",
            )
        })
    }

    async fn abort_multipart_upload(&self, path: &str, upload_id: &str) -> Result<()> {
        let key = self.full_key(path);
        self.client
            .abort_multipart_upload()
            .bucket(&self.bucket)
            .key(&key)
            .upload_id(upload_id)
            .send()
            .await
            .map_err(|err| Self::map_sdk_error("S3 abort_multipart_upload failed", err))?;
        Ok(())
    }

    async fn list_uploaded_parts(&self, path: &str, upload_id: &str) -> Result<Vec<i32>> {
        let key = self.full_key(path);
        let mut part_numbers = Vec::new();
        let mut part_marker: Option<String> = None;

        loop {
            let mut req = self
                .client
                .list_parts()
                .bucket(&self.bucket)
                .key(&key)
                .upload_id(upload_id);
            if let Some(marker) = &part_marker {
                req = req.part_number_marker(marker.as_str());
            }

            let resp = req
                .send()
                .await
                .map_err(|err| Self::map_sdk_error("S3 list_parts failed", err))?;

            for part in resp.parts() {
                part_numbers.push(part.part_number.unwrap_or(0));
            }

            if resp.is_truncated() == Some(true) {
                part_marker = resp.next_part_number_marker().map(|s| s.to_string());
            } else {
                break;
            }
        }

        Ok(part_numbers)
    }
}

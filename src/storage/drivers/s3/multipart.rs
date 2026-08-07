use std::time::Duration;

use async_trait::async_trait;
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::primitives::ByteStream;
use bytes::Bytes;
use tokio::io::AsyncRead;

use aster_drive_storage::PresignedUploadRequest;
use aster_drive_storage::traits::multipart::{MultipartStorageDriver, UploadedMultipartPart};
use aster_drive_storage::{MapStorageErr, StorageErrorKind, storage_driver_error};

use super::S3Driver;
use super::presigned::{clamp_presign_ttl, sdk_presigned_upload_request};

// =============================================================================
// MultipartStorageDriver 扩展
// =============================================================================

#[async_trait]
impl MultipartStorageDriver for S3Driver {
    async fn create_multipart_upload(&self, path: &str) -> aster_drive_storage::Result<String> {
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

    async fn presigned_upload_part_request(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        expires: Duration,
    ) -> aster_drive_storage::Result<PresignedUploadRequest> {
        let key = self.full_key(path);
        let presign_config = PresigningConfig::builder()
            .expires_in(clamp_presign_ttl(
                expires,
                "S3 presigned_upload_part_request",
            ))
            .build()
            .map_storage_err_ctx(StorageErrorKind::Misconfigured, "presign config")?;

        let url = self
            .client
            .upload_part()
            .bucket(&self.bucket)
            .key(&key)
            .upload_id(upload_id)
            .part_number(part_number)
            .presigned(presign_config)
            .await
            .map_storage_err_ctx(
                StorageErrorKind::Misconfigured,
                "S3 presigned upload_part failed",
            )?;

        Ok(sdk_presigned_upload_request(url))
    }

    async fn complete_multipart_upload(
        &self,
        path: &str,
        upload_id: &str,
        parts: Vec<(i32, String)>,
    ) -> aster_drive_storage::Result<()> {
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
    ) -> aster_drive_storage::Result<String> {
        self.upload_multipart_part_bytes(path, upload_id, part_number, Bytes::copy_from_slice(data))
            .await
    }

    async fn upload_multipart_part_bytes(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        data: Bytes,
    ) -> aster_drive_storage::Result<String> {
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
    ) -> aster_drive_storage::Result<String> {
        let key = self.full_key(path);
        let content_length = aster_forge_utils::numbers::i64_to_u64(size, "S3 multipart part size")
            .map_storage_err(StorageErrorKind::Misconfigured)?;
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

    async fn abort_multipart_upload(
        &self,
        path: &str,
        upload_id: &str,
    ) -> aster_drive_storage::Result<()> {
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

    async fn list_uploaded_part_details(
        &self,
        path: &str,
        upload_id: &str,
    ) -> aster_drive_storage::Result<Vec<UploadedMultipartPart>> {
        let key = self.full_key(path);
        let mut parts = Vec::new();
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
                let part_number = part.part_number.ok_or_else(|| {
                    storage_driver_error(
                        StorageErrorKind::Unknown,
                        "S3 list_parts response missing part_number",
                    )
                })?;
                let size = part.size.ok_or_else(|| {
                    storage_driver_error(
                        StorageErrorKind::Unknown,
                        "S3 list_parts response missing part size",
                    )
                })?;
                parts.push(UploadedMultipartPart { part_number, size });
            }

            if resp.is_truncated() == Some(true) {
                part_marker = resp
                    .next_part_number_marker()
                    .map(|s| s.to_string())
                    .or_else(|| {
                        resp.parts().last().and_then(|part| {
                            part.part_number.map(|number| (number + 1).to_string())
                        })
                    });
                if part_marker.is_none() {
                    return Err(storage_driver_error(
                        StorageErrorKind::Unknown,
                        "S3 list_parts response is truncated without a next marker",
                    ));
                }
            } else {
                break;
            }
        }

        Ok(parts)
    }
}

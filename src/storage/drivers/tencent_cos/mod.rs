//! 腾讯云 COS 存储驱动。
//!
//! 基础对象读写复用 S3 兼容驱动；COS/CI 数据处理使用 COS 原生 query
//! 签名，因为 CI 处理参数必须参与签名，不能追加在普通 S3 presigned URL 后面。

pub(crate) mod cors;
mod native_media_metadata;
mod native_thumbnail;
mod signing;
#[cfg(test)]
mod tests;

use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use tokio::io::AsyncRead;
use url::Url;

use super::s3::{S3Driver, S3DriverOptions};
use super::s3_compatible::S3CompatibleDriver;
use super::s3_config::{S3ConfigError, normalize_s3_endpoint_and_bucket};
use crate::config::OUTBOUND_HTTP_USER_AGENT;
use aster_drive_model::entities::storage_policy;
use aster_drive_storage::error::{StorageErrorKind, storage_driver_error};
use aster_drive_storage::object_key;
use aster_drive_storage::{
    BlobMetadata, MapStorageErr, MultipartStorageDriver, Result, StorageDriver,
    UploadedMultipartPart,
};

pub(super) const COS_NATIVE_PROCESSING_PROVIDER: &str = "tencent_cos_ci";
pub(super) const MAX_COS_THUMBNAIL_TTL: Duration = Duration::from_secs(5 * 60);

fn non_empty_xml_text(text: Option<&str>) -> Option<String> {
    let trimmed = text?.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

pub struct TencentCosDriver {
    storage: S3CompatibleDriver,
    client: reqwest::Client,
    endpoint: String,
    bucket: String,
    access_key: String,
    secret_key: String,
    base_path: String,
}

impl TencentCosDriver {
    pub fn validate_policy(policy: &storage_policy::Model) -> Result<()> {
        S3Driver::validate_policy(policy)?;
        let normalized = normalize_s3_endpoint_and_bucket(&policy.endpoint, &policy.bucket)
            .map_err(Self::rewrap_s3_config_error)?;
        if normalized.endpoint.trim().is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "COS endpoint is required",
            ));
        }
        let endpoint = Url::parse(&normalized.endpoint)
            .map_storage_err_ctx(StorageErrorKind::Misconfigured, "parse COS endpoint")?;
        let host = endpoint.host_str().ok_or_else(|| {
            storage_driver_error(StorageErrorKind::Misconfigured, "COS endpoint missing host")
        })?;
        if !host.ends_with(".myqcloud.com") {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "COS endpoint must use a Tencent COS myqcloud.com host",
            ));
        }
        Ok(())
    }

    pub fn new(policy: &storage_policy::Model) -> Result<Self> {
        Self::validate_policy(policy)?;
        let normalized = normalize_s3_endpoint_and_bucket(&policy.endpoint, &policy.bucket)
            .map_err(Self::rewrap_s3_config_error)?;
        let mut storage_policy = policy.clone();
        storage_policy.endpoint =
            signing::cos_virtual_hosted_s3_endpoint(&normalized.endpoint, &normalized.bucket)?;
        storage_policy.bucket = normalized.bucket.clone();
        let storage = S3CompatibleDriver::new_with_s3_options(
            &storage_policy,
            S3DriverOptions::virtual_hosted_style(),
        )?;
        let client = cos_ci_http_client(policy)?;

        Ok(Self {
            storage,
            client,
            endpoint: normalized.endpoint,
            bucket: normalized.bucket,
            access_key: policy.access_key.clone(),
            secret_key: policy.secret_key.clone(),
            base_path: policy.base_path.clone(),
        })
    }

    pub fn s3_driver(&self) -> std::sync::Arc<super::s3::S3Driver> {
        self.storage.s3_driver()
    }

    fn rewrap_s3_config_error(error: S3ConfigError) -> aster_drive_storage::StorageError {
        let message = match error {
            S3ConfigError::MissingBucket => {
                "bucket is required for S3-compatible storage".to_string()
            }
            S3ConfigError::InvalidEndpoint(message) => message,
        };
        storage_driver_error(StorageErrorKind::Misconfigured, message)
    }

    fn full_key(&self, path: &str) -> String {
        object_key::join_key_prefix(&self.base_path, path)
    }
}

fn cos_ci_http_client(policy: &storage_policy::Model) -> Result<reqwest::Client> {
    let options = aster_drive_model::types::parse_storage_policy_options(policy.options.as_ref());
    reqwest::Client::builder()
        .connect_timeout(options.effective_s3_connect_timeout())
        .read_timeout(options.effective_s3_read_timeout())
        .timeout(options.effective_s3_operation_timeout())
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(OUTBOUND_HTTP_USER_AGENT)
        .build()
        .map_storage_err_ctx(StorageErrorKind::Misconfigured, "build COS CI HTTP client")
}

#[async_trait]
impl StorageDriver for TencentCosDriver {
    async fn put(&self, path: &str, data: &[u8]) -> aster_drive_storage::Result<String> {
        self.storage.put(path, data).await
    }

    async fn get(&self, path: &str) -> aster_drive_storage::Result<Vec<u8>> {
        self.storage.get(path).await
    }

    async fn get_stream(
        &self,
        path: &str,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.storage.get_stream(path).await
    }

    async fn get_range(
        &self,
        path: &str,
        offset: u64,
        length: Option<u64>,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.storage.get_range(path, offset, length).await
    }

    fn supports_efficient_range(&self) -> bool {
        self.storage.supports_efficient_range()
    }

    async fn delete(&self, path: &str) -> aster_drive_storage::Result<()> {
        self.storage.delete(path).await
    }

    async fn exists(&self, path: &str) -> aster_drive_storage::Result<bool> {
        self.storage.exists(path).await
    }

    async fn metadata(&self, path: &str) -> aster_drive_storage::Result<BlobMetadata> {
        self.storage.metadata(path).await
    }

    async fn readiness_check(&self) -> aster_drive_storage::Result<()> {
        self.storage.readiness_check().await
    }

    async fn copy_object(
        &self,
        src_path: &str,
        dest_path: &str,
    ) -> aster_drive_storage::Result<String> {
        self.storage.copy_object(src_path, dest_path).await
    }

    fn extensions(&self) -> aster_drive_storage::StorageDriverExtensions<'_> {
        let base = self.storage.extensions();
        aster_drive_storage::StorageDriverExtensions {
            presigned: base.presigned,
            list: base.list,
            stream_upload: base.stream_upload,
            native_thumbnail: Some(self),
            native_media_metadata: Some(self),
            multipart: Some(self),
            ..Default::default()
        }
    }

    async fn capacity_info(
        &self,
    ) -> aster_drive_storage::Result<aster_drive_storage::StorageCapacityInfo> {
        self.storage.capacity_info().await
    }
}

#[async_trait]
impl MultipartStorageDriver for TencentCosDriver {
    async fn create_multipart_upload(&self, path: &str) -> aster_drive_storage::Result<String> {
        self.storage.create_multipart_upload(path).await
    }

    async fn presigned_upload_part_url(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        expires: Duration,
    ) -> aster_drive_storage::Result<String> {
        self.storage
            .presigned_upload_part_url(path, upload_id, part_number, expires)
            .await
    }

    async fn complete_multipart_upload(
        &self,
        path: &str,
        upload_id: &str,
        parts: Vec<(i32, String)>,
    ) -> aster_drive_storage::Result<()> {
        self.storage
            .complete_multipart_upload(path, upload_id, parts)
            .await
    }

    async fn upload_multipart_part(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        data: &[u8],
    ) -> aster_drive_storage::Result<String> {
        self.storage
            .upload_multipart_part(path, upload_id, part_number, data)
            .await
    }

    async fn upload_multipart_part_bytes(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        data: Bytes,
    ) -> aster_drive_storage::Result<String> {
        self.storage
            .upload_multipart_part_bytes(path, upload_id, part_number, data)
            .await
    }

    async fn upload_multipart_part_reader(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> aster_drive_storage::Result<String> {
        self.storage
            .upload_multipart_part_reader(path, upload_id, part_number, reader, size)
            .await
    }

    async fn abort_multipart_upload(
        &self,
        path: &str,
        upload_id: &str,
    ) -> aster_drive_storage::Result<()> {
        self.storage.abort_multipart_upload(path, upload_id).await
    }

    async fn list_uploaded_part_details(
        &self,
        path: &str,
        upload_id: &str,
    ) -> aster_drive_storage::Result<Vec<UploadedMultipartPart>> {
        self.storage
            .list_uploaded_part_details(path, upload_id)
            .await
    }
}

//! S3-compatible provider wrapper.
//!
//! 厂商对象存储通常复用 S3 API 做基础对象读写、presigned 和 multipart，
//! 但又会额外暴露各自的数据处理能力。这个模块把通用 S3-compatible 行为
//! 抽出来，厂商 driver 只需要实现自己的能力扩展。

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use tokio::io::AsyncRead;

use super::s3::{S3Driver, S3DriverOptions};
use crate::entities::storage_policy;
use crate::errors::Result;
use crate::storage::driver::{BlobMetadata, StorageDriver};
use crate::storage::extensions::{
    ListStorageDriver, NativePreviewStorageDriver, NativeThumbnailStorageDriver,
    PresignedStorageDriver, StorageCapacityInfo, StreamUploadDriver,
};
use crate::storage::multipart::MultipartStorageDriver;

pub struct S3CompatibleDriver {
    inner: Arc<S3Driver>,
}

impl S3CompatibleDriver {
    pub fn validate_policy(policy: &storage_policy::Model) -> Result<()> {
        S3Driver::validate_policy(policy)
    }

    pub fn new(policy: &storage_policy::Model) -> Result<Self> {
        Self::new_with_s3_options(policy, S3DriverOptions::default())
    }

    pub fn new_with_s3_options(
        policy: &storage_policy::Model,
        options: S3DriverOptions,
    ) -> Result<Self> {
        Ok(Self {
            inner: Arc::new(S3Driver::new_with_options(policy, options)?),
        })
    }

    pub fn from_s3_driver(inner: Arc<S3Driver>) -> Self {
        Self { inner }
    }

    pub fn s3_driver(&self) -> Arc<S3Driver> {
        self.inner.clone()
    }

    fn inner(&self) -> &S3Driver {
        &self.inner
    }
}

pub trait S3CompatibleProvider: Send + Sync {
    fn s3_compatible_driver(&self) -> &S3CompatibleDriver;

    fn as_provider_native_thumbnail(&self) -> Option<&dyn NativeThumbnailStorageDriver> {
        None
    }

    fn as_provider_native_preview(&self) -> Option<&dyn NativePreviewStorageDriver> {
        None
    }
}

impl S3CompatibleProvider for S3CompatibleDriver {
    fn s3_compatible_driver(&self) -> &S3CompatibleDriver {
        self
    }
}

#[async_trait]
impl<T> StorageDriver for T
where
    T: S3CompatibleProvider,
{
    async fn put(&self, path: &str, data: &[u8]) -> Result<String> {
        self.s3_compatible_driver().inner().put(path, data).await
    }

    async fn get(&self, path: &str) -> Result<Vec<u8>> {
        self.s3_compatible_driver().inner().get(path).await
    }

    async fn get_stream(&self, path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.s3_compatible_driver().inner().get_stream(path).await
    }

    async fn get_range(
        &self,
        path: &str,
        offset: u64,
        length: Option<u64>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.s3_compatible_driver()
            .inner()
            .get_range(path, offset, length)
            .await
    }

    fn supports_efficient_range(&self) -> bool {
        self.s3_compatible_driver()
            .inner()
            .supports_efficient_range()
    }

    async fn delete(&self, path: &str) -> Result<()> {
        self.s3_compatible_driver().inner().delete(path).await
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        self.s3_compatible_driver().inner().exists(path).await
    }

    async fn metadata(&self, path: &str) -> Result<BlobMetadata> {
        self.s3_compatible_driver().inner().metadata(path).await
    }

    async fn readiness_check(&self) -> Result<()> {
        self.s3_compatible_driver().inner().readiness_check().await
    }

    async fn copy_object(&self, src_path: &str, dest_path: &str) -> Result<String> {
        self.s3_compatible_driver()
            .inner()
            .copy_object(src_path, dest_path)
            .await
    }

    fn as_presigned(&self) -> Option<&dyn PresignedStorageDriver> {
        self.s3_compatible_driver().inner().as_presigned()
    }

    fn as_list(&self) -> Option<&dyn ListStorageDriver> {
        self.s3_compatible_driver().inner().as_list()
    }

    fn as_stream_upload(&self) -> Option<&dyn StreamUploadDriver> {
        self.s3_compatible_driver().inner().as_stream_upload()
    }

    fn as_native_thumbnail(&self) -> Option<&dyn NativeThumbnailStorageDriver> {
        self.as_provider_native_thumbnail()
    }

    fn as_native_preview(&self) -> Option<&dyn NativePreviewStorageDriver> {
        self.as_provider_native_preview()
    }

    async fn capacity_info(&self) -> Result<StorageCapacityInfo> {
        self.s3_compatible_driver().inner().capacity_info().await
    }

    fn as_multipart(&self) -> Option<&dyn MultipartStorageDriver> {
        Some(self)
    }
}

#[async_trait]
impl<T> MultipartStorageDriver for T
where
    T: S3CompatibleProvider,
{
    async fn create_multipart_upload(&self, path: &str) -> Result<String> {
        self.s3_compatible_driver()
            .inner()
            .create_multipart_upload(path)
            .await
    }

    async fn presigned_upload_part_url(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        expires: Duration,
    ) -> Result<String> {
        self.s3_compatible_driver()
            .inner()
            .presigned_upload_part_url(path, upload_id, part_number, expires)
            .await
    }

    async fn complete_multipart_upload(
        &self,
        path: &str,
        upload_id: &str,
        parts: Vec<(i32, String)>,
    ) -> Result<()> {
        self.s3_compatible_driver()
            .inner()
            .complete_multipart_upload(path, upload_id, parts)
            .await
    }

    async fn upload_multipart_part(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        data: &[u8],
    ) -> Result<String> {
        self.s3_compatible_driver()
            .inner()
            .upload_multipart_part(path, upload_id, part_number, data)
            .await
    }

    async fn upload_multipart_part_bytes(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        data: Bytes,
    ) -> Result<String> {
        self.s3_compatible_driver()
            .inner()
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
    ) -> Result<String> {
        self.s3_compatible_driver()
            .inner()
            .upload_multipart_part_reader(path, upload_id, part_number, reader, size)
            .await
    }

    async fn abort_multipart_upload(&self, path: &str, upload_id: &str) -> Result<()> {
        self.s3_compatible_driver()
            .inner()
            .abort_multipart_upload(path, upload_id)
            .await
    }

    async fn list_uploaded_parts(&self, path: &str, upload_id: &str) -> Result<Vec<i32>> {
        self.s3_compatible_driver()
            .inner()
            .list_uploaded_parts(path, upload_id)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::storage_policy;
    use crate::types::{DriverType, StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions};

    fn sample_policy() -> storage_policy::Model {
        storage_policy::Model {
            id: 1,
            name: "S3 compatible".to_string(),
            driver_type: DriverType::S3,
            endpoint: "https://s3.example.test".to_string(),
            bucket: "bucket".to_string(),
            access_key: "access-key".to_string(),
            secret_key: "secret-key".to_string(),
            base_path: "tenant-a".to_string(),
            remote_node_id: None,
            max_file_size: 0,
            allowed_types: StoredStoragePolicyAllowedTypes::empty(),
            options: StoredStoragePolicyOptions::empty(),
            is_default: false,
            chunk_size: 0,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn exposes_s3_compatible_optional_capabilities() {
        let driver = S3CompatibleDriver::new(&sample_policy()).expect("driver should build");

        assert!(driver.supports_efficient_range());
        assert!(driver.as_presigned().is_some());
        assert!(driver.as_list().is_some());
        assert!(driver.as_stream_upload().is_some());
        assert!(driver.as_multipart().is_some());
        assert!(driver.as_native_thumbnail().is_none());
        assert!(driver.as_native_preview().is_none());
    }

    #[tokio::test]
    async fn presigned_urls_are_forwarded_through_s3_driver() {
        let driver = S3CompatibleDriver::new(&sample_policy()).expect("driver should build");
        let presigned = driver
            .as_presigned()
            .expect("presigned capability")
            .presigned_put_url("docs/report.txt", Duration::from_secs(60))
            .await
            .expect("presigned URL should build")
            .expect("S3-compatible driver should return URL");

        assert!(
            presigned.starts_with("https://s3.example.test/bucket/tenant-a/docs/report.txt"),
            "unexpected presigned URL: {presigned}"
        );
        assert!(
            presigned.contains("X-Amz-Signature="),
            "expected AWS query signature in {presigned}"
        );
    }

    #[test]
    fn validate_policy_keeps_s3_validation_errors() {
        let mut policy = sample_policy();
        policy.access_key = String::new();

        let err =
            S3CompatibleDriver::validate_policy(&policy).expect_err("empty access key should fail");

        assert_eq!(err.code(), "E031");
        assert!(err.message().contains("access_key cannot be empty"));
    }
}

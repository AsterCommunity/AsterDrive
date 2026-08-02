//! S3-compatible provider wrapper.
//!
//! 厂商对象存储通常复用 S3 API 做基础对象读写、presigned 和 multipart，
//! 但又会额外暴露各自的数据处理能力。这个模块把通用 S3-compatible 行为
//! 抽出来，厂商 driver 只需要实现自己的能力扩展。

use aster_drive_storage::Result;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use tokio::io::AsyncRead;

use super::s3::{S3Driver, S3DriverOptions};
use aster_drive_model::entities::storage_policy;
use aster_drive_storage::traits::driver::{BlobMetadata, StorageDriver};
use aster_drive_storage::traits::extensions::StorageCapacityInfo;
use aster_drive_storage::traits::multipart::MultipartStorageDriver;

pub struct S3CompatibleDriver {
    inner: Arc<S3Driver>,
}

impl S3CompatibleDriver {
    pub fn new(policy: &storage_policy::Model) -> Result<Self> {
        Ok(Self {
            inner: Arc::new(S3Driver::new(
                policy,
                S3DriverOptions::default(),
                std::convert::identity,
            )?),
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

#[async_trait]
impl StorageDriver for S3CompatibleDriver {
    async fn put(&self, path: &str, data: &[u8]) -> aster_drive_storage::Result<String> {
        self.inner().put(path, data).await
    }

    async fn get(&self, path: &str) -> aster_drive_storage::Result<Vec<u8>> {
        self.inner().get(path).await
    }

    async fn get_stream(
        &self,
        path: &str,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.inner().get_stream(path).await
    }

    async fn get_range(
        &self,
        path: &str,
        offset: u64,
        length: Option<u64>,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.inner().get_range(path, offset, length).await
    }

    fn supports_efficient_range(&self) -> bool {
        self.inner().supports_efficient_range()
    }

    async fn delete(&self, path: &str) -> aster_drive_storage::Result<()> {
        self.inner().delete(path).await
    }

    async fn exists(&self, path: &str) -> aster_drive_storage::Result<bool> {
        self.inner().exists(path).await
    }

    async fn metadata(&self, path: &str) -> aster_drive_storage::Result<BlobMetadata> {
        self.inner().metadata(path).await
    }

    async fn readiness_check(&self) -> aster_drive_storage::Result<()> {
        self.inner().readiness_check().await
    }

    async fn copy_object(
        &self,
        src_path: &str,
        dest_path: &str,
    ) -> aster_drive_storage::Result<String> {
        self.inner().copy_object(src_path, dest_path).await
    }

    fn extensions(&self) -> aster_drive_storage::traits::StorageDriverExtensions<'_> {
        aster_drive_storage::traits::StorageDriverExtensions {
            presigned: self.inner().extensions().presigned,
            list: self.inner().extensions().list,
            stream_upload: self.inner().extensions().stream_upload,
            multipart: Some(self),
            ..Default::default()
        }
    }

    async fn capacity_info(&self) -> aster_drive_storage::Result<StorageCapacityInfo> {
        self.inner().capacity_info().await
    }
}

#[async_trait]
impl MultipartStorageDriver for S3CompatibleDriver {
    async fn create_multipart_upload(&self, path: &str) -> aster_drive_storage::Result<String> {
        self.inner().create_multipart_upload(path).await
    }

    async fn presigned_upload_part_url(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        expires: Duration,
    ) -> aster_drive_storage::Result<String> {
        self.inner()
            .presigned_upload_part_url(path, upload_id, part_number, expires)
            .await
    }

    async fn complete_multipart_upload(
        &self,
        path: &str,
        upload_id: &str,
        parts: Vec<(i32, String)>,
    ) -> aster_drive_storage::Result<()> {
        self.inner()
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
        self.inner()
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
        self.inner()
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
        self.inner()
            .upload_multipart_part_reader(path, upload_id, part_number, reader, size)
            .await
    }

    async fn abort_multipart_upload(
        &self,
        path: &str,
        upload_id: &str,
    ) -> aster_drive_storage::Result<()> {
        self.inner().abort_multipart_upload(path, upload_id).await
    }

    async fn list_uploaded_part_details(
        &self,
        path: &str,
        upload_id: &str,
    ) -> aster_drive_storage::Result<
        Vec<aster_drive_storage::traits::multipart::UploadedMultipartPart>,
    > {
        self.inner()
            .list_uploaded_part_details(path, upload_id)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aster_drive_model::entities::storage_policy;
    use aster_drive_model::types::{
        DriverType, StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions,
    };
    use aster_drive_storage::StorageErrorKind;

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
            remote_storage_target_key: None,
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
        assert!(driver.extensions().presigned.is_some());
        assert!(driver.extensions().list.is_some());
        assert!(driver.extensions().stream_upload.is_some());
        assert!(driver.extensions().multipart.is_some());
        assert!(driver.extensions().native_thumbnail.is_none());
    }

    #[tokio::test]
    async fn presigned_urls_are_forwarded_through_s3_driver() {
        let driver = S3CompatibleDriver::new(&sample_policy()).expect("driver should build");
        let presigned = driver
            .extensions()
            .presigned
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
    fn construction_keeps_s3_validation_errors() {
        let mut policy = sample_policy();
        policy.access_key = String::new();

        let err = S3CompatibleDriver::new(&policy)
            .err()
            .expect("empty access key should fail");

        assert_eq!(err.kind(), StorageErrorKind::Auth);
        assert!(err.message().contains("access_key cannot be empty"));
    }
}

//! S3-compatible provider wrapper.
//!
//! 厂商对象存储通常复用 S3 API 做基础对象读写、presigned 和 multipart，
//! 但又会额外暴露各自的数据处理能力。这个模块把通用 S3-compatible 行为
//! 抽出来，厂商 driver 只需要实现自己的能力扩展。

use super::s3::S3Driver;
use std::sync::Arc;

pub struct S3CompatibleDriver {
    inner: Arc<S3Driver>,
}

impl S3CompatibleDriver {
    pub fn from_s3_driver(inner: Arc<S3Driver>) -> Self {
        Self { inner }
    }

    pub fn s3_driver(&self) -> Arc<S3Driver> {
        self.inner.clone()
    }
}

macro_rules! delegate_s3_compatible_storage_driver {
    ($driver:ty, $field:ident $(, $native_extension:ident)*) => {
        $crate::storage::drivers::s3_compatible::delegate_s3_compatible_storage_driver!(
            @impl $driver, $field, inherited_list $(, $native_extension)*
        );
    };
    ($driver:ty, $field:ident, list = self $(, $native_extension:ident)*) => {
        $crate::storage::drivers::s3_compatible::delegate_s3_compatible_storage_driver!(
            @impl $driver, $field, own_list $(, $native_extension)*
        );
    };
    (@impl $driver:ty, $field:ident, $list_mode:ident $(, $native_extension:ident)*) => {
        #[async_trait::async_trait]
        impl aster_drive_storage::StorageDriver for $driver {
            async fn put(&self, path: &str, data: &[u8]) -> aster_drive_storage::Result<String> {
                self.$field.put(path, data).await
            }

            async fn get(&self, path: &str) -> aster_drive_storage::Result<Vec<u8>> {
                self.$field.get(path).await
            }

            async fn get_stream(
                &self,
                path: &str,
            ) -> aster_drive_storage::Result<Box<dyn tokio::io::AsyncRead + Unpin + Send>> {
                self.$field.get_stream(path).await
            }

            async fn get_range(
                &self,
                path: &str,
                offset: u64,
                length: Option<u64>,
            ) -> aster_drive_storage::Result<Box<dyn tokio::io::AsyncRead + Unpin + Send>> {
                self.$field.get_range(path, offset, length).await
            }

            fn supports_efficient_range(&self) -> bool {
                self.$field.supports_efficient_range()
            }

            async fn delete(&self, path: &str) -> aster_drive_storage::Result<()> {
                self.$field.delete(path).await
            }

            async fn exists(&self, path: &str) -> aster_drive_storage::Result<bool> {
                self.$field.exists(path).await
            }

            async fn metadata(
                &self,
                path: &str,
            ) -> aster_drive_storage::Result<aster_drive_storage::BlobMetadata> {
                self.$field.metadata(path).await
            }

            async fn readiness_check(&self) -> aster_drive_storage::Result<()> {
                self.$field.readiness_check().await
            }

            async fn copy_object(
                &self,
                src_path: &str,
                dest_path: &str,
            ) -> aster_drive_storage::Result<String> {
                self.$field.copy_object(src_path, dest_path).await
            }

            fn extensions(&self) -> aster_drive_storage::StorageDriverExtensions<'_> {
                let this = self;
                let base = this.$field.extensions();
                aster_drive_storage::StorageDriverExtensions {
                    presigned: base.presigned,
                    list: $crate::storage::drivers::s3_compatible::delegate_s3_compatible_storage_driver!(@list $list_mode, base, this),
                    stream_upload: base.stream_upload,
                    multipart: Some(this),
                    $($native_extension: Some(this),)*
                    ..Default::default()
                }
            }

            async fn capacity_info(
                &self,
            ) -> aster_drive_storage::Result<aster_drive_storage::StorageCapacityInfo> {
                self.$field.capacity_info().await
            }
        }
    };
    (@list inherited_list, $base:ident, $this:ident) => {
        $base.list
    };
    (@list own_list, $base:ident, $this:ident) => {{
        let _ = $base;
        Some($this)
    }};
}

macro_rules! delegate_s3_compatible_multipart_driver {
    ($driver:ty, $field:ident) => {
        #[async_trait::async_trait]
        impl aster_drive_storage::MultipartStorageDriver for $driver {
            async fn create_multipart_upload(
                &self,
                path: &str,
            ) -> aster_drive_storage::Result<String> {
                self.$field.create_multipart_upload(path).await
            }

            async fn presigned_upload_part_url(
                &self,
                path: &str,
                upload_id: &str,
                part_number: i32,
                expires: std::time::Duration,
            ) -> aster_drive_storage::Result<String> {
                self.$field
                    .presigned_upload_part_url(path, upload_id, part_number, expires)
                    .await
            }

            async fn complete_multipart_upload(
                &self,
                path: &str,
                upload_id: &str,
                parts: Vec<(i32, String)>,
            ) -> aster_drive_storage::Result<()> {
                self.$field
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
                self.$field
                    .upload_multipart_part(path, upload_id, part_number, data)
                    .await
            }

            async fn upload_multipart_part_bytes(
                &self,
                path: &str,
                upload_id: &str,
                part_number: i32,
                data: bytes::Bytes,
            ) -> aster_drive_storage::Result<String> {
                self.$field
                    .upload_multipart_part_bytes(path, upload_id, part_number, data)
                    .await
            }

            async fn upload_multipart_part_reader(
                &self,
                path: &str,
                upload_id: &str,
                part_number: i32,
                reader: Box<dyn tokio::io::AsyncRead + Unpin + Send + Sync>,
                size: i64,
            ) -> aster_drive_storage::Result<String> {
                self.$field
                    .upload_multipart_part_reader(path, upload_id, part_number, reader, size)
                    .await
            }

            async fn abort_multipart_upload(
                &self,
                path: &str,
                upload_id: &str,
            ) -> aster_drive_storage::Result<()> {
                self.$field.abort_multipart_upload(path, upload_id).await
            }

            async fn list_uploaded_part_details(
                &self,
                path: &str,
                upload_id: &str,
            ) -> aster_drive_storage::Result<Vec<aster_drive_storage::UploadedMultipartPart>> {
                self.$field
                    .list_uploaded_part_details(path, upload_id)
                    .await
            }
        }
    };
}

pub(super) use delegate_s3_compatible_multipart_driver;
pub(super) use delegate_s3_compatible_storage_driver;

delegate_s3_compatible_storage_driver!(S3CompatibleDriver, inner);
delegate_s3_compatible_multipart_driver!(S3CompatibleDriver, inner);

#[cfg(test)]
mod tests {
    use super::*;
    use aster_drive_storage::{StorageDriver, StorageErrorKind};
    use std::time::Duration;

    fn build_driver_with_access_key(
        access_key: &str,
    ) -> aster_drive_storage::Result<S3CompatibleDriver> {
        let driver = S3Driver::new(
            super::super::s3::S3DriverConfig {
                endpoint: "https://s3.example.test".to_string(),
                bucket: "bucket".to_string(),
                base_path: "tenant-a".to_string(),
                region: "auto".to_string(),
                path_style: true,
                connect_timeout: Duration::from_secs(5),
                read_timeout: Duration::from_secs(30),
                operation_timeout: Duration::from_secs(3_600),
            },
            super::super::s3::S3StaticCredentials {
                access_key: access_key.to_string(),
                secret_key: "secret-key".to_string(),
            },
            super::super::s3::S3DriverOptions::default(),
            std::convert::identity,
        )?;
        Ok(S3CompatibleDriver::from_s3_driver(Arc::new(driver)))
    }

    fn build_driver() -> aster_drive_storage::Result<S3CompatibleDriver> {
        build_driver_with_access_key("access-key")
    }

    #[test]
    fn exposes_s3_compatible_optional_capabilities() {
        let driver = build_driver().expect("driver should build");

        assert!(driver.supports_efficient_range());
        assert!(driver.extensions().presigned.is_some());
        assert!(driver.extensions().list.is_some());
        assert!(driver.extensions().stream_upload.is_some());
        assert!(driver.extensions().multipart.is_some());
        assert!(driver.extensions().native_thumbnail.is_none());
    }

    #[tokio::test]
    async fn presigned_urls_are_forwarded_through_s3_driver() {
        let driver = build_driver().expect("driver should build");
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
        let err = build_driver_with_access_key("")
            .err()
            .expect("empty access key should fail");

        assert_eq!(err.kind(), StorageErrorKind::Auth);
        assert!(err.message().contains("access_key cannot be empty"));
    }
}

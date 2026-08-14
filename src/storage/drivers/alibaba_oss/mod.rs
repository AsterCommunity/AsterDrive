//! 阿里云 OSS 存储驱动。
//!
//! 对象读写、Range、流式上传和 multipart 编排复用 AWS S3 SDK；请求在 Smithy
//! 鉴权阶段改写为 OSS V4。后端 I/O 可以走内网 endpoint，而所有浏览器可见的
//! presigned URL 始终由 public endpoint client 生成。

mod signing;
#[cfg(test)]
mod tests;

use std::{sync::Arc, time::Duration};

use aster_drive_storage::traits::driver::{PresignedDownloadOptions, StorageDriver};
use aster_drive_storage::traits::extensions::{PresignedStorageDriver, StorageDriverExtensions};
use aster_drive_storage::traits::multipart::{MultipartStorageDriver, UploadedMultipartPart};
use aster_drive_storage::{Result, StorageCapacityInfo};
use bytes::Bytes;
use tokio::io::AsyncRead;
use url::Url;

use super::s3::{S3Driver, S3DriverConfig, S3DriverOptions, S3StaticCredentials};
use super::s3_compatible::S3CompatibleDriver;
use super::s3_config::{S3ConfigError, normalize_s3_endpoint_and_bucket};
use aster_drive_storage::error::{StorageErrorKind, storage_driver_error};

pub struct AlibabaOssDriver {
    storage: S3CompatibleDriver,
    public_driver: Arc<S3Driver>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlibabaOssDriverConfig {
    pub endpoint: String,
    pub server_side_endpoint: String,
    pub region: String,
    pub bucket: String,
    pub base_path: String,
    pub use_cname: bool,
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
    pub operation_timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlibabaOssStaticCredentials {
    pub access_key: String,
    pub secret_key: String,
}

impl AlibabaOssDriver {
    pub fn validate_connection_config(config: &AlibabaOssDriverConfig) -> Result<()> {
        let public = normalize_oss_endpoint(
            &config.endpoint,
            &config.bucket,
            config.use_cname,
            "OSS public endpoint",
        )?;
        validate_oss_bucket(&public.bucket)?;
        validate_oss_region(&config.region)?;

        if !config.server_side_endpoint.trim().is_empty() {
            normalize_oss_endpoint(
                &config.server_side_endpoint,
                &config.bucket,
                false,
                "OSS server-side endpoint",
            )?;
        }
        Ok(())
    }

    pub fn validate_config(
        config: &AlibabaOssDriverConfig,
        credentials: &AlibabaOssStaticCredentials,
    ) -> Result<()> {
        Self::validate_connection_config(config)?;
        if credentials.access_key.trim().is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Auth,
                "OSS access key ID cannot be empty",
            ));
        }
        if credentials.secret_key.trim().is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Auth,
                "OSS access key secret cannot be empty",
            ));
        }
        Ok(())
    }

    pub fn new(
        config: AlibabaOssDriverConfig,
        credentials: AlibabaOssStaticCredentials,
    ) -> Result<Self> {
        Self::validate_config(&config, &credentials)?;

        let public_driver = Arc::new(build_oss_s3_driver(
            &config.endpoint,
            &config,
            &credentials,
            config.use_cname,
        )?);
        let backend_driver = if config.server_side_endpoint.trim().is_empty() {
            public_driver.clone()
        } else {
            Arc::new(build_oss_s3_driver(
                &config.server_side_endpoint,
                &config,
                &credentials,
                false,
            )?)
        };

        Ok(Self {
            storage: S3CompatibleDriver::from_s3_driver(backend_driver),
            public_driver,
        })
    }

    pub fn backend_s3_driver(&self) -> Arc<S3Driver> {
        self.storage.s3_driver()
    }

    pub fn public_s3_driver(&self) -> Arc<S3Driver> {
        self.public_driver.clone()
    }
}

fn build_oss_s3_driver(
    endpoint: &str,
    config: &AlibabaOssDriverConfig,
    credentials: &AlibabaOssStaticCredentials,
    use_cname: bool,
) -> Result<S3Driver> {
    let normalized = normalize_oss_endpoint(endpoint, &config.bucket, use_cname, "OSS endpoint")?;
    let bucket = normalized.bucket.clone();
    let signer_bucket = normalized.bucket.clone();
    let region = config.region.trim().to_string();
    let signer_region = region.clone();

    S3Driver::new(
        S3DriverConfig {
            endpoint: normalized.endpoint,
            bucket,
            base_path: config.base_path.clone(),
            region,
            path_style: use_cname,
            connect_timeout: config.connect_timeout,
            read_timeout: config.read_timeout,
            operation_timeout: config.operation_timeout,
        },
        S3StaticCredentials {
            access_key: credentials.access_key.clone(),
            secret_key: credentials.secret_key.clone(),
        },
        if use_cname {
            S3DriverOptions::path_style()
        } else {
            S3DriverOptions::virtual_hosted_style()
        },
        move |builder| {
            signing::configure_oss_auth(builder, signer_bucket, signer_region, use_cname)
        },
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedOssEndpoint {
    endpoint: String,
    bucket: String,
}

fn normalize_oss_endpoint(
    endpoint: &str,
    bucket: &str,
    use_cname: bool,
    label: &str,
) -> Result<NormalizedOssEndpoint> {
    let normalized =
        normalize_s3_endpoint_and_bucket(endpoint, bucket).map_err(rewrap_s3_config_error)?;
    if normalized.endpoint.is_empty() {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("{label} is required"),
        ));
    }

    let mut url = Url::parse(&normalized.endpoint).map_err(|error| {
        storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("invalid {label}: {error}"),
        )
    })?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("{label} must not contain credentials"),
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("{label} must not contain query or fragment components"),
        ));
    }
    if url.path() != "/" && !url.path().is_empty() {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("{label} must not contain a path"),
        ));
    }

    let host = url
        .host_str()
        .ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                format!("{label} must contain a hostname"),
            )
        })?
        .to_string();
    let is_aliyun_host = host == "aliyuncs.com" || host.ends_with(".aliyuncs.com");
    if use_cname == is_aliyun_host {
        let message = if use_cname {
            "OSS CNAME mode requires a custom-domain endpoint"
        } else {
            "OSS endpoint must use an aliyuncs.com host unless CNAME mode is enabled"
        };
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            message,
        ));
    }

    if !use_cname {
        let bucket_prefix = format!("{}.", normalized.bucket);
        if let Some(root_host) = host.strip_prefix(&bucket_prefix) {
            url.set_host(Some(root_host)).map_err(|_| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    format!("invalid {label} hostname"),
                )
            })?;
        }
    }
    url.set_path("");
    url.set_query(None);
    url.set_fragment(None);

    Ok(NormalizedOssEndpoint {
        endpoint: String::from(url).trim_end_matches('/').to_string(),
        bucket: normalized.bucket,
    })
}

fn validate_oss_bucket(bucket: &str) -> Result<()> {
    let bytes = bucket.as_bytes();
    let valid = (3..=63).contains(&bytes.len())
        && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-');
    if valid {
        Ok(())
    } else {
        Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "OSS bucket must be 3-63 lowercase letters, digits, or hyphens and start/end with a letter or digit",
        ))
    }
}

fn validate_oss_region(region: &str) -> Result<()> {
    let region = region.trim();
    if region.is_empty() {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "OSS region is required",
        ));
    }
    if region.len() > 128
        || !region
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "OSS region contains invalid characters",
        ));
    }
    Ok(())
}

fn rewrap_s3_config_error(error: S3ConfigError) -> aster_drive_storage::StorageError {
    let message = match error {
        S3ConfigError::MissingBucket => "OSS bucket is required".to_string(),
        S3ConfigError::InvalidEndpoint(message) => message.replace("S3 endpoint", "OSS endpoint"),
        S3ConfigError::InvalidRegion => {
            "OSS region must be 1-128 printable ASCII characters without whitespace or '/'"
                .to_string()
        }
    };
    storage_driver_error(StorageErrorKind::Misconfigured, message)
}

#[async_trait::async_trait]
impl StorageDriver for AlibabaOssDriver {
    async fn put(&self, path: &str, data: &[u8]) -> Result<String> {
        self.storage.put(path, data).await
    }

    async fn get(&self, path: &str) -> Result<Vec<u8>> {
        self.storage.get(path).await
    }

    async fn get_stream(&self, path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.storage.get_stream(path).await
    }

    async fn get_range(
        &self,
        path: &str,
        offset: u64,
        length: Option<u64>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.storage.get_range(path, offset, length).await
    }

    fn supports_efficient_range(&self) -> bool {
        self.storage.supports_efficient_range()
    }

    async fn delete(&self, path: &str) -> Result<()> {
        self.storage.delete(path).await
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        self.storage.exists(path).await
    }

    async fn metadata(&self, path: &str) -> Result<aster_drive_storage::BlobMetadata> {
        self.storage.metadata(path).await
    }

    async fn readiness_check(&self) -> Result<()> {
        self.storage.readiness_check().await
    }

    async fn copy_object(&self, src_path: &str, dest_path: &str) -> Result<String> {
        self.storage.copy_object(src_path, dest_path).await
    }

    fn extensions(&self) -> StorageDriverExtensions<'_> {
        StorageDriverExtensions {
            presigned: Some(self),
            multipart: Some(self),
            ..self.storage.extensions()
        }
    }

    async fn capacity_info(&self) -> Result<StorageCapacityInfo> {
        self.storage.capacity_info().await
    }
}

#[async_trait::async_trait]
impl PresignedStorageDriver for AlibabaOssDriver {
    async fn presigned_url(
        &self,
        path: &str,
        expires: Duration,
        options: PresignedDownloadOptions,
    ) -> Result<Option<String>> {
        // OSS rejects response-content-type on signed GET requests with
        // 0017-00000902. The object's stored Content-Type remains authoritative.
        // https://help.aliyun.com/zh/oss/support/0017-00000902
        let options = PresignedDownloadOptions {
            response_content_type: None,
            ..options
        };
        self.public_driver
            .presigned_url(path, expires, options)
            .await
    }

    async fn presigned_put_request(
        &self,
        path: &str,
        expires: Duration,
    ) -> Result<Option<aster_drive_storage::PresignedUploadRequest>> {
        self.public_driver
            .presigned_put_request(path, expires)
            .await
    }

    fn presigned_single_put_requires_etag(&self) -> bool {
        // Single-object completion verifies the uploaded object's metadata and
        // size server-side. ETag remains required for presigned multipart parts.
        false
    }
}

#[async_trait::async_trait]
impl MultipartStorageDriver for AlibabaOssDriver {
    async fn create_multipart_upload(&self, path: &str) -> Result<String> {
        self.storage.create_multipart_upload(path).await
    }

    async fn presigned_upload_part_request(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        expires: Duration,
    ) -> Result<aster_drive_storage::PresignedUploadRequest> {
        self.public_driver
            .presigned_upload_part_request(path, upload_id, part_number, expires)
            .await
    }

    async fn complete_multipart_upload(
        &self,
        path: &str,
        upload_id: &str,
        parts: Vec<(i32, String)>,
    ) -> Result<()> {
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
    ) -> Result<String> {
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
    ) -> Result<String> {
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
    ) -> Result<String> {
        self.storage
            .upload_multipart_part_reader(path, upload_id, part_number, reader, size)
            .await
    }

    async fn abort_multipart_upload(&self, path: &str, upload_id: &str) -> Result<()> {
        self.storage.abort_multipart_upload(path, upload_id).await
    }

    async fn list_uploaded_part_details(
        &self,
        path: &str,
        upload_id: &str,
    ) -> Result<Vec<UploadedMultipartPart>> {
        self.storage
            .list_uploaded_part_details(path, upload_id)
            .await
    }
}

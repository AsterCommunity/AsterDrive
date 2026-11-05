//! 阿里云 OSS 存储驱动。
//!
//! 对象读写、Range、流式上传和 multipart 编排复用 AWS S3 SDK；请求在 Smithy
//! 鉴权阶段改写为 OSS V4。后端 I/O 可以走内网 endpoint，而所有浏览器可见的
//! presigned URL 始终由 public endpoint client 生成。

pub mod cdn;
pub(crate) mod cors;
mod signing;
#[cfg(test)]
mod tests;

use std::{sync::Arc, time::Duration};

use aster_drive_storage::traits::driver::{DirectDownloadOptions, StorageDriver};
use aster_drive_storage::traits::extensions::{
    DirectDownloadStorageDriver, PresignedUploadStorageDriver, StorageDriverExtensions,
};
use aster_drive_storage::traits::multipart::{
    MultipartStorageCapabilities, MultipartStorageDriver, MultipartUploadMode,
    UploadedMultipartPart,
};
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
    download_delivery: DownloadDelivery,
}

enum DownloadDelivery {
    OssOrigin,
    AlibabaCdnTypeA(CdnConfig),
}

struct CdnConfig {
    domain: Url,
    auth_key: String,
    validity_seconds: u32,
    /// base_path 前缀，用于 CDN URL 的对象键拼接。
    /// CDN 交付不经过 OSS API client，必须在 driver 层自行完成 key 拼接。
    base_path: String,
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
    pub download_delivery: cdn::OssDownloadDelivery,
    pub cdn_domain: Option<String>,
    pub cdn_auth_validity_seconds: u32,
}

#[derive(Clone, PartialEq, Eq)]
pub struct AlibabaOssStaticCredentials {
    pub access_key: String,
    pub secret_key: String,
    pub cdn_auth_key: Option<String>,
}

impl std::fmt::Debug for AlibabaOssStaticCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AlibabaOssStaticCredentials")
            .field("access_key", &self.access_key)
            .field("secret_key", &"[REDACTED]")
            .field(
                "cdn_auth_key",
                &self.cdn_auth_key.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
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
        let endpoint_region = match config.use_cname {
            false => official_oss_endpoint_region(&public.endpoint)?,
            true => None,
        };
        match endpoint_region {
            Some(region) if !region.eq_ignore_ascii_case(config.region.trim()) => {
                return Err(storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    format!(
                        "OSS region '{}' does not match public endpoint region '{region}'",
                        config.region.trim()
                    ),
                ));
            }
            Some(_) | None => {}
        }

        match config.server_side_endpoint.trim() {
            "" => {}
            endpoint => {
                normalize_oss_endpoint(
                    endpoint,
                    &config.bucket,
                    false,
                    "OSS server-side endpoint",
                )?;
            }
        }

        match config.download_delivery {
            cdn::OssDownloadDelivery::OssOrigin => {}
            cdn::OssDownloadDelivery::AlibabaCdnTypeA => {
                let domain = config.cdn_domain.as_deref().ok_or_else(|| {
                    storage_driver_error(
                        StorageErrorKind::Misconfigured,
                        "Alibaba CDN download domain is required",
                    )
                })?;
                cdn::validated_cdn_domain(domain)?;
                cdn::validate_cdn_auth_validity(config.cdn_auth_validity_seconds)?;
            }
        }

        Ok(())
    }

    pub fn validate_config(
        config: &AlibabaOssDriverConfig,
        credentials: &AlibabaOssStaticCredentials,
    ) -> Result<()> {
        Self::validate_connection_config(config)?;
        match (credentials.access_key.trim(), credentials.secret_key.trim()) {
            ("", _) => {
                return Err(storage_driver_error(
                    StorageErrorKind::Auth,
                    "OSS access key ID cannot be empty",
                ));
            }
            (_, "") => {
                return Err(storage_driver_error(
                    StorageErrorKind::Auth,
                    "OSS access key secret cannot be empty",
                ));
            }
            (_, _) => {}
        }

        match (
            config.download_delivery,
            credentials.cdn_auth_key.as_deref(),
        ) {
            (cdn::OssDownloadDelivery::OssOrigin, _) => {}
            (cdn::OssDownloadDelivery::AlibabaCdnTypeA, Some(key)) => {
                cdn::validate_cdn_auth_key(key)?
            }
            (cdn::OssDownloadDelivery::AlibabaCdnTypeA, None) => {
                return Err(storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "Alibaba CDN authentication key is required",
                ));
            }
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
        let backend_driver = match config.server_side_endpoint.trim() {
            "" => public_driver.clone(),
            endpoint => Arc::new(build_oss_s3_driver(endpoint, &config, &credentials, false)?),
        };

        let download_delivery = match (
            config.download_delivery,
            config.cdn_domain,
            credentials.cdn_auth_key,
        ) {
            (cdn::OssDownloadDelivery::OssOrigin, _, _) => DownloadDelivery::OssOrigin,
            (cdn::OssDownloadDelivery::AlibabaCdnTypeA, Some(domain), Some(auth_key)) => {
                DownloadDelivery::AlibabaCdnTypeA(CdnConfig {
                    domain: cdn::validated_cdn_domain(&domain)?,
                    auth_key,
                    validity_seconds: config.cdn_auth_validity_seconds,
                    base_path: config.base_path,
                })
            }
            (cdn::OssDownloadDelivery::AlibabaCdnTypeA, _, _) => {
                return Err(storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "Alibaba CDN domain and authentication key are required",
                ));
            }
        };

        Ok(Self {
            storage: S3CompatibleDriver::from_s3_driver(backend_driver),
            public_driver,
            download_delivery,
        })
    }

    /// Configure bucket CORS only for an explicitly requested admin action.
    /// The management client is created here, rather than during ordinary
    /// object-driver construction, so read/write paths do not inherit its
    /// endpoint or HTTP-client lifecycle.
    pub(crate) async fn configure_asterdrive_cors(
        config: &AlibabaOssDriverConfig,
        credentials: &AlibabaOssStaticCredentials,
        origins: &[String],
    ) -> crate::errors::Result<cors::OssCorsApplyResult> {
        Self::validate_config(config, credentials)?;
        cors::OssCorsClient::new(config, credentials)?
            .configure(origins)
            .await
    }

    pub fn backend_s3_driver(&self) -> Arc<S3Driver> {
        self.storage.s3_driver()
    }

    pub fn public_s3_driver(&self) -> Arc<S3Driver> {
        self.public_driver.clone()
    }
}

fn official_oss_endpoint_region(endpoint: &str) -> Result<Option<String>> {
    let url = Url::parse(endpoint).map_err(|error| {
        storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("invalid OSS endpoint while resolving region: {error}"),
        )
    })?;
    match url
        .host_str()
        .and_then(|host| host.split('.').next())
        .and_then(|label| label.strip_prefix("oss-"))
    {
        Some(region) => match region.strip_suffix("-internal").unwrap_or(region) {
            "" | "accelerate" | "accelerate-overseas" => Ok(None),
            region => Ok(Some(region.to_string())),
        },
        None => Ok(None),
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
        match use_cname {
            true => S3DriverOptions::path_style(),
            false => S3DriverOptions::virtual_hosted_style(),
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
        match normalize_s3_endpoint_and_bucket(endpoint, bucket).map_err(rewrap_s3_config_error)? {
            normalized if normalized.endpoint.is_empty() => {
                return Err(storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    format!("{label} is required"),
                ));
            }
            normalized => normalized,
        };

    let mut url = Url::parse(&normalized.endpoint).map_err(|error| {
        storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("invalid {label}: {error}"),
        )
    })?;
    match (
        url.username(),
        url.password(),
        url.query(),
        url.fragment(),
        url.path(),
    ) {
        (username, password, _, _, _) if !username.is_empty() || password.is_some() => {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                format!("{label} must not contain credentials"),
            ));
        }
        (_, _, Some(_), _, _) | (_, _, _, Some(_), _) => {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                format!("{label} must not contain query or fragment components"),
            ));
        }
        (_, _, _, _, "" | "/") => {}
        _ => {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                format!("{label} must not contain a path"),
            ));
        }
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
    match (use_cname, is_aliyun_host) {
        (true, true) => {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "OSS CNAME mode requires a custom-domain endpoint",
            ));
        }
        (false, false) => {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "OSS endpoint must use an aliyuncs.com host unless CNAME mode is enabled",
            ));
        }
        (true, false) => {}
        (false, true) => {
            let bucket_prefix = format!("{}.", normalized.bucket);
            let root_host = host.strip_prefix(&bucket_prefix).unwrap_or(&host);
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
    match valid {
        true => Ok(()),
        false => Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "OSS bucket must be 3-63 lowercase letters, digits, or hyphens and start/end with a letter or digit",
        )),
    }
}

fn validate_oss_region(region: &str) -> Result<()> {
    let region = region.trim();
    match region {
        "" => Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "OSS region is required",
        )),
        region
            if region.len() > 128
                || !region
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-') =>
        {
            Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "OSS region contains invalid characters",
            ))
        }
        _ => Ok(()),
    }
}

fn rewrap_s3_config_error(error: S3ConfigError) -> aster_drive_storage::StorageError {
    let message = match error {
        S3ConfigError::MissingBucket => "OSS bucket is required".to_string(),
        S3ConfigError::InvalidEndpoint(message) => message.replace("S3 endpoint", "OSS endpoint"),
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

    fn max_single_put_size(&self) -> Option<u64> {
        self.storage.max_single_put_size()
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
            direct_download: Some(self),
            presigned_upload: Some(self),
            multipart: Some(self),
            ..self.storage.extensions()
        }
    }

    async fn capacity_info(&self) -> Result<StorageCapacityInfo> {
        self.storage.capacity_info().await
    }
}

#[async_trait::async_trait]
impl DirectDownloadStorageDriver for AlibabaOssDriver {
    async fn resolve_download_url(
        &self,
        path: &str,
        expires: Duration,
        options: DirectDownloadOptions,
    ) -> Result<Option<aster_drive_storage::DirectDownloadRequest>> {
        match &self.download_delivery {
            DownloadDelivery::AlibabaCdnTypeA(config) => {
                config.download_request(path, expires, &options)
            }
            DownloadDelivery::OssOrigin => {
                // OSS rejects response-content-type with 0017-00000902.
                // The object's stored Content-Type remains authoritative.
                // https://help.aliyun.com/zh/oss/support/0017-00000902
                self.public_driver
                    .resolve_download_url(
                        path,
                        expires,
                        DirectDownloadOptions {
                            response_content_type: None,
                            ..options
                        },
                    )
                    .await
            }
        }
    }
}

#[async_trait::async_trait]
impl PresignedUploadStorageDriver for AlibabaOssDriver {
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
    fn capabilities(&self) -> MultipartStorageCapabilities {
        MultipartStorageCapabilities {
            min_part_size: 100 * 1024,
            max_part_size: Some(5 * 1024 * 1024 * 1024),
            max_parts: 10_000,
            max_object_size: None,
            upload_mode: MultipartUploadMode::NativeStreaming,
        }
    }

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

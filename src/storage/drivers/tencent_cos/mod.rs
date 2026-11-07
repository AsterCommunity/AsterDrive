//! Provider evidence: developer-docs/zh-CN/reference/storage-provider-evidence.md#tencent_cos
//! 腾讯云 COS 存储驱动。
//!
//! 基础对象读写复用 S3 兼容驱动；COS/CI 数据处理使用 COS 原生 query
//! 签名，因为 CI 处理参数必须参与签名，不能追加在普通 S3 presigned URL 后面。

mod cdn;
pub(crate) mod cors;
pub use cdn::{CosDownloadDelivery, default_cdn_auth_validity_seconds, validate_cdn_auth_key};
mod native_media_metadata;
mod native_thumbnail;
mod signing;
#[cfg(test)]
mod tests;

use std::{sync::Arc, time::Duration};

use url::Url;

use super::s3::{S3Driver, S3DriverConfig, S3DriverOptions, S3StaticCredentials};
use super::s3_compatible::S3CompatibleDriver;
use super::s3_config::{S3ConfigError, normalize_s3_endpoint_and_bucket};
use aster_drive_storage::error::{StorageErrorKind, storage_driver_error};
use aster_drive_storage::object_key;
use aster_drive_storage::traits::driver::{DirectDownloadOptions, DirectDownloadRequest};
use aster_drive_storage::traits::extensions::{
    DirectDownloadStorageDriver, PresignedUploadStorageDriver,
};
use aster_drive_storage::{
    MapStorageErr, MultipartStorageCapabilities, MultipartUploadMode, Result,
};

pub(super) const COS_NATIVE_PROCESSING_PROVIDER: &str = "tencent_cos_ci";
pub(super) const MAX_COS_THUMBNAIL_TTL: Duration = Duration::from_secs(5 * 60);

fn non_empty_xml_text(text: Option<&str>) -> Option<String> {
    let trimmed = text?.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

pub struct TencentCosDriver {
    storage: S3CompatibleDriver,
    client: aster_forge_http::HttpClient,
    endpoint: String,
    bucket: String,
    access_key: String,
    secret_key: String,
    base_path: String,
    use_cname: bool,
    download_base_url: Option<String>,
    download_delivery: CosDownloadDelivery,
    cdn_auth_validity_seconds: u32,
    cdn_auth_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TencentCosDriverConfig {
    pub endpoint: String,
    pub bucket: String,
    pub base_path: String,
    pub use_cname: bool,
    pub download_base_url: Option<String>,
    pub download_delivery: CosDownloadDelivery,
    pub cdn_auth_validity_seconds: u32,
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
    pub operation_timeout: Duration,
}

#[derive(Clone, PartialEq, Eq)]
pub struct TencentCosStaticCredentials {
    pub access_key: String,
    pub secret_key: String,
    pub cdn_auth_key: Option<String>,
}

impl std::fmt::Debug for TencentCosStaticCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TencentCosStaticCredentials")
            .finish_non_exhaustive()
    }
}

impl TencentCosDriver {
    pub fn validate_connection_config(config: &TencentCosDriverConfig) -> Result<()> {
        match config.download_delivery {
            CosDownloadDelivery::CosOrigin => {}
            CosDownloadDelivery::CustomOrigin => {
                let base = config.download_base_url.as_deref().ok_or_else(|| {
                    storage_driver_error(
                        StorageErrorKind::Misconfigured,
                        "Custom COS origin download URL is required",
                    )
                })?;
                cdn::validated_base_url(base)?;
            }
            CosDownloadDelivery::TencentCdnTypeDSha256 => {
                let base = config.download_base_url.as_deref().ok_or_else(|| {
                    storage_driver_error(
                        StorageErrorKind::Misconfigured,
                        "Tencent CDN download URL is required",
                    )
                })?;
                cdn::validated_base_url(base)?;
                if !(1..=3600).contains(&config.cdn_auth_validity_seconds) {
                    return Err(storage_driver_error(
                        StorageErrorKind::Misconfigured,
                        "Tencent CDN authentication validity must be between 1 and 3600 seconds",
                    ));
                }
            }
        }
        let normalized = normalize_s3_endpoint_and_bucket(&config.endpoint, &config.bucket)
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
        if !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
            || config.endpoint.contains('\\')
            || config.endpoint.chars().any(char::is_control)
        {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "COS endpoint must not contain credentials, query, fragment, backslashes, or control characters",
            ));
        }
        if config.use_cname && endpoint.path() != "/" && !endpoint.path().is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "COS CNAME endpoint must not contain a path; use base_path for object key prefixes",
            ));
        }
        if !config.use_cname && !host.ends_with(".myqcloud.com") {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "COS endpoint must use a Tencent COS myqcloud.com host unless CNAME mode is enabled",
            ));
        }
        if let Some(download_base_url) = config.download_base_url.as_deref() {
            let url = Url::parse(download_base_url).map_storage_err_ctx(
                StorageErrorKind::Misconfigured,
                "parse COS download base URL",
            )?;
            if url.scheme() != "https"
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.port().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "COS download base URL must be an HTTPS URL with a hostname and no credentials, non-default port, query, or fragment",
                ));
            }
        }
        Ok(())
    }

    pub fn validate_config(
        config: &TencentCosDriverConfig,
        credentials: &TencentCosStaticCredentials,
    ) -> Result<()> {
        Self::validate_connection_config(config)?;
        match (
            config.download_delivery,
            credentials.cdn_auth_key.as_deref(),
        ) {
            (
                CosDownloadDelivery::CosOrigin | CosDownloadDelivery::CustomOrigin,
                None | Some(""),
            ) => {}
            (
                CosDownloadDelivery::CosOrigin
                | CosDownloadDelivery::CustomOrigin
                | CosDownloadDelivery::TencentCdnTypeDSha256,
                Some(key),
            ) => {
                cdn::validate_cdn_auth_key(key)?;
            }
            (CosDownloadDelivery::TencentCdnTypeDSha256, None) => {
                return Err(storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "Tencent CDN authentication key is required",
                ));
            }
        }
        let normalized = normalize_s3_endpoint_and_bucket(&config.endpoint, &config.bucket)
            .map_err(Self::rewrap_s3_config_error)?;
        S3Driver::validate_config(
            &S3DriverConfig {
                endpoint: signing::cos_s3_endpoint(
                    &normalized.endpoint,
                    &normalized.bucket,
                    config.use_cname,
                )?,
                bucket: normalized.bucket,
                base_path: config.base_path.clone(),
                region: "auto".to_string(),
                path_style: config.use_cname,
                connect_timeout: config.connect_timeout,
                read_timeout: config.read_timeout,
                operation_timeout: config.operation_timeout,
            },
            &S3StaticCredentials {
                access_key: credentials.access_key.clone(),
                secret_key: credentials.secret_key.clone(),
            },
        )?;
        Ok(())
    }

    pub fn new(
        config: TencentCosDriverConfig,
        credentials: TencentCosStaticCredentials,
    ) -> Result<Self> {
        Self::validate_config(&config, &credentials)?;
        let normalized = normalize_s3_endpoint_and_bucket(&config.endpoint, &config.bucket)
            .map_err(Self::rewrap_s3_config_error)?;
        let s3_driver = S3Driver::new(
            S3DriverConfig {
                endpoint: signing::cos_s3_endpoint(
                    &normalized.endpoint,
                    &normalized.bucket,
                    config.use_cname,
                )?,
                bucket: normalized.bucket.clone(),
                base_path: config.base_path.clone(),
                region: "auto".to_string(),
                path_style: config.use_cname,
                connect_timeout: config.connect_timeout,
                read_timeout: config.read_timeout,
                operation_timeout: config.operation_timeout,
            },
            S3StaticCredentials {
                access_key: credentials.access_key.clone(),
                secret_key: credentials.secret_key.clone(),
            },
            match config.use_cname {
                true => S3DriverOptions::path_style(),
                false => S3DriverOptions::virtual_hosted_style(),
            },
            |builder| {
                signing::configure_cos_auth(
                    builder,
                    config.use_cname.then(|| normalized.bucket.clone()),
                )
            },
        )?;
        let storage = S3CompatibleDriver::from_s3_driver_with_multipart_capabilities(
            Arc::new(s3_driver),
            MultipartStorageCapabilities {
                // Tencent COS Upload Part official contract: up to 10,000
                // parts, each 1 MiB..5 GiB; the final part may be smaller.
                min_part_size: 1024 * 1024,
                max_part_size: Some(5 * 1024 * 1024 * 1024),
                max_parts: 10_000,
                max_object_size: None,
                upload_mode: MultipartUploadMode::NativeStreaming,
            },
        );
        let client = cos_ci_http_client(&config)?;

        Ok(Self {
            storage,
            client,
            endpoint: normalized.endpoint,
            bucket: normalized.bucket,
            access_key: credentials.access_key,
            secret_key: credentials.secret_key,
            base_path: config.base_path,
            use_cname: config.use_cname,
            download_base_url: config.download_base_url,
            download_delivery: config.download_delivery,
            cdn_auth_validity_seconds: config.cdn_auth_validity_seconds,
            cdn_auth_key: credentials.cdn_auth_key,
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

fn cos_ci_http_client(config: &TencentCosDriverConfig) -> Result<aster_forge_http::HttpClient> {
    aster_drive_outbound::ClientConfig::new(aster_drive_outbound::Profile::ProviderControl)
        .connect_timeout(config.connect_timeout)
        .read_timeout(config.read_timeout)
        .timeout(config.operation_timeout)
        .build()
        .map_storage_err_ctx(StorageErrorKind::Misconfigured, "build COS CI HTTP client")
}

#[async_trait::async_trait]
impl DirectDownloadStorageDriver for TencentCosDriver {
    async fn resolve_download_url(
        &self,
        path: &str,
        expires: Duration,
        options: DirectDownloadOptions,
    ) -> Result<Option<DirectDownloadRequest>> {
        match self.download_delivery {
            CosDownloadDelivery::TencentCdnTypeDSha256 => {
                self.cdn_download_request(path, expires, &options)
            }
            CosDownloadDelivery::CustomOrigin => {
                let expires =
                    super::s3::presigned::clamp_presign_ttl(expires, "COS resolve_download_url");
                let url = self.signed_custom_download_url(path, expires, &options)?;
                Ok(Some(DirectDownloadRequest::temporary_url(url, expires)))
            }
            CosDownloadDelivery::CosOrigin => {
                self.storage
                    .resolve_download_url(path, expires, options)
                    .await
            }
        }
    }
}

#[async_trait::async_trait]
impl PresignedUploadStorageDriver for TencentCosDriver {
    async fn presigned_put_request(
        &self,
        path: &str,
        expires: Duration,
    ) -> Result<Option<aster_drive_storage::PresignedUploadRequest>> {
        let _ = (path, expires);
        // Static COS signatures do not forbid unsigned copy-source headers.
        // Browser writes must use relay until per-upload scoped credentials exist.
        Ok(None)
    }

    fn presigned_single_put_requires_etag(&self) -> bool {
        // Single-object completion verifies the uploaded object's metadata and
        // size server-side. ETag remains required for presigned multipart parts.
        false
    }
}

super::s3_compatible::delegate_s3_compatible_storage_driver!(
    TencentCosDriver,
    storage,
    native_thumbnail,
    native_media_metadata
);

#[async_trait::async_trait]
impl aster_drive_storage::MultipartStorageDriver for TencentCosDriver {
    fn capabilities(&self) -> aster_drive_storage::traits::multipart::MultipartStorageCapabilities {
        aster_drive_storage::MultipartStorageDriver::capabilities(&self.storage)
    }

    async fn create_multipart_upload(&self, path: &str) -> aster_drive_storage::Result<String> {
        self.storage.create_multipart_upload(path).await
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
        data: bytes::Bytes,
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
        reader: Box<dyn tokio::io::AsyncRead + Unpin + Send + Sync>,
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
    ) -> aster_drive_storage::Result<Vec<aster_drive_storage::UploadedMultipartPart>> {
        self.storage
            .list_uploaded_part_details(path, upload_id)
            .await
    }
}

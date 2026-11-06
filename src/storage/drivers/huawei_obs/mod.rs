//! Provider evidence: developer-docs/zh-CN/reference/storage-provider-evidence.md#huawei_obs
//! Huawei Cloud OBS storage driver.
//!
//! Object operations reuse the AWS S3 serializer, while `signing` replaces
//! AWS SigV4 with Huawei OBS `SignatureObs`. This keeps range reads, multipart
//! uploads, copies, streaming uploads, and presigned operations on one
//! executable OBS request contract instead of treating OBS as generic S3.
//! Object listing uses OBS' native marker-based ListObjects contract rather
//! than S3 ListObjectsV2.

pub mod cdn;
pub(crate) mod cors;
mod signing;
#[cfg(test)]
mod tests;

use std::{sync::Arc, time::Duration};

use url::Url;

use super::s3::{S3Driver, S3DriverConfig, S3DriverOptions, S3StaticCredentials};
use super::s3_compatible::S3CompatibleDriver;
use super::s3_config::{S3ConfigError, normalize_s3_endpoint_and_bucket};
use aster_drive_storage::Result;
use aster_drive_storage::error::{StorageErrorKind, storage_driver_error};
use aster_drive_storage::{MultipartStorageCapabilities, MultipartUploadMode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HuaweiObsDriverConfig {
    pub endpoint: String,
    pub bucket: String,
    pub base_path: String,
    pub region: String,
    pub use_cname: bool,
    pub download_delivery: cdn::ObsDownloadDelivery,
    pub cdn_domain: Option<String>,
    pub cdn_auth_validity_seconds: u32,
    pub cdn_auth_param: String,
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
    pub operation_timeout: Duration,
}

#[derive(Clone, PartialEq, Eq)]
pub struct HuaweiObsStaticCredentials {
    pub access_key: String,
    pub secret_key: String,
    pub cdn_auth_key: Option<String>,
}

impl std::fmt::Debug for HuaweiObsStaticCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HuaweiObsStaticCredentials")
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedHuaweiObsEndpoint {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
}

pub struct HuaweiObsDriver {
    storage: S3CompatibleDriver,
    download_delivery: DownloadDelivery,
}

enum DownloadDelivery {
    ObsOrigin,
    HuaweiCdnTypeASha256(cdn::CdnConfig),
}

impl HuaweiObsDriver {
    pub fn normalize_endpoint(
        endpoint: &str,
        bucket: &str,
        region: &str,
        use_cname: bool,
    ) -> Result<NormalizedHuaweiObsEndpoint> {
        let normalized = normalize_s3_endpoint_and_bucket(endpoint, bucket)
            .map_err(Self::rewrap_s3_config_error)?;
        validate_bucket_name(&normalized.bucket)?;

        let mut endpoint = Url::parse(&normalized.endpoint).map_err(|error| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                format!("invalid Huawei OBS endpoint URL: {error}"),
            )
        })?;
        if endpoint.username() != ""
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
            || !matches!(endpoint.path(), "" | "/")
        {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Huawei OBS endpoint must contain only scheme, host, and optional port",
            ));
        }
        let host = endpoint
            .host_str()
            .ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "Huawei OBS endpoint is missing a host",
                )
            })?
            .to_string();
        let region = normalize_region(region)?;

        if !use_cname {
            if region.is_empty() {
                return Err(storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "obs_region is required for virtual-hosted Huawei OBS endpoints",
                ));
            }
            let root_host = host
                .strip_prefix(&format!("{}.", normalized.bucket))
                .unwrap_or(&host)
                .to_string();
            if !is_official_obs_endpoint(&root_host, &region) {
                return Err(storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    format!(
                        "Huawei OBS virtual-hosted endpoint must match obs.{region}.myhuaweicloud.com (or the documented .eu endpoint)"
                    ),
                ));
            }
            endpoint.set_host(Some(&root_host)).map_err(|_| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "failed to normalize Huawei OBS endpoint host",
                )
            })?;
        } else if is_any_official_obs_endpoint(&host) {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "custom-domain addressing requires the mapped custom hostname, not a bucket OBS endpoint",
            ));
        }

        endpoint.set_path("");
        Ok(NormalizedHuaweiObsEndpoint {
            endpoint: String::from(endpoint).trim_end_matches('/').to_string(),
            bucket: normalized.bucket,
            region,
        })
    }

    pub fn validate_connection_config(config: &HuaweiObsDriverConfig) -> Result<()> {
        Self::normalize_endpoint(
            &config.endpoint,
            &config.bucket,
            &config.region,
            config.use_cname,
        )?;
        match config.download_delivery {
            cdn::ObsDownloadDelivery::ObsOrigin => {}
            cdn::ObsDownloadDelivery::HuaweiCdnTypeASha256 => {
                let domain = config.cdn_domain.as_deref().ok_or_else(|| {
                    storage_driver_error(
                        StorageErrorKind::Misconfigured,
                        "Huawei CDN download domain is required",
                    )
                })?;
                cdn::validated_cdn_domain(domain)?;
                cdn::validate_cdn_auth_validity(config.cdn_auth_validity_seconds)?;
                cdn::validate_cdn_auth_param(&config.cdn_auth_param)?;
            }
        }
        Ok(())
    }

    pub fn validate_config(
        config: &HuaweiObsDriverConfig,
        credentials: &HuaweiObsStaticCredentials,
    ) -> Result<()> {
        Self::validate_connection_config(config)?;
        match (
            config.download_delivery,
            credentials.cdn_auth_key.as_deref(),
        ) {
            (cdn::ObsDownloadDelivery::ObsOrigin, _) => {}
            (cdn::ObsDownloadDelivery::HuaweiCdnTypeASha256, Some(key)) => {
                cdn::validate_cdn_auth_key(key)?
            }
            (cdn::ObsDownloadDelivery::HuaweiCdnTypeASha256, None) => {
                return Err(storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "Huawei CDN authentication key is required",
                ));
            }
        }
        let normalized = Self::normalize_endpoint(
            &config.endpoint,
            &config.bucket,
            &config.region,
            config.use_cname,
        )?;
        S3Driver::validate_config(
            &S3DriverConfig {
                endpoint: normalized.endpoint,
                bucket: normalized.bucket,
                base_path: config.base_path.clone(),
                region: sdk_region(&normalized.region),
                path_style: config.use_cname,
                connect_timeout: config.connect_timeout,
                read_timeout: config.read_timeout,
                operation_timeout: config.operation_timeout,
            },
            &S3StaticCredentials {
                access_key: credentials.access_key.clone(),
                secret_key: credentials.secret_key.clone(),
            },
        )
    }

    pub fn new(
        config: HuaweiObsDriverConfig,
        credentials: HuaweiObsStaticCredentials,
    ) -> Result<Self> {
        Self::validate_config(&config, &credentials)?;
        let normalized = Self::normalize_endpoint(
            &config.endpoint,
            &config.bucket,
            &config.region,
            config.use_cname,
        )?;
        let bucket = normalized.bucket.clone();
        let use_cname = config.use_cname;
        let download_delivery = match (
            config.download_delivery,
            config.cdn_domain,
            credentials.cdn_auth_key,
        ) {
            (cdn::ObsDownloadDelivery::ObsOrigin, _, _) => DownloadDelivery::ObsOrigin,
            (cdn::ObsDownloadDelivery::HuaweiCdnTypeASha256, Some(domain), Some(auth_key)) => {
                DownloadDelivery::HuaweiCdnTypeASha256(cdn::CdnConfig {
                    domain: cdn::validated_cdn_domain(&domain)?,
                    auth_key,
                    validity_seconds: config.cdn_auth_validity_seconds,
                    auth_param: config.cdn_auth_param,
                    base_path: config.base_path.clone(),
                })
            }
            (cdn::ObsDownloadDelivery::HuaweiCdnTypeASha256, _, _) => {
                return Err(storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "Huawei CDN domain and authentication key are required",
                ));
            }
        };
        let storage = S3CompatibleDriver::from_s3_driver_with_multipart_capabilities(
            Arc::new(S3Driver::new(
                S3DriverConfig {
                    endpoint: normalized.endpoint,
                    bucket: normalized.bucket,
                    base_path: config.base_path,
                    region: sdk_region(&normalized.region),
                    path_style: config.use_cname,
                    connect_timeout: config.connect_timeout,
                    read_timeout: config.read_timeout,
                    operation_timeout: config.operation_timeout,
                },
                S3StaticCredentials {
                    access_key: credentials.access_key,
                    secret_key: credentials.secret_key,
                },
                match use_cname {
                    true => S3DriverOptions::path_style(),
                    false => S3DriverOptions::virtual_hosted_style(),
                },
                move |builder| signing::configure_obs_auth(builder, bucket, use_cname),
            )?),
            MultipartStorageCapabilities {
                // Huawei OBS UploadPart official contract (2026-09-10):
                // part size [100 KB, 5 GB], final part [0, 5 GB], number [1, 10000].
                min_part_size: 100 * 1024,
                max_part_size: Some(5 * 1024 * 1024 * 1024),
                max_parts: 10_000,
                max_object_size: None,
                upload_mode: MultipartUploadMode::NativeStreaming,
            },
        );
        Ok(Self {
            storage,
            download_delivery,
        })
    }

    pub fn s3_driver(&self) -> Arc<S3Driver> {
        self.storage.s3_driver()
    }

    fn rewrap_s3_config_error(error: S3ConfigError) -> aster_drive_storage::StorageError {
        let message = match error {
            S3ConfigError::MissingBucket => "bucket is required for Huawei OBS".to_string(),
            S3ConfigError::InvalidEndpoint(message) => message,
        };
        storage_driver_error(StorageErrorKind::Misconfigured, message)
    }
}

#[async_trait::async_trait]
impl aster_drive_storage::DirectDownloadStorageDriver for HuaweiObsDriver {
    async fn resolve_download_url(
        &self,
        path: &str,
        expires: Duration,
        options: aster_drive_storage::DirectDownloadOptions,
    ) -> Result<Option<aster_drive_storage::DirectDownloadRequest>> {
        match &self.download_delivery {
            DownloadDelivery::ObsOrigin => {
                self.storage
                    .resolve_download_url(path, expires, options)
                    .await
            }
            DownloadDelivery::HuaweiCdnTypeASha256(config) => {
                config.download_request(path, expires, &options)
            }
        }
    }
}

#[async_trait::async_trait]
impl aster_drive_storage::PresignedUploadStorageDriver for HuaweiObsDriver {
    async fn presigned_put_request(
        &self,
        path: &str,
        expires: Duration,
    ) -> Result<Option<aster_drive_storage::PresignedUploadRequest>> {
        self.storage.presigned_put_request(path, expires).await
    }

    fn presigned_single_put_requires_etag(&self) -> bool {
        false
    }
}

fn sdk_region(region: &str) -> String {
    if region.is_empty() {
        "auto".to_string()
    } else {
        region.to_string()
    }
}

fn normalize_region(region: &str) -> Result<String> {
    let region = region.trim().to_ascii_lowercase();
    if region.len() > 128
        || region
            .bytes()
            .any(|byte| !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'))
    {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "obs_region must contain only lowercase ASCII letters, digits, and '-'",
        ));
    }
    Ok(region)
}

fn validate_bucket_name(bucket: &str) -> Result<()> {
    let valid = (3..=63).contains(&bucket.len())
        && bucket.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'.')
        })
        && bucket
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && bucket
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
        && !bucket.contains("..")
        && !bucket.split('.').any(|label| label.is_empty());
    if !valid {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "Huawei OBS bucket must be 3-63 lowercase letters, digits, dots, or hyphens and start/end with a letter or digit",
        ));
    }
    Ok(())
}

fn is_official_obs_endpoint(host: &str, region: &str) -> bool {
    let prefix = format!("obs.{region}.");
    host.strip_prefix(&prefix)
        .is_some_and(|suffix| matches!(suffix, "myhuaweicloud.com" | "myhuaweicloud.eu"))
}

fn is_any_official_obs_endpoint(host: &str) -> bool {
    let Some(rest) = host
        .strip_prefix("obs.")
        .or_else(|| host.split_once(".obs.").map(|(_, rest)| rest))
    else {
        return false;
    };
    rest.ends_with(".myhuaweicloud.com") || rest.ends_with(".myhuaweicloud.eu")
}

#[async_trait::async_trait]
impl aster_drive_storage::traits::extensions::ListStorageDriver for HuaweiObsDriver {
    async fn list_paths(&self, prefix: Option<&str>) -> Result<Vec<String>> {
        self.s3_driver().list_paths_v1(prefix).await
    }

    async fn scan_paths(
        &self,
        prefix: Option<&str>,
        visitor: &mut dyn aster_drive_storage::traits::driver::StoragePathVisitor,
    ) -> Result<()> {
        self.s3_driver().scan_paths_v1(prefix, visitor).await
    }
}

super::s3_compatible::delegate_s3_compatible_storage_driver!(HuaweiObsDriver, storage, list = self);
super::s3_compatible::delegate_s3_compatible_multipart_driver!(HuaweiObsDriver, storage);

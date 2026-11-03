//! Huawei Cloud OBS storage driver.
//!
//! Object operations reuse the AWS S3 serializer, while `signing` replaces
//! AWS SigV4 with Huawei OBS `SignatureObs`. This keeps range reads, multipart
//! uploads, copies, streaming uploads, and presigned operations on one
//! executable OBS request contract instead of treating OBS as generic S3.
//! Object listing uses OBS' native marker-based ListObjects contract rather
//! than S3 ListObjectsV2.

mod cdn;
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
    pub download_base_url: Option<String>,
    pub cdn_auth_type: String,
    pub cdn_expire_time: u64,
    pub cdn_sign_arg: String,
    pub cdn_time_arg: String,
    pub cdn_signing_algorithm: String,
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
    pub operation_timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HuaweiObsStaticCredentials {
    pub access_key: String,
    pub secret_key: String,
    pub cdn_primary_key: Option<String>,
    pub cdn_backup_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedHuaweiObsEndpoint {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
}

pub struct HuaweiObsDriver {
    storage: S3CompatibleDriver,
    download_base_url: Option<Url>,
    cdn_auth_type: String,
    cdn_expire_time: Duration,
    cdn_sign_arg: String,
    cdn_time_arg: String,
    cdn_signing_algorithm: String,
    cdn_primary_key: Option<String>,
    cdn_backup_key: Option<String>,
    base_path: String,
}

impl HuaweiObsDriver {
    pub fn validate_download_config(
        download_base_url: Option<&str>,
        auth_type: &str,
        expire_time: u64,
        sign_arg: &str,
        time_arg: &str,
        signing_algorithm: &str,
    ) -> Result<()> {
        if download_base_url.is_none() {
            return Ok(());
        }
        let auth_type = auth_type.trim();
        if !matches!(
            auth_type,
            "type_a" | "type_b" | "type_c1" | "type_c2" | "public"
        ) {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Huawei CDN auth type must be type_a, type_b, type_c1, type_c2, or public",
            ));
        }
        if auth_type == "public" {
            return Ok(());
        }
        if sign_arg.trim().is_empty() || time_arg.trim().is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Huawei CDN auth parameter names are required",
            ));
        }
        for (name, value) in [("sign_arg", sign_arg), ("time_arg", time_arg)] {
            if !value
                .trim()
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphabetic)
                || !value
                    .trim()
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            {
                return Err(storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    format!(
                        "Huawei CDN {name} must start with a letter and contain only letters, digits, and underscores"
                    ),
                ));
            }
        }
        if auth_type == "type_b" && !time_arg.trim().eq_ignore_ascii_case("timestamp") {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Huawei CDN type_b uses the fixed timestamp parameter name",
            ));
        }
        if !matches!(signing_algorithm.trim(), "md5" | "sha256") {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Huawei CDN signing algorithm must be md5 or sha256",
            ));
        }
        if expire_time == 0 {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Huawei CDN expire_time must be greater than zero for authenticated downloads",
            ));
        }
        Ok(())
    }

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

    pub fn validate_config(
        config: &HuaweiObsDriverConfig,
        credentials: &HuaweiObsStaticCredentials,
    ) -> Result<()> {
        let normalized = Self::normalize_endpoint(
            &config.endpoint,
            &config.bucket,
            &config.region,
            config.use_cname,
        )?;
        Self::validate_download_config(
            config.download_base_url.as_deref(),
            &config.cdn_auth_type,
            config.cdn_expire_time,
            &config.cdn_sign_arg,
            &config.cdn_time_arg,
            &config.cdn_signing_algorithm,
        )?;
        S3Driver::validate_config(
            &S3DriverConfig {
                endpoint: normalized.endpoint,
                bucket: normalized.bucket,
                base_path: config.base_path.clone(),
                region: sdk_region(&normalized.region),
                path_style: false,
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
        let normalized = Self::normalize_endpoint(
            &config.endpoint,
            &config.bucket,
            &config.region,
            config.use_cname,
        )?;
        let bucket = normalized.bucket.clone();
        let use_cname = config.use_cname;
        let storage = S3CompatibleDriver::from_s3_driver_with_multipart_capabilities(
            Arc::new(S3Driver::new(
                S3DriverConfig {
                    endpoint: normalized.endpoint,
                    bucket: normalized.bucket,
                    base_path: config.base_path.clone(),
                    region: sdk_region(&normalized.region),
                    path_style: false,
                    connect_timeout: config.connect_timeout,
                    read_timeout: config.read_timeout,
                    operation_timeout: config.operation_timeout,
                },
                S3StaticCredentials {
                    access_key: credentials.access_key,
                    secret_key: credentials.secret_key,
                },
                S3DriverOptions::virtual_hosted_style(),
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
        let download_base_url = config
            .download_base_url
            .as_deref()
            .map(Url::parse)
            .transpose()
            .map_err(|_| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "Huawei CDN download URL is invalid",
                )
            })?;
        for (name, key) in [
            ("cdn_primary_key", credentials.cdn_primary_key.as_deref()),
            ("cdn_backup_key", credentials.cdn_backup_key.as_deref()),
        ] {
            if let Some(key) = key {
                validate_cdn_key(name, key)?;
            }
        }
        if config.download_base_url.is_some() && config.cdn_auth_type.trim() != "public" {
            let _ = Self::cdn_authentication_key(
                credentials.cdn_primary_key.as_deref(),
                credentials.cdn_backup_key.as_deref(),
            )?;
        }
        Ok(Self {
            storage,
            download_base_url,
            cdn_auth_type: config.cdn_auth_type,
            cdn_expire_time: Duration::from_secs(config.cdn_expire_time),
            cdn_sign_arg: config.cdn_sign_arg,
            cdn_time_arg: config.cdn_time_arg,
            cdn_signing_algorithm: config.cdn_signing_algorithm,
            cdn_primary_key: credentials.cdn_primary_key,
            cdn_backup_key: credentials.cdn_backup_key,
            base_path: config.base_path,
        })
    }

    fn cdn_authentication_key<'a>(
        primary_key: Option<&'a str>,
        backup_key: Option<&'a str>,
    ) -> Result<&'a str> {
        primary_key
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .or_else(|| backup_key.map(str::trim).filter(|key| !key.is_empty()))
            .ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "Huawei CDN authenticated downloads require a primary or backup authentication key",
                )
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
        if let Some(base_url) = &self.download_base_url {
            let ttl = expires.min(self.cdn_expire_time);
            let url = cdn::signed_download_url(
                base_url,
                &self.base_path,
                path,
                ttl,
                &self.cdn_auth_type,
                &self.cdn_sign_arg,
                &self.cdn_time_arg,
                &self.cdn_signing_algorithm,
                self.cdn_primary_key.as_deref(),
                self.cdn_backup_key.as_deref(),
                &options,
            )?;
            return Ok(Some(
                aster_drive_storage::DirectDownloadRequest::temporary_url(url, ttl),
            ));
        }
        self.storage
            .resolve_download_url(path, expires, options)
            .await
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

fn validate_cdn_key(name: &str, key: &str) -> Result<()> {
    let valid =
        (6..=32).contains(&key.len()) && key.bytes().all(|byte| byte.is_ascii_alphanumeric());
    if !valid {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("{name} must be 6-32 ASCII letters or digits"),
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

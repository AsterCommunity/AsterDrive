//! Qiniu Kodo S3-compatible storage driver.
//!
//! Qiniu's S3-compatible API uses the standard S3 data plane and AWS SigV4.
//! Provider-specific QBox, UpToken, and form-upload protocols deliberately do
//! not cross this boundary.

#[cfg(test)]
mod tests;

use std::{sync::Arc, time::Duration};

use aster_drive_storage::Result;
use aster_drive_storage::error::{StorageErrorKind, storage_driver_error};
use aster_drive_storage::traits::extensions::PresignedStorageDriver;

use super::s3::{S3Driver, S3DriverConfig, S3DriverOptions, S3StaticCredentials};
use super::s3_compatible::{
    S3CompatibleDriver, delegate_s3_compatible_multipart_driver,
    delegate_s3_compatible_storage_driver,
};
use super::s3_config::{S3ConfigError, normalize_s3_endpoint_and_bucket, validate_s3_region};

pub struct QiniuDriver {
    storage: S3CompatibleDriver,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QiniuDriverConfig {
    pub endpoint: String,
    pub bucket: String,
    pub base_path: String,
    pub region: String,
    pub path_style: bool,
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
    pub operation_timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QiniuStaticCredentials {
    pub access_key: String,
    pub secret_key: String,
}

impl QiniuDriver {
    pub fn validate_config(
        config: &QiniuDriverConfig,
        credentials: &QiniuStaticCredentials,
    ) -> Result<()> {
        let normalized = normalize_s3_endpoint_and_bucket(&config.endpoint, &config.bucket)
            .map_err(Self::rewrap_s3_config_error)?;
        if normalized.endpoint.is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Qiniu S3 endpoint cannot be empty",
            ));
        }
        validate_s3_region(&config.region).map_err(Self::rewrap_s3_config_error)?;
        S3Driver::validate_config(
            &Self::s3_config(config),
            &S3StaticCredentials {
                access_key: credentials.access_key.clone(),
                secret_key: credentials.secret_key.clone(),
            },
        )
    }

    pub fn new(config: QiniuDriverConfig, credentials: QiniuStaticCredentials) -> Result<Self> {
        Self::validate_config(&config, &credentials)?;
        let path_style = config.path_style;
        let s3_driver = S3Driver::new(
            Self::s3_config(&config),
            S3StaticCredentials {
                access_key: credentials.access_key,
                secret_key: credentials.secret_key,
            },
            if path_style {
                S3DriverOptions::path_style()
            } else {
                S3DriverOptions::virtual_hosted_style()
            },
            |builder| {
                builder
                    .request_checksum_calculation(
                        aws_sdk_s3::config::RequestChecksumCalculation::WhenRequired,
                    )
                    .response_checksum_validation(
                        aws_sdk_s3::config::ResponseChecksumValidation::WhenRequired,
                    )
            },
        )?;

        Ok(Self {
            storage: S3CompatibleDriver::from_s3_driver(Arc::new(s3_driver)),
        })
    }

    pub fn s3_driver(&self) -> Arc<S3Driver> {
        self.storage.s3_driver()
    }

    fn s3_config(config: &QiniuDriverConfig) -> S3DriverConfig {
        S3DriverConfig {
            endpoint: config.endpoint.clone(),
            bucket: config.bucket.clone(),
            base_path: config.base_path.clone(),
            region: config.region.clone(),
            path_style: config.path_style,
            connect_timeout: config.connect_timeout,
            read_timeout: config.read_timeout,
            operation_timeout: config.operation_timeout,
        }
    }

    fn rewrap_s3_config_error(error: S3ConfigError) -> aster_drive_storage::StorageError {
        let message = match error {
            S3ConfigError::MissingBucket => "Qiniu bucket is required".to_string(),
            S3ConfigError::InvalidEndpoint(message) => {
                message.replace("S3 endpoint", "Qiniu S3 endpoint")
            }
            S3ConfigError::InvalidRegion => {
                "Qiniu S3 region must be 1-128 printable ASCII characters without whitespace or '/'"
                    .to_string()
            }
        };
        storage_driver_error(StorageErrorKind::Misconfigured, message)
    }
}

#[async_trait::async_trait]
impl PresignedStorageDriver for QiniuDriver {
    async fn presigned_url(
        &self,
        path: &str,
        expires: Duration,
        options: aster_drive_storage::PresignedDownloadOptions,
    ) -> Result<Option<String>> {
        self.storage.presigned_url(path, expires, options).await
    }

    async fn presigned_put_request(
        &self,
        path: &str,
        expires: Duration,
    ) -> Result<Option<aster_drive_storage::PresignedUploadRequest>> {
        self.storage.presigned_put_request(path, expires).await
    }
}

delegate_s3_compatible_storage_driver!(QiniuDriver, storage);
delegate_s3_compatible_multipart_driver!(QiniuDriver, storage);

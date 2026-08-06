//! 存储驱动实现：`s3`。

mod error;
mod list;
mod multipart;
mod presigned;
mod storage_driver;
mod stream_upload;
#[cfg(test)]
mod tests;

use aws_credential_types::Credentials;
use aws_sdk_s3::Client;
use aws_sdk_s3::config::{BehaviorVersion, Region, timeout::TimeoutConfig};
use std::time::Duration;

use super::s3_config::normalize_s3_endpoint_and_bucket;
use aster_drive_storage::Result;
use aster_drive_storage::error::{StorageErrorKind, storage_driver_error};
use aster_drive_storage::object_key;

pub struct S3Driver {
    client: Client,
    bucket: String,
    base_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct S3DriverConfig {
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
pub struct S3StaticCredentials {
    pub access_key: String,
    pub secret_key: String,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct S3DriverOptions {
    pub force_path_style: Option<bool>,
}

impl S3DriverOptions {
    pub const fn path_style() -> Self {
        Self {
            force_path_style: Some(true),
        }
    }

    pub const fn virtual_hosted_style() -> Self {
        Self {
            force_path_style: Some(false),
        }
    }
}

impl S3Driver {
    pub fn validate_config(
        config: &S3DriverConfig,
        credentials: &S3StaticCredentials,
    ) -> Result<()> {
        normalize_s3_endpoint_and_bucket(&config.endpoint, &config.bucket)
            .map_err(Self::rewrap_s3_config_error)?;
        if credentials.access_key.trim().is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Auth,
                "access_key cannot be empty",
            ));
        }
        if credentials.secret_key.trim().is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Auth,
                "secret_key cannot be empty",
            ));
        }
        Ok(())
    }

    pub fn new<F>(
        driver_config: S3DriverConfig,
        credentials: S3StaticCredentials,
        driver_options: S3DriverOptions,
        configure: F,
    ) -> Result<Self>
    where
        F: FnOnce(aws_sdk_s3::config::Builder) -> aws_sdk_s3::config::Builder,
    {
        Self::validate_config(&driver_config, &credentials)?;
        let normalized =
            normalize_s3_endpoint_and_bucket(&driver_config.endpoint, &driver_config.bucket)
                .map_err(Self::rewrap_s3_config_error)?;

        let credentials = Credentials::new(
            credentials.access_key,
            credentials.secret_key,
            None,
            None,
            "aster-s3-driver",
        );

        let timeout_config = TimeoutConfig::builder()
            .connect_timeout(driver_config.connect_timeout)
            .read_timeout(driver_config.read_timeout)
            .operation_timeout(driver_config.operation_timeout)
            .build();
        let force_path_style = driver_options
            .force_path_style
            // Provider wrappers such as Tencent COS may override addressing
            // style explicitly; plain S3 policies read the persisted option.
            .unwrap_or(driver_config.path_style);

        let mut config_builder = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new(driver_config.region))
            .credentials_provider(credentials)
            .timeout_config(timeout_config)
            .force_path_style(force_path_style);

        if !normalized.endpoint.is_empty() {
            config_builder = config_builder.endpoint_url(&normalized.endpoint);
        }

        let config = configure(config_builder).build();
        let client = Client::from_conf(config);

        Ok(Self {
            client,
            bucket: normalized.bucket,
            base_path: driver_config.base_path,
        })
    }

    fn full_key(&self, path: &str) -> String {
        object_key::join_key_prefix(&self.base_path, path)
    }

    fn relative_key<'a>(&self, key: &'a str) -> Option<&'a str> {
        object_key::strip_key_prefix(&self.base_path, key)
    }

    fn normalize_multipart_etag(etag: &str) -> String {
        let etag = etag.trim();
        if etag.starts_with('"') && etag.ends_with('"') && etag.len() >= 2 {
            etag.to_string()
        } else {
            format!("\"{etag}\"")
        }
    }
}

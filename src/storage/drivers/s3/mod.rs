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

use super::s3_config::normalize_s3_endpoint_and_bucket;
use crate::entities::storage_policy;
use crate::errors::Result;
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::storage::object_key;

pub struct S3Driver {
    client: Client,
    bucket: String,
    base_path: String,
}

#[derive(Debug, Clone, Copy)]
pub struct S3DriverOptions {
    pub force_path_style: bool,
}

impl S3DriverOptions {
    pub const fn path_style() -> Self {
        Self {
            force_path_style: true,
        }
    }

    pub const fn virtual_hosted_style() -> Self {
        Self {
            force_path_style: false,
        }
    }
}

impl Default for S3DriverOptions {
    fn default() -> Self {
        Self::path_style()
    }
}

impl S3Driver {
    pub fn validate_policy(policy: &storage_policy::Model) -> Result<()> {
        normalize_s3_endpoint_and_bucket(&policy.endpoint, &policy.bucket)
            .map_err(Self::rewrap_message_as_storage_error)?;
        if policy.access_key.trim().is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Auth,
                "access_key cannot be empty",
            ));
        }
        if policy.secret_key.trim().is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Auth,
                "secret_key cannot be empty",
            ));
        }
        Ok(())
    }

    pub fn new(policy: &storage_policy::Model) -> Result<Self> {
        Self::new_with_options(policy, S3DriverOptions::default())
    }

    pub fn new_with_options(
        policy: &storage_policy::Model,
        driver_options: S3DriverOptions,
    ) -> Result<Self> {
        Self::validate_policy(policy)?;
        let normalized = normalize_s3_endpoint_and_bucket(&policy.endpoint, &policy.bucket)
            .map_err(Self::rewrap_message_as_storage_error)?;
        let options = crate::types::parse_storage_policy_options(policy.options.as_ref());

        let credentials = Credentials::new(
            &policy.access_key,
            &policy.secret_key,
            None,
            None,
            "aster-s3-driver",
        );

        let timeout_config = TimeoutConfig::builder()
            .connect_timeout(options.effective_s3_connect_timeout())
            .read_timeout(options.effective_s3_read_timeout())
            .operation_timeout(options.effective_s3_operation_timeout())
            .build();

        let mut config_builder = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new("auto"))
            .credentials_provider(credentials)
            .timeout_config(timeout_config)
            .force_path_style(driver_options.force_path_style);

        if !normalized.endpoint.is_empty() {
            config_builder = config_builder.endpoint_url(&normalized.endpoint);
        }

        let config = config_builder.build();
        let client = Client::from_conf(config);

        Ok(Self {
            client,
            bucket: normalized.bucket,
            base_path: policy.base_path.clone(),
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

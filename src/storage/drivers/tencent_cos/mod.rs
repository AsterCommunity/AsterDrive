//! 腾讯云 COS 存储驱动。
//!
//! 基础对象读写复用 S3 兼容驱动；COS/CI 数据处理使用 COS 原生 query
//! 签名，因为 CI 处理参数必须参与签名，不能追加在普通 S3 presigned URL 后面。

pub(crate) mod cors;
mod native_media_metadata;
mod native_thumbnail;
mod signing;
#[cfg(test)]
mod tests;

use std::{sync::Arc, time::Duration};

use url::Url;

use super::s3::{S3Driver, S3DriverOptions};
use super::s3_compatible::S3CompatibleDriver;
use super::s3_config::{S3ConfigError, normalize_s3_endpoint_and_bucket};
use crate::config::OUTBOUND_HTTP_USER_AGENT;
use aster_drive_model::entities::storage_policy;
use aster_drive_storage::error::{StorageErrorKind, storage_driver_error};
use aster_drive_storage::object_key;
use aster_drive_storage::{MapStorageErr, Result};

pub(super) const COS_NATIVE_PROCESSING_PROVIDER: &str = "tencent_cos_ci";
pub(super) const MAX_COS_THUMBNAIL_TTL: Duration = Duration::from_secs(5 * 60);

fn non_empty_xml_text(text: Option<&str>) -> Option<String> {
    let trimmed = text?.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

pub struct TencentCosDriver {
    storage: S3CompatibleDriver,
    client: reqwest::Client,
    endpoint: String,
    bucket: String,
    access_key: String,
    secret_key: String,
    base_path: String,
}

impl TencentCosDriver {
    pub fn validate_policy(policy: &storage_policy::Model) -> Result<()> {
        S3Driver::validate_policy(policy)?;
        let normalized = normalize_s3_endpoint_and_bucket(&policy.endpoint, &policy.bucket)
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
        if !host.ends_with(".myqcloud.com") {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "COS endpoint must use a Tencent COS myqcloud.com host",
            ));
        }
        Ok(())
    }

    pub fn new(policy: &storage_policy::Model) -> Result<Self> {
        Self::validate_policy(policy)?;
        let normalized = normalize_s3_endpoint_and_bucket(&policy.endpoint, &policy.bucket)
            .map_err(Self::rewrap_s3_config_error)?;
        let mut storage_policy = policy.clone();
        storage_policy.endpoint =
            signing::cos_virtual_hosted_s3_endpoint(&normalized.endpoint, &normalized.bucket)?;
        storage_policy.bucket = normalized.bucket.clone();
        let s3_driver = S3Driver::new(
            &storage_policy,
            S3DriverOptions::virtual_hosted_style(),
            signing::configure_cos_auth,
        )?;
        let storage = S3CompatibleDriver::from_s3_driver(Arc::new(s3_driver));
        let client = cos_ci_http_client(policy)?;

        Ok(Self {
            storage,
            client,
            endpoint: normalized.endpoint,
            bucket: normalized.bucket,
            access_key: policy.access_key.clone(),
            secret_key: policy.secret_key.clone(),
            base_path: policy.base_path.clone(),
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

fn cos_ci_http_client(policy: &storage_policy::Model) -> Result<reqwest::Client> {
    let options = aster_drive_model::types::parse_storage_policy_options(policy.options.as_ref());
    reqwest::Client::builder()
        .connect_timeout(options.effective_s3_connect_timeout())
        .read_timeout(options.effective_s3_read_timeout())
        .timeout(options.effective_s3_operation_timeout())
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(OUTBOUND_HTTP_USER_AGENT)
        .build()
        .map_storage_err_ctx(StorageErrorKind::Misconfigured, "build COS CI HTTP client")
}

super::s3_compatible::delegate_s3_compatible_storage_driver!(
    TencentCosDriver,
    storage,
    native_thumbnail,
    native_media_metadata
);
super::s3_compatible::delegate_s3_compatible_multipart_driver!(TencentCosDriver, storage);

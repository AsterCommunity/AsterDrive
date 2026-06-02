//! 腾讯云 COS 存储驱动。
//!
//! 基础对象读写复用 S3 兼容驱动；COS/CI 文档预览使用 COS 原生 query
//! 签名，因为 CI 处理参数必须参与签名，不能追加在普通 S3 presigned URL 后面。

mod native_media_metadata;
mod native_preview;
mod native_thumbnail;
mod signing;
#[cfg(test)]
mod tests;

use std::time::Duration;

use url::Url;

use super::s3::S3DriverOptions;
use super::s3_compatible::{S3CompatibleDriver, S3CompatibleProvider};
use super::s3_config::normalize_s3_endpoint_and_bucket;
use crate::entities::storage_policy;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::storage::object_key;
use crate::storage::traits::extensions::{
    NativeMediaMetadataStorageDriver, NativePreviewStorageDriver, NativeThumbnailStorageDriver,
};

pub(super) const COS_NATIVE_PREVIEW_PROVIDER: &str = "tencent_cos_ci";
pub(super) const COS_NATIVE_PREVIEW_VERSION: &str = "cos-ci-doc-preview-html-v1";
pub(super) const MAX_COS_PREVIEW_TTL: Duration = Duration::from_secs(60 * 60);
pub(super) const MAX_COS_THUMBNAIL_TTL: Duration = Duration::from_secs(5 * 60);

pub struct TencentCosDriver {
    storage: S3CompatibleDriver,
    endpoint: String,
    bucket: String,
    access_key: String,
    secret_key: String,
    base_path: String,
}

impl TencentCosDriver {
    pub fn validate_policy(policy: &storage_policy::Model) -> Result<()> {
        S3CompatibleDriver::validate_policy(policy)?;
        let normalized = normalize_s3_endpoint_and_bucket(&policy.endpoint, &policy.bucket)
            .map_err(Self::rewrap_message_as_storage_error)?;
        if normalized.endpoint.trim().is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "COS endpoint is required",
            ));
        }
        let endpoint = Url::parse(&normalized.endpoint)
            .map_aster_err_ctx("parse COS endpoint", AsterError::storage_driver_error)?;
        let host = endpoint.host_str().unwrap_or_default();
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
            .map_err(Self::rewrap_message_as_storage_error)?;
        let mut storage_policy = policy.clone();
        storage_policy.endpoint =
            signing::cos_virtual_hosted_s3_endpoint(&normalized.endpoint, &normalized.bucket)?;
        storage_policy.bucket = normalized.bucket.clone();
        let storage = S3CompatibleDriver::new_with_s3_options(
            &storage_policy,
            S3DriverOptions::virtual_hosted_style(),
        )?;

        Ok(Self {
            storage,
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

    fn rewrap_message_as_storage_error(error: AsterError) -> AsterError {
        storage_driver_error(StorageErrorKind::Misconfigured, error.message())
    }

    fn full_key(&self, path: &str) -> String {
        object_key::join_key_prefix(&self.base_path, path)
    }
}

impl S3CompatibleProvider for TencentCosDriver {
    fn s3_compatible_driver(&self) -> &S3CompatibleDriver {
        &self.storage
    }

    fn as_provider_native_preview(&self) -> Option<&dyn NativePreviewStorageDriver> {
        Some(self)
    }

    fn as_provider_native_thumbnail(&self) -> Option<&dyn NativeThumbnailStorageDriver> {
        Some(self)
    }

    fn as_provider_native_media_metadata(&self) -> Option<&dyn NativeMediaMetadataStorageDriver> {
        Some(self)
    }
}

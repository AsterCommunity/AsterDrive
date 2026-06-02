use std::time::Duration;

use async_trait::async_trait;
use aws_sdk_s3::presigning::PresigningConfig;

use crate::errors::{MapAsterErr, Result};
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::storage::traits::driver::PresignedDownloadOptions;
use crate::storage::traits::extensions::PresignedStorageDriver;

use super::S3Driver;

/// Presigned URL 的最长 TTL 上限。
///
/// AWS S3 SigV4 presigned URL 协议层最长支持 7 天，但任何超过 1 小时的链接一旦泄露
/// 就是相对长寿的凭证；服务端调用方理论上不应该传超过这个值，这里在 driver 层做
/// 兜底钳制（防御性编程，非业务逻辑），避免未来某处误传 30 天导致泄露窗口被放大。
pub(super) const MAX_PRESIGN_TTL: Duration = Duration::from_secs(60 * 60); // 1 hour

/// 钳制 presigned TTL：不可超过 `MAX_PRESIGN_TTL`，也不可为 0。
/// 0/超限都按上限处理，并记 warn 日志。
pub(super) fn clamp_presign_ttl(requested: Duration, ctx: &'static str) -> Duration {
    if requested > MAX_PRESIGN_TTL {
        tracing::warn!(
            requested_secs = requested.as_secs(),
            max_secs = MAX_PRESIGN_TTL.as_secs(),
            "{ctx}: presign TTL exceeds MAX_PRESIGN_TTL, clamping"
        );
        MAX_PRESIGN_TTL
    } else if requested.is_zero() {
        tracing::warn!("{ctx}: zero presign TTL requested, falling back to MAX_PRESIGN_TTL");
        MAX_PRESIGN_TTL
    } else {
        requested
    }
}
// =============================================================================
// PresignedStorageDriver 扩展
// =============================================================================

#[async_trait]
impl PresignedStorageDriver for S3Driver {
    async fn presigned_url(
        &self,
        path: &str,
        expires: Duration,
        options: PresignedDownloadOptions,
    ) -> Result<Option<String>> {
        let key = self.full_key(path);
        let presign_config = PresigningConfig::builder()
            .expires_in(clamp_presign_ttl(expires, "S3 presigned_url"))
            .build()
            .map_aster_err_ctx("presign config", |message| {
                storage_driver_error(StorageErrorKind::Misconfigured, message)
            })?;

        let mut request = self.client.get_object().bucket(&self.bucket).key(&key);
        if let Some(cache_control) = options.response_cache_control {
            request = request.response_cache_control(cache_control);
        }
        if let Some(content_disposition) = options.response_content_disposition {
            request = request.response_content_disposition(content_disposition);
        }
        if let Some(content_type) = options.response_content_type {
            request = request.response_content_type(content_type);
        }

        let url = request
            .presigned(presign_config)
            .await
            .map_aster_err_ctx("S3 presigned URL failed", |message| {
                storage_driver_error(StorageErrorKind::Misconfigured, message)
            })?;

        Ok(Some(url.uri().to_string()))
    }

    async fn presigned_put_url(&self, path: &str, expires: Duration) -> Result<Option<String>> {
        let key = self.full_key(path);
        let presign_config = PresigningConfig::builder()
            .expires_in(clamp_presign_ttl(expires, "S3 presigned_put_url"))
            .build()
            .map_aster_err_ctx("presign config", |message| {
                storage_driver_error(StorageErrorKind::Misconfigured, message)
            })?;

        let url = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .presigned(presign_config)
            .await
            .map_aster_err_ctx("S3 presigned PUT failed", |message| {
                storage_driver_error(StorageErrorKind::Misconfigured, message)
            })?;

        Ok(Some(url.uri().to_string()))
    }
}

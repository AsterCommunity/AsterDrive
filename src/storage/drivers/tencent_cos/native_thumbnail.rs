use async_trait::async_trait;
use chrono::Utc;
use url::Url;

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::storage::traits::extensions::{NativeThumbnailRequest, NativeThumbnailStorageDriver};

use super::{COS_NATIVE_PROCESSING_PROVIDER, MAX_COS_THUMBNAIL_TTL, TencentCosDriver};

impl TencentCosDriver {
    pub(super) fn signed_ci_thumbnail_url(
        &self,
        path: &str,
        max_width: u32,
        max_height: u32,
    ) -> Result<String> {
        let width = max_width.max(1);
        let height = max_height.max(1);
        let now = Utc::now();
        let start = now.timestamp();
        let end = (now
            + chrono::Duration::from_std(MAX_COS_THUMBNAIL_TTL)
                .map_aster_err_ctx("COS thumbnail expiry", AsterError::storage_driver_error)?)
        .timestamp();
        let key_time = format!("{start};{end}");
        let process = format!("imageMogr2/thumbnail/{width}x{height}>/format/webp");
        let params = [(process.as_str(), "")];
        let (url, _) = self.signed_cos_query_url(path, &params, &key_time)?;
        Ok(String::from(url))
    }
}

pub(super) fn is_cos_image_thumbnail_candidate(mime_type: &str) -> bool {
    matches!(
        mime_type.trim().to_ascii_lowercase().as_str(),
        "image/jpeg"
            | "image/jpg"
            | "image/png"
            | "image/webp"
            | "image/gif"
            | "image/bmp"
            | "image/tiff"
    )
}

#[async_trait]
impl NativeThumbnailStorageDriver for TencentCosDriver {
    async fn get_native_thumbnail(
        &self,
        request: &NativeThumbnailRequest,
    ) -> Result<Option<Vec<u8>>> {
        if !is_cos_image_thumbnail_candidate(&request.source_mime_type) {
            return Ok(None);
        }

        let url = self.signed_ci_thumbnail_url(
            &request.storage_path,
            request.max_width,
            request.max_height,
        )?;
        if let Ok(parsed_url) = Url::parse(&url) {
            tracing::debug!(
                processor = "storage_native",
                provider = COS_NATIVE_PROCESSING_PROVIDER,
                host = parsed_url.host_str(),
                path = parsed_url.path(),
                "requesting COS native thumbnail"
            );
        }
        let response = reqwest::Client::new()
            .get(url)
            .send()
            .await
            .map_aster_err_ctx(
                "COS native thumbnail request",
                AsterError::storage_driver_error,
            )?;
        let status = response.status();
        if !status.is_success() {
            tracing::debug!(
                processor = "storage_native",
                provider = COS_NATIVE_PROCESSING_PROVIDER,
                http_status = %status,
                "COS native thumbnail request returned non-success status"
            );
            return Err(storage_driver_error(
                if status == reqwest::StatusCode::NOT_FOUND {
                    StorageErrorKind::NotFound
                } else if status == reqwest::StatusCode::FORBIDDEN
                    || status == reqwest::StatusCode::UNAUTHORIZED
                {
                    StorageErrorKind::Auth
                } else if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    StorageErrorKind::RateLimited
                } else if status.is_server_error() {
                    StorageErrorKind::Transient
                } else {
                    StorageErrorKind::Unsupported
                },
                format!("COS native thumbnail request failed with HTTP {status}"),
            ));
        }
        let bytes = response.bytes().await.map_aster_err_ctx(
            "COS native thumbnail body",
            AsterError::storage_driver_error,
        )?;
        Ok(Some(bytes.to_vec()))
    }
}

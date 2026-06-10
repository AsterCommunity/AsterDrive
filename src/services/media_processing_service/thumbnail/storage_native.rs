use crate::api::api_error_code::ApiErrorCode;
use crate::errors::{Result, precondition_failed_with_code};
use crate::storage::{NativeThumbnailRequest, StorageDriver};

use crate::entities::file_blob;

pub(super) async fn render_thumbnail_with_storage_native(
    blob: &file_blob::Model,
    driver: &dyn StorageDriver,
    source_mime_type: &str,
    max_dim: u32,
) -> Result<Vec<u8>> {
    let native = driver.as_native_thumbnail().ok_or_else(|| {
        precondition_failed_with_code(
            ApiErrorCode::ThumbnailProcessorUnavailable,
            "storage driver does not support native thumbnail processing",
        )
    })?;
    let bytes = native
        .get_native_thumbnail(&NativeThumbnailRequest {
            storage_path: blob.storage_path.clone(),
            source_mime_type: source_mime_type.to_string(),
            max_width: max_dim,
            max_height: max_dim,
        })
        .await?
        .ok_or_else(|| {
            precondition_failed_with_code(
                ApiErrorCode::ThumbnailProcessorUnavailable,
                "storage driver could not produce a native thumbnail",
            )
        })?;
    tracing::debug!(
        blob_id = blob.id,
        processor = "storage_native",
        bytes = bytes.len(),
        "storage-native thumbnail render completed"
    );
    Ok(bytes)
}

pub(super) async fn render_image_preview_with_storage_native(
    blob: &file_blob::Model,
    driver: &dyn StorageDriver,
    source_mime_type: &str,
    max_dim: u32,
) -> Result<Vec<u8>> {
    let native = driver.as_native_thumbnail().ok_or_else(|| {
        precondition_failed_with_code(
            ApiErrorCode::ThumbnailProcessorUnavailable,
            "storage driver does not support native thumbnail processing",
        )
    })?;
    let bytes = native
        .get_native_thumbnail(&NativeThumbnailRequest {
            storage_path: blob.storage_path.clone(),
            source_mime_type: source_mime_type.to_string(),
            max_width: max_dim,
            max_height: max_dim,
        })
        .await?
        .ok_or_else(|| {
            precondition_failed_with_code(
                ApiErrorCode::ThumbnailProcessorUnavailable,
                "storage driver could not produce a native image preview",
            )
        })?;
    tracing::debug!(
        blob_id = blob.id,
        processor = "storage_native",
        bytes = bytes.len(),
        "storage-native image preview render completed"
    );
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::{render_image_preview_with_storage_native, render_thumbnail_with_storage_native};
    use crate::entities::file_blob;
    use crate::errors::Result;
    use crate::storage::{
        BlobMetadata, NativeThumbnailRequest, NativeThumbnailStorageDriver, StorageDriver,
    };
    use async_trait::async_trait;
    use chrono::Utc;
    use std::sync::Mutex;
    use tokio::io::AsyncRead;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct CapturedNativeRequest {
        storage_path: String,
        source_mime_type: String,
        max_width: u32,
        max_height: u32,
    }

    struct CapturingNativeDriver {
        requests: Mutex<Vec<CapturedNativeRequest>>,
    }

    impl CapturingNativeDriver {
        fn new() -> Self {
            Self {
                requests: Mutex::new(Vec::new()),
            }
        }

        fn requests(&self) -> Vec<CapturedNativeRequest> {
            self.requests
                .lock()
                .expect("request capture lock should not be poisoned")
                .clone()
        }
    }

    #[async_trait]
    impl StorageDriver for CapturingNativeDriver {
        async fn put(&self, _path: &str, _data: &[u8]) -> Result<String> {
            unreachable!()
        }

        async fn get(&self, _path: &str) -> Result<Vec<u8>> {
            unreachable!()
        }

        async fn get_stream(&self, _path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
            unreachable!()
        }

        async fn delete(&self, _path: &str) -> Result<()> {
            unreachable!()
        }

        async fn exists(&self, _path: &str) -> Result<bool> {
            unreachable!()
        }

        async fn metadata(&self, _path: &str) -> Result<BlobMetadata> {
            unreachable!()
        }

        fn as_native_thumbnail(&self) -> Option<&dyn NativeThumbnailStorageDriver> {
            Some(self)
        }
    }

    #[async_trait]
    impl NativeThumbnailStorageDriver for CapturingNativeDriver {
        async fn get_native_thumbnail(
            &self,
            request: &NativeThumbnailRequest,
        ) -> Result<Option<Vec<u8>>> {
            self.requests
                .lock()
                .expect("request capture lock should not be poisoned")
                .push(CapturedNativeRequest {
                    storage_path: request.storage_path.clone(),
                    source_mime_type: request.source_mime_type.clone(),
                    max_width: request.max_width,
                    max_height: request.max_height,
                });
            Ok(Some(vec![1, 2, 3]))
        }
    }

    fn blob() -> file_blob::Model {
        file_blob::Model {
            id: 1,
            hash: "abc".repeat(21) + "a",
            size: 10,
            policy_id: 1,
            storage_path: "objects/source.png".to_string(),
            thumbnail_path: None,
            thumbnail_processor: None,
            thumbnail_version: None,
            ref_count: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn storage_native_thumbnail_request_uses_configured_max_dimension() {
        let driver = CapturingNativeDriver::new();

        let bytes = render_thumbnail_with_storage_native(&blob(), &driver, "image/png", 320)
            .await
            .unwrap();

        assert_eq!(bytes, vec![1, 2, 3]);
        assert_eq!(
            driver.requests(),
            vec![CapturedNativeRequest {
                storage_path: "objects/source.png".to_string(),
                source_mime_type: "image/png".to_string(),
                max_width: 320,
                max_height: 320,
            }]
        );
    }

    #[tokio::test]
    async fn storage_native_image_preview_request_uses_configured_max_dimension() {
        let driver = CapturingNativeDriver::new();

        let bytes = render_image_preview_with_storage_native(&blob(), &driver, "image/heic", 2048)
            .await
            .unwrap();

        assert_eq!(bytes, vec![1, 2, 3]);
        assert_eq!(
            driver.requests(),
            vec![CapturedNativeRequest {
                storage_path: "objects/source.png".to_string(),
                source_mime_type: "image/heic".to_string(),
                max_width: 2048,
                max_height: 2048,
            }]
        );
    }
}

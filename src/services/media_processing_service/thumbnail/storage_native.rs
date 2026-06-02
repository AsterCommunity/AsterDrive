use crate::api::subcode::ApiSubcode;
use crate::errors::{Result, precondition_failed_with_subcode};
use crate::storage::{NativeThumbnailRequest, StorageDriver};

use crate::entities::file_blob;

pub(super) async fn render_thumbnail_with_storage_native(
    blob: &file_blob::Model,
    driver: &dyn StorageDriver,
    source_mime_type: &str,
) -> Result<Vec<u8>> {
    let native = driver.as_native_thumbnail().ok_or_else(|| {
        precondition_failed_with_subcode(
            ApiSubcode::ThumbnailProcessorUnavailable,
            "storage driver does not support native thumbnail processing",
        )
    })?;
    let bytes = native
        .get_native_thumbnail(&NativeThumbnailRequest {
            storage_path: blob.storage_path.clone(),
            source_mime_type: source_mime_type.to_string(),
            max_width: crate::services::thumbnail_service::current_thumbnail_max_dim(),
            max_height: crate::services::thumbnail_service::current_thumbnail_max_dim(),
        })
        .await?
        .ok_or_else(|| {
            precondition_failed_with_subcode(
                ApiSubcode::ThumbnailProcessorUnavailable,
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
) -> Result<Vec<u8>> {
    let native = driver.as_native_thumbnail().ok_or_else(|| {
        precondition_failed_with_subcode(
            ApiSubcode::ThumbnailProcessorUnavailable,
            "storage driver does not support native thumbnail processing",
        )
    })?;
    let bytes = native
        .get_native_thumbnail(&NativeThumbnailRequest {
            storage_path: blob.storage_path.clone(),
            source_mime_type: source_mime_type.to_string(),
            max_width: crate::services::thumbnail_service::current_image_preview_max_dim(),
            max_height: crate::services::thumbnail_service::current_image_preview_max_dim(),
        })
        .await?
        .ok_or_else(|| {
            precondition_failed_with_subcode(
                ApiSubcode::ThumbnailProcessorUnavailable,
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

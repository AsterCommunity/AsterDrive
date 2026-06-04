use crate::entities::file_blob;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;

use crate::services::media_processing_service::resolve::build_thumbnail_context;
use crate::services::media_processing_service::shared::ImagePreviewData;

use super::cache::load_thumbnail_from_path;
use super::render::render_image_preview_bytes;

pub async fn generate_and_store_image_preview(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    file_name: &str,
    source_mime_type: &str,
) -> Result<ImagePreviewData> {
    let ctx = build_thumbnail_context(state, blob, file_name, source_mime_type)?;
    let preview_path = ctx.processor.image_preview_cache_path(&blob.hash);
    let preview_processor = ctx.processor.image_preview_processor().to_string();
    let preview_version = ctx.processor.image_preview_version().to_string();

    if let Some(data) =
        load_thumbnail_from_path(state, blob, &ctx.driver, &preview_path, false).await?
    {
        tracing::debug!(
            blob_id = blob.id,
            processor = ctx.processor.kind().as_str(),
            image_preview_path = preview_path,
            image_preview_processor = preview_processor,
            image_preview_version = preview_version,
            cache_source = "computed_path",
            "image preview cache hit"
        );
        return Ok(ImagePreviewData {
            data: data.to_vec(),
            image_preview_processor: preview_processor,
            image_preview_version: preview_version,
        });
    }

    tracing::debug!(
        blob_id = blob.id,
        processor = ctx.processor.kind().as_str(),
        image_preview_path = preview_path,
        image_preview_processor = preview_processor,
        image_preview_version = preview_version,
        "rendering image preview because cache miss"
    );

    let webp_bytes = render_image_preview_bytes(
        state,
        blob,
        file_name,
        source_mime_type,
        &ctx.driver,
        &ctx.processor,
    )
    .await?;

    if let Err(error) = ctx.driver.put(&preview_path, &webp_bytes).await {
        tracing::warn!(
            blob_id = blob.id,
            path = preview_path,
            "failed to store image preview: {error}"
        );
    }

    Ok(ImagePreviewData {
        data: webp_bytes,
        image_preview_processor: preview_processor,
        image_preview_version: preview_version,
    })
}

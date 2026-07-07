use crate::entities::file_blob;
use crate::errors::Result;
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use bytes::Bytes;

use crate::services::media::processing::resolve::build_thumbnail_context;
use crate::services::media::processing::resolve::build_thumbnail_context_with_processor;
use crate::services::media::processing::shared::{
    ImagePreviewData, StoredImagePreview, ThumbnailContext,
};
use crate::types::MediaProcessorKind;

use super::cache::load_thumbnail_from_path;
use super::render::render_image_preview_bytes;

pub async fn load_image_preview_if_exists(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    file_name: &str,
    source_mime_type: &str,
) -> Result<Option<ImagePreviewData>> {
    let ctx = build_thumbnail_context(state, blob, file_name, source_mime_type)?;
    let preview_path = ctx
        .processor
        .image_preview_cache_path(&blob.hash, state.runtime_config());
    let preview_processor = ctx.processor.image_preview_processor().to_string();
    let preview_version = ctx.processor.image_preview_version(state.runtime_config());

    let data = load_thumbnail_from_path(state, blob, &ctx.driver, &preview_path, false).await?;
    Ok(data.map(|data| {
        tracing::debug!(
            blob_id = blob.id,
            processor = ctx.processor.kind().as_str(),
            image_preview_path = preview_path,
            image_preview_processor = preview_processor,
            image_preview_version = preview_version,
            cache_source = "computed_path",
            "image preview cache hit"
        );
        ImagePreviewData {
            data,
            image_preview_processor: preview_processor,
            image_preview_version: preview_version,
        }
    }))
}

pub async fn generate_and_store_image_preview(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    file_name: &str,
    source_mime_type: &str,
) -> Result<ImagePreviewData> {
    let ctx = build_thumbnail_context(state, blob, file_name, source_mime_type)?;
    generate_and_store_image_preview_with_context(state, blob, file_name, source_mime_type, &ctx)
        .await
        .map(|stored| ImagePreviewData {
            data: stored.data,
            image_preview_processor: stored.image_preview_processor,
            image_preview_version: stored.image_preview_version,
        })
}

pub(crate) async fn generate_and_store_image_preview_with_processor(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    source_file_name: &str,
    source_mime_type: &str,
    processor_kind: MediaProcessorKind,
) -> Result<StoredImagePreview> {
    let policy = state.policy_snapshot().get_policy_or_err(blob.policy_id)?;
    let ctx =
        build_thumbnail_context_with_processor(state, &policy, source_file_name, processor_kind)?;
    let stored = generate_and_store_image_preview_with_context(
        state,
        blob,
        source_file_name,
        source_mime_type,
        &ctx,
    )
    .await?;
    Ok(StoredImagePreview {
        image_preview_path: stored.image_preview_path,
        image_preview_processor: stored.image_preview_processor,
        image_preview_version: stored.image_preview_version,
        reused_existing_preview: stored.reused_existing_preview,
    })
}

struct ImagePreviewWithData {
    data: Bytes,
    image_preview_path: String,
    image_preview_processor: String,
    image_preview_version: String,
    reused_existing_preview: bool,
}

async fn generate_and_store_image_preview_with_context(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    file_name: &str,
    source_mime_type: &str,
    ctx: &ThumbnailContext,
) -> Result<ImagePreviewWithData> {
    let preview_path = ctx
        .processor
        .image_preview_cache_path(&blob.hash, state.runtime_config());
    let preview_processor = ctx.processor.image_preview_processor().to_string();
    let preview_version = ctx.processor.image_preview_version(state.runtime_config());

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
        return Ok(ImagePreviewWithData {
            data,
            image_preview_path: preview_path,
            image_preview_processor: preview_processor,
            image_preview_version: preview_version,
            reused_existing_preview: true,
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

    Ok(ImagePreviewWithData {
        data: Bytes::from(webp_bytes),
        image_preview_path: preview_path,
        image_preview_processor: preview_processor,
        image_preview_version: preview_version,
        reused_existing_preview: false,
    })
}

mod cache;
mod cli;
mod errors;
mod preview;
mod probe;
mod render;
mod storage_native;

use crate::db::repository::file_repo;
use crate::entities::file_blob;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::types::MediaProcessorKind;

use super::resolve::{build_thumbnail_context, build_thumbnail_context_with_processor};
use super::shared::{StoredThumbnail, ThumbnailContext, ThumbnailData};

pub use cache::delete_thumbnail;
pub use preview::generate_and_store_image_preview;
pub use probe::probe_ffmpeg_cli_command;

pub async fn load_thumbnail_if_exists(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    file_name: &str,
    source_mime_type: &str,
) -> Result<Option<ThumbnailData>> {
    let ctx = build_thumbnail_context(state, blob, file_name, source_mime_type)?;
    cache::load_thumbnail_if_exists_with_context(state, blob, &ctx).await
}

pub async fn get_or_generate_thumbnail(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    file_name: &str,
    source_mime_type: &str,
) -> Result<ThumbnailData> {
    let ctx = build_thumbnail_context(state, blob, file_name, source_mime_type)?;
    if let Some(data) = cache::load_thumbnail_if_exists_with_context(state, blob, &ctx).await? {
        return Ok(data);
    }

    let thumbnail_processor = ctx.processor.thumbnail_processor().to_string();
    let thumbnail_version = ctx.processor.thumbnail_version().to_string();
    let thumbnail_path = ctx.processor.cache_path(&blob.hash);
    let webp_bytes = render::render_thumbnail_bytes(
        state,
        blob,
        file_name,
        source_mime_type,
        &ctx.driver,
        &ctx.processor,
    )
    .await?;

    if let Err(error) = ctx.driver.put(&thumbnail_path, &webp_bytes).await {
        tracing::warn!("failed to store thumbnail {thumbnail_path}: {error}");
    } else if let Err(error) = file_repo::set_thumbnail_metadata(
        state.writer_db(),
        blob.id,
        &thumbnail_path,
        &thumbnail_processor,
        &thumbnail_version,
    )
    .await
    {
        tracing::warn!(
            blob_id = blob.id,
            path = thumbnail_path,
            "failed to persist thumbnail metadata after synchronous generation: {error}"
        );
    }

    Ok(ThumbnailData {
        data: webp_bytes,
        thumbnail_processor,
        thumbnail_version,
    })
}

pub async fn generate_and_store_thumbnail(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    file_name: &str,
    source_mime_type: &str,
) -> Result<StoredThumbnail> {
    let ctx = build_thumbnail_context(state, blob, file_name, source_mime_type)?;
    generate_and_store_with_context(state, blob, file_name, source_mime_type, &ctx).await
}

pub(crate) async fn generate_and_store_thumbnail_with_processor(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    source_file_name: &str,
    source_mime_type: &str,
    processor_kind: MediaProcessorKind,
) -> Result<StoredThumbnail> {
    let policy = state.policy_snapshot.get_policy_or_err(blob.policy_id)?;
    let ctx =
        build_thumbnail_context_with_processor(state, &policy, source_file_name, processor_kind)?;
    generate_and_store_with_context(state, blob, source_file_name, source_mime_type, &ctx).await
}

async fn generate_and_store_with_context(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    source_file_name: &str,
    source_mime_type: &str,
    ctx: &ThumbnailContext,
) -> Result<StoredThumbnail> {
    if let Some(existing) = cache::load_thumbnail_if_exists_with_context(state, blob, ctx).await? {
        tracing::debug!(
            blob_id = blob.id,
            processor = ctx.processor.kind().as_str(),
            thumbnail_version = existing.thumbnail_version,
            "reusing existing thumbnail without rendering"
        );
        return Ok(StoredThumbnail {
            thumbnail_path: ctx.processor.cache_path(&blob.hash),
            thumbnail_processor: existing.thumbnail_processor,
            thumbnail_version: existing.thumbnail_version,
            reused_existing_thumbnail: true,
        });
    }

    let thumbnail_processor = ctx.processor.thumbnail_processor().to_string();
    let thumbnail_version = ctx.processor.thumbnail_version().to_string();
    let thumbnail_path = ctx.processor.cache_path(&blob.hash);
    tracing::debug!(
        blob_id = blob.id,
        processor = ctx.processor.kind().as_str(),
        thumbnail_path,
        thumbnail_processor,
        thumbnail_version,
        "rendering thumbnail because cache miss"
    );
    let webp_bytes = render::render_thumbnail_bytes(
        state,
        blob,
        source_file_name,
        source_mime_type,
        &ctx.driver,
        &ctx.processor,
    )
    .await?;
    let stored_path = ctx.driver.put(&thumbnail_path, &webp_bytes).await?;
    file_repo::set_thumbnail_metadata(
        state.writer_db(),
        blob.id,
        &stored_path,
        &thumbnail_processor,
        &thumbnail_version,
    )
    .await?;

    tracing::debug!(
        blob_id = blob.id,
        processor = ctx.processor.kind().as_str(),
        stored_path,
        thumbnail_processor,
        thumbnail_version,
        bytes = webp_bytes.len(),
        "thumbnail rendered and stored"
    );

    Ok(StoredThumbnail {
        thumbnail_path: stored_path,
        thumbnail_processor,
        thumbnail_version,
        reused_existing_thumbnail: false,
    })
}

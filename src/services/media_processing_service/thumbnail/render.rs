use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;

use crate::config::operations;
use crate::entities::file_blob;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::storage::StorageDriver;
use crate::types::MediaProcessorKind;

use crate::services::media_processing_service::cli_input::prepare_cli_source;
use crate::services::media_processing_service::shared::ResolvedMediaProcessor;

use super::cli::{
    render_image_preview_with_vips_cli, render_thumbnail_with_ffmpeg_cli,
    render_thumbnail_with_vips_cli,
};
use super::storage_native::{
    render_image_preview_with_storage_native, render_thumbnail_with_storage_native,
};

pub(super) async fn render_thumbnail_bytes(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    source_file_name: &str,
    source_mime_type: &str,
    driver: &Arc<dyn StorageDriver>,
    processor: &ResolvedMediaProcessor,
) -> Result<Vec<u8>> {
    match processor.kind() {
        MediaProcessorKind::Images => {
            tracing::debug!(
                blob_id = blob.id,
                processor = "images",
                "rendering thumbnail via built-in images pipeline"
            );
            crate::services::thumbnail_service::ensure_source_size_supported(
                blob,
                operations::thumbnail_max_source_bytes(state.runtime_config()),
            )?;
            crate::services::thumbnail_service::render_thumbnail_bytes(
                driver.as_ref(),
                blob,
                &state.config().server.temp_dir,
                operations::thumbnail_max_dimension(state.runtime_config()),
            )
            .await
        }
        MediaProcessorKind::VipsCli => {
            let command = processor.vips_command().to_string();
            tracing::debug!(
                blob_id = blob.id,
                processor = "vips_cli",
                command,
                "rendering thumbnail via vips CLI pipeline"
            );
            crate::services::thumbnail_service::ensure_source_size_supported(
                blob,
                operations::thumbnail_max_source_bytes(state.runtime_config()),
            )?;
            render_thumbnail_with_vips_cli(
                state,
                blob,
                source_file_name,
                source_mime_type,
                driver.as_ref(),
                &command,
                operations::thumbnail_max_dimension(state.runtime_config()),
            )
            .await
        }
        MediaProcessorKind::FfmpegCli => {
            let command = processor.ffmpeg_command().to_string();
            tracing::debug!(
                blob_id = blob.id,
                processor = "ffmpeg_cli",
                command,
                "rendering thumbnail via ffmpeg CLI pipeline"
            );
            crate::services::thumbnail_service::ensure_source_size_supported(
                blob,
                operations::thumbnail_max_source_bytes(state.runtime_config()),
            )?;
            render_thumbnail_with_ffmpeg_cli(
                state,
                blob,
                source_file_name,
                source_mime_type,
                driver.as_ref(),
                &command,
                operations::thumbnail_max_dimension(state.runtime_config()),
            )
            .await
        }
        MediaProcessorKind::Lofty => {
            tracing::debug!(
                blob_id = blob.id,
                processor = "lofty",
                "rendering thumbnail from embedded audio artwork"
            );
            crate::services::thumbnail_service::ensure_source_size_supported(
                blob,
                operations::thumbnail_max_source_bytes(state.runtime_config()),
            )?;
            render_thumbnail_with_lofty(
                state,
                blob,
                source_file_name,
                source_mime_type,
                driver.as_ref(),
                operations::thumbnail_max_dimension(state.runtime_config()),
            )
            .await
        }
        MediaProcessorKind::StorageNative => {
            tracing::debug!(
                blob_id = blob.id,
                processor = "storage_native",
                "rendering thumbnail via storage-native pipeline"
            );
            render_thumbnail_with_storage_native(
                blob,
                driver.as_ref(),
                source_mime_type,
                operations::thumbnail_max_dimension(state.runtime_config()),
            )
            .await
        }
        MediaProcessorKind::FfprobeCli => Err(crate::errors::AsterError::internal_error(
            "ffprobe_cli cannot render thumbnails",
        )),
    }
}

async fn render_thumbnail_with_lofty(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    source_file_name: &str,
    source_mime_type: &str,
    driver: &dyn StorageDriver,
    max_dim: u32,
) -> Result<Vec<u8>> {
    let temp_root = crate::utils::paths::runtime_temp_dir(&state.config().server.temp_dir);
    let temp_dir =
        std::path::PathBuf::from(temp_root).join(format!("media-lofty-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .map_aster_err_ctx("create lofty temp dir", AsterError::storage_driver_error)?;
    let temp_dir = crate::services::media_processing_service::shared::TempDirGuard::new(
        temp_dir,
        "lofty thumbnail temp dir",
    );
    let prepared_input = prepare_cli_source(
        driver,
        &blob.storage_path,
        source_file_name,
        source_mime_type,
        temp_dir.path(),
        false,
    )
    .await?;
    let source_path = std::path::PathBuf::from(prepared_input.input_arg());
    tokio::task::spawn_blocking(move || {
        render_audio_artwork_thumbnail_from_path(&source_path, max_dim)
    })
    .await
    .map_aster_err_ctx("lofty thumbnail task panicked", AsterError::internal_error)?
}

fn render_audio_artwork_thumbnail_from_path(path: &Path, max_dim: u32) -> Result<Vec<u8>> {
    let mut options = lofty::config::ParseOptions::new();
    options = options.read_cover_art(true);
    let tagged_file = lofty::probe::Probe::open(path)
        .map_aster_err_ctx(
            "open audio thumbnail source",
            AsterError::storage_driver_error,
        )?
        .guess_file_type()
        .map_aster_err_ctx("guess audio thumbnail format", AsterError::validation_error)?
        .options(options)
        .read()
        .map_aster_err_ctx(
            "read audio thumbnail metadata",
            AsterError::validation_error,
        )?;
    let tag = lofty::file::TaggedFileExt::primary_tag(&tagged_file)
        .or_else(|| lofty::file::TaggedFileExt::first_tag(&tagged_file));
    let picture = tag
        .and_then(|tag| tag.pictures().first())
        .ok_or_else(|| AsterError::validation_error("audio file has no embedded artwork"))?;
    crate::services::thumbnail_service::render_webp_derivative_from_image_bytes(
        Cursor::new(picture.data()),
        max_dim,
    )
}

pub(super) async fn render_image_preview_bytes(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    source_file_name: &str,
    source_mime_type: &str,
    driver: &Arc<dyn StorageDriver>,
    processor: &ResolvedMediaProcessor,
) -> Result<Vec<u8>> {
    match processor.kind() {
        MediaProcessorKind::Images => {
            tracing::debug!(
                blob_id = blob.id,
                processor = "images",
                "rendering image preview via built-in images pipeline"
            );
            crate::services::thumbnail_service::ensure_source_size_supported(
                blob,
                operations::thumbnail_max_source_bytes(state.runtime_config()),
            )?;
            crate::services::thumbnail_service::render_webp_derivative_bytes(
                driver.as_ref(),
                blob,
                &state.config().server.temp_dir,
                operations::image_preview_max_dimension(state.runtime_config()),
            )
            .await
        }
        MediaProcessorKind::VipsCli => {
            let command = processor.vips_command().to_string();
            tracing::debug!(
                blob_id = blob.id,
                processor = "vips_cli",
                command,
                "rendering image preview via vips CLI pipeline"
            );
            crate::services::thumbnail_service::ensure_source_size_supported(
                blob,
                operations::thumbnail_max_source_bytes(state.runtime_config()),
            )?;
            render_image_preview_with_vips_cli(
                state,
                blob,
                source_file_name,
                source_mime_type,
                driver.as_ref(),
                &command,
                operations::image_preview_max_dimension(state.runtime_config()),
            )
            .await
        }
        MediaProcessorKind::StorageNative => {
            tracing::debug!(
                blob_id = blob.id,
                processor = "storage_native",
                "rendering image preview via storage-native pipeline"
            );
            render_image_preview_with_storage_native(
                blob,
                driver.as_ref(),
                source_mime_type,
                operations::image_preview_max_dimension(state.runtime_config()),
            )
            .await
        }
        MediaProcessorKind::FfmpegCli => {
            let command = processor.ffmpeg_command().to_string();
            tracing::debug!(
                blob_id = blob.id,
                processor = "ffmpeg_cli",
                command,
                "rendering image preview via ffmpeg CLI pipeline"
            );
            crate::services::thumbnail_service::ensure_source_size_supported(
                blob,
                operations::thumbnail_max_source_bytes(state.runtime_config()),
            )?;
            render_thumbnail_with_ffmpeg_cli(
                state,
                blob,
                source_file_name,
                source_mime_type,
                driver.as_ref(),
                &command,
                operations::image_preview_max_dimension(state.runtime_config()),
            )
            .await
        }
        MediaProcessorKind::Lofty | MediaProcessorKind::FfprobeCli => {
            Err(crate::errors::AsterError::internal_error(format!(
                "{} cannot render image previews",
                processor.kind().as_str()
            )))
        }
    }
}

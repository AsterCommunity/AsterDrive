use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::{ImageFormat, ImageReader, Limits};
use tokio::io::AsyncReadExt;

use crate::entities::file_blob;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::PrimaryAppState;
use crate::storage::StorageDriver;

use crate::services::media_processing_service::cli_input::prepare_cli_source;
use crate::services::media_processing_service::shared::{
    TempDirGuard, run_cli_command_with_timeout,
};

use super::errors::{thumbnail_output_invalid, thumbnail_render_failed};

const FFMPEG_THUMBNAIL_BATCH_SIZE: u32 = 50;
const MAX_CLI_THUMBNAIL_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
const MAX_CLI_THUMBNAIL_OUTPUT_BYTES_U64: u64 = 16 * 1024 * 1024;
const MAX_CLI_THUMBNAIL_DECODE_ALLOC: u64 = 64 * 1024 * 1024;

pub(super) async fn render_thumbnail_with_vips_cli(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    source_file_name: &str,
    source_mime_type: &str,
    driver: &dyn StorageDriver,
    command: &str,
) -> Result<Vec<u8>> {
    let temp_root = crate::utils::paths::runtime_temp_dir(&state.config.server.temp_dir);
    let temp_dir = PathBuf::from(temp_root).join(format!("media-vips-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .map_aster_err_ctx("create vips temp dir", AsterError::storage_driver_error)?;
    let temp_dir = TempDirGuard::new(temp_dir, "vips thumbnail temp dir");

    let output_path = temp_dir.path().join("thumbnail.webp");
    let prepared_input = prepare_cli_source(
        driver,
        &blob.storage_path,
        source_file_name,
        source_mime_type,
        temp_dir.path(),
        false,
    )
    .await?;

    let command = command.to_string();
    let input_arg = prepared_input.input_arg().to_string();
    let output_arg = output_path.to_string_lossy().to_string();
    let max_dim = crate::services::thumbnail_service::current_thumbnail_max_dim();
    tracing::debug!(
        blob_id = blob.id,
        processor = "vips_cli",
        command,
        input_source = prepared_input.kind().as_str(),
        output_path = "<redacted>",
        max_dim,
        "starting vips CLI thumbnail render"
    );
    tokio::task::spawn_blocking(move || {
        let max_dim_arg = max_dim.to_string();
        let output = run_cli_command_with_timeout(
            &command,
            &[
                "thumbnail",
                &input_arg,
                &output_arg,
                &max_dim_arg,
                "--height",
                &max_dim_arg,
                "--size",
                "down",
            ],
            thumbnail_render_failed,
        )?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let detail = if !stderr.is_empty() {
                stderr
            } else if !stdout.is_empty() {
                stdout
            } else {
                format!("exit status {}", output.status)
            };
            return Err(thumbnail_render_failed(format!(
                "vips CLI thumbnail command failed: {detail}"
            )));
        }
        Ok::<(), AsterError>(())
    })
    .await
    .map_aster_err_ctx("vips CLI thumbnail task panicked", thumbnail_render_failed)??;

    let thumbnail = read_cli_thumbnail_output(&output_path, "read vips thumbnail output").await;
    if let Ok(bytes) = &thumbnail {
        tracing::debug!(
            blob_id = blob.id,
            processor = "vips_cli",
            bytes = bytes.len(),
            "vips CLI thumbnail render completed"
        );
    }
    thumbnail
}

pub(super) async fn render_image_preview_with_vips_cli(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    source_file_name: &str,
    source_mime_type: &str,
    driver: &dyn StorageDriver,
    command: &str,
) -> Result<Vec<u8>> {
    let temp_root = crate::utils::paths::runtime_temp_dir(&state.config.server.temp_dir);
    let temp_dir =
        PathBuf::from(temp_root).join(format!("media-vips-preview-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .map_aster_err_ctx("create vips temp dir", AsterError::storage_driver_error)?;
    let temp_dir = TempDirGuard::new(temp_dir, "vips image preview temp dir");

    let output_path = temp_dir.path().join("preview.webp");
    let prepared_input = prepare_cli_source(
        driver,
        &blob.storage_path,
        source_file_name,
        source_mime_type,
        temp_dir.path(),
        false,
    )
    .await?;

    let command = command.to_string();
    let input_arg = prepared_input.input_arg().to_string();
    let output_arg = output_path.to_string_lossy().to_string();
    let max_dim = crate::services::thumbnail_service::current_image_preview_max_dim();
    tracing::debug!(
        blob_id = blob.id,
        processor = "vips_cli",
        command,
        input_source = prepared_input.kind().as_str(),
        output_path = "<redacted>",
        max_dim,
        "starting vips CLI image preview render"
    );
    tokio::task::spawn_blocking(move || {
        let max_dim_arg = max_dim.to_string();
        let output = run_cli_command_with_timeout(
            &command,
            &[
                "thumbnail",
                &input_arg,
                &output_arg,
                &max_dim_arg,
                "--height",
                &max_dim_arg,
                "--size",
                "down",
            ],
            thumbnail_render_failed,
        )?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let detail = if !stderr.is_empty() {
                stderr
            } else if !stdout.is_empty() {
                stdout
            } else {
                format!("exit status {}", output.status)
            };
            return Err(thumbnail_render_failed(format!(
                "vips CLI image preview command failed: {detail}"
            )));
        }
        Ok::<(), AsterError>(())
    })
    .await
    .map_aster_err_ctx(
        "vips CLI image preview task panicked",
        thumbnail_render_failed,
    )??;

    let preview = read_cli_thumbnail_output(&output_path, "read vips image preview output").await;
    if let Ok(bytes) = &preview {
        tracing::debug!(
            blob_id = blob.id,
            processor = "vips_cli",
            bytes = bytes.len(),
            "vips CLI image preview render completed"
        );
    }
    preview
}

pub(super) async fn render_thumbnail_with_ffmpeg_cli(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    source_file_name: &str,
    source_mime_type: &str,
    driver: &dyn StorageDriver,
    command: &str,
) -> Result<Vec<u8>> {
    let temp_root = crate::utils::paths::runtime_temp_dir(&state.config.server.temp_dir);
    let temp_dir = PathBuf::from(temp_root).join(format!("media-ffmpeg-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .map_aster_err_ctx("create ffmpeg temp dir", AsterError::storage_driver_error)?;
    let temp_dir = TempDirGuard::new(temp_dir, "ffmpeg thumbnail temp dir");

    let output_path = temp_dir.path().join("thumbnail.png");
    let prepared_input = prepare_cli_source(
        driver,
        &blob.storage_path,
        source_file_name,
        source_mime_type,
        temp_dir.path(),
        true,
    )
    .await?;

    let command = command.to_string();
    let input_arg = prepared_input.input_arg().to_string();
    let output_arg = output_path.to_string_lossy().to_string();
    let max_dim = crate::services::thumbnail_service::current_thumbnail_max_dim();
    let filter_arg = format!(
        "thumbnail={FFMPEG_THUMBNAIL_BATCH_SIZE}:log=quiet,scale=min(iw\\,{max_dim}):min(ih\\,{max_dim}):force_original_aspect_ratio=decrease"
    );
    tracing::debug!(
        blob_id = blob.id,
        processor = "ffmpeg_cli",
        command,
        input_source = prepared_input.kind().as_str(),
        output_path = "<redacted>",
        max_dim,
        "starting ffmpeg CLI thumbnail render"
    );
    tokio::task::spawn_blocking(move || {
        let output = run_cli_command_with_timeout(
            &command,
            &[
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-i",
                &input_arg,
                "-map",
                "0:v:0",
                "-vf",
                &filter_arg,
                "-frames:v",
                "1",
                "-an",
                "-sn",
                "-c:v",
                "png",
                "-y",
                &output_arg,
            ],
            thumbnail_render_failed,
        )?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let detail = if !stderr.is_empty() {
                stderr
            } else if !stdout.is_empty() {
                stdout
            } else {
                format!("exit status {}", output.status)
            };
            return Err(thumbnail_render_failed(format!(
                "ffmpeg CLI thumbnail command failed: {detail}"
            )));
        }
        Ok::<(), AsterError>(())
    })
    .await
    .map_aster_err_ctx(
        "ffmpeg CLI thumbnail task panicked",
        thumbnail_render_failed,
    )??;

    let thumbnail_png =
        read_cli_thumbnail_output(&output_path, "read ffmpeg thumbnail output").await;
    let thumbnail = match thumbnail_png {
        Ok(bytes) => tokio::task::spawn_blocking(move || encode_webp_from_image_bytes(bytes))
            .await
            .map_aster_err_ctx(
                "ffmpeg thumbnail webp encode task panicked",
                thumbnail_render_failed,
            )?,
        Err(error) => Err(error),
    };
    if let Ok(bytes) = &thumbnail {
        tracing::debug!(
            blob_id = blob.id,
            processor = "ffmpeg_cli",
            bytes = bytes.len(),
            "ffmpeg CLI thumbnail render completed"
        );
    }
    thumbnail
}

async fn read_cli_thumbnail_output(path: &Path, context: &'static str) -> Result<Vec<u8>> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_aster_err_ctx(context, thumbnail_render_failed)?;
    if metadata.len() > MAX_CLI_THUMBNAIL_OUTPUT_BYTES_U64 {
        return Err(thumbnail_output_invalid(format!(
            "{context}: output exceeds {} MiB limit",
            MAX_CLI_THUMBNAIL_OUTPUT_BYTES_U64 / 1024 / 1024
        )));
    }

    let file = tokio::fs::File::open(path)
        .await
        .map_aster_err_ctx(context, thumbnail_render_failed)?;
    let mut limited = file.take(MAX_CLI_THUMBNAIL_OUTPUT_BYTES_U64 + 1);
    let mut bytes = Vec::new();
    limited
        .read_to_end(&mut bytes)
        .await
        .map_aster_err_ctx(context, thumbnail_render_failed)?;

    if bytes.len() > MAX_CLI_THUMBNAIL_OUTPUT_BYTES {
        return Err(thumbnail_output_invalid(format!(
            "{context}: output exceeds {} MiB limit",
            MAX_CLI_THUMBNAIL_OUTPUT_BYTES_U64 / 1024 / 1024
        )));
    }

    Ok(bytes)
}

fn encode_webp_from_image_bytes(bytes: Vec<u8>) -> Result<Vec<u8>> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_aster_err_ctx(
            "guess ffmpeg thumbnail output format",
            thumbnail_output_invalid,
        )?;

    let mut limits = Limits::default();
    limits.max_alloc = Some(MAX_CLI_THUMBNAIL_DECODE_ALLOC);
    reader.limits(limits);

    let image = reader
        .decode()
        .map_aster_err_ctx("decode ffmpeg thumbnail output", thumbnail_output_invalid)?;
    let mut buf = Cursor::new(Vec::new());
    image
        .write_to(&mut buf, ImageFormat::WebP)
        .map_aster_err_ctx("encode ffmpeg thumbnail webp", thumbnail_output_invalid)?;
    Ok(buf.into_inner())
}

use std::fs::File;
use std::io::{BufReader, Cursor};
use std::path::PathBuf;
use std::time::Instant;

use crate::api::api_error_code::ApiErrorCode;
use crate::config::media_processing as media_processing_config;
use crate::errors::{
    AsterError, MapAsterErr, Result, file_upload_error_with_code, precondition_failed_with_code,
    validation_error_with_code,
};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::user::profile::shared::{
    AVATAR_SIZE_LG, AVATAR_SIZE_SM, MAX_AVATAR_DECODE_ALLOC, MAX_AVATAR_IMAGE_DIMENSION,
};
use aster_drive_model::types::MediaProcessorKind;
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageFormat, ImageReader, Limits};

use super::resolve::resolve_avatar_processor;
use super::shared::{
    MediaOperation, ProcessedAvatar, TempDirGuard, cli_output_detail, run_cli_command_with_timeout,
};

pub async fn probe_vips_cli_command(command: &str) -> Result<String> {
    let command = media_processing_config::normalize_vips_command(command)?;
    if !media_processing_config::command_is_available(&command) {
        return Err(AsterError::validation_error(format!(
            "vips_cli command '{command}' is not available"
        )));
    }

    tracing::debug!(
        processor = "vips_cli",
        command = %command,
        "starting vips CLI probe"
    );

    let probe_command = command.clone();
    let output = tokio::task::spawn_blocking(move || {
        run_cli_command_with_timeout(&probe_command, &["--version"], |message| {
            AsterError::validation_error(format!("vips_cli probe failed: {message}"))
        })
    })
    .await
    .map_aster_err_ctx("vips CLI probe task panicked", AsterError::validation_error)??;

    if !output.status.success() {
        return Err(AsterError::validation_error(format!(
            "vips_cli probe failed for '{command}': {}",
            cli_output_detail(&output)
        )));
    }

    let detail = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !detail.is_empty() {
        detail
    } else {
        String::from_utf8_lossy(&output.stderr).trim().to_string()
    };

    tracing::debug!(
        processor = "vips_cli",
        command = %command,
        version = detail.as_str(),
        "vips CLI probe completed"
    );

    if detail.is_empty() {
        Ok(format!("vips_cli command '{command}' is available"))
    } else {
        Ok(format!(
            "vips_cli command '{command}' is available: {detail}"
        ))
    }
}

pub async fn process_staged_avatar(
    state: &PrimaryAppState,
    file_name: &str,
    source_path: PathBuf,
) -> Result<ProcessedAvatar> {
    let _permit = state
        .runtime_config()
        .avatar_render_runtime()
        .acquire_render()
        .await?;
    let processor = resolve_avatar_processor(state.runtime_config(), file_name)?;
    let processor_label = processor.kind().as_str();
    let started_at = Instant::now();
    tracing::debug!(
        operation = MediaOperation::Avatar.as_str(),
        processor = processor.kind().as_str(),
        file_name,
        source_path = %source_path.display(),
        "processing staged avatar via resolved media processor"
    );

    let inspected_path = source_path.clone();
    let dimensions =
        match tokio::task::spawn_blocking(move || inspect_avatar_dimensions(&inspected_path))
            .await
            .map_aster_err_ctx("avatar dimension inspection task panicked", |message| {
                file_upload_error_with_code(ApiErrorCode::AvatarRenderFailed, message)
            })? {
            Ok(dimensions) => dimensions,
            Err(error) => {
                state.metrics().record_avatar_rejection("dimensions");
                return Err(error);
            }
        };
    state
        .metrics()
        .record_avatar_dimension("width", dimensions.0);
    state
        .metrics()
        .record_avatar_dimension("height", dimensions.1);
    state
        .metrics()
        .set_avatar_budget_bytes("decode_alloc", MAX_AVATAR_DECODE_ALLOC);

    let result = match processor.kind() {
        MediaProcessorKind::Images => {
            tokio::task::spawn_blocking(move || generate_avatar_variants_from_path(&source_path))
                .await
                .map_aster_err_ctx("avatar processing task panicked", |message| {
                    file_upload_error_with_code(ApiErrorCode::AvatarRenderFailed, message)
                })?
        }
        MediaProcessorKind::VipsCli => {
            let command = processor.vips_command().to_string();
            render_avatar_path_with_vips_cli(state, source_path, &command).await
        }
        MediaProcessorKind::FfmpegCli => Err(precondition_failed_with_code(
            ApiErrorCode::AvatarProcessorUnavailable,
            "ffmpeg_cli avatar processing is not supported",
        )),
        MediaProcessorKind::FfprobeCli => Err(precondition_failed_with_code(
            ApiErrorCode::AvatarProcessorUnavailable,
            "ffprobe_cli avatar processing is not supported",
        )),
        MediaProcessorKind::Lofty => Err(precondition_failed_with_code(
            ApiErrorCode::AvatarProcessorUnavailable,
            "lofty avatar processing is not supported",
        )),
        MediaProcessorKind::StorageNative => Err(precondition_failed_with_code(
            ApiErrorCode::AvatarProcessorUnavailable,
            "storage-native avatar processing is not supported",
        )),
    };
    state
        .metrics()
        .record_avatar_render_duration(processor_label, started_at.elapsed().as_secs_f64());
    if result.is_err() {
        state.metrics().record_avatar_rejection("decode_or_render");
    }
    result
}

pub(super) fn avatar_decode_limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_AVATAR_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_AVATAR_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_AVATAR_DECODE_ALLOC);
    limits
}

fn open_avatar_reader(path: &std::path::Path) -> Result<ImageReader<BufReader<File>>> {
    let file = File::open(path).map_aster_err_ctx(
        "open staged avatar source",
        AsterError::storage_driver_error,
    )?;
    ImageReader::new(BufReader::new(file))
        .with_guessed_format()
        .map_aster_err_ctx("guess avatar format", AsterError::file_type_not_allowed)
}

pub(super) fn inspect_avatar_dimensions(path: &std::path::Path) -> Result<(u32, u32)> {
    let mut reader = open_avatar_reader(path)?;
    reader.limits(avatar_decode_limits());
    let dimensions = reader.into_dimensions().map_aster_err_ctx(
        "inspect avatar dimensions",
        AsterError::file_type_not_allowed,
    )?;
    validate_avatar_dimensions(dimensions)?;
    Ok(dimensions)
}

pub(super) fn validate_avatar_dimensions((width, height): (u32, u32)) -> Result<()> {
    if width == 0 || height == 0 {
        return Err(validation_error_with_code(
            ApiErrorCode::AvatarEmptyImage,
            "empty image",
        ));
    }
    if width > MAX_AVATAR_IMAGE_DIMENSION || height > MAX_AVATAR_IMAGE_DIMENSION {
        return Err(validation_error_with_code(
            ApiErrorCode::AvatarRenderFailed,
            format!(
                "avatar dimensions {width}x{height} exceed {MAX_AVATAR_IMAGE_DIMENSION}x{MAX_AVATAR_IMAGE_DIMENSION}"
            ),
        ));
    }
    Ok(())
}

fn generate_avatar_variants_from_path(path: &std::path::Path) -> Result<ProcessedAvatar> {
    let mut reader = open_avatar_reader(path)?;
    reader.limits(avatar_decode_limits());
    let image = reader
        .decode()
        .map_aster_err_ctx("decode avatar", AsterError::file_type_not_allowed)?;
    generate_avatar_variants_from_image(&image)
}

#[cfg(test)]
pub(super) fn generate_avatar_variants(data: Vec<u8>) -> Result<ProcessedAvatar> {
    let mut reader = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_aster_err_ctx("guess avatar format", AsterError::file_type_not_allowed)?;

    reader.limits(avatar_decode_limits());

    let img = reader
        .decode()
        .map_aster_err_ctx("decode avatar", AsterError::file_type_not_allowed)?;

    generate_avatar_variants_from_image(&img)
}

fn generate_avatar_variants_from_image(img: &DynamicImage) -> Result<ProcessedAvatar> {
    let (width, height) = img.dimensions();
    validate_avatar_dimensions((width, height))?;

    let side = width.min(height);
    let left = (width - side) / 2;
    let top = (height - side) / 2;
    let square = img.view(left, top, side, side);

    let large_bytes = {
        let large = image::imageops::resize(
            &*square,
            AVATAR_SIZE_LG,
            AVATAR_SIZE_LG,
            FilterType::Triangle,
        );
        encode_avatar_webp(&DynamicImage::ImageRgba8(large))?
    };
    let small_bytes = {
        let small = image::imageops::resize(
            &*square,
            AVATAR_SIZE_SM,
            AVATAR_SIZE_SM,
            FilterType::Triangle,
        );
        encode_avatar_webp(&DynamicImage::ImageRgba8(small))?
    };

    Ok(ProcessedAvatar {
        small_bytes,
        large_bytes,
    })
}

fn encode_avatar_webp(img: &DynamicImage) -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    img.write_to(&mut buf, ImageFormat::WebP)
        .map_aster_err_ctx("encode avatar webp", |message| {
            file_upload_error_with_code(ApiErrorCode::AvatarRenderFailed, message)
        })?;
    Ok(buf.into_inner())
}

fn validate_avatar_variant_output(bytes: &[u8], expected_size: u32, label: &str) -> Result<()> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_aster_err_ctx("guess avatar vips output format", |message| {
            file_upload_error_with_code(ApiErrorCode::AvatarOutputInvalid, message)
        })?;

    if reader.format() != Some(ImageFormat::WebP) {
        return Err(file_upload_error_with_code(
            ApiErrorCode::AvatarOutputInvalid,
            format!("avatar vips {label} output is not WebP"),
        ));
    }

    let mut limits = Limits::default();
    limits.max_alloc = Some(MAX_AVATAR_DECODE_ALLOC);
    reader.limits(limits);

    let image = reader
        .decode()
        .map_aster_err_ctx("decode avatar vips output", |message| {
            file_upload_error_with_code(ApiErrorCode::AvatarOutputInvalid, message)
        })?;
    let (width, height) = image.dimensions();
    if width != expected_size || height != expected_size {
        return Err(file_upload_error_with_code(
            ApiErrorCode::AvatarOutputInvalid,
            format!("avatar vips {label} output has unexpected dimensions {width}x{height}"),
        ));
    }

    Ok(())
}

async fn render_avatar_path_with_vips_cli(
    state: &PrimaryAppState,
    input_path: PathBuf,
    command: &str,
) -> Result<ProcessedAvatar> {
    let temp_root = aster_forge_utils::paths::runtime_temp_dir(&state.config().server.temp_dir);
    let temp_dir =
        PathBuf::from(temp_root).join(format!("media-vips-avatar-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .map_aster_err_ctx(
            "create avatar vips temp dir",
            AsterError::storage_driver_error,
        )?;
    let temp_dir = TempDirGuard::new(temp_dir, "media processing avatar temp dir");

    let small_output_path = temp_dir.path().join("avatar-512.webp");
    let large_output_path = temp_dir.path().join("avatar-1024.webp");

    let command = command.to_string();
    let input_arg = input_path.to_string_lossy().to_string();
    let small_output_arg = small_output_path.to_string_lossy().to_string();
    let large_output_arg = large_output_path.to_string_lossy().to_string();
    tracing::debug!(
        operation = MediaOperation::Avatar.as_str(),
        processor = MediaProcessorKind::VipsCli.as_str(),
        command,
        input_path = input_arg,
        small_output_path = small_output_arg,
        large_output_path = large_output_arg,
        "starting vips CLI avatar render"
    );
    tokio::task::spawn_blocking({
        let command = command.clone();
        let input_arg = input_arg.clone();
        let output_arg = large_output_arg.clone();
        move || run_avatar_vips_variant(&command, &input_arg, &output_arg, AVATAR_SIZE_LG)
    })
    .await
    .map_aster_err_ctx("avatar vips CLI 1024 task panicked", |message| {
        file_upload_error_with_code(ApiErrorCode::AvatarRenderFailed, message)
    })??;
    tokio::task::spawn_blocking({
        let command = command.clone();
        let input_arg = input_arg.clone();
        let output_arg = small_output_arg.clone();
        move || run_avatar_vips_variant(&command, &input_arg, &output_arg, AVATAR_SIZE_SM)
    })
    .await
    .map_aster_err_ctx("avatar vips CLI 512 task panicked", |message| {
        file_upload_error_with_code(ApiErrorCode::AvatarRenderFailed, message)
    })??;

    let small_bytes = tokio::fs::read(&small_output_path)
        .await
        .map_aster_err_ctx("read avatar vips 512 output", |message| {
            file_upload_error_with_code(ApiErrorCode::AvatarRenderFailed, message)
        })?;
    let large_bytes = tokio::fs::read(&large_output_path)
        .await
        .map_aster_err_ctx("read avatar vips 1024 output", |message| {
            file_upload_error_with_code(ApiErrorCode::AvatarRenderFailed, message)
        })?;
    validate_avatar_variant_output(&small_bytes, AVATAR_SIZE_SM, "512")?;
    validate_avatar_variant_output(&large_bytes, AVATAR_SIZE_LG, "1024")?;
    tracing::debug!(
        operation = MediaOperation::Avatar.as_str(),
        processor = MediaProcessorKind::VipsCli.as_str(),
        small_bytes = small_bytes.len(),
        large_bytes = large_bytes.len(),
        "avatar vips CLI render completed and validated"
    );

    Ok(ProcessedAvatar {
        small_bytes,
        large_bytes,
    })
}

fn run_avatar_vips_variant(
    command: &str,
    input_arg: &str,
    output_arg: &str,
    size: u32,
) -> Result<()> {
    let size_arg = size.to_string();
    let output = run_cli_command_with_timeout(
        command,
        &[
            "thumbnail",
            input_arg,
            output_arg,
            &size_arg,
            "--height",
            &size_arg,
            "--size",
            "both",
            "--crop",
            "centre",
        ],
        |message| file_upload_error_with_code(ApiErrorCode::AvatarRenderFailed, message),
    )?;
    if !output.status.success() {
        return Err(file_upload_error_with_code(
            ApiErrorCode::AvatarRenderFailed,
            format!(
                "vips CLI avatar command failed for {size}px output: {}",
                cli_output_detail(&output)
            ),
        ));
    }
    Ok(())
}

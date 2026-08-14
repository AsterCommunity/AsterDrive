use std::fs::File;
use std::io::{BufRead, BufReader, Cursor, Seek};
use std::path::PathBuf;
use std::time::Instant;

use crate::api::api_error_code::ApiErrorCode;
use crate::config::{
    avatar_render::AvatarRenderRuntime, media_processing as media_processing_config,
};
use crate::errors::{
    AsterError, MapAsterErr, Result, file_upload_error_with_code, precondition_failed_with_code,
    validation_error_with_code,
};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::user::profile::shared::{
    AVATAR_SIZE_LG, AVATAR_SIZE_SM, MAX_AVATAR_DECODE_ALLOC, MAX_AVATAR_IMAGE_DIMENSION,
};
use aster_drive_metrics::SharedMetricsRecorder;
use aster_drive_model::types::MediaProcessorKind;
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageDecoder, ImageFormat, ImageReader, Limits};
use tokio::sync::OwnedSemaphorePermit;

use super::resolve::resolve_avatar_processor;
use super::shared::{
    MediaOperation, ProcessedAvatar, TempDirGuard, cli_output_detail, run_cli_command_with_timeout,
};

enum AvatarRenderGauge {
    Waiting,
    Active,
}

struct AvatarRenderGaugeGuard {
    metrics: SharedMetricsRecorder,
    gauge: AvatarRenderGauge,
}

impl AvatarRenderGaugeGuard {
    fn new(metrics: SharedMetricsRecorder, gauge: AvatarRenderGauge) -> Self {
        match gauge {
            AvatarRenderGauge::Waiting => metrics.adjust_avatar_render_waiting(1),
            AvatarRenderGauge::Active => metrics.adjust_avatar_render_active(1),
        }
        Self { metrics, gauge }
    }
}

impl Drop for AvatarRenderGaugeGuard {
    fn drop(&mut self) {
        match self.gauge {
            AvatarRenderGauge::Waiting => self.metrics.adjust_avatar_render_waiting(-1),
            AvatarRenderGauge::Active => self.metrics.adjust_avatar_render_active(-1),
        }
    }
}

struct AvatarRenderPermit {
    // Struct fields drop in declaration order, so publish the active gauge
    // transition before returning the semaphore permit to the next waiter.
    _active: AvatarRenderGaugeGuard,
    _permit: OwnedSemaphorePermit,
}

async fn acquire_avatar_render_permit(
    runtime: &AvatarRenderRuntime,
    metrics: SharedMetricsRecorder,
) -> Result<AvatarRenderPermit> {
    let wait_started_at = Instant::now();
    let waiting = AvatarRenderGaugeGuard::new(metrics.clone(), AvatarRenderGauge::Waiting);
    let permit = runtime.acquire_render().await?;
    metrics.record_avatar_render_wait_duration(wait_started_at.elapsed().as_secs_f64());
    drop(waiting);
    let active = AvatarRenderGaugeGuard::new(metrics, AvatarRenderGauge::Active);
    Ok(AvatarRenderPermit {
        _active: active,
        _permit: permit,
    })
}

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
    let _permit = acquire_avatar_render_permit(
        state.runtime_config().avatar_render_runtime(),
        state.metrics().clone(),
    )
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

    let (dimensions, result) = if processor.kind() == MediaProcessorKind::Images {
        let output =
            tokio::task::spawn_blocking(move || generate_avatar_variants_from_path(&source_path))
                .await
                .map_aster_err_ctx("avatar processing task panicked", |message| {
                    file_upload_error_with_code(ApiErrorCode::AvatarRenderFailed, message)
                })?;
        match output {
            Ok(output) => (output.dimensions, Ok(output.processed)),
            Err(AvatarImagesProcessingError::Dimensions(error)) => {
                state.metrics().record_avatar_rejection("dimensions");
                return Err(error);
            }
            Err(AvatarImagesProcessingError::DecodeOrRender { dimensions, error }) => {
                (dimensions, Err(error))
            }
        }
    } else {
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
        let result = match processor.kind() {
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
            MediaProcessorKind::Images => Err(AsterError::internal_error(
                "images avatar processor reached non-images dispatch",
            )),
        };
        (dimensions, result)
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

struct AvatarImagesProcessingOutput {
    dimensions: (u32, u32),
    processed: ProcessedAvatar,
}

#[derive(Debug)]
enum AvatarImagesProcessingError {
    Dimensions(AsterError),
    DecodeOrRender {
        dimensions: (u32, u32),
        error: AsterError,
    },
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

fn generate_avatar_variants_from_path(
    path: &std::path::Path,
) -> std::result::Result<AvatarImagesProcessingOutput, AvatarImagesProcessingError> {
    let reader = open_avatar_reader(path).map_err(AvatarImagesProcessingError::Dimensions)?;
    generate_avatar_variants_from_reader(reader)
}

fn generate_avatar_variants_from_reader<R: BufRead + Seek>(
    mut reader: ImageReader<R>,
) -> std::result::Result<AvatarImagesProcessingOutput, AvatarImagesProcessingError> {
    let mut limits = avatar_decode_limits();
    reader.limits(limits.clone());
    // Keep the decoder created for the dimensions check and consume that same
    // decoder for pixels. In image 0.25 the JPEG adapter buffers the complete
    // compressed source while constructing the decoder, so calling
    // `into_dimensions` and then reopening the file for `decode` would read and
    // buffer the source twice. The render permit intentionally covers decoder
    // construction as well as resize/encode work for the same reason.
    let mut decoder = reader
        .into_decoder()
        .map_aster_err_ctx(
            "inspect avatar dimensions",
            AsterError::file_type_not_allowed,
        )
        .map_err(AvatarImagesProcessingError::Dimensions)?;
    let dimensions = decoder.dimensions();
    validate_avatar_dimensions(dimensions).map_err(AvatarImagesProcessingError::Dimensions)?;

    limits
        .reserve(decoder.total_bytes())
        .map_aster_err_ctx(
            "reserve avatar decode output",
            AsterError::file_type_not_allowed,
        )
        .map_err(|error| AvatarImagesProcessingError::DecodeOrRender { dimensions, error })?;
    decoder
        .set_limits(limits)
        .map_aster_err_ctx(
            "apply avatar decode limits",
            AsterError::file_type_not_allowed,
        )
        .map_err(|error| AvatarImagesProcessingError::DecodeOrRender { dimensions, error })?;
    let image = DynamicImage::from_decoder(decoder)
        .map_aster_err_ctx("decode avatar", AsterError::file_type_not_allowed)
        .map_err(|error| AvatarImagesProcessingError::DecodeOrRender { dimensions, error })?;
    let processed = generate_avatar_variants_from_image(&image)
        .map_err(|error| AvatarImagesProcessingError::DecodeOrRender { dimensions, error })?;
    Ok(AvatarImagesProcessingOutput {
        dimensions,
        processed,
    })
}

#[cfg(test)]
pub(super) fn generate_avatar_variants(data: Vec<u8>) -> Result<ProcessedAvatar> {
    let reader = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_aster_err_ctx("guess avatar format", AsterError::file_type_not_allowed)?;
    generate_avatar_variants_from_reader(reader)
        .map(|output| output.processed)
        .map_err(|error| match error {
            AvatarImagesProcessingError::Dimensions(error)
            | AvatarImagesProcessingError::DecodeOrRender { error, .. } => error,
        })
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

#[cfg(test)]
mod tests {
    use std::io::{Read, Result as IoResult, SeekFrom};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
    use std::time::Duration;

    use aster_drive_metrics::MetricsRecorder;
    use image::{ImageBuffer, Rgb};

    use super::*;

    #[derive(Default)]
    struct RecordingAvatarMetrics {
        active: AtomicI64,
        max_active: AtomicI64,
        waiting: AtomicI64,
        wait_samples: AtomicUsize,
        rejections: AtomicUsize,
    }

    impl MetricsRecorder for RecordingAvatarMetrics {
        fn record_avatar_render_wait_duration(&self, _duration_seconds: f64) {
            self.wait_samples.fetch_add(1, Ordering::SeqCst);
        }

        fn adjust_avatar_render_waiting(&self, delta: i64) {
            self.waiting.fetch_add(delta, Ordering::SeqCst);
        }

        fn adjust_avatar_render_active(&self, delta: i64) {
            let active = self.active.fetch_add(delta, Ordering::SeqCst) + delta;
            self.max_active.fetch_max(active, Ordering::SeqCst);
        }

        fn record_avatar_rejection(&self, _reason: &'static str) {
            self.rejections.fetch_add(1, Ordering::SeqCst);
        }
    }

    async fn wait_for_metric(value: &AtomicI64, expected: i64) {
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if value.load(Ordering::SeqCst) == expected {
                    return;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn normal_render_requests_wait_and_eventually_acquire_without_rejection() {
        let runtime = AvatarRenderRuntime::new(2).unwrap();
        let recorded = Arc::new(RecordingAvatarMetrics::default());
        let metrics: SharedMetricsRecorder = recorded.clone();
        let first = acquire_avatar_render_permit(&runtime, metrics.clone())
            .await
            .unwrap();
        let second = acquire_avatar_render_permit(&runtime, metrics.clone())
            .await
            .unwrap();
        assert_eq!(recorded.active.load(Ordering::SeqCst), 2);

        let mut waiting = Vec::new();
        for _ in 0..6 {
            let waiting_runtime = runtime.clone();
            let waiting_metrics = metrics.clone();
            waiting.push(tokio::spawn(async move {
                acquire_avatar_render_permit(&waiting_runtime, waiting_metrics).await
            }));
        }
        wait_for_metric(&recorded.waiting, 6).await;

        drop(first);
        drop(second);
        for waiter in waiting {
            let permit = tokio::time::timeout(Duration::from_secs(1), waiter)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            drop(permit);
        }

        assert_eq!(recorded.waiting.load(Ordering::SeqCst), 0);
        assert_eq!(recorded.active.load(Ordering::SeqCst), 0);
        assert_eq!(recorded.max_active.load(Ordering::SeqCst), 2);
        assert_eq!(recorded.wait_samples.load(Ordering::SeqCst), 8);
        assert_eq!(recorded.rejections.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn cancelled_render_waiter_releases_waiting_metric() {
        let runtime = AvatarRenderRuntime::new(1).unwrap();
        let recorded = Arc::new(RecordingAvatarMetrics::default());
        let metrics: SharedMetricsRecorder = recorded.clone();
        let held = acquire_avatar_render_permit(&runtime, metrics.clone())
            .await
            .unwrap();

        let waiting_runtime = runtime.clone();
        let waiting_metrics = metrics.clone();
        let waiting = tokio::spawn(async move {
            acquire_avatar_render_permit(&waiting_runtime, waiting_metrics).await
        });
        wait_for_metric(&recorded.waiting, 1).await;
        waiting.abort();
        assert!(matches!(waiting.await, Err(error) if error.is_cancelled()));
        wait_for_metric(&recorded.waiting, 0).await;

        drop(held);
        assert_eq!(recorded.active.load(Ordering::SeqCst), 0);
        assert_eq!(recorded.wait_samples.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn cancelled_active_render_releases_permit_and_active_metric() {
        let runtime = AvatarRenderRuntime::new(1).unwrap();
        let recorded = Arc::new(RecordingAvatarMetrics::default());
        let metrics: SharedMetricsRecorder = recorded.clone();
        let active_runtime = runtime.clone();
        let active_metrics = metrics.clone();
        let (acquired_tx, acquired_rx) = tokio::sync::oneshot::channel();
        let active = tokio::spawn(async move {
            let _permit = acquire_avatar_render_permit(&active_runtime, active_metrics)
                .await
                .unwrap();
            acquired_tx.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        acquired_rx.await.unwrap();
        assert_eq!(recorded.active.load(Ordering::SeqCst), 1);

        active.abort();
        assert!(matches!(active.await, Err(error) if error.is_cancelled()));
        wait_for_metric(&recorded.active, 0).await;

        let reacquired = tokio::time::timeout(
            Duration::from_secs(1),
            acquire_avatar_render_permit(&runtime, metrics),
        )
        .await
        .unwrap()
        .unwrap();
        drop(reacquired);
        assert_eq!(recorded.active.load(Ordering::SeqCst), 0);
        assert_eq!(recorded.wait_samples.load(Ordering::SeqCst), 2);
    }

    struct CountingReader {
        inner: Cursor<Vec<u8>>,
        bytes_read: Arc<AtomicUsize>,
    }

    impl Read for CountingReader {
        fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
            let read = self.inner.read(buf)?;
            self.bytes_read.fetch_add(read, Ordering::SeqCst);
            Ok(read)
        }
    }

    impl BufRead for CountingReader {
        fn fill_buf(&mut self) -> IoResult<&[u8]> {
            self.inner.fill_buf()
        }

        fn consume(&mut self, amount: usize) {
            self.bytes_read.fetch_add(amount, Ordering::SeqCst);
            self.inner.consume(amount);
        }
    }

    impl Seek for CountingReader {
        fn seek(&mut self, position: SeekFrom) -> IoResult<u64> {
            self.inner.seek(position)
        }
    }

    #[test]
    fn images_processor_reads_jpeg_source_once_for_dimensions_and_decode() {
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_fn(32, 24, |x, y| {
            Rgb([
                x.to_le_bytes()[0].wrapping_mul(7),
                y.to_le_bytes()[0].wrapping_mul(11),
                x.wrapping_add(y).to_le_bytes()[0].wrapping_mul(5),
            ])
        }));
        let mut encoded = Cursor::new(Vec::new());
        source.write_to(&mut encoded, ImageFormat::Jpeg).unwrap();
        let encoded = encoded.into_inner();
        let source_len = encoded.len();
        let bytes_read = Arc::new(AtomicUsize::new(0));
        let reader = ImageReader::new(CountingReader {
            inner: Cursor::new(encoded),
            bytes_read: bytes_read.clone(),
        })
        .with_guessed_format()
        .unwrap();

        let output = generate_avatar_variants_from_reader(reader).unwrap();

        assert_eq!(output.dimensions, (32, 24));
        assert!(!output.processed.large_bytes.is_empty());
        assert!(!output.processed.small_bytes.is_empty());
        assert!(bytes_read.load(Ordering::SeqCst) <= source_len + 16);
    }
}

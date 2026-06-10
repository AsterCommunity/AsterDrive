use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;

use crate::config::RuntimeConfig;
use crate::config::media_processing as media_processing_config;
use crate::config::operations;
use crate::errors::{AsterError, Result};
use crate::storage::StorageDriver;
use crate::types::MediaProcessorKind;

const CLI_PROCESS_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_CLI_OUTPUT_BYTES: usize = 16 * 1024 * 1024;

pub(crate) use crate::utils::raii::TempDirGuard;

pub(crate) fn run_cli_command_with_timeout(
    command: &str,
    args: &[&str],
    error: impl Fn(String) -> AsterError,
) -> Result<Output> {
    let mut child = Command::new(command)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|spawn_error| error(format!("spawn CLI '{command}': {spawn_error}")))?;
    let stdout_reader = child.stdout.take().map(spawn_pipe_reader);
    let stderr_reader = child.stderr.take().map(spawn_pipe_reader);
    let started_at = Instant::now();

    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|wait_error| error(format!("wait for CLI '{command}': {wait_error}")))?
        {
            break status;
        }

        if started_at.elapsed() >= CLI_PROCESS_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            let stdout = join_pipe_reader(stdout_reader);
            let stderr = join_pipe_reader(stderr_reader);
            if let Some(error) = stdout.err().or_else(|| stderr.err()) {
                tracing::warn!("failed to read CLI output after timeout: {error}");
            }
            return Err(error(format!(
                "CLI '{command}' timed out after {} seconds",
                CLI_PROCESS_TIMEOUT.as_secs()
            )));
        }

        std::thread::sleep(Duration::from_millis(50));
    };

    let stdout = join_pipe_reader(stdout_reader)
        .map_err(|read_error| error(format!("read CLI stdout: {read_error}")))?;
    let stderr = join_pipe_reader(stderr_reader)
        .map_err(|read_error| error(format!("read CLI stderr: {read_error}")))?;

    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

pub(crate) fn cli_output_detail(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("exit status {}", output.status)
    }
}

fn spawn_pipe_reader<R>(mut pipe: R) -> std::thread::JoinHandle<std::io::Result<Vec<u8>>>
where
    R: Read + Send + 'static,
{
    std::thread::spawn(move || {
        let mut output = Vec::new();
        let mut buffer = [0_u8; 8192];
        loop {
            let read = pipe.read(&mut buffer)?;
            if read == 0 {
                break;
            }

            if output.len() + read > MAX_CLI_OUTPUT_BYTES {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("CLI output exceeded {} bytes", MAX_CLI_OUTPUT_BYTES),
                ));
            }

            output.extend_from_slice(&buffer[..read]);
        }
        Ok(output)
    })
}

fn join_pipe_reader(
    reader: Option<std::thread::JoinHandle<std::io::Result<Vec<u8>>>>,
) -> std::io::Result<Vec<u8>> {
    match reader {
        Some(reader) => reader
            .join()
            .map_err(|_| std::io::Error::other("CLI output reader thread panicked"))?,
        None => Ok(Vec::new()),
    }
}

pub(crate) const FFMPEG_CLI_THUMBNAIL_PROCESSOR_NAMESPACE: &str = "ffmpeg-cli";
pub(crate) const LOFTY_THUMBNAIL_PROCESSOR_NAMESPACE: &str = "lofty";
pub(crate) const STORAGE_NATIVE_THUMBNAIL_PROCESSOR_NAMESPACE: &str = "storage-native";
pub(crate) const VIPS_CLI_THUMBNAIL_PROCESSOR_NAMESPACE: &str = "vips-cli";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MediaOperation {
    Thumbnail,
    Avatar,
}

impl MediaOperation {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Thumbnail => "thumbnail",
            Self::Avatar => "avatar",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedMediaProcessor {
    kind: MediaProcessorKind,
    command: Option<String>,
}

impl ResolvedMediaProcessor {
    pub(crate) fn new(kind: MediaProcessorKind) -> Self {
        Self {
            kind,
            command: None,
        }
    }

    pub(crate) fn with_command(kind: MediaProcessorKind, command: String) -> Self {
        Self {
            kind,
            command: Some(command),
        }
    }

    pub(crate) fn kind(&self) -> MediaProcessorKind {
        self.kind
    }

    pub(crate) fn vips_command(&self) -> &str {
        self.command
            .as_deref()
            .unwrap_or(media_processing_config::DEFAULT_VIPS_COMMAND)
    }

    pub(crate) fn ffmpeg_command(&self) -> &str {
        self.command
            .as_deref()
            .unwrap_or(media_processing_config::DEFAULT_FFMPEG_COMMAND)
    }

    pub(crate) fn thumbnail_processor(&self) -> &'static str {
        match self.kind {
            MediaProcessorKind::Images => {
                crate::services::thumbnail_service::IMAGES_THUMBNAIL_PROCESSOR_NAMESPACE
            }
            MediaProcessorKind::VipsCli => VIPS_CLI_THUMBNAIL_PROCESSOR_NAMESPACE,
            MediaProcessorKind::FfmpegCli => FFMPEG_CLI_THUMBNAIL_PROCESSOR_NAMESPACE,
            MediaProcessorKind::Lofty => LOFTY_THUMBNAIL_PROCESSOR_NAMESPACE,
            MediaProcessorKind::FfprobeCli => {
                crate::services::thumbnail_service::IMAGES_THUMBNAIL_PROCESSOR_NAMESPACE
            }
            MediaProcessorKind::StorageNative => STORAGE_NATIVE_THUMBNAIL_PROCESSOR_NAMESPACE,
        }
    }

    pub(crate) fn thumbnail_version(&self, runtime_config: &RuntimeConfig) -> String {
        crate::services::thumbnail_service::thumbnail_version_for_dimension(
            operations::thumbnail_max_dimension(runtime_config),
        )
    }

    pub(crate) fn image_preview_processor(&self) -> &'static str {
        self.thumbnail_processor()
    }

    pub(crate) fn image_preview_version(&self, runtime_config: &RuntimeConfig) -> String {
        crate::services::thumbnail_service::image_preview_version_for_dimension(
            operations::image_preview_max_dimension(runtime_config),
        )
    }

    pub(crate) fn cache_path(&self, blob_hash: &str, runtime_config: &RuntimeConfig) -> String {
        let thumbnail_version = self.thumbnail_version(runtime_config);
        crate::services::thumbnail_service::thumb_path_for(
            blob_hash,
            self.thumbnail_processor(),
            &thumbnail_version,
        )
    }

    pub(crate) fn image_preview_cache_path(
        &self,
        blob_hash: &str,
        runtime_config: &RuntimeConfig,
    ) -> String {
        let image_preview_version = self.image_preview_version(runtime_config);
        crate::services::thumbnail_service::image_preview_path_for(
            blob_hash,
            self.image_preview_processor(),
            &image_preview_version,
        )
    }
}

pub struct ThumbnailData {
    pub data: Bytes,
    pub thumbnail_processor: String,
    pub thumbnail_version: String,
}

pub struct StoredThumbnail {
    pub thumbnail_path: String,
    pub thumbnail_processor: String,
    pub thumbnail_version: String,
    pub reused_existing_thumbnail: bool,
}

pub struct ImagePreviewData {
    pub data: Bytes,
    pub image_preview_processor: String,
    pub image_preview_version: String,
}

pub struct StoredImagePreview {
    pub image_preview_path: String,
    pub image_preview_processor: String,
    pub image_preview_version: String,
    pub reused_existing_preview: bool,
}

#[derive(Debug)]
pub struct ProcessedAvatar {
    pub small_bytes: Vec<u8>,
    pub large_bytes: Vec<u8>,
}

pub(crate) struct ThumbnailContext {
    pub(crate) driver: Arc<dyn StorageDriver>,
    pub(crate) processor: ResolvedMediaProcessor,
}

pub fn thumbnail_etag_value_for(
    blob_hash: &str,
    thumbnail_processor: Option<&str>,
    thumbnail_version: Option<&str>,
) -> String {
    crate::services::thumbnail_service::thumbnail_etag_value_for(
        blob_hash,
        thumbnail_processor,
        thumbnail_version,
    )
}

pub fn image_preview_etag_value_for(
    blob_hash: &str,
    image_preview_processor: &str,
    image_preview_version: &str,
) -> String {
    crate::services::thumbnail_service::image_preview_etag_value_for(
        blob_hash,
        image_preview_processor,
        image_preview_version,
    )
}

pub(crate) fn known_thumbnail_cache_paths(blob_hash: &str, current_max_dim: u32) -> Vec<String> {
    let processors = [
        crate::services::thumbnail_service::IMAGES_THUMBNAIL_PROCESSOR_NAMESPACE,
        VIPS_CLI_THUMBNAIL_PROCESSOR_NAMESPACE,
        FFMPEG_CLI_THUMBNAIL_PROCESSOR_NAMESPACE,
        LOFTY_THUMBNAIL_PROCESSOR_NAMESPACE,
        STORAGE_NATIVE_THUMBNAIL_PROCESSOR_NAMESPACE,
    ];
    let default_version = crate::services::thumbnail_service::CURRENT_THUMBNAIL_VERSION;
    let current_version =
        crate::services::thumbnail_service::thumbnail_version_for_dimension(current_max_dim);
    let mut paths = processors
        .iter()
        .map(|thumbnail_processor| {
            crate::services::thumbnail_service::thumb_path_for(
                blob_hash,
                thumbnail_processor,
                default_version,
            )
        })
        .collect::<Vec<_>>();
    if current_version != default_version {
        paths.extend(processors.iter().map(|thumbnail_processor| {
            crate::services::thumbnail_service::thumb_path_for(
                blob_hash,
                thumbnail_processor,
                &current_version,
            )
        }));
    }
    paths
}

pub(crate) fn known_image_preview_cache_paths(
    blob_hash: &str,
    current_max_dim: u32,
) -> Vec<String> {
    let processors = [
        crate::services::thumbnail_service::IMAGES_THUMBNAIL_PROCESSOR_NAMESPACE,
        VIPS_CLI_THUMBNAIL_PROCESSOR_NAMESPACE,
        FFMPEG_CLI_THUMBNAIL_PROCESSOR_NAMESPACE,
        STORAGE_NATIVE_THUMBNAIL_PROCESSOR_NAMESPACE,
    ];
    let default_version = crate::services::thumbnail_service::CURRENT_IMAGE_PREVIEW_VERSION;
    let current_version =
        crate::services::thumbnail_service::image_preview_version_for_dimension(current_max_dim);
    let mut paths = processors
        .iter()
        .map(|image_preview_processor| {
            crate::services::thumbnail_service::image_preview_path_for(
                blob_hash,
                image_preview_processor,
                default_version,
            )
        })
        .collect::<Vec<_>>();
    if current_version != default_version {
        paths.extend(processors.iter().map(|image_preview_processor| {
            crate::services::thumbnail_service::image_preview_path_for(
                blob_hash,
                image_preview_processor,
                &current_version,
            )
        }));
    }
    paths
}

pub(crate) fn requires_server_side_source_limit(processor: &ResolvedMediaProcessor) -> bool {
    processor.kind() != MediaProcessorKind::StorageNative
}

pub(crate) fn cli_source_temp_path(
    temp_dir: &std::path::Path,
    source_file_name: &str,
    source_mime_type: &str,
) -> PathBuf {
    let extension = media_processing_config::file_extension(source_file_name)
        .or_else(|| infer_extension_from_mime(source_mime_type))
        .unwrap_or_else(|| "bin".to_string());
    temp_dir.join(format!("source.{extension}"))
}

fn infer_extension_from_mime(source_mime_type: &str) -> Option<String> {
    mime_guess::get_mime_extensions_str(source_mime_type)
        .and_then(|extensions| extensions.first().copied())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::{
        FFMPEG_CLI_THUMBNAIL_PROCESSOR_NAMESPACE, LOFTY_THUMBNAIL_PROCESSOR_NAMESPACE,
        MAX_CLI_OUTPUT_BYTES, ResolvedMediaProcessor, STORAGE_NATIVE_THUMBNAIL_PROCESSOR_NAMESPACE,
        VIPS_CLI_THUMBNAIL_PROCESSOR_NAMESPACE, join_pipe_reader, known_image_preview_cache_paths,
        known_thumbnail_cache_paths, spawn_pipe_reader,
    };
    use crate::config::definitions::CONFIG_CATEGORY_FILE_PROCESSING_MEDIA;
    use crate::config::{
        RuntimeConfig,
        operations::{IMAGE_PREVIEW_MAX_DIMENSION_KEY, THUMBNAIL_MAX_DIMENSION_KEY},
    };
    use crate::entities::system_config;
    use crate::types::{MediaProcessorKind, SystemConfigSource, SystemConfigValueType};
    use chrono::Utc;
    use std::io::Cursor;

    const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn config_model(key: &str, value: &str) -> system_config::Model {
        system_config::Model {
            id: 1,
            key: key.to_string(),
            value: value.to_string(),
            value_type: SystemConfigValueType::Number,
            requires_restart: false,
            is_sensitive: false,
            source: SystemConfigSource::System,
            visibility: crate::types::SystemConfigVisibility::Private,
            namespace: String::new(),
            category: CONFIG_CATEGORY_FILE_PROCESSING_MEDIA.to_string(),
            description: "test".to_string(),
            updated_at: Utc::now(),
            updated_by: None,
        }
    }

    #[test]
    fn known_thumbnail_cache_paths_include_normalized_namespaces() {
        let paths = known_thumbnail_cache_paths(
            HASH,
            crate::config::operations::DEFAULT_THUMBNAIL_MAX_DIMENSION,
        );

        assert_eq!(
            paths,
            vec![
                crate::services::thumbnail_service::thumb_path_for(
                    HASH,
                    crate::services::thumbnail_service::IMAGES_THUMBNAIL_PROCESSOR_NAMESPACE,
                    crate::services::thumbnail_service::CURRENT_THUMBNAIL_VERSION,
                ),
                crate::services::thumbnail_service::thumb_path_for(
                    HASH,
                    VIPS_CLI_THUMBNAIL_PROCESSOR_NAMESPACE,
                    crate::services::thumbnail_service::CURRENT_THUMBNAIL_VERSION,
                ),
                crate::services::thumbnail_service::thumb_path_for(
                    HASH,
                    FFMPEG_CLI_THUMBNAIL_PROCESSOR_NAMESPACE,
                    crate::services::thumbnail_service::CURRENT_THUMBNAIL_VERSION,
                ),
                crate::services::thumbnail_service::thumb_path_for(
                    HASH,
                    LOFTY_THUMBNAIL_PROCESSOR_NAMESPACE,
                    crate::services::thumbnail_service::CURRENT_THUMBNAIL_VERSION,
                ),
                crate::services::thumbnail_service::thumb_path_for(
                    HASH,
                    STORAGE_NATIVE_THUMBNAIL_PROCESSOR_NAMESPACE,
                    crate::services::thumbnail_service::CURRENT_THUMBNAIL_VERSION,
                ),
            ]
        );
    }

    #[test]
    fn known_thumbnail_cache_paths_include_configured_dimension_namespace() {
        let paths = known_thumbnail_cache_paths(HASH, 320);

        assert_eq!(
            paths,
            vec![
                crate::services::thumbnail_service::thumb_path_for(
                    HASH,
                    crate::services::thumbnail_service::IMAGES_THUMBNAIL_PROCESSOR_NAMESPACE,
                    crate::services::thumbnail_service::CURRENT_THUMBNAIL_VERSION,
                ),
                crate::services::thumbnail_service::thumb_path_for(
                    HASH,
                    VIPS_CLI_THUMBNAIL_PROCESSOR_NAMESPACE,
                    crate::services::thumbnail_service::CURRENT_THUMBNAIL_VERSION,
                ),
                crate::services::thumbnail_service::thumb_path_for(
                    HASH,
                    FFMPEG_CLI_THUMBNAIL_PROCESSOR_NAMESPACE,
                    crate::services::thumbnail_service::CURRENT_THUMBNAIL_VERSION,
                ),
                crate::services::thumbnail_service::thumb_path_for(
                    HASH,
                    LOFTY_THUMBNAIL_PROCESSOR_NAMESPACE,
                    crate::services::thumbnail_service::CURRENT_THUMBNAIL_VERSION,
                ),
                crate::services::thumbnail_service::thumb_path_for(
                    HASH,
                    STORAGE_NATIVE_THUMBNAIL_PROCESSOR_NAMESPACE,
                    crate::services::thumbnail_service::CURRENT_THUMBNAIL_VERSION,
                ),
                crate::services::thumbnail_service::thumb_path_for(HASH, "images", "1-d320",),
                crate::services::thumbnail_service::thumb_path_for(
                    HASH,
                    VIPS_CLI_THUMBNAIL_PROCESSOR_NAMESPACE,
                    "1-d320",
                ),
                crate::services::thumbnail_service::thumb_path_for(
                    HASH,
                    FFMPEG_CLI_THUMBNAIL_PROCESSOR_NAMESPACE,
                    "1-d320",
                ),
                crate::services::thumbnail_service::thumb_path_for(
                    HASH,
                    LOFTY_THUMBNAIL_PROCESSOR_NAMESPACE,
                    "1-d320",
                ),
                crate::services::thumbnail_service::thumb_path_for(
                    HASH,
                    STORAGE_NATIVE_THUMBNAIL_PROCESSOR_NAMESPACE,
                    "1-d320",
                ),
            ]
        );
    }

    #[test]
    fn known_image_preview_cache_paths_include_normalized_namespaces() {
        let paths = known_image_preview_cache_paths(
            HASH,
            crate::config::operations::DEFAULT_IMAGE_PREVIEW_MAX_DIMENSION,
        );

        assert_eq!(
            paths,
            vec![
                crate::services::thumbnail_service::image_preview_path_for(
                    HASH,
                    crate::services::thumbnail_service::IMAGES_THUMBNAIL_PROCESSOR_NAMESPACE,
                    crate::services::thumbnail_service::CURRENT_IMAGE_PREVIEW_VERSION,
                ),
                crate::services::thumbnail_service::image_preview_path_for(
                    HASH,
                    VIPS_CLI_THUMBNAIL_PROCESSOR_NAMESPACE,
                    crate::services::thumbnail_service::CURRENT_IMAGE_PREVIEW_VERSION,
                ),
                crate::services::thumbnail_service::image_preview_path_for(
                    HASH,
                    FFMPEG_CLI_THUMBNAIL_PROCESSOR_NAMESPACE,
                    crate::services::thumbnail_service::CURRENT_IMAGE_PREVIEW_VERSION,
                ),
                crate::services::thumbnail_service::image_preview_path_for(
                    HASH,
                    STORAGE_NATIVE_THUMBNAIL_PROCESSOR_NAMESPACE,
                    crate::services::thumbnail_service::CURRENT_IMAGE_PREVIEW_VERSION,
                ),
            ]
        );
    }

    #[test]
    fn known_image_preview_cache_paths_include_configured_dimension_namespace() {
        let paths = known_image_preview_cache_paths(HASH, 2048);

        assert_eq!(
            paths,
            vec![
                crate::services::thumbnail_service::image_preview_path_for(
                    HASH,
                    crate::services::thumbnail_service::IMAGES_THUMBNAIL_PROCESSOR_NAMESPACE,
                    crate::services::thumbnail_service::CURRENT_IMAGE_PREVIEW_VERSION,
                ),
                crate::services::thumbnail_service::image_preview_path_for(
                    HASH,
                    VIPS_CLI_THUMBNAIL_PROCESSOR_NAMESPACE,
                    crate::services::thumbnail_service::CURRENT_IMAGE_PREVIEW_VERSION,
                ),
                crate::services::thumbnail_service::image_preview_path_for(
                    HASH,
                    FFMPEG_CLI_THUMBNAIL_PROCESSOR_NAMESPACE,
                    crate::services::thumbnail_service::CURRENT_IMAGE_PREVIEW_VERSION,
                ),
                crate::services::thumbnail_service::image_preview_path_for(
                    HASH,
                    STORAGE_NATIVE_THUMBNAIL_PROCESSOR_NAMESPACE,
                    crate::services::thumbnail_service::CURRENT_IMAGE_PREVIEW_VERSION,
                ),
                crate::services::thumbnail_service::image_preview_path_for(
                    HASH,
                    crate::services::thumbnail_service::IMAGES_THUMBNAIL_PROCESSOR_NAMESPACE,
                    "1-d2048",
                ),
                crate::services::thumbnail_service::image_preview_path_for(
                    HASH,
                    VIPS_CLI_THUMBNAIL_PROCESSOR_NAMESPACE,
                    "1-d2048",
                ),
                crate::services::thumbnail_service::image_preview_path_for(
                    HASH,
                    FFMPEG_CLI_THUMBNAIL_PROCESSOR_NAMESPACE,
                    "1-d2048",
                ),
                crate::services::thumbnail_service::image_preview_path_for(
                    HASH,
                    STORAGE_NATIVE_THUMBNAIL_PROCESSOR_NAMESPACE,
                    "1-d2048",
                ),
            ]
        );
    }

    #[test]
    fn resolved_processor_versions_and_paths_use_runtime_dimensions() {
        let runtime_config = RuntimeConfig::new();
        let processor = ResolvedMediaProcessor::new(MediaProcessorKind::Images);

        assert_eq!(processor.thumbnail_version(&runtime_config), "1");
        assert_eq!(
            processor.cache_path(HASH, &runtime_config),
            crate::services::thumbnail_service::thumb_path_for(HASH, "images", "1")
        );
        assert_eq!(processor.image_preview_version(&runtime_config), "1");
        assert_eq!(
            processor.image_preview_cache_path(HASH, &runtime_config),
            crate::services::thumbnail_service::image_preview_path_for(HASH, "images", "1")
        );

        runtime_config.apply(config_model(THUMBNAIL_MAX_DIMENSION_KEY, "320"));
        runtime_config.apply(config_model(IMAGE_PREVIEW_MAX_DIMENSION_KEY, "2048"));

        assert_eq!(processor.thumbnail_version(&runtime_config), "1-d320");
        assert_eq!(
            processor.cache_path(HASH, &runtime_config),
            crate::services::thumbnail_service::thumb_path_for(HASH, "images", "1-d320")
        );
        assert_eq!(processor.image_preview_version(&runtime_config), "1-d2048");
        assert_eq!(
            processor.image_preview_cache_path(HASH, &runtime_config),
            crate::services::thumbnail_service::image_preview_path_for(HASH, "images", "1-d2048")
        );
    }

    #[test]
    fn spawn_pipe_reader_rejects_output_over_limit() {
        let data = vec![0_u8; MAX_CLI_OUTPUT_BYTES + 1];
        let result = join_pipe_reader(Some(spawn_pipe_reader(Cursor::new(data))));

        assert!(result.is_err());
    }
}

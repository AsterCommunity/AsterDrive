//! Blob-level media metadata extraction and cache orchestration.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use crate::config::{media_processing, operations};
use crate::db::repository::{file_repo, media_metadata_repo};
use crate::entities::{blob_media_metadata, file, file_blob};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::PrimaryAppState;
use crate::services::workspace_storage_service::WorkspaceStorageScope;
use crate::storage::StorageDriver;
use crate::types::{
    FileCategory, MediaMetadataKind, MediaMetadataPayload, MediaMetadataStatus,
    StoredMediaMetadataPayload, VideoMediaMetadata,
};
use crate::utils::raii::TempFileGuard;

mod audio;
mod image;
mod video;

use audio::parse_audio_metadata_from_path;
use image::parse_image_metadata_from_path;
use video::parse_video_metadata_from_path;

pub use video::probe_ffprobe_cli_command;

const PARSER_VERSION: &str = "1";
const IMAGE_PARSER_NAME: &str = "image";
const AUDIO_PARSER_NAME: &str = "lofty";
const VIDEO_PARSER_NAME: &str = "ffprobe";
const VIDEO_UNSUPPORTED_PARSER_NAME: &str = "unsupported";
const CACHE_ERROR_MAX_LEN: usize = 512;

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct MediaMetadataInfo {
    pub blob_id: i64,
    pub blob_hash: String,
    pub kind: MediaMetadataKind,
    pub status: MediaMetadataStatus,
    pub metadata: Option<MediaMetadataPayload>,
    pub error: Option<String>,
    pub parser: String,
    pub parser_version: String,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTime<Utc>,
}

pub enum MediaMetadataLookup {
    Ready(MediaMetadataInfo),
    Pending,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct MediaMetadataExtractTaskPayload {
    pub blob_id: i64,
    pub blob_hash: String,
    pub source_file_name: String,
    pub source_mime_type: String,
    pub kind: MediaMetadataKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct MediaMetadataExtractTaskResult {
    pub blob_id: i64,
    pub kind: MediaMetadataKind,
    pub status: MediaMetadataStatus,
    pub parser: String,
}

#[derive(Debug, Clone)]
pub struct ExtractedMediaMetadata {
    pub kind: MediaMetadataKind,
    pub status: MediaMetadataStatus,
    pub metadata: Option<MediaMetadataPayload>,
    pub error_message: Option<String>,
    pub parser: String,
    pub parser_version: String,
}

pub(crate) async fn get_for_file_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_id: i64,
) -> Result<MediaMetadataLookup> {
    let f = crate::services::file_service::get_info_in_scope(state, scope, file_id).await?;
    get_for_file(state, &f).await
}

pub async fn get_for_file(state: &PrimaryAppState, f: &file::Model) -> Result<MediaMetadataLookup> {
    if !operations::media_metadata_enabled(&state.runtime_config) {
        return Ok(MediaMetadataLookup::Ready(disabled_metadata_info(f)));
    }

    let Some(kind) = metadata_kind_for_file(f) else {
        let blob = file_repo::find_blob_by_id(&state.db, f.blob_id).await?;
        return Ok(MediaMetadataLookup::Ready(unsupported_file_metadata_info(
            &blob,
            f,
            "file type is not supported for media metadata",
        )));
    };

    let blob = file_repo::find_blob_by_id(&state.db, f.blob_id).await?;
    if media_metadata_processor_for_file_name(&state.runtime_config, kind, &f.name).is_none() {
        return Ok(MediaMetadataLookup::Ready(unsupported_kind_metadata_info(
            &blob,
            kind,
            format!(
                "no enabled {} media metadata processor matched '{}'",
                kind.as_str(),
                f.name
            ),
        )));
    }

    if let Some(cached) = media_metadata_repo::find_by_blob_id(&state.db, blob.id).await?
        && cached.blob_hash == blob.hash
        && cached.kind == kind
        && should_use_cached_metadata(state, f, &cached)
    {
        return Ok(MediaMetadataLookup::Ready(info_from_record(&cached)?));
    }

    crate::services::task_service::ensure_media_metadata_task(state, &blob, f, kind).await?;
    Ok(MediaMetadataLookup::Pending)
}

fn should_use_cached_metadata(
    state: &PrimaryAppState,
    f: &file::Model,
    record: &blob_media_metadata::Model,
) -> bool {
    if record.kind == MediaMetadataKind::Video
        && record.status == MediaMetadataStatus::Unsupported
        && record.parser == VIDEO_UNSUPPORTED_PARSER_NAME
        && let Some(processor) =
            media_metadata_processor_for_file_name(&state.runtime_config, record.kind, &f.name)
    {
        let command = processor
            .config
            .command
            .as_deref()
            .unwrap_or(media_processing::DEFAULT_FFPROBE_COMMAND);
        if media_processing::command_is_available(command) {
            return false;
        }
    }
    true
}

pub async fn extract_for_blob(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    source_file_name: &str,
    source_mime_type: &str,
    kind: MediaMetadataKind,
) -> Result<ExtractedMediaMetadata> {
    if media_metadata_processor_for_file_name(&state.runtime_config, kind, source_file_name)
        .is_none()
    {
        return Ok(unsupported_extract_result(
            kind,
            format!(
                "no enabled {} media metadata processor matched '{}'",
                kind.as_str(),
                source_file_name
            ),
        ));
    }

    match kind {
        MediaMetadataKind::Image => {
            extract_image_metadata(state, blob, source_file_name, source_mime_type).await
        }
        MediaMetadataKind::Audio => {
            extract_audio_metadata(state, blob, source_file_name, source_mime_type).await
        }
        MediaMetadataKind::Video => {
            extract_video_metadata(state, blob, source_file_name, source_mime_type).await
        }
    }
}

pub async fn persist_extracted(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    extracted: ExtractedMediaMetadata,
) -> Result<blob_media_metadata::Model> {
    let metadata_json = match extracted.metadata.as_ref() {
        Some(metadata) => Some(
            serde_json::to_string(metadata)
                .map(StoredMediaMetadataPayload)
                .map_aster_err_ctx(
                    "serialize media metadata payload",
                    AsterError::internal_error,
                )?,
        ),
        None => None,
    };

    media_metadata_repo::upsert_record(
        &state.db,
        media_metadata_repo::MediaMetadataRecordInput {
            blob_id: blob.id,
            blob_hash: &blob.hash,
            kind: extracted.kind,
            status: extracted.status,
            metadata_json: metadata_json.as_ref(),
            error_message: extracted.error_message.as_deref(),
            parser: &extracted.parser,
            parser_version: &extracted.parser_version,
            now: Utc::now(),
        },
    )
    .await
}

pub fn metadata_kind_for_file(f: &file::Model) -> Option<MediaMetadataKind> {
    match f.file_category {
        FileCategory::Image => Some(MediaMetadataKind::Image),
        FileCategory::Audio => Some(MediaMetadataKind::Audio),
        FileCategory::Video => Some(MediaMetadataKind::Video),
        _ => match f.mime_type.split_once('/') {
            Some(("image", _)) => Some(MediaMetadataKind::Image),
            Some(("audio", _)) => Some(MediaMetadataKind::Audio),
            Some(("video", _)) => Some(MediaMetadataKind::Video),
            _ => None,
        },
    }
}

fn media_metadata_use_for_kind(kind: MediaMetadataKind) -> media_processing::MediaProcessingUse {
    match kind {
        MediaMetadataKind::Image => media_processing::MediaProcessingUse::MetadataImage,
        MediaMetadataKind::Audio => media_processing::MediaProcessingUse::MetadataAudio,
        MediaMetadataKind::Video => media_processing::MediaProcessingUse::MetadataVideo,
    }
}

fn media_metadata_processor_for_file_name(
    runtime_config: &crate::config::RuntimeConfig,
    kind: MediaMetadataKind,
    file_name: &str,
) -> Option<media_processing::MediaProcessingProcessorConfig> {
    let registry = media_processing::media_processing_registry(runtime_config);
    media_processing::processor_candidates_for_use(
        &registry,
        media_metadata_use_for_kind(kind),
        file_name,
    )
    .into_iter()
    .next()
    .map(|candidate| candidate.processor)
}

fn info_from_record(record: &blob_media_metadata::Model) -> Result<MediaMetadataInfo> {
    Ok(MediaMetadataInfo {
        blob_id: record.blob_id,
        blob_hash: record.blob_hash.clone(),
        kind: record.kind,
        status: record.status,
        metadata: match record.metadata_json.as_ref() {
            Some(raw) => {
                Some(serde_json::from_str(raw.as_ref()).map_aster_err_ctx(
                    "parse media metadata payload",
                    AsterError::internal_error,
                )?)
            }
            None => None,
        },
        error: record.error_message.clone(),
        parser: record.parser.clone(),
        parser_version: record.parser_version.clone(),
        updated_at: record.updated_at,
    })
}

fn disabled_metadata_info(f: &file::Model) -> MediaMetadataInfo {
    MediaMetadataInfo {
        blob_id: f.blob_id,
        blob_hash: String::new(),
        kind: metadata_kind_for_file(f).unwrap_or(MediaMetadataKind::Image),
        status: MediaMetadataStatus::Unsupported,
        metadata: None,
        error: Some("media metadata extraction is disabled".to_string()),
        parser: "disabled".to_string(),
        parser_version: PARSER_VERSION.to_string(),
        updated_at: Utc::now(),
    }
}

fn unsupported_kind_metadata_info(
    blob: &file_blob::Model,
    kind: MediaMetadataKind,
    message: impl Into<String>,
) -> MediaMetadataInfo {
    MediaMetadataInfo {
        blob_id: blob.id,
        blob_hash: blob.hash.clone(),
        kind,
        status: MediaMetadataStatus::Unsupported,
        metadata: None,
        error: Some(message.into()),
        parser: VIDEO_UNSUPPORTED_PARSER_NAME.to_string(),
        parser_version: PARSER_VERSION.to_string(),
        updated_at: Utc::now(),
    }
}

fn unsupported_file_metadata_info(
    blob: &file_blob::Model,
    f: &file::Model,
    message: &str,
) -> MediaMetadataInfo {
    MediaMetadataInfo {
        blob_id: blob.id,
        blob_hash: blob.hash.clone(),
        kind: metadata_kind_for_file(f).unwrap_or(MediaMetadataKind::Image),
        status: MediaMetadataStatus::Unsupported,
        metadata: None,
        error: Some(message.to_string()),
        parser: VIDEO_UNSUPPORTED_PARSER_NAME.to_string(),
        parser_version: PARSER_VERSION.to_string(),
        updated_at: Utc::now(),
    }
}

fn unsupported_extract_result(
    kind: MediaMetadataKind,
    message: impl Into<String>,
) -> ExtractedMediaMetadata {
    ExtractedMediaMetadata {
        kind,
        status: MediaMetadataStatus::Unsupported,
        metadata: None,
        error_message: Some(message.into()),
        parser: VIDEO_UNSUPPORTED_PARSER_NAME.to_string(),
        parser_version: PARSER_VERSION.to_string(),
    }
}

fn unsupported_video_result() -> ExtractedMediaMetadata {
    ExtractedMediaMetadata {
        kind: MediaMetadataKind::Video,
        status: MediaMetadataStatus::Unsupported,
        metadata: Some(MediaMetadataPayload::Video(VideoMediaMetadata::default())),
        error_message: Some(
            "video metadata extraction is not available without a video probe".to_string(),
        ),
        parser: VIDEO_UNSUPPORTED_PARSER_NAME.to_string(),
        parser_version: PARSER_VERSION.to_string(),
    }
}

async fn extract_video_metadata(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    source_file_name: &str,
    source_mime_type: &str,
) -> Result<ExtractedMediaMetadata> {
    ensure_media_metadata_source_size_supported(state, blob)?;
    let Some(processor) = media_metadata_processor_for_file_name(
        &state.runtime_config,
        MediaMetadataKind::Video,
        source_file_name,
    ) else {
        return Ok(unsupported_video_result());
    };
    let command = processor
        .config
        .command
        .as_deref()
        .unwrap_or(media_processing::DEFAULT_FFPROBE_COMMAND)
        .to_string();
    if !media_processing::command_is_available(&command) {
        return Ok(unsupported_video_result());
    }

    let source =
        prepare_media_metadata_source(state, blob, source_file_name, source_mime_type).await?;
    let path = source.path().to_path_buf();
    let video_metadata =
        tokio::task::spawn_blocking(move || parse_video_metadata_from_path(&command, &path))
            .await
            .map_aster_err_ctx("video metadata task panicked", AsterError::internal_error)??;

    Ok(ExtractedMediaMetadata {
        kind: MediaMetadataKind::Video,
        status: MediaMetadataStatus::Ready,
        metadata: Some(MediaMetadataPayload::Video(video_metadata)),
        error_message: None,
        parser: VIDEO_PARSER_NAME.to_string(),
        parser_version: PARSER_VERSION.to_string(),
    })
}

async fn extract_image_metadata(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    source_file_name: &str,
    source_mime_type: &str,
) -> Result<ExtractedMediaMetadata> {
    ensure_media_metadata_source_size_supported(state, blob)?;
    let source =
        prepare_media_metadata_source(state, blob, source_file_name, source_mime_type).await?;
    let path = source.path().to_path_buf();
    let image_metadata = tokio::task::spawn_blocking(move || parse_image_metadata_from_path(&path))
        .await
        .map_aster_err_ctx("image metadata task panicked", AsterError::internal_error)??;

    Ok(ExtractedMediaMetadata {
        kind: MediaMetadataKind::Image,
        status: MediaMetadataStatus::Ready,
        metadata: Some(MediaMetadataPayload::Image(image_metadata)),
        error_message: None,
        parser: IMAGE_PARSER_NAME.to_string(),
        parser_version: PARSER_VERSION.to_string(),
    })
}

async fn extract_audio_metadata(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    source_file_name: &str,
    source_mime_type: &str,
) -> Result<ExtractedMediaMetadata> {
    ensure_media_metadata_source_size_supported(state, blob)?;
    let source =
        prepare_media_metadata_source(state, blob, source_file_name, source_mime_type).await?;
    let path = source.path().to_path_buf();
    let audio_metadata = tokio::task::spawn_blocking(move || parse_audio_metadata_from_path(&path))
        .await
        .map_aster_err_ctx("audio metadata task panicked", AsterError::internal_error)??;

    Ok(ExtractedMediaMetadata {
        kind: MediaMetadataKind::Audio,
        status: MediaMetadataStatus::Ready,
        metadata: Some(MediaMetadataPayload::Audio(audio_metadata)),
        error_message: None,
        parser: AUDIO_PARSER_NAME.to_string(),
        parser_version: PARSER_VERSION.to_string(),
    })
}

fn ensure_media_metadata_source_size_supported(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
) -> Result<()> {
    let max_source_bytes = operations::media_metadata_max_source_bytes(&state.runtime_config);
    if blob.size > max_source_bytes {
        return Err(AsterError::validation_error(format!(
            "media metadata source exceeds {} MiB limit",
            max_source_bytes / 1024 / 1024
        )));
    }
    Ok(())
}

async fn prepare_media_metadata_source(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    source_file_name: &str,
    source_mime_type: &str,
) -> Result<PreparedMediaMetadataSource> {
    let policy = state.policy_snapshot.get_policy_or_err(blob.policy_id)?;
    let driver = state.driver_registry.get_driver(&policy)?;

    if let Some(local_path_driver) = driver.as_local_path() {
        return Ok(PreparedMediaMetadataSource::Local(
            local_path_driver.resolve_local_path(&blob.storage_path)?,
        ));
    }

    let temp_source = stream_blob_to_temp_source(
        driver,
        blob,
        &state.config.server.temp_dir,
        source_file_name,
        source_mime_type,
    )
    .await?;
    Ok(PreparedMediaMetadataSource::Temp(temp_source))
}

async fn stream_blob_to_temp_source(
    driver: Arc<dyn StorageDriver>,
    blob: &file_blob::Model,
    temp_root: &str,
    source_file_name: &str,
    source_mime_type: &str,
) -> Result<TempFileGuard> {
    let temp_dir = PathBuf::from(crate::utils::paths::runtime_temp_dir(temp_root));
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .map_aster_err_ctx(
            "create media metadata temp dir",
            AsterError::storage_driver_error,
        )?;
    let extension = media_metadata_source_extension(source_file_name, source_mime_type);
    let temp_source = TempFileGuard::new(
        temp_dir.join(format!(
            "media-metadata-source-{}.{}",
            uuid::Uuid::new_v4(),
            extension
        )),
        "media metadata source temp file",
    );

    let mut stream = driver.get_stream(&blob.storage_path).await?;
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp_source.path())
        .await
        .map_aster_err_ctx(
            "create media metadata source temp file",
            AsterError::storage_driver_error,
        )?;
    let copied = tokio::io::copy(&mut stream, &mut file)
        .await
        .map_aster_err_ctx(
            "copy media metadata source stream",
            AsterError::storage_driver_error,
        )?;
    file.flush().await.map_aster_err_ctx(
        "flush media metadata source temp file",
        AsterError::storage_driver_error,
    )?;
    drop(file);

    let expected_size = crate::utils::numbers::i64_to_u64(blob.size, "media metadata source size")?;
    if copied != expected_size {
        return Err(AsterError::storage_driver_error(format!(
            "media metadata source stream size mismatch: expected {expected_size} bytes, got {copied}"
        )));
    }

    Ok(temp_source)
}

fn media_metadata_source_extension(source_file_name: &str, source_mime_type: &str) -> String {
    std::path::Path::new(source_file_name)
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.to_ascii_lowercase())
        .or_else(|| {
            mime_guess::get_mime_extensions_str(source_mime_type)
                .and_then(|extensions| extensions.first().copied())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "bin".to_string())
}

enum PreparedMediaMetadataSource {
    Local(PathBuf),
    Temp(TempFileGuard),
}

impl PreparedMediaMetadataSource {
    fn path(&self) -> &Path {
        match self {
            Self::Local(path) => path.as_path(),
            Self::Temp(guard) => guard.path(),
        }
    }
}

pub fn result_status_text(status: MediaMetadataStatus) -> &'static str {
    match status {
        MediaMetadataStatus::Ready => "Media metadata ready",
        MediaMetadataStatus::Failed => "Media metadata failed",
        MediaMetadataStatus::Unsupported => "Media metadata unsupported",
    }
}

pub fn task_display_name(blob_id: i64, kind: MediaMetadataKind) -> String {
    format!("Extract {} metadata for blob #{blob_id}", kind.as_str())
}

pub fn cache_error_message(error: &AsterError) -> String {
    crate::utils::truncate_utf8_to_max_bytes(error.message(), CACHE_ERROR_MAX_LEN)
}

use std::collections::{BTreeSet, HashSet};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
#[cfg(windows)]
use std::path::PathBuf;

use crate::config::{RuntimeConfig, operations};
use crate::errors::{AsterError, Result};
use crate::types::{MediaMetadataKind, MediaProcessorKind};

use super::types::{
    BUILTIN_AUDIO_METADATA_EXTENSIONS, BUILTIN_IMAGE_METADATA_EXTENSIONS,
    BUILTIN_IMAGES_SUPPORTED_EXTENSIONS, DEFAULT_FFMPEG_COMMAND, DEFAULT_FFMPEG_EXTENSIONS,
    DEFAULT_FFPROBE_COMMAND, DEFAULT_FFPROBE_EXTENSIONS, DEFAULT_VIPS_COMMAND,
    DEFAULT_VIPS_EXTENSIONS, MEDIA_PROCESSING_REGISTRY_VERSION, MatchedMediaProcessor,
    MediaProcessingMatchKind, MediaProcessingProcessorConfig,
    MediaProcessingProcessorRuntimeConfig, MediaProcessingRegistryConfig, MediaProcessingUse,
    PUBLIC_MEDIA_DATA_MAX_SAFE_SOURCE_BYTES, PUBLIC_MEDIA_DATA_SUPPORT_VERSION,
    PUBLIC_THUMBNAIL_SUPPORT_VERSION, PublicMediaDataKindSupport, PublicMediaDataKindsSupport,
    PublicMediaDataSupport, PublicMediaDataSupportMatch, PublicThumbnailSupport,
};
use crate::config::definitions::MEDIA_PROCESSING_REGISTRY_JSON_KEY;

const MEDIA_PROCESSOR_PRIORITY: [MediaProcessorKind; 5] = [
    MediaProcessorKind::VipsCli,
    MediaProcessorKind::FfmpegCli,
    MediaProcessorKind::FfprobeCli,
    MediaProcessorKind::Lofty,
    MediaProcessorKind::Images,
];

const THUMBNAIL_PROCESSOR_PRIORITY: [MediaProcessorKind; 4] = [
    MediaProcessorKind::VipsCli,
    MediaProcessorKind::FfmpegCli,
    MediaProcessorKind::Lofty,
    MediaProcessorKind::Images,
];

const METADATA_IMAGE_PROCESSOR_PRIORITY: [MediaProcessorKind; 1] = [MediaProcessorKind::Images];
const METADATA_AUDIO_PROCESSOR_PRIORITY: [MediaProcessorKind; 1] = [MediaProcessorKind::Lofty];
const METADATA_VIDEO_PROCESSOR_PRIORITY: [MediaProcessorKind; 1] = [MediaProcessorKind::FfprobeCli];

pub fn parse_media_processor_kind(value: &str) -> Option<MediaProcessorKind> {
    match value.trim().to_ascii_lowercase().as_str() {
        "images" => Some(MediaProcessorKind::Images),
        "lofty" => Some(MediaProcessorKind::Lofty),
        "vips_cli" => Some(MediaProcessorKind::VipsCli),
        "ffmpeg_cli" => Some(MediaProcessorKind::FfmpegCli),
        "ffprobe_cli" => Some(MediaProcessorKind::FfprobeCli),
        "storage_native" => Some(MediaProcessorKind::StorageNative),
        _ => None,
    }
}

pub fn normalize_vips_command(value: &str) -> Result<String> {
    normalize_processor_command(value, DEFAULT_VIPS_COMMAND, "vips command")
}

pub fn normalize_ffmpeg_command(value: &str) -> Result<String> {
    normalize_processor_command(value, DEFAULT_FFMPEG_COMMAND, "ffmpeg command")
}

pub fn normalize_ffprobe_command(value: &str) -> Result<String> {
    normalize_processor_command(value, DEFAULT_FFPROBE_COMMAND, "ffprobe command")
}

fn normalize_processor_command(value: &str, default_command: &str, label: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(default_command.to_string());
    }
    if trimmed.chars().any(|ch| ch.is_control()) {
        return Err(AsterError::validation_error(format!(
            "{label} cannot contain control characters"
        )));
    }
    Ok(trimmed.to_string())
}

pub fn builtin_images_supports_extension(extension: &str) -> bool {
    BUILTIN_IMAGES_SUPPORTED_EXTENSIONS.contains(&extension)
}

pub fn builtin_image_metadata_supports_extension(extension: &str) -> bool {
    BUILTIN_IMAGE_METADATA_EXTENSIONS.contains(&extension)
}

pub fn builtin_audio_metadata_supports_extension(extension: &str) -> bool {
    BUILTIN_AUDIO_METADATA_EXTENSIONS.contains(&extension)
}

pub fn vips_command_from_registry_value(value: &str) -> Result<String> {
    let config = parse_media_processing_registry_config(value)?;
    let command = processor_config_for_kind(&config, MediaProcessorKind::VipsCli)
        .and_then(|processor| processor.config.command.as_deref())
        .unwrap_or(DEFAULT_VIPS_COMMAND);
    normalize_vips_command(command)
}

pub fn ffmpeg_command_from_registry_value(value: &str) -> Result<String> {
    let config = parse_media_processing_registry_config(value)?;
    let command = processor_config_for_kind(&config, MediaProcessorKind::FfmpegCli)
        .and_then(|processor| processor.config.command.as_deref())
        .unwrap_or(DEFAULT_FFMPEG_COMMAND);
    normalize_ffmpeg_command(command)
}

pub fn ffprobe_command_from_registry_value(value: &str) -> Result<String> {
    let config = parse_media_processing_registry_config(value)?;
    ffprobe_command_from_registry(&config)
}

pub fn ffprobe_command_from_registry(config: &MediaProcessingRegistryConfig) -> Result<String> {
    let command = processor_config_for_kind(config, MediaProcessorKind::FfprobeCli)
        .and_then(|processor| processor.config.command.as_deref())
        .unwrap_or(DEFAULT_FFPROBE_COMMAND);
    normalize_ffprobe_command(command)
}

pub fn public_thumbnail_support(runtime_config: &RuntimeConfig) -> PublicThumbnailSupport {
    let registry = media_processing_registry(runtime_config);
    let mut extensions = BTreeSet::new();

    for processor in registry
        .processors
        .iter()
        .filter(|processor| processor.enabled)
    {
        match processor.kind {
            MediaProcessorKind::Images
                if processor_supports_use(processor, MediaProcessingUse::ThumbnailImage) =>
            {
                extensions.extend(
                    BUILTIN_IMAGES_SUPPORTED_EXTENSIONS
                        .iter()
                        .map(|extension| (*extension).to_string()),
                );
            }
            MediaProcessorKind::VipsCli
                if processor_supports_use(processor, MediaProcessingUse::ThumbnailImage) =>
            {
                let command = processor
                    .config
                    .command
                    .as_deref()
                    .unwrap_or(DEFAULT_VIPS_COMMAND);
                if command_is_available(command) {
                    extensions.extend(processor.extensions.iter().cloned());
                }
            }
            MediaProcessorKind::FfmpegCli
                if processor_supports_use(processor, MediaProcessingUse::ThumbnailVideo) =>
            {
                let command = processor
                    .config
                    .command
                    .as_deref()
                    .unwrap_or(DEFAULT_FFMPEG_COMMAND);
                if command_is_available(command) {
                    extensions.extend(processor.extensions.iter().cloned());
                }
            }
            MediaProcessorKind::Lofty
                if processor_supports_use(processor, MediaProcessingUse::ThumbnailAudio) =>
            {
                extensions.extend(
                    BUILTIN_AUDIO_METADATA_EXTENSIONS
                        .iter()
                        .map(|extension| (*extension).to_string()),
                );
            }
            MediaProcessorKind::Images
            | MediaProcessorKind::VipsCli
            | MediaProcessorKind::FfmpegCli
            | MediaProcessorKind::FfprobeCli
            | MediaProcessorKind::Lofty
            | MediaProcessorKind::StorageNative => {}
        }
    }

    PublicThumbnailSupport {
        version: PUBLIC_THUMBNAIL_SUPPORT_VERSION,
        extensions: extensions.into_iter().collect(),
    }
}

pub fn public_media_data_support(runtime_config: &RuntimeConfig) -> PublicMediaDataSupport {
    let registry = media_processing_registry(runtime_config);
    let enabled = operations::media_metadata_enabled(runtime_config);

    PublicMediaDataSupport {
        version: PUBLIC_MEDIA_DATA_SUPPORT_VERSION,
        enabled,
        max_source_bytes: operations::media_metadata_max_source_bytes(runtime_config)
            .min(PUBLIC_MEDIA_DATA_MAX_SAFE_SOURCE_BYTES),
        kinds: PublicMediaDataKindsSupport {
            image: public_media_data_kind_support(&registry, enabled, MediaMetadataKind::Image),
            audio: public_media_data_kind_support(&registry, enabled, MediaMetadataKind::Audio),
            video: public_media_data_kind_support(&registry, enabled, MediaMetadataKind::Video),
        },
    }
}

fn public_media_data_kind_support(
    registry: &MediaProcessingRegistryConfig,
    media_enabled: bool,
    kind: MediaMetadataKind,
) -> PublicMediaDataKindSupport {
    if !media_enabled {
        return disabled_media_data_kind_support();
    }

    match kind {
        MediaMetadataKind::Image => builtin_media_data_kind_support(
            registry,
            MediaProcessorKind::Images,
            MediaProcessingUse::MetadataImage,
            BUILTIN_IMAGE_METADATA_EXTENSIONS.iter().copied(),
        ),
        MediaMetadataKind::Audio => builtin_media_data_kind_support(
            registry,
            MediaProcessorKind::Lofty,
            MediaProcessingUse::MetadataAudio,
            BUILTIN_AUDIO_METADATA_EXTENSIONS.iter().copied(),
        ),
        MediaMetadataKind::Video => ffprobe_media_data_kind_support(registry),
    }
}

fn builtin_media_data_kind_support(
    registry: &MediaProcessingRegistryConfig,
    processor_kind: MediaProcessorKind,
    media_use: MediaProcessingUse,
    extensions: impl IntoIterator<Item = &'static str>,
) -> PublicMediaDataKindSupport {
    let Some(processor) = processor_config_for_kind(registry, processor_kind) else {
        return disabled_media_data_kind_support();
    };
    if !processor.enabled || !processor_supports_use(processor, media_use) {
        return disabled_media_data_kind_support();
    }

    PublicMediaDataKindSupport {
        enabled: true,
        match_kind: PublicMediaDataSupportMatch::Extensions,
        extensions: sorted_extensions(extensions),
    }
}

fn ffprobe_media_data_kind_support(
    registry: &MediaProcessingRegistryConfig,
) -> PublicMediaDataKindSupport {
    let Some(processor) = processor_config_for_kind(registry, MediaProcessorKind::FfprobeCli)
    else {
        return disabled_media_data_kind_support();
    };
    if !processor.enabled || !processor_supports_use(processor, MediaProcessingUse::MetadataVideo) {
        return disabled_media_data_kind_support();
    }

    let command = processor
        .config
        .command
        .as_deref()
        .unwrap_or(DEFAULT_FFPROBE_COMMAND);
    if !command_is_available(command) {
        return disabled_media_data_kind_support();
    }

    PublicMediaDataKindSupport {
        enabled: true,
        match_kind: if processor.extensions.is_empty() {
            PublicMediaDataSupportMatch::Any
        } else {
            PublicMediaDataSupportMatch::Extensions
        },
        extensions: sorted_extensions(processor.extensions.iter().map(String::as_str)),
    }
}

fn disabled_media_data_kind_support() -> PublicMediaDataKindSupport {
    PublicMediaDataKindSupport {
        enabled: false,
        match_kind: PublicMediaDataSupportMatch::Extensions,
        extensions: Vec::new(),
    }
}

fn sorted_extensions<'a>(extensions: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    extensions
        .into_iter()
        .map(str::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub fn default_media_processing_registry() -> MediaProcessingRegistryConfig {
    MediaProcessingRegistryConfig {
        version: MEDIA_PROCESSING_REGISTRY_VERSION,
        processors: MEDIA_PROCESSOR_PRIORITY
            .into_iter()
            .map(default_processor_config_for_kind)
            .collect(),
    }
}

pub fn default_media_processing_registry_json() -> String {
    serde_json::to_string_pretty(&default_media_processing_registry())
        .expect("default media processing registry should serialize")
}

pub fn normalize_media_processing_registry_config_value(value: &str) -> Result<String> {
    normalize_media_processing_registry_config_value_with_command_validation(value, true)
}

fn normalize_media_processing_registry_config_value_with_command_validation(
    value: &str,
    validate_runtime_commands: bool,
) -> Result<String> {
    let mut config: MediaProcessingRegistryConfig =
        serde_json::from_str(value).map_err(|error| {
            AsterError::validation_error(format!(
                "media processing config must be valid JSON: {error}",
            ))
        })?;
    if config.version == 1 {
        config.version = MEDIA_PROCESSING_REGISTRY_VERSION;
    }
    validate_media_processing_registry_config(&mut config, validate_runtime_commands)?;
    serde_json::to_string_pretty(&config).map_err(|error| {
        AsterError::internal_error(format!(
            "failed to serialize normalized media processing config: {error}",
        ))
    })
}

pub fn media_processing_registry(runtime_config: &RuntimeConfig) -> MediaProcessingRegistryConfig {
    let Some(raw) = runtime_config.get(MEDIA_PROCESSING_REGISTRY_JSON_KEY) else {
        return default_media_processing_registry();
    };

    match parse_media_processing_registry_config(&raw) {
        Ok(config) => config,
        Err(error) => {
            tracing::warn!("failed to parse media processing config: {error}");
            default_media_processing_registry()
        }
    }
}

pub fn normalize_existing_media_processing_registry_config_value(value: &str) -> Result<String> {
    normalize_media_processing_registry_config_value_with_command_validation(value, false)
}

pub fn processor_candidates_for_file_name(
    config: &MediaProcessingRegistryConfig,
    file_name: &str,
) -> Vec<MatchedMediaProcessor> {
    processor_candidates_for_use(config, MediaProcessingUse::ThumbnailImage, file_name)
        .into_iter()
        .chain(processor_candidates_for_use(
            config,
            MediaProcessingUse::ThumbnailAudio,
            file_name,
        ))
        .chain(processor_candidates_for_use(
            config,
            MediaProcessingUse::ThumbnailVideo,
            file_name,
        ))
        .collect()
}

pub fn processor_candidates_for_use(
    config: &MediaProcessingRegistryConfig,
    media_use: MediaProcessingUse,
    file_name: &str,
) -> Vec<MatchedMediaProcessor> {
    let extension = file_extension(file_name);
    let mut matched = Vec::new();

    for kind in processor_priority_for_use(media_use) {
        let Some(processor) = processor_config_for_kind(config, *kind) else {
            continue;
        };
        if !processor.enabled {
            continue;
        }
        if !processor_supports_use(processor, media_use) {
            continue;
        }

        if processor.kind == MediaProcessorKind::Images {
            let Some(extension) = extension.as_deref() else {
                continue;
            };
            let supported = match media_use {
                MediaProcessingUse::ThumbnailImage => builtin_images_supports_extension(extension),
                MediaProcessingUse::MetadataImage => {
                    builtin_image_metadata_supports_extension(extension)
                }
                MediaProcessingUse::ThumbnailAudio
                | MediaProcessingUse::ThumbnailVideo
                | MediaProcessingUse::MetadataAudio
                | MediaProcessingUse::MetadataVideo => false,
            };
            if supported {
                matched.push(MatchedMediaProcessor {
                    processor: processor.clone(),
                    match_kind: MediaProcessingMatchKind::Extension,
                });
            }
            continue;
        }

        if processor.kind == MediaProcessorKind::Lofty
            && media_use == MediaProcessingUse::MetadataAudio
        {
            let Some(extension) = extension.as_deref() else {
                continue;
            };
            if builtin_audio_metadata_supports_extension(extension) {
                matched.push(MatchedMediaProcessor {
                    processor: processor.clone(),
                    match_kind: MediaProcessingMatchKind::Extension,
                });
            }
            continue;
        }

        if processor.extensions.is_empty() {
            matched.push(MatchedMediaProcessor {
                processor: processor.clone(),
                match_kind: MediaProcessingMatchKind::Any,
            });
            continue;
        }

        let Some(extension) = extension.as_deref() else {
            continue;
        };
        if processor
            .extensions
            .iter()
            .any(|candidate| candidate == extension)
        {
            matched.push(MatchedMediaProcessor {
                processor: processor.clone(),
                match_kind: MediaProcessingMatchKind::Extension,
            });
        }
    }

    matched
}

pub fn processor_supports_use(
    processor: &MediaProcessingProcessorConfig,
    media_use: MediaProcessingUse,
) -> bool {
    processor.uses.contains(&media_use)
}

fn processor_priority_for_use(media_use: MediaProcessingUse) -> &'static [MediaProcessorKind] {
    match media_use {
        MediaProcessingUse::ThumbnailImage
        | MediaProcessingUse::ThumbnailAudio
        | MediaProcessingUse::ThumbnailVideo => &THUMBNAIL_PROCESSOR_PRIORITY,
        MediaProcessingUse::MetadataImage => &METADATA_IMAGE_PROCESSOR_PRIORITY,
        MediaProcessingUse::MetadataAudio => &METADATA_AUDIO_PROCESSOR_PRIORITY,
        MediaProcessingUse::MetadataVideo => &METADATA_VIDEO_PROCESSOR_PRIORITY,
    }
}

pub fn file_extension(file_name: &str) -> Option<String> {
    Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.trim_start_matches('.').to_ascii_lowercase())
}

pub fn default_processor_config_for_kind(
    kind: MediaProcessorKind,
) -> MediaProcessingProcessorConfig {
    MediaProcessingProcessorConfig {
        kind,
        enabled: matches!(kind, MediaProcessorKind::Images | MediaProcessorKind::Lofty),
        uses: default_uses_for_kind(kind),
        extensions: match kind {
            MediaProcessorKind::VipsCli => DEFAULT_VIPS_EXTENSIONS
                .iter()
                .map(|extension| (*extension).to_string())
                .collect(),
            MediaProcessorKind::FfmpegCli => DEFAULT_FFMPEG_EXTENSIONS
                .iter()
                .map(|extension| (*extension).to_string())
                .collect(),
            MediaProcessorKind::FfprobeCli => DEFAULT_FFPROBE_EXTENSIONS
                .iter()
                .map(|extension| (*extension).to_string())
                .collect(),
            MediaProcessorKind::Lofty => BUILTIN_AUDIO_METADATA_EXTENSIONS
                .iter()
                .map(|extension| (*extension).to_string())
                .collect(),
            _ => Vec::new(),
        },
        config: match kind {
            MediaProcessorKind::VipsCli => MediaProcessingProcessorRuntimeConfig {
                command: Some(DEFAULT_VIPS_COMMAND.to_string()),
            },
            MediaProcessorKind::FfmpegCli => MediaProcessingProcessorRuntimeConfig {
                command: Some(DEFAULT_FFMPEG_COMMAND.to_string()),
            },
            MediaProcessorKind::FfprobeCli => MediaProcessingProcessorRuntimeConfig {
                command: Some(DEFAULT_FFPROBE_COMMAND.to_string()),
            },
            _ => MediaProcessingProcessorRuntimeConfig::default(),
        },
    }
}

pub fn default_uses_for_kind(kind: MediaProcessorKind) -> Vec<MediaProcessingUse> {
    match kind {
        MediaProcessorKind::Images => vec![
            MediaProcessingUse::ThumbnailImage,
            MediaProcessingUse::MetadataImage,
        ],
        MediaProcessorKind::Lofty => vec![
            MediaProcessingUse::ThumbnailAudio,
            MediaProcessingUse::MetadataAudio,
        ],
        MediaProcessorKind::VipsCli => vec![MediaProcessingUse::ThumbnailImage],
        MediaProcessorKind::FfmpegCli => vec![MediaProcessingUse::ThumbnailVideo],
        MediaProcessorKind::FfprobeCli => vec![MediaProcessingUse::MetadataVideo],
        MediaProcessorKind::StorageNative => Vec::new(),
    }
}

pub fn processor_config_for_kind(
    config: &MediaProcessingRegistryConfig,
    kind: MediaProcessorKind,
) -> Option<&MediaProcessingProcessorConfig> {
    config
        .processors
        .iter()
        .find(|processor| processor.kind == kind)
}

pub fn command_is_available(command: &str) -> bool {
    if command.trim().is_empty() {
        return false;
    }

    let command_path = Path::new(command);
    if command_path.is_absolute() || command.contains(std::path::MAIN_SEPARATOR) {
        return is_executable_file(command_path);
    }

    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let candidate = dir.join(command);
                is_executable_file(&candidate)
            })
        })
        .unwrap_or(false)
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(windows)]
fn is_executable_file(path: &Path) -> bool {
    windows_executable_candidates(path)
        .into_iter()
        .any(|candidate| candidate.is_file())
}

#[cfg(windows)]
fn windows_executable_candidates(path: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![path.to_path_buf()];
    if path.extension().is_some() {
        return candidates;
    }

    let base = path.as_os_str().to_os_string();
    for extension in windows_pathext_values() {
        let mut candidate = base.clone();
        candidate.push(extension);
        candidates.push(PathBuf::from(candidate));
    }
    candidates
}

#[cfg(windows)]
fn windows_pathext_values() -> Vec<String> {
    std::env::var_os("PATHEXT")
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".to_string())
        .split(';')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            if value.starts_with('.') {
                value.to_string()
            } else {
                format!(".{value}")
            }
        })
        .collect()
}

fn parse_media_processing_registry_config(value: &str) -> Result<MediaProcessingRegistryConfig> {
    parse_media_processing_registry_config_with_command_validation(value, false)
}

fn parse_media_processing_registry_config_with_command_validation(
    value: &str,
    validate_runtime_commands: bool,
) -> Result<MediaProcessingRegistryConfig> {
    let normalized = normalize_media_processing_registry_config_value_with_command_validation(
        value,
        validate_runtime_commands,
    )?;
    serde_json::from_str(&normalized).map_err(|error| {
        AsterError::internal_error(format!(
            "normalized media processing config failed to parse: {error}",
        ))
    })
}

fn validate_media_processing_registry_config(
    config: &mut MediaProcessingRegistryConfig,
    validate_runtime_commands: bool,
) -> Result<()> {
    if config.version != MEDIA_PROCESSING_REGISTRY_VERSION {
        return Err(AsterError::validation_error(format!(
            "media processing config version must be {MEDIA_PROCESSING_REGISTRY_VERSION}",
        )));
    }

    let mut normalized = Vec::with_capacity(config.processors.len());
    let mut seen_kinds = HashSet::new();
    for mut processor in std::mem::take(&mut config.processors) {
        match processor.kind {
            MediaProcessorKind::Images => {
                processor.extensions.clear();
                processor.config = MediaProcessingProcessorRuntimeConfig::default();
                normalize_uses_for_kind(&mut processor.uses, processor.kind)?;
            }
            MediaProcessorKind::Lofty => {
                normalize_uses_for_kind(&mut processor.uses, processor.kind)?;
                normalize_match_list(&mut processor.extensions)?;
                processor.config = MediaProcessingProcessorRuntimeConfig::default();
            }
            MediaProcessorKind::VipsCli => {
                normalize_uses_for_kind(&mut processor.uses, processor.kind)?;
                normalize_match_list(&mut processor.extensions)?;
                let command = processor
                    .config
                    .command
                    .as_deref()
                    .unwrap_or(DEFAULT_VIPS_COMMAND);
                let normalized_command = normalize_vips_command(command)?;
                if validate_runtime_commands
                    && processor.enabled
                    && !command_is_available(&normalized_command)
                {
                    return Err(AsterError::validation_error(format!(
                        "enabled vips_cli processor command '{normalized_command}' is not available",
                    )));
                }
                processor.config.command = Some(normalized_command);
            }
            MediaProcessorKind::FfmpegCli => {
                normalize_uses_for_kind(&mut processor.uses, processor.kind)?;
                normalize_match_list(&mut processor.extensions)?;
                let command = processor
                    .config
                    .command
                    .as_deref()
                    .unwrap_or(DEFAULT_FFMPEG_COMMAND);
                let normalized_command = normalize_ffmpeg_command(command)?;
                if validate_runtime_commands
                    && processor.enabled
                    && !command_is_available(&normalized_command)
                {
                    return Err(AsterError::validation_error(format!(
                        "enabled ffmpeg_cli processor command '{normalized_command}' is not available",
                    )));
                }
                processor.config.command = Some(normalized_command);
            }
            MediaProcessorKind::FfprobeCli => {
                normalize_uses_for_kind(&mut processor.uses, processor.kind)?;
                normalize_match_list(&mut processor.extensions)?;
                let command = processor
                    .config
                    .command
                    .as_deref()
                    .unwrap_or(DEFAULT_FFPROBE_COMMAND);
                let normalized_command = normalize_ffprobe_command(command)?;
                if validate_runtime_commands
                    && processor.enabled
                    && !command_is_available(&normalized_command)
                {
                    return Err(AsterError::validation_error(format!(
                        "enabled ffprobe_cli processor command '{normalized_command}' is not available",
                    )));
                }
                processor.config.command = Some(normalized_command);
            }
            MediaProcessorKind::StorageNative => {
                return Err(AsterError::validation_error(
                    "media processing config does not support 'storage_native'; use storage policy thumbnail options instead",
                ));
            }
        }

        let kind_key = processor.kind.as_str();
        if !seen_kinds.insert(kind_key) {
            return Err(AsterError::validation_error(format!(
                "duplicate media processing processor '{}'",
                kind_key
            )));
        }

        normalized.push(processor);
    }

    config.processors = MEDIA_PROCESSOR_PRIORITY
        .into_iter()
        .map(|kind| {
            normalized
                .iter()
                .find(|processor| processor.kind == kind)
                .cloned()
                .unwrap_or_else(|| default_processor_config_for_kind(kind))
        })
        .collect();

    if !config.processors.iter().any(|processor| processor.enabled) {
        return Err(AsterError::validation_error(
            "media processing config must enable at least one processor",
        ));
    }

    Ok(())
}

fn normalize_uses_for_kind(
    uses: &mut Vec<MediaProcessingUse>,
    kind: MediaProcessorKind,
) -> Result<()> {
    if uses.is_empty() {
        *uses = default_uses_for_kind(kind);
        return Ok(());
    }

    let supported = default_uses_for_kind(kind);
    let mut unique = Vec::new();
    for media_use in std::mem::take(uses) {
        if !supported.contains(&media_use) {
            return Err(AsterError::validation_error(format!(
                "processor '{}' does not support media use '{}'",
                kind.as_str(),
                media_use.as_str()
            )));
        }
        if !unique.contains(&media_use) {
            unique.push(media_use);
        }
    }
    for default_use in supported {
        if !unique.contains(&default_use) {
            unique.push(default_use);
        }
    }
    *uses = unique;
    Ok(())
}

fn normalize_match_list(items: &mut Vec<String>) -> Result<()> {
    let mut unique = BTreeSet::new();
    for item in std::mem::take(items) {
        unique.insert(normalize_extension(&item)?);
    }
    *items = unique.into_iter().collect();
    Ok(())
}

fn normalize_extension(value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AsterError::validation_error(
            "processor extension must not be empty",
        ));
    }
    Ok(trimmed.trim_start_matches('.').to_ascii_lowercase())
}

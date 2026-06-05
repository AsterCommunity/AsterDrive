//! 媒体处理相关 runtime config。

mod registry;
#[cfg(test)]
mod tests;
mod types;

pub use crate::config::definitions::MEDIA_PROCESSING_REGISTRY_JSON_KEY;
pub use registry::default_processor_config_for_kind;
pub use registry::{
    builtin_audio_metadata_supports_extension, builtin_image_metadata_supports_extension,
    builtin_images_supports_extension, command_is_available, default_media_processing_registry,
    default_media_processing_registry_json, default_uses_for_kind,
    ffmpeg_command_from_registry_value, ffprobe_command_from_registry,
    ffprobe_command_from_registry_value, file_extension, media_processing_registry,
    normalize_existing_media_processing_registry_config_value, normalize_ffmpeg_command,
    normalize_ffprobe_command, normalize_media_processing_registry_config_value,
    normalize_vips_command, parse_media_processor_kind, processor_candidates_for_file_name,
    processor_candidates_for_use, processor_config_for_kind, processor_supports_use,
    public_media_data_support, public_thumbnail_support, vips_command_from_registry_value,
};
pub use types::{
    BUILTIN_AUDIO_METADATA_EXTENSIONS, BUILTIN_AUDIO_THUMBNAIL_EXTENSIONS,
    BUILTIN_IMAGE_METADATA_EXTENSIONS, BUILTIN_IMAGES_SUPPORTED_EXTENSIONS, DEFAULT_FFMPEG_COMMAND,
    DEFAULT_FFMPEG_EXTENSIONS, DEFAULT_FFPROBE_COMMAND, DEFAULT_FFPROBE_EXTENSIONS,
    DEFAULT_VIPS_COMMAND, DEFAULT_VIPS_EXTENSIONS, MEDIA_PROCESSING_REGISTRY_VERSION,
    MatchedMediaProcessor, MediaProcessingMatchKind, MediaProcessingProcessorConfig,
    MediaProcessingProcessorRuntimeConfig, MediaProcessingRegistryConfig, MediaProcessingUse,
    PUBLIC_MEDIA_DATA_MAX_SAFE_SOURCE_BYTES, PUBLIC_MEDIA_DATA_SUPPORT_VERSION,
    PUBLIC_THUMBNAIL_SUPPORT_VERSION, PublicImagePreviewPreference, PublicMediaDataKindSupport,
    PublicMediaDataKindsSupport, PublicMediaDataSupport, PublicMediaDataSupportMatch,
    PublicThumbnailSupport,
};

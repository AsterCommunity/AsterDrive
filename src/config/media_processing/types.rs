use crate::types::MediaProcessorKind;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

pub const MEDIA_PROCESSING_REGISTRY_VERSION: i32 = 2;
pub const PUBLIC_THUMBNAIL_SUPPORT_VERSION: i32 = 1;
pub const DEFAULT_VIPS_COMMAND: &str = "vips";
pub const DEFAULT_FFMPEG_COMMAND: &str = "ffmpeg";
pub const DEFAULT_FFPROBE_COMMAND: &str = "ffprobe";
pub const BUILTIN_IMAGES_SUPPORTED_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "jpe", "png", "gif", "webp", "bmp", "tif", "tiff",
];
/// Common libvips input suffixes used as the default binding for `vips_cli`.
///
/// Actual availability still depends on how libvips was built on the server.
pub const DEFAULT_VIPS_EXTENSIONS: &[&str] = &[
    "csv", "mat", "img", "hdr", "pbm", "pgm", "ppm", "pfm", "pnm", "svg", "svgz", "j2k", "jp2",
    "jpt", "j2c", "jpc", "gif", "png", "jpg", "jpeg", "jpe", "webp", "tif", "tiff", "fits", "fit",
    "fts", "exr", "jxl", "pdf", "heic", "heif", "avif", "svs", "vms", "vmu", "ndpi", "scn", "mrxs",
    "svslide", "bif", "raw",
];
pub const DEFAULT_FFMPEG_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mov", "mkv", "webm", "avi", "mpg", "mpeg", "m2v", "ts", "m2ts", "mts", "3gp",
    "3g2", "ogv", "flv", "wmv",
];
pub const DEFAULT_LOFTY_EXTENSIONS: &[&str] = &[
    "aac", "aiff", "aif", "ape", "flac", "m4a", "m4b", "m4p", "m4r", "mka", "mp3", "oga", "ogg",
    "opus", "wav", "wv",
];
pub const DEFAULT_FFPROBE_EXTENSIONS: &[&str] = DEFAULT_FFMPEG_EXTENSIONS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaProcessingUse {
    #[serde(rename = "thumbnail:image")]
    ThumbnailImage,
    #[serde(rename = "thumbnail:audio")]
    ThumbnailAudio,
    #[serde(rename = "thumbnail:video")]
    ThumbnailVideo,
    #[serde(rename = "metadata:image")]
    MetadataImage,
    #[serde(rename = "metadata:audio")]
    MetadataAudio,
    #[serde(rename = "metadata:video")]
    MetadataVideo,
}

impl MediaProcessingUse {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ThumbnailImage => "thumbnail:image",
            Self::ThumbnailAudio => "thumbnail:audio",
            Self::ThumbnailVideo => "thumbnail:video",
            Self::MetadataImage => "metadata:image",
            Self::MetadataAudio => "metadata:audio",
            Self::MetadataVideo => "metadata:video",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PublicThumbnailSupport {
    pub version: i32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MediaProcessingProcessorRuntimeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

impl MediaProcessingProcessorRuntimeConfig {
    fn is_empty(&self) -> bool {
        self.command.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaProcessingRegistryConfig {
    #[serde(default = "default_media_processing_registry_version")]
    pub version: i32,
    #[serde(default)]
    pub processors: Vec<MediaProcessingProcessorConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaProcessingProcessorConfig {
    pub kind: MediaProcessorKind,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uses: Vec<MediaProcessingUse>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    #[serde(
        default,
        skip_serializing_if = "MediaProcessingProcessorRuntimeConfig::is_empty"
    )]
    pub config: MediaProcessingProcessorRuntimeConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaProcessingMatchKind {
    Policy,
    Extension,
    Any,
}

impl MediaProcessingMatchKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Policy => "policy",
            Self::Extension => "extension",
            Self::Any => "any",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedMediaProcessor {
    pub processor: MediaProcessingProcessorConfig,
    pub match_kind: MediaProcessingMatchKind,
}

const fn default_media_processing_registry_version() -> i32 {
    MEDIA_PROCESSING_REGISTRY_VERSION
}

const fn default_true() -> bool {
    true
}

use std::sync::LazyLock;

use crate::types::MediaProcessorKind;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

pub const MEDIA_PROCESSING_REGISTRY_VERSION: i32 = 2;
pub const PUBLIC_THUMBNAIL_SUPPORT_VERSION: i32 = 1;
pub const PUBLIC_MEDIA_DATA_SUPPORT_VERSION: i32 = 1;
pub const PUBLIC_MEDIA_DATA_MAX_SAFE_SOURCE_BYTES: i64 = 9_007_199_254_740_991;
pub const DEFAULT_VIPS_COMMAND: &str = "vips";
pub const DEFAULT_FFMPEG_COMMAND: &str = "ffmpeg";
pub const DEFAULT_FFPROBE_COMMAND: &str = "ffprobe";
const BUILTIN_IMAGE_EXTENSION_ALIASES: &[&str] = &["apng", "jfif"];
const BUILTIN_AUDIO_METADATA_EXTENSION_ALIASES: &[&str] = &["wave"];
pub static BUILTIN_IMAGES_SUPPORTED_EXTENSIONS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    let mut extensions = std::collections::BTreeSet::new();
    for format in image::ImageFormat::all().filter(|format| format.reading_enabled()) {
        extensions.extend(format.extensions_str().iter().copied());
    }
    extensions.extend(
        BUILTIN_IMAGE_EXTENSION_ALIASES
            .iter()
            .copied()
            .filter(|extension| {
                image::ImageFormat::from_extension(extension)
                    .is_some_and(|format| format.reading_enabled())
            }),
    );
    extensions.into_iter().collect()
});
pub const BUILTIN_IMAGE_METADATA_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "jfif", "png", "apng", "tif", "tiff", "heic", "heif", "avif", "cr3", "cr2",
    "raf", "iiq", "3fr", "ari", "arw", "bay", "cap", "dcr", "dng", "erf", "fff", "k25", "kdc",
    "mef", "mos", "mrw", "nef", "nrw", "orf", "pef", "ptx", "pxn", "r3d", "raw", "rw2", "rwl",
    "sr2", "srf", "srw", "x3f",
];
/// Common libvips input suffixes used as the default binding for `vips_cli`.
///
/// Actual availability still depends on how libvips was built on the server.
pub const DEFAULT_VIPS_EXTENSIONS: &[&str] = &[
    "csv", "mat", "img", "hdr", "pbm", "pgm", "ppm", "pfm", "pnm", "svg", "svgz", "j2k", "jp2",
    "jpt", "j2c", "jpc", "gif", "png", "jpg", "jpeg", "jpe", "webp", "tif", "tiff", "fits", "fit",
    "fts", "exr", "jxl", "pdf", "heic", "heif", "avif", "svs", "vms", "vmu", "ndpi", "scn", "mrxs",
    "svslide", "bif", "raw", "nef",
];
pub const DEFAULT_FFMPEG_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mov", "mkv", "webm", "avi", "mpg", "mpeg", "m2v", "ts", "m2ts", "mts", "3gp",
    "3g2", "ogv", "flv", "wmv",
];
pub static BUILTIN_AUDIO_METADATA_EXTENSIONS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    let mut extensions = std::collections::BTreeSet::new();
    extensions.extend(lofty::file::EXTENSIONS.iter().copied());
    extensions.extend(
        BUILTIN_AUDIO_METADATA_EXTENSION_ALIASES
            .iter()
            .copied()
            .filter(|extension| lofty::file::FileType::from_ext(extension).is_some()),
    );
    extensions.into_iter().collect()
});
pub const BUILTIN_AUDIO_THUMBNAIL_EXTENSIONS: &[&str] = &[
    "aac", "aif", "aifc", "aiff", "ape", "flac", "m4a", "m4b", "m4p", "m4r", "mp+", "mp1", "mp2",
    "mp3", "mpc", "mpp", "ogg", "opus", "spx", "wav", "wave", "wv",
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum PublicMediaDataSupportMatch {
    Extensions,
    Any,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PublicMediaDataKindSupport {
    pub enabled: bool,
    #[serde(rename = "match")]
    pub match_kind: PublicMediaDataSupportMatch,
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PublicMediaDataKindsSupport {
    pub image: PublicMediaDataKindSupport,
    pub audio: PublicMediaDataKindSupport,
    pub video: PublicMediaDataKindSupport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PublicMediaDataSupport {
    pub version: i32,
    pub enabled: bool,
    pub max_source_bytes: i64,
    pub kinds: PublicMediaDataKindsSupport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum PublicImagePreviewPreference {
    PreviewFirst,
    OriginalFirst,
}

impl PublicImagePreviewPreference {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PreviewFirst => "preview_first",
            Self::OriginalFirst => "original_first",
        }
    }

    pub fn from_config_value(value: &str) -> Self {
        match value.trim() {
            "preview_first" => Self::PreviewFirst,
            "original_first" => Self::OriginalFirst,
            _ => Self::OriginalFirst,
        }
    }
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

use std::sync::LazyLock;

use aster_drive_model::types::MediaProcessorKind;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Schema version of the media processing registry JSON stored in runtime config.
///
/// Payloads with this version (or legacy version `1`) are accepted; any other value is rejected.
pub const MEDIA_PROCESSING_REGISTRY_VERSION: i32 = 2;
/// Schema version of the media processing runtime availability snapshot.
pub const MEDIA_PROCESSING_RUNTIME_STATUS_VERSION: i32 = 1;
/// Schema version of the public thumbnail capability document served to clients.
pub const PUBLIC_THUMBNAIL_SUPPORT_VERSION: i32 = 1;
/// Schema version of the public media-data capability document served to clients.
pub const PUBLIC_MEDIA_DATA_SUPPORT_VERSION: i32 = 1;
/// Upper clamp applied to the configured media metadata source size limit.
///
/// Equals `2^53 - 1` so the byte count stays representable as a JavaScript number in API clients.
pub const PUBLIC_MEDIA_DATA_MAX_SAFE_SOURCE_BYTES: i64 = 9_007_199_254_740_991;
/// Default executable used by the `vips_cli` processor when no command is configured.
pub const DEFAULT_VIPS_COMMAND: &str = "vips";
/// Default executable used by the `ffmpeg_cli` processor when no command is configured.
pub const DEFAULT_FFMPEG_COMMAND: &str = "ffmpeg";
/// Default executable used by the `ffprobe_cli` processor when no command is configured.
pub const DEFAULT_FFPROBE_COMMAND: &str = "ffprobe";
const BUILTIN_IMAGE_EXTENSION_ALIASES: &[&str] = &["apng", "jfif"];
const BUILTIN_AUDIO_METADATA_EXTENSION_ALIASES: &[&str] = &["wave"];
/// Lowercase file extensions the built-in `images` processor can decode.
///
/// Derived from all reading-enabled formats of the `image` crate plus the curated aliases
/// `apng` and `jfif`; sorted ascending.
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
/// Lowercase file extensions whose embedded metadata the built-in `images` processor can extract,
/// including common camera RAW formats; sorted ascending.
pub const BUILTIN_IMAGE_METADATA_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "jfif", "png", "apng", "webp", "tif", "tiff", "heic", "heif", "avif", "cr3",
    "cr2", "raf", "iiq", "3fr", "ari", "arw", "bay", "cap", "dcr", "dng", "erf", "fff", "k25",
    "kdc", "mef", "mos", "mrw", "nef", "nrw", "orf", "pef", "ptx", "pxn", "r3d", "raw", "rw2",
    "rwl", "sr2", "srf", "srw", "x3f",
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
/// Default video container and codec extensions bound to the `ffmpeg_cli` processor; sorted ascending.
pub const DEFAULT_FFMPEG_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mov", "mkv", "webm", "avi", "mpg", "mpeg", "m2v", "ts", "m2ts", "mts", "3gp",
    "3g2", "ogv", "flv", "wmv",
];
/// Lowercase file extensions the built-in `lofty` processor can read audio metadata from.
///
/// Derived from the `lofty` crate's supported extensions plus the `wave` alias; sorted ascending.
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
/// Audio extensions for which embedded cover art is offered as an audio thumbnail; sorted ascending.
pub const BUILTIN_AUDIO_THUMBNAIL_EXTENSIONS: &[&str] = &[
    "aac", "aif", "aifc", "aiff", "ape", "flac", "m4a", "m4b", "m4p", "m4r", "mp+", "mp1", "mp2",
    "mp3", "mpc", "mpp", "ogg", "opus", "spx", "wav", "wave", "wv",
];
/// Default extension binding for the `ffprobe_cli` processor, mirroring [`DEFAULT_FFMPEG_EXTENSIONS`].
pub const DEFAULT_FFPROBE_EXTENSIONS: &[&str] = DEFAULT_FFMPEG_EXTENSIONS;

/// A processing role a media processor can be assigned to.
///
/// Serialized as `"<role>:<media kind>"`, e.g. `thumbnail:image`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaProcessingUse {
    /// Generates image thumbnails and previews.
    #[serde(rename = "thumbnail:image")]
    ThumbnailImage,
    /// Extracts embedded cover art as an audio thumbnail.
    #[serde(rename = "thumbnail:audio")]
    ThumbnailAudio,
    /// Generates video thumbnails.
    #[serde(rename = "thumbnail:video")]
    ThumbnailVideo,
    /// Reads embedded metadata (EXIF etc.) from images.
    #[serde(rename = "metadata:image")]
    MetadataImage,
    /// Reads tags and other metadata from audio files.
    #[serde(rename = "metadata:audio")]
    MetadataAudio,
    /// Probes container and stream metadata from video files.
    #[serde(rename = "metadata:video")]
    MetadataVideo,
}

impl MediaProcessingUse {
    /// Returns the canonical serialized spelling of this use, e.g. `thumbnail:image`.
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
/// Public capability entry listing the file extensions one processing capability covers.
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PublicExtensionSupport {
    /// Whether the capability is available for at least one extension.
    pub enabled: bool,
    /// Covered extensions, lowercase and sorted; omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
/// Public thumbnail capability document: which image, audio, and video extensions can currently
/// be turned into thumbnails or previews.
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PublicThumbnailSupport {
    /// Capability document schema version; see [`PUBLIC_THUMBNAIL_SUPPORT_VERSION`].
    pub version: i32,
    /// Image extensions eligible for full-size previews.
    pub image_preview: PublicExtensionSupport,
    /// Image extensions eligible for thumbnails.
    pub image_thumbnail: PublicExtensionSupport,
    /// Audio extensions whose embedded cover art can be used as thumbnails.
    pub audio_thumbnail: PublicExtensionSupport,
    /// Video extensions eligible for thumbnails.
    pub video_thumbnail: PublicExtensionSupport,
}

/// How the public media-data capability matches a file against the listed extensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum PublicMediaDataSupportMatch {
    /// Only files whose extension appears in `extensions` are handled.
    Extensions,
    /// All files are handled regardless of extension.
    Any,
}

/// Public media-data capability for a single media kind (image, audio, or video).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PublicMediaDataKindSupport {
    /// Whether metadata extraction is offered for this media kind.
    pub enabled: bool,
    /// How `extensions` should be interpreted when matching files.
    #[serde(rename = "match")]
    pub match_kind: PublicMediaDataSupportMatch,
    /// Sorted lowercase extensions; omitted when empty or when `match_kind` is `any`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
}

/// Per-media-kind media-data capability breakdown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PublicMediaDataKindsSupport {
    /// Metadata extraction support for images.
    pub image: PublicMediaDataKindSupport,
    /// Metadata extraction support for audio files.
    pub audio: PublicMediaDataKindSupport,
    /// Metadata extraction support for video files.
    pub video: PublicMediaDataKindSupport,
}

/// Public media-data capability document describing which files can have metadata extracted,
/// and up to which source size.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PublicMediaDataSupport {
    /// Capability document schema version; see [`PUBLIC_MEDIA_DATA_SUPPORT_VERSION`].
    pub version: i32,
    /// Whether media metadata extraction is enabled overall.
    pub enabled: bool,
    /// Upper bound in bytes on the source file size accepted for metadata extraction.
    ///
    /// Clamped to [`PUBLIC_MEDIA_DATA_MAX_SAFE_SOURCE_BYTES`].
    pub max_source_bytes: i64,
    /// Capability breakdown per media kind.
    pub kinds: PublicMediaDataKindsSupport,
}

/// Client-side preference for how image previews are served; serialized as `snake_case`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum PublicImagePreviewPreference {
    /// Prefer the generated preview rendition over the original file.
    PreviewFirst,
    /// Prefer serving the original file over a generated preview.
    OriginalFirst,
}

impl PublicImagePreviewPreference {
    /// Returns the canonical config spelling of this preference, e.g. `preview_first`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PreviewFirst => "preview_first",
            Self::OriginalFirst => "original_first",
        }
    }

    /// Parses a configured preference value, falling back to [`Self::OriginalFirst`] for
    /// unknown or empty values.
    pub fn from_config_value(value: &str) -> Self {
        match value.trim() {
            "preview_first" => Self::PreviewFirst,
            "original_first" => Self::OriginalFirst,
            _ => Self::OriginalFirst,
        }
    }
}

/// Runtime command settings for a single media processor.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MediaProcessingProcessorRuntimeConfig {
    /// Executable override for the processor; left unset to use the kind's default command
    /// (e.g. [`DEFAULT_VIPS_COMMAND`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

impl MediaProcessingProcessorRuntimeConfig {
    fn is_empty(&self) -> bool {
        self.command.is_none()
    }
}

/// Deserialized form of the media processing registry JSON stored in runtime config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaProcessingRegistryConfig {
    /// Registry schema version; see [`MEDIA_PROCESSING_REGISTRY_VERSION`].
    #[serde(default = "default_media_processing_registry_version")]
    pub version: i32,
    /// Processor entries; persisted in the canonical priority order.
    #[serde(default)]
    pub processors: Vec<MediaProcessingProcessorConfig>,
}

/// Configuration for a single media processor inside the registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaProcessingProcessorConfig {
    /// Which processor this entry configures.
    pub kind: MediaProcessorKind,
    /// Whether the processor participates in candidate selection; defaults to `true`.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Processing roles assigned to the processor; empty means the kind-specific defaults.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uses: Vec<MediaProcessingUse>,
    /// Sorted lowercase extension match list; empty means "match any extension".
    ///
    /// Built-in processors ignore or normalize this list when the registry is saved.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    /// Processor-specific runtime settings, such as the executable override.
    #[serde(
        default,
        skip_serializing_if = "MediaProcessingProcessorRuntimeConfig::is_empty"
    )]
    pub config: MediaProcessingProcessorRuntimeConfig,
}

/// Why a configured processor cannot run right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum MediaProcessingUnavailableReason {
    /// The configured executable was not found on `PATH` or at the given path.
    CommandNotFound,
}

/// Runtime availability snapshot for a single configured processor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct MediaProcessingProcessorRuntimeStatus {
    /// Which processor this status describes.
    pub kind: MediaProcessorKind,
    /// Whether the registry configuration enables this processor.
    pub configured_enabled: bool,
    /// Whether the processor can actually run on this host; CLI processors require their
    /// executable to be present, built-ins always can.
    pub runtime_available: bool,
    /// Combination of `configured_enabled` and `runtime_available`; only processors that
    /// pass both handle media.
    pub effective_enabled: bool,
    /// Set when the processor is configured as enabled but is unavailable at runtime.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<MediaProcessingUnavailableReason>,
}

/// Public snapshot of the runtime availability of all configured processors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct MediaProcessingRuntimeStatus {
    /// Snapshot schema version; see [`MEDIA_PROCESSING_RUNTIME_STATUS_VERSION`].
    pub version: i32,
    /// One status entry per configured processor.
    pub processors: Vec<MediaProcessingProcessorRuntimeStatus>,
}

/// How a processor was matched to a file during candidate selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaProcessingMatchKind {
    /// Matched through a storage policy binding (storage-native thumbnails) rather than a
    /// global extension list.
    Policy,
    /// Matched because the file extension appears in the processor's extension binding.
    Extension,
    /// Matched because the processor accepts files regardless of extension.
    Any,
}

impl MediaProcessingMatchKind {
    /// Returns the canonical log and identifier spelling of this match kind, e.g. `extension`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Policy => "policy",
            Self::Extension => "extension",
            Self::Any => "any",
        }
    }
}

/// A processor selected to handle a file, together with how it matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedMediaProcessor {
    /// The matched processor's registry entry.
    pub processor: MediaProcessingProcessorConfig,
    /// How the processor matched the file.
    pub match_kind: MediaProcessingMatchKind,
}

const fn default_media_processing_registry_version() -> i32 {
    MEDIA_PROCESSING_REGISTRY_VERSION
}

const fn default_true() -> bool {
    true
}

//! 媒体元数据抽取结果的载荷与存储类型。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "snake_case")]
/// Media family a metadata record describes.
pub enum MediaMetadataKind {
    /// Image files; payload carries EXIF / dimension data.
    #[sea_orm(string_value = "image")]
    Image,
    /// Audio files; payload carries tag and stream data.
    #[sea_orm(string_value = "audio")]
    Audio,
    /// Video files; payload carries stream and container data.
    #[sea_orm(string_value = "video")]
    Video,
}

impl MediaMetadataKind {
    /// Returns the snake_case kind name stored in `blob_media_metadata.kind`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Audio => "audio",
            Self::Video => "video",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "snake_case")]
/// Extraction outcome for a media metadata record.
pub enum MediaMetadataStatus {
    /// Extraction produced a stored payload.
    #[sea_orm(string_value = "ready")]
    Ready,
    /// Extraction ran but failed for this blob.
    #[sea_orm(string_value = "failed")]
    Failed,
    /// The blob's media type has no supported extractor.
    #[sea_orm(string_value = "unsupported")]
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
/// Extracted metadata for image files; every field is `None` when the
/// corresponding tag is absent or unreadable.
pub struct ImageMediaMetadata {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Image format name, for example `jpeg`.
    pub format: Option<String>,
    /// Camera manufacturer from EXIF.
    pub camera_make: Option<String>,
    /// Camera model from EXIF.
    pub camera_model: Option<String>,
    /// Lens manufacturer from EXIF.
    pub lens_make: Option<String>,
    /// Lens model from EXIF.
    pub lens_model: Option<String>,
    /// Aperture f-number from EXIF.
    pub f_number: Option<f64>,
    /// Exposure time in seconds.
    pub exposure_time_seconds: Option<f64>,
    /// ISO sensitivity from EXIF.
    pub iso: Option<u32>,
    /// Exposure bias in EV stops.
    pub exposure_bias_ev: Option<f64>,
    /// Whether the flash fired, from the EXIF flash tag.
    pub flash_fired: Option<bool>,
    /// Raw EXIF flash mode value.
    pub flash_mode: Option<u16>,
    /// Physical focal length in millimeters.
    pub focal_length_mm: Option<f64>,
    /// 35mm-equivalent focal length in millimeters.
    pub focal_length_35mm: Option<u32>,
    /// Capture timestamp (EXIF `DateTimeOriginal`) as a formatted string.
    pub taken_at: Option<String>,
    /// Raw EXIF orientation tag value.
    pub orientation: Option<u16>,
    /// GPS latitude in decimal degrees; north is positive.
    pub gps_latitude: Option<f64>,
    /// GPS longitude in decimal degrees; east is positive.
    pub gps_longitude: Option<f64>,
    /// GPS altitude in meters above sea level.
    pub gps_altitude_meters: Option<f64>,
    /// Photographer or creator from EXIF.
    pub artist: Option<String>,
    /// Copyright notice from EXIF.
    pub copyright: Option<String>,
    /// Creating or editing software from EXIF.
    pub software: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
/// Extracted metadata for audio files; optional fields are `None` when the
/// corresponding tag or stream property is absent.
pub struct AudioMediaMetadata {
    /// Track title tag.
    pub title: Option<String>,
    /// Primary artist tag.
    pub artist: Option<String>,
    /// Full artist list; empty when the file carries none.
    pub artists: Vec<String>,
    /// Album title tag.
    pub album: Option<String>,
    /// Album-level artist tag.
    pub album_artist: Option<String>,
    /// Playback duration in milliseconds.
    pub duration_ms: Option<u64>,
    /// Audio sample rate in hertz.
    pub sample_rate: Option<u32>,
    /// Audio channel count.
    pub channels: Option<u8>,
    /// Per-sample bit depth in bits.
    pub bit_depth: Option<u8>,
    /// Overall bitrate in bits per second.
    pub overall_bitrate: Option<u32>,
    /// Audio stream bitrate in bits per second.
    pub audio_bitrate: Option<u32>,
    /// Track number within the disc.
    pub track_number: Option<u32>,
    /// Total track count of the disc.
    pub track_total: Option<u32>,
    /// Disc number within the release.
    pub disc_number: Option<u32>,
    /// Total disc count of the release.
    pub disc_total: Option<u32>,
    /// Genre tag.
    pub genre: Option<String>,
    /// Release or recording date as written in the tag.
    pub date: Option<String>,
    /// Whether the file embeds cover art.
    pub has_embedded_picture: bool,
    /// MIME type of the embedded cover art.
    pub embedded_picture_mime_type: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
/// Extracted metadata for video files; optional fields are `None` when the
/// corresponding stream property or tag is absent.
pub struct VideoMediaMetadata {
    /// Playback duration in milliseconds.
    pub duration_ms: Option<u64>,
    /// Coded frame width in pixels.
    pub width: Option<u32>,
    /// Coded frame height in pixels.
    pub height: Option<u32>,
    /// Display width in pixels after rotation is applied.
    pub display_width: Option<u32>,
    /// Display height in pixels after rotation is applied.
    pub display_height: Option<u32>,
    /// Container rotation metadata in degrees, clockwise.
    pub rotation_degrees: Option<i32>,
    /// Video codec name, for example `h264`.
    pub codec: Option<String>,
    /// Container format name, for example `mov,mp4,m4a,3gp,3g2,mj2`.
    pub container: Option<String>,
    /// Frame rate as an FFmpeg ratio string, for example `30000/1001`.
    pub frame_rate: Option<String>,
    /// Video stream bitrate in bits per second.
    pub video_bitrate: Option<u64>,
    /// Overall bitrate across all streams in bits per second.
    pub overall_bitrate: Option<u64>,
    /// Raw pixel format name, for example `yuv420p`.
    pub pixel_format: Option<String>,
    /// Per-sample bit depth in bits.
    pub bit_depth: Option<u8>,
    /// Color space name from stream metadata.
    pub color_space: Option<String>,
    /// Color transfer characteristic from stream metadata.
    pub color_transfer: Option<String>,
    /// Color primaries from stream metadata.
    pub color_primaries: Option<String>,
    /// HDR format identifier, for example `smpte2084`.
    pub hdr_format: Option<String>,
    /// First audio track codec name.
    pub audio_codec: Option<String>,
    /// First audio track channel count.
    pub audio_channels: Option<u32>,
    /// First audio track sample rate in hertz.
    pub audio_sample_rate: Option<u32>,
    /// First audio track bitrate in bits per second.
    pub audio_bitrate: Option<u64>,
    /// Number of audio tracks; zero when the file is silent.
    pub audio_stream_count: u32,
    /// Number of subtitle tracks; zero when the file has none.
    pub subtitle_stream_count: u32,
    /// Container creation timestamp as an FFmpeg tag string.
    pub creation_time: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
/// Kind-tagged metadata payload stored as JSON for one media file.
pub enum MediaMetadataPayload {
    /// Payload extracted from an image file.
    Image(ImageMediaMetadata),
    /// Payload extracted from an audio file.
    Audio(AudioMediaMetadata),
    /// Payload extracted from a video file.
    Video(VideoMediaMetadata),
}

/// Raw JSON serialization of `MediaMetadataPayload` stored in
/// `blob_media_metadata.metadata_json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, DeriveValueType)]
pub struct StoredMediaMetadataPayload(pub String);

impl AsRef<str> for StoredMediaMetadataPayload {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for StoredMediaMetadataPayload {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<StoredMediaMetadataPayload> for String {
    fn from(value: StoredMediaMetadataPayload) -> Self {
        value.0
    }
}

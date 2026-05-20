use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "snake_case")]
pub enum MediaMetadataKind {
    #[sea_orm(string_value = "image")]
    Image,
    #[sea_orm(string_value = "audio")]
    Audio,
    #[sea_orm(string_value = "video")]
    Video,
}

impl MediaMetadataKind {
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
pub enum MediaMetadataStatus {
    #[sea_orm(string_value = "ready")]
    Ready,
    #[sea_orm(string_value = "failed")]
    Failed,
    #[sea_orm(string_value = "unsupported")]
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ImageMediaMetadata {
    pub width: u32,
    pub height: u32,
    pub format: Option<String>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_make: Option<String>,
    pub lens_model: Option<String>,
    pub f_number: Option<f64>,
    pub exposure_time_seconds: Option<f64>,
    pub iso: Option<u32>,
    pub exposure_bias_ev: Option<f64>,
    pub flash_fired: Option<bool>,
    pub flash_mode: Option<u16>,
    pub focal_length_mm: Option<f64>,
    pub focal_length_35mm: Option<u32>,
    pub taken_at: Option<String>,
    pub orientation: Option<u16>,
    pub artist: Option<String>,
    pub copyright: Option<String>,
    pub software: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AudioMediaMetadata {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub duration_ms: Option<u64>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub bit_depth: Option<u8>,
    pub overall_bitrate: Option<u32>,
    pub audio_bitrate: Option<u32>,
    pub track_number: Option<u32>,
    pub track_total: Option<u32>,
    pub disc_number: Option<u32>,
    pub disc_total: Option<u32>,
    pub genre: Option<String>,
    pub date: Option<String>,
    pub has_embedded_picture: bool,
    pub embedded_picture_mime_type: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct VideoMediaMetadata {
    pub duration_ms: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub display_width: Option<u32>,
    pub display_height: Option<u32>,
    pub rotation_degrees: Option<i32>,
    pub codec: Option<String>,
    pub container: Option<String>,
    pub frame_rate: Option<String>,
    pub video_bitrate: Option<u64>,
    pub overall_bitrate: Option<u64>,
    pub pixel_format: Option<String>,
    pub bit_depth: Option<u8>,
    pub color_space: Option<String>,
    pub color_transfer: Option<String>,
    pub color_primaries: Option<String>,
    pub hdr_format: Option<String>,
    pub audio_codec: Option<String>,
    pub audio_channels: Option<u32>,
    pub audio_sample_rate: Option<u32>,
    pub audio_bitrate: Option<u64>,
    pub audio_stream_count: u32,
    pub subtitle_stream_count: u32,
    pub creation_time: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MediaMetadataPayload {
    Image(ImageMediaMetadata),
    Audio(AudioMediaMetadata),
    Video(VideoMediaMetadata),
}

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

use std::collections::BTreeMap;

use aster_forge_xml::{XmlEvent, XmlSafetyPolicy, XmlWalkError, walk_validated_xml};
use async_trait::async_trait;
use chrono::Utc;

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::storage::http_body::read_reqwest_response_body_limited;
use crate::storage::traits::extensions::{
    NativeMediaMetadataRequest, NativeMediaMetadataResult, NativeMediaMetadataStorageDriver,
};
use crate::types::{
    AudioMediaMetadata, MediaMetadataKind, MediaMetadataPayload, VideoMediaMetadata,
};

use super::{MAX_COS_THUMBNAIL_TTL, TencentCosDriver};

const COS_NATIVE_MEDIA_METADATA_PARSER: &str = "tencent_cos_ci_videoinfo";
const COS_NATIVE_MEDIA_METADATA_VERSION: &str = "1";

impl TencentCosDriver {
    pub(super) fn signed_ci_media_info_url(&self, path: &str) -> Result<String> {
        let now = Utc::now();
        let start = now.timestamp();
        let end = (now
            + chrono::Duration::from_std(MAX_COS_THUMBNAIL_TTL)
                .map_aster_err_ctx("COS media info expiry", AsterError::storage_driver_error)?)
        .timestamp();
        let key_time = format!("{start};{end}");
        let params = [("ci-process", "videoinfo")];
        let (url, _) = self.signed_cos_query_url(path, &params, &key_time)?;
        Ok(String::from(url))
    }
}

#[async_trait]
impl NativeMediaMetadataStorageDriver for TencentCosDriver {
    async fn get_native_media_metadata(
        &self,
        request: &NativeMediaMetadataRequest,
    ) -> Result<Option<NativeMediaMetadataResult>> {
        if !is_cos_media_info_candidate(request.kind, &request.source_mime_type) {
            return Ok(None);
        }

        let url = self.signed_ci_media_info_url(&request.storage_path)?;
        let response = self.client.get(url).send().await.map_aster_err_ctx(
            "COS native media metadata request",
            AsterError::storage_driver_error,
        )?;
        let status = response.status();
        if !status.is_success() {
            return Err(storage_driver_error(
                if status == reqwest::StatusCode::NOT_FOUND {
                    StorageErrorKind::NotFound
                } else if status == reqwest::StatusCode::FORBIDDEN
                    || status == reqwest::StatusCode::UNAUTHORIZED
                {
                    StorageErrorKind::Auth
                } else if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    StorageErrorKind::RateLimited
                } else if status.is_server_error() {
                    StorageErrorKind::Transient
                } else {
                    StorageErrorKind::Unsupported
                },
                format!("COS native media metadata request failed with HTTP {status}"),
            ));
        }

        let body = read_reqwest_response_body_limited(
            response,
            "COS native media metadata body",
            XmlSafetyPolicy::untrusted().max_input_bytes,
            AsterError::storage_driver_error,
        )
        .await?;
        parse_cos_media_info_xml(&body, request.kind).map(Some)
    }
}

pub(super) fn is_cos_media_info_candidate(kind: MediaMetadataKind, mime_type: &str) -> bool {
    let mime_type = mime_type.trim().to_ascii_lowercase();
    match kind {
        MediaMetadataKind::Audio => mime_type.starts_with("audio/"),
        MediaMetadataKind::Video => mime_type.starts_with("video/"),
        MediaMetadataKind::Image => false,
    }
}

fn parse_cos_media_info_xml(
    body: &[u8],
    kind: MediaMetadataKind,
) -> Result<NativeMediaMetadataResult> {
    let fields = collect_cos_media_fields(body)?;
    let metadata = match kind {
        MediaMetadataKind::Video => MediaMetadataPayload::Video(parse_cos_video_metadata(&fields)),
        MediaMetadataKind::Audio => MediaMetadataPayload::Audio(parse_cos_audio_metadata(&fields)),
        MediaMetadataKind::Image => {
            return Err(storage_driver_error(
                StorageErrorKind::Unsupported,
                "COS native media metadata does not support image metadata",
            ));
        }
    };
    Ok(NativeMediaMetadataResult {
        kind,
        metadata,
        parser: COS_NATIVE_MEDIA_METADATA_PARSER.to_string(),
        parser_version: COS_NATIVE_MEDIA_METADATA_VERSION.to_string(),
    })
}

#[derive(Default)]
struct CosMediaFields {
    video: Option<BTreeMap<String, String>>,
    audio: Option<BTreeMap<String, String>>,
    format: Option<BTreeMap<String, String>>,
    audio_stream_count: u32,
    subtitle_stream_count: u32,
}

fn collect_cos_media_fields(body: &[u8]) -> Result<CosMediaFields> {
    let mut stack = Vec::<String>::new();
    let mut text = Vec::<String>::new();
    let mut fields = CosMediaFields::default();
    walk_validated_xml(body, XmlSafetyPolicy::untrusted(), |event| {
        match event {
            XmlEvent::Start(element) => {
                let name = element.local_name().to_string();
                if name.eq_ignore_ascii_case("Audio") {
                    fields.audio_stream_count = fields.audio_stream_count.saturating_add(1);
                } else if name.eq_ignore_ascii_case("Subtitle") {
                    fields.subtitle_stream_count = fields.subtitle_stream_count.saturating_add(1);
                }
                stack.push(name);
                text.push(String::new());
            }
            XmlEvent::Text(value) | XmlEvent::CData(value) => {
                if let Some(text) = text.last_mut() {
                    text.push_str(&value);
                }
            }
            XmlEvent::End { .. } => {
                let Some(name) = stack.pop() else {
                    return Err(AsterError::storage_driver_error(
                        "parse COS native media metadata XML",
                    ));
                };
                let value = text.pop().unwrap_or_default().trim().to_string();
                let parent = stack.last().map(String::as_str);
                match parent {
                    Some("Video") => insert_first_field(&mut fields.video, name, value),
                    Some("Audio") => insert_first_field(&mut fields.audio, name, value),
                    Some("Format") => insert_first_field(&mut fields.format, name, value),
                    _ => {}
                }
            }
            _ => {}
        }
        Ok(())
    })
    .map_err(|error| match error {
        XmlWalkError::Xml(error) => AsterError::storage_driver_error(error.to_string()),
        XmlWalkError::Visitor(error) => error,
    })?;
    Ok(fields)
}

fn insert_first_field(target: &mut Option<BTreeMap<String, String>>, name: String, value: String) {
    if value.is_empty() {
        return;
    }
    target
        .get_or_insert_with(BTreeMap::new)
        .entry(name)
        .or_insert(value);
}

fn parse_cos_video_metadata(fields: &CosMediaFields) -> VideoMediaMetadata {
    let video = fields.video.as_ref();
    let audio = fields.audio.as_ref();
    let format = fields.format.as_ref();
    let width = video.and_then(|node| child_u32(node, &["Width"]));
    let height = video.and_then(|node| child_u32(node, &["Height"]));
    let rotation_degrees = video.and_then(|node| child_i32(node, &["Rotate", "Rotation"]));
    let (display_width, display_height) = display_dimensions(width, height, rotation_degrees);

    VideoMediaMetadata {
        duration_ms: video
            .and_then(|node| child_duration_ms(node, &["Duration"]))
            .or_else(|| format.and_then(|node| child_duration_ms(node, &["Duration"]))),
        width,
        height,
        display_width,
        display_height,
        rotation_degrees,
        codec: video.and_then(|node| child_string(node, &["CodecName", "Codec"])),
        container: format.and_then(|node| child_string(node, &["FormatName", "Format"])),
        frame_rate: video
            .and_then(|node| child_string(node, &["Fps", "FrameRate", "AvgFrameRate"])),
        video_bitrate: video.and_then(|node| child_u64(node, &["Bitrate", "BitRate"])),
        overall_bitrate: format.and_then(|node| child_u64(node, &["Bitrate", "BitRate"])),
        pixel_format: video.and_then(|node| child_string(node, &["PixFormat", "PixelFormat"])),
        bit_depth: video.and_then(|node| child_u8(node, &["BitDepth"])),
        color_space: video.and_then(|node| child_string(node, &["ColorSpace"])),
        color_transfer: video.and_then(|node| child_string(node, &["ColorTransfer"])),
        color_primaries: video.and_then(|node| child_string(node, &["ColorPrimaries"])),
        hdr_format: video.and_then(|node| child_string(node, &["HdrFormat", "HDRFormat"])),
        audio_codec: audio.and_then(|node| child_string(node, &["CodecName", "Codec"])),
        audio_channels: audio.and_then(|node| child_u32(node, &["Channel", "Channels"])),
        audio_sample_rate: audio.and_then(|node| child_u32(node, &["SampleRate"])),
        audio_bitrate: audio.and_then(|node| child_u64(node, &["Bitrate", "BitRate"])),
        audio_stream_count: fields.audio_stream_count,
        subtitle_stream_count: fields.subtitle_stream_count,
        creation_time: format
            .and_then(|node| child_string(node, &["CreationTime"]))
            .or_else(|| video.and_then(|node| child_string(node, &["CreationTime"]))),
    }
}

fn parse_cos_audio_metadata(fields: &CosMediaFields) -> AudioMediaMetadata {
    let audio = fields.audio.as_ref();
    let format = fields.format.as_ref();

    AudioMediaMetadata {
        title: None,
        artist: None,
        artists: Vec::new(),
        album: None,
        album_artist: None,
        duration_ms: audio
            .and_then(|node| child_duration_ms(node, &["Duration"]))
            .or_else(|| format.and_then(|node| child_duration_ms(node, &["Duration"]))),
        sample_rate: audio.and_then(|node| child_u32(node, &["SampleRate"])),
        channels: audio
            .and_then(|node| child_u8(node, &["Channel", "Channels"]))
            .or_else(|| {
                audio
                    .and_then(|node| child_u32(node, &["Channel", "Channels"]))
                    .and_then(|value| u8::try_from(value).ok())
            }),
        bit_depth: audio.and_then(|node| child_u8(node, &["BitDepth"])),
        overall_bitrate: format
            .and_then(|node| child_u32(node, &["Bitrate", "BitRate"]))
            .or_else(|| audio.and_then(|node| child_u32(node, &["Bitrate", "BitRate"]))),
        audio_bitrate: audio.and_then(|node| child_u32(node, &["Bitrate", "BitRate"])),
        track_number: None,
        track_total: None,
        disc_number: None,
        disc_total: None,
        genre: None,
        date: format.and_then(|node| child_string(node, &["CreationTime"])),
        has_embedded_picture: false,
        embedded_picture_mime_type: None,
    }
}

fn display_dimensions(
    width: Option<u32>,
    height: Option<u32>,
    rotation_degrees: Option<i32>,
) -> (Option<u32>, Option<u32>) {
    let Some(width) = width else {
        return (None, None);
    };
    let Some(height) = height else {
        return (Some(width), None);
    };

    if rotation_degrees
        .map(|degrees| degrees.rem_euclid(180) == 90)
        .unwrap_or(false)
    {
        (Some(height), Some(width))
    } else {
        (Some(width), Some(height))
    }
}

fn child_string(element: &BTreeMap<String, String>, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        element.iter().find_map(|(field_name, value)| {
            field_name.eq_ignore_ascii_case(name).then(|| value.clone())
        })
    })
}

fn child_u8(element: &BTreeMap<String, String>, names: &[&str]) -> Option<u8> {
    child_string(element, names).and_then(|value| value.parse().ok())
}

fn child_u32(element: &BTreeMap<String, String>, names: &[&str]) -> Option<u32> {
    child_string(element, names).and_then(|value| value.parse().ok())
}

fn child_i32(element: &BTreeMap<String, String>, names: &[&str]) -> Option<i32> {
    child_string(element, names).and_then(|value| value.parse().ok())
}

fn child_u64(element: &BTreeMap<String, String>, names: &[&str]) -> Option<u64> {
    child_string(element, names).and_then(|value| value.parse().ok())
}

fn child_duration_ms(element: &BTreeMap<String, String>, names: &[&str]) -> Option<u64> {
    child_string(element, names).and_then(|value| parse_duration_ms(&value))
}

fn parse_duration_ms(value: &str) -> Option<u64> {
    let seconds = value.parse::<f64>().ok()?;
    if !seconds.is_finite() || seconds <= 0.0 {
        return None;
    }
    aster_forge_utils::numbers::f64_seconds_to_u64_millis(
        seconds,
        "COS native media metadata duration",
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use aster_forge_xml::{
        DEFAULT_XML_MAX_ATTRIBUTES_PER_ELEMENT, DEFAULT_XML_MAX_DEPTH, DEFAULT_XML_MAX_TEXT_BYTES,
    };

    use super::{parse_cos_media_info_xml, parse_duration_ms};
    use crate::types::{MediaMetadataKind, MediaMetadataPayload};

    #[test]
    fn parses_cos_video_media_info_xml() {
        let xml = br#"
            <Response>
              <MediaInfo>
                <Stream>
                  <Video>
                    <CodecName>h264</CodecName>
                    <Width>1920</Width>
                    <Height>1080</Height>
                    <Duration>12.345000</Duration>
                    <Bitrate>8000000</Bitrate>
                    <Fps>30000/1001</Fps>
                    <Rotate>90</Rotate>
                  </Video>
                  <Audio>
                    <CodecName>aac</CodecName>
                    <Channel>2</Channel>
                    <SampleRate>48000</SampleRate>
                    <Bitrate>192000</Bitrate>
                  </Audio>
                  <Subtitle>
                    <CodecName>subrip</CodecName>
                  </Subtitle>
                </Stream>
                <Format>
                  <FormatName>mov,mp4,m4a,3gp,3g2,mj2</FormatName>
                  <Duration>12.345000</Duration>
                  <Bitrate>8192000</Bitrate>
                  <CreationTime>2026-06-03T10:00:00Z</CreationTime>
                </Format>
              </MediaInfo>
            </Response>
        "#;

        let result = parse_cos_media_info_xml(xml, MediaMetadataKind::Video).unwrap();
        let MediaMetadataPayload::Video(metadata) = result.metadata else {
            panic!("expected video payload");
        };
        assert_eq!(metadata.duration_ms, Some(12_345));
        assert_eq!(metadata.width, Some(1920));
        assert_eq!(metadata.height, Some(1080));
        assert_eq!(metadata.display_width, Some(1080));
        assert_eq!(metadata.display_height, Some(1920));
        assert_eq!(metadata.rotation_degrees, Some(90));
        assert_eq!(metadata.codec.as_deref(), Some("h264"));
        assert_eq!(
            metadata.container.as_deref(),
            Some("mov,mp4,m4a,3gp,3g2,mj2")
        );
        assert_eq!(metadata.audio_codec.as_deref(), Some("aac"));
        assert_eq!(metadata.audio_stream_count, 1);
        assert_eq!(metadata.subtitle_stream_count, 1);
    }

    #[test]
    fn parses_cos_audio_media_info_xml() {
        let xml = br#"
            <MediaInfo>
              <Stream>
                <Audio>
                  <SampleRate>44100</SampleRate>
                  <Channel>2</Channel>
                  <Duration>5.5</Duration>
                  <Bitrate>128000</Bitrate>
                </Audio>
              </Stream>
              <Format>
                <Duration>5.5</Duration>
                <Bitrate>130000</Bitrate>
              </Format>
            </MediaInfo>
        "#;

        let result = parse_cos_media_info_xml(xml, MediaMetadataKind::Audio).unwrap();
        let MediaMetadataPayload::Audio(metadata) = result.metadata else {
            panic!("expected audio payload");
        };
        assert_eq!(metadata.duration_ms, Some(5_500));
        assert_eq!(metadata.sample_rate, Some(44_100));
        assert_eq!(metadata.channels, Some(2));
        assert_eq!(metadata.audio_bitrate, Some(128_000));
        assert_eq!(metadata.overall_bitrate, Some(130_000));
    }

    #[test]
    fn parses_cos_duration_with_checked_rounding_and_rejects_invalid_values() {
        assert_eq!(parse_duration_ms("1.2345"), Some(1235));

        for value in ["0", "-1", "NaN", "not-a-number"] {
            assert_eq!(parse_duration_ms(value), None);
        }
    }

    #[test]
    fn media_info_parser_covers_xml_safety_boundaries() {
        for xml in [
            "<!DOCTYPE Response><Response/>",
            "junk<Response/>",
            "<Response/>junk",
        ] {
            assert!(parse_cos_media_info_xml(xml.as_bytes(), MediaMetadataKind::Video).is_err());
        }

        let comment = "x".repeat(DEFAULT_XML_MAX_TEXT_BYTES + 1);
        assert!(
            parse_cos_media_info_xml(
                format!("<Response><!--{comment}--></Response>").as_bytes(),
                MediaMetadataKind::Video,
            )
            .is_err()
        );

        let mut exact_depth = String::from("<Response>");
        for _ in 1..DEFAULT_XML_MAX_DEPTH {
            exact_depth.push_str("<x>");
        }
        for _ in 1..DEFAULT_XML_MAX_DEPTH {
            exact_depth.push_str("</x>");
        }
        exact_depth.push_str("</Response>");
        assert!(parse_cos_media_info_xml(exact_depth.as_bytes(), MediaMetadataKind::Video).is_ok());

        let mut attributes = String::new();
        for index in 0..DEFAULT_XML_MAX_ATTRIBUTES_PER_ELEMENT {
            attributes.push_str(&format!("x{index}=\"value\" "));
        }
        assert!(
            parse_cos_media_info_xml(
                format!("<Response {attributes}/>").as_bytes(),
                MediaMetadataKind::Video,
            )
            .is_ok()
        );
        attributes.push_str("overflow=\"value\" ");
        assert!(
            parse_cos_media_info_xml(
                format!("<Response {attributes}/>").as_bytes(),
                MediaMetadataKind::Video,
            )
            .is_err()
        );
    }
}

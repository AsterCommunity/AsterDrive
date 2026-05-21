use std::path::Path;

use serde_json::Value;

use crate::config::media_processing;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::services::media_processing_service::run_cli_command_with_timeout;
use crate::types::VideoMediaMetadata;

pub(super) fn parse_video_metadata_from_path(
    command: &str,
    path: &Path,
) -> Result<VideoMediaMetadata> {
    let path_arg = path.to_string_lossy().to_string();
    let output = run_cli_command_with_timeout(
        command,
        &[
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_streams",
            "-show_format",
            &path_arg,
        ],
        |message| AsterError::validation_error(format!("ffprobe metadata failed: {message}")),
    )?;
    if !output.status.success() {
        let detail = crate::services::media_processing_service::cli_output_detail(&output);
        return Err(AsterError::validation_error(format!(
            "ffprobe metadata command failed: {detail}"
        )));
    }

    let value: Value = serde_json::from_slice(&output.stdout)
        .map_aster_err_ctx("parse ffprobe metadata JSON", AsterError::validation_error)?;
    let streams = value.get("streams").and_then(Value::as_array);
    let video_stream = first_stream_of_type(streams, "video");
    let audio_stream = first_stream_of_type(streams, "audio");
    let format = value.get("format");
    let width = video_stream.and_then(|stream| json_u32(stream.get("width")));
    let height = video_stream.and_then(|stream| json_u32(stream.get("height")));
    let rotation_degrees = video_stream.and_then(video_rotation_degrees);
    let (display_width, display_height) = display_dimensions(width, height, rotation_degrees);

    Ok(VideoMediaMetadata {
        duration_ms: video_stream
            .and_then(|stream| json_duration_ms(stream.get("duration")))
            .or_else(|| format.and_then(|format| json_duration_ms(format.get("duration")))),
        width,
        height,
        display_width,
        display_height,
        rotation_degrees,
        codec: video_stream
            .and_then(|stream| clean_json_string(stream.get("codec_name")))
            .or_else(|| {
                video_stream.and_then(|stream| clean_json_string(stream.get("codec_tag_string")))
            }),
        container: format.and_then(|format| clean_json_string(format.get("format_name"))),
        frame_rate: video_stream
            .and_then(|stream| clean_json_string(stream.get("avg_frame_rate")))
            .filter(|value| value != "0/0")
            .or_else(|| {
                video_stream
                    .and_then(|stream| clean_json_string(stream.get("r_frame_rate")))
                    .filter(|value| value != "0/0")
            }),
        video_bitrate: video_stream.and_then(|stream| json_u64(stream.get("bit_rate"))),
        overall_bitrate: format.and_then(|format| json_u64(format.get("bit_rate"))),
        pixel_format: video_stream.and_then(|stream| clean_json_string(stream.get("pix_fmt"))),
        bit_depth: video_stream.and_then(video_bit_depth),
        color_space: video_stream.and_then(|stream| clean_json_string(stream.get("color_space"))),
        color_transfer: video_stream
            .and_then(|stream| clean_json_string(stream.get("color_transfer"))),
        color_primaries: video_stream
            .and_then(|stream| clean_json_string(stream.get("color_primaries"))),
        hdr_format: video_stream.and_then(detect_hdr_format),
        audio_codec: audio_stream
            .and_then(|stream| clean_json_string(stream.get("codec_name")))
            .or_else(|| {
                audio_stream.and_then(|stream| clean_json_string(stream.get("codec_tag_string")))
            }),
        audio_channels: audio_stream.and_then(|stream| json_u32(stream.get("channels"))),
        audio_sample_rate: audio_stream.and_then(|stream| json_u32(stream.get("sample_rate"))),
        audio_bitrate: audio_stream.and_then(|stream| json_u64(stream.get("bit_rate"))),
        audio_stream_count: stream_count(streams, "audio"),
        subtitle_stream_count: stream_count(streams, "subtitle"),
        creation_time: format
            .and_then(|format| json_tag_string(format, "creation_time"))
            .or_else(|| video_stream.and_then(|stream| json_tag_string(stream, "creation_time"))),
    })
}

fn first_stream_of_type<'a>(
    streams: Option<&'a Vec<Value>>,
    codec_type: &str,
) -> Option<&'a Value> {
    streams?.iter().find(|stream| {
        stream
            .get("codec_type")
            .and_then(Value::as_str)
            .is_some_and(|value| value == codec_type)
    })
}

fn stream_count(streams: Option<&Vec<Value>>, codec_type: &str) -> u32 {
    let count = streams
        .map(|streams| {
            streams
                .iter()
                .filter(|stream| {
                    stream
                        .get("codec_type")
                        .and_then(Value::as_str)
                        .is_some_and(|value| value == codec_type)
                })
                .count()
        })
        .unwrap_or(0);
    crate::utils::numbers::usize_to_u32(count, "media metadata stream count").unwrap_or(u32::MAX)
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

fn video_rotation_degrees(stream: &Value) -> Option<i32> {
    stream
        .get("side_data_list")
        .and_then(Value::as_array)
        .and_then(|side_data| {
            side_data
                .iter()
                .find_map(|entry| json_i32(entry.get("rotation")))
        })
        .or_else(|| json_tag_string(stream, "rotate").and_then(|value| value.parse::<i32>().ok()))
}

fn video_bit_depth(stream: &Value) -> Option<u8> {
    json_u8(stream.get("bits_per_raw_sample")).or_else(|| {
        stream
            .get("pix_fmt")
            .and_then(Value::as_str)
            .and_then(bit_depth_from_pixel_format)
    })
}

fn bit_depth_from_pixel_format(pixel_format: &str) -> Option<u8> {
    let normalized = pixel_format.to_ascii_lowercase();
    if normalized.contains("p16") || normalized.contains("16le") || normalized.contains("16be") {
        Some(16)
    } else if normalized.contains("p14") {
        Some(14)
    } else if normalized.contains("p12") {
        Some(12)
    } else if normalized.contains("p10") {
        Some(10)
    } else if normalized.contains("p9") {
        Some(9)
    } else if normalized.contains("yuv")
        || normalized.contains("rgb")
        || normalized.contains("gray")
    {
        Some(8)
    } else {
        None
    }
}

fn detect_hdr_format(stream: &Value) -> Option<String> {
    if stream
        .get("side_data_list")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| clean_json_string(entry.get("side_data_type")))
        .any(|value| {
            let normalized = value.to_ascii_lowercase();
            normalized.contains("dovi") || normalized.contains("dolby vision")
        })
    {
        return Some("Dolby Vision".to_string());
    }

    let color_transfer = clean_json_string(stream.get("color_transfer"))?;
    let color_transfer = color_transfer.to_ascii_lowercase();
    let color_primaries = clean_json_string(stream.get("color_primaries"))
        .unwrap_or_default()
        .to_ascii_lowercase();

    if color_transfer == "smpte2084" {
        if color_primaries == "bt2020" {
            Some("HDR10".to_string())
        } else {
            Some("PQ HDR".to_string())
        }
    } else if color_transfer == "arib-std-b67" {
        Some("HLG".to_string())
    } else {
        None
    }
}

pub async fn probe_ffprobe_cli_command(command: &str) -> Result<String> {
    let command = media_processing::normalize_ffprobe_command(command)?;
    if !media_processing::command_is_available(&command) {
        return Err(AsterError::validation_error(format!(
            "ffprobe command '{command}' is not available"
        )));
    }

    tracing::debug!(
        command = %command,
        "starting ffprobe CLI probe for media metadata"
    );

    let probe_command = command.clone();
    let output = tokio::task::spawn_blocking(move || {
        run_cli_command_with_timeout(&probe_command, &["-version"], |message| {
            AsterError::validation_error(format!("ffprobe probe failed: {message}"))
        })
    })
    .await
    .map_aster_err_ctx("ffprobe probe task panicked", AsterError::validation_error)??;

    if !output.status.success() {
        let detail = crate::services::media_processing_service::cli_output_detail(&output);
        return Err(AsterError::validation_error(format!(
            "ffprobe probe failed for '{command}': {detail}"
        )));
    }

    let detail = first_non_empty_output_line(&output.stdout)
        .or_else(|| first_non_empty_output_line(&output.stderr))
        .unwrap_or_default();

    tracing::debug!(
        command = %command,
        version = detail.as_str(),
        "ffprobe CLI probe completed"
    );

    if detail.is_empty() {
        Ok(format!("ffprobe command '{command}' is available"))
    } else {
        Ok(format!(
            "ffprobe command '{command}' is available: {detail}"
        ))
    }
}

fn first_non_empty_output_line(output: &[u8]) -> Option<String> {
    String::from_utf8_lossy(output)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

fn clean_json_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "N/A")
        .map(str::to_string)
}

fn json_u32(value: Option<&Value>) -> Option<u32> {
    match value? {
        Value::Number(number) => number.as_u64().and_then(|value| u32::try_from(value).ok()),
        Value::String(value) => value.trim().parse::<u32>().ok(),
        _ => None,
    }
}

fn json_u64(value: Option<&Value>) -> Option<u64> {
    match value? {
        Value::Number(number) => number.as_u64(),
        Value::String(value) => value.trim().parse::<u64>().ok(),
        _ => None,
    }
}

fn json_i32(value: Option<&Value>) -> Option<i32> {
    match value? {
        Value::Number(number) => number.as_i64().and_then(|value| i32::try_from(value).ok()),
        Value::String(value) => value.trim().parse::<i32>().ok(),
        _ => None,
    }
}

fn json_u8(value: Option<&Value>) -> Option<u8> {
    match value? {
        Value::Number(number) => number.as_u64().and_then(|value| u8::try_from(value).ok()),
        Value::String(value) => value.trim().parse::<u8>().ok(),
        _ => None,
    }
}

fn json_tag_string(value: &Value, key: &str) -> Option<String> {
    value
        .get("tags")
        .and_then(|tags| tags.get(key))
        .and_then(|tag| clean_json_string(Some(tag)))
}

fn json_duration_ms(value: Option<&Value>) -> Option<u64> {
    let raw = match value? {
        Value::Number(number) => number.as_f64()?,
        Value::String(value) => value.trim().parse::<f64>().ok()?,
        _ => return None,
    };
    if !raw.is_finite() || raw <= 0.0 {
        return None;
    }
    crate::utils::numbers::f64_seconds_to_u64_millis(raw, "media metadata duration").ok()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn json_duration_ms_rounds_ffprobe_seconds_to_milliseconds() {
        assert_eq!(json_duration_ms(Some(&json!(1.2344))), Some(1234));
        assert_eq!(json_duration_ms(Some(&json!(1.2345))), Some(1235));
        assert_eq!(json_duration_ms(Some(&json!("2.5"))), Some(2500));
    }

    #[test]
    fn json_duration_ms_rejects_non_positive_or_invalid_values() {
        assert_eq!(json_duration_ms(Some(&json!(0))), None);
        assert_eq!(json_duration_ms(Some(&json!(-1))), None);
        assert_eq!(json_duration_ms(Some(&json!("N/A"))), None);
        assert_eq!(json_duration_ms(Some(&json!(null))), None);
        assert_eq!(json_duration_ms(None), None);
    }
}

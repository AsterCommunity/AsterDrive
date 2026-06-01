use reqwest::header::CONTENT_DISPOSITION;
use url::Url;

use crate::config::operations;
use crate::errors::Result;
use crate::services::task_service::types::OfflineDownloadTaskPayload;

pub(super) fn offline_download_task_base_display_name(
    payload: &OfflineDownloadTaskPayload,
) -> String {
    match payload.filename.as_deref() {
        Some(filename) => format!("Import {filename} from link"),
        None => format!(
            "Import from {}",
            payload
                .source_display_url
                .as_deref()
                .unwrap_or("external link")
        ),
    }
}

pub(super) fn offline_download_task_display_name_with_engine(
    base_display_name: &str,
    engine: operations::OfflineDownloadEngine,
) -> String {
    format!("{base_display_name} via {}", engine.display_name())
}

pub(in crate::services::task_service) fn response_filename(
    headers: &reqwest::header::HeaderMap,
) -> Option<String> {
    headers
        .get(CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .and_then(filename_from_content_disposition)
}

pub(super) fn filename_from_content_disposition(raw: &str) -> Option<String> {
    let mut fallback = None;
    for part in raw.split(';').skip(1) {
        let (name, value) = part.trim().split_once('=')?;
        let name = name.trim();
        if name.eq_ignore_ascii_case("filename*") {
            if let Some(decoded) = decode_rfc5987_filename(value.trim())
                && let Ok(name) = crate::utils::normalize_validate_name(&decoded)
            {
                return Some(name);
            }
        } else if name.eq_ignore_ascii_case("filename") {
            let value = value.trim().trim_matches('"');
            if !value.is_empty()
                && let Ok(name) = crate::utils::normalize_validate_name(value)
            {
                fallback = Some(name);
            }
        }
    }
    fallback
}

fn decode_rfc5987_filename(raw: &str) -> Option<String> {
    let raw = raw.trim().trim_matches('"');
    let mut parts = raw.splitn(3, '\'');
    let charset = parts.next()?.trim();
    let _language = parts.next()?;
    let encoded = parts.next()?;
    let decoded_bytes = percent_encoding::percent_decode_str(encoded).collect::<Vec<u8>>();
    match charset.to_ascii_lowercase().as_str() {
        "utf-8" | "us-ascii" => String::from_utf8(decoded_bytes).ok(),
        _ => None,
    }
}

pub(super) fn resolve_offline_download_filename(
    requested: Option<&str>,
    response: Option<&str>,
    url: &Url,
) -> Result<String> {
    if let Some(name) = requested {
        return crate::utils::normalize_validate_name(name);
    }
    if let Some(name) = response {
        return crate::utils::normalize_validate_name(name);
    }
    let from_path = url
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .and_then(|segment| {
            percent_encoding::percent_decode_str(segment)
                .decode_utf8()
                .ok()
        })
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(name) = from_path
        && let Ok(name) = crate::utils::normalize_validate_name(&name)
    {
        return Ok(name);
    }
    Ok("download".to_string())
}

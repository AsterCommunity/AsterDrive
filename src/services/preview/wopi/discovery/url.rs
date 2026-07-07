use reqwest::Url;

use crate::config::cors;
use crate::errors::{AsterError, Result};
use crate::services::preview::apps;

pub(crate) fn expand_action_url(raw: &str, wopi_src: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AsterError::validation_error(
            "WOPI action_url must not be empty",
        ));
    }

    let wopi_src_encoded = urlencoding::encode(wopi_src);
    let resolved = trimmed
        .replace("{{wopi_src}}", &wopi_src_encoded)
        .replace("{{WOPISrc}}", &wopi_src_encoded);
    if resolved.contains("{{wopi_src}}") || resolved.contains("{{WOPISrc}}") {
        return Err(AsterError::validation_error(
            "WOPI action_url contains an unresolved WOPISrc placeholder",
        ));
    }

    let resolved = expand_discovery_url_placeholders(&resolved, &wopi_src_encoded);
    if resolved.contains('<') || resolved.contains('>') {
        return Err(AsterError::validation_error(
            "WOPI action_url contains unresolved discovery placeholders",
        ));
    }

    if resolved == trimmed {
        return append_wopi_src(trimmed, wopi_src);
    }

    Url::parse(&resolved).map_err(|error| {
        AsterError::validation_error(format!("invalid WOPI action_url: {error}"))
    })?;
    append_wopi_src_if_missing(&resolved, wopi_src)
}

fn expand_discovery_url_placeholders(raw: &str, wopi_src_encoded: &str) -> String {
    let mut output = String::with_capacity(raw.len() + wopi_src_encoded.len());
    let mut index = 0;

    while let Some(start_offset) = raw[index..].find('<') {
        let start = index + start_offset;
        output.push_str(&raw[index..start]);

        let Some(end_offset) = raw[start + 1..].find('>') else {
            output.push_str(&raw[start..]);
            return output;
        };
        let end = start + 1 + end_offset;
        let placeholder = &raw[start + 1..end];
        if let Some(replacement) = resolve_discovery_placeholder(placeholder, wopi_src_encoded) {
            output.push_str(&replacement);
        }
        index = end + 1;
    }

    output.push_str(&raw[index..]);
    output
}

fn resolve_discovery_placeholder(placeholder: &str, wopi_src_encoded: &str) -> Option<String> {
    let trimmed = placeholder.trim();
    let (key, value) = trimmed.split_once('=')?;
    let key = key.trim();
    let value = value.trim().trim_end_matches('&').trim();
    if key.is_empty() {
        return None;
    }

    if key.eq_ignore_ascii_case("wopisrc") || value.eq_ignore_ascii_case("wopi_source") {
        return Some(format!("{key}={wopi_src_encoded}&"));
    }

    None
}

fn append_wopi_src_if_missing(url: &str, wopi_src: &str) -> Result<String> {
    let parsed = Url::parse(url).map_err(|error| {
        AsterError::validation_error(format!("invalid WOPI action URL: {error}"))
    })?;
    let has_wopi_src = parsed
        .query_pairs()
        .any(|(key, _)| key.as_ref().eq_ignore_ascii_case("wopisrc"));
    if has_wopi_src {
        return Ok(parsed.to_string());
    }

    append_wopi_src(url, wopi_src)
}

pub(crate) fn append_wopi_src(url: &str, wopi_src: &str) -> Result<String> {
    let mut parsed = Url::parse(url).map_err(|error| {
        AsterError::validation_error(format!("invalid WOPI action URL: {error}"))
    })?;
    parsed.query_pairs_mut().append_pair("WOPISrc", wopi_src);
    Ok(parsed.to_string())
}

pub(super) fn origin_from_url(raw: &str) -> Option<String> {
    let parsed = Url::parse(raw.trim()).ok()?;
    let scheme = parsed.scheme().to_ascii_lowercase();
    let host = parsed.host_str()?.to_ascii_lowercase();
    let port = parsed
        .port()
        .map(|port| format!(":{port}"))
        .unwrap_or_default();
    cors::normalize_origin(&format!("{scheme}://{host}{port}"), false).ok()
}

pub(crate) fn trusted_origins_for_app(app: &apps::PublicPreviewAppDefinition) -> Vec<String> {
    let mut origins = Vec::new();

    for origin in &app.config.allowed_origins {
        if let Ok(origin) = cors::normalize_origin(origin, false) {
            push_unique(&mut origins, origin);
        }
    }

    for raw in [
        app.config.action_url.as_deref(),
        app.config.action_url_template.as_deref(),
        app.config.discovery_url.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(origin) = origin_from_url(raw) {
            push_unique(&mut origins, origin);
        }
    }

    origins
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

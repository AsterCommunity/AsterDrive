//! Shared HTTP validator helpers.

use std::time::{SystemTime, UNIX_EPOCH};

use actix_web::http::header;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EtagListError {
    Empty,
}

pub(crate) fn format_http_date(time: SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(time)
        .format("%a, %d %b %Y %H:%M:%S GMT")
        .to_string()
}

pub(crate) fn parse_http_date(value: &str) -> Result<SystemTime, ()> {
    value
        .parse::<header::HttpDate>()
        .map(SystemTime::from)
        .map_err(|_| ())
}

pub(crate) fn http_date_epoch_seconds(time: SystemTime) -> i128 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => i128::from(duration.as_secs()),
        Err(error) => -i128::from(error.duration().as_secs()),
    }
}

pub(crate) fn if_match_header_matches(
    raw: &str,
    resource_exists: bool,
    current_etag: Option<&str>,
) -> Result<bool, EtagListError> {
    let raw = raw.trim();
    if raw == "*" {
        return Ok(resource_exists);
    }
    let Some(current_etag) = current_etag else {
        return Ok(false);
    };
    let mut saw_tag = false;
    for candidate in raw
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        saw_tag = true;
        if is_weak_etag(candidate) {
            continue;
        }
        if strong_etag_matches(candidate, current_etag) {
            return Ok(true);
        }
    }
    if saw_tag {
        Ok(false)
    } else {
        Err(EtagListError::Empty)
    }
}

pub(crate) fn if_none_match_header_matches(
    raw: &str,
    resource_exists: bool,
    current_etag: Option<&str>,
) -> Result<bool, EtagListError> {
    let raw = raw.trim();
    if raw == "*" {
        return Ok(resource_exists);
    }
    let Some(current_etag) = current_etag else {
        return Ok(false);
    };
    let mut saw_tag = false;
    for candidate in raw
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        saw_tag = true;
        if etag_matches(candidate, current_etag) {
            return Ok(true);
        }
    }
    if saw_tag {
        Ok(false)
    } else {
        Err(EtagListError::Empty)
    }
}

fn etag_matches(header_value: &str, current_etag: &str) -> bool {
    let header_value = strip_weak_etag_prefix(header_value.trim());
    let current = strip_weak_etag_prefix(current_etag.trim());
    let header_value = strip_etag_quotes(header_value);
    let current = strip_etag_quotes(current);
    header_value == current
}

fn strong_etag_matches(candidate: &str, current_etag: &str) -> bool {
    if is_weak_etag(current_etag) {
        return false;
    }
    strip_etag_quotes(candidate.trim()) == strip_etag_quotes(current_etag.trim())
}

fn is_weak_etag(value: &str) -> bool {
    value
        .trim()
        .get(..2)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("W/"))
}

fn strip_weak_etag_prefix(value: &str) -> &str {
    value
        .strip_prefix("W/")
        .or_else(|| value.strip_prefix("w/"))
        .unwrap_or(value)
}

fn strip_etag_quotes(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::{
        EtagListError, if_match_header_matches, if_none_match_header_matches, parse_http_date,
    };

    #[test]
    fn if_none_match_uses_weak_comparison() {
        assert_eq!(
            if_none_match_header_matches(r#"W/"etag-1", "etag-2""#, true, Some(r#""etag-1""#)),
            Ok(true)
        );
    }

    #[test]
    fn if_match_requires_strong_comparison() {
        assert_eq!(
            if_match_header_matches(r#"W/"etag-1""#, true, Some(r#""etag-1""#)),
            Ok(false)
        );
        assert_eq!(
            if_match_header_matches(r#""etag-1""#, true, Some(r#""etag-1""#)),
            Ok(true)
        );
    }

    #[test]
    fn empty_etag_lists_are_invalid() {
        assert_eq!(
            if_none_match_header_matches(" , ", true, Some("etag")),
            Err(EtagListError::Empty)
        );
        assert_eq!(
            if_match_header_matches(" , ", true, Some("etag")),
            Err(EtagListError::Empty)
        );
    }

    #[test]
    fn parses_http_date() {
        assert!(parse_http_date("Sun, 06 Nov 1994 08:49:37 GMT").is_ok());
        assert!(parse_http_date("not a date").is_err());
    }
}

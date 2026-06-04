//! WebDAV protocol parsing helpers.

use actix_web::HttpResponse;
use actix_web::http::header;

use crate::webdav::decode_relative_path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Depth {
    Zero,
    One,
    Infinity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IfCondition {
    pub(crate) tagged_path: Option<String>,
    pub(crate) tokens: Vec<String>,
}

impl Depth {
    pub(crate) fn is_infinity(self) -> bool {
        matches!(self, Self::Infinity)
    }
}

pub(crate) fn parse_propfind_depth(headers: &header::HeaderMap) -> Result<Depth, HttpResponse> {
    match parse_depth_header(headers)? {
        Some(Depth::Zero) => Ok(Depth::Zero),
        Some(Depth::One) => Ok(Depth::One),
        Some(Depth::Infinity) | None => Err(HttpResponse::NotImplemented().finish()),
    }
}

pub(crate) fn parse_copy_depth(headers: &header::HeaderMap) -> Result<Depth, HttpResponse> {
    match parse_depth_header(headers)? {
        Some(Depth::Zero) => Ok(Depth::Zero),
        Some(Depth::Infinity) | None => Ok(Depth::Infinity),
        Some(Depth::One) => Err(HttpResponse::BadRequest().finish()),
    }
}

pub(crate) fn parse_move_depth(headers: &header::HeaderMap) -> Result<Depth, HttpResponse> {
    match parse_depth_header(headers)? {
        Some(Depth::Infinity) | None => Ok(Depth::Infinity),
        Some(Depth::Zero | Depth::One) => Err(HttpResponse::BadRequest().finish()),
    }
}

pub(crate) fn parse_delete_depth(headers: &header::HeaderMap) -> Result<Depth, HttpResponse> {
    match parse_depth_header(headers)? {
        None | Some(Depth::Infinity) => Ok(Depth::Infinity),
        Some(Depth::Zero | Depth::One) => Err(HttpResponse::BadRequest().finish()),
    }
}

pub(crate) fn parse_lock_depth(headers: &header::HeaderMap) -> Result<Depth, HttpResponse> {
    match parse_depth_header(headers)? {
        None | Some(Depth::Infinity) => Ok(Depth::Infinity),
        Some(Depth::Zero) => Ok(Depth::Zero),
        Some(Depth::One) => Err(HttpResponse::BadRequest().finish()),
    }
}

fn parse_depth_header(headers: &header::HeaderMap) -> Result<Option<Depth>, HttpResponse> {
    match headers.get("Depth").and_then(|value| value.to_str().ok()) {
        None => Ok(None),
        Some(value) if value.eq_ignore_ascii_case("0") => Ok(Some(Depth::Zero)),
        Some(value) if value.eq_ignore_ascii_case("1") => Ok(Some(Depth::One)),
        Some(value) if value.eq_ignore_ascii_case("infinity") => Ok(Some(Depth::Infinity)),
        Some(_) => Err(HttpResponse::BadRequest().finish()),
    }
}

pub(crate) fn overwrite_enabled(headers: &header::HeaderMap) -> bool {
    headers
        .get("Overwrite")
        .and_then(|value| value.to_str().ok())
        .is_none_or(|value| !value.eq_ignore_ascii_case("F"))
}

pub(crate) fn destination_relative_path(
    headers: &header::HeaderMap,
    prefix: &str,
) -> Result<String, HttpResponse> {
    let raw = headers
        .get("Destination")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| HttpResponse::BadRequest().body("Missing Destination header"))?;
    let path = if raw.starts_with("http://") || raw.starts_with("https://") {
        let uri: http::Uri = raw
            .parse()
            .map_err(|_| HttpResponse::BadRequest().body("Invalid Destination header"))?;
        uri.path().to_string()
    } else {
        raw.to_string()
    };
    let relative = path
        .strip_prefix(prefix)
        .filter(|_| {
            path == prefix
                || path
                    .as_bytes()
                    .get(prefix.len())
                    .is_some_and(|byte| *byte == b'/')
        })
        .ok_or_else(|| {
            HttpResponse::BadRequest().body("Destination must stay under WebDAV prefix")
        })?;
    decode_relative_path(relative).map(|(_, relative)| relative)
}

pub(crate) fn submitted_lock_tokens(headers: &header::HeaderMap) -> Vec<String> {
    let mut tokens = Vec::new();

    if let Some(token) = headers
        .get("Lock-Token")
        .and_then(|value| value.to_str().ok())
        .map(normalize_lock_token)
        .filter(|token| !token.is_empty())
    {
        tokens.push(token);
    }

    for condition in parse_if_conditions(headers) {
        tokens.extend(condition.tokens);
    }

    tokens.sort();
    tokens.dedup();
    tokens
}

pub(crate) fn parse_if_conditions(headers: &header::HeaderMap) -> Vec<IfCondition> {
    let Some(raw) = headers.get("If").and_then(|value| value.to_str().ok()) else {
        return Vec::new();
    };

    let mut conditions = Vec::new();
    let mut current_tag = None;
    let mut rest = raw.trim();
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('<') {
            let Some(end) = after.find('>') else {
                break;
            };
            let tag = after[..end].trim();
            current_tag = (!tag.is_empty()).then(|| tag.to_string());
            rest = after[end + 1..].trim_start();
            continue;
        }

        if let Some(after) = rest.strip_prefix('(') {
            let Some(end) = after.find(')') else {
                break;
            };
            let list = &after[..end];
            let tokens = lock_tokens_from_condition_list(list);
            if !tokens.is_empty() {
                conditions.push(IfCondition {
                    tagged_path: current_tag.clone(),
                    tokens,
                });
            }
            rest = after[end + 1..].trim_start();
            continue;
        }

        break;
    }

    conditions
}

fn lock_tokens_from_condition_list(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut rest = value;
    while let Some(start) = rest.find('<') {
        let next = &rest[start + 1..];
        let Some(end) = next.find('>') else {
            break;
        };
        let token = normalize_lock_token(&next[..end]);
        tokens.push(token);
        rest = &next[end + 1..];
    }
    tokens
}

fn normalize_lock_token(value: &str) -> String {
    value
        .trim()
        .trim_matches(|c| c == '<' || c == '>')
        .to_string()
}

#[cfg(test)]
mod tests {
    use actix_web::http::header::{HeaderMap, HeaderName, HeaderValue};

    use super::{
        Depth, parse_copy_depth, parse_delete_depth, parse_if_conditions, parse_move_depth,
        parse_propfind_depth, submitted_lock_tokens,
    };

    fn headers(name: &'static str, value: &'static str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_bytes(name.as_bytes()).expect("test header name should be valid"),
            HeaderValue::from_static(value),
        );
        headers
    }

    #[test]
    fn propfind_missing_depth_is_explicitly_unsupported_instead_of_depth_zero() {
        let headers = HeaderMap::new();

        assert!(parse_propfind_depth(&headers).is_err());
    }

    #[test]
    fn propfind_accepts_zero_and_one_depth() {
        assert_eq!(
            parse_propfind_depth(&headers("Depth", "0")).unwrap(),
            Depth::Zero
        );
        assert_eq!(
            parse_propfind_depth(&headers("Depth", "1")).unwrap(),
            Depth::One
        );
    }

    #[test]
    fn copy_defaults_to_infinity_and_accepts_zero() {
        assert_eq!(
            parse_copy_depth(&HeaderMap::new()).unwrap(),
            Depth::Infinity
        );
        assert_eq!(
            parse_copy_depth(&headers("Depth", "0")).unwrap(),
            Depth::Zero
        );
    }

    #[test]
    fn move_and_delete_reject_non_infinity_depth() {
        assert!(parse_move_depth(&headers("Depth", "0")).is_err());
        assert!(parse_delete_depth(&headers("Depth", "1")).is_err());
    }

    #[test]
    fn if_header_extracts_tagged_condition_tokens() {
        let conditions = parse_if_conditions(&headers(
            "If",
            r#"</webdav/a.txt> (<urn:uuid:one> ["etag"]) (Not <urn:uuid:two>)"#,
        ));

        assert_eq!(conditions.len(), 2);
        assert_eq!(conditions[0].tagged_path.as_deref(), Some("/webdav/a.txt"));
        assert_eq!(conditions[0].tokens, ["urn:uuid:one"]);
        assert_eq!(conditions[1].tokens, ["urn:uuid:two"]);
    }

    #[test]
    fn submitted_tokens_include_lock_token_and_if_tokens() {
        let mut headers = headers("If", "(<urn:uuid:two>)");
        headers.insert(
            HeaderName::from_static("lock-token"),
            HeaderValue::from_static("<urn:uuid:one>"),
        );

        assert_eq!(
            submitted_lock_tokens(&headers),
            ["urn:uuid:one".to_string(), "urn:uuid:two".to_string()]
        );
    }
}

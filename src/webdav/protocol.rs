//! WebDAV protocol parsing helpers.

use actix_web::HttpResponse;
use actix_web::http::header;

use crate::webdav::dav::{DavFileSystem, DavLockSystem, DavPath, FsError};
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IfHeader {
    groups: Vec<IfResourceGroup>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HttpEtagPrecondition {
    Proceed,
    NotModified,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IfResourceGroup {
    tagged_path: Option<String>,
    lists: Vec<IfStateList>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IfStateList {
    conditions: Vec<IfStateCondition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum IfStateCondition {
    Token { value: String, negated: bool },
    Etag { value: String, negated: bool },
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
        Some(Depth::Infinity) | None => Ok(Depth::Infinity),
    }
}

pub(crate) fn parse_copy_depth(headers: &header::HeaderMap) -> Result<Depth, HttpResponse> {
    match parse_depth_header(headers)? {
        Some(Depth::Zero) => Ok(Depth::Zero),
        Some(Depth::Infinity) | None => Ok(Depth::Infinity),
        Some(Depth::One) => Ok(Depth::One),
    }
}

pub(crate) fn parse_move_depth(headers: &header::HeaderMap) -> Result<Depth, HttpResponse> {
    Ok(parse_depth_header(headers)?.unwrap_or(Depth::Infinity))
}

pub(crate) fn parse_delete_depth(headers: &header::HeaderMap) -> Result<Depth, HttpResponse> {
    Ok(parse_depth_header(headers)?.unwrap_or(Depth::Infinity))
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

pub(crate) fn parse_overwrite(headers: &header::HeaderMap) -> Result<bool, HttpResponse> {
    let Some(value) = headers.get("Overwrite") else {
        return Ok(true);
    };
    let value = value
        .to_str()
        .map_err(|_| HttpResponse::BadRequest().body("Invalid Overwrite header"))?
        .trim();
    if value.eq_ignore_ascii_case("T") {
        Ok(true)
    } else if value.eq_ignore_ascii_case("F") {
        Ok(false)
    } else {
        Err(HttpResponse::BadRequest().body("Invalid Overwrite header"))
    }
}

pub(crate) fn destination_relative_path(
    headers: &header::HeaderMap,
    prefix: &str,
    request_scheme: &str,
    request_host: &str,
) -> Result<String, HttpResponse> {
    let raw = headers
        .get("Destination")
        .ok_or_else(|| HttpResponse::BadRequest().body("Missing Destination header"))?
        .to_str()
        .map_err(|_| HttpResponse::BadRequest().body("Invalid Destination header"))?
        .trim();
    let uri: http::Uri = raw
        .parse()
        .map_err(|_| HttpResponse::BadRequest().body("Invalid Destination header"))?;
    match (uri.scheme_str(), uri.authority()) {
        (Some(scheme), Some(authority)) => {
            if !scheme.eq_ignore_ascii_case(request_scheme)
                || !authority.as_str().eq_ignore_ascii_case(request_host)
            {
                return Err(
                    HttpResponse::BadGateway().body("Destination must stay on this WebDAV server")
                );
            }
        }
        (None, None) => {
            if !raw.starts_with('/') {
                return Err(HttpResponse::BadRequest().body("Invalid Destination header"));
            }
        }
        _ => return Err(HttpResponse::BadRequest().body("Invalid Destination header")),
    };
    let path = uri.path().to_string();
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

#[cfg(test)]
fn submitted_lock_tokens(headers: &header::HeaderMap) -> Vec<String> {
    let mut tokens = Vec::new();

    for condition in parse_if_conditions(headers) {
        tokens.extend(condition.tokens);
    }

    tokens.sort();
    tokens.dedup();
    tokens
}

pub(crate) fn submitted_lock_tokens_for_path(
    headers: &header::HeaderMap,
    request_path: &str,
    request_scheme: &str,
    request_host: &str,
) -> Vec<String> {
    let mut tokens = Vec::new();

    for condition in parse_if_conditions(headers) {
        match condition.tagged_path.as_deref() {
            None => tokens.extend(condition.tokens),
            Some(tagged_path)
                if if_tag_matches_path(tagged_path, request_path, request_scheme, request_host) =>
            {
                tokens.extend(condition.tokens);
            }
            Some(_) => {}
        }
    }

    tokens.sort();
    tokens.dedup();
    tokens
}

pub(crate) async fn ensure_if_header(
    headers: &header::HeaderMap,
    dav_fs: &dyn DavFileSystem,
    lock_system: &dyn DavLockSystem,
    request_path: &DavPath,
    prefix: &str,
    request_scheme: &str,
    request_host: &str,
) -> Result<(), HttpResponse> {
    let Some(if_header) = parse_if_header(headers)? else {
        return Ok(());
    };

    if evaluate_if_header(
        &if_header,
        dav_fs,
        lock_system,
        request_path,
        prefix,
        request_scheme,
        request_host,
    )
    .await?
    {
        Ok(())
    } else {
        Err(HttpResponse::PreconditionFailed().finish())
    }
}

pub(crate) fn evaluate_http_etag_preconditions(
    headers: &header::HeaderMap,
    resource_exists: bool,
    current_etag: Option<&str>,
    safe_method: bool,
) -> Result<HttpEtagPrecondition, HttpResponse> {
    if let Some(value) = headers.get(header::IF_MATCH) {
        let raw = value
            .to_str()
            .map_err(|_| HttpResponse::BadRequest().body("Invalid If-Match header"))?;
        if !if_match_header_matches(raw, resource_exists, current_etag)? {
            return Err(HttpResponse::PreconditionFailed().finish());
        }
    }

    if let Some(value) = headers.get(header::IF_NONE_MATCH) {
        let raw = value
            .to_str()
            .map_err(|_| HttpResponse::BadRequest().body("Invalid If-None-Match header"))?;
        if if_none_match_header_matches(raw, resource_exists, current_etag)? {
            return if safe_method {
                Ok(HttpEtagPrecondition::NotModified)
            } else {
                Err(HttpResponse::PreconditionFailed().finish())
            };
        }
    }

    Ok(HttpEtagPrecondition::Proceed)
}

async fn evaluate_if_header(
    if_header: &IfHeader,
    dav_fs: &dyn DavFileSystem,
    lock_system: &dyn DavLockSystem,
    request_path: &DavPath,
    prefix: &str,
    request_scheme: &str,
    request_host: &str,
) -> Result<bool, HttpResponse> {
    for group in &if_header.groups {
        let path = match group.tagged_path.as_deref() {
            Some(tagged_path) => {
                tagged_dav_path(prefix, tagged_path, request_scheme, request_host)?
            }
            None => Some(request_path.clone()),
        };
        if evaluate_if_resource_group(group, dav_fs, lock_system, path.as_ref()).await? {
            return Ok(true);
        }
    }

    Ok(false)
}

async fn evaluate_if_resource_group(
    group: &IfResourceGroup,
    dav_fs: &dyn DavFileSystem,
    lock_system: &dyn DavLockSystem,
    path: Option<&DavPath>,
) -> Result<bool, HttpResponse> {
    for list in &group.lists {
        if evaluate_if_state_list(list, dav_fs, lock_system, path).await? {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn evaluate_if_state_list(
    list: &IfStateList,
    dav_fs: &dyn DavFileSystem,
    lock_system: &dyn DavLockSystem,
    path: Option<&DavPath>,
) -> Result<bool, HttpResponse> {
    let current_etag = match path {
        Some(path) => match dav_fs.metadata(path).await {
            Ok(meta) => meta.etag(),
            Err(FsError::NotFound) => None,
            Err(err) => return Err(crate::webdav::fs_error_response(err)),
        },
        None => None,
    };
    let lock_tokens = match path {
        Some(path) => lock_system
            .discover(path)
            .await
            .into_iter()
            .map(|lock| lock.token)
            .collect::<Vec<_>>(),
        None => Vec::new(),
    };

    for condition in &list.conditions {
        let matched = match condition {
            IfStateCondition::Token { value, negated } => {
                lock_tokens.iter().any(|token| token == value) ^ *negated
            }
            IfStateCondition::Etag { value, negated } => {
                current_etag
                    .as_deref()
                    .is_some_and(|etag| etag_matches(value, etag))
                    ^ *negated
            }
        };
        if !matched {
            return Ok(false);
        }
    }

    Ok(true)
}

fn parse_if_header(headers: &header::HeaderMap) -> Result<Option<IfHeader>, HttpResponse> {
    let Some(value) = headers.get("If") else {
        return Ok(None);
    };
    let raw = value
        .to_str()
        .map_err(|_| HttpResponse::BadRequest().body("Invalid If header"))?;

    let mut parser = IfHeaderParser::new(raw);
    parser.parse().map(Some)
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

fn tagged_dav_path(
    prefix: &str,
    tagged_path: &str,
    request_scheme: &str,
    request_host: &str,
) -> Result<Option<DavPath>, HttpResponse> {
    let uri: http::Uri = tagged_path
        .parse()
        .map_err(|_| HttpResponse::BadRequest().body("Invalid If header"))?;
    let path = match (uri.scheme_str(), uri.authority()) {
        (Some(scheme), Some(authority)) => {
            if !scheme.eq_ignore_ascii_case(request_scheme)
                || !authority.as_str().eq_ignore_ascii_case(request_host)
            {
                return Ok(None);
            }
            uri.path().to_string()
        }
        (None, None) => uri.path().to_string(),
        _ => return Err(HttpResponse::BadRequest().body("Invalid If header")),
    };
    if !path.starts_with('/') {
        return Err(HttpResponse::BadRequest().body("Invalid If header"));
    }

    let Some(relative) = path.strip_prefix(prefix).filter(|_| {
        path == prefix
            || path
                .as_bytes()
                .get(prefix.len())
                .is_some_and(|byte| *byte == b'/')
    }) else {
        return Ok(None);
    };
    let (path, _) = decode_relative_path(relative)?;
    Ok(Some(path))
}

fn etag_matches(header_value: &str, current_etag: &str) -> bool {
    let header_value = strip_weak_etag_prefix(header_value.trim());
    let current = strip_weak_etag_prefix(current_etag.trim());
    let header_value = strip_etag_quotes(header_value);
    let current = strip_etag_quotes(current);
    header_value == current
}

fn if_match_header_matches(
    raw: &str,
    resource_exists: bool,
    current_etag: Option<&str>,
) -> Result<bool, HttpResponse> {
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
        Err(HttpResponse::BadRequest().body("Invalid If-Match header"))
    }
}

fn if_none_match_header_matches(
    raw: &str,
    resource_exists: bool,
    current_etag: Option<&str>,
) -> Result<bool, HttpResponse> {
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
        Err(HttpResponse::BadRequest().body("Invalid If-None-Match header"))
    }
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

struct IfHeaderParser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> IfHeaderParser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn parse(&mut self) -> Result<IfHeader, HttpResponse> {
        self.skip_lws();
        if self.is_eof() {
            return Err(HttpResponse::BadRequest().body("Invalid If header"));
        }

        let first_is_tagged = self.peek_char() == Some('<');
        let mut groups = Vec::new();
        if first_is_tagged {
            while !self.is_eof() {
                let tagged_path = self.parse_angle_value()?;
                let mut lists = Vec::new();
                loop {
                    self.skip_lws();
                    if self.peek_char() != Some('(') {
                        break;
                    }
                    lists.push(self.parse_state_list()?);
                }
                if lists.is_empty() {
                    return Err(HttpResponse::BadRequest().body("Invalid If header"));
                }
                groups.push(IfResourceGroup {
                    tagged_path: Some(tagged_path),
                    lists,
                });
                self.skip_lws();
                if self.is_eof() {
                    break;
                }
                if self.peek_char() != Some('<') {
                    return Err(HttpResponse::BadRequest().body("Invalid If header"));
                }
            }
        } else {
            let mut lists = Vec::new();
            while !self.is_eof() {
                lists.push(self.parse_state_list()?);
                self.skip_lws();
                if self.peek_char() == Some('<') {
                    return Err(HttpResponse::BadRequest().body("Invalid If header"));
                }
            }
            groups.push(IfResourceGroup {
                tagged_path: None,
                lists,
            });
        }

        Ok(IfHeader { groups })
    }

    fn parse_state_list(&mut self) -> Result<IfStateList, HttpResponse> {
        self.expect_char('(')?;
        let mut conditions = Vec::new();
        loop {
            self.skip_lws();
            if self.peek_char() == Some(')') {
                self.pos += 1;
                break;
            }
            if self.is_eof() {
                return Err(HttpResponse::BadRequest().body("Invalid If header"));
            }

            let negated = self.consume_not();
            self.skip_lws();
            let condition = match self.peek_char() {
                Some('<') => IfStateCondition::Token {
                    value: normalize_lock_token(&self.parse_angle_value()?),
                    negated,
                },
                Some('[') => IfStateCondition::Etag {
                    value: self.parse_bracket_value()?,
                    negated,
                },
                _ => return Err(HttpResponse::BadRequest().body("Invalid If header")),
            };
            conditions.push(condition);
        }

        if conditions.is_empty() {
            return Err(HttpResponse::BadRequest().body("Invalid If header"));
        }
        Ok(IfStateList { conditions })
    }

    fn parse_angle_value(&mut self) -> Result<String, HttpResponse> {
        self.expect_char('<')?;
        let start = self.pos;
        while let Some(ch) = self.peek_char() {
            if ch == '>' {
                let value = self.input[start..self.pos].trim();
                self.pos += 1;
                if value.is_empty() {
                    return Err(HttpResponse::BadRequest().body("Invalid If header"));
                }
                return Ok(value.to_string());
            }
            self.pos += ch.len_utf8();
        }
        Err(HttpResponse::BadRequest().body("Invalid If header"))
    }

    fn parse_bracket_value(&mut self) -> Result<String, HttpResponse> {
        self.expect_char('[')?;
        let start = self.pos;
        while let Some(ch) = self.peek_char() {
            if ch == ']' {
                let value = self.input[start..self.pos].trim();
                self.pos += 1;
                if value.is_empty() {
                    return Err(HttpResponse::BadRequest().body("Invalid If header"));
                }
                return Ok(value.to_string());
            }
            self.pos += ch.len_utf8();
        }
        Err(HttpResponse::BadRequest().body("Invalid If header"))
    }

    fn consume_not(&mut self) -> bool {
        let rest = &self.input[self.pos..];
        let Some(after_not) = rest.strip_prefix("Not") else {
            return false;
        };
        if after_not
            .chars()
            .next()
            .is_some_and(|ch| !ch.is_ascii_whitespace() && ch != '<' && ch != '[')
        {
            return false;
        }
        self.pos += "Not".len();
        true
    }

    fn expect_char(&mut self, expected: char) -> Result<(), HttpResponse> {
        if self.peek_char() == Some(expected) {
            self.pos += expected.len_utf8();
            Ok(())
        } else {
            Err(HttpResponse::BadRequest().body("Invalid If header"))
        }
    }

    fn skip_lws(&mut self) {
        while self
            .peek_char()
            .is_some_and(|ch| ch == ' ' || ch == '\t' || ch == '\r' || ch == '\n')
        {
            self.pos += 1;
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.input.len()
    }
}

fn if_tag_matches_path(
    tagged_path: &str,
    request_path: &str,
    request_scheme: &str,
    request_host: &str,
) -> bool {
    if path_equivalent(tagged_path, request_path) {
        return true;
    }
    let parsed = tagged_path.parse::<http::Uri>();
    let Ok(uri) = parsed else {
        return false;
    };
    match (uri.scheme_str(), uri.authority()) {
        (Some(scheme), Some(authority)) => {
            scheme.eq_ignore_ascii_case(request_scheme)
                && authority.as_str().eq_ignore_ascii_case(request_host)
                && path_equivalent(uri.path(), request_path)
        }
        (None, None) => path_equivalent(uri.path(), request_path),
        _ => false,
    }
}

fn path_equivalent(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    let left_decoded = urlencoding::decode(left).ok();
    let right_decoded = urlencoding::decode(right).ok();
    match (left_decoded.as_deref(), right_decoded.as_deref()) {
        (Some(left), Some(right)) => left == right,
        (Some(left), None) => left == right,
        (None, Some(right)) => left == right,
        (None, None) => false,
    }
}

#[cfg(test)]
mod tests {
    use actix_web::http::header::{HeaderMap, HeaderName, HeaderValue};

    use super::{
        Depth, IfHeader, IfStateCondition, parse_copy_depth, parse_delete_depth,
        parse_if_conditions, parse_if_header, parse_move_depth, parse_propfind_depth,
        submitted_lock_tokens, submitted_lock_tokens_for_path,
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
    fn propfind_missing_depth_defaults_to_infinity() {
        let headers = HeaderMap::new();

        assert_eq!(parse_propfind_depth(&headers).unwrap(), Depth::Infinity);
    }

    #[test]
    fn propfind_accepts_zero_one_and_infinity_depth() {
        assert_eq!(
            parse_propfind_depth(&headers("Depth", "0")).unwrap(),
            Depth::Zero
        );
        assert_eq!(
            parse_propfind_depth(&headers("Depth", "1")).unwrap(),
            Depth::One
        );
        assert_eq!(
            parse_propfind_depth(&headers("Depth", "infinity")).unwrap(),
            Depth::Infinity
        );
    }

    #[test]
    fn copy_defaults_to_infinity_and_parses_legal_depths() {
        assert_eq!(
            parse_copy_depth(&HeaderMap::new()).unwrap(),
            Depth::Infinity
        );
        assert_eq!(
            parse_copy_depth(&headers("Depth", "0")).unwrap(),
            Depth::Zero
        );
        assert_eq!(
            parse_copy_depth(&headers("Depth", "1")).unwrap(),
            Depth::One
        );
    }

    #[test]
    fn move_and_delete_parse_legal_depths_before_resource_type_is_known() {
        assert_eq!(
            parse_move_depth(&headers("Depth", "0")).unwrap(),
            Depth::Zero
        );
        assert_eq!(
            parse_delete_depth(&headers("Depth", "1")).unwrap(),
            Depth::One
        );
        assert!(parse_move_depth(&headers("Depth", "invalid")).is_err());
        assert!(parse_delete_depth(&headers("Depth", "invalid")).is_err());
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
    fn submitted_tokens_ignore_lock_token_header() {
        let mut headers = headers("If", "(<urn:uuid:two>)");
        headers.insert(
            HeaderName::from_static("lock-token"),
            HeaderValue::from_static("<urn:uuid:one>"),
        );

        assert_eq!(
            submitted_lock_tokens(&headers),
            ["urn:uuid:two".to_string()]
        );
    }

    #[test]
    fn submitted_tokens_for_path_ignore_other_tagged_resources() {
        let mut headers = headers(
            "If",
            r#"</webdav/other.txt> (<urn:uuid:other>) </webdav/current.txt> (<urn:uuid:current>) (<urn:uuid:untagged>)"#,
        );
        headers.insert(
            HeaderName::from_static("lock-token"),
            HeaderValue::from_static("<urn:uuid:header>"),
        );

        assert_eq!(
            submitted_lock_tokens_for_path(&headers, "/webdav/current.txt", "http", "localhost"),
            [
                "urn:uuid:current".to_string(),
                "urn:uuid:untagged".to_string()
            ]
        );
    }

    #[test]
    fn submitted_tokens_for_path_honor_absolute_tag_origin() {
        let headers = headers(
            "If",
            r#"<http://localhost:8080/webdav/current.txt> (<urn:uuid:current>) <http://remote.example/webdav/current.txt> (<urn:uuid:remote>)"#,
        );

        assert_eq!(
            submitted_lock_tokens_for_path(
                &headers,
                "/webdav/current.txt",
                "http",
                "localhost:8080"
            ),
            ["urn:uuid:current".to_string()]
        );
    }

    #[test]
    fn if_header_parser_preserves_not_and_etag_conditions() {
        let headers = headers(
            "If",
            r#"(<urn:uuid:one> ["etag-one"]) (Not <urn:uuid:two> [W/"etag-two"])"#,
        );
        let parsed = parse_if_header(&headers)
            .expect("If header should parse")
            .expect("If header should exist");

        assert_eq!(parsed.groups.len(), 1);
        assert_eq!(parsed.groups[0].lists.len(), 2);
        assert_eq!(
            parsed.groups[0].lists[0].conditions,
            [
                IfStateCondition::Token {
                    value: "urn:uuid:one".to_string(),
                    negated: false,
                },
                IfStateCondition::Etag {
                    value: "\"etag-one\"".to_string(),
                    negated: false,
                },
            ]
        );
        assert_eq!(
            parsed.groups[0].lists[1].conditions,
            [
                IfStateCondition::Token {
                    value: "urn:uuid:two".to_string(),
                    negated: true,
                },
                IfStateCondition::Etag {
                    value: "W/\"etag-two\"".to_string(),
                    negated: false,
                },
            ]
        );
    }

    #[test]
    fn if_header_parser_rejects_mixed_tagged_and_untagged_lists() {
        let headers = headers(
            "If",
            r#"(<urn:uuid:one>) </webdav/file.txt> (<urn:uuid:two>)"#,
        );

        assert!(parse_if_header(&headers).is_err());
    }

    #[test]
    fn if_header_parser_rejects_empty_lists() {
        let headers = headers("If", "()");

        assert!(parse_if_header(&headers).is_err());
    }

    #[test]
    fn if_header_parser_groups_tagged_lists_by_resource() {
        let parsed = parse_if_header(&headers(
            "If",
            r#"</webdav/a.txt> (<urn:uuid:a1>) (<urn:uuid:a2>) </webdav/b.txt> (Not <urn:uuid:b>)"#,
        ))
        .expect("If header should parse")
        .expect("If header should exist");

        let IfHeader { groups } = parsed;
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].tagged_path.as_deref(), Some("/webdav/a.txt"));
        assert_eq!(groups[0].lists.len(), 2);
        assert_eq!(groups[1].tagged_path.as_deref(), Some("/webdav/b.txt"));
        assert_eq!(groups[1].lists.len(), 1);
    }
}

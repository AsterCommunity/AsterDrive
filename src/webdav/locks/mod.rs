//! WebDAV LOCK / UNLOCK handlers and lock XML helpers.

use std::io::Cursor;
use std::time::Duration;

use actix_web::http::{StatusCode, header};
use actix_web::{HttpRequest, HttpResponse};
use xmltree::{Element, XMLNode};

use crate::webdav::dav::{DavFileSystem, DavLock, DavLockSystem, FsError, OpenOptions};
use crate::webdav::protocol::{self, Depth};
use crate::webdav::{
    child_elements, dav_element, encode_href, fs, fs_error_response, href_for_dav_path,
    lock_token_matches_request_uri_response, lock_token_submitted_response, request_origin,
    request_path, responses, text_element,
};

const MAX_LOCK_DURATION_SECS: u64 = 604_800;

pub(crate) async fn handle_lock(
    req: &HttpRequest,
    dav_fs: &fs::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    body: &[u8],
) -> HttpResponse {
    let (path, _) = match request_path(req, prefix) {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    if body.is_empty() {
        let timeout = match parse_timeout(req.headers()) {
            Ok(timeout) => timeout,
            Err(resp) => return resp,
        };
        let (request_scheme, request_host) = request_origin(req);
        if let Err(resp) = protocol::ensure_if_header(
            req.headers(),
            dav_fs,
            lock_system,
            &path,
            prefix,
            &request_scheme,
            &request_host,
        )
        .await
        {
            return resp;
        }
        let request_href = href_for_dav_path(prefix, &path);
        let tokens = protocol::submitted_lock_tokens_for_path(
            req.headers(),
            &request_href,
            &request_scheme,
            &request_host,
        );
        if tokens.len() != 1 {
            return responses::bad_request();
        }
        if lock_system
            .check(&path, None, false, false, &tokens)
            .await
            .is_err()
        {
            return responses::precondition_failed();
        }
        let lock = match lock_system.refresh(&path, &tokens[0], timeout).await {
            Ok(lock) => lock,
            Err(_) => return responses::precondition_failed(),
        };
        return lock_response(lock, StatusCode::OK, prefix, false);
    }

    let timeout = match parse_timeout(req.headers()) {
        Ok(timeout) => timeout,
        Err(resp) => return resp,
    };
    let depth = match protocol::parse_lock_depth(req.headers()) {
        Ok(depth) => depth,
        Err(resp) => return resp,
    };

    let tree = match Element::parse(Cursor::new(body)) {
        Ok(tree) => tree,
        Err(_) => return responses::invalid_xml_body(),
    };
    if !is_dav_element(&tree, "lockinfo") {
        return invalid_lock_body();
    }

    let mut shared = None;
    let mut owner = None;
    let mut write_lock = false;
    for elem in child_elements(&tree) {
        match elem.name.as_str() {
            "lockscope" if is_dav_element(elem, "lockscope") => {
                let children = child_elements(elem).collect::<Vec<_>>();
                if children.len() != 1 {
                    return responses::bad_request();
                }
                let scope = children.first().map(|child| child.name.as_str());
                match scope {
                    Some("exclusive") if is_dav_element(children[0], "exclusive") => {
                        shared = Some(false)
                    }
                    Some("shared") if is_dav_element(children[0], "shared") => shared = Some(true),
                    _ => return responses::bad_request(),
                }
            }
            "locktype" if is_dav_element(elem, "locktype") => {
                let children = child_elements(elem).collect::<Vec<_>>();
                if children.len() != 1 || !is_dav_element(children[0], "write") {
                    return responses::bad_request();
                }
                write_lock = true;
            }
            "owner" if is_dav_element(elem, "owner") => owner = Some(elem.clone()),
            _ => return responses::bad_request(),
        }
    }
    if shared.is_none() || !write_lock {
        return responses::bad_request();
    }

    let resource_existed = match ensure_lock_target_exists(dav_fs, &path, depth).await {
        Ok(resource_existed) => resource_existed,
        Err(err) => return fs_error_response(err),
    };

    let lock = match lock_system
        .lock(
            &path,
            None,
            owner.as_ref(),
            timeout,
            shared.unwrap_or(false),
            depth.is_infinity(),
        )
        .await
    {
        Ok(lock) => lock,
        Err(lock) => {
            return lock_token_submitted_response(StatusCode::LOCKED, prefix, &lock.path);
        }
    };

    let status = if resource_existed {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    lock_response(lock, status, prefix, true)
}

pub(crate) async fn handle_unlock(
    req: &HttpRequest,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
) -> HttpResponse {
    let (path, _) = match request_path(req, prefix) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let token = match req.headers().get("Lock-Token") {
        Some(token) => match parse_lock_token_header(token) {
            Ok(token) => token,
            Err(resp) => return resp,
        },
        None => return responses::bad_request(),
    };

    match lock_system.unlock(&path, &token).await {
        Ok(()) => responses::no_store(StatusCode::NO_CONTENT),
        Err(()) => lock_token_matches_request_uri_response(StatusCode::CONFLICT),
    }
}

fn parse_lock_token_header(value: &header::HeaderValue) -> Result<String, HttpResponse> {
    let raw = value
        .to_str()
        .map_err(|_| invalid_lock_token_header())?
        .trim();
    let Some(token) = raw
        .strip_prefix('<')
        .and_then(|value| value.strip_suffix('>'))
    else {
        return Err(invalid_lock_token_header());
    };
    if token.is_empty() || token.contains(['<', '>']) {
        return Err(invalid_lock_token_header());
    }
    Ok(token.to_string())
}

fn parse_timeout(headers: &header::HeaderMap) -> Result<Option<Duration>, HttpResponse> {
    let Some(raw) = headers.get("Timeout") else {
        return Ok(Some(Duration::from_secs(MAX_LOCK_DURATION_SECS)));
    };
    let raw = raw.to_str().map_err(|_| invalid_timeout_header())?;
    for candidate in raw
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if candidate.eq_ignore_ascii_case("Infinite") {
            return Ok(Some(Duration::from_secs(MAX_LOCK_DURATION_SECS)));
        }
        if let Some(seconds) = candidate
            .strip_prefix("Second-")
            .and_then(|seconds| seconds.parse::<u64>().ok())
        {
            if seconds > MAX_LOCK_DURATION_SECS {
                return Err(invalid_timeout_header());
            }
            return Ok(Some(Duration::from_secs(seconds)));
        }
    }
    Err(invalid_timeout_header())
}

fn invalid_lock_body() -> HttpResponse {
    responses::bad_request_text("Invalid LOCK body")
}

fn invalid_lock_token_header() -> HttpResponse {
    responses::bad_request_text("Invalid Lock-Token header")
}

fn invalid_timeout_header() -> HttpResponse {
    responses::bad_request_text("Invalid Timeout header")
}

async fn ensure_lock_target_exists(
    dav_fs: &fs::AsterDavFs,
    path: &crate::webdav::dav::DavPath,
    depth: Depth,
) -> Result<bool, FsError> {
    let _ = depth;
    match dav_fs.metadata(path).await {
        Ok(_) => Ok(true),
        Err(FsError::NotFound) if !path.is_collection() => {
            let mut file = dav_fs
                .open(
                    path,
                    OpenOptions {
                        write: true,
                        create: true,
                        truncate: true,
                        size: Some(0),
                        ..OpenOptions::default()
                    },
                )
                .await?;
            file.flush().await?;
            Ok(false)
        }
        Err(FsError::NotFound) => Err(FsError::NotFound),
        Err(err) => Err(err),
    }
}

pub(crate) fn supportedlock_element() -> Element {
    let mut supported = dav_element("supportedlock");

    let mut exclusive = dav_element("lockentry");
    let mut exclusive_scope = dav_element("lockscope");
    exclusive_scope
        .children
        .push(XMLNode::Element(dav_element("exclusive")));
    exclusive.children.push(XMLNode::Element(exclusive_scope));
    let mut exclusive_type = dav_element("locktype");
    exclusive_type
        .children
        .push(XMLNode::Element(dav_element("write")));
    exclusive.children.push(XMLNode::Element(exclusive_type));
    supported.children.push(XMLNode::Element(exclusive));

    let mut shared = dav_element("lockentry");
    let mut shared_scope = dav_element("lockscope");
    shared_scope
        .children
        .push(XMLNode::Element(dav_element("shared")));
    shared.children.push(XMLNode::Element(shared_scope));
    let mut shared_type = dav_element("locktype");
    shared_type
        .children
        .push(XMLNode::Element(dav_element("write")));
    shared.children.push(XMLNode::Element(shared_type));
    supported.children.push(XMLNode::Element(shared));

    supported
}

pub(crate) fn lockdiscovery_element(locks: &[DavLock], prefix: &str) -> Element {
    let mut discovery = dav_element("lockdiscovery");
    for lock in locks {
        discovery
            .children
            .push(XMLNode::Element(active_lock_element(lock, prefix)));
    }
    discovery
}

fn active_lock_element(lock: &DavLock, prefix: &str) -> Element {
    let mut active = dav_element("activelock");

    let mut lockscope = dav_element("lockscope");
    lockscope.children.push(XMLNode::Element(if lock.shared {
        dav_element("shared")
    } else {
        dav_element("exclusive")
    }));
    active.children.push(XMLNode::Element(lockscope));

    let mut locktype = dav_element("locktype");
    locktype
        .children
        .push(XMLNode::Element(dav_element("write")));
    active.children.push(XMLNode::Element(locktype));

    if let Some(owner) = &lock.owner {
        active.children.push(XMLNode::Element((**owner).clone()));
    }

    let mut timeout = dav_element("timeout");
    let timeout_value = lock
        .timeout
        .map(|duration| format!("Second-{}", duration.as_secs()))
        .unwrap_or_else(|| "Infinite".to_string());
    timeout.children.push(XMLNode::Text(timeout_value));
    active.children.push(XMLNode::Element(timeout));

    let mut token = dav_element("locktoken");
    token.children.push(XMLNode::Element(text_element(
        "D:href",
        &encode_href(&lock.token),
    )));
    active.children.push(XMLNode::Element(token));

    let mut depth = dav_element("depth");
    depth.children.push(XMLNode::Text(if lock.deep {
        "Infinity".to_string()
    } else {
        "0".to_string()
    }));
    active.children.push(XMLNode::Element(depth));

    let mut lockroot = dav_element("lockroot");
    lockroot.children.push(XMLNode::Element(text_element(
        "D:href",
        &href_for_dav_path(prefix, &lock.path),
    )));
    active.children.push(XMLNode::Element(lockroot));

    active
}

fn lock_response(
    lock: DavLock,
    status: StatusCode,
    prefix: &str,
    include_lock_token_header: bool,
) -> HttpResponse {
    let mut prop = dav_element("prop");
    prop.attributes
        .insert("xmlns:D".to_string(), "DAV:".to_string());
    prop.children.push(XMLNode::Element(lockdiscovery_element(
        std::slice::from_ref(&lock),
        prefix,
    )));

    let (mut response, body) = match responses::xml_response_builder(prop, status) {
        Ok(parts) => parts,
        Err(resp) => return resp,
    };

    if include_lock_token_header {
        response.insert_header(("Lock-Token", format!("<{}>", lock.token)));
    }
    response.body(body)
}

fn is_dav_element(element: &Element, local_name: &str) -> bool {
    element.name == local_name && element.namespace.as_deref() == Some("DAV:")
}

#[cfg(test)]
mod tests {
    use super::{MAX_LOCK_DURATION_SECS, parse_timeout};
    use actix_web::http::header::{HeaderMap, HeaderName, HeaderValue};
    use std::time::Duration;

    fn timeout_header(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("timeout"),
            HeaderValue::from_str(value).expect("timeout test header should parse"),
        );
        headers
    }

    #[test]
    fn parse_timeout_defaults_to_server_max_duration() {
        let headers = HeaderMap::new();

        let timeout = parse_timeout(&headers)
            .expect("missing Timeout should use default")
            .expect("missing Timeout should not create an infinite lock");

        assert_eq!(timeout, Duration::from_secs(MAX_LOCK_DURATION_SECS));
    }

    #[test]
    fn parse_timeout_clamps_infinite_to_server_max_duration() {
        let headers = timeout_header("Infinite");

        let timeout = parse_timeout(&headers)
            .expect("Infinite should be accepted")
            .expect("Infinite should be clamped instead of stored as None");

        assert_eq!(timeout, Duration::from_secs(MAX_LOCK_DURATION_SECS));
    }

    #[test]
    fn parse_timeout_accepts_exact_server_max_duration() {
        let headers = timeout_header("Second-604800");

        let timeout = parse_timeout(&headers)
            .expect("exact max timeout should be accepted")
            .expect("exact max timeout should produce a duration");

        assert_eq!(timeout, Duration::from_secs(MAX_LOCK_DURATION_SECS));
    }

    #[test]
    fn parse_timeout_rejects_above_server_max_duration() {
        let headers = timeout_header("Second-604801");

        assert!(
            parse_timeout(&headers).is_err(),
            "timeout above server maximum must be rejected"
        );
    }

    #[test]
    fn parse_timeout_rejects_u64_values_too_large_for_chrono() {
        let headers = timeout_header("Second-9223372036854775808");

        assert!(
            parse_timeout(&headers).is_err(),
            "overflow-sized timeout must be rejected before DB conversion"
        );
    }

    #[test]
    fn parse_timeout_skips_unsupported_candidate_before_valid_timeout() {
        let headers = timeout_header("nonsense, Second-60");

        let timeout = parse_timeout(&headers)
            .expect("later valid Timeout candidate should be accepted")
            .expect("valid Timeout candidate should produce a duration");

        assert_eq!(timeout, Duration::from_secs(60));
    }
}

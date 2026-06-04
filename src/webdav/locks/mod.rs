//! WebDAV LOCK / UNLOCK handlers and lock XML helpers.

use std::io::Cursor;
use std::time::Duration;

use actix_web::http::{StatusCode, header};
use actix_web::{HttpRequest, HttpResponse};
use xmltree::{Element, XMLNode};

use crate::webdav::dav::{DavFileSystem, DavLock, DavLockSystem, FsError, OpenOptions};
use crate::webdav::protocol::{self, Depth};
use crate::webdav::{
    XML_CONTENT_TYPE, child_elements, dav_element, encode_href, fs, fs_error_response,
    request_path, text_element, xml_bytes,
};

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
        let tokens = protocol::submitted_lock_tokens(req.headers());
        if tokens.len() != 1 {
            return HttpResponse::BadRequest().finish();
        }
        if lock_system
            .check(&path, None, false, false, &tokens)
            .await
            .is_err()
        {
            return HttpResponse::PreconditionFailed().finish();
        }
        let lock = match lock_system
            .refresh(&path, &tokens[0], parse_timeout(req.headers()))
            .await
        {
            Ok(lock) => lock,
            Err(_) => return HttpResponse::PreconditionFailed().finish(),
        };
        return lock_response(lock, StatusCode::OK);
    }

    let depth = match protocol::parse_lock_depth(req.headers()) {
        Ok(depth) => depth,
        Err(resp) => return resp,
    };

    let tree = match Element::parse(Cursor::new(body)) {
        Ok(tree) => tree,
        Err(_) => return HttpResponse::BadRequest().body("Invalid XML body"),
    };
    if tree.name != "lockinfo" {
        return HttpResponse::BadRequest().body("Invalid LOCK body");
    }

    let mut shared = None;
    let mut owner = None;
    let mut write_lock = false;
    for elem in child_elements(&tree) {
        match elem.name.as_str() {
            "lockscope" => {
                let scope = child_elements(elem).next().map(|child| child.name.as_str());
                match scope {
                    Some("exclusive") => shared = Some(false),
                    Some("shared") => shared = Some(true),
                    _ => return HttpResponse::BadRequest().finish(),
                }
            }
            "locktype" => {
                write_lock = child_elements(elem).any(|child| child.name == "write");
            }
            "owner" => owner = Some(elem.clone()),
            _ => return HttpResponse::BadRequest().finish(),
        }
    }
    if shared.is_none() || !write_lock {
        return HttpResponse::BadRequest().finish();
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
            parse_timeout(req.headers()),
            shared.unwrap_or(false),
            depth.is_infinity(),
        )
        .await
    {
        Ok(lock) => lock,
        Err(_) => return HttpResponse::Locked().finish(),
    };

    let status = if resource_existed {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    lock_response(lock, status)
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
    let token = match req
        .headers()
        .get("Lock-Token")
        .and_then(|value| value.to_str().ok())
    {
        Some(token) => token.trim().trim_matches(|c| c == '<' || c == '>'),
        None => return HttpResponse::BadRequest().finish(),
    };

    match lock_system.unlock(&path, token).await {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(()) => HttpResponse::Conflict().finish(),
    }
}

fn parse_timeout(headers: &header::HeaderMap) -> Option<Duration> {
    let raw = headers
        .get("Timeout")
        .and_then(|value| value.to_str().ok())?;
    let candidate = raw.split(',').map(str::trim).next()?;
    if candidate.eq_ignore_ascii_case("Infinite") {
        return None;
    }
    let seconds = candidate.strip_prefix("Second-")?.parse::<u64>().ok()?;
    Some(Duration::from_secs(seconds))
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

pub(crate) fn lockdiscovery_element(locks: &[DavLock]) -> Element {
    let mut discovery = dav_element("lockdiscovery");
    for lock in locks {
        discovery
            .children
            .push(XMLNode::Element(active_lock_element(lock)));
    }
    discovery
}

fn active_lock_element(lock: &DavLock) -> Element {
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

    active
}

fn lock_response(lock: DavLock, status: StatusCode) -> HttpResponse {
    let mut prop = dav_element("prop");
    prop.attributes
        .insert("xmlns:D".to_string(), "DAV:".to_string());
    prop.children.push(XMLNode::Element(lockdiscovery_element(
        std::slice::from_ref(&lock),
    )));

    let body = match xml_bytes(&prop) {
        Ok(body) => body,
        Err(resp) => return resp,
    };

    HttpResponse::build(status)
        .insert_header(("Lock-Token", format!("<{}>", lock.token)))
        .content_type(XML_CONTENT_TYPE)
        .body(body)
}

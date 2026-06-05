//! WebDAV 模块导出。

pub mod auth;
pub mod dav;
pub mod db_lock_system;
pub mod deltav;
pub mod dir_entry;
pub mod file;
pub mod fs;
mod locks;
pub mod metadata;
pub mod path_resolver;
mod props;
mod protocol;
mod resources;
pub mod system_file;
mod transfer;

use actix_web::http::{StatusCode, header};
use actix_web::{HttpRequest, HttpResponse, web};
use futures::StreamExt;
use xmltree::{Element, XMLNode};

use crate::config::WebDavConfig;
use crate::runtime::PrimaryAppState;
use crate::services::audit_service;
use crate::utils::numbers::u64_to_usize;
use crate::webdav::dav::{DavLockSystem, DavPath, FsError};

const XML_CONTENT_TYPE: &str = "application/xml; charset=utf-8";

pub(crate) fn encode_href(path: &str) -> String {
    use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

    const PATH_SET: &AsciiSet = &CONTROLS
        .add(b' ')
        .add(b'"')
        .add(b'#')
        .add(b'<')
        .add(b'>')
        .add(b'?')
        .add(b'`')
        .add(b'{')
        .add(b'}')
        .add(b'&')
        .add(b'\'')
        .add(b'+')
        .add(b'%');

    utf8_percent_encode(path, PATH_SET).to_string()
}

/// WebDAV 共享状态（单例）
pub struct WebDavState {
    pub prefix: String,
    pub xml_payload_limit: usize,
}

/// WebDAV handler — 所有协议方法都由自研分发层处理
pub async fn webdav_handler(
    req: HttpRequest,
    mut payload: web::Payload,
    state: web::Data<PrimaryAppState>,
    webdav: web::Data<WebDavState>,
) -> HttpResponse {
    if !state.runtime_config.get_bool_or("webdav_enabled", true) {
        return HttpResponse::ServiceUnavailable().body("WebDAV is disabled");
    }

    let auth_result = match auth::authenticate_webdav(req.headers(), &state).await {
        Ok(result) => result,
        Err(_) => return unauthorized_response(),
    };

    let audit_info = audit_service::AuditRequestInfo::from_request(&req);
    let audit_ctx = audit_info.to_context(auth_result.scope.actor_user_id());

    let dav_fs = fs::AsterDavFs::new_with_audit(
        state.get_ref().clone(),
        auth_result.scope,
        auth_result.root_folder_id,
        audit_ctx.clone(),
    );
    let lock_system = db_lock_system::DbLockSystem::new_with_audit(
        state.get_ref().clone(),
        auth_result.scope,
        auth_result.root_folder_id,
        audit_ctx,
    );

    match req.method().as_str() {
        "OPTIONS" => match resources::ensure_empty_body(&mut payload).await {
            Ok(()) => handle_options(),
            Err(resp) => resp,
        },
        "REPORT" => match collect_xml_payload(&mut payload, webdav.xml_payload_limit).await {
            Ok(body) => {
                deltav::handle_report(
                    req.uri(),
                    &body,
                    state.writer_db(),
                    &auth_result,
                    &webdav.prefix,
                )
                .await
            }
            Err(resp) => resp,
        },
        "VERSION-CONTROL" => {
            deltav::handle_version_control(
                req.uri(),
                state.writer_db(),
                &auth_result,
                &webdav.prefix,
            )
            .await
        }
        "PROPFIND" => match collect_xml_payload(&mut payload, webdav.xml_payload_limit).await {
            Ok(body) => {
                props::handle_propfind(&req, &dav_fs, lock_system.as_ref(), &webdav.prefix, &body)
                    .await
            }
            Err(resp) => resp,
        },
        "PROPPATCH" => match collect_xml_payload(&mut payload, webdav.xml_payload_limit).await {
            Ok(body) => {
                props::handle_proppatch(&req, &dav_fs, lock_system.as_ref(), &webdav.prefix, &body)
                    .await
            }
            Err(resp) => resp,
        },
        "GET" => {
            transfer::handle_get_head(&req, &dav_fs, lock_system.as_ref(), &webdav.prefix, false)
                .await
        }
        "HEAD" => {
            transfer::handle_get_head(&req, &dav_fs, lock_system.as_ref(), &webdav.prefix, true)
                .await
        }
        "PUT" => {
            let system_file_policy =
                system_file::SystemFileBlockPolicy::from_runtime_config(&state.runtime_config);
            transfer::handle_put(
                &req,
                &dav_fs,
                lock_system.as_ref(),
                &webdav.prefix,
                &system_file_policy,
                &mut payload,
            )
            .await
        }
        "MKCOL" => {
            let system_file_policy =
                system_file::SystemFileBlockPolicy::from_runtime_config(&state.runtime_config);
            resources::handle_mkcol(
                &req,
                &dav_fs,
                lock_system.as_ref(),
                &webdav.prefix,
                &system_file_policy,
                &mut payload,
            )
            .await
        }
        "DELETE" => match resources::ensure_empty_body(&mut payload).await {
            Ok(()) => {
                resources::handle_delete(&req, &dav_fs, lock_system.as_ref(), &webdav.prefix).await
            }
            Err(resp) => resp,
        },
        "COPY" => match resources::ensure_empty_body(&mut payload).await {
            Ok(()) => {
                let system_file_policy =
                    system_file::SystemFileBlockPolicy::from_runtime_config(&state.runtime_config);
                resources::handle_copy_move(
                    &req,
                    &dav_fs,
                    lock_system.as_ref(),
                    &webdav.prefix,
                    &system_file_policy,
                    false,
                )
                .await
            }
            Err(resp) => resp,
        },
        "MOVE" => match resources::ensure_empty_body(&mut payload).await {
            Ok(()) => {
                let system_file_policy =
                    system_file::SystemFileBlockPolicy::from_runtime_config(&state.runtime_config);
                resources::handle_copy_move(
                    &req,
                    &dav_fs,
                    lock_system.as_ref(),
                    &webdav.prefix,
                    &system_file_policy,
                    true,
                )
                .await
            }
            Err(resp) => resp,
        },
        "LOCK" => match collect_xml_payload(&mut payload, webdav.xml_payload_limit).await {
            Ok(body) => {
                locks::handle_lock(&req, &dav_fs, lock_system.as_ref(), &webdav.prefix, &body).await
            }
            Err(resp) => resp,
        },
        "UNLOCK" => match resources::ensure_empty_body(&mut payload).await {
            Ok(()) => locks::handle_unlock(&req, lock_system.as_ref(), &webdav.prefix).await,
            Err(resp) => resp,
        },
        _ => HttpResponse::MethodNotAllowed()
            .insert_header((header::ALLOW, allow_header_value()))
            .finish(),
    }
}

async fn collect_xml_payload(
    payload: &mut web::Payload,
    max_len: usize,
) -> Result<Vec<u8>, HttpResponse> {
    let mut data = Vec::with_capacity(max_len.min(4096));
    while let Some(chunk) = payload.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(_) => return Err(HttpResponse::BadRequest().body("Failed to read request body")),
        };
        let next_len = match data.len().checked_add(chunk.len()) {
            Some(next_len) => next_len,
            None => return Err(HttpResponse::PayloadTooLarge().body("WebDAV XML body too large")),
        };
        if next_len > max_len {
            return Err(HttpResponse::PayloadTooLarge().body("WebDAV XML body too large"));
        }
        data.extend_from_slice(&chunk);
    }
    Ok(data)
}

fn handle_options() -> HttpResponse {
    HttpResponse::Ok()
        .insert_header((header::ALLOW, allow_header_value()))
        .insert_header(("DAV", "1, 2, version-control"))
        .insert_header(("MS-Author-Via", "DAV"))
        .finish()
}

pub(crate) fn ensure_system_file_name_allowed(
    system_file_policy: &system_file::SystemFileBlockPolicy,
    relative: &str,
) -> Result<(), HttpResponse> {
    let name = display_name(relative);
    if name.is_empty() || !system_file_policy.is_blocked_name(name) {
        return Ok(());
    }

    Err(HttpResponse::Forbidden().body("WebDAV system file name is blocked"))
}

pub(crate) async fn ensure_unlocked(
    lock_system: &dyn DavLockSystem,
    path: &DavPath,
    deep: bool,
    prefix: &str,
    headers: &header::HeaderMap,
    request_scheme: &str,
    request_host: &str,
) -> Result<(), HttpResponse> {
    let request_href = href_for_dav_path(prefix, path);
    let tokens = protocol::submitted_lock_tokens_for_path(
        headers,
        &request_href,
        request_scheme,
        request_host,
    );
    match lock_system.check(path, None, false, deep, &tokens).await {
        Ok(()) => Ok(()),
        Err(_) => Err(HttpResponse::Locked().finish()),
    }
}

pub(crate) async fn ensure_parent_unlocked(
    lock_system: &dyn DavLockSystem,
    relative: &str,
    prefix: &str,
    headers: &header::HeaderMap,
    request_scheme: &str,
    request_host: &str,
) -> Result<(), HttpResponse> {
    let Some(parent) = parent_relative_path(relative) else {
        return Ok(());
    };
    let parent_path = DavPath::new(&parent)
        .map_err(|_| HttpResponse::BadRequest().body("Invalid request path"))?;
    ensure_unlocked(
        lock_system,
        &parent_path,
        false,
        prefix,
        headers,
        request_scheme,
        request_host,
    )
    .await
}

pub(crate) fn request_path(
    req: &HttpRequest,
    prefix: &str,
) -> Result<(DavPath, String), HttpResponse> {
    decode_relative_path(req.path().strip_prefix(prefix).unwrap_or(req.path()))
}

pub(crate) fn decode_relative_path(relative: &str) -> Result<(DavPath, String), HttpResponse> {
    let normalized = normalize_relative_path(relative);
    let path = DavPath::new(&normalized)
        .map_err(|_| HttpResponse::BadRequest().body("Invalid request path"))?;
    let decoded = decoded_path_string(&path);
    Ok((path, decoded))
}

fn normalize_relative_path(path: &str) -> String {
    if path.is_empty() || path == "/" {
        return "/".to_string();
    }
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}

pub(crate) fn xml_response(root: Element, status: StatusCode) -> HttpResponse {
    match xml_bytes(&root) {
        Ok(body) => HttpResponse::build(status)
            .content_type(XML_CONTENT_TYPE)
            .body(body),
        Err(resp) => resp,
    }
}

pub(crate) fn xml_bytes(root: &Element) -> Result<Vec<u8>, HttpResponse> {
    let mut buffer = Vec::new();
    root.write(&mut buffer)
        .map_err(|_| HttpResponse::InternalServerError().finish())?;
    Ok(buffer)
}

pub(crate) fn dav_element(name: &str) -> Element {
    Element::new(&format!("D:{name}"))
}

pub(crate) fn text_element(tag: &str, text: &str) -> Element {
    let mut element = Element::new(tag);
    element.children.push(XMLNode::Text(text.to_string()));
    element
}

pub(crate) fn status_element(status: StatusCode) -> Element {
    text_element(
        "D:status",
        &format!(
            "HTTP/1.1 {} {}",
            status.as_u16(),
            status.canonical_reason().unwrap_or("Unknown"),
        ),
    )
}

pub(crate) fn child_elements(element: &Element) -> impl Iterator<Item = &Element> {
    element.children.iter().filter_map(|child| match child {
        XMLNode::Element(element) => Some(element),
        _ => None,
    })
}

pub(crate) fn child_relative_path(parent: &str, name: &[u8], is_dir: bool) -> String {
    let name = String::from_utf8_lossy(name);
    let mut relative = if parent == "/" {
        format!("/{name}")
    } else if parent.ends_with('/') {
        format!("{parent}{name}")
    } else {
        format!("{parent}/{name}")
    };
    if is_dir && !relative.ends_with('/') {
        relative.push('/');
    }
    relative
}

pub(crate) fn decoded_path_string(path: &DavPath) -> String {
    String::from_utf8_lossy(path.as_bytes()).into_owned()
}

pub(crate) fn href_for_relative(prefix: &str, relative: &str) -> String {
    let href = if relative == "/" {
        format!("{prefix}/")
    } else {
        format!("{prefix}{relative}")
    };
    encode_href(&href)
}

pub(crate) fn href_for_dav_path(prefix: &str, path: &DavPath) -> String {
    href_for_relative(prefix, &decoded_path_string(path))
}

pub(crate) fn display_name(relative: &str) -> &str {
    if relative == "/" {
        ""
    } else {
        relative
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("")
    }
}

pub(crate) fn parent_relative_path(relative: &str) -> Option<String> {
    if relative == "/" {
        return None;
    }
    let trimmed = relative.trim_end_matches('/');
    let segments: Vec<_> = trimmed
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.len() <= 1 {
        return Some("/".to_string());
    }
    Some(format!("/{}/", segments[..segments.len() - 1].join("/")))
}

pub(crate) fn fs_error_response(err: FsError) -> HttpResponse {
    HttpResponse::build(fs_error_status(&err)).finish()
}

fn fs_error_status(err: &FsError) -> StatusCode {
    match err {
        FsError::NotFound => StatusCode::NOT_FOUND,
        FsError::Forbidden => StatusCode::FORBIDDEN,
        FsError::Exists => StatusCode::CONFLICT,
        FsError::InsufficientStorage => StatusCode::INSUFFICIENT_STORAGE,
        FsError::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
        FsError::BadRequest => StatusCode::BAD_REQUEST,
        FsError::GeneralFailure => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub(crate) fn format_http_date(time: std::time::SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(time)
        .format("%a, %d %b %Y %H:%M:%S GMT")
        .to_string()
}

pub(crate) fn format_creation_date(time: std::time::SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(time).to_rfc3339()
}

fn allow_header_value() -> &'static str {
    "OPTIONS, GET, HEAD, PUT, DELETE, MKCOL, COPY, MOVE, PROPFIND, PROPPATCH, LOCK, UNLOCK, REPORT, VERSION-CONTROL"
}

pub(crate) fn multi_status() -> StatusCode {
    StatusCode::MULTI_STATUS
}

fn unauthorized_response() -> HttpResponse {
    HttpResponse::Unauthorized()
        .insert_header(("WWW-Authenticate", "Basic realm=\"AsterDrive WebDAV\""))
        .body("Unauthorized")
}

/// 注册 WebDAV 路由
pub fn configure(
    cfg: &mut web::ServiceConfig,
    webdav_config: &WebDavConfig,
    _db: &sea_orm::DatabaseConnection,
) {
    let payload_limit = u64_to_usize(webdav_config.payload_limit, "webdav.payload_limit")
        .unwrap_or_else(|_| {
            tracing::warn!(
                configured = webdav_config.payload_limit,
                platform_limit = usize::MAX,
                "webdav.payload_limit exceeds platform usize range; using platform limit"
            );
            usize::MAX
        });
    let webdav_state = web::Data::new(WebDavState {
        prefix: webdav_config.prefix.clone(),
        xml_payload_limit: u64_to_usize(
            webdav_config.xml_payload_limit,
            "webdav.xml_payload_limit",
        )
        .unwrap_or_else(|_| {
            tracing::warn!(
                configured = webdav_config.xml_payload_limit,
                platform_limit = usize::MAX,
                "webdav.xml_payload_limit exceeds platform usize range; using platform limit"
            );
            usize::MAX
        }),
    });

    cfg.app_data(webdav_state).service(
        web::scope(&webdav_config.prefix)
            .app_data(web::PayloadConfig::new(payload_limit))
            .default_service(web::to(webdav_handler)),
    );
}

#[cfg(test)]
mod handler_tests;

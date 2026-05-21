//! WebDAV 模块导出。

pub mod auth;
pub mod dav;
pub mod db_lock_system;
pub mod deltav;
pub mod dir_entry;
pub mod file;
pub mod fs;
pub mod metadata;
pub mod path_resolver;
pub mod system_file;

use std::collections::BTreeMap;
use std::io::Cursor;
use std::time::Duration;

use actix_web::http::{StatusCode, header};
use actix_web::{HttpRequest, HttpResponse, web};
use futures::{StreamExt, pin_mut};
use tokio_util::io::ReaderStream;
use xmltree::{Element, XMLNode};

use crate::config::WebDavConfig;
use crate::runtime::PrimaryAppState;
use crate::services::{audit_service, property_service};
use crate::utils::numbers::u64_to_usize;
use crate::webdav::dav::{
    DavFileSystem, DavLock, DavLockSystem, DavMetaData, DavPath, DavProp, FsError, OpenOptions,
    ReadDirMeta,
};

const XML_CONTENT_TYPE: &str = "application/xml; charset=utf-8";
const CHUNK_SIZE: usize = 16 * 1024;

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

struct RequestedProp {
    name: String,
    namespace: Option<String>,
    prefix: Option<String>,
}

enum PropfindKind {
    AllProp,
    PropName,
    Prop(Vec<RequestedProp>),
}

struct PropfindResource {
    path: DavPath,
    relative: String,
    meta: Box<dyn DavMetaData>,
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
    let audit_ctx = audit_info.to_context(auth_result.user_id);

    let dav_fs = fs::AsterDavFs::new_with_audit(
        state.get_ref().clone(),
        auth_result.user_id,
        auth_result.root_folder_id,
        audit_ctx.clone(),
    );
    let lock_system = db_lock_system::DbLockSystem::new_with_audit(
        state.get_ref().clone(),
        auth_result.user_id,
        auth_result.root_folder_id,
        audit_ctx,
    );

    match req.method().as_str() {
        "OPTIONS" => handle_options(),
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
                handle_propfind(&req, &dav_fs, lock_system.as_ref(), &webdav.prefix, &body).await
            }
            Err(resp) => resp,
        },
        "PROPPATCH" => match collect_xml_payload(&mut payload, webdav.xml_payload_limit).await {
            Ok(body) => {
                handle_proppatch(&req, &dav_fs, lock_system.as_ref(), &webdav.prefix, &body).await
            }
            Err(resp) => resp,
        },
        "GET" => handle_get_head(&req, &dav_fs, &webdav.prefix, false).await,
        "HEAD" => handle_get_head(&req, &dav_fs, &webdav.prefix, true).await,
        "PUT" => {
            let system_file_policy =
                system_file::SystemFileBlockPolicy::from_runtime_config(&state.runtime_config);
            handle_put(
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
            handle_mkcol(
                &req,
                &dav_fs,
                lock_system.as_ref(),
                &webdav.prefix,
                &system_file_policy,
                &mut payload,
            )
            .await
        }
        "DELETE" => handle_delete(&req, &dav_fs, lock_system.as_ref(), &webdav.prefix).await,
        "COPY" => {
            let system_file_policy =
                system_file::SystemFileBlockPolicy::from_runtime_config(&state.runtime_config);
            handle_copy_move(
                &req,
                &dav_fs,
                lock_system.as_ref(),
                &webdav.prefix,
                &system_file_policy,
                false,
            )
            .await
        }
        "MOVE" => {
            let system_file_policy =
                system_file::SystemFileBlockPolicy::from_runtime_config(&state.runtime_config);
            handle_copy_move(
                &req,
                &dav_fs,
                lock_system.as_ref(),
                &webdav.prefix,
                &system_file_policy,
                true,
            )
            .await
        }
        "LOCK" => match collect_xml_payload(&mut payload, webdav.xml_payload_limit).await {
            Ok(body) => {
                handle_lock(&req, &dav_fs, lock_system.as_ref(), &webdav.prefix, &body).await
            }
            Err(resp) => resp,
        },
        "UNLOCK" => handle_unlock(&req, lock_system.as_ref(), &webdav.prefix).await,
        _ => HttpResponse::MethodNotAllowed()
            .insert_header((header::ALLOW, allow_header_value()))
            .finish(),
    }
}

async fn collect_payload(payload: &mut web::Payload) -> Result<Vec<u8>, HttpResponse> {
    let mut data = Vec::new();
    while let Some(chunk) = payload.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(_) => return Err(HttpResponse::BadRequest().body("Failed to read request body")),
        };
        data.extend_from_slice(&chunk);
    }
    Ok(data)
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

async fn handle_get_head(
    req: &HttpRequest,
    dav_fs: &fs::AsterDavFs,
    prefix: &str,
    head_only: bool,
) -> HttpResponse {
    let (path, relative) = match request_path(req, prefix) {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let meta = match dav_fs.metadata(&path).await {
        Ok(meta) => meta,
        Err(err) => return fs_error_response(err),
    };
    if meta.is_dir() {
        return HttpResponse::MethodNotAllowed().finish();
    }

    let content_length = meta.len().to_string();
    let content_type = mime_guess::from_path(relative.trim_end_matches('/'))
        .first_or_octet_stream()
        .essence_str()
        .to_string();

    let mut response = HttpResponse::Ok();
    response.insert_header((header::CONTENT_LENGTH, content_length));
    response.insert_header((header::CONTENT_TYPE, content_type));
    if let Some(etag) = meta.etag() {
        response.insert_header((header::ETAG, format!("\"{etag}\"")));
    }

    if head_only {
        return response.finish();
    }

    // GET 必须直接走存储流，不要回退到 DavFileSystem::open(read) 的缓冲兜底，
    // 否则会重新引入额外复制/临时文件。
    let stream = match dav_fs.open_read_stream(&path).await {
        Ok(stream) => stream,
        Err(err) => return fs_error_response(err),
    };
    response.streaming(ReaderStream::with_capacity(stream, CHUNK_SIZE))
}

async fn handle_put(
    req: &HttpRequest,
    dav_fs: &fs::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    system_file_policy: &system_file::SystemFileBlockPolicy,
    payload: &mut web::Payload,
) -> HttpResponse {
    let (path, relative) = match request_path(req, prefix) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if let Err(resp) = ensure_system_file_name_allowed(system_file_policy, &relative) {
        return resp;
    }
    let existed = dav_fs.metadata(&path).await.is_ok();

    if let Err(resp) = ensure_unlocked(lock_system, &path, false, req.headers()).await {
        return resp;
    }

    let create_new = header_equals(req.headers(), header::IF_NONE_MATCH, "*");
    let create = !header_equals(req.headers(), header::IF_MATCH, "*");
    let mut options = OpenOptions::write();
    options.create = create;
    options.create_new = create_new;
    options.truncate = true;
    // 已知 Content-Length 才能命中本地 staging / S3 put_reader 快路径。
    options.size = req
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());

    let mut file = match dav_fs.open(&path, options).await {
        Ok(file) => file,
        Err(FsError::Exists) => return HttpResponse::PreconditionFailed().finish(),
        Err(FsError::NotFound) => return HttpResponse::Conflict().finish(),
        Err(err) => return fs_error_response(err),
    };

    while let Some(chunk) = payload.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(_) => return HttpResponse::BadRequest().body("Failed to read request body"),
        };
        if let Err(err) = file.write_bytes(chunk).await {
            return fs_error_response(err);
        }
    }

    if let Err(err) = file.flush().await {
        return fs_error_response(err);
    }

    if existed {
        HttpResponse::NoContent().finish()
    } else {
        HttpResponse::Created()
            .insert_header((
                header::CONTENT_LOCATION,
                href_for_relative(prefix, &relative),
            ))
            .finish()
    }
}

async fn handle_mkcol(
    req: &HttpRequest,
    dav_fs: &fs::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    system_file_policy: &system_file::SystemFileBlockPolicy,
    payload: &mut web::Payload,
) -> HttpResponse {
    if let Err(resp) = ensure_empty_body(payload).await {
        return resp;
    }

    let (path, relative) = match request_path(req, prefix) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if relative == "/" {
        return HttpResponse::MethodNotAllowed().finish();
    }
    if let Err(resp) = ensure_system_file_name_allowed(system_file_policy, &relative) {
        return resp;
    }

    if let Err(resp) = ensure_parent_exists(dav_fs, &relative).await {
        return resp;
    }
    if let Err(resp) = ensure_unlocked(lock_system, &path, false, req.headers()).await {
        return resp;
    }

    match dav_fs.create_dir(&path).await {
        Ok(()) => HttpResponse::Created()
            .insert_header((
                header::CONTENT_LOCATION,
                href_for_relative(prefix, &relative),
            ))
            .finish(),
        Err(FsError::Exists) => HttpResponse::MethodNotAllowed().finish(),
        Err(FsError::NotFound) => HttpResponse::Conflict().finish(),
        Err(err) => fs_error_response(err),
    }
}

async fn handle_delete(
    req: &HttpRequest,
    dav_fs: &fs::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
) -> HttpResponse {
    let (path, _) = match request_path(req, prefix) {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let meta = match dav_fs.metadata(&path).await {
        Ok(meta) => meta,
        Err(err) => return fs_error_response(err),
    };
    if let Err(resp) = ensure_unlocked(lock_system, &path, true, req.headers()).await {
        return resp;
    }

    let result = if meta.is_dir() {
        dav_fs.remove_dir(&path).await
    } else {
        dav_fs.remove_file(&path).await
    };
    match result {
        Ok(()) => {
            if lock_system.delete(&path).await.is_err() {
                tracing::warn!(
                    path = %decoded_path_string(&path),
                    "failed to delete WebDAV locks after resource deletion"
                );
            }
            HttpResponse::NoContent().finish()
        }
        Err(err) => fs_error_response(err),
    }
}

async fn handle_copy_move(
    req: &HttpRequest,
    dav_fs: &fs::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    system_file_policy: &system_file::SystemFileBlockPolicy,
    is_move: bool,
) -> HttpResponse {
    let (source, source_relative) = match request_path(req, prefix) {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let destination_relative = match destination_relative_path(req, prefix) {
        Ok(path) => path,
        Err(resp) => return resp,
    };
    if source_relative == destination_relative {
        return HttpResponse::Forbidden().finish();
    }
    if let Err(resp) = ensure_system_file_name_allowed(system_file_policy, &destination_relative) {
        return resp;
    }
    if let Err(resp) = ensure_parent_exists(dav_fs, &destination_relative).await {
        return resp;
    }

    let destination = match DavPath::new(&destination_relative) {
        Ok(path) => path,
        Err(_) => return HttpResponse::BadRequest().body("Invalid destination path"),
    };
    if is_move && let Err(resp) = ensure_unlocked(lock_system, &source, true, req.headers()).await {
        return resp;
    }
    if let Err(resp) = ensure_unlocked(lock_system, &destination, true, req.headers()).await {
        return resp;
    }

    let destination_exists = dav_fs.metadata(&destination).await.is_ok();
    if !overwrite_enabled(req.headers()) && destination_exists {
        return HttpResponse::PreconditionFailed().finish();
    }

    let result = if is_move {
        dav_fs.rename(&source, &destination).await
    } else {
        dav_fs.copy(&source, &destination).await
    };

    match result {
        Ok(()) => {
            if is_move && lock_system.delete(&source).await.is_err() {
                tracing::warn!(
                    path = %source_relative,
                    "failed to delete WebDAV locks after move"
                );
            }
            if destination_exists {
                HttpResponse::NoContent().finish()
            } else {
                HttpResponse::Created().finish()
            }
        }
        Err(err) => fs_error_response(err),
    }
}

async fn handle_propfind(
    req: &HttpRequest,
    dav_fs: &fs::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    body: &[u8],
) -> HttpResponse {
    let (path, relative) = match request_path(req, prefix) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let depth = match parse_propfind_depth(req.headers()) {
        Ok(depth) => depth,
        Err(resp) => return resp,
    };
    let request_kind = match parse_propfind_request(body) {
        Ok(kind) => kind,
        Err(resp) => return resp,
    };

    let mut resources = match collect_propfind_resources(dav_fs, &path, &relative, depth).await {
        Ok(resources) => resources,
        Err(err) => return fs_error_response(err),
    };

    let mut multistatus = dav_element("multistatus");
    multistatus
        .attributes
        .insert("xmlns:D".to_string(), "DAV:".to_string());

    for resource in resources.drain(..) {
        let response =
            match build_propfind_response(dav_fs, lock_system, prefix, &request_kind, resource)
                .await
            {
                Ok(response) => response,
                Err(resp) => return resp,
            };
        multistatus.children.push(XMLNode::Element(response));
    }

    xml_response(multistatus, multi_status())
}

async fn handle_proppatch(
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
    if let Err(resp) = ensure_unlocked(lock_system, &path, false, req.headers()).await {
        return resp;
    }

    let root = match Element::parse(Cursor::new(body)) {
        Ok(root) => root,
        Err(_) => return HttpResponse::BadRequest().body("Invalid XML body"),
    };
    if root.name != "propertyupdate" {
        return HttpResponse::BadRequest().body("Invalid PROPPATCH body");
    }

    let mut patches = Vec::new();
    for action in child_elements(&root) {
        let set = match action.name.as_str() {
            "set" => true,
            "remove" => false,
            _ => continue,
        };
        for prop_container in child_elements(action).filter(|elem| elem.name == "prop") {
            for prop in child_elements(prop_container) {
                patches.push((set, prop_from_xml(prop)));
            }
        }
    }

    let results = match dav_fs.patch_props(&path, patches).await {
        Ok(results) => results,
        Err(err) => return fs_error_response(err),
    };

    let mut multistatus = dav_element("multistatus");
    multistatus
        .attributes
        .insert("xmlns:D".to_string(), "DAV:".to_string());

    let mut response = dav_element("response");
    response.children.push(XMLNode::Element(text_element(
        "D:href",
        &href_for_dav_path(prefix, &path),
    )));

    let mut groups: BTreeMap<u16, Vec<DavProp>> = BTreeMap::new();
    for (status, prop) in results {
        groups.entry(status.as_u16()).or_default().push(prop);
    }

    for (status_code, props) in groups {
        let status = StatusCode::from_u16(status_code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let mut propstat = dav_element("propstat");
        let mut prop = dav_element("prop");
        for item in props {
            prop.children
                .push(XMLNode::Element(prop_element(&item, None)));
        }
        propstat.children.push(XMLNode::Element(prop));
        propstat
            .children
            .push(XMLNode::Element(status_element(status)));
        response.children.push(XMLNode::Element(propstat));
    }

    multistatus.children.push(XMLNode::Element(response));
    xml_response(multistatus, multi_status())
}

async fn handle_lock(
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
        let tokens = submitted_lock_tokens(req.headers());
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

    let depth = match parse_lock_depth(req.headers()) {
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

    match dav_fs.metadata(&path).await {
        Ok(_) => {}
        Err(FsError::NotFound) => {
            // 现有锁系统只能锁定已解析到数据库实体的资源；
            // 对不存在路径直接返回 404，避免误报成 423 Locked。
            return HttpResponse::NotFound().finish();
        }
        Err(err) => return fs_error_response(err),
    }

    let lock = match lock_system
        .lock(
            &path,
            None,
            owner.as_ref(),
            parse_timeout(req.headers()),
            shared.unwrap_or(false),
            depth,
        )
        .await
    {
        Ok(lock) => lock,
        Err(_) => return HttpResponse::Locked().finish(),
    };

    lock_response(lock, StatusCode::OK)
}

async fn handle_unlock(
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

async fn ensure_empty_body(payload: &mut web::Payload) -> Result<(), HttpResponse> {
    if collect_payload(payload).await?.is_empty() {
        Ok(())
    } else {
        Err(HttpResponse::UnsupportedMediaType().finish())
    }
}

async fn ensure_parent_exists(dav_fs: &fs::AsterDavFs, relative: &str) -> Result<(), HttpResponse> {
    let Some(parent) = parent_relative_path(relative) else {
        return Err(HttpResponse::MethodNotAllowed().finish());
    };
    if parent == "/" {
        return Ok(());
    }
    let parent_path = DavPath::new(&parent).map_err(|_| HttpResponse::BadRequest().finish())?;
    match dav_fs.metadata(&parent_path).await {
        Ok(meta) if meta.is_dir() => Ok(()),
        Ok(_) => Err(HttpResponse::Conflict().finish()),
        Err(FsError::NotFound) => Err(HttpResponse::Conflict().finish()),
        Err(err) => Err(fs_error_response(err)),
    }
}

fn ensure_system_file_name_allowed(
    system_file_policy: &system_file::SystemFileBlockPolicy,
    relative: &str,
) -> Result<(), HttpResponse> {
    let name = display_name(relative);
    if name.is_empty() || !system_file_policy.is_blocked_name(name) {
        return Ok(());
    }

    Err(HttpResponse::Forbidden().body("WebDAV system file name is blocked"))
}

async fn ensure_unlocked(
    lock_system: &dyn DavLockSystem,
    path: &DavPath,
    deep: bool,
    headers: &header::HeaderMap,
) -> Result<(), HttpResponse> {
    let tokens = submitted_lock_tokens(headers);
    match lock_system.check(path, None, false, deep, &tokens).await {
        Ok(()) => Ok(()),
        Err(_) => Err(HttpResponse::Locked().finish()),
    }
}

fn request_path(req: &HttpRequest, prefix: &str) -> Result<(DavPath, String), HttpResponse> {
    decode_relative_path(req.path().strip_prefix(prefix).unwrap_or(req.path()))
}

fn destination_relative_path(req: &HttpRequest, prefix: &str) -> Result<String, HttpResponse> {
    let raw = req
        .headers()
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
    let relative = path.strip_prefix(prefix).ok_or_else(|| {
        HttpResponse::BadRequest().body("Destination must stay under WebDAV prefix")
    })?;
    decode_relative_path(relative).map(|(_, relative)| relative)
}

fn decode_relative_path(relative: &str) -> Result<(DavPath, String), HttpResponse> {
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

fn parent_relative_path(relative: &str) -> Option<String> {
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

fn parse_propfind_depth(headers: &header::HeaderMap) -> Result<u8, HttpResponse> {
    match headers.get("Depth").and_then(|value| value.to_str().ok()) {
        Some("0") => Ok(0),
        Some("1") => Ok(1),
        Some("infinity") => Err(HttpResponse::NotImplemented().finish()),
        Some(_) => Err(HttpResponse::BadRequest().finish()),
        None => Ok(0),
    }
}

fn parse_lock_depth(headers: &header::HeaderMap) -> Result<bool, HttpResponse> {
    match headers.get("Depth").and_then(|value| value.to_str().ok()) {
        None | Some("infinity") => Ok(true),
        Some("0") => Ok(false),
        Some(_) => Err(HttpResponse::BadRequest().finish()),
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

fn overwrite_enabled(headers: &header::HeaderMap) -> bool {
    headers
        .get("Overwrite")
        .and_then(|value| value.to_str().ok())
        .is_none_or(|value| !value.eq_ignore_ascii_case("F"))
}

fn header_equals(headers: &header::HeaderMap, name: header::HeaderName, expected: &str) -> bool {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim() == expected)
}

fn submitted_lock_tokens(headers: &header::HeaderMap) -> Vec<String> {
    let mut tokens = Vec::new();

    if let Some(token) = headers
        .get("Lock-Token")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().trim_matches(|c| c == '<' || c == '>'))
        .filter(|token| !token.is_empty())
    {
        tokens.push(token.to_string());
    }

    if let Some(if_header) = headers.get("If").and_then(|value| value.to_str().ok()) {
        let mut rest = if_header;
        while let Some(start) = rest.find('<') {
            let next = &rest[start + 1..];
            let Some(end) = next.find('>') else {
                break;
            };
            let token = &next[..end];
            if !token.is_empty() {
                tokens.push(token.to_string());
            }
            rest = &next[end + 1..];
        }
    }

    tokens.sort();
    tokens.dedup();
    tokens
}

fn parse_propfind_request(body: &[u8]) -> Result<PropfindKind, HttpResponse> {
    if body.is_empty() {
        return Ok(PropfindKind::AllProp);
    }

    let root = Element::parse(Cursor::new(body))
        .map_err(|_| HttpResponse::BadRequest().body("Invalid XML body"))?;
    if root.name != "propfind" {
        return Err(HttpResponse::BadRequest().body("Invalid PROPFIND body"));
    }

    for child in child_elements(&root) {
        match child.name.as_str() {
            "propname" => return Ok(PropfindKind::PropName),
            "allprop" => return Ok(PropfindKind::AllProp),
            "prop" => {
                let props = child_elements(child).map(RequestedProp::from).collect();
                return Ok(PropfindKind::Prop(props));
            }
            _ => {}
        }
    }

    Ok(PropfindKind::AllProp)
}

async fn collect_propfind_resources(
    dav_fs: &fs::AsterDavFs,
    path: &DavPath,
    relative: &str,
    depth: u8,
) -> Result<Vec<PropfindResource>, FsError> {
    let root_meta = dav_fs.metadata(path).await?;
    let root_is_dir = root_meta.is_dir();
    let mut resources = vec![PropfindResource {
        path: path.clone(),
        relative: relative.to_string(),
        meta: root_meta,
    }];

    if depth == 1 && root_is_dir {
        let entries = dav_fs.read_dir(path, ReadDirMeta::Data).await?;
        pin_mut!(entries);
        while let Some(entry) = entries.next().await {
            let entry = entry?;
            let meta = entry.metadata().await?;
            let child_relative = child_relative_path(relative, &entry.name(), meta.is_dir());
            let child_path = DavPath::new(&child_relative).map_err(|_| FsError::GeneralFailure)?;
            resources.push(PropfindResource {
                path: child_path,
                relative: child_relative,
                meta,
            });
        }
    }

    Ok(resources)
}

async fn build_propfind_response(
    dav_fs: &fs::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    request_kind: &PropfindKind,
    resource: PropfindResource,
) -> Result<Element, HttpResponse> {
    let mut response = dav_element("response");
    response.children.push(XMLNode::Element(text_element(
        "D:href",
        &href_for_relative(prefix, &resource.relative),
    )));

    let propstats = match request_kind {
        PropfindKind::AllProp => vec![(
            StatusCode::OK,
            all_prop_elements(dav_fs, lock_system, &resource).await?,
        )],
        PropfindKind::PropName => {
            vec![(
                StatusCode::OK,
                prop_name_elements(dav_fs, lock_system, &resource).await?,
            )]
        }
        PropfindKind::Prop(requested) => {
            requested_prop_elements(dav_fs, lock_system, &resource, requested).await?
        }
    };

    for (status, props) in propstats {
        if props.is_empty() {
            continue;
        }
        let mut propstat = dav_element("propstat");
        let mut prop = dav_element("prop");
        for item in props {
            prop.children.push(XMLNode::Element(item));
        }
        propstat.children.push(XMLNode::Element(prop));
        propstat
            .children
            .push(XMLNode::Element(status_element(status)));
        response.children.push(XMLNode::Element(propstat));
    }

    Ok(response)
}

async fn all_prop_elements(
    dav_fs: &fs::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    resource: &PropfindResource,
) -> Result<Vec<Element>, HttpResponse> {
    let mut props = standard_prop_name_list()
        .into_iter()
        .map(|prop| RequestedProp {
            name: prop.to_string(),
            namespace: Some("DAV:".to_string()),
            prefix: Some("D".to_string()),
        })
        .collect::<Vec<_>>();
    let custom_props = if is_root_resource(resource) {
        Vec::new()
    } else {
        dav_fs
            .get_props(&resource.path, true)
            .await
            .map_err(fs_error_response)?
    };
    let mut elements = Vec::new();
    for requested in props.drain(..) {
        if let Some(element) = standard_prop_element(lock_system, resource, &requested).await? {
            elements.push(element);
        }
    }
    for prop in custom_props {
        elements.push(prop_element(&prop, None));
    }
    Ok(elements)
}

async fn prop_name_elements(
    dav_fs: &fs::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    resource: &PropfindResource,
) -> Result<Vec<Element>, HttpResponse> {
    let mut elements = Vec::new();
    for name in standard_prop_name_list() {
        let requested = RequestedProp {
            name: name.to_string(),
            namespace: Some("DAV:".to_string()),
            prefix: Some("D".to_string()),
        };
        if standard_prop_element(lock_system, resource, &requested)
            .await?
            .is_some()
        {
            elements.push(requested.empty_element());
        }
    }
    let custom_props = if is_root_resource(resource) {
        Vec::new()
    } else {
        dav_fs
            .get_props(&resource.path, false)
            .await
            .map_err(fs_error_response)?
    };
    for prop in custom_props {
        elements.push(prop_element(
            &prop,
            Some(&RequestedProp {
                name: prop.name.clone(),
                namespace: prop.namespace.clone(),
                prefix: prop.prefix.clone(),
            }),
        ));
    }
    Ok(elements)
}

async fn requested_prop_elements(
    dav_fs: &fs::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    resource: &PropfindResource,
    requested: &[RequestedProp],
) -> Result<Vec<(StatusCode, Vec<Element>)>, HttpResponse> {
    let custom_props = if is_root_resource(resource) {
        Vec::new()
    } else {
        dav_fs
            .get_props(&resource.path, true)
            .await
            .map_err(fs_error_response)?
    };
    let mut ok = Vec::new();
    let mut missing = Vec::new();

    for prop in requested {
        if prop.is_system_namespace() {
            missing.push(prop.empty_element());
            continue;
        }

        if let Some(element) = standard_prop_element(lock_system, resource, prop).await? {
            ok.push(element);
            continue;
        }

        if let Some(stored) = custom_props
            .iter()
            .find(|candidate| prop.matches(candidate))
        {
            ok.push(prop_element(stored, Some(prop)));
        } else {
            missing.push(prop.empty_element());
        }
    }

    let mut result = Vec::new();
    if !ok.is_empty() {
        result.push((StatusCode::OK, ok));
    }
    if !missing.is_empty() {
        result.push((StatusCode::NOT_FOUND, missing));
    }
    Ok(result)
}

async fn standard_prop_element(
    lock_system: &dyn DavLockSystem,
    resource: &PropfindResource,
    requested: &RequestedProp,
) -> Result<Option<Element>, HttpResponse> {
    if requested.namespace.as_deref().unwrap_or("DAV:") != "DAV:" {
        return Ok(None);
    }

    let mut element = requested.empty_element();
    match requested.name.as_str() {
        "displayname" => {
            let display = display_name(&resource.relative);
            if !display.is_empty() {
                element.children.push(XMLNode::Text(display.to_string()));
            }
            Ok(Some(element))
        }
        "resourcetype" => {
            if resource.meta.is_dir() {
                element
                    .children
                    .push(XMLNode::Element(dav_element("collection")));
            }
            Ok(Some(element))
        }
        "getcontentlength" => {
            element
                .children
                .push(XMLNode::Text(resource.meta.len().to_string()));
            Ok(Some(element))
        }
        "getlastmodified" => {
            let modified = resource.meta.modified().map_err(fs_error_response)?;
            element
                .children
                .push(XMLNode::Text(format_http_date(modified)));
            Ok(Some(element))
        }
        "creationdate" => {
            let created = resource.meta.created().map_err(fs_error_response)?;
            element
                .children
                .push(XMLNode::Text(format_creation_date(created)));
            Ok(Some(element))
        }
        "getetag" => {
            if let Some(etag) = resource.meta.etag() {
                element.children.push(XMLNode::Text(format!("\"{etag}\"")));
            }
            Ok(Some(element))
        }
        "supportedlock" => {
            let supported = supportedlock_element();
            Ok(Some(supported))
        }
        "lockdiscovery" => {
            let locks = lock_system.discover(&resource.path).await;
            Ok(Some(lockdiscovery_element(&locks)))
        }
        _ => Ok(None),
    }
}

fn prop_from_xml(prop: &Element) -> DavProp {
    DavProp {
        name: prop.name.clone(),
        prefix: prop.prefix.clone(),
        namespace: prop.namespace.clone(),
        xml: prop.get_text().map(|text| text.into_owned().into_bytes()),
    }
}

fn prop_element(prop: &DavProp, requested: Option<&RequestedProp>) -> Element {
    let requested_prefix = requested.and_then(|prop| prop.prefix.as_deref());
    let requested_namespace = requested.and_then(|prop| prop.namespace.as_deref());
    let namespace = requested_namespace.or(prop.namespace.as_deref());
    let prefix = requested_prefix
        .or(prop.prefix.as_deref())
        .unwrap_or_else(|| default_prefix(namespace));
    let tag = if namespace.is_some() {
        format!("{prefix}:{}", prop.name)
    } else {
        prop.name.clone()
    };
    let mut element = Element::new(&tag);
    if let Some(namespace) = namespace
        && namespace != "DAV:"
    {
        element
            .attributes
            .insert(format!("xmlns:{prefix}"), namespace.to_string());
    }
    if let Some(xml) = &prop.xml
        && !xml.is_empty()
    {
        element
            .children
            .push(XMLNode::Text(String::from_utf8_lossy(xml).into_owned()));
    }
    element
}

fn supportedlock_element() -> Element {
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

fn lockdiscovery_element(locks: &[DavLock]) -> Element {
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

fn xml_response(root: Element, status: StatusCode) -> HttpResponse {
    match xml_bytes(&root) {
        Ok(body) => HttpResponse::build(status)
            .content_type(XML_CONTENT_TYPE)
            .body(body),
        Err(resp) => resp,
    }
}

fn xml_bytes(root: &Element) -> Result<Vec<u8>, HttpResponse> {
    let mut buffer = Vec::new();
    root.write(&mut buffer)
        .map_err(|_| HttpResponse::InternalServerError().finish())?;
    Ok(buffer)
}

fn dav_element(name: &str) -> Element {
    Element::new(&format!("D:{name}"))
}

fn text_element(tag: &str, text: &str) -> Element {
    let mut element = Element::new(tag);
    element.children.push(XMLNode::Text(text.to_string()));
    element
}

fn status_element(status: StatusCode) -> Element {
    text_element(
        "D:status",
        &format!(
            "HTTP/1.1 {} {}",
            status.as_u16(),
            status.canonical_reason().unwrap_or("Unknown"),
        ),
    )
}

fn child_elements(element: &Element) -> impl Iterator<Item = &Element> {
    element.children.iter().filter_map(|child| match child {
        XMLNode::Element(element) => Some(element),
        _ => None,
    })
}

fn child_relative_path(parent: &str, name: &[u8], is_dir: bool) -> String {
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

fn decoded_path_string(path: &DavPath) -> String {
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

fn href_for_dav_path(prefix: &str, path: &DavPath) -> String {
    href_for_relative(prefix, &decoded_path_string(path))
}

fn display_name(relative: &str) -> &str {
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

fn is_root_resource(resource: &PropfindResource) -> bool {
    resource.relative == "/"
}

fn standard_prop_name_list() -> [&'static str; 8] {
    [
        "displayname",
        "resourcetype",
        "getcontentlength",
        "getlastmodified",
        "creationdate",
        "getetag",
        "lockdiscovery",
        "supportedlock",
    ]
}

fn default_prefix(namespace: Option<&str>) -> &str {
    match namespace {
        Some("DAV:") => "D",
        Some(_) => "A",
        None => "",
    }
}

fn fs_error_response(err: FsError) -> HttpResponse {
    HttpResponse::build(fs_error_status(&err)).finish()
}

fn fs_error_status(err: &FsError) -> StatusCode {
    match err {
        FsError::NotFound => StatusCode::NOT_FOUND,
        FsError::Forbidden => StatusCode::FORBIDDEN,
        FsError::Exists => StatusCode::CONFLICT,
        FsError::InsufficientStorage => StatusCode::INSUFFICIENT_STORAGE,
        FsError::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
        FsError::GeneralFailure => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn format_http_date(time: std::time::SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(time)
        .format("%a, %d %b %Y %H:%M:%S GMT")
        .to_string()
}

fn format_creation_date(time: std::time::SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(time).to_rfc3339()
}

fn allow_header_value() -> &'static str {
    "OPTIONS, GET, HEAD, PUT, DELETE, MKCOL, COPY, MOVE, PROPFIND, PROPPATCH, LOCK, UNLOCK, REPORT, VERSION-CONTROL"
}

fn multi_status() -> StatusCode {
    StatusCode::MULTI_STATUS
}

fn unauthorized_response() -> HttpResponse {
    HttpResponse::Unauthorized()
        .insert_header(("WWW-Authenticate", "Basic realm=\"AsterDrive WebDAV\""))
        .body("Unauthorized")
}

impl RequestedProp {
    fn from(element: &Element) -> Self {
        Self {
            name: element.name.clone(),
            namespace: element.namespace.clone(),
            prefix: element.prefix.clone(),
        }
    }

    fn empty_element(&self) -> Element {
        let prefix = self
            .prefix
            .as_deref()
            .unwrap_or_else(|| default_prefix(self.namespace.as_deref()));
        let tag = if self.namespace.is_some() {
            format!("{prefix}:{}", self.name)
        } else {
            self.name.clone()
        };
        let mut element = Element::new(&tag);
        if let Some(namespace) = &self.namespace
            && namespace != "DAV:"
        {
            element
                .attributes
                .insert(format!("xmlns:{prefix}"), namespace.clone());
        }
        element
    }

    fn matches(&self, prop: &DavProp) -> bool {
        self.name == prop.name && self.namespace.as_deref() == prop.namespace.as_deref()
    }

    fn is_system_namespace(&self) -> bool {
        self.namespace
            .as_deref()
            .is_some_and(property_service::is_system_namespace)
    }
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
mod tests {
    use super::{handle_get_head, handle_put};
    use crate::cache;
    use crate::config::{CacheConfig, Config, DatabaseConfig, RuntimeConfig};
    use crate::db::repository::file_repo;
    use crate::entities::{file, file_blob, storage_policy, user};
    use crate::runtime::PrimaryAppState;
    use crate::services::{mail_service, policy_service};
    use crate::storage::driver::BlobMetadata;
    use crate::storage::{DriverRegistry, PolicySnapshot, StorageDriver, StreamUploadDriver};
    use crate::types::{
        DriverType, S3UploadStrategy, StoragePolicyOptions, StoredStoragePolicyAllowedTypes,
        UserRole, UserStatus, serialize_storage_policy_options,
    };
    use crate::webdav::dav::{DavLock, DavLockSystem, LsFuture};
    use crate::webdav::fs::AsterDavFs;
    use actix_web::body::to_bytes;
    use actix_web::http::{StatusCode, header};
    use actix_web::{FromRequest, web};
    use async_trait::async_trait;
    use chrono::Utc;
    use migration::Migrator;
    use quick_xml::Reader;
    use quick_xml::events::Event;
    use sea_orm::{ActiveModelTrait, Set};
    use std::collections::HashMap;
    use std::io;
    use std::path::PathBuf;
    use std::pin::Pin;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    use std::task::{Context, Poll};
    use std::time::Duration;
    use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt, ReadBuf};

    async fn build_webdav_test_state(
        driver_type: DriverType,
        options: crate::types::StoredStoragePolicyOptions,
        driver: Arc<dyn StorageDriver>,
    ) -> (PrimaryAppState, user::Model, storage_policy::Model, PathBuf) {
        let temp_root = std::env::temp_dir().join(format!(
            "asterdrive-webdav-handler-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&temp_root).expect("webdav handler temp root should exist");

        let db = crate::db::connect(&DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            pool_size: 1,
            retry_count: 0,
        })
        .await
        .expect("webdav handler database should connect");
        Migrator::up(&db, None)
            .await
            .expect("webdav handler migrations should succeed");

        let now = Utc::now();
        let policy = storage_policy::ActiveModel {
            name: Set("WebDAV Test Policy".to_string()),
            driver_type: Set(driver_type),
            endpoint: Set("https://mock-storage.example".to_string()),
            bucket: Set("mock-bucket".to_string()),
            access_key: Set("mock-access".to_string()),
            secret_key: Set("mock-secret".to_string()),
            base_path: Set(temp_root.to_string_lossy().into_owned()),
            max_file_size: Set(0),
            allowed_types: Set(StoredStoragePolicyAllowedTypes::empty()),
            options: Set(options),
            is_default: Set(true),
            chunk_size: Set(5_242_880),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("webdav handler policy should be inserted");

        let user = user::ActiveModel {
            username: Set("davhdl".to_string()),
            email: Set("davhdl@example.com".to_string()),
            password_hash: Set("unused".to_string()),
            role: Set(UserRole::User),
            status: Set(UserStatus::Active),
            session_version: Set(0),
            email_verified_at: Set(Some(now)),
            pending_email: Set(None),
            storage_used: Set(0),
            storage_quota: Set(0),
            policy_group_id: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            config: Set(None),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("webdav handler user should be inserted");

        policy_service::ensure_policy_groups_seeded(&db)
            .await
            .expect("webdav handler policy groups should be seeded");

        let policy_snapshot = Arc::new(PolicySnapshot::new());
        policy_snapshot
            .reload(&db)
            .await
            .expect("webdav handler policy snapshot should reload");

        let driver_registry = Arc::new(DriverRegistry::noop());
        driver_registry.insert_for_test(policy.id, driver);

        let runtime_config = Arc::new(RuntimeConfig::new());
        let cache = cache::create_cache(&CacheConfig {
            enabled: false,
            ..Default::default()
        })
        .await;

        let mut config = Config::default();
        config.server.temp_dir = temp_root.join(".tmp").to_string_lossy().into_owned();
        config.server.upload_temp_dir = temp_root.join(".uploads").to_string_lossy().into_owned();

        let (storage_change_tx, _) = tokio::sync::broadcast::channel(
            crate::services::storage_change_service::STORAGE_CHANGE_CHANNEL_CAPACITY,
        );
        let share_download_rollback =
            crate::services::share_service::spawn_detached_share_download_rollback_queue(
                db.clone(),
                crate::config::operations::share_download_rollback_queue_capacity(&runtime_config),
            );

        let state = PrimaryAppState {
            db_handles: crate::db::DbHandles::single(db.clone()),
            driver_registry,
            runtime_config: runtime_config.clone(),
            policy_snapshot,
            config: Arc::new(config),
            cache,
            metrics: crate::metrics_core::NoopMetrics::arc(),
            mail_sender: mail_service::runtime_sender(runtime_config),
            storage_change_tx,
            share_download_rollback,
            background_task_dispatch_wakeup:
                crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
        };

        (state, user, policy, temp_root)
    }

    async fn create_root_file(
        state: &PrimaryAppState,
        user_id: i64,
        policy_id: i64,
        filename: &str,
        size: i64,
        storage_path: &str,
    ) -> (file::Model, file_blob::Model) {
        let now = Utc::now();
        let blob = file_repo::create_blob(
            state.writer_db(),
            file_blob::ActiveModel {
                hash: Set(format!("webdav-blob-{}", uuid::Uuid::new_v4())),
                size: Set(size),
                policy_id: Set(policy_id),
                storage_path: Set(storage_path.to_string()),
                ref_count: Set(1),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .expect("webdav handler blob should be inserted");

        let file = file_repo::create(
            state.writer_db(),
            file::ActiveModel {
                name: Set(filename.to_string()),
                folder_id: Set(None),
                team_id: Set(None),
                blob_id: Set(blob.id),
                size: Set(size),
                owner_user_id: Set(Some(user_id)),
                created_by_user_id: Set(Some(user_id)),
                created_by_username: Set("tester".to_string()),
                mime_type: Set("text/plain".to_string()),
                created_at: Set(now),
                updated_at: Set(now),
                deleted_at: Set(None),
                is_locked: Set(false),
                ..Default::default()
            },
        )
        .await
        .expect("webdav handler file should be inserted");

        (file, blob)
    }

    struct NoopLockSystem;

    impl DavLockSystem for NoopLockSystem {
        fn lock(
            &self,
            _path: &crate::webdav::dav::DavPath,
            _principal: Option<&str>,
            _owner: Option<&xmltree::Element>,
            _timeout: Option<Duration>,
            _shared: bool,
            _deep: bool,
        ) -> LsFuture<'_, Result<DavLock, DavLock>> {
            Box::pin(async { panic!("lock should not be called in these WebDAV handler tests") })
        }

        fn unlock(
            &self,
            _path: &crate::webdav::dav::DavPath,
            _token: &str,
        ) -> LsFuture<'_, Result<(), ()>> {
            Box::pin(async { Ok(()) })
        }

        fn refresh(
            &self,
            _path: &crate::webdav::dav::DavPath,
            _token: &str,
            _timeout: Option<Duration>,
        ) -> LsFuture<'_, Result<DavLock, ()>> {
            Box::pin(async { panic!("refresh should not be called in these WebDAV handler tests") })
        }

        fn check(
            &self,
            _path: &crate::webdav::dav::DavPath,
            _principal: Option<&str>,
            _ignore_principal: bool,
            _deep: bool,
            _submitted_tokens: &[String],
        ) -> LsFuture<'_, Result<(), DavLock>> {
            Box::pin(async { Ok(()) })
        }

        fn discover(&self, _path: &crate::webdav::dav::DavPath) -> LsFuture<'_, Vec<DavLock>> {
            Box::pin(async { Vec::new() })
        }

        fn delete(&self, _path: &crate::webdav::dav::DavPath) -> LsFuture<'_, Result<(), ()>> {
            Box::pin(async { Ok(()) })
        }
    }

    struct OneChunkThenErrorReader {
        yielded_first_chunk: bool,
    }

    impl AsyncRead for OneChunkThenErrorReader {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<Result<(), io::Error>> {
            if !self.yielded_first_chunk {
                self.yielded_first_chunk = true;
                buf.put_slice(b"abc");
                return Poll::Ready(Ok(()));
            }
            Poll::Ready(Err(io::Error::other(
                "intentional trailing read failure for direct-stream regression test",
            )))
        }
    }

    #[derive(Clone, Default)]
    struct TrailingErrorStreamDriver {
        get_stream_calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl StorageDriver for TrailingErrorStreamDriver {
        async fn put(&self, path: &str, _data: &[u8]) -> crate::errors::Result<String> {
            Ok(path.to_string())
        }

        async fn get(&self, _path: &str) -> crate::errors::Result<Vec<u8>> {
            Err(crate::errors::AsterError::storage_driver_error(
                "WebDAV direct-stream test should not use get()",
            ))
        }

        async fn get_stream(
            &self,
            _path: &str,
        ) -> crate::errors::Result<Box<dyn AsyncRead + Unpin + Send>> {
            self.get_stream_calls.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(OneChunkThenErrorReader {
                yielded_first_chunk: false,
            }))
        }

        async fn delete(&self, _path: &str) -> crate::errors::Result<()> {
            Ok(())
        }

        async fn exists(&self, _path: &str) -> crate::errors::Result<bool> {
            Ok(true)
        }

        async fn metadata(&self, _path: &str) -> crate::errors::Result<BlobMetadata> {
            Ok(BlobMetadata {
                size: 3,
                content_type: Some("text/plain".to_string()),
            })
        }
    }

    #[derive(Clone, Default)]
    struct CountingDirectUploadDriver {
        objects: Arc<Mutex<HashMap<String, Vec<u8>>>>,
        put_file_calls: Arc<AtomicUsize>,
        put_reader_calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl StorageDriver for CountingDirectUploadDriver {
        async fn put(&self, path: &str, data: &[u8]) -> crate::errors::Result<String> {
            self.objects
                .lock()
                .expect("direct upload test driver lock should succeed")
                .insert(path.to_string(), data.to_vec());
            Ok(path.to_string())
        }

        async fn get(&self, path: &str) -> crate::errors::Result<Vec<u8>> {
            Ok(self
                .objects
                .lock()
                .expect("direct upload test driver lock should succeed")
                .get(path)
                .cloned()
                .unwrap_or_default())
        }

        async fn get_stream(
            &self,
            path: &str,
        ) -> crate::errors::Result<Box<dyn AsyncRead + Unpin + Send>> {
            let payload = self
                .objects
                .lock()
                .expect("direct upload test driver lock should succeed")
                .get(path)
                .cloned()
                .unwrap_or_default();
            let (mut writer, reader) = tokio::io::duplex(payload.len().max(1));
            tokio::spawn(async move {
                if let Err(error) = writer.write_all(&payload).await {
                    tracing::trace!("mock direct upload stream write failed: {error}");
                }
                if let Err(error) = writer.shutdown().await {
                    tracing::trace!("mock direct upload stream shutdown failed: {error}");
                }
            });
            Ok(Box::new(reader))
        }

        async fn delete(&self, path: &str) -> crate::errors::Result<()> {
            self.objects
                .lock()
                .expect("direct upload test driver lock should succeed")
                .remove(path);
            Ok(())
        }

        async fn exists(&self, path: &str) -> crate::errors::Result<bool> {
            Ok(self
                .objects
                .lock()
                .expect("direct upload test driver lock should succeed")
                .contains_key(path))
        }

        async fn metadata(&self, path: &str) -> crate::errors::Result<BlobMetadata> {
            let size = self
                .objects
                .lock()
                .expect("direct upload test driver lock should succeed")
                .get(path)
                .map(|bytes| u64::try_from(bytes.len()).expect("mock object size should fit u64"))
                .unwrap_or(0);
            Ok(BlobMetadata {
                size,
                content_type: Some("text/plain".to_string()),
            })
        }

        fn as_stream_upload(&self) -> Option<&dyn StreamUploadDriver> {
            Some(self)
        }
    }

    #[async_trait]
    impl StreamUploadDriver for CountingDirectUploadDriver {
        async fn put_file(
            &self,
            storage_path: &str,
            local_path: &str,
        ) -> crate::errors::Result<String> {
            self.put_file_calls.fetch_add(1, Ordering::SeqCst);
            let data = tokio::fs::read(local_path).await.map_err(|error| {
                crate::errors::AsterError::storage_driver_error(format!(
                    "direct upload test put_file failed: {error}"
                ))
            })?;
            self.objects
                .lock()
                .expect("direct upload test driver lock should succeed")
                .insert(storage_path.to_string(), data);
            Ok(storage_path.to_string())
        }

        async fn put_reader(
            &self,
            storage_path: &str,
            mut reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
            _size: i64,
        ) -> crate::errors::Result<String> {
            self.put_reader_calls.fetch_add(1, Ordering::SeqCst);
            let mut data = Vec::new();
            reader.read_to_end(&mut data).await.map_err(|error| {
                crate::errors::AsterError::storage_driver_error(format!(
                    "direct upload test put_reader failed: {error}"
                ))
            })?;
            self.objects
                .lock()
                .expect("direct upload test driver lock should succeed")
                .insert(storage_path.to_string(), data);
            Ok(storage_path.to_string())
        }
    }

    #[actix_web::test]
    async fn handle_get_returns_response_before_consuming_the_storage_stream() {
        let driver = TrailingErrorStreamDriver::default();
        let get_stream_calls = driver.get_stream_calls.clone();
        let (state, user, policy, temp_root) = build_webdav_test_state(
            DriverType::Local,
            crate::types::StoredStoragePolicyOptions::empty(),
            Arc::new(driver),
        )
        .await;
        create_root_file(
            &state,
            user.id,
            policy.id,
            "streamed.txt",
            3,
            "files/streamed.txt",
        )
        .await;

        let dav_fs = AsterDavFs::new(state.clone(), user.id, None);
        let req = actix_web::test::TestRequest::get()
            .uri("/webdav/streamed.txt")
            .to_http_request();
        let response = handle_get_head(&req, &dav_fs, "/webdav", false).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            get_stream_calls.load(Ordering::SeqCst),
            1,
            "GET should open exactly one streaming reader from storage"
        );

        drop(state);
        let _ = std::fs::remove_dir_all(temp_root);
    }

    #[actix_web::test]
    async fn propfind_href_is_percent_encoded_and_xml_parseable() {
        let driver = CountingDirectUploadDriver::default();
        let (state, user, policy, temp_root) = build_webdav_test_state(
            DriverType::Local,
            crate::types::StoredStoragePolicyOptions::empty(),
            std::sync::Arc::new(driver),
        )
        .await;
        let filename = "测试 文件 & report.txt";
        create_root_file(
            &state,
            user.id,
            policy.id,
            filename,
            4,
            "files/weird-name.txt",
        )
        .await;

        let dav_fs = AsterDavFs::new(state.clone(), user.id, None);
        let lock_system = NoopLockSystem;
        let encoded_uri = format!("/webdav{}", super::encode_href(&format!("/{filename}")));
        let req = actix_web::test::TestRequest::default()
            .method(actix_web::http::Method::from_bytes(b"PROPFIND").expect("valid method"))
            .uri(&encoded_uri)
            .to_http_request();

        let response = super::handle_propfind(&req, &dav_fs, &lock_system, "/webdav", &[]).await;

        assert_eq!(response.status(), StatusCode::from_u16(207).unwrap());
        let body = to_bytes(response.into_body())
            .await
            .expect("PROPFIND response body should be readable");

        let mut reader = Reader::from_reader(body.as_ref());
        let mut buf = Vec::new();
        let mut in_href = false;
        let mut hrefs = Vec::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(event))
                    if event.name().as_ref() == b"D:href" || event.name().as_ref() == b"href" =>
                {
                    in_href = true;
                }
                Ok(Event::Text(text)) if in_href => {
                    hrefs.push(text.decode().expect("href text should decode").into_owned());
                    in_href = false;
                }
                Ok(Event::End(event))
                    if event.name().as_ref() == b"D:href" || event.name().as_ref() == b"href" =>
                {
                    in_href = false;
                }
                Ok(Event::Eof) => break,
                Ok(_) => {}
                Err(error) => panic!("PROPFIND XML should parse cleanly: {error}"),
            }
            buf.clear();
        }

        assert_eq!(hrefs.len(), 1);
        let decoded = percent_encoding::percent_decode_str(&hrefs[0])
            .decode_utf8_lossy()
            .into_owned();
        assert_eq!(decoded, format!("/webdav/{filename}"));

        drop(state);
        let _ = std::fs::remove_dir_all(temp_root);
    }

    #[actix_web::test]
    async fn handle_head_does_not_open_the_storage_stream() {
        let driver = TrailingErrorStreamDriver::default();
        let get_stream_calls = driver.get_stream_calls.clone();
        let (state, user, policy, temp_root) = build_webdav_test_state(
            DriverType::Local,
            crate::types::StoredStoragePolicyOptions::empty(),
            Arc::new(driver),
        )
        .await;
        create_root_file(&state, user.id, policy.id, "head.txt", 3, "files/head.txt").await;

        let dav_fs = AsterDavFs::new(state.clone(), user.id, None);
        let req = actix_web::test::TestRequest::default()
            .method(actix_web::http::Method::HEAD)
            .uri("/webdav/head.txt")
            .to_http_request();
        let response = handle_get_head(&req, &dav_fs, "/webdav", true).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            get_stream_calls.load(Ordering::SeqCst),
            0,
            "HEAD should return metadata without opening the storage stream"
        );

        drop(state);
        let _ = std::fs::remove_dir_all(temp_root);
    }

    #[actix_web::test]
    async fn handle_put_with_content_length_uses_direct_s3_stream_upload() {
        let driver = CountingDirectUploadDriver::default();
        let put_file_calls = driver.put_file_calls.clone();
        let put_reader_calls = driver.put_reader_calls.clone();
        let options = serialize_storage_policy_options(&StoragePolicyOptions {
            s3_upload_strategy: Some(S3UploadStrategy::RelayStream),
            ..Default::default()
        })
        .expect("direct upload policy options should serialize");
        let (state, user, _policy, temp_root) =
            build_webdav_test_state(DriverType::S3, options, Arc::new(driver.clone())).await;

        let dav_fs = AsterDavFs::new(state.clone(), user.id, None);
        let lock_system = NoopLockSystem;
        let system_file_policy =
            crate::webdav::system_file::SystemFileBlockPolicy::from_runtime_config(
                &state.runtime_config,
            );
        let body = "webdav direct stream upload";
        let (req, mut dev_payload) = actix_web::test::TestRequest::put()
            .uri("/webdav/direct.txt")
            .insert_header((header::CONTENT_LENGTH, body.len().to_string()))
            .set_payload(body)
            .to_http_parts();
        let mut payload = web::Payload::from_request(&req, &mut dev_payload)
            .await
            .expect("webdav test payload should extract");
        let response = handle_put(
            &req,
            &dav_fs,
            &lock_system,
            "/webdav",
            &system_file_policy,
            &mut payload,
        )
        .await;

        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(
            put_reader_calls.load(Ordering::SeqCst),
            1,
            "known-size WebDAV PUT should use StorageDriver::put_reader()"
        );
        assert_eq!(
            put_file_calls.load(Ordering::SeqCst),
            0,
            "known-size WebDAV PUT should not fall back to StorageDriver::put_file()"
        );

        let stored =
            file_repo::find_by_name_in_folder(state.writer_db(), user.id, None, "direct.txt")
                .await
                .expect("stored WebDAV file lookup should succeed")
                .expect("direct WebDAV PUT should create a file");
        assert_eq!(
            stored.size,
            i64::try_from(body.len()).expect("request body length should fit i64")
        );
        assert!(
            driver
                .objects
                .lock()
                .expect("direct upload test driver lock should succeed")
                .values()
                .any(|bytes| bytes.as_slice() == body.as_bytes()),
            "direct stream upload should persist the request payload"
        );

        drop(state);
        let _ = std::fs::remove_dir_all(temp_root);
    }
}

//! WebDAV 模块导出。

pub mod auth;
pub mod dav;
pub mod db_lock_system;
pub mod deltav;
pub mod dir_entry;
mod download_audit;
pub mod file;
pub mod fs;
mod locks;
pub mod metadata;
pub mod path_resolver;
mod props;
mod protocol;
mod resources;
mod responses;
pub mod system_file;
mod transfer;

use actix_web::http::{StatusCode, header};
use actix_web::{HttpRequest, HttpResponse, web};
use futures::StreamExt;
use std::io::Cursor;
use xml::reader::{EventReader, XmlEvent};
use xmltree::{Element, XMLNode};

use crate::config::{NetworkTrustConfig, RateLimitConfig, WebDavConfig};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::ops::audit;
use crate::webdav::dav::{DavLockSystem, DavPath};
use aster_forge_utils::numbers::u64_to_usize;

pub(crate) use responses::{
    fs_error_response, lock_token_matches_request_uri_response, lock_token_submitted_element,
    lock_token_submitted_response, xml_bytes, xml_response,
};

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

pub(crate) const WEB_DAV_XML_MAX_DEPTH: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum XmlSafetyError {
    ExternalEntity,
    TooDeep,
    Malformed,
}

pub(crate) fn reject_xml_dtd_or_entity(body: &[u8]) -> Result<(), ()> {
    let mut index = 0;
    while let Some(offset) = find_byte(&body[index..], b'<') {
        index += offset + 1;
        let Some(marker) = body.get(index) else {
            break;
        };
        let Some(after_bang) = body.get(index + 1..) else {
            break;
        };
        if *marker == b'!'
            && (starts_with_ascii_case_insensitive(after_bang, b"DOCTYPE")
                || starts_with_ascii_case_insensitive(after_bang, b"ENTITY"))
        {
            return Err(());
        }
    }
    Ok(())
}

pub(crate) fn validate_xml_safety(body: &[u8]) -> Result<(), XmlSafetyError> {
    if reject_xml_dtd_or_entity(body).is_err() {
        return Err(XmlSafetyError::ExternalEntity);
    }

    let mut depth = 0usize;
    let reader = EventReader::new(Cursor::new(body));
    for event in reader {
        match event.map_err(|_| XmlSafetyError::Malformed)? {
            XmlEvent::StartElement { .. } => {
                depth = depth.checked_add(1).ok_or(XmlSafetyError::TooDeep)?;
                if depth > WEB_DAV_XML_MAX_DEPTH {
                    return Err(XmlSafetyError::TooDeep);
                }
            }
            XmlEvent::EndElement { .. } => {
                depth = depth.checked_sub(1).ok_or(XmlSafetyError::Malformed)?;
            }
            XmlEvent::Doctype { .. } => return Err(XmlSafetyError::ExternalEntity),
            XmlEvent::EndDocument => {
                if depth != 0 {
                    return Err(XmlSafetyError::Malformed);
                }
            }
            _ => {}
        }
    }

    Ok(())
}

pub(crate) fn parse_webdav_element(body: &[u8]) -> Result<Element, XmlSafetyError> {
    validate_xml_safety(body)?;
    Element::parse(Cursor::new(body)).map_err(|_| XmlSafetyError::Malformed)
}

fn find_byte(haystack: &[u8], needle: u8) -> Option<usize> {
    haystack.iter().position(|byte| *byte == needle)
}

fn starts_with_ascii_case_insensitive(value: &[u8], prefix: &[u8]) -> bool {
    value.len() >= prefix.len() && value[..prefix.len()].eq_ignore_ascii_case(prefix)
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
    if !state
        .get_ref()
        .runtime_config()
        .get_bool_or("webdav_enabled", true)
    {
        return responses::webdav_disabled();
    }

    let auth_result = match auth::authenticate_webdav(&req, state.get_ref()).await {
        Ok(result) => result,
        Err(auth::WebdavAuthError::RateLimited { retry_after }) => {
            return responses::unauthorized_retry_after(retry_after);
        }
        Err(auth::WebdavAuthError::Rejected) => return responses::unauthorized(),
    };

    let audit_info = audit::AuditRequestInfo::from_request(&req);
    let audit_ctx = audit_info.to_context(auth_result.scope.actor_user_id());

    let dav_fs = fs::AsterDavFs::new_with_audit(
        state.get_ref().clone(),
        Some(auth_result.account_id),
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
                    state.get_ref().writer_db(),
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
                state.get_ref().writer_db(),
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
            let system_file_policy = system_file::SystemFileBlockPolicy::from_runtime_config(
                state.get_ref().runtime_config(),
            );
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
            let system_file_policy = system_file::SystemFileBlockPolicy::from_runtime_config(
                state.get_ref().runtime_config(),
            );
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
                let system_file_policy = system_file::SystemFileBlockPolicy::from_runtime_config(
                    state.get_ref().runtime_config(),
                );
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
                let system_file_policy = system_file::SystemFileBlockPolicy::from_runtime_config(
                    state.get_ref().runtime_config(),
                );
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
        _ => responses::method_not_allowed(allow_header_value()),
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
            Err(_) => return Err(responses::request_body_read_error()),
        };
        let next_len = match data.len().checked_add(chunk.len()) {
            Some(next_len) => next_len,
            None => return Err(responses::xml_body_too_large()),
        };
        if next_len > max_len {
            return Err(responses::xml_body_too_large());
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

    Err(responses::system_file_name_blocked())
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
        Err(lock) => Err(lock_token_submitted_response(
            StatusCode::LOCKED,
            prefix,
            &lock.path,
        )),
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
    let parent_path = DavPath::new(&parent).map_err(|_| responses::invalid_request_path())?;
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

pub(crate) fn request_origin(req: &HttpRequest) -> (String, String) {
    let connection = req.connection_info();
    (
        connection.scheme().to_string(),
        connection.host().to_string(),
    )
}

pub(crate) fn decode_relative_path(relative: &str) -> Result<(DavPath, String), HttpResponse> {
    let normalized = normalize_relative_path(relative);
    let path = DavPath::new(&normalized).map_err(|_| responses::invalid_request_path())?;
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

pub(crate) fn format_creation_date(time: std::time::SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(time).to_rfc3339()
}

fn allow_header_value() -> &'static str {
    "OPTIONS, GET, HEAD, PUT, DELETE, MKCOL, COPY, MOVE, PROPFIND, PROPPATCH, LOCK, UNLOCK, REPORT, VERSION-CONTROL"
}

pub(crate) fn multi_status() -> StatusCode {
    StatusCode::MULTI_STATUS
}

/// 注册 WebDAV 路由
pub fn configure(
    cfg: &mut web::ServiceConfig,
    webdav_config: &WebDavConfig,
    db: &sea_orm::DatabaseConnection,
) {
    let config = crate::config::try_get_config()
        .map(|config| (*config).clone())
        .unwrap_or_default();
    configure_with_rate_limit(
        cfg,
        webdav_config,
        db,
        &config.rate_limit,
        &config.network_trust,
    );
}

pub fn configure_with_rate_limit(
    cfg: &mut web::ServiceConfig,
    webdav_config: &WebDavConfig,
    _db: &sea_orm::DatabaseConnection,
    rate_limit: &RateLimitConfig,
    network_trust: &NetworkTrustConfig,
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

    let auth_protection = web::Data::new(auth::WebdavAuthProtection::new(
        rate_limit.enabled,
        &rate_limit.auth,
        &network_trust.trusted_proxies,
    ));

    cfg.app_data(webdav_state)
        .app_data(auth_protection)
        .service(
            web::scope(&webdav_config.prefix)
                .app_data(web::PayloadConfig::new(payload_limit))
                .default_service(web::to(webdav_handler)),
        );
}

#[cfg(test)]
mod handler_tests;

#[cfg(test)]
mod tests {
    use super::{
        WEB_DAV_XML_MAX_DEPTH, XmlSafetyError, parse_webdav_element, reject_xml_dtd_or_entity,
        validate_xml_safety,
    };

    #[test]
    fn reject_xml_dtd_or_entity_allows_regular_webdav_xml() {
        let body = br#"<?xml version="1.0"?><propfind xmlns="DAV:"><allprop/></propfind>"#;

        assert!(reject_xml_dtd_or_entity(body).is_ok());
    }

    #[test]
    fn reject_xml_dtd_or_entity_rejects_doctype_case_insensitively() {
        let body = br#"<!doctype propfind [<!ENTITY x "boom">]><propfind/>"#;

        assert!(reject_xml_dtd_or_entity(body).is_err());
    }

    #[test]
    fn reject_xml_dtd_or_entity_rejects_entity_declaration() {
        let body = br#"<!ENTITY x "boom"><propfind/>"#;

        assert!(reject_xml_dtd_or_entity(body).is_err());
    }

    #[test]
    fn reject_xml_dtd_or_entity_handles_trailing_markup_start() {
        assert!(reject_xml_dtd_or_entity(b"<").is_ok());
    }

    #[test]
    fn validate_xml_safety_rejects_deep_nesting_without_recursive_parse() {
        let mut body = String::from("<propfind>");
        for _ in 1..WEB_DAV_XML_MAX_DEPTH {
            body.push_str("<prop>");
        }
        for _ in 1..WEB_DAV_XML_MAX_DEPTH {
            body.push_str("</prop>");
        }
        body.push_str("</propfind>");

        assert_eq!(validate_xml_safety(body.as_bytes()), Ok(()));

        let mut too_deep = String::from("<propfind>");
        for _ in 1..=WEB_DAV_XML_MAX_DEPTH {
            too_deep.push_str("<prop>");
        }
        for _ in 1..=WEB_DAV_XML_MAX_DEPTH {
            too_deep.push_str("</prop>");
        }
        too_deep.push_str("</propfind>");
        assert_eq!(
            validate_xml_safety(too_deep.as_bytes()),
            Err(XmlSafetyError::TooDeep)
        );
    }

    #[test]
    fn parse_webdav_element_rejects_probe_depth_before_xmltree_parse() {
        const PROBE_DEPTH: usize = 30_000;

        let mut body = String::with_capacity(64 + PROBE_DEPTH * 7);
        body.push_str(r#"<D:propfind xmlns:D="DAV:"><D:prop>"#);
        for _ in 0..PROBE_DEPTH {
            body.push_str("<x>");
        }
        for _ in 0..PROBE_DEPTH {
            body.push_str("</x>");
        }
        body.push_str("</D:prop></D:propfind>");

        assert_eq!(
            parse_webdav_element(body.as_bytes()),
            Err(XmlSafetyError::TooDeep)
        );
    }

    #[test]
    fn validate_xml_safety_rejects_doctype_and_malformed_xml() {
        assert_eq!(
            validate_xml_safety(br#"<!DOCTYPE propfind><propfind/>"#),
            Err(XmlSafetyError::ExternalEntity)
        );
        assert_eq!(
            validate_xml_safety(br#"<propfind>"#),
            Err(XmlSafetyError::Malformed)
        );
    }
}

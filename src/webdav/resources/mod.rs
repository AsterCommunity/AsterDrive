//! WebDAV resource mutation handlers: MKCOL, DELETE, COPY, MOVE.

use actix_web::http::header;
use actix_web::{HttpRequest, HttpResponse, web};

use crate::webdav::dav::{DavFileSystem, DavLockSystem, DavPath, FsError};
use crate::webdav::{
    collect_payload, decode_relative_path, decoded_path_string, ensure_system_file_name_allowed,
    ensure_unlocked, fs, fs_error_response, href_for_relative, request_path, system_file,
};

pub(crate) async fn handle_mkcol(
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

pub(crate) async fn handle_delete(
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

pub(crate) async fn handle_copy_move(
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

fn overwrite_enabled(headers: &header::HeaderMap) -> bool {
    headers
        .get("Overwrite")
        .and_then(|value| value.to_str().ok())
        .is_none_or(|value| !value.eq_ignore_ascii_case("F"))
}

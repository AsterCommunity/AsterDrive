//! WebDAV resource mutation handlers: MKCOL, DELETE, COPY, MOVE.

use actix_web::http::header;
use actix_web::{HttpRequest, HttpResponse, web};
use futures::StreamExt;

use crate::webdav::dav::{DavFileSystem, DavLockSystem, DavPath, FsError};
use crate::webdav::protocol::{self, Depth};
use crate::webdav::{
    decoded_path_string, ensure_system_file_name_allowed, ensure_unlocked, fs, fs_error_response,
    href_for_relative, request_path, system_file,
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
    if let Err(resp) = protocol::parse_delete_depth(req.headers()) {
        return resp;
    }

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
    let depth = if is_move {
        match protocol::parse_move_depth(req.headers()) {
            Ok(depth) => depth,
            Err(resp) => return resp,
        }
    } else {
        match protocol::parse_copy_depth(req.headers()) {
            Ok(depth) => depth,
            Err(resp) => return resp,
        }
    };

    let (source, source_relative) = match request_path(req, prefix) {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let destination_relative = match protocol::destination_relative_path(req.headers(), prefix) {
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

    let source_meta = match dav_fs.metadata(&source).await {
        Ok(meta) => meta,
        Err(err) => return fs_error_response(err),
    };
    let destination_exists = dav_fs.metadata(&destination).await.is_ok();
    if !protocol::overwrite_enabled(req.headers()) && destination_exists {
        return HttpResponse::PreconditionFailed().finish();
    }

    let result = if is_move {
        dav_fs.rename(&source, &destination).await
    } else if source_meta.is_dir() && depth == Depth::Zero {
        dav_fs.copy_dir_shallow(&source, &destination).await
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
    while let Some(chunk) = payload.next().await {
        let chunk =
            chunk.map_err(|_| HttpResponse::BadRequest().body("Failed to read request body"))?;
        if !chunk.is_empty() {
            return Err(HttpResponse::UnsupportedMediaType().finish());
        }
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::ensure_empty_body;
    use actix_web::FromRequest;
    use actix_web::http::StatusCode;
    use actix_web::web;
    use bytes::Bytes;

    async fn payload_from_bytes(bytes: Bytes) -> web::Payload {
        let (req, mut dev_payload) = actix_web::test::TestRequest::default()
            .set_payload(bytes)
            .to_http_parts();
        web::Payload::from_request(&req, &mut dev_payload)
            .await
            .expect("test payload should extract")
    }

    #[actix_web::test]
    async fn ensure_empty_body_accepts_empty_payload() {
        let mut payload = payload_from_bytes(Bytes::new()).await;

        ensure_empty_body(&mut payload)
            .await
            .expect("empty MKCOL body should be accepted");
    }

    #[actix_web::test]
    async fn ensure_empty_body_ignores_empty_chunks() {
        let mut payload = payload_from_bytes(Bytes::new()).await;

        ensure_empty_body(&mut payload)
            .await
            .expect("empty MKCOL body chunks should be accepted");
    }

    #[actix_web::test]
    async fn ensure_empty_body_rejects_first_non_empty_chunk() {
        let mut payload = payload_from_bytes(Bytes::from_static(b"x")).await;

        let response = ensure_empty_body(&mut payload)
            .await
            .expect_err("non-empty MKCOL body should be rejected");

        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[actix_web::test]
    async fn ensure_empty_body_stops_after_first_non_empty_chunk() {
        let mut payload = payload_from_bytes(Bytes::from(vec![b'x'; 2 * 1024 * 1024])).await;

        let response = ensure_empty_body(&mut payload)
            .await
            .expect_err("large non-empty MKCOL body should be rejected");

        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
}

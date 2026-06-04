//! WebDAV GET/HEAD/PUT transfer handlers.

use actix_web::http::{StatusCode, header};
use actix_web::{HttpRequest, HttpResponse, web};
use futures::StreamExt;
use tokio_util::io::ReaderStream;

use crate::services::file_service;
use crate::webdav::dav::{DavFileSystem, DavLockSystem, FsError, OpenOptions};
use crate::webdav::{
    ensure_system_file_name_allowed, ensure_unlocked, fs, fs_error_response, href_for_relative,
    request_path, system_file,
};

const CHUNK_SIZE: usize = 16 * 1024;

pub(crate) async fn handle_get_head(
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

    let content_type = mime_guess::from_path(relative.trim_end_matches('/'))
        .first_or_octet_stream()
        .essence_str()
        .to_string();
    let range = match file_service::parse_range_header(
        req.headers().get(header::RANGE),
        i64::try_from(meta.len()).unwrap_or(i64::MAX),
    ) {
        Ok(range) => range,
        Err(_) => return HttpResponse::RangeNotSatisfiable().finish(),
    };
    let (status, content_length, content_range) = match range {
        Some(range) => (
            StatusCode::PARTIAL_CONTENT,
            range.length(),
            Some(range.content_range_header()),
        ),
        None => (StatusCode::OK, meta.len(), None),
    };

    let mut response = HttpResponse::build(status);
    response.insert_header((header::CONTENT_LENGTH, content_length.to_string()));
    response.insert_header((header::CONTENT_TYPE, content_type));
    response.insert_header(("Accept-Ranges", "bytes"));
    response.insert_header((header::CONTENT_ENCODING, "identity"));
    if let Some(content_range) = content_range {
        response.insert_header((header::CONTENT_RANGE, content_range));
    }
    if let Some(etag) = meta.etag() {
        response.insert_header((header::ETAG, format!("\"{etag}\"")));
    }

    if head_only {
        return response.finish();
    }

    // GET must stream directly from storage; do not fall back to DavFileSystem::open(read).
    let stream = match match range {
        Some(range) => {
            dav_fs
                .open_read_stream_with_range(&path, Some(range.start()), Some(range.length()))
                .await
        }
        None => dav_fs.open_read_stream(&path).await,
    } {
        Ok(stream) => stream,
        Err(err) => return fs_error_response(err),
    };
    response.streaming(ReaderStream::with_capacity(stream, CHUNK_SIZE))
}

pub(crate) async fn handle_put(
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
    let existed = match dav_fs.metadata(&path).await {
        Ok(_) => true,
        Err(FsError::NotFound) => false,
        Err(err) => return fs_error_response(err),
    };

    if let Err(resp) = ensure_unlocked(lock_system, &path, false, req.headers()).await {
        return resp;
    }

    let create_new = header_equals(req.headers(), header::IF_NONE_MATCH, "*");
    let create = !header_equals(req.headers(), header::IF_MATCH, "*");
    let mut options = OpenOptions::write();
    options.create = create;
    options.create_new = create_new;
    options.truncate = true;
    options.size = content_length_hint(req.headers());

    let mut file = match dav_fs.open(&path, options).await {
        Ok(file) => file,
        Err(FsError::Exists) => return HttpResponse::PreconditionFailed().finish(),
        Err(FsError::NotFound) => return HttpResponse::Conflict().finish(),
        Err(err) => {
            tracing::warn!(path = %relative, error = %err, "WebDAV PUT open failed");
            return fs_error_response(err);
        }
    };

    while let Some(chunk) = payload.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(_) => return HttpResponse::BadRequest().body("Failed to read request body"),
        };
        if let Err(err) = file.write_bytes(chunk).await {
            tracing::warn!(path = %relative, error = %err, "WebDAV PUT write failed");
            return fs_error_response(err);
        }
    }

    if let Err(err) = file.flush().await {
        tracing::warn!(path = %relative, error = %err, "WebDAV PUT flush failed");
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

fn content_length_hint(headers: &header::HeaderMap) -> Option<u64> {
    headers
        .get("X-Expected-Entity-Length")
        .or_else(|| headers.get(header::CONTENT_LENGTH))
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
}

fn header_equals(headers: &header::HeaderMap, name: header::HeaderName, expected: &str) -> bool {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim() == expected)
}

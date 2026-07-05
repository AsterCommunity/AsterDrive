//! WebDAV GET/HEAD/PUT transfer handlers.

use std::time::Instant;

use actix_web::http::{StatusCode, header};
use actix_web::{HttpRequest, HttpResponse, web};
use futures::StreamExt;
use tokio_util::io::ReaderStream;

use crate::services::file_service;
use crate::webdav::dav::{DavFileSystem, DavLockSystem, DavMetaData, FsError, OpenOptions};
use crate::webdav::{
    ensure_parent_unlocked, ensure_system_file_name_allowed, ensure_unlocked, format_http_date, fs,
    fs_error_response, href_for_relative, protocol, request_origin, request_path, responses,
    system_file,
};
use protocol::HttpEtagPrecondition;

const CHUNK_SIZE: usize = 64 * 1024;

pub(crate) async fn handle_get_head(
    req: &HttpRequest,
    dav_fs: &fs::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    head_only: bool,
) -> HttpResponse {
    let request_started_at = Instant::now();
    let (path, relative) = match request_path(req, prefix) {
        Ok(v) => v,
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

    let resolve_started_at = Instant::now();
    let target = match dav_fs.resolve_download_target(&path).await {
        Ok(target) => target,
        Err(err) => return fs_error_response(err),
    };
    let resolve_elapsed_ms = resolve_started_at.elapsed().as_millis();
    let fs::AsterDavDownloadTarget::File { file, blob, meta } = target else {
        return responses::empty(StatusCode::METHOD_NOT_ALLOWED);
    };
    let last_modified = match meta.modified() {
        Ok(modified) => modified,
        Err(err) => return fs_error_response(err),
    };
    let last_modified_header = format_http_date(last_modified);
    let etag = meta.etag();
    match protocol::evaluate_http_download_preconditions(
        req.headers(),
        etag.as_deref(),
        Some(last_modified),
    ) {
        Ok(HttpEtagPrecondition::Proceed) => {}
        Ok(HttpEtagPrecondition::NotModified) => {
            let mut response = HttpResponse::NotModified();
            response.insert_header((header::LAST_MODIFIED, last_modified_header));
            if let Some(etag) = etag {
                response.insert_header((header::ETAG, format!("\"{etag}\"")));
            }
            return response.finish();
        }
        Err(resp) => return resp,
    }

    let content_type = mime_guess::from_path(relative.trim_end_matches('/'))
        .first_or_octet_stream()
        .essence_str()
        .to_string();
    let range = if head_only {
        None
    } else {
        match file_service::parse_range_header(
            req.headers().get(header::RANGE),
            i64::try_from(meta.len()).unwrap_or(i64::MAX),
        ) {
            Ok(range) => range,
            Err(_) => return range_not_satisfiable_response(meta.len()),
        }
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
    response.insert_header((header::LAST_MODIFIED, last_modified_header));
    if let Some(content_range) = content_range {
        response.insert_header((header::CONTENT_RANGE, content_range));
    }
    if let Some(etag) = etag {
        response.insert_header((header::ETAG, format!("\"{etag}\"")));
    }

    if head_only {
        return response.finish();
    }

    // GET must stream directly from storage; do not fall back to DavFileSystem::open(read).
    let storage_started_at = Instant::now();
    let stream = match match range {
        Some(range) => {
            dav_fs
                .open_download_stream_for_file(
                    &file,
                    &blob,
                    Some(range.start()),
                    Some(range.length()),
                )
                .await
        }
        None => {
            dav_fs
                .open_download_stream_for_file(&file, &blob, None, None)
                .await
        }
    } {
        Ok(stream) => stream,
        Err(err) => return fs_error_response(err),
    };
    tracing::debug!(
        path = %relative,
        head_only,
        has_range = range.is_some(),
        content_length,
        resolve_elapsed_ms,
        storage_open_elapsed_ms = storage_started_at.elapsed().as_millis(),
        total_prepare_elapsed_ms = request_started_at.elapsed().as_millis(),
        "WebDAV GET/HEAD stream prepared"
    );
    response.streaming(ReaderStream::with_capacity(stream, CHUNK_SIZE))
}

fn range_not_satisfiable_response(length: u64) -> HttpResponse {
    responses::range_not_satisfiable(length)
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
        Ok(meta) if meta.is_dir() => return responses::empty(StatusCode::METHOD_NOT_ALLOWED),
        Ok(meta) => {
            if let Err(resp) = protocol::evaluate_http_etag_preconditions(
                req.headers(),
                true,
                meta.etag().as_deref(),
                false,
            ) {
                return resp;
            }
            true
        }
        Err(FsError::NotFound) => {
            if let Err(resp) =
                protocol::evaluate_http_etag_preconditions(req.headers(), false, None, false)
            {
                return resp;
            }
            false
        }
        Err(err) => return fs_error_response(err),
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
    if let Err(resp) = ensure_unlocked(
        lock_system,
        &path,
        false,
        prefix,
        req.headers(),
        &request_scheme,
        &request_host,
    )
    .await
    {
        return resp;
    }
    if !existed
        && let Err(resp) = ensure_parent_unlocked(
            lock_system,
            &relative,
            prefix,
            req.headers(),
            &request_scheme,
            &request_host,
        )
        .await
    {
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
        Err(FsError::Exists) => return responses::precondition_failed(),
        Err(FsError::NotFound) => return responses::conflict(),
        Err(err) => {
            tracing::warn!(path = %relative, error = %err, "WebDAV PUT open failed");
            return fs_error_response(err);
        }
    };

    while let Some(chunk) = payload.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(_) => return responses::request_body_read_error(),
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
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .or_else(|| {
            headers
                .get(header::CONTENT_LENGTH)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.trim().parse::<u64>().ok())
        })
}

fn header_equals(headers: &header::HeaderMap, name: header::HeaderName, expected: &str) -> bool {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim() == expected)
}

#[cfg(test)]
mod tests {
    use actix_web::http::header::{self, HeaderMap, HeaderName, HeaderValue};

    use super::content_length_hint;

    fn expected_entity_length_header() -> HeaderName {
        HeaderName::from_static("x-expected-entity-length")
    }

    #[test]
    fn content_length_hint_prefers_valid_expected_entity_length() {
        let mut headers = HeaderMap::new();
        headers.insert(
            expected_entity_length_header(),
            HeaderValue::from_static(" 8 "),
        );
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from_static("4"));

        assert_eq!(content_length_hint(&headers), Some(8));
    }

    #[test]
    fn content_length_hint_falls_back_to_content_length_when_expected_is_invalid() {
        let mut headers = HeaderMap::new();
        headers.insert(
            expected_entity_length_header(),
            HeaderValue::from_static("invalid"),
        );
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from_static("4"));

        assert_eq!(content_length_hint(&headers), Some(4));
    }

    #[test]
    fn content_length_hint_returns_none_when_no_header_can_be_parsed() {
        let mut headers = HeaderMap::new();
        headers.insert(
            expected_entity_length_header(),
            HeaderValue::from_static("-1"),
        );
        headers.insert(
            header::CONTENT_LENGTH,
            HeaderValue::from_static("not-a-number"),
        );

        assert_eq!(content_length_hint(&headers), None);
    }
}

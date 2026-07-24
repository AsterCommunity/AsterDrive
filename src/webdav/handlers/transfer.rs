//! WebDAV GET/HEAD/PUT transfer handlers.

use std::time::Instant;

use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse, web};
use aster_forge_webdav::{
    DavBackendError, DavBackendErrorKind, DavDownloadBody, DavDownloadPlanError,
    DavPutResourceState, DavRequestHead, DavResponseBody, plan_download_response, plan_put_request,
    put_plan_error_response, put_success_response,
};
use futures::StreamExt;
use tokio_util::io::ReaderStream;

use crate::webdav::{
    backend, ensure_parent_unlocked, ensure_system_file_name_allowed, ensure_unlocked,
    fs_error_response, protocol, responses, system_file,
};
use aster_forge_webdav::{DavFileSystem, DavLockSystem, DavMetaData, FsError, OpenOptions};

const CHUNK_SIZE: usize = 64 * 1024;

pub(crate) async fn handle_get_head(
    req: &HttpRequest,
    request_head: &DavRequestHead,
    dav_fs: &backend::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    head_only: bool,
) -> HttpResponse {
    let request_started_at = Instant::now();
    let path = request_head.target.clone();
    let relative = path.as_str().to_owned();
    let request_scheme = request_head.origin.scheme.as_str();
    let request_host = request_head.origin.host.as_str();
    if let Err(resp) = protocol::ensure_if_header(
        request_head.if_header.as_ref(),
        dav_fs,
        lock_system,
        &path,
        prefix,
        request_scheme,
        request_host,
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
    let Some(backend::AsterDavDownloadFile { file, blob, meta }) = target else {
        return aster_forge_webdav::actix::into_response(
            aster_forge_webdav::method_not_allowed_response(),
        );
    };
    let last_modified = match meta.modified() {
        Ok(modified) => modified,
        Err(err) => return fs_error_response(err),
    };
    let etag = meta.etag();
    let content_type = meta
        .content_type()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            mime_guess::from_path(relative.trim_end_matches('/'))
                .first_or_octet_stream()
                .essence_str()
                .to_string()
        });
    let headers = match protocol::converted_headers(req.headers()) {
        Ok(headers) => headers,
        Err(response) => return response,
    };
    let plan = match plan_download_response(
        &headers,
        head_only,
        meta.len(),
        &content_type,
        etag.as_deref(),
        last_modified,
    ) {
        Ok(plan) => plan,
        Err(DavDownloadPlanError::Protocol(error)) => {
            return protocol::protocol_error_response(error);
        }
        Err(DavDownloadPlanError::InvalidRepresentation) => {
            tracing::warn!(path = %relative, "invalid WebDAV download response metadata");
            return responses::empty(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    let mut response = plan.response;

    // GET must stream directly from storage; do not fall back to DavFileSystem::open(read).
    let storage_started_at = Instant::now();
    let (range_offset, range_length, content_length, has_range) = match plan.body {
        DavDownloadBody::Empty => return aster_forge_webdav::actix::into_response(response),
        DavDownloadBody::Full => (None, None, meta.len(), false),
        DavDownloadBody::Range(range) => (
            Some(range.start()),
            Some(range.length()),
            range.length(),
            true,
        ),
    };
    let stream = match dav_fs
        .open_download_stream_for_file(&file, &blob, range_offset, range_length)
        .await
    {
        Ok(stream) => stream,
        Err(err) => return fs_error_response(err),
    };
    tracing::debug!(
        path = %relative,
        head_only,
        has_range,
        content_length,
        resolve_elapsed_ms,
        storage_open_elapsed_ms = storage_started_at.elapsed().as_millis(),
        total_prepare_elapsed_ms = request_started_at.elapsed().as_millis(),
        "WebDAV GET/HEAD stream prepared"
    );
    let stream_path = relative.clone();
    let stream = ReaderStream::with_capacity(stream, CHUNK_SIZE).map(move |result| {
        result.map_err(|error| {
            tracing::warn!(path = %stream_path, error = %error, "WebDAV download stream failed");
            DavBackendError::new(DavBackendErrorKind::Internal)
        })
    });
    response.body = DavResponseBody::Stream(Box::pin(stream));
    aster_forge_webdav::actix::into_response(response)
}

pub(crate) async fn handle_put(
    req: &HttpRequest,
    request_head: &DavRequestHead,
    dav_fs: &backend::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    system_file_policy: &system_file::SystemFileBlockPolicy,
    payload: &mut web::Payload,
) -> HttpResponse {
    let path = request_head.target.clone();
    let relative = path.as_str().to_owned();
    if let Err(resp) = ensure_system_file_name_allowed(system_file_policy, &relative) {
        return resp;
    }
    let (resource_existed, target_is_collection, target_etag) = match dav_fs.metadata(&path).await {
        Ok(meta) => (true, meta.is_dir(), meta.etag()),
        Err(FsError::NotFound) => (false, false, None),
        Err(err) => return fs_error_response(err),
    };
    let headers = match protocol::converted_headers(req.headers()) {
        Ok(headers) => headers,
        Err(response) => return response,
    };
    let resource_state = if target_is_collection {
        DavPutResourceState::Collection
    } else if resource_existed {
        DavPutResourceState::File {
            etag: target_etag.as_deref(),
        }
    } else {
        DavPutResourceState::Missing
    };
    let plan = match plan_put_request(&headers, resource_state) {
        Ok(plan) => plan,
        Err(error) => {
            return aster_forge_webdav::actix::into_response(put_plan_error_response(&error));
        }
    };

    let request_scheme = request_head.origin.scheme.as_str();
    let request_host = request_head.origin.host.as_str();
    if let Err(resp) = protocol::ensure_if_header(
        request_head.if_header.as_ref(),
        dav_fs,
        lock_system,
        &path,
        prefix,
        request_scheme,
        request_host,
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
        request_head.if_header.as_ref(),
        request_scheme,
        request_host,
    )
    .await
    {
        return resp;
    }
    if !plan.resource_existed
        && let Err(resp) = ensure_parent_unlocked(
            lock_system,
            &relative,
            prefix,
            request_head.if_header.as_ref(),
            request_scheme,
            request_host,
        )
        .await
    {
        return resp;
    }

    let mut options = OpenOptions::write();
    options.create = plan.create;
    options.create_new = plan.create_new;
    options.truncate = true;
    options.size = plan.content_length_hint;

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

    match put_success_response(&plan, prefix, &path) {
        Ok(response) => aster_forge_webdav::actix::into_response(response),
        Err(_) => responses::empty(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

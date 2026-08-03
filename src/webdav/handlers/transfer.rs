//! WebDAV GET/HEAD/PUT transfer handlers.

use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse, web};
use aster_forge_webdav::{
    DavCapabilitySnapshot, DavDownloadOpenError, DavDownloadPlanError, DavMetaData,
    DavMultiRangeLimits, DavMultiRangePolicy, DavPutResourceState, DavPutWritePlan,
    DavRangeLimitBehavior, DavRequestHead, DavResponseBody, DavWriteHandle, DavWriteOptions,
    DavWriteSystem, open_download, plan_download_response_with_multi_range, plan_put_request,
    put_plan_error_response, put_success_response,
};
use futures::StreamExt;

use crate::webdav::{
    backend, ensure_system_file_name_allowed, fs_error_response, responses, system_file,
};
use aster_forge_webdav::{DavLockSystem, FsError};

const MULTI_RANGE_MAXIMUM_HEADER_BYTES: usize = 8 * 1024;
const MULTI_RANGE_MAXIMUM_RAW_RANGES: usize = 16;
const MULTI_RANGE_MAXIMUM_SEGMENTS: usize = 8;
const MULTI_RANGE_MAXIMUM_AGGREGATE_BYTES: u64 = 64 * 1024 * 1024;
const MULTI_RANGE_MAXIMUM_BACKEND_OPENS: usize = 8;
const MULTI_RANGE_COALESCE_GAP_BYTES: u64 = 80;
const MULTI_RANGE_POLICY: DavMultiRangePolicy = DavMultiRangePolicy::new(
    DavMultiRangeLimits::new(
        MULTI_RANGE_MAXIMUM_HEADER_BYTES,
        MULTI_RANGE_MAXIMUM_RAW_RANGES,
        MULTI_RANGE_MAXIMUM_SEGMENTS,
        MULTI_RANGE_MAXIMUM_AGGREGATE_BYTES,
        MULTI_RANGE_MAXIMUM_BACKEND_OPENS,
    ),
    MULTI_RANGE_COALESCE_GAP_BYTES,
    DavRangeLimitBehavior::IgnoreRange,
);

pub(crate) async fn handle_get_head(
    req: &HttpRequest,
    request_head: &DavRequestHead,
    dav_fs: &backend::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    head_only: bool,
    _capabilities: &DavCapabilitySnapshot,
) -> HttpResponse {
    let path = request_head.target.clone();
    let relative = path.as_str().to_owned();
    let request_scheme = request_head.origin.scheme.as_str();
    let request_host = request_head.origin.host.as_str();
    if let Err(resp) = aster_forge_webdav::actix::enforce_if_header_with_backends(
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

    let meta = match aster_forge_webdav::DavDownloadSource::metadata(dav_fs, &path).await {
        Ok(meta) => meta,
        Err(error) => {
            return aster_forge_webdav::actix::into_response(
                aster_forge_webdav::backend_error_response(&error),
            );
        }
    };
    crate::webdav::observation::add_backend_call();
    crate::webdav::observation::add_resource();
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
    let headers = match aster_forge_webdav::actix::converted_headers(req.headers()) {
        Ok(headers) => headers,
        Err(response) => return response,
    };
    let plan = match plan_download_response_with_multi_range(
        &headers,
        head_only,
        meta.len(),
        &content_type,
        etag.as_deref(),
        last_modified,
        MULTI_RANGE_POLICY,
    ) {
        Ok(plan) => plan,
        Err(DavDownloadPlanError::Protocol(error)) => {
            return aster_forge_webdav::actix::protocol_error_response(error);
        }
        Err(DavDownloadPlanError::InvalidRepresentation) => {
            tracing::warn!(path = %relative, "invalid WebDAV download response metadata");
            return responses::empty(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    let mut response = plan.response;
    let requested_ranges = requested_range_count(req);
    let served_ranges = match &plan.body {
        aster_forge_webdav::DavDownloadBody::Empty
        | aster_forge_webdav::DavDownloadBody::Full { .. } => 0,
        aster_forge_webdav::DavDownloadBody::Range(_) => 1,
        aster_forge_webdav::DavDownloadBody::Multipart(plan) => plan.segments().len(),
    };
    crate::webdav::observation::set_ranges(requested_ranges, served_ranges);
    let opened = match open_download(dav_fs, &path, plan.body).await {
        Ok(opened) => opened,
        Err(DavDownloadOpenError::Backend(error)) => {
            return aster_forge_webdav::actix::into_response(
                aster_forge_webdav::backend_error_response(&error),
            );
        }
        Err(DavDownloadOpenError::LengthMismatch { planned, opened }) => {
            tracing::warn!(path = %relative, planned, opened, "WebDAV download length contract drift");
            return responses::empty(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    if let Some(opened) = opened {
        response.body = DavResponseBody::Stream(opened.stream);
    }
    aster_forge_webdav::actix::into_response(response)
}

#[expect(
    clippy::too_many_arguments,
    reason = "The WebDAV PUT adapter receives the parsed protocol context and product-owned write dependencies explicitly."
)]
pub(crate) async fn handle_put(
    req: &HttpRequest,
    request_head: &DavRequestHead,
    dav_fs: &backend::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    system_file_policy: &system_file::SystemFileBlockPolicy,
    payload: &mut web::Payload,
    capabilities: &DavCapabilitySnapshot,
) -> HttpResponse {
    let path = request_head.target.clone();
    let relative = path.as_str().to_owned();
    if let Err(resp) = ensure_system_file_name_allowed(system_file_policy, &relative) {
        return resp;
    }
    let target_meta = match dav_fs.metadata_for_write(&path).await {
        Ok(meta) => Some(meta),
        Err(FsError::NotFound) => None,
        Err(err) => return fs_error_response(err),
    };
    let headers = match aster_forge_webdav::actix::converted_headers(req.headers()) {
        Ok(headers) => headers,
        Err(response) => return response,
    };
    let target_is_collection = target_meta.as_ref().is_some_and(|meta| meta.is_dir());
    let target_etag = target_meta.as_ref().and_then(|meta| meta.etag());
    let target_last_modified = match target_meta.as_ref() {
        Some(meta) => match meta.modified() {
            Ok(modified) => Some(modified),
            Err(error) => return fs_error_response(error),
        },
        None => None,
    };
    let resource_state = if target_is_collection {
        DavPutResourceState::Collection
    } else if target_meta.is_some() {
        DavPutResourceState::File {
            etag: target_etag.as_deref(),
            last_modified: target_last_modified,
        }
    } else {
        DavPutResourceState::Missing
    };
    let plan = match plan_put_request(capabilities, &headers, resource_state) {
        Ok(plan) => plan,
        Err(error) => {
            return aster_forge_webdav::actix::into_response(put_plan_error_response(&error));
        }
    };

    let request_scheme = request_head.origin.scheme.as_str();
    let request_host = request_head.origin.host.as_str();
    if let Err(resp) = aster_forge_webdav::actix::enforce_if_header_with_backends(
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
    let mut credentials = match aster_forge_webdav::actix::enforce_unlocked(
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
        Ok(credentials) => credentials,
        Err(resp) => return resp,
    };
    if !plan.resource_existed {
        let parent_credentials = match aster_forge_webdav::actix::enforce_parent_unlocked(
            lock_system,
            &path,
            prefix,
            request_head.if_header.as_ref(),
            request_scheme,
            request_host,
        )
        .await
        {
            Ok(credentials) => credentials,
            Err(resp) => return resp,
        };
        credentials.merge(parent_credentials);
    }

    if !matches!(&plan.write, DavPutWritePlan::Replace) {
        return responses::bad_request_text("Partial WebDAV writes are disabled");
    }
    let mut writer = match dav_fs
        .open_write(
            &path,
            DavWriteOptions {
                truncate: true,
                create: plan.create,
                create_new: plan.create_new,
                expected_length: plan.content_length_hint,
                checksum: None,
                credentials,
            },
        )
        .await
    {
        Ok(writer) => writer,
        Err(error) => {
            tracing::warn!(path = %relative, error = %error, "WebDAV PUT open failed");
            return aster_forge_webdav::actix::into_response(
                aster_forge_webdav::backend_error_response(&error),
            );
        }
    };

    while let Some(chunk) = payload.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(_) => {
                if let Err(error) = writer.abort().await {
                    tracing::warn!(path = %relative, error = %error, "WebDAV PUT abort after body read failure failed");
                }
                return responses::request_body_read_error();
            }
        };
        crate::webdav::observation::add_bytes_received(chunk.len());
        if let Err(error) = writer.write_bytes(chunk).await {
            tracing::warn!(path = %relative, error = %error, "WebDAV PUT write failed");
            if let Err(abort_error) = writer.abort().await {
                tracing::warn!(path = %relative, error = %abort_error, "WebDAV PUT abort after write failure failed");
            }
            return aster_forge_webdav::actix::into_response(
                aster_forge_webdav::backend_error_response(&error),
            );
        }
    }

    if let Err(error) = writer.finish().await {
        tracing::warn!(path = %relative, error = %error, "WebDAV PUT finish failed");
        return aster_forge_webdav::actix::into_response(
            aster_forge_webdav::backend_error_response(&error),
        );
    }

    match put_success_response(&plan, prefix, &path) {
        Ok(response) => aster_forge_webdav::actix::into_response(response),
        Err(_) => responses::empty(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

fn requested_range_count(req: &HttpRequest) -> usize {
    req.headers()
        .get(actix_web::http::header::RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split_once('=').map(|(_, ranges)| ranges))
        .map(|ranges| {
            ranges
                .split(',')
                .filter(|range| !range.trim().is_empty())
                .count()
        })
        .unwrap_or(0)
}

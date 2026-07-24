//! Drive-to-Forge HTTP precondition and lock-state bridge.

use actix_web::HttpResponse;
use actix_web::http::header;
use aster_forge_webdav::protocol::IfHeader;
use aster_forge_webdav::{
    DavBackendError, DavIfEvaluationError, DavIfResourceState, DavIfStateResolver, DavProtocolError,
};
use async_trait::async_trait;

use aster_forge_webdav::{DavFileSystem, DavLockSystem, DavPath, FsError};

pub(crate) use aster_forge_webdav::protocol::DavPrecondition as HttpEtagPrecondition;
pub(crate) use aster_forge_webdav::protocol::Depth;

pub(crate) fn converted_headers(
    headers: &header::HeaderMap,
) -> Result<http::HeaderMap, HttpResponse> {
    aster_forge_webdav::actix::convert_header_map(headers).map_err(protocol_error_response)
}

pub(crate) fn protocol_error_response(error: DavProtocolError) -> HttpResponse {
    aster_forge_webdav::actix::into_response(aster_forge_webdav::protocol_error_response(&error))
}

pub(crate) fn submitted_lock_tokens_for_path(
    if_header: Option<&IfHeader>,
    request_path: &str,
    request_scheme: &str,
    request_host: &str,
) -> Vec<String> {
    let Some(if_header) = if_header else {
        return Vec::new();
    };
    aster_forge_webdav::submitted_lock_tokens(if_header, request_path, request_scheme, request_host)
}

pub(crate) async fn ensure_if_header(
    if_header: Option<&IfHeader>,
    dav_fs: &dyn DavFileSystem,
    lock_system: &dyn DavLockSystem,
    request_path: &DavPath,
    prefix: &str,
    request_scheme: &str,
    request_host: &str,
) -> Result<(), HttpResponse> {
    let resolver = DriveIfStateResolver {
        dav_fs,
        lock_system,
    };
    match aster_forge_webdav::enforce_if_header(
        if_header,
        &resolver,
        request_path,
        prefix,
        request_scheme,
        request_host,
    )
    .await
    {
        Ok(()) => Ok(()),
        Err(DavIfEvaluationError::Protocol(error)) => Err(protocol_error_response(error)),
        Err(DavIfEvaluationError::Backend(error)) => Err(backend_error_response(error)),
    }
}

struct DriveIfStateResolver<'a> {
    dav_fs: &'a dyn DavFileSystem,
    lock_system: &'a dyn DavLockSystem,
}

#[async_trait]
impl DavIfStateResolver for DriveIfStateResolver<'_> {
    async fn resolve_if_state(
        &self,
        path: &DavPath,
    ) -> Result<DavIfResourceState, DavBackendError> {
        let etag = match self.dav_fs.metadata(path).await {
            Ok(metadata) => metadata.etag(),
            Err(FsError::NotFound) => None,
            Err(error) => return Err(error.into()),
        };
        let lock_tokens = self
            .lock_system
            .discover(path)
            .await
            .into_iter()
            .map(|lock| lock.token)
            .collect();
        Ok(DavIfResourceState { etag, lock_tokens })
    }
}

fn backend_error_response(error: DavBackendError) -> HttpResponse {
    aster_forge_webdav::actix::into_response(aster_forge_webdav::backend_error_response(&error))
}

pub(crate) fn evaluate_http_etag_preconditions(
    headers: &header::HeaderMap,
    resource_exists: bool,
    current_etag: Option<&str>,
    safe_method: bool,
) -> Result<HttpEtagPrecondition, HttpResponse> {
    let headers = converted_headers(headers)?;
    aster_forge_webdav::evaluate_http_etag_preconditions(
        &headers,
        resource_exists,
        current_etag,
        safe_method,
    )
    .map_err(protocol_error_response)
}

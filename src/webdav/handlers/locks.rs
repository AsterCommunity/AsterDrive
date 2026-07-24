//! WebDAV LOCK / UNLOCK handlers and lock XML helpers.

use std::time::Duration;

use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse};
use aster_forge_webdav::{
    DavLockInfo, DavLockPlan, DavLockPlanError, DavLockXml, DavRequestHead, DavXmlElement,
    DavXmlError, dav_lock_discovery_element, dav_supported_lock_element,
    lock_acquire_success_response, lock_conflict_response, lock_limit_response,
    lock_refresh_success_response, lock_xml_error_response, plan_lock_request,
    unlock_success_response, unlock_token_mismatch_response,
};

use crate::webdav::protocol::{self, Depth};
use crate::webdav::{backend, fs_error_response, href_for_dav_path, responses};
use aster_forge_webdav::{
    DavFileSystem, DavLock, DavLockError, DavLockPreflightError, DavLockSystem, FsError,
    OpenOptions,
};

const MAX_LOCK_DURATION_SECS: u64 = 604_800;

pub(crate) async fn handle_lock(
    req: &HttpRequest,
    request_head: &DavRequestHead,
    dav_fs: &backend::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    body: &[u8],
) -> HttpResponse {
    let path = request_head.target.clone();
    let headers = match protocol::converted_headers(req.headers()) {
        Ok(headers) => headers,
        Err(resp) => return resp,
    };

    let Some(depth) = request_head.depth else {
        return responses::empty(StatusCode::INTERNAL_SERVER_ERROR);
    };
    let request_scheme = request_head.origin.scheme.as_str();
    let request_host = request_head.origin.host.as_str();
    let plan = match plan_lock_request(
        &headers,
        body,
        request_head,
        prefix,
        Duration::from_secs(MAX_LOCK_DURATION_SECS),
    ) {
        Ok(plan) => plan,
        Err(DavLockPlanError::Protocol(error)) => {
            return protocol::protocol_error_response(error);
        }
        Err(DavLockPlanError::Xml(error)) => {
            return into_xml_response(lock_xml_error_response(error));
        }
    };

    match plan {
        DavLockPlan::Refresh { token, timeout } => {
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
            if lock_system
                .check(&path, None, false, false, std::slice::from_ref(&token))
                .await
                .is_err()
            {
                return responses::precondition_failed();
            }
            let lock = match lock_system.refresh(&path, &token, Some(timeout)).await {
                Ok(lock) => lock,
                Err(_) => return responses::precondition_failed(),
            };
            into_xml_response(lock_refresh_success_response(&lock_info(lock), prefix))
        }
        DavLockPlan::Acquire {
            owner,
            timeout,
            shared,
            deep,
        } => {
            if let Err(error) = lock_system.prepare_lock(&path).await {
                return match error {
                    DavLockPreflightError::LimitExceeded => {
                        aster_forge_webdav::actix::into_response(lock_limit_response())
                    }
                    DavLockPreflightError::GeneralFailure => {
                        responses::empty(StatusCode::INTERNAL_SERVER_ERROR)
                    }
                };
            }

            let resource_existed = match ensure_lock_target_exists(dav_fs, &path, depth).await {
                Ok(resource_existed) => resource_existed,
                Err(err) => return fs_error_response(err),
            };

            let lock = match lock_system
                .lock(&path, None, owner.as_ref(), Some(timeout), shared, deep)
                .await
            {
                Ok(lock) => lock,
                Err(DavLockError::Conflict(lock)) => {
                    return into_xml_response(lock_conflict_response(prefix, &lock.path));
                }
                Err(DavLockError::LimitExceeded) => {
                    return aster_forge_webdav::actix::into_response(lock_limit_response());
                }
                Err(DavLockError::Backend) => {
                    return responses::empty(StatusCode::INTERNAL_SERVER_ERROR);
                }
            };

            into_xml_response(lock_acquire_success_response(
                &lock_info(lock),
                prefix,
                resource_existed,
            ))
        }
    }
}

pub(crate) async fn handle_unlock(
    req: &HttpRequest,
    request_head: &DavRequestHead,
    lock_system: &dyn DavLockSystem,
) -> HttpResponse {
    let path = request_head.target.clone();
    let headers = match protocol::converted_headers(req.headers()) {
        Ok(headers) => headers,
        Err(resp) => return resp,
    };
    let token = match aster_forge_webdav::parse_lock_token_header(&headers) {
        Ok(token) => token,
        Err(error) => return protocol::protocol_error_response(error),
    };

    match lock_system.unlock(&path, &token).await {
        Ok(()) => aster_forge_webdav::actix::into_response(unlock_success_response()),
        Err(()) => into_xml_response(unlock_token_mismatch_response()),
    }
}

fn into_xml_response(
    response: Result<aster_forge_webdav::DavResponse, DavXmlError>,
) -> HttpResponse {
    match response {
        Ok(response) => aster_forge_webdav::actix::into_response(response),
        Err(_) => responses::empty(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn ensure_lock_target_exists(
    dav_fs: &backend::AsterDavFs,
    path: &aster_forge_webdav::DavPath,
    depth: Depth,
) -> Result<bool, FsError> {
    let _ = depth;
    match dav_fs.metadata(path).await {
        Ok(_) => Ok(true),
        Err(FsError::NotFound) if !path.is_collection() => {
            let mut file = dav_fs
                .open(
                    path,
                    OpenOptions {
                        write: true,
                        create: true,
                        truncate: true,
                        size: Some(0),
                        ..OpenOptions::default()
                    },
                )
                .await?;
            file.flush().await?;
            Ok(false)
        }
        Err(FsError::NotFound) => Err(FsError::NotFound),
        Err(err) => Err(err),
    }
}

pub(crate) fn supportedlock_element() -> DavXmlElement {
    dav_supported_lock_element()
}

pub(crate) fn lockdiscovery_element(locks: &[DavLock], prefix: &str) -> DavXmlElement {
    let locks = locks
        .iter()
        .map(|lock| lock_xml(lock, prefix))
        .collect::<Vec<_>>();
    dav_lock_discovery_element(&locks)
}

fn lock_xml(lock: &DavLock, prefix: &str) -> DavLockXml {
    DavLockXml {
        token: lock.token.clone(),
        owner: lock.owner.as_deref().cloned(),
        timeout: lock.timeout,
        shared: lock.shared,
        deep: lock.deep,
        root_href: href_for_dav_path(prefix, &lock.path),
    }
}

fn lock_info(lock: DavLock) -> DavLockInfo {
    DavLockInfo {
        token: lock.token,
        path: *lock.path,
        owner_xml: lock.owner.map(|owner| *owner),
        timeout_at: lock.timeout_at,
        timeout: lock.timeout,
        shared: lock.shared,
        deep: lock.deep,
    }
}

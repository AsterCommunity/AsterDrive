//! WebDAV LOCK / UNLOCK handlers and lock XML helpers.

use std::time::Duration;

use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse};
use aster_forge_webdav::{
    DavLockPlan, DavLockPlanError, DavRequestHead, DavXmlError, lock_acquire_success_response,
    lock_conflict_response, lock_limit_response, lock_refresh_success_response,
    lock_xml_error_response, plan_lock_request, unlock_success_response,
    unlock_token_mismatch_response,
};

use crate::webdav::{backend, responses};
use aster_forge_webdav::{DavLockError, DavLockPreflightError, DavLockSystem};

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
    let headers = match aster_forge_webdav::actix::converted_headers(req.headers()) {
        Ok(headers) => headers,
        Err(resp) => return resp,
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
            return aster_forge_webdav::actix::protocol_error_response(error);
        }
        Err(DavLockPlanError::Xml(error)) => {
            return into_xml_response(lock_xml_error_response(error));
        }
    };

    match plan {
        DavLockPlan::Refresh { token, timeout } => {
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
            match lock_system
                .check(&path, None, false, false, std::slice::from_ref(&token))
                .await
            {
                Ok(()) => {}
                Err(DavLockError::Conflict(_) | DavLockError::TokenMismatch) => {
                    return responses::precondition_failed();
                }
                Err(DavLockError::LimitExceeded) => {
                    return aster_forge_webdav::actix::into_response(lock_limit_response());
                }
                Err(DavLockError::NotFound) => {
                    return responses::empty(StatusCode::NOT_FOUND);
                }
                Err(DavLockError::Backend) => {
                    return responses::empty(StatusCode::INTERNAL_SERVER_ERROR);
                }
            }
            let lock = match lock_system.refresh(&path, &token, Some(timeout)).await {
                Ok(lock) => lock,
                Err(DavLockError::TokenMismatch | DavLockError::Conflict(_)) => {
                    return responses::precondition_failed();
                }
                Err(DavLockError::LimitExceeded) => {
                    return aster_forge_webdav::actix::into_response(lock_limit_response());
                }
                Err(DavLockError::NotFound) => {
                    return responses::empty(StatusCode::NOT_FOUND);
                }
                Err(DavLockError::Backend) => {
                    return responses::empty(StatusCode::INTERNAL_SERVER_ERROR);
                }
            };
            into_xml_response(lock_refresh_success_response(&lock, prefix))
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

            let resource_existed =
                match aster_forge_webdav::ensure_lock_target_exists(dav_fs, dav_fs, &path).await {
                    Ok(resource_existed) => resource_existed,
                    Err(error) => {
                        return aster_forge_webdav::actix::into_response(
                            aster_forge_webdav::backend_error_response(&error),
                        );
                    }
                };

            let lock = match lock_system
                .lock(&path, None, owner.as_ref(), Some(timeout), shared, deep)
                .await
            {
                Ok(lock) => lock,
                Err(DavLockError::Conflict(lock)) => {
                    return into_xml_response(lock_conflict_response(prefix, &lock.path));
                }
                Err(DavLockError::TokenMismatch) => {
                    return responses::precondition_failed();
                }
                Err(DavLockError::LimitExceeded) => {
                    return aster_forge_webdav::actix::into_response(lock_limit_response());
                }
                Err(DavLockError::NotFound) => {
                    return responses::empty(StatusCode::NOT_FOUND);
                }
                Err(DavLockError::Backend) => {
                    return responses::empty(StatusCode::INTERNAL_SERVER_ERROR);
                }
            };

            into_xml_response(lock_acquire_success_response(
                &lock,
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
    let headers = match aster_forge_webdav::actix::converted_headers(req.headers()) {
        Ok(headers) => headers,
        Err(resp) => return resp,
    };
    let token = match aster_forge_webdav::parse_lock_token_header(&headers) {
        Ok(token) => token,
        Err(error) => return aster_forge_webdav::actix::protocol_error_response(error),
    };

    match lock_system.unlock(&path, &token).await {
        Ok(()) => aster_forge_webdav::actix::into_response(unlock_success_response()),
        Err(DavLockError::TokenMismatch | DavLockError::Conflict(_)) => {
            into_xml_response(unlock_token_mismatch_response())
        }
        Err(DavLockError::LimitExceeded) => {
            aster_forge_webdav::actix::into_response(lock_limit_response())
        }
        Err(DavLockError::NotFound) => responses::empty(StatusCode::NOT_FOUND),
        Err(DavLockError::Backend) => responses::empty(StatusCode::INTERNAL_SERVER_ERROR),
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

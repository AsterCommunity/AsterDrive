//! WOPI HTTP 路由映射。
//!
//! 本地路径固定对应官方 WOPI file endpoints：
//! - `GET  /api/v1/wopi/files/{id}` -> CheckFileInfo
//! - `POST /api/v1/wopi/files/{id}` -> Lock / Unlock / RefreshLock / RenameFile / PutRelativeFile / PutUserInfo
//! - `GET  /api/v1/wopi/files/{id}/contents` -> GetFile
//! - `POST /api/v1/wopi/files/{id}/contents` -> PutFile
//!
//! 维护这组路由时务必同时对照：
//! - https://learn.microsoft.com/en-us/microsoft-365/cloud-storage-partner-program/rest/files/checkfileinfo
//! - https://learn.microsoft.com/en-us/microsoft-365/cloud-storage-partner-program/rest/files/getfile
//! - https://learn.microsoft.com/en-us/microsoft-365/cloud-storage-partner-program/rest/files/putfile
//! - https://learn.microsoft.com/en-us/microsoft-365/cloud-storage-partner-program/rest/files/renamefile
//! - https://learn.microsoft.com/en-us/microsoft-365/cloud-storage-partner-program/rest/files/putrelativefile
//! - https://learn.microsoft.com/en-us/microsoft-365/cloud-storage-partner-program/rest/files/putuserinfo
//!
//! 这些路径还和 `session::build_public_wopi_src()`、PUT_RELATIVE 返回 URL 直接耦合，
//! 不能只改路由而不改 launch / response 生成逻辑。

use crate::api::dto::validate_request;
use crate::api::dto::wopi::WopiAccessQuery;
use crate::config::site_url;
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::{audit_service, files::file, preview::wopi};
use actix_web::http::header;
use actix_web::{HttpRequest, HttpResponse, web};

pub fn routes() -> impl actix_web::dev::HttpServiceFactory + use<> {
    web::scope("/wopi")
        .route("/files/{id}", web::get().to(check_file_info))
        .route("/files/{id}", web::post().to(file_operation))
        .route("/files/{id}/contents", web::get().to(get_file_contents))
        .route("/files/{id}/contents", web::post().to(put_file_contents))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/wopi/files/{id}",
    tag = "public",
    operation_id = "wopi_check_file_info",
    params(
        ("id" = i64, Path, description = "File ID"),
        ("access_token" = Option<String>, Query, description = "WOPI access token"),
        ("X-WOPI-Token" = Option<String>, Header, description = "WOPI access token header")
    ),
    responses((status = 200, description = "WOPI CheckFileInfo response")),
)]
pub async fn check_file_info(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    path: web::Path<i64>,
    query: web::Query<WopiAccessQuery>,
) -> HttpResponse {
    if let Err(error) = validate_request(&*query) {
        return wopi_no_store(protocol_error_response(error));
    }
    let access_token = match wopi_access_token(&req, &query) {
        Ok(access_token) => access_token,
        Err(response) => return response,
    };
    match wopi::check_file_info(
        state.get_ref(),
        *path,
        access_token,
        request_source(state.get_ref(), &req),
    )
    .await
    {
        Ok(info) => wopi_no_store(HttpResponse::Ok().json(info)),
        Err(error) => wopi_no_store(protocol_error_response(error)),
    }
}

pub async fn get_file_contents(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    path: web::Path<i64>,
    query: web::Query<WopiAccessQuery>,
) -> HttpResponse {
    if let Err(error) = validate_request(&*query) {
        return wopi_no_store(protocol_error_response(error));
    }
    let access_token = match wopi_access_token(&req, &query) {
        Ok(access_token) => access_token,
        Err(response) => return response,
    };
    let audit_info = audit_service::AuditRequestInfo::from_request(&req);
    let if_none_match = req
        .headers()
        .get("If-None-Match")
        .and_then(|value| value.to_str().ok());
    let max_expected_size = optional_header_value(&req, "X-WOPI-MaxExpectedSize");
    match wopi::get_file_contents(
        state.get_ref(),
        *path,
        access_token,
        if_none_match,
        max_expected_size,
        &audit_info,
        request_source(state.get_ref(), &req),
    )
    .await
    {
        Ok(result) => {
            let mut response = file::outcome_to_response(result.outcome);
            // X-WOPI-ItemVersion は 304 / 302 でも添付する（WOPI 仕様要求）
            if let Ok(version_value) =
                actix_web::http::header::HeaderValue::from_str(&result.item_version)
            {
                response.headers_mut().insert(
                    actix_web::http::header::HeaderName::from_static("x-wopi-itemversion"),
                    version_value,
                );
            }
            wopi_no_store(response)
        }
        Err(error) => wopi_no_store(protocol_error_response(error)),
    }
}

pub async fn put_file_contents(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    path: web::Path<i64>,
    query: web::Query<WopiAccessQuery>,
    mut payload: web::Payload,
) -> HttpResponse {
    if let Err(error) = validate_request(&*query) {
        return wopi_no_store(protocol_error_response(error));
    }
    let access_token = match wopi_access_token(&req, &query) {
        Ok(access_token) => access_token,
        Err(response) => return response,
    };
    let audit_info = audit_service::AuditRequestInfo::from_request(&req);
    let override_value = header_value(&req, "X-WOPI-Override");
    if !override_value.eq_ignore_ascii_case("PUT") {
        return wopi_no_store(HttpResponse::NotImplemented().finish());
    }

    match wopi::put_file_contents(
        state.get_ref(),
        wopi::WopiPutFileRequest {
            file_id: *path,
            access_token,
            payload: &mut payload,
            content_length: request_content_length(&req),
            requested_lock: optional_header_value(&req, "X-WOPI-Lock"),
            audit_info: &audit_info,
            request_source: request_source(state.get_ref(), &req),
        },
    )
    .await
    {
        Ok(wopi::WopiPutFileResult::Success { item_version }) => wopi_no_store(
            HttpResponse::Ok()
                .insert_header(("X-WOPI-ItemVersion", item_version))
                .finish(),
        ),
        Ok(wopi::WopiPutFileResult::Conflict(conflict)) => {
            wopi_no_store(conflict_response(&conflict))
        }
        Err(error) => wopi_no_store(protocol_error_response(error)),
    }
}

pub async fn file_operation(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    path: web::Path<i64>,
    query: web::Query<WopiAccessQuery>,
    mut payload: web::Payload,
) -> HttpResponse {
    if let Err(error) = validate_request(&*query) {
        return wopi_no_store(protocol_error_response(error));
    }
    let access_token = match wopi_access_token(&req, &query) {
        Ok(access_token) => access_token,
        Err(response) => return response,
    };
    let audit_info = audit_service::AuditRequestInfo::from_request(&req);
    let override_value = header_value(&req, "X-WOPI-Override");
    let requested_lock = optional_header_value(&req, "X-WOPI-Lock").unwrap_or_default();
    let old_lock = optional_header_value(&req, "X-WOPI-OldLock").unwrap_or_default();

    if override_value.eq_ignore_ascii_case("PUT_RELATIVE") {
        return match wopi::put_relative_file(
            state.get_ref(),
            wopi::WopiPutRelativeRequest {
                file_id: *path,
                access_token,
                payload: &mut payload,
                suggested_target: optional_header_value(&req, "X-WOPI-SuggestedTarget"),
                relative_target: optional_header_value(&req, "X-WOPI-RelativeTarget"),
                overwrite_relative_target: optional_header_value(
                    &req,
                    "X-WOPI-OverwriteRelativeTarget",
                ),
                size_header: optional_header_value(&req, "X-WOPI-Size"),
                content_length: request_content_length(&req),
                audit_info: &audit_info,
                request_source: request_source(state.get_ref(), &req),
            },
        )
        .await
        {
            Ok(wopi::WopiPutRelativeResult::Success(response)) => {
                wopi_no_store(HttpResponse::Ok().json(response))
            }
            Ok(wopi::WopiPutRelativeResult::Conflict(conflict)) => {
                wopi_no_store(put_relative_conflict_response(&conflict))
            }
            Err(error) => wopi_no_store(protocol_error_response(error)),
        };
    }

    if override_value.eq_ignore_ascii_case("GET_LOCK") {
        return match wopi::get_lock(
            state.get_ref(),
            *path,
            access_token,
            request_source(state.get_ref(), &req),
        )
        .await
        {
            Ok(wopi::WopiGetLockResult::Success { current_lock }) => wopi_no_store(
                HttpResponse::Ok()
                    .insert_header(("X-WOPI-Lock", current_lock))
                    .finish(),
            ),
            Ok(wopi::WopiGetLockResult::Conflict(conflict)) => {
                wopi_no_store(conflict_response(&conflict))
            }
            Err(error) => wopi_no_store(protocol_error_response(error)),
        };
    }

    if override_value.eq_ignore_ascii_case("RENAME_FILE") {
        return match wopi::rename_file(
            state.get_ref(),
            *path,
            access_token,
            optional_header_value(&req, "X-WOPI-RequestedName"),
            optional_header_value(&req, "X-WOPI-Lock"),
            &audit_info,
            request_source(state.get_ref(), &req),
        )
        .await
        {
            Ok(wopi::WopiRenameFileResult::Success(response)) => {
                wopi_no_store(HttpResponse::Ok().json(response))
            }
            Ok(wopi::WopiRenameFileResult::Conflict(conflict)) => {
                wopi_no_store(conflict_response(&conflict))
            }
            Ok(wopi::WopiRenameFileResult::InvalidName { reason }) => {
                wopi_no_store(invalid_file_name_response(&reason))
            }
            Err(error) => wopi_no_store(protocol_error_response(error)),
        };
    }

    if override_value.eq_ignore_ascii_case("PUT_USER_INFO") {
        return match wopi::put_user_info(
            state.get_ref(),
            *path,
            access_token,
            &mut payload,
            &audit_info,
            request_source(state.get_ref(), &req),
        )
        .await
        {
            Ok(()) => wopi_no_store(HttpResponse::Ok().finish()),
            Err(error) => wopi_no_store(protocol_error_response(error)),
        };
    }

    let result = if override_value.eq_ignore_ascii_case("LOCK") && !old_lock.is_empty() {
        wopi::unlock_and_relock_file(
            state.get_ref(),
            *path,
            access_token,
            requested_lock,
            old_lock,
            &audit_info,
            request_source(state.get_ref(), &req),
        )
        .await
    } else if override_value.eq_ignore_ascii_case("LOCK") {
        wopi::lock_file(
            state.get_ref(),
            *path,
            access_token,
            requested_lock,
            &audit_info,
            request_source(state.get_ref(), &req),
        )
        .await
    } else if override_value.eq_ignore_ascii_case("UNLOCK") {
        wopi::unlock_file(
            state.get_ref(),
            *path,
            access_token,
            requested_lock,
            &audit_info,
            request_source(state.get_ref(), &req),
        )
        .await
    } else if override_value.eq_ignore_ascii_case("REFRESH_LOCK") {
        wopi::refresh_lock(
            state.get_ref(),
            *path,
            access_token,
            requested_lock,
            &audit_info,
            request_source(state.get_ref(), &req),
        )
        .await
    } else {
        return wopi_no_store(HttpResponse::NotImplemented().finish());
    };

    match result {
        Ok(wopi::WopiLockOperationResult::Success) => {
            wopi_no_store(HttpResponse::Ok().finish())
        }
        Ok(wopi::WopiLockOperationResult::Conflict(conflict)) => {
            wopi_no_store(conflict_response(&conflict))
        }
        Err(error) => wopi_no_store(protocol_error_response(error)),
    }
}

fn wopi_access_token<'a>(
    req: &'a HttpRequest,
    query: &'a WopiAccessQuery,
) -> Result<&'a str, HttpResponse> {
    if let Some(token) = optional_header_value(req, "X-WOPI-Token") {
        return Ok(token);
    }
    if let Some(token) = query.access_token.as_deref() {
        let token = token.trim();
        if token.is_empty() {
            return Err(wopi_no_store(protocol_error_response(
                crate::errors::AsterError::validation_error("value cannot be empty"),
            )));
        }
        return Ok(token);
    }
    Err(wopi_no_store(protocol_error_response(
        crate::errors::AsterError::validation_error("access_token is required"),
    )))
}

fn conflict_response(conflict: &wopi::WopiConflict) -> HttpResponse {
    let mut response = HttpResponse::Conflict();
    if let Some(current_lock) = &conflict.current_lock {
        response.insert_header(("X-WOPI-Lock", current_lock.as_str()));
    }
    response
        .insert_header(("X-WOPI-LockFailureReason", conflict.reason.as_str()))
        .finish()
}

fn put_relative_conflict_response(
    conflict: &wopi::WopiPutRelativeConflict,
) -> HttpResponse {
    let mut response = HttpResponse::Conflict();
    response.insert_header((
        "X-WOPI-Lock",
        conflict.current_lock.as_deref().unwrap_or_default(),
    ));
    if let Some(valid_target) = &conflict.valid_target {
        response.insert_header(("X-WOPI-ValidRelativeTarget", valid_target.as_str()));
    }
    response
        .insert_header(("X-WOPI-LockFailureReason", conflict.reason.as_str()))
        .finish()
}

fn invalid_file_name_response(reason: &str) -> HttpResponse {
    HttpResponse::BadRequest()
        .insert_header(("X-WOPI-InvalidFileNameError", reason))
        .finish()
}

fn protocol_error_response(error: crate::errors::AsterError) -> HttpResponse {
    actix_web::ResponseError::error_response(&error)
}

fn wopi_no_store(mut response: HttpResponse) -> HttpResponse {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    response
}

fn header_value(req: &HttpRequest, name: &str) -> String {
    optional_header_value(req, name)
        .unwrap_or_default()
        .to_string()
}

fn optional_header_value<'a>(req: &'a HttpRequest, name: &str) -> Option<&'a str> {
    req.headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn request_source<'a>(
    state: &PrimaryAppState,
    req: &'a HttpRequest,
) -> wopi::WopiRequestSource<'a> {
    let conn = req.connection_info();
    wopi::WopiRequestSource {
        origin: optional_header_value(req, "Origin"),
        referer: optional_header_value(req, "Referer"),
        proof: optional_header_value(req, "X-WOPI-Proof"),
        proof_old: optional_header_value(req, "X-WOPI-ProofOld"),
        timestamp: optional_header_value(req, "X-WOPI-TimeStamp"),
        public_url: site_url::public_app_url_for_request(
            state.runtime_config(),
            &req.uri().to_string(),
            conn.scheme(),
            conn.host(),
        ),
        public_origin: site_url::public_site_url_for_request(
            state.runtime_config(),
            conn.scheme(),
            conn.host(),
        ),
    }
}

fn request_content_length(req: &HttpRequest) -> Option<i64> {
    req.headers()
        .get(actix_web::http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .and_then(|value| crate::utils::numbers::u64_to_i64(value, "content length").ok())
}

#[cfg(test)]
mod tests {
    use super::{wopi_access_token, wopi_no_store};
    use crate::api::dto::wopi::WopiAccessQuery;
    use actix_web::HttpResponse;
    use actix_web::http::header;

    #[test]
    fn wopi_access_token_prefers_header_over_query() {
        let req = actix_web::test::TestRequest::default()
            .insert_header(("X-WOPI-Token", "header-token"))
            .uri("/api/v1/wopi/files/1?access_token=query-token")
            .to_http_request();
        let query = WopiAccessQuery {
            access_token: Some("query-token".to_string()),
        };

        let token = wopi_access_token(&req, &query).expect("header token should be accepted");

        assert_eq!(token, "header-token");
    }

    #[test]
    fn wopi_access_token_falls_back_to_query_for_wopi_clients() {
        let req = actix_web::test::TestRequest::default().to_http_request();
        let query = WopiAccessQuery {
            access_token: Some("query-token".to_string()),
        };

        let token = wopi_access_token(&req, &query).expect("query token should remain supported");

        assert_eq!(token, "query-token");
    }

    #[test]
    fn wopi_access_token_rejects_missing_token() {
        let req = actix_web::test::TestRequest::default().to_http_request();
        let query = WopiAccessQuery { access_token: None };

        let response = wopi_access_token(&req, &query).expect_err("missing token should fail");

        assert_eq!(response.status(), actix_web::http::StatusCode::BAD_REQUEST);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL),
            Some(&header::HeaderValue::from_static("no-store"))
        );
    }

    #[test]
    fn wopi_no_store_overwrites_existing_cache_control() {
        let response = HttpResponse::Ok()
            .insert_header((header::CACHE_CONTROL, "public, max-age=60"))
            .finish();

        let response = wopi_no_store(response);

        assert_eq!(
            response.headers().get(header::CACHE_CONTROL),
            Some(&header::HeaderValue::from_static("no-store"))
        );
    }
}

//! AsterDrive authentication and product-policy responses for WebDAV endpoints.

use actix_web::http::{StatusCode, header};
use actix_web::{HttpResponse, HttpResponseBuilder};

use aster_forge_webdav::FsError;

pub(crate) const TEXT_CONTENT_TYPE: &str = "text/plain; charset=utf-8";
const NO_STORE: &str = "no-store";

pub(crate) fn build(status: StatusCode) -> HttpResponseBuilder {
    let mut response = HttpResponse::build(status);
    if status.is_client_error() || status.is_server_error() {
        response.insert_header((header::CACHE_CONTROL, NO_STORE));
    }
    response
}

pub(crate) fn empty(status: StatusCode) -> HttpResponse {
    build(status).finish()
}

pub(crate) fn text(status: StatusCode, body: impl Into<String>) -> HttpResponse {
    build(status)
        .content_type(TEXT_CONTENT_TYPE)
        .body(body.into())
}

pub(crate) fn unauthorized() -> HttpResponse {
    build(StatusCode::UNAUTHORIZED)
        .insert_header(("WWW-Authenticate", "Basic realm=\"AsterDrive WebDAV\""))
        .content_type(TEXT_CONTENT_TYPE)
        .body("Unauthorized")
}

pub(crate) fn unauthorized_retry_after(retry_after: u64) -> HttpResponse {
    build(StatusCode::UNAUTHORIZED)
        .insert_header(("WWW-Authenticate", "Basic realm=\"AsterDrive WebDAV\""))
        .insert_header(("Retry-After", retry_after.to_string()))
        .content_type(TEXT_CONTENT_TYPE)
        .body("Unauthorized")
}

pub(crate) fn bad_request_text(body: &'static str) -> HttpResponse {
    text(StatusCode::BAD_REQUEST, body)
}

pub(crate) fn conflict() -> HttpResponse {
    empty(StatusCode::CONFLICT)
}

pub(crate) fn forbidden_text(body: &'static str) -> HttpResponse {
    text(StatusCode::FORBIDDEN, body)
}

pub(crate) fn precondition_failed() -> HttpResponse {
    empty(StatusCode::PRECONDITION_FAILED)
}

pub(crate) fn service_unavailable_text(body: &'static str) -> HttpResponse {
    text(StatusCode::SERVICE_UNAVAILABLE, body)
}

pub(crate) fn webdav_disabled() -> HttpResponse {
    service_unavailable_text("WebDAV is disabled")
}

pub(crate) fn request_body_read_error() -> HttpResponse {
    bad_request_text("Failed to read request body")
}

pub(crate) fn system_file_name_blocked() -> HttpResponse {
    forbidden_text("WebDAV system file name is blocked")
}

pub(crate) fn unsupported_root_proppatch() -> HttpResponse {
    forbidden_text("PROPPATCH on the WebDAV mount root is not supported")
}

pub(crate) fn fs_error_response(err: FsError) -> HttpResponse {
    aster_forge_webdav::actix::into_response(aster_forge_webdav::backend_error_response(
        &err.into(),
    ))
}

#[cfg(test)]
mod tests {
    use actix_web::body;
    use actix_web::http::{StatusCode, header};

    use super::{
        fs_error_response, request_body_read_error, system_file_name_blocked, unauthorized,
        unauthorized_retry_after, unsupported_root_proppatch,
    };
    use aster_forge_webdav::FsError;

    fn assert_no_store(response: &actix_web::HttpResponse) {
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL),
            Some(&header::HeaderValue::from_static("no-store"))
        );
    }

    async fn body_text(response: actix_web::HttpResponse) -> String {
        let bytes = body::to_bytes(response.into_body())
            .await
            .expect("response body should be readable");
        String::from_utf8(bytes.to_vec()).expect("response body should be utf-8")
    }

    #[test]
    fn fs_errors_map_to_webdav_statuses_and_are_not_cacheable() {
        let cases = [
            (FsError::NotFound, StatusCode::NOT_FOUND),
            (FsError::Forbidden, StatusCode::FORBIDDEN),
            (FsError::Exists, StatusCode::CONFLICT),
            (
                FsError::InsufficientStorage,
                StatusCode::INSUFFICIENT_STORAGE,
            ),
            (FsError::TooLarge, StatusCode::PAYLOAD_TOO_LARGE),
            (FsError::BadRequest, StatusCode::BAD_REQUEST),
            (FsError::GeneralFailure, StatusCode::INTERNAL_SERVER_ERROR),
        ];

        for (err, expected) in cases {
            let response = fs_error_response(err);

            assert_eq!(response.status(), expected);
            assert_no_store(&response);
        }
    }

    #[test]
    fn unauthorized_response_sets_basic_challenge_and_plain_text() {
        let response = unauthorized();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_no_store(&response);
        assert_eq!(
            response.headers().get("WWW-Authenticate"),
            Some(&header::HeaderValue::from_static(
                "Basic realm=\"AsterDrive WebDAV\""
            ))
        );
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE),
            Some(&header::HeaderValue::from_static(
                "text/plain; charset=utf-8"
            ))
        );
    }

    #[test]
    fn rate_limited_unauthorized_response_preserves_webdav_challenge() {
        let response = unauthorized_retry_after(7);

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_no_store(&response);
        assert_eq!(
            response.headers().get("WWW-Authenticate"),
            Some(&header::HeaderValue::from_static(
                "Basic realm=\"AsterDrive WebDAV\""
            ))
        );
        assert_eq!(
            response.headers().get("Retry-After"),
            Some(&header::HeaderValue::from_static("7"))
        );
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE),
            Some(&header::HeaderValue::from_static(
                "text/plain; charset=utf-8"
            ))
        );
    }

    #[actix_web::test]
    async fn simple_text_helpers_return_expected_bodies() {
        let cases = [
            (
                request_body_read_error(),
                StatusCode::BAD_REQUEST,
                "Failed to read request body",
            ),
            (
                system_file_name_blocked(),
                StatusCode::FORBIDDEN,
                "WebDAV system file name is blocked",
            ),
            (
                unsupported_root_proppatch(),
                StatusCode::FORBIDDEN,
                "PROPPATCH on the WebDAV mount root is not supported",
            ),
        ];

        for (response, expected_status, expected_body) in cases {
            assert_eq!(response.status(), expected_status);
            assert_no_store(&response);
            assert_eq!(body_text(response).await, expected_body);
        }
    }
}

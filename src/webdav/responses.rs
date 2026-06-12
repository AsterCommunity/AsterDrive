//! Centralized WebDAV response builders.

use actix_web::http::{StatusCode, header};
use actix_web::{HttpResponse, HttpResponseBuilder};
use xmltree::{Element, XMLNode};

use crate::webdav::dav::{DavPath, FsError};
use crate::webdav::{dav_element, href_for_dav_path, text_element};

pub(crate) const XML_CONTENT_TYPE: &str = "application/xml; charset=utf-8";
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

pub(crate) fn xml_response(root: Element, status: StatusCode) -> HttpResponse {
    match xml_bytes(&root) {
        Ok(body) => xml_body(status, body),
        Err(resp) => resp,
    }
}

pub(crate) fn xml_response_builder(
    root: Element,
    status: StatusCode,
) -> Result<(HttpResponseBuilder, Vec<u8>), HttpResponse> {
    xml_bytes(&root).map(|body| {
        let mut builder = build(status);
        builder.content_type(XML_CONTENT_TYPE);
        (builder, body)
    })
}

pub(crate) fn xml_body(status: StatusCode, body: Vec<u8>) -> HttpResponse {
    build(status).content_type(XML_CONTENT_TYPE).body(body)
}

pub(crate) fn xml_bytes(root: &Element) -> Result<Vec<u8>, HttpResponse> {
    let mut buffer = Vec::new();
    root.write(&mut buffer)
        .map_err(|_| empty(StatusCode::INTERNAL_SERVER_ERROR))?;
    Ok(buffer)
}

pub(crate) fn with_no_store(mut response: HttpResponse) -> HttpResponse {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static(NO_STORE),
    );
    response
}

pub(crate) fn no_store(status: StatusCode) -> HttpResponse {
    HttpResponse::build(status)
        .insert_header((header::CACHE_CONTROL, NO_STORE))
        .finish()
}

pub(crate) fn range_not_satisfiable(length: u64) -> HttpResponse {
    build(StatusCode::RANGE_NOT_SATISFIABLE)
        .insert_header((header::CONTENT_RANGE, format!("bytes */{length}")))
        .insert_header(("Accept-Ranges", "bytes"))
        .finish()
}

pub(crate) fn method_not_allowed(allow: &'static str) -> HttpResponse {
    build(StatusCode::METHOD_NOT_ALLOWED)
        .insert_header((header::ALLOW, allow))
        .finish()
}

pub(crate) fn unauthorized() -> HttpResponse {
    build(StatusCode::UNAUTHORIZED)
        .insert_header(("WWW-Authenticate", "Basic realm=\"AsterDrive WebDAV\""))
        .content_type(TEXT_CONTENT_TYPE)
        .body("Unauthorized")
}

pub(crate) fn bad_request() -> HttpResponse {
    empty(StatusCode::BAD_REQUEST)
}

pub(crate) fn bad_request_text(body: &'static str) -> HttpResponse {
    text(StatusCode::BAD_REQUEST, body)
}

pub(crate) fn conflict() -> HttpResponse {
    empty(StatusCode::CONFLICT)
}

pub(crate) fn forbidden() -> HttpResponse {
    empty(StatusCode::FORBIDDEN)
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

pub(crate) fn unsupported_media_type() -> HttpResponse {
    empty(StatusCode::UNSUPPORTED_MEDIA_TYPE)
}

pub(crate) fn payload_too_large_text(body: &'static str) -> HttpResponse {
    text(StatusCode::PAYLOAD_TOO_LARGE, body)
}

pub(crate) fn webdav_disabled() -> HttpResponse {
    service_unavailable_text("WebDAV is disabled")
}

pub(crate) fn request_body_read_error() -> HttpResponse {
    bad_request_text("Failed to read request body")
}

pub(crate) fn xml_body_too_large() -> HttpResponse {
    payload_too_large_text("WebDAV XML body too large")
}

pub(crate) fn invalid_xml_body() -> HttpResponse {
    bad_request_text("Invalid XML body")
}

pub(crate) fn no_external_entities() -> HttpResponse {
    let mut error = dav_element("error");
    error
        .children
        .push(XMLNode::Element(dav_element("no-external-entities")));
    xml_response(error, StatusCode::FORBIDDEN)
}

pub(crate) fn invalid_request_path() -> HttpResponse {
    bad_request_text("Invalid request path")
}

pub(crate) fn system_file_name_blocked() -> HttpResponse {
    forbidden_text("WebDAV system file name is blocked")
}

pub(crate) fn unsupported_root_proppatch() -> HttpResponse {
    forbidden_text("PROPPATCH on the WebDAV mount root is not supported")
}

pub(crate) fn webdav_lock_limit_exceeded() -> HttpResponse {
    text(
        StatusCode::INSUFFICIENT_STORAGE,
        "WebDAV active lock limit exceeded",
    )
}

pub(crate) fn fs_error_response(err: FsError) -> HttpResponse {
    empty(fs_error_status(&err))
}

pub(crate) fn lock_token_submitted_element(prefix: &str, path: &DavPath) -> Element {
    let mut submitted = dav_element("lock-token-submitted");
    submitted.children.push(XMLNode::Element(text_element(
        "D:href",
        &href_for_dav_path(prefix, path),
    )));
    submitted
}

pub(crate) fn lock_token_submitted_response(
    status: StatusCode,
    prefix: &str,
    path: &DavPath,
) -> HttpResponse {
    let mut error = dav_element("error");
    error
        .attributes
        .insert("xmlns:D".to_string(), "DAV:".to_string());
    error
        .children
        .push(XMLNode::Element(lock_token_submitted_element(prefix, path)));
    xml_response(error, status)
}

pub(crate) fn lock_token_matches_request_uri_response(status: StatusCode) -> HttpResponse {
    let mut error = dav_element("error");
    error
        .attributes
        .insert("xmlns:D".to_string(), "DAV:".to_string());
    error.children.push(XMLNode::Element(dav_element(
        "lock-token-matches-request-uri",
    )));
    xml_response(error, status)
}

pub(crate) fn propfind_finite_depth_response() -> HttpResponse {
    let mut error = dav_element("error");
    error
        .attributes
        .insert("xmlns:D".to_string(), "DAV:".to_string());
    error
        .children
        .push(XMLNode::Element(dav_element("propfind-finite-depth")));
    xml_response(error, StatusCode::FORBIDDEN)
}

fn fs_error_status(err: &FsError) -> StatusCode {
    match err {
        FsError::NotFound => StatusCode::NOT_FOUND,
        FsError::Forbidden => StatusCode::FORBIDDEN,
        FsError::Exists => StatusCode::CONFLICT,
        FsError::InsufficientStorage => StatusCode::INSUFFICIENT_STORAGE,
        FsError::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
        FsError::BadRequest => StatusCode::BAD_REQUEST,
        FsError::GeneralFailure => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[cfg(test)]
mod tests {
    use actix_web::body;
    use actix_web::http::{StatusCode, header};

    use super::{
        XML_CONTENT_TYPE, fs_error_response, invalid_xml_body,
        lock_token_matches_request_uri_response, lock_token_submitted_response, method_not_allowed,
        no_external_entities, no_store, propfind_finite_depth_response, range_not_satisfiable,
        request_body_read_error, system_file_name_blocked, text, unauthorized,
        unsupported_root_proppatch, with_no_store, xml_body_too_large, xml_response,
        xml_response_builder,
    };
    use crate::webdav::dav::{DavPath, FsError};
    use crate::webdav::dav_element;

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
    fn text_error_responses_are_plain_text_and_not_cacheable() {
        let response = invalid_xml_body();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_no_store(&response);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE),
            Some(&header::HeaderValue::from_static(
                "text/plain; charset=utf-8"
            ))
        );
    }

    #[test]
    fn xml_error_responses_are_xml_and_not_cacheable() {
        let response = lock_token_matches_request_uri_response(StatusCode::CONFLICT);

        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert_no_store(&response);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE),
            Some(&header::HeaderValue::from_static(XML_CONTENT_TYPE))
        );
    }

    #[actix_web::test]
    async fn no_external_entities_uses_webdav_condition_response() {
        let response = no_external_entities();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_no_store(&response);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE),
            Some(&header::HeaderValue::from_static(XML_CONTENT_TYPE))
        );

        let body = body_text(response).await;
        assert!(body.contains("no-external-entities"), "{body}");
    }

    #[test]
    fn successful_text_and_multistatus_xml_do_not_force_no_store() {
        let text_response = text(StatusCode::OK, "Already under version control");
        let xml_response = xml_response(dav_element("multistatus"), StatusCode::MULTI_STATUS);

        assert_eq!(text_response.status(), StatusCode::OK);
        assert!(text_response.headers().get(header::CACHE_CONTROL).is_none());
        assert_eq!(xml_response.status(), StatusCode::MULTI_STATUS);
        assert!(xml_response.headers().get(header::CACHE_CONTROL).is_none());
    }

    #[actix_web::test]
    async fn xml_response_builder_allows_headers_before_body() {
        let (mut builder, body) = match xml_response_builder(dav_element("prop"), StatusCode::OK) {
            Ok(parts) => parts,
            Err(response) => panic!(
                "xml response builder should serialize test XML, got {}",
                response.status()
            ),
        };
        builder.insert_header(("Lock-Token", "<urn:uuid:test>"));
        let response = builder.body(body);

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE),
            Some(&header::HeaderValue::from_static(XML_CONTENT_TYPE))
        );
        assert_eq!(
            response.headers().get("Lock-Token"),
            Some(&header::HeaderValue::from_static("<urn:uuid:test>"))
        );

        let body = body_text(response).await;
        assert!(body.contains("<D:prop"), "{body}");
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
    fn method_not_allowed_response_preserves_allow_header() {
        let response = method_not_allowed("OPTIONS, GET");

        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_no_store(&response);
        assert_eq!(
            response.headers().get(header::ALLOW),
            Some(&header::HeaderValue::from_static("OPTIONS, GET"))
        );
    }

    #[test]
    fn range_not_satisfiable_response_preserves_range_headers() {
        let response = range_not_satisfiable(0);

        assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_no_store(&response);
        assert_eq!(
            response.headers().get(header::CONTENT_RANGE),
            Some(&header::HeaderValue::from_static("bytes */0"))
        );
        assert_eq!(
            response.headers().get("Accept-Ranges"),
            Some(&header::HeaderValue::from_static("bytes"))
        );
    }

    #[test]
    fn with_no_store_adds_cache_control_to_success_responses() {
        let response = with_no_store(text(StatusCode::OK, "done"));

        assert_eq!(response.status(), StatusCode::OK);
        assert_no_store(&response);
    }

    #[test]
    fn explicit_no_store_keeps_success_status_without_content_type() {
        let response = no_store(StatusCode::NO_CONTENT);

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_no_store(&response);
        assert!(response.headers().get(header::CONTENT_TYPE).is_none());
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
                xml_body_too_large(),
                StatusCode::PAYLOAD_TOO_LARGE,
                "WebDAV XML body too large",
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

    #[actix_web::test]
    async fn lock_token_submitted_xml_uses_encoded_href() {
        let path = DavPath::new("/dir/space file.txt").expect("test path should be valid");
        let response = lock_token_submitted_response(StatusCode::LOCKED, "/webdav", &path);

        assert_eq!(response.status(), StatusCode::LOCKED);
        assert_no_store(&response);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE),
            Some(&header::HeaderValue::from_static(XML_CONTENT_TYPE))
        );

        let body = body_text(response).await;
        assert!(body.contains("<D:lock-token-submitted>"), "{body}");
        assert!(body.contains("/webdav/dir/space%20file.txt"), "{body}");
    }

    #[actix_web::test]
    async fn propfind_finite_depth_xml_is_rfc4918_precondition_body() {
        let response = propfind_finite_depth_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_no_store(&response);

        let body = body_text(response).await;
        assert!(body.contains("<D:error"), "{body}");
        assert!(body.contains("xmlns:D=\"DAV:\""), "{body}");
        assert!(body.contains("<D:propfind-finite-depth />"), "{body}");
    }

    #[actix_web::test]
    async fn lock_token_matches_request_uri_xml_has_precondition_element() {
        let response = lock_token_matches_request_uri_response(StatusCode::CONFLICT);

        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert_no_store(&response);

        let body = body_text(response).await;
        assert!(body.contains("<D:error"), "{body}");
        assert!(body.contains("xmlns:D=\"DAV:\""), "{body}");
        assert!(
            body.contains("<D:lock-token-matches-request-uri />"),
            "{body}"
        );
    }
}

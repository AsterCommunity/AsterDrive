//! CSRF 中间件测试。

use actix_web::cookie::Cookie;

use crate::api::subcode::ApiSubcode;
use crate::config::RuntimeConfig;

use super::source::{ensure_headers_allowed, ensure_request_source_allowed};
use super::{
    CSRF_COOKIE, CSRF_HEADER, RequestSourceMode, build_csrf_token, ensure_double_submit_token,
};

fn host_with_len(len: usize) -> String {
    let suffix = ".example.com";
    format!("{}{}", "a".repeat(len - suffix.len()), suffix)
}

#[test]
fn accepts_same_origin_and_public_site_origin() {
    assert!(
        ensure_headers_allowed(
            Some("http://localhost"),
            None,
            Some("same-origin"),
            "http://localhost",
            &["https://drive.example.com".to_string()],
            RequestSourceMode::Required,
        )
        .is_ok()
    );

    assert!(
        ensure_headers_allowed(
            Some("https://drive.example.com"),
            None,
            Some("same-origin"),
            "http://127.0.0.1:3000",
            &["https://drive.example.com".to_string()],
            RequestSourceMode::Required,
        )
        .is_ok()
    );
}

#[test]
fn same_site_fetch_metadata_requires_trusted_origin_or_referer() {
    assert!(
        ensure_headers_allowed(
            Some("https://panel.example.com"),
            None,
            Some("same-site"),
            "https://api.example.com",
            &[
                "https://api.example.com".to_string(),
                "https://panel.example.com".to_string()
            ],
            RequestSourceMode::OptionalWhenPresent,
        )
        .is_ok()
    );

    assert!(
        ensure_headers_allowed(
            None,
            Some("https://panel.example.com/files"),
            Some("same-site"),
            "https://api.example.com",
            &[
                "https://api.example.com".to_string(),
                "https://panel.example.com".to_string()
            ],
            RequestSourceMode::OptionalWhenPresent,
        )
        .is_ok()
    );

    let err = ensure_headers_allowed(
        None,
        None,
        Some("same-site"),
        "https://api.example.com",
        &["https://api.example.com".to_string()],
        RequestSourceMode::OptionalWhenPresent,
    )
    .unwrap_err();
    assert!(err.message().contains("missing trusted request source"));
}

#[test]
fn rejects_untrusted_fetch_metadata_values() {
    for fetch_site in ["cross-site", "none"] {
        let err = ensure_headers_allowed(
            None,
            None,
            Some(fetch_site),
            "https://drive.example.com",
            &[],
            RequestSourceMode::OptionalWhenPresent,
        )
        .unwrap_err();
        assert!(err.message().contains("untrusted request source"));
    }
}

#[test]
fn rejects_untrusted_origin_and_missing_required_source() {
    let err = ensure_headers_allowed(
        Some("https://evil.example.com"),
        None,
        None,
        "https://drive.example.com",
        &[],
        RequestSourceMode::OptionalWhenPresent,
    )
    .unwrap_err();
    assert!(err.message().contains("untrusted request origin"));

    let err = ensure_headers_allowed(
        None,
        None,
        None,
        "https://drive.example.com",
        &[],
        RequestSourceMode::Required,
    )
    .unwrap_err();
    assert!(err.message().contains("missing request source"));
}

#[test]
fn rejects_oversized_request_source_values_before_normalization() {
    let max_host = host_with_len(512);
    let req = actix_web::test::TestRequest::post()
        .insert_header(("Host", max_host.as_str()))
        .insert_header(("Origin", format!("http://{max_host}")))
        .to_http_request();
    assert!(
        ensure_request_source_allowed(&req, &RuntimeConfig::new(), RequestSourceMode::Required)
            .is_ok()
    );

    let long_host = host_with_len(513);
    let req = actix_web::test::TestRequest::post()
        .insert_header(("Host", long_host))
        .insert_header(("Origin", "https://drive.example.com"))
        .to_http_request();
    let err =
        ensure_request_source_allowed(&req, &RuntimeConfig::new(), RequestSourceMode::Required)
            .unwrap_err();
    assert!(err.message().contains("request host"));

    let req = actix_web::test::TestRequest::post()
        .insert_header(("Host", "drive.example.com"))
        .insert_header(("X-Forwarded-Proto", "x".repeat(17)))
        .insert_header(("Origin", "https://drive.example.com"))
        .to_http_request();
    let err =
        ensure_request_source_allowed(&req, &RuntimeConfig::new(), RequestSourceMode::Required)
            .unwrap_err();
    assert!(err.message().contains("request scheme"));

    let max_origin = format!("https://{}", host_with_len(2040));
    assert_eq!(max_origin.len(), 2048);
    assert!(
        ensure_headers_allowed(
            Some(&max_origin),
            None,
            None,
            "https://drive.example.com",
            std::slice::from_ref(&max_origin),
            RequestSourceMode::OptionalWhenPresent,
        )
        .is_ok()
    );

    let long_origin = format!("https://{}", host_with_len(2041));
    assert_eq!(long_origin.len(), 2049);
    let err = ensure_headers_allowed(
        Some(&long_origin),
        None,
        None,
        "https://drive.example.com",
        &[],
        RequestSourceMode::OptionalWhenPresent,
    )
    .unwrap_err();
    assert!(err.message().contains("Origin"));

    let max_referer_authority = host_with_len(528);
    let max_referer_origin = format!("https://{max_referer_authority}");
    let max_referer = format!("{max_referer_origin}/files");
    assert!(
        ensure_headers_allowed(
            None,
            Some(&max_referer),
            None,
            "https://drive.example.com",
            &[max_referer_origin],
            RequestSourceMode::OptionalWhenPresent,
        )
        .is_ok()
    );

    let long_referer_authority = format!("https://{}.example.com/files", "a".repeat(600));
    let err = ensure_headers_allowed(
        None,
        Some(&long_referer_authority),
        None,
        "https://drive.example.com",
        &[],
        RequestSourceMode::OptionalWhenPresent,
    )
    .unwrap_err();
    assert!(err.message().contains("Referer authority"));

    let max_fetch_site = "x".repeat(64);
    assert!(
        ensure_headers_allowed(
            None,
            None,
            Some(&max_fetch_site),
            "https://drive.example.com",
            &[],
            RequestSourceMode::OptionalWhenPresent,
        )
        .is_ok()
    );

    let long_fetch_site = "x".repeat(65);
    let err = ensure_headers_allowed(
        None,
        None,
        Some(&long_fetch_site),
        "https://drive.example.com",
        &[],
        RequestSourceMode::OptionalWhenPresent,
    )
    .unwrap_err();
    assert!(err.message().contains("Sec-Fetch-Site"));
}

#[test]
fn accepts_ipv6_request_host_origin_match() {
    let req = actix_web::test::TestRequest::post()
        .insert_header(("Host", "[2001:db8::1]:8443"))
        .insert_header(("Origin", "http://[2001:db8::1]:8443"))
        .to_http_request();

    assert!(
        ensure_request_source_allowed(&req, &RuntimeConfig::new(), RequestSourceMode::Required)
            .is_ok()
    );
}

#[test]
fn referer_source_check_ignores_long_path_after_bounded_origin() {
    let long_referer = format!("https://drive.example.com/files/{}", "a".repeat(10_000));

    assert!(
        ensure_headers_allowed(
            None,
            Some(&long_referer),
            Some("same-origin"),
            "https://drive.example.com",
            &[],
            RequestSourceMode::Required,
        )
        .is_ok()
    );
}

#[test]
fn invalid_referer_missing_scheme_has_scheme_subcode() {
    let err = ensure_headers_allowed(
        None,
        Some("drive.example.com/files"),
        None,
        "https://drive.example.com",
        &[],
        RequestSourceMode::OptionalWhenPresent,
    )
    .unwrap_err();

    assert_eq!(
        err.api_error_subcode(),
        Some(ApiSubcode::ValidationRequestSchemeInvalid)
    );
}

#[test]
fn accepts_missing_optional_source() {
    assert!(
        ensure_headers_allowed(
            None,
            None,
            None,
            "https://drive.example.com",
            &[],
            RequestSourceMode::OptionalWhenPresent,
        )
        .is_ok()
    );
}

#[test]
fn build_csrf_token_returns_url_safe_random_value() {
    let token_a = build_csrf_token();
    let token_b = build_csrf_token();

    assert_ne!(token_a, token_b);
    assert!(token_a.len() >= 32);
    assert!(
        token_a
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    );
}

#[test]
fn csrf_token_check_requires_cookie_for_cookie_authenticated_writes() {
    let req = actix_web::test::TestRequest::post()
        .uri("/api/v1/auth/profile")
        .to_http_request();

    let err = ensure_double_submit_token(&req).unwrap_err();
    assert!(err.message().contains("missing CSRF cookie"));
}

#[test]
fn csrf_token_check_requires_matching_cookie_and_header() {
    let req = actix_web::test::TestRequest::patch()
        .uri("/api/v1/auth/profile")
        .insert_header(("Origin", "http://localhost"))
        .cookie(Cookie::new(CSRF_COOKIE, "token-a"))
        .insert_header((CSRF_HEADER, "token-a"))
        .to_http_request();
    assert!(ensure_double_submit_token(&req).is_ok());

    let missing_header = actix_web::test::TestRequest::patch()
        .uri("/api/v1/auth/profile")
        .insert_header(("Origin", "http://localhost"))
        .cookie(Cookie::new(CSRF_COOKIE, "token-a"))
        .to_http_request();
    let err = ensure_double_submit_token(&missing_header).unwrap_err();
    assert!(err.message().contains("missing X-CSRF-Token"));

    let mismatch = actix_web::test::TestRequest::patch()
        .uri("/api/v1/auth/profile")
        .insert_header(("Origin", "http://localhost"))
        .cookie(Cookie::new(CSRF_COOKIE, "token-a"))
        .insert_header((CSRF_HEADER, "token-b"))
        .to_http_request();
    let err = ensure_double_submit_token(&mismatch).unwrap_err();
    assert!(err.message().contains("invalid CSRF token"));
}

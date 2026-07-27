//! 集成测试：`webdav_deltav`。

use crate::common;

use actix_web::test;
use base64::Engine;

fn basic_auth_header(username: &str, password: &str) -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"))
    )
}

fn webdav_test_username(label: &str) -> String {
    format!("webdav-{label}-{}", uuid::Uuid::new_v4().simple())
}

fn webdav_test_password(label: &str) -> String {
    format!("TEST_PASSWORD_{label}_{}", uuid::Uuid::new_v4().simple())
}

macro_rules! create_webdav_basic_auth {
    ($app:expr, $token:expr) => {{
        let username = webdav_test_username("deltav");
        let password = webdav_test_password("DELTAV");
        let req = test::TestRequest::post()
            .uri("/api/v1/webdav-accounts")
            .insert_header(("Cookie", common::access_cookie_header(&$token)))
            .insert_header(common::csrf_header_for(&$token))
            .set_json(serde_json::json!({
                "username": &username,
                "password": &password
            }))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 201, "create WebDAV account should return 201");
        basic_auth_header(&username, &password)
    }};
}

/// 辅助宏：WebDAV PUT 上传文件
macro_rules! webdav_put {
    ($app:expr, $path:expr, $auth:expr, $content:expr) => {{
        let req = test::TestRequest::put()
            .uri($path)
            .insert_header(("Authorization", $auth.to_string()))
            .insert_header(("Content-Type", "application/octet-stream"))
            .set_payload($content)
            .to_request();
        let resp: actix_web::dev::ServiceResponse = test::call_service(&$app, req).await;
        assert!(
            resp.status() == 201 || resp.status() == 204,
            "PUT {} should return 201/204, got {}",
            $path,
            resp.status()
        );
    }};
}

/// 辅助宏：发送 REPORT version-tree 请求
macro_rules! send_version_tree {
    ($app:expr, $path:expr, $auth:expr) => {{
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
<D:version-tree xmlns:D="DAV:">
  <D:prop>
    <D:version-name/>
    <D:creator-displayname/>
    <D:getcontentlength/>
    <D:getlastmodified/>
  </D:prop>
</D:version-tree>"#;

        let req = test::TestRequest::with_uri($path)
            .method(actix_web::http::Method::from_bytes(b"REPORT").unwrap())
            .insert_header(("Authorization", $auth.to_string()))
            .insert_header(("Content-Type", "application/xml"))
            .set_payload(body)
            .to_request();
        let resp: actix_web::dev::ServiceResponse = test::call_service(&$app, req).await;
        resp
    }};
}

// ── REPORT version-tree：文件存在但无历史版本 ──────────────────

#[actix_web::test]
async fn test_deltav_report_no_versions() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    // 上传文件（首次上传不产生历史版本）
    webdav_put!(app, "/webdav/hello.txt", auth, "v1 content");

    // REPORT version-tree
    let resp = send_version_tree!(app, "/webdav/hello.txt", auth);
    assert_eq!(resp.status(), 207, "REPORT should return 207");

    let body = test::read_body(resp).await;
    let xml = String::from_utf8_lossy(&body);

    // 应包含当前版本
    assert!(
        xml.contains("D:multistatus"),
        "should have multistatus root: {xml}"
    );
    assert!(
        xml.contains("D:response"),
        "should have at least one response: {xml}"
    );
    assert!(
        xml.contains("hello.txt"),
        "href should contain filename: {xml}"
    );
    assert!(
        xml.contains("current"),
        "current version should have version-name 'current': {xml}"
    );

    // 没有历史版本，不应有 V1、V2 这样的标记
    assert!(
        !xml.contains(">V1<"),
        "should have no history versions: {xml}"
    );
}

// ── REPORT version-tree：文件有历史版本 ──────────────────────

#[actix_web::test]
async fn test_deltav_report_with_versions() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    // 上传 → 覆盖 → 覆盖（产生 2 个历史版本）
    webdav_put!(app, "/webdav/data.txt", auth, "version 1");
    webdav_put!(app, "/webdav/data.txt", auth, "version 2");
    webdav_put!(app, "/webdav/data.txt", auth, "version 3");

    let resp = send_version_tree!(app, "/webdav/data.txt", auth);
    assert_eq!(resp.status(), 207);

    let body = test::read_body(resp).await;
    let xml = String::from_utf8_lossy(&body);

    // 当前版本 + 2 个历史版本 = 至少 3 个 response
    let response_count = xml.matches("<D:response>").count();
    assert!(
        response_count >= 3,
        "should have >=3 responses (current + 2 history), got {response_count}: {xml}"
    );

    // 应包含版本属性
    assert!(
        xml.contains("current"),
        "should have current version: {xml}"
    );
    assert!(
        xml.contains("D:version-name"),
        "should have version-name prop: {xml}"
    );
    assert!(
        xml.contains("D:getcontentlength"),
        "should have size: {xml}"
    );
    assert!(
        xml.contains("D:creator-displayname"),
        "should have creator: {xml}"
    );
    assert!(
        xml.contains("testuser"),
        "creator should be testuser: {xml}"
    );
}

// ── REPORT：对文件夹应返回 409 ──────────────────────────────

#[actix_web::test]
async fn test_deltav_report_on_folder() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::with_uri("/webdav/mydir/")
        .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let resp = send_version_tree!(app, "/webdav/mydir/", auth);
    assert_eq!(
        resp.status(),
        409,
        "REPORT on folder should return 409, got {}",
        resp.status()
    );
}

// ── REPORT：不存在的文件应返回 404 ──────────────────────────

#[actix_web::test]
async fn test_deltav_report_not_found() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let resp = send_version_tree!(app, "/webdav/nonexistent.txt", auth);
    assert_eq!(resp.status(), 404);
}

// ── REPORT：不支持的报告类型应返回 501 ──────────────────────

#[actix_web::test]
async fn test_deltav_report_unsupported_type() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    webdav_put!(app, "/webdav/file.txt", auth, "content");

    let body = r#"<?xml version="1.0" encoding="utf-8"?>
<D:expand-property xmlns:D="DAV:"/>"#;

    let req = test::TestRequest::with_uri("/webdav/file.txt")
        .method(actix_web::http::Method::from_bytes(b"REPORT").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(body)
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 501);
}

// ── VERSION-CONTROL：文件返回 200 ───────────────────────────

#[actix_web::test]
async fn test_deltav_version_control_file() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    webdav_put!(app, "/webdav/vc.txt", auth, "content");

    let req = test::TestRequest::with_uri("/webdav/vc.txt")
        .method(actix_web::http::Method::from_bytes(b"VERSION-CONTROL").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_deltav_version_control_accepts_rfc3253_any_extensions() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    webdav_put!(app, "/webdav/vc-any.txt", auth, "content");

    for body in [
        r#"<D:version-control xmlns:D="DAV:"/>"#,
        r#"<V:version-control xmlns:V="DAV:" xmlns:X="urn:x" X:flag="1">
              text<X:future><V:nested/></X:future><![CDATA[more]]>
            </V:version-control>"#,
    ] {
        let req = test::TestRequest::with_uri("/webdav/vc-any.txt")
            .method(actix_web::http::Method::from_bytes(b"VERSION-CONTROL").unwrap())
            .insert_header(("Authorization", auth.clone()))
            .insert_header(("Content-Type", "application/xml"))
            .set_payload(body)
            .to_request();
        let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            200,
            "VERSION-CONTROL should accept a safe DAV:version-control ANY body"
        );
    }
}

#[actix_web::test]
async fn test_deltav_version_control_rejects_invalid_xml_grammar_and_entities() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    webdav_put!(app, "/webdav/vc-invalid.txt", auth, "content");

    let cases = [
        (
            "wrong root namespace",
            r#"<X:version-control xmlns:X="urn:x"/>"#,
            actix_web::http::StatusCode::BAD_REQUEST,
        ),
        (
            "whitespace-only body",
            " \r\n\t",
            actix_web::http::StatusCode::BAD_REQUEST,
        ),
        (
            "external entity",
            r#"<!DOCTYPE x [<!ENTITY e SYSTEM "file:///TARGET">]><D:version-control xmlns:D="DAV:">&e;</D:version-control>"#,
            actix_web::http::StatusCode::FORBIDDEN,
        ),
    ];

    for (label, body, expected_status) in cases {
        let req = test::TestRequest::with_uri("/webdav/vc-invalid.txt")
            .method(actix_web::http::Method::from_bytes(b"VERSION-CONTROL").unwrap())
            .insert_header(("Authorization", auth.clone()))
            .insert_header(("Content-Type", "application/xml"))
            .set_payload(body)
            .to_request();
        let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            expected_status,
            "invalid VERSION-CONTROL case should be rejected: {label}"
        );
    }
}

#[actix_web::test]
async fn test_deltav_report_enforces_version_tree_prop_grammar() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    webdav_put!(app, "/webdav/report-grammar.txt", auth, "content");

    let valid = r#"<D:version-tree xmlns:D="DAV:" xmlns:X="urn:x">
      <D:future><D:prop><D:not-active/></D:prop></D:future>
      <D:prop><D:getetag><D:ignored>value</D:ignored></D:getetag></D:prop>
      <X:future/>
    </D:version-tree>"#;
    let req = test::TestRequest::with_uri("/webdav/report-grammar.txt")
        .method(actix_web::http::Method::from_bytes(b"REPORT").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(valid)
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);

    for body in [
        r#"<D:version-tree xmlns:D="DAV:"><D:prop/><D:prop/></D:version-tree>"#,
        r#"<D:version-tree xmlns:D="DAV:">text</D:version-tree>"#,
        r#"<D:version-tree xmlns:D="DAV:"><D:prop><D:getetag>value</D:getetag></D:prop></D:version-tree>"#,
    ] {
        let req = test::TestRequest::with_uri("/webdav/report-grammar.txt")
            .method(actix_web::http::Method::from_bytes(b"REPORT").unwrap())
            .insert_header(("Authorization", auth.clone()))
            .insert_header(("Content-Type", "application/xml"))
            .set_payload(body)
            .to_request();
        let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400, "invalid version-tree grammar must fail");
    }
}

// ── VERSION-CONTROL：文件夹返回 405 ─────────────────────────

#[actix_web::test]
async fn test_deltav_version_control_folder() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::with_uri("/webdav/somedir/")
        .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let _: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;

    let req = test::TestRequest::with_uri("/webdav/somedir/")
        .method(actix_web::http::Method::from_bytes(b"VERSION-CONTROL").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 405);
}

// ── VERSION-CONTROL：不存在返回 404 ─────────────────────────

#[actix_web::test]
async fn test_deltav_version_control_not_found() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::with_uri("/webdav/nope.txt")
        .method(actix_web::http::Method::from_bytes(b"VERSION-CONTROL").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

// ── OPTIONS 应包含 version-control ──────────────────────────

#[actix_web::test]
async fn test_deltav_options_header() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::with_uri("/webdav/")
        .method(actix_web::http::Method::OPTIONS)
        .insert_header(("Authorization", auth))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let dav_header = resp
        .headers()
        .get("DAV")
        .map(|v| v.to_str().unwrap_or(""))
        .unwrap_or("");
    assert!(
        dav_header.contains("version-control"),
        "DAV header should contain 'version-control', got: '{dav_header}'"
    );
}

// ── 未认证应返回 401 ────────────────────────────────────────

#[actix_web::test]
async fn test_deltav_report_unauthorized() {
    let app = setup_with_webdav!();

    let body = r#"<?xml version="1.0" encoding="utf-8"?>
<D:version-tree xmlns:D="DAV:"/>"#;

    let req = test::TestRequest::with_uri("/webdav/any.txt")
        .method(actix_web::http::Method::from_bytes(b"REPORT").unwrap())
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(body)
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
    assert_eq!(
        resp.headers()
            .get("WWW-Authenticate")
            .and_then(|value| value.to_str().ok()),
        Some("Basic realm=\"AsterDrive WebDAV\"")
    );
}

//! RFC 3253 capability-withdrawal integration tests.

use crate::common;

use actix_web::test;
use base64::Engine;

fn basic_auth_header(username: &str, password: &str) -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"))
    )
}

async fn create_webdav_auth(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    token: &str,
) -> String {
    let username = format!("webdav-deltav-{}", uuid::Uuid::new_v4().simple());
    let password = format!("TEST_PASSWORD_{}", uuid::Uuid::new_v4().simple());
    let req = test::TestRequest::post()
        .uri("/api/v1/webdav-accounts")
        .insert_header(("Cookie", common::access_cookie_header(token)))
        .insert_header(common::csrf_header_for(token))
        .set_json(serde_json::json!({
            "username": &username,
            "password": &password
        }))
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 201);
    basic_auth_header(&username, &password)
}

async fn put_file(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    auth: &str,
    path: &str,
) {
    let req = test::TestRequest::put()
        .uri(path)
        .insert_header(("Authorization", auth))
        .set_payload("content")
        .to_request();
    let resp = test::call_service(app, req).await;
    assert!(matches!(resp.status().as_u16(), 201 | 204));
}

#[actix_web::test]
async fn deltav_methods_are_not_advertised_or_dispatched() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_auth(&app, &token).await;
    put_file(&app, &auth, "/webdav/file.txt").await;

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri("/webdav/file.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let dav = resp
        .headers()
        .get("DAV")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let allow = resp
        .headers()
        .get("Allow")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(
        !dav.split(',')
            .any(|token| token.trim() == "version-control")
    );
    assert!(!allow.split(',').any(|method| method.trim() == "REPORT"));
    assert!(
        !allow
            .split(',')
            .any(|method| method.trim() == "VERSION-CONTROL")
    );

    for method in ["REPORT", "VERSION-CONTROL"] {
        let req = test::TestRequest::default()
            .method(actix_web::http::Method::from_bytes(method.as_bytes()).unwrap())
            .uri("/webdav/file.txt")
            .insert_header(("Authorization", auth.clone()))
            .insert_header(("Content-Type", "application/xml"))
            .set_payload("<D:version-tree xmlns:D=\"DAV:\"><broken")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 405);
        let allow = resp
            .headers()
            .get("Allow")
            .and_then(|value| value.to_str().ok())
            .expect("405 must carry the target capability Allow set");
        assert!(!allow.split(',').any(|value| value.trim() == method));
    }
}

#[actix_web::test]
async fn deltav_methods_are_withdrawn_for_collection_and_unmapped_targets() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_auth(&app, &token).await;
    let req = test::TestRequest::default()
        .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
        .uri("/webdav/collection/")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 201);

    for target in ["/webdav/collection/", "/webdav/missing.txt"] {
        for method in ["REPORT", "VERSION-CONTROL"] {
            let req = test::TestRequest::default()
                .method(actix_web::http::Method::from_bytes(method.as_bytes()).unwrap())
                .uri(target)
                .insert_header(("Authorization", auth.clone()))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(
                resp.status(),
                405,
                "{method} must stay withdrawn for {target}"
            );
            assert!(resp.headers().contains_key("Allow"));
        }
    }
}

#[actix_web::test]
async fn deltav_requests_still_require_webdav_authentication() {
    let app = setup_with_webdav!();
    for method in ["REPORT", "VERSION-CONTROL"] {
        let req = test::TestRequest::default()
            .method(actix_web::http::Method::from_bytes(method.as_bytes()).unwrap())
            .uri("/webdav/any.txt")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
        assert_eq!(
            resp.headers()
                .get("WWW-Authenticate")
                .and_then(|value| value.to_str().ok()),
            Some("Basic realm=\"AsterDrive WebDAV\"")
        );
    }
}

//! RFC 3253 core DeltaV integration tests.

use crate::common;

use actix_web::test;
use base64::Engine;

const VERSION_HREF_PREFIX: &str = "/webdav/.asterdrive-deltav/versions/";

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

async fn version_tree_report(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    auth: &str,
    path: &str,
) -> String {
    let req = test::TestRequest::default()
        .method(actix_web::http::Method::from_bytes(b"REPORT").unwrap())
        .uri(path)
        .insert_header(("Authorization", auth))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload("<D:version-tree xmlns:D=\"DAV:\"/>")
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 207);
    String::from_utf8(test::read_body(resp).await.to_vec()).expect("REPORT XML should be UTF-8")
}

fn version_hrefs(report: &str) -> Vec<String> {
    let mut hrefs = Vec::new();
    let mut remaining = report;
    while let Some(offset) = remaining.find(VERSION_HREF_PREFIX) {
        let candidate = &remaining[offset..];
        let end = candidate
            .find('<')
            .expect("version href should terminate before the next XML element");
        let href = candidate[..end].to_owned();
        if !hrefs.contains(&href) {
            hrefs.push(href);
        }
        remaining = &candidate[end..];
    }
    hrefs
}

#[actix_web::test]
async fn deltav_version_control_report_and_immutable_resource_workflow() {
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
        .expect("OPTIONS should include DAV")
        .to_str()
        .expect("DAV should be valid UTF-8");
    let allow = resp
        .headers()
        .get("Allow")
        .expect("OPTIONS should include Allow")
        .to_str()
        .expect("Allow should be valid UTF-8");
    assert!(
        dav.split(',')
            .any(|token| token.trim() == "version-control")
    );
    assert!(
        allow
            .split(',')
            .any(|method| method.trim() == "VERSION-CONTROL")
    );
    assert!(!allow.split(',').any(|method| method.trim() == "REPORT"));

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::from_bytes(b"VERSION-CONTROL").unwrap())
        .uri("/webdav/file.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload("<D:version-control xmlns:D=\"DAV:\"/>")
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 200);

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::from_bytes(b"VERSION-CONTROL").unwrap())
        .uri("/webdav/file.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload("<D:version-control xmlns:D=\"DAV:\"/>")
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 200);

    let req = test::TestRequest::with_uri("/webdav/file.txt")
        .method(actix_web::http::Method::from_bytes(b"PROPPATCH").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(
            "<D:propertyupdate xmlns:D=\"DAV:\" xmlns:A=\"urn:test\"><D:set><D:prop><A:note>one</A:note></D:prop></D:set></D:propertyupdate>",
        )
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 207);

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri("/webdav/file.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let allow = resp
        .headers()
        .get("Allow")
        .and_then(|value| value.to_str().ok())
        .expect("OPTIONS should include Allow");
    assert!(allow.split(',').any(|method| method.trim() == "REPORT"));

    let report = version_tree_report(&app, &auth, "/webdav/file.txt").await;
    let hrefs = version_hrefs(&report);
    assert_eq!(
        hrefs.len(),
        2,
        "controlled PROPPATCH should append exactly one immutable revision:\n{report}"
    );
    let version_path = hrefs[0].clone();
    let property_version_path = hrefs[1].clone();
    assert!(report.contains("version-name"));

    let req = test::TestRequest::with_uri("/webdav/file.txt")
        .method(actix_web::http::Method::from_bytes(b"REPORT").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(
            "<D:expand-property xmlns:D=\"DAV:\"><D:property name=\"checked-in\"><D:property name=\"version-name\"/></D:property></D:expand-property>",
        )
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);
    let expanded = String::from_utf8(test::read_body(resp).await.to_vec()).unwrap();
    assert!(expanded.contains("checked-in"));
    assert!(expanded.contains("version-name"));

    let req = test::TestRequest::get()
        .uri(&version_path)
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("Content-Length")
            .and_then(|value| value.to_str().ok()),
        Some("7")
    );
    assert_eq!(&test::read_body(resp).await[..], b"content");

    let req = test::TestRequest::get()
        .uri(&version_path)
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Range", "bytes=1-3"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 206);
    assert_eq!(
        resp.headers()
            .get("Content-Range")
            .and_then(|value| value.to_str().ok()),
        Some("bytes 1-3/7")
    );
    assert_eq!(&test::read_body(resp).await[..], b"ont");

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::HEAD)
        .uri(&version_path)
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert!(resp.headers().contains_key("ETag"));

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri(&version_path)
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let allow = resp
        .headers()
        .get("Allow")
        .and_then(|value| value.to_str().ok())
        .expect("immutable OPTIONS should include Allow");
    for method in ["GET", "HEAD", "PROPFIND", "REPORT", "COPY"] {
        assert!(
            allow.split(',').any(|candidate| candidate.trim() == method),
            "immutable Allow should contain {method}: {allow}"
        );
    }
    for method in ["PUT", "PROPPATCH", "MOVE", "DELETE", "LOCK"] {
        assert!(
            !allow.split(',').any(|candidate| candidate.trim() == method),
            "immutable Allow should omit {method}: {allow}"
        );
    }

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .uri(&version_path)
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(
            "<D:lockinfo xmlns:D=\"DAV:\"><D:lockscope><D:exclusive/></D:lockscope><D:locktype><D:write/></D:locktype></D:lockinfo>",
        )
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 405);
    assert!(resp.headers().contains_key("Allow"));

    let req = test::TestRequest::with_uri(&version_path)
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Depth", "0"))
        .set_payload("<D:propfind xmlns:D=\"DAV:\"><D:prop><D:version-name/><D:getetag/></D:prop></D:propfind>")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);
    let propfind = String::from_utf8(test::read_body(resp).await.to_vec()).unwrap();
    assert!(propfind.contains("version-name"));
    assert!(propfind.contains("getetag"));

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::from_bytes(b"COPY").unwrap())
        .uri(&property_version_path)
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/copied-version.txt"))
        .insert_header(("Depth", "infinity"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::get()
        .uri("/webdav/copied-version.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(&test::read_body(resp).await[..], b"content");

    let req = test::TestRequest::with_uri("/webdav/copied-version.txt")
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Depth", "0"))
        .set_payload(
            "<D:propfind xmlns:D=\"DAV:\" xmlns:A=\"urn:test\"><D:prop><A:note/></D:prop></D:propfind>",
        )
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);
    let copied_propfind = String::from_utf8(test::read_body(resp).await.to_vec()).unwrap();
    assert!(copied_propfind.contains("note"));
    assert!(copied_propfind.contains("one"));

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri("/webdav/copied-version.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let allow = resp
        .headers()
        .get("Allow")
        .and_then(|value| value.to_str().ok())
        .expect("copied file OPTIONS should include Allow");
    assert!(
        allow
            .split(',')
            .any(|method| method.trim() == "VERSION-CONTROL")
    );
    assert!(!allow.split(',').any(|method| method.trim() == "REPORT"));

    let req = test::TestRequest::put()
        .uri(&version_path)
        .insert_header(("Authorization", auth.clone()))
        .set_payload("changed")
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 403);

    let req = test::TestRequest::delete()
        .uri("/webdav/file.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 204);

    let req = test::TestRequest::get()
        .uri(&version_path)
        .insert_header(("Authorization", auth))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 404);
}

#[actix_web::test]
async fn deltav_proppatch_revisions_require_a_real_controlled_property_change() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_auth(&app, &token).await;
    put_file(&app, &auth, "/webdav/property-revisions.txt").await;

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::HEAD)
        .uri("/webdav/property-revisions.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let initial_etag = resp
        .headers()
        .get("ETag")
        .expect("HEAD should expose the initial ETag")
        .clone();

    let property_one = "<D:propertyupdate xmlns:D=\"DAV:\" xmlns:A=\"urn:test\"><D:set><D:prop><A:note>one</A:note></D:prop></D:set></D:propertyupdate>";
    let req = test::TestRequest::with_uri("/webdav/property-revisions.txt")
        .method(actix_web::http::Method::from_bytes(b"PROPPATCH").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(property_one)
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 207);

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::HEAD)
        .uri("/webdav/property-revisions.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("ETag"),
        Some(&initial_etag),
        "uncontrolled PROPPATCH must not advance the representation revision"
    );

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::from_bytes(b"VERSION-CONTROL").unwrap())
        .uri("/webdav/property-revisions.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload("<D:version-control xmlns:D=\"DAV:\"/>")
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 200);
    let initial_report = version_tree_report(&app, &auth, "/webdav/property-revisions.txt").await;
    assert_eq!(version_hrefs(&initial_report).len(), 1);

    let req = test::TestRequest::with_uri("/webdav/property-revisions.txt")
        .method(actix_web::http::Method::from_bytes(b"PROPPATCH").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(property_one)
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 207);
    let no_op_set_report = version_tree_report(&app, &auth, "/webdav/property-revisions.txt").await;
    assert_eq!(
        version_hrefs(&no_op_set_report).len(),
        1,
        "setting the existing value must not append a revision"
    );

    let req = test::TestRequest::with_uri("/webdav/property-revisions.txt")
        .method(actix_web::http::Method::from_bytes(b"PROPPATCH").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(
            "<D:propertyupdate xmlns:D=\"DAV:\" xmlns:A=\"urn:test\"><D:remove><D:prop><A:missing/></D:prop></D:remove></D:propertyupdate>",
        )
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 207);
    let no_op_delete_report =
        version_tree_report(&app, &auth, "/webdav/property-revisions.txt").await;
    assert_eq!(
        version_hrefs(&no_op_delete_report).len(),
        1,
        "deleting a missing property must not append a revision"
    );

    let req = test::TestRequest::with_uri("/webdav/property-revisions.txt")
        .method(actix_web::http::Method::from_bytes(b"PROPPATCH").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(
            "<D:propertyupdate xmlns:D=\"DAV:\" xmlns:A=\"urn:test\"><D:set><D:prop><A:note>two</A:note></D:prop></D:set></D:propertyupdate>",
        )
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 207);
    let changed_report = version_tree_report(&app, &auth, "/webdav/property-revisions.txt").await;
    assert_eq!(
        version_hrefs(&changed_report).len(),
        2,
        "one real controlled property change must append exactly one revision"
    );
}

#[actix_web::test]
async fn deltav_methods_remain_unsupported_for_collection_and_unmapped_targets() {
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

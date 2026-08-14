//! RFC 3253 core DeltaV integration tests.

use crate::common;

use actix_web::test;
use base64::Engine;

fn basic_auth_header(username: &str, password: &str) -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"))
    )
}

async fn property_audit_count(
    state: &aster_drive::runtime::PrimaryAppState,
    entity_id: i64,
) -> u64 {
    use aster_drive_model::entities::audit_log;
    use aster_drive_model::types::{AuditAction, AuditEntityType};
    use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};

    audit_log::Entity::find()
        .filter(audit_log::Column::EntityId.eq(entity_id))
        .filter(audit_log::Column::EntityType.eq(AuditEntityType::File.as_str()))
        .filter(
            audit_log::Column::Action
                .is_in([AuditAction::PropertySet, AuditAction::PropertyDelete]),
        )
        .count(state.writer_db())
        .await
        .unwrap()
}

#[actix_web::test]
async fn deltav_version_xml_parser_skips_empty_version_name_propstats() {
    let xml = r#"<D:multistatus xmlns:D="DAV:">
        <D:response>
            <D:href>/webdav/.asterdrive-deltav/versions/00000000-0000-0000-0000-000000000001</D:href>
            <D:propstat><D:prop><D:version-name/></D:prop><D:status>HTTP/1.1 404 Not Found</D:status></D:propstat>
            <D:propstat><D:prop><D:version-name>1</D:version-name></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat>
        </D:response>
    </D:multistatus>"#;

    assert_eq!(
        common::deltav_version_entries(xml),
        vec![(
            "/webdav/.asterdrive-deltav/versions/00000000-0000-0000-0000-000000000001".to_owned(),
            "1".to_owned(),
        )]
    );
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
    let hrefs = common::deltav_version_hrefs(&report);
    assert_eq!(
        hrefs.len(),
        2,
        "controlled PROPPATCH should append exactly one immutable revision:\n{report}"
    );
    let version_path = common::deltav_version_href_by_name(&report, "1")
        .expect("version-tree should expose the activation-root revision");
    let property_version_path = common::deltav_version_href_by_name(&report, "2")
        .expect("version-tree should expose the property-change revision");
    assert_ne!(version_path, property_version_path);

    for (body, expected_status, expected_message) in [
        (
            "<D:locate-by-history xmlns:D=\"DAV:\"/>",
            409,
            "REPORT locate-by-history is not available",
        ),
        (
            "<D:unknown-report xmlns:D=\"DAV:\"/>",
            422,
            "unknown REPORT",
        ),
    ] {
        let req = test::TestRequest::with_uri("/webdav/file.txt")
            .method(actix_web::http::Method::from_bytes(b"REPORT").unwrap())
            .insert_header(("Authorization", auth.clone()))
            .insert_header(("Content-Type", "application/xml"))
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), expected_status);
        assert_eq!(
            resp.headers()
                .get("Cache-Control")
                .and_then(|value| value.to_str().ok()),
            Some("no-store")
        );
        assert!(
            String::from_utf8(test::read_body(resp).await.to_vec())
                .unwrap()
                .contains(expected_message)
        );
    }

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
    let version_etag = resp
        .headers()
        .get("ETag")
        .expect("immutable HEAD should expose an ETag")
        .clone();

    for (range, expected_content_range, expected_body) in [
        ("bytes=-3", "bytes 4-6/7", &b"ent"[..]),
        ("bytes=4-", "bytes 4-6/7", &b"ent"[..]),
    ] {
        let req = test::TestRequest::get()
            .uri(&version_path)
            .insert_header(("Authorization", auth.clone()))
            .insert_header(("Range", range))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 206, "range {range} should be satisfiable");
        assert_eq!(
            resp.headers()
                .get("Content-Range")
                .and_then(|value| value.to_str().ok()),
            Some(expected_content_range)
        );
        assert_eq!(&test::read_body(resp).await[..], expected_body);
    }

    let req = test::TestRequest::get()
        .uri(&version_path)
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Range", "bytes=100-200"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 416);
    assert_eq!(
        resp.headers()
            .get("Content-Range")
            .and_then(|value| value.to_str().ok()),
        Some("bytes */7")
    );

    let req = test::TestRequest::get()
        .uri(&version_path)
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Range", "bytes=1-3"))
        .insert_header(("If-Range", version_etag.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 206);
    assert_eq!(&test::read_body(resp).await[..], b"ont");

    let req = test::TestRequest::get()
        .uri(&version_path)
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Range", "bytes=1-3"))
        .insert_header(("If-Range", "\"different-version\""))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(&test::read_body(resp).await[..], b"content");

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

    let req = test::TestRequest::with_uri(&property_version_path)
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Depth", "0"))
        .set_payload(
            "<D:propfind xmlns:D=\"DAV:\" xmlns:A=\"urn:test\"><D:prop><A:note/></D:prop></D:propfind>",
        )
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);
    let immutable_propfind = String::from_utf8(test::read_body(resp).await.to_vec()).unwrap();
    assert!(immutable_propfind.contains("note"));
    assert!(immutable_propfind.contains("one"));

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
        .uri(&version_path)
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 403);

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::from_bytes(b"MOVE").unwrap())
        .uri(&version_path)
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/moved-version.txt"))
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
    use aster_drive::db::repository::{file_repo, user_repo};

    let (app, state) = setup_with_webdav!(with_state);
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_auth(&app, &token).await;
    put_file(&app, &auth, "/webdav/property-revisions.txt").await;
    let user = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .expect("test user should exist");
    let file = file_repo::find_by_name_in_folder(
        state.writer_db(),
        user.id,
        None,
        "property-revisions.txt",
    )
    .await
    .unwrap()
    .expect("WebDAV file should exist");

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
    assert_eq!(property_audit_count(&state, file.id).await, 1);

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
    assert_eq!(common::deltav_version_hrefs(&initial_report).len(), 1);

    let req = test::TestRequest::with_uri("/webdav/property-revisions.txt")
        .method(actix_web::http::Method::from_bytes(b"PROPPATCH").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(property_one)
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 207);
    let no_op_set_report = version_tree_report(&app, &auth, "/webdav/property-revisions.txt").await;
    assert_eq!(
        common::deltav_version_hrefs(&no_op_set_report).len(),
        1,
        "setting the existing value must not append a revision"
    );
    assert_eq!(
        property_audit_count(&state, file.id).await,
        1,
        "setting the existing value must not emit a mutation audit"
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
        common::deltav_version_hrefs(&no_op_delete_report).len(),
        1,
        "deleting a missing property must not append a revision"
    );
    assert_eq!(
        property_audit_count(&state, file.id).await,
        1,
        "deleting a missing property must not emit a mutation audit"
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
        common::deltav_version_hrefs(&changed_report).len(),
        2,
        "one real controlled property change must append exactly one revision"
    );
    assert_eq!(property_audit_count(&state, file.id).await, 2);
}

#[actix_web::test]
async fn deltav_property_revision_refcount_quota_and_rollback_stay_balanced() {
    use aster_drive::db::repository::{file_repo, property_repo, revision_repo, user_repo};
    use aster_drive_model::types::EntityType;
    use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};

    let (app, state) = setup_with_webdav!(with_state);
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_auth(&app, &token).await;
    put_file(&app, &auth, "/webdav/refcounted-property.txt").await;

    let user = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .expect("test user should exist");
    let file = file_repo::find_by_name_in_folder(
        state.writer_db(),
        user.id,
        None,
        "refcounted-property.txt",
    )
    .await
    .unwrap()
    .expect("WebDAV file should exist");
    let blob = file_repo::find_blob_by_id(state.writer_db(), file.blob_id)
        .await
        .unwrap();
    assert_eq!(blob.ref_count, 1);
    assert_eq!(user.storage_used, 7);

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::from_bytes(b"VERSION-CONTROL").unwrap())
        .uri("/webdav/refcounted-property.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload("<D:version-control xmlns:D=\"DAV:\"/>")
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 200);

    let mut quota_limited = user.clone().into_active_model();
    quota_limited.storage_quota = Set(user.storage_used);
    quota_limited.update(state.writer_db()).await.unwrap();

    let property_patch = "<D:propertyupdate xmlns:D=\"DAV:\" xmlns:A=\"urn:test\"><D:set><D:prop><A:note>one</A:note></D:prop></D:set></D:propertyupdate>";
    let req = test::TestRequest::with_uri("/webdav/refcounted-property.txt")
        .method(actix_web::http::Method::from_bytes(b"PROPPATCH").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(property_patch)
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        507,
        "quota failure should roll back both the property and revision"
    );

    let history = revision_repo::find_history_by_file_id(state.writer_db(), file.id)
        .await
        .unwrap();
    assert_eq!(
        revision_repo::find_deltav_revisions(state.writer_db(), &history, 10)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        property_repo::find_by_key(
            state.writer_db(),
            EntityType::File,
            file.id,
            "urn:test",
            "note",
        )
        .await
        .unwrap()
        .is_none()
    );
    assert_eq!(
        file_repo::find_blob_by_id(state.writer_db(), file.blob_id)
            .await
            .unwrap()
            .ref_count,
        1
    );
    assert_eq!(
        user_repo::find_by_id(state.writer_db(), user.id)
            .await
            .unwrap()
            .storage_used,
        7
    );

    let mut quota_restored = user_repo::find_by_id(state.writer_db(), user.id)
        .await
        .unwrap()
        .into_active_model();
    quota_restored.storage_quota = Set(1024);
    quota_restored.update(state.writer_db()).await.unwrap();

    let req = test::TestRequest::with_uri("/webdav/refcounted-property.txt")
        .method(actix_web::http::Method::from_bytes(b"PROPPATCH").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(property_patch)
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 207);

    let history = revision_repo::find_history_by_file_id(state.writer_db(), file.id)
        .await
        .unwrap();
    assert_eq!(
        revision_repo::find_deltav_revisions(state.writer_db(), &history, 10)
            .await
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        file_repo::find_blob_by_id(state.writer_db(), file.blob_id)
            .await
            .unwrap()
            .ref_count,
        2
    );
    assert_eq!(
        user_repo::find_by_id(state.writer_db(), user.id)
            .await
            .unwrap()
            .storage_used,
        14
    );

    let root_revision_id = history
        .deltav_root_revision_id
        .expect("controlled history should retain an activation root");
    let req = test::TestRequest::delete()
        .uri(&format!(
            "/api/v1/files/{}/versions/{root_revision_id}",
            file.id
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 200);
    assert_eq!(
        file_repo::find_blob_by_id(state.writer_db(), file.blob_id)
            .await
            .unwrap()
            .ref_count,
        1
    );
    assert_eq!(
        user_repo::find_by_id(state.writer_db(), user.id)
            .await
            .unwrap()
            .storage_used,
        7
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

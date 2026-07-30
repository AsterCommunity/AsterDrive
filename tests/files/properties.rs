//! 集成测试：`properties`。

use crate::common;

use actix_web::test;
use aster_drive::db::repository::property_repo;
use aster_drive::runtime::SharedRuntimeState;
use aster_drive_model::types::EntityType;
use serde_json::Value;

#[actix_web::test]
async fn test_entity_properties() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    // 设置属性
    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/properties/file/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "namespace": "aster:",
            "name": "color",
            "value": "red"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "color");
    assert_eq!(body["data"]["value"], "red");

    // 列出属性
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/properties/file/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 1);

    // 删除属性
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/properties/file/{file_id}/aster:/color"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // 列出为空
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/properties/file/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 0);

    // DAV: 命名空间被拒绝
    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/properties/file/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "namespace": "DAV:",
            "name": "getcontenttype",
            "value": "text/plain"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 403 || resp.status() == 423);

    // system.* 命名空间保留给内部缓存，用户 API 不允许写入
    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/properties/file/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "namespace": "system.archive_preview",
            "name": "zip_manifest.v2",
            "value": "{}"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    property_repo::upsert(
        state.writer_db(),
        EntityType::File,
        file_id,
        "system.archive_preview",
        "zip_manifest.v2",
        Some("{}"),
    )
    .await
    .expect("internal system property should be writable through repo");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/properties/file/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let listed = body["data"].as_array().unwrap();
    assert!(
        listed
            .iter()
            .all(|item| item["namespace"] != "system.archive_preview"),
        "system properties must be hidden from user property listing: {listed:?}"
    );
}

#[actix_web::test]
async fn test_properties_reject_long_namespace_in_body_and_path() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);
    let long_namespace = "n".repeat(257);

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/properties/file/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "namespace": long_namespace,
            "name": "color",
            "value": "red"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["msg"], "namespace too long (max 256)");

    let req = test::TestRequest::delete()
        .uri(&format!(
            "/api/v1/properties/file/{file_id}/{}/color",
            "n".repeat(257)
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["msg"], "namespace too long (max 256)");
}

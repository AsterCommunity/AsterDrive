//! CORS is available only through the existing administrator connector action.
use crate::common;
use actix_web::test;
use aster_drive::api::api_error_code::ApiErrorCode;
use serde_json::{Value, json};

fn connection() -> Value {
    json!({"connector_config":{"format_version":1,"connector_id":"asterdrive.storage.qiniu","schema_version":2,"values":{
        "bucket":"archive","base_path":"tenant","download_origin":"https://origin.test",
        "provider_resumable_upload_strategy":"server_relay","object_storage_download_strategy":"presigned"
    }},"behavior":{},"credential":{"mode":"static","values":{"qiniu_access_key":"TEST_AK","qiniu_secret_key":"TEST_SK"}}})
}

#[actix_web::test]
async fn qiniu_cors_catalog_and_draft_saved_action_validation_use_generic_endpoints() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies/storage-drivers")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let descriptor = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["connector_id"] == "asterdrive.storage.qiniu")
        .unwrap();
    let action = descriptor["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["action_id"] == "configure_qiniu_cors")
        .unwrap();
    assert_eq!(action["requires_confirmation"], true);
    assert_eq!(action["mutates_remote_state"], true);
    assert_eq!(
        action["endpoints"],
        json!([
            "execute_draft_storage_policy_action",
            "execute_saved_storage_policy_action"
        ])
    );
    assert!(action["fields"].is_null());
    assert!(action.get("confirmation_fields").is_none());
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(json!({"name":"Kodo CORS","connection":connection()}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let id = body["data"]["id"].as_i64().unwrap();
    for (values, expected) in [
        (json!({}), ApiErrorCode::PolicyActionParameterRequired),
        (
            json!({"unexpected":true}),
            ApiErrorCode::PolicyActionParameterInvalid,
        ),
    ] {
        for saved in [false, true] {
            let uri = if saved {
                format!("/api/v1/admin/policies/{id}/action")
            } else {
                "/api/v1/admin/policies/action".into()
            };
            let payload = if saved {
                json!({"action_id":"configure_qiniu_cors","values":values})
            } else {
                json!({"action_id":"configure_qiniu_cors","values":values,"connection":connection()})
            };
            let req = test::TestRequest::post()
                .uri(&uri)
                .insert_header(("Cookie", common::access_cookie_header(&token)))
                .insert_header(common::csrf_header_for(&token))
                .set_json(payload)
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 400);
            let body: Value = test::read_body_json(resp).await;
            assert_eq!(body["code"], expected.as_str());
            assert!(!body.to_string().contains("TEST_SK"));
        }
    }
}

#[actix_web::test]
async fn qiniu_cors_requires_administrator_for_both_action_routes() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin, _) = register_and_login!(app);
    admin_create_user!(
        app,
        admin,
        "cors-user",
        "cors-user@example.com",
        "password123"
    );
    let (token, _) = login_user!(app, "cors-user", "password123");
    for uri in [
        "/api/v1/admin/policies/action",
        "/api/v1/admin/policies/1/action",
    ] {
        let req = test::TestRequest::post()
            .uri(uri)
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(
                json!({"action_id":"configure_qiniu_cors","values":{},"connection":connection()}),
            )
            .to_request();
        let error = test::try_call_service(&app, req).await.unwrap_err();
        assert_eq!(error.as_response_error().status_code(), 403);
    }
}

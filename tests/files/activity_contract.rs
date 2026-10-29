//! Black-box regressions that also compile against the pre-#657 API implementation.
use crate::common;
use actix_web::test;
use serde_json::{Value, json};

#[actix_web::test]
async fn creator_identity_is_available_in_details_and_list() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let id =
        common::create_empty_file_via_api(&app, &token, "attribution-contract.txt", None).await;
    for uri in [format!("/api/v1/files/{id}"), "/api/v1/folders".into()] {
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&uri)
                .insert_header(("Cookie", common::access_cookie_header(&token)))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), 200);
        let body: Value = test::read_body_json(response).await;
        let resource = if uri.ends_with("folders") {
            &body["data"]["files"][0]
        } else {
            &body["data"]
        };
        assert_eq!(
            resource["created_by"]["name"], "testuser",
            "creator identity missing from {uri}"
        );
    }
}

#[actix_web::test]
async fn rename_returns_last_modifier_even_when_optional_audit_is_off() {
    let state = common::setup().await;
    state
        .runtime_config
        .apply(common::system_config_model("audit_log_enabled", "false"));
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let id = common::create_empty_file_via_api(&app, &token, "before-contract.txt", None).await;
    let response = test::call_service(
        &app,
        test::TestRequest::patch()
            .uri(&format!("/api/v1/files/{id}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(json!({"name":"after-contract.txt"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(
        body["data"]["modified_by"]["name"], "testuser",
        "last modifier missing after successful rename"
    );
}

#[actix_web::test]
async fn rename_has_a_durable_resource_activity_feed() {
    let state = common::setup().await;
    state
        .runtime_config
        .apply(common::system_config_model("audit_log_enabled", "false"));
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let id = common::create_empty_file_via_api(&app, &token, "activity-contract.txt", None).await;
    let response = test::call_service(
        &app,
        test::TestRequest::patch()
            .uri(&format!("/api/v1/files/{id}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(json!({"name":"activity-renamed.txt"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/v1/files/{id}/activities"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .to_request(),
    )
    .await;
    assert_eq!(
        response.status(),
        200,
        "durable resource activity endpoint missing"
    );
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["data"]["items"][0]["action"], "file_rename");
    assert_eq!(body["data"]["items"][0]["actor"]["name"], "testuser");
}

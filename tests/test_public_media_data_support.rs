//! 集成测试：`public_media_data_support`。

#[macro_use]
mod common;

use actix_web::test;
use serde_json::{Value, json};

fn available_test_command() -> String {
    std::env::current_exe()
        .expect("current test executable path should be available")
        .to_string_lossy()
        .into_owned()
}

#[actix_web::test]
async fn test_public_media_data_support_returns_default_capabilities() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let req = test::TestRequest::get()
        .uri("/api/v1/public/media-data-support")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("Cache-Control")
            .and_then(|value| value.to_str().ok()),
        Some("public, max-age=60")
    );

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["version"], 1);
    assert_eq!(body["data"]["enabled"], true);
    assert_eq!(body["data"]["max_source_bytes"], 256 * 1024 * 1024);
    assert_eq!(body["data"]["kinds"]["image"]["enabled"], true);
    assert_eq!(body["data"]["kinds"]["image"]["match"], "extensions");
    assert!(
        body["data"]["kinds"]["image"]["extensions"]
            .as_array()
            .expect("image extensions should be an array")
            .iter()
            .any(|value| value == "jpg")
    );
    assert_eq!(body["data"]["kinds"]["audio"]["enabled"], true);
    assert!(
        body["data"]["kinds"]["audio"]["extensions"]
            .as_array()
            .expect("audio extensions should be an array")
            .iter()
            .any(|value| value == "mp3")
    );
    assert_eq!(body["data"]["kinds"]["video"]["enabled"], false);
}

#[actix_web::test]
async fn test_public_media_data_support_exposes_enabled_ffprobe_extensions() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/media_processing_registry_json")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(json!({
            "value": json!({
                "version": 2,
                "processors": [
                    {
                        "kind": "ffprobe_cli",
                        "enabled": true,
                        "uses": ["metadata:video"],
                        "extensions": ["MP4", ".mov"],
                        "config": {
                            "command": available_test_command()
                        }
                    },
                    {
                        "kind": "images",
                        "enabled": true,
                        "uses": ["metadata:image"]
                    },
                    {
                        "kind": "lofty",
                        "enabled": true,
                        "uses": ["metadata:audio"]
                    }
                ]
            })
            .to_string()
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/public/media-data-support")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["kinds"]["video"]["enabled"], true);
    assert_eq!(body["data"]["kinds"]["video"]["match"], "extensions");
    assert_eq!(
        body["data"]["kinds"]["video"]["extensions"],
        json!(["mov", "mp4"])
    );
}

#[actix_web::test]
async fn test_public_media_data_support_cache_is_invalidated_after_config_update() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/public/media-data-support")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["enabled"], true);
    assert_eq!(body["data"]["kinds"]["image"]["enabled"], true);

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/media_metadata_enabled")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(json!({ "value": "false" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/public/media-data-support")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["enabled"], false);
    assert_eq!(body["data"]["kinds"]["image"]["enabled"], false);
    assert_eq!(body["data"]["kinds"]["audio"]["enabled"], false);
    assert_eq!(body["data"]["kinds"]["video"]["enabled"], false);
}

//! 集成测试：`public_preview_apps`。

use crate::common;

use actix_web::test;
use serde_json::{Value, json};

#[actix_web::test]
async fn test_public_preview_apps_returns_default_registry() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let req = test::TestRequest::get()
        .uri("/api/v1/public/preview-apps")
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
    assert_eq!(body["data"]["version"], 2);
    assert!(
        body["data"]["apps"]
            .as_array()
            .is_some_and(|apps| !apps.is_empty())
    );
    assert!(body["data"].get("rules").is_none());
    assert!(body["data"]["apps"].as_array().unwrap().iter().any(|app| {
        app["key"] == "builtin.code"
            && app["labels"]["en"] == "Source view"
            && app["labels"]["zh"] == "源码视图"
    }));
    assert!(body["data"]["apps"].as_array().unwrap().iter().any(|app| {
        app["key"] == "builtin.try_text"
            && app["icon"] == "/static/preview-apps/file.svg"
            && app["labels"]["en"] == "Open as text"
    }));
    assert!(body["data"]["apps"].as_array().unwrap().iter().any(|app| {
        app["key"] == "builtin.formatted"
            && app["extensions"] == json!(["json", "xml"])
            && app["labels"]["zh"] == "格式化视图"
    }));
    assert!(body["data"]["apps"].as_array().unwrap().iter().any(|app| {
        app["key"] == "builtin.archive"
            && app["extensions"] == json!(["zip"])
            && app["labels"]["zh"] == "压缩包预览"
    }));
}

#[actix_web::test]
async fn test_public_preview_apps_uses_admin_config_and_filters_disabled_apps() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/public/preview-apps")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let default_body: Value = test::read_body_json(resp).await;
    let mut custom_config = default_body["data"].clone();
    let apps = custom_config["apps"]
        .as_array_mut()
        .expect("default apps should be an array");
    for app in apps.iter_mut() {
        let key = app["key"].as_str().unwrap_or_default();
        if key != "builtin.code" {
            app["enabled"] = json!(false);
        }
    }
    apps.push(json!({
        "key": "custom.viewer",
        "provider": "url_template",
        "icon": "Globe",
        "enabled": false,
        "labels": {
            "en": "Viewer"
        },
        "extensions": ["txt"],
        "config": {
            "mode": "iframe",
            "url_template": "https://viewer.example.com/?src={{file_preview_url}}"
        }
    }));

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/frontend_preview_apps_json")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(json!({ "value": custom_config.to_string() }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/public/preview-apps")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    let apps = body["data"]["apps"]
        .as_array()
        .expect("apps should be an array");
    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0]["key"], "builtin.code");
    assert!(body["data"].get("rules").is_none());
}

#[actix_web::test]
async fn test_admin_preview_apps_config_rejects_invalid_json() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/frontend_preview_apps_json")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(json!({ "value": "{bad json" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let body: Value = test::read_body_json(resp).await;
    assert!(
        body["msg"]
            .as_str()
            .is_some_and(|msg| msg.contains("valid JSON"))
    );
}

#[actix_web::test]
async fn test_admin_preview_apps_config_restores_missing_builtins() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/frontend_preview_apps_json")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(json!({
            "value": json!({
                "version": 2,
                "apps": [
                    {
                        "key": "custom.viewer",
                        "provider": "url_template",
                        "icon": "Globe",
                        "labels": {
                            "en": "Viewer"
                        },
                        "extensions": ["txt"],
                        "config": {
                            "mode": "iframe",
                            "url_template": "https://viewer.example.com/?src={{file_preview_url}}"
                        }
                    }
                ]
            }).to_string()
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    let value = body["data"]["value"]
        .as_str()
        .expect("saved preview apps config value should be a string");
    let stored: Value = serde_json::from_str(value).expect("saved config should be JSON");
    let stored_apps = stored["apps"]
        .as_array()
        .expect("stored apps should be an array");
    assert!(stored_apps.iter().any(|app| app["key"] == "custom.viewer"));
    assert!(stored_apps.iter().any(|app| app["key"] == "builtin.code"));
    assert!(
        stored_apps
            .iter()
            .any(|app| app["key"] == "builtin.archive")
    );

    let req = test::TestRequest::get()
        .uri("/api/v1/public/preview-apps")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let apps = body["data"]["apps"]
        .as_array()
        .expect("public apps should be an array");
    assert!(apps.iter().any(|app| app["key"] == "custom.viewer"));
    assert!(apps.iter().any(|app| app["key"] == "builtin.code"));
    assert!(apps.iter().any(|app| app["key"] == "builtin.archive"));
}

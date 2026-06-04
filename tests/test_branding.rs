//! 集成测试：`branding`。

#[macro_use]
mod common;

use actix_web::test;
use serde_json::Value;

#[actix_web::test]
async fn test_public_branding_returns_defaults() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let req = test::TestRequest::get()
        .uri("/api/v1/public/branding")
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
    assert_eq!(body["data"]["title"], "AsterDrive");
    assert_eq!(body["data"]["description"], "Self-hosted cloud storage");
    assert_eq!(body["data"]["favicon_url"], "/favicon.svg");
    assert_eq!(
        body["data"]["wordmark_dark_url"],
        "/static/asterdrive/asterdrive-dark.svg"
    );
    assert_eq!(
        body["data"]["wordmark_light_url"],
        "/static/asterdrive/asterdrive-light.svg"
    );
    assert_eq!(body["data"]["site_urls"], Value::Array(vec![]));
    assert_eq!(body["data"]["allow_user_registration"], true);
    assert_eq!(body["data"]["passkey_login_enabled"], true);
}

#[actix_web::test]
async fn test_public_branding_uses_admin_updated_values() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    for (key, value) in [
        (
            "public_site_url",
            serde_json::json!(["https://drive.example.com", "https://panel.example.com"]),
        ),
        ("auth_allow_user_registration", serde_json::json!("false")),
        ("auth_passkey_login_enabled", serde_json::json!("false")),
        ("branding_title", serde_json::json!("Nebula Drive")),
        (
            "branding_description",
            serde_json::json!("Team storage for the squad"),
        ),
        (
            "branding_favicon_url",
            serde_json::json!("https://cdn.example.com/branding/favicon.png?v=2"),
        ),
        (
            "branding_wordmark_dark_url",
            serde_json::json!("https://cdn.example.com/branding/wordmark-dark.svg?v=2"),
        ),
        (
            "branding_wordmark_light_url",
            serde_json::json!("https://cdn.example.com/branding/wordmark-light.svg?v=2"),
        ),
    ] {
        let req = test::TestRequest::put()
            .uri(&format!("/api/v1/admin/config/{key}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({ "value": value }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200, "setting {key} should succeed");
    }

    let req = test::TestRequest::get()
        .uri("/api/v1/public/branding")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["title"], "Nebula Drive");
    assert_eq!(body["data"]["description"], "Team storage for the squad");
    assert_eq!(
        body["data"]["favicon_url"],
        "https://cdn.example.com/branding/favicon.png?v=2"
    );
    assert_eq!(
        body["data"]["wordmark_dark_url"],
        "https://cdn.example.com/branding/wordmark-dark.svg?v=2"
    );
    assert_eq!(
        body["data"]["wordmark_light_url"],
        "https://cdn.example.com/branding/wordmark-light.svg?v=2"
    );
    assert_eq!(
        body["data"]["site_urls"],
        serde_json::json!(["https://drive.example.com", "https://panel.example.com"])
    );
    assert_eq!(body["data"]["allow_user_registration"], false);
    assert_eq!(body["data"]["passkey_login_enabled"], false);
}

#[actix_web::test]
async fn test_public_branding_blank_values_fall_back_to_defaults() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    for (key, value) in [
        ("branding_title", "   "),
        ("branding_description", "   "),
        ("branding_favicon_url", ""),
        ("branding_wordmark_dark_url", ""),
        ("branding_wordmark_light_url", ""),
    ] {
        let req = test::TestRequest::put()
            .uri(&format!("/api/v1/admin/config/{key}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({ "value": value }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200, "setting {key} should succeed");
    }

    let req = test::TestRequest::get()
        .uri("/api/v1/public/branding")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["title"], "AsterDrive");
    assert_eq!(body["data"]["description"], "Self-hosted cloud storage");
    assert_eq!(body["data"]["favicon_url"], "/favicon.svg");
    assert_eq!(
        body["data"]["wordmark_dark_url"],
        "/static/asterdrive/asterdrive-dark.svg"
    );
    assert_eq!(
        body["data"]["wordmark_light_url"],
        "/static/asterdrive/asterdrive-light.svg"
    );
}

#[actix_web::test]
async fn test_public_branding_preserves_non_ascii_text() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    for (key, value) in [
        ("branding_title", "猫猫云盘"),
        ("branding_description", "团队私有云存储"),
        (
            "branding_favicon_url",
            "https://cdn.example.com/branding/favicon.png?v=unicode",
        ),
        (
            "branding_wordmark_dark_url",
            "https://cdn.example.com/branding/深色.svg?v=unicode",
        ),
        (
            "branding_wordmark_light_url",
            "https://cdn.example.com/branding/浅色.svg?v=unicode",
        ),
    ] {
        let req = test::TestRequest::put()
            .uri(&format!("/api/v1/admin/config/{key}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({ "value": value }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200, "setting {key} should succeed");
    }

    let req = test::TestRequest::get()
        .uri("/api/v1/public/branding")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["title"], "猫猫云盘");
    assert_eq!(body["data"]["description"], "团队私有云存储");
    assert_eq!(
        body["data"]["favicon_url"],
        "https://cdn.example.com/branding/favicon.png?v=unicode"
    );
    assert_eq!(
        body["data"]["wordmark_dark_url"],
        "https://cdn.example.com/branding/深色.svg?v=unicode"
    );
    assert_eq!(
        body["data"]["wordmark_light_url"],
        "https://cdn.example.com/branding/浅色.svg?v=unicode"
    );
}

#[actix_web::test]
async fn test_frontend_shell_injects_branding_into_initial_html() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    for (key, value) in [
        ("branding_title", "猫猫云盘"),
        ("branding_description", "团队私有云存储"),
        (
            "branding_favicon_url",
            "https://cdn.example.com/branding/favicon.png?v=initial",
        ),
    ] {
        let req = test::TestRequest::put()
            .uri(&format!("/api/v1/admin/config/{key}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({ "value": value }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200, "setting {key} should succeed");
    }

    let req = test::TestRequest::get().uri("/login").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body = test::read_body(resp).await;
    let html = String::from_utf8_lossy(&body);

    assert!(html.contains("<title>猫猫云盘</title>"));
    assert!(html.contains("content=\"团队私有云存储\""));
    assert!(html.contains("href=\"https://cdn.example.com/branding/favicon.png?v=initial\""));
    assert!(!html.contains("<title>AsterDrive</title>"));
}

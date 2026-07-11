//! 集成测试：`branding`。

#[macro_use]
mod common;
use aster_drive::runtime::SharedRuntimeState;

use actix_web::test;
use serde_json::Value;

#[actix_web::test]
async fn test_public_frontend_config_returns_default_bootstrap_config() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let req = test::TestRequest::get()
        .uri("/api/v1/public/frontend-config")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("Cache-Control")
            .and_then(|value| value.to_str().ok()),
        Some("public, max-age=60")
    );
    assert_eq!(
        resp.headers()
            .get("Vary")
            .and_then(|value| value.to_str().ok()),
        Some("Authorization, Cookie")
    );

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["version"], 1);
    assert_eq!(body["data"]["branding"]["title"], "AsterDrive");
    assert_eq!(body["data"]["branding"]["allow_user_registration"], true);
    assert_eq!(body["data"]["branding"]["passkey_login_enabled"], true);
    assert_eq!(
        body["data"]["media"]["image_preview_preference"],
        "original_first"
    );
    assert!(body["data"].get("image_preview").is_none());
}

#[actix_web::test]
async fn test_public_frontend_config_uses_admin_updated_branding_and_preview_preference() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    for (key, value) in [
        (
            "public_site_url",
            serde_json::json!(["https://drive.example.com"]),
        ),
        ("auth_allow_user_registration", serde_json::json!("false")),
        ("auth_passkey_login_enabled", serde_json::json!("false")),
        ("branding_title", serde_json::json!("Nebula Drive")),
        (
            "frontend_image_preview_preference",
            serde_json::json!("preview_first"),
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
        .uri("/api/v1/public/frontend-config")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["branding"]["title"], "Nebula Drive");
    assert_eq!(
        body["data"]["branding"]["site_urls"],
        serde_json::json!(["https://drive.example.com"])
    );
    assert_eq!(body["data"]["branding"]["allow_user_registration"], false);
    assert_eq!(body["data"]["branding"]["passkey_login_enabled"], false);
    assert_eq!(
        body["data"]["media"]["image_preview_preference"],
        "preview_first"
    );
}

#[actix_web::test]
async fn test_admin_rejects_invalid_frontend_preview_preference() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let key = "frontend_image_preview_preference";
    let before = aster_drive::db::repository::config_repo::find_by_key(&db, key)
        .await
        .expect("config lookup should succeed")
        .expect("frontend preview preference config should exist");

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/admin/config/{key}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "value": "sideways" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert!(status.is_client_error(), "{body:#?}");

    let after = aster_drive::db::repository::config_repo::find_by_key(&db, key)
        .await
        .expect("config lookup should succeed")
        .expect("frontend preview preference config should exist");
    assert_eq!(after.value, before.value);
}

#[actix_web::test]
async fn test_public_frontend_config_falls_back_for_invalid_preview_preference() {
    let state = common::setup().await;
    state
        .runtime_config
        .apply(aster_forge_db::system_config::Model {
            id: 9_999,
            key: "frontend_image_preview_preference".to_string(),
            value: "sideways".to_string(),
            value_type: aster_drive::types::ConfigValueType::String,
            requires_restart: false,
            is_sensitive: false,
            source: aster_drive::types::ConfigSource::System,
            visibility: aster_drive::types::ConfigVisibility::Private,
            namespace: String::new(),
            category: String::new(),
            description: String::new(),
            updated_at: chrono::Utc::now(),
            updated_by: None,
        });
    let app = create_test_app!(state);

    let req = test::TestRequest::get()
        .uri("/api/v1/public/frontend-config")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["media"]["image_preview_preference"],
        "original_first"
    );
}

#[actix_web::test]
async fn test_public_frontend_config_uses_admin_updated_branding_values() {
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
        .uri("/api/v1/public/frontend-config")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    let branding = &body["data"]["branding"];
    assert_eq!(branding["title"], "Nebula Drive");
    assert_eq!(branding["description"], "Team storage for the squad");
    assert_eq!(
        branding["favicon_url"],
        "https://cdn.example.com/branding/favicon.png?v=2"
    );
    assert_eq!(
        branding["wordmark_dark_url"],
        "https://cdn.example.com/branding/wordmark-dark.svg?v=2"
    );
    assert_eq!(
        branding["wordmark_light_url"],
        "https://cdn.example.com/branding/wordmark-light.svg?v=2"
    );
    assert_eq!(
        branding["site_urls"],
        serde_json::json!(["https://drive.example.com", "https://panel.example.com"])
    );
    assert_eq!(branding["allow_user_registration"], false);
    assert_eq!(branding["passkey_login_enabled"], false);
}

#[actix_web::test]
async fn test_public_frontend_config_branding_blank_values_fall_back_to_defaults() {
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
        .uri("/api/v1/public/frontend-config")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    let branding = &body["data"]["branding"];
    assert_eq!(branding["title"], "AsterDrive");
    assert_eq!(branding["description"], "Self-hosted cloud storage");
    assert_eq!(branding["favicon_url"], "/favicon.svg");
    assert_eq!(
        branding["wordmark_dark_url"],
        "/static/asterdrive/asterdrive-dark.svg"
    );
    assert_eq!(
        branding["wordmark_light_url"],
        "/static/asterdrive/asterdrive-light.svg"
    );
}

#[actix_web::test]
async fn test_public_frontend_config_branding_preserves_non_ascii_text() {
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
        .uri("/api/v1/public/frontend-config")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    let branding = &body["data"]["branding"];
    assert_eq!(branding["title"], "猫猫云盘");
    assert_eq!(branding["description"], "团队私有云存储");
    assert_eq!(
        branding["favicon_url"],
        "https://cdn.example.com/branding/favicon.png?v=unicode"
    );
    assert_eq!(
        branding["wordmark_dark_url"],
        "https://cdn.example.com/branding/深色.svg?v=unicode"
    );
    assert_eq!(
        branding["wordmark_light_url"],
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

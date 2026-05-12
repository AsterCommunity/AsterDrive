//! 集成测试：`preferences`。

#[macro_use]
mod common;

use actix_web::test;
use aster_drive::db::repository::user_repo;
use aster_drive::types::StoredUserConfig;
use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};
use serde_json::Value;

// ── /me 不泄漏 password_hash 和 config ─────────────────────

#[actix_web::test]
async fn test_me_no_sensitive_fields() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;

    let user = &body["data"]["user"];
    assert!(
        user.get("password_hash").is_none(),
        "password_hash must not be serialized"
    );
    assert!(
        user.get("config").is_none(),
        "config blob must not be serialized"
    );
}

// ── 偏好设置：PATCH 合并 + GET 返回完整值 ───────────────────

#[actix_web::test]
async fn test_preferences_patch_and_get() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 初始状态：无偏好
    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert!(
        body["data"]["preferences"].is_null(),
        "new user should have no preferences"
    );

    // PATCH 设置 theme_mode
    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/preferences")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "theme_mode": "dark",
            "browser_open_mode": "double_click",
            "display_time_zone": "UTC",
            "storage_event_stream_enabled": false
        }))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(body["data"]["theme_mode"], "dark");
    assert_eq!(body["data"]["browser_open_mode"], "double_click");
    assert_eq!(body["data"]["display_time_zone"], "UTC");
    assert_eq!(body["data"]["storage_event_stream_enabled"], false);

    // 再 PATCH 设置 language（合并，不覆盖之前的）
    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/preferences")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "language": "zh" }))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(
        body["data"]["theme_mode"], "dark",
        "existing pref preserved"
    );
    assert_eq!(body["data"]["browser_open_mode"], "double_click");
    assert_eq!(body["data"]["language"], "zh");
    assert_eq!(body["data"]["display_time_zone"], "UTC");
    assert_eq!(body["data"]["storage_event_stream_enabled"], false);

    // /me 也返回完整偏好
    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(body["data"]["preferences"]["theme_mode"], "dark");
    assert_eq!(
        body["data"]["preferences"]["browser_open_mode"],
        "double_click"
    );
    assert_eq!(body["data"]["preferences"]["language"], "zh");
    assert_eq!(body["data"]["preferences"]["display_time_zone"], "UTC");
    assert_eq!(
        body["data"]["preferences"]["storage_event_stream_enabled"],
        false
    );
}

// ── 偏好设置：空 PATCH 不改现有值 ──────────────────────────

#[actix_web::test]
async fn test_preferences_empty_patch_noop() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 先设一个值
    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/preferences")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "color_preset": "#0f766e" }))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(body["data"]["color_preset"], "#0f766e");
    assert!(body["data"]["display_time_zone"].is_null());
    assert!(body["data"]["storage_event_stream_enabled"].is_null());

    // 空 PATCH（全 None）
    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/preferences")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({}))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(
        body["data"]["color_preset"], "#0f766e",
        "empty patch preserves existing"
    );
    assert!(body["data"]["display_time_zone"].is_null());
    assert!(body["data"]["storage_event_stream_enabled"].is_null());
}

// ── 偏好设置：PATCH 内置字段时保留自定义 config key ──────────────

#[actix_web::test]
async fn test_preferences_patch_preserves_custom_user_config_keys() {
    let state = common::setup().await;
    let db = state.db.clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let user = user_repo::find_by_email(&db, "test@example.com")
        .await
        .unwrap()
        .expect("registered user should exist");
    let mut active = user.clone().into_active_model();
    active.config = Set(Some(StoredUserConfig(
        r#"{"theme_mode":"light","custom_ui":"nebula","sidebar":{"collapsed":true}}"#.to_string(),
    )));
    active.updated_at = Set(chrono::Utc::now());
    active.update(&db).await.unwrap();

    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/preferences")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "language": "zh" }))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(body["data"]["theme_mode"], "light");
    assert_eq!(body["data"]["language"], "zh");
    assert_eq!(body["data"]["custom"]["custom_ui"], "nebula");
    assert_eq!(body["data"]["custom"]["sidebar"]["collapsed"], true);

    let updated = user_repo::find_by_id(&db, user.id).await.unwrap();
    let stored = updated.config.expect("config should still be stored");
    let json: Value = serde_json::from_str(stored.as_ref()).unwrap();
    assert_eq!(json["theme_mode"], "light");
    assert_eq!(json["language"], "zh");
    assert_eq!(json["custom_ui"], "nebula");
    assert_eq!(json["sidebar"]["collapsed"], true);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(body["data"]["preferences"]["custom"]["custom_ui"], "nebula");
    assert_eq!(
        body["data"]["preferences"]["custom"]["sidebar"]["collapsed"],
        true
    );
}

// ── 偏好设置：自定义 KV 可读写删除 ─────────────────────────────

#[actix_web::test]
async fn test_preferences_custom_kv_roundtrip_and_delete() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/preferences")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "custom": {
                "my-frontend.sidebar": { "collapsed": true },
                "my-frontend.accent": "sunset"
            }
        }))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(
        body["data"]["custom"]["my-frontend.sidebar"]["collapsed"],
        true
    );
    assert_eq!(body["data"]["custom"]["my-frontend.accent"], "sunset");

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(
        body["data"]["preferences"]["custom"]["my-frontend.sidebar"]["collapsed"],
        true
    );
    assert_eq!(
        body["data"]["preferences"]["custom"]["my-frontend.accent"],
        "sunset"
    );

    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/preferences")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "remove_custom_keys": ["my-frontend.sidebar", "my-frontend.accent"]
        }))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert!(body["data"]["custom"].is_null());

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert!(
        body["data"]["preferences"].is_null(),
        "removing the last custom preference should clear /auth/me preferences"
    );
}

#[actix_web::test]
async fn test_preferences_custom_kv_rejects_reserved_key() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/preferences")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "custom": {
                "theme_mode": "dark"
            }
        }))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        400,
        "custom preference keys must not shadow built-in preferences"
    );
}

// ── 偏好设置：非法值被拒绝 ────────────────────────────────

#[actix_web::test]
async fn test_preferences_invalid_value_rejected() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/preferences")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "theme_mode": "neon" }))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400, "invalid enum value should be rejected");
}

#[actix_web::test]
async fn test_preferences_color_preset_accepts_hex_color() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/preferences")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "color_preset": "#0F766E" }))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["color_preset"], "#0f766e");

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(
        body["data"]["preferences"]["color_preset"],
        "#0f766e"
    );
}

#[actix_web::test]
async fn test_preferences_color_preset_rejects_invalid_color() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    for color in ["greenish", "#12345", "#1234567", "rgb(1, 2, 3)"] {
        let req = test::TestRequest::patch()
            .uri("/api/v1/auth/preferences")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({ "color_preset": color }))
            .to_request();
        let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400, "{color} should be rejected");
    }
}

#[actix_web::test]
async fn test_preferences_color_preset_normalizes_legacy_names() {
    let state = common::setup().await;
    let db = state.db.clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let user = user_repo::find_by_email(&db, "test@example.com")
        .await
        .unwrap()
        .expect("registered user should exist");
    let mut active = user.clone().into_active_model();
    active.config = Set(Some(StoredUserConfig(
        r#"{"color_preset":"green"}"#.to_string(),
    )));
    active.updated_at = Set(chrono::Utc::now());
    active.update(&db).await.unwrap();

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(
        body["data"]["preferences"]["color_preset"],
        "#16a34a"
    );
}

#[actix_web::test]
async fn test_preferences_invalid_display_time_zone_rejected() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/preferences")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "display_time_zone": "Mars/Olympus_Mons" }))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        400,
        "invalid display_time_zone should be rejected"
    );
}

// ── 未认证访问偏好设置被拒 ────────────────────────────────

#[actix_web::test]
async fn test_preferences_unauthenticated() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/preferences")
        .set_json(serde_json::json!({ "theme_mode": "dark" }))
        .to_request();
    // 未认证请求由 JwtAuth 中间件拦截，返回 401
    let result = test::try_call_service(&app, req).await;
    assert!(
        result.is_err(),
        "unauthenticated request should be rejected"
    );
}

//! 集成测试：`public_custom_config`。

#[macro_use]
mod common;

use actix_web::test;
use serde_json::{Value, json};

async fn set_custom_config(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    token: &str,
    key: &str,
    value: &str,
    visibility: &str,
) -> Value {
    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/admin/config/{key}"))
        .insert_header(("Cookie", common::access_cookie_header(token)))
        .insert_header(common::csrf_header_for(token))
        .set_json(json!({
            "value": value,
            "visibility": visibility,
        }))
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 200, "setting {key} should succeed");
    test::read_body_json(resp).await
}

async fn get_public_custom_config(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    token: Option<&str>,
) -> Value {
    let mut req = test::TestRequest::get().uri("/api/v1/public/custom-config");
    if let Some(token) = token {
        req = req.insert_header(("Authorization", format!("Bearer {token}")));
    }
    let resp = test::call_service(app, req.to_request()).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("Cache-Control")
            .and_then(|value| value.to_str().ok()),
        if token.is_some() {
            Some("private, max-age=60")
        } else {
            Some("public, max-age=60")
        }
    );
    test::read_body_json(resp).await
}

#[actix_web::test]
async fn test_public_custom_config_filters_by_visibility_and_authentication() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let public_body =
        set_custom_config(&app, &token, "custom.public_theme", "nebula", "public").await;
    assert_eq!(public_body["data"]["visibility"], "public");

    let authenticated_body = set_custom_config(
        &app,
        &token,
        "custom.authenticated_flag",
        "enabled",
        "authenticated",
    )
    .await;
    assert_eq!(authenticated_body["data"]["visibility"], "authenticated");

    let private_body =
        set_custom_config(&app, &token, "custom.private_secret", "hidden", "private").await;
    assert_eq!(private_body["data"]["visibility"], "private");

    let anonymous_body = get_public_custom_config(&app, None).await;
    assert_eq!(
        anonymous_body["data"]["entries"]["custom.public_theme"],
        "nebula"
    );
    assert!(
        anonymous_body["data"]["entries"]
            .get("custom.authenticated_flag")
            .is_none()
    );
    assert!(
        anonymous_body["data"]["entries"]
            .get("custom.private_secret")
            .is_none()
    );

    let authenticated_body = get_public_custom_config(&app, Some(&token)).await;
    assert_eq!(
        authenticated_body["data"]["entries"]["custom.public_theme"],
        "nebula"
    );
    assert_eq!(
        authenticated_body["data"]["entries"]["custom.authenticated_flag"],
        "enabled"
    );
    assert!(
        authenticated_body["data"]["entries"]
            .get("custom.private_secret")
            .is_none()
    );
}

#[actix_web::test]
async fn test_custom_config_defaults_to_private_until_visibility_is_changed() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/custom.default_private")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(json!({ "value": "draft" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let saved_body: Value = test::read_body_json(resp).await;
    assert_eq!(saved_body["data"]["visibility"], "private");

    let anonymous_body = get_public_custom_config(&app, None).await;
    assert!(
        anonymous_body["data"]["entries"]
            .get("custom.default_private")
            .is_none()
    );

    let authenticated_body = get_public_custom_config(&app, Some(&token)).await;
    assert!(
        authenticated_body["data"]["entries"]
            .get("custom.default_private")
            .is_none()
    );
}

#[actix_web::test]
async fn test_public_custom_config_reflects_visibility_update_and_delete() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    set_custom_config(
        &app,
        &token,
        "custom.mutable_visibility",
        "public-value",
        "public",
    )
    .await;
    let anonymous_body = get_public_custom_config(&app, None).await;
    assert_eq!(
        anonymous_body["data"]["entries"]["custom.mutable_visibility"],
        "public-value"
    );

    set_custom_config(
        &app,
        &token,
        "custom.mutable_visibility",
        "auth-value",
        "authenticated",
    )
    .await;
    let anonymous_body = get_public_custom_config(&app, None).await;
    assert!(
        anonymous_body["data"]["entries"]
            .get("custom.mutable_visibility")
            .is_none()
    );
    let authenticated_body = get_public_custom_config(&app, Some(&token)).await;
    assert_eq!(
        authenticated_body["data"]["entries"]["custom.mutable_visibility"],
        "auth-value"
    );

    set_custom_config(
        &app,
        &token,
        "custom.mutable_visibility",
        "private-value",
        "private",
    )
    .await;
    let authenticated_body = get_public_custom_config(&app, Some(&token)).await;
    assert!(
        authenticated_body["data"]["entries"]
            .get("custom.mutable_visibility")
            .is_none()
    );

    set_custom_config(
        &app,
        &token,
        "custom.mutable_visibility",
        "delete-me",
        "public",
    )
    .await;
    let req = test::TestRequest::delete()
        .uri("/api/v1/admin/config/custom.mutable_visibility")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let anonymous_body = get_public_custom_config(&app, None).await;
    assert!(
        anonymous_body["data"]["entries"]
            .get("custom.mutable_visibility")
            .is_none()
    );
}

#[actix_web::test]
async fn test_public_custom_config_authentication_uses_cookie_before_bearer() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    set_custom_config(
        &app,
        &token,
        "custom.cookie_authenticated",
        "cookie-ok",
        "authenticated",
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/public/custom-config")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(("Authorization", "Bearer fake.token.here"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["entries"]["custom.cookie_authenticated"],
        "cookie-ok"
    );
}

#[actix_web::test]
async fn test_public_custom_config_rejects_invalid_present_token() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let req = test::TestRequest::get()
        .uri("/api/v1/public/custom-config")
        .insert_header(("Authorization", "Bearer fake.token.here"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn test_system_config_rejects_visibility_update() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/branding_title")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(json!({
            "value": "AsterDrive",
            "visibility": "public",
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

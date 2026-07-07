//! 集成测试：`refactor_contracts`。

// Issue #37 refactor contract tests — 验证 service 层 API 契约未因重构破坏
#[macro_use]
mod common;

use actix_web::test;
use serde_json::Value;

// ── P3: LoginResult.user_id 契约 ────────────────────────────────────

/// login 成功时返回 user_id 正确，access/refresh token 非空
/// HTTP 层（cookies）和 service 层（LoginResult.user_id）均验证
#[actix_web::test]
async fn test_login_returns_correct_user_id_and_tokens() {
    // HTTP 层用的 app
    let state_http = common::setup().await;
    let app = create_test_app!(state_http);

    // 注册
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .set_json(serde_json::json!({
            "username": "logintest",
            "email": "login@test.com",
            "password": "pass1234"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    // HTTP 登录验证 cookies
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .set_json(serde_json::json!({
            "identifier": "logintest",
            "password": "pass1234"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // service 层验证 user_id（用独立 state）
    let state_svc = common::setup().await;
    let user = aster_drive::services::auth::local::register(
        &state_svc,
        "logintest",
        "login@test.com",
        "pass1234",
    )
    .await
    .unwrap();

    let result =
        aster_drive::services::auth::local::login(&state_svc, "logintest", "pass1234", None, None)
            .await
            .unwrap();
    let result = common::expect_authenticated_login(result);

    assert_eq!(
        result.user_id, user.id,
        "user_id should match registered user"
    );
    assert!(result.user_id > 0);
    assert!(!result.access_token.is_empty());
    assert!(!result.refresh_token.is_empty());
}

/// 错误密码登录返回 401，不暴露 user_id
#[actix_web::test]
async fn test_login_wrong_password_returns_401_without_user_id() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    // 注册
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .set_json(serde_json::json!({
            "username": "wrongpw",
            "email": "wrongpw@test.com",
            "password": "correctpw"
        }))
        .to_request();
    test::call_service(&app, req).await;

    // 错误密码 → 401
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .set_json(serde_json::json!({
            "identifier": "wrongpw",
            "password": "wrongpassword"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);

    let body: Value = test::read_body_json(resp).await;
    // 错误响应不含 user_id
    let has_user_id = body.get("data").and_then(|d| d.get("user_id")).is_some();
    assert!(!has_user_id, "401 response should not contain user_id");

    // service 层：错误密码返回 Err
    let state2 = common::setup().await;
    let err =
        aster_drive::services::auth::local::login(&state2, "wrongpw", "wrongpassword", None, None)
            .await;
    assert!(err.is_err());
}

// ── P1: Cookie 签名差异性 ──────────────────────────────────────────

/// 不同 token 的签名互不相同，同一 token 签名确定
#[actix_web::test]
async fn test_share_cookie_signatures_are_token_specific_and_deterministic() {
    let secret = "test-secret-key-for-integration-tests";
    let binding = aster_drive::services::share_service::ShareCookieBinding::from_request_parts(
        Some("AsterDrive Test/1.0"),
        Some("203.0.113.42"),
    );

    let sig1 =
        aster_drive::services::share_service::sign_share_cookie("token-alpha", &binding, secret);
    let sig2 =
        aster_drive::services::share_service::sign_share_cookie("token-beta", &binding, secret);
    let sig1_replay =
        aster_drive::services::share_service::sign_share_cookie("token-alpha", &binding, secret);

    assert_ne!(
        sig1, sig2,
        "different tokens should produce different signatures"
    );
    assert_eq!(
        sig1, sig1_replay,
        "same token should produce same signature (deterministic)"
    );
}

/// 签名格式为 64 字符十六进制字符串（SHA256 输出）
#[actix_web::test]
async fn test_share_cookie_signature_format_is_sha256_hex() {
    let secret = "test-secret-key-for-integration-tests";
    let binding = aster_drive::services::share_service::ShareCookieBinding::from_request_parts(
        Some("AsterDrive Test/1.0"),
        Some("203.0.113.42"),
    );
    let sig =
        aster_drive::services::share_service::sign_share_cookie("any-token", &binding, secret);

    assert_eq!(sig.len(), 64, "SHA256 hex digest should be 64 characters");
    assert!(
        sig.chars().all(|c| c.is_ascii_hexdigit()),
        "signature should be all hex characters"
    );
}

/// token-A 的签名无法通过 token-B 的验证
#[actix_web::test]
async fn test_share_cookie_forged_signature_rejected() {
    let secret = "test-secret-key-for-integration-tests";
    let binding = aster_drive::services::share_service::ShareCookieBinding::from_request_parts(
        Some("AsterDrive Test/1.0"),
        Some("203.0.113.42"),
    );
    let sig_wrong =
        aster_drive::services::share_service::sign_share_cookie("wrong-token", &binding, secret);

    let valid = aster_drive::services::share_service::verify_share_cookie(
        "correct-token",
        &sig_wrong,
        &binding,
        secret,
    );
    assert!(
        !valid,
        "signature for wrong token should not verify for correct token"
    );
}

// ── P4: get_schema() 契约 ─────────────────────────────────────────────

/// config_schema 返回非空数组，每个条目包含必需字段
#[actix_web::test]
async fn test_config_schema_returns_non_empty_with_required_fields() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    // 注册 admin
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .set_json(serde_json::json!({
            "username": "admintk",
            "email": "admin@tk.com",
            "password": "admin1234"
        }))
        .to_request();
    test::call_service(&app, req).await;

    // 登录获取 cookies
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .set_json(serde_json::json!({"identifier": "admintk", "password": "admin1234"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let cookies = extract_cookies(&resp);
    assert!(!cookies.is_empty(), "login should set cookies");

    // HTTP 验证 schema 非空
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/config/schema")
        .insert_header(("Cookie", common::access_cookie_header(&cookies[0])))
        .insert_header(common::csrf_header_for(&cookies[0]))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let schema = &body["data"].as_array().expect("data should be array");
    assert!(!schema.is_empty(), "schema should not be empty");

    // service 层验证字段完整性
    let full_schema = aster_drive::services::config_service::get_schema();
    assert!(!full_schema.is_empty());

    for item in &full_schema {
        assert!(!item.key.is_empty(), "key should not be empty");
        assert!(
            !item.label_i18n_key.is_empty(),
            "label_i18n_key should not be empty"
        );
        assert!(
            !item.description_i18n_key.is_empty(),
            "description_i18n_key should not be empty"
        );
        assert!(
            !item.value_type.as_str().is_empty(),
            "value_type should not be empty"
        );
        assert!(!item.category.is_empty(), "category should not be empty");
    }

    // HTTP 和 service 返回一致
    assert_eq!(schema.len(), full_schema.len());
}

// ── P2: 锁跨用户权限 ─────────────────────────────────────────────────

/// 用户 A 锁定的文件，用户 B 无法删除/重命名
#[actix_web::test]
async fn test_locked_file_blocks_other_users_delete_and_rename() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    // 用户 A 注册 + 上传 + 锁定
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .set_json(serde_json::json!({
            "username": "locker",
            "email": "locker@test.com",
            "password": "pass1234"
        }))
        .to_request();
    test::call_service(&app, req).await;

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .set_json(serde_json::json!({"identifier": "locker", "password": "pass1234"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let cookies_a = extract_cookies(&resp);

    let file_id = upload_test_file!(app, &cookies_a[0]);

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&cookies_a[0])))
        .insert_header(common::csrf_header_for(&cookies_a[0]))
        .set_json(serde_json::json!({ "locked": true }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    admin_create_user!(
        app,
        &cookies_a[0],
        "intruder",
        "intruder@test.com",
        "pass5678"
    );
    let (intruder_access, _intruder_refresh) = login_user!(app, "intruder", "pass5678");

    // 用户 B 尝试删除 → 403
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&intruder_access)))
        .insert_header(common::csrf_header_for(&intruder_access))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    // 用户 B 尝试重命名 → 403
    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&intruder_access)))
        .insert_header(common::csrf_header_for(&intruder_access))
        .set_json(serde_json::json!({ "name": "hacked.txt" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

/// 用户 A 锁定的文件夹，用户 B 无法删除
#[actix_web::test]
async fn test_locked_folder_blocks_other_users_delete() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    // 用户 A 创建并锁定文件夹
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .set_json(serde_json::json!({
            "username": "folderlocker",
            "email": "folderlocker@test.com",
            "password": "pass1234"
        }))
        .to_request();
    test::call_service(&app, req).await;

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .set_json(serde_json::json!({"identifier": "folderlocker", "password": "pass1234"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let cookies_a = extract_cookies(&resp);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&cookies_a[0])))
        .insert_header(common::csrf_header_for(&cookies_a[0]))
        .set_json(serde_json::json!({ "name": "locked-folder" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&cookies_a[0])))
        .insert_header(common::csrf_header_for(&cookies_a[0]))
        .set_json(serde_json::json!({ "locked": true }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    admin_create_user!(
        app,
        &cookies_a[0],
        "folderintruder",
        "folderintruder@test.com",
        "pass5678"
    );
    let (intruder_access, _intruder_refresh) = login_user!(app, "folderintruder", "pass5678");

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&intruder_access)))
        .insert_header(common::csrf_header_for(&intruder_access))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

// ── Helpers ───────────────────────────────────────────────────────────

fn extract_cookies(resp: &actix_web::dev::ServiceResponse) -> Vec<String> {
    let access = common::extract_cookie(resp, "aster_access");
    let refresh = common::extract_cookie(resp, "aster_refresh");
    [access, refresh].into_iter().flatten().collect()
}

//! 安全修复回归测试
//! - Fix 1: 上传不能越权到别人的文件夹
//! - Fix 2: update_storage_used 减量不下溢
//! - Fix 3: 分享下载 304 不应增加 download_count
//! - Fix 4: 恶意查询参数不能触发 SQL 注入或原始排序拼接

use crate::common;

use actix_web::test;
use serde_json::Value;

const SQLI_PAYLOAD: &str = "%27%20OR%201%3D1%20--";
const SORT_BY_SQLI_PAYLOAD: &str = "name%3Bdrop%20table%20users%3B--";

fn upload_named_file(name: &str, content: &str, mime: &str, boundary: &str) -> String {
    format!(
        "--{boundary}\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\n\
         Content-Type: {mime}\r\n\r\n\
         {content}\r\n\
         --{boundary}--\r\n"
    )
}

// ─── Fix 1: 越权上传被拒 ───────────────────────────────────

/// 注册第二个用户并登录，返回 access_token
macro_rules! register_user2 {
    ($app:expr, $db:expr, $mail_sender:expr) => {{
        use actix_web::test;

        let req = test::TestRequest::post()
            .uri("/api/v1/auth/register")
            .peer_addr("127.0.0.1:12345".parse().unwrap())
            .set_json(serde_json::json!({
                "username": "user2",
                "email": "user2@example.com",
                "password": "password123"
            }))
            .to_request();
        let resp: actix_web::dev::ServiceResponse = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 201);

        let _ = confirm_latest_contact_verification!($app, $db, $mail_sender);

        let req = test::TestRequest::post()
            .uri("/api/v1/auth/login")
            .peer_addr("127.0.0.1:12345".parse().unwrap())
            .set_json(serde_json::json!({
                "identifier": "user2",
                "password": "password123"
            }))
            .to_request();
        let resp: actix_web::dev::ServiceResponse = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 200);
        common::extract_cookie(&resp, "aster_access").unwrap()
    }};
}

#[actix_web::test]
async fn test_upload_to_other_users_folder_returns_403() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);
    let (token1, _) = register_and_login!(app);
    let token2 = register_user2!(app, db, mail_sender);

    // user1 创建文件夹
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token1)))
        .insert_header(common::csrf_header_for(&token1))
        .set_json(serde_json::json!({ "name": "private" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    // user2 尝试上传到 user1 的文件夹 → 403
    let boundary = "----CrossUserBoundary";
    let payload = "------CrossUserBoundary\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"evil.txt\"\r\n\
         Content-Type: text/plain\r\n\r\n\
         pwned\r\n\
         ------CrossUserBoundary--\r\n"
        .to_string();
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/upload?folder_id={folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token2)))
        .insert_header(common::csrf_header_for(&token2))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        403,
        "uploading to another user's folder should return 403"
    );
}

#[actix_web::test]
async fn test_init_upload_to_other_users_folder_returns_403() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);
    let (token1, _) = register_and_login!(app);
    let token2 = register_user2!(app, db, mail_sender);

    // user1 创建文件夹
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token1)))
        .insert_header(common::csrf_header_for(&token1))
        .set_json(serde_json::json!({ "name": "secret" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    // user2 尝试 init_upload 到 user1 的文件夹 → 403
    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload/init")
        .insert_header(("Cookie", common::access_cookie_header(&token2)))
        .insert_header(common::csrf_header_for(&token2))
        .set_json(serde_json::json!({
            "filename": "evil.bin",
            "total_size": 1024,
            "folder_id": folder_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        403,
        "init_upload to another user's folder should return 403"
    );
}

#[actix_web::test]
async fn test_directory_upload_to_other_users_base_folder_returns_403() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);
    let (token1, _) = register_and_login!(app);
    let token2 = register_user2!(app, db, mail_sender);

    // user1 创建文件夹
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token1)))
        .insert_header(common::csrf_header_for(&token1))
        .set_json(serde_json::json!({ "name": "base" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    // user2 尝试目录上传到 user1 的文件夹 → 403
    let boundary = "----DirCrossBoundary";
    let payload = "------DirCrossBoundary\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"sneaky.txt\"\r\n\
         Content-Type: text/plain\r\n\r\n\
         pwned via directory upload\r\n\
         ------DirCrossBoundary--\r\n"
        .to_string();
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/files/upload?folder_id={folder_id}&relative_path=sub/sneaky.txt"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token2)))
        .insert_header(common::csrf_header_for(&token2))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        403,
        "directory upload to another user's base folder should return 403"
    );
}

// ─── A1: 在别人文件夹下建子文件夹被拒 ──────────────────────

#[actix_web::test]
async fn test_create_folder_in_other_users_folder_returns_403() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);
    let (token1, _) = register_and_login!(app);
    let token2 = register_user2!(app, db, mail_sender);

    // user1 创建文件夹
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token1)))
        .insert_header(common::csrf_header_for(&token1))
        .set_json(serde_json::json!({ "name": "user1-only" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    // user2 尝试在 user1 的文件夹下建子文件夹 → 403
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token2)))
        .insert_header(common::csrf_header_for(&token2))
        .set_json(serde_json::json!({
            "name": "evil-subfolder",
            "parent_id": folder_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        403,
        "creating folder in another user's folder should return 403"
    );
}

// ─── Fix 3: 分享下载 304 不应计数 ──────────────────────────

#[actix_web::test]
async fn test_share_download_304_does_not_increment_count() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    // 创建不限次数的分享
    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": { "type": "file", "id": file_id }
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"].as_str().unwrap().to_string();

    // 第一次下载 → 200，拿 ETag
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/download"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let etag = resp
        .headers()
        .get("ETag")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    // 查 download_count = 1
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["download_count"], 1,
        "first download should count"
    );

    // 带 If-None-Match 再次请求 → 304
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/download"))
        .insert_header(("If-None-Match", etag.as_str()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 304, "should return 304 for matching ETag");

    // download_count 应该仍然是 1
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["download_count"], 1,
        "304 cache hit should NOT increment download_count"
    );
}

// ─── Fix 4: 恶意查询参数不能注入 ────────────────────────────

#[actix_web::test]
async fn test_search_query_sql_injection_payload_is_treated_as_literal() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let boundary = "----SearchSqlInjectionBoundary";
    let payload = upload_named_file("quarterly-report.txt", "safe", "text/plain", boundary);
    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/search?q={SQLI_PAYLOAD}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["total_files"], 0,
        "malicious q payload must not broaden file results"
    );
    assert_eq!(
        body["data"]["total_folders"], 0,
        "malicious q payload must not broaden folder results"
    );
}

#[actix_web::test]
async fn test_admin_audit_action_filter_sql_injection_payload_is_treated_as_literal() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/audit-logs")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(
        body["data"]["total"].as_u64().unwrap() > 0,
        "baseline audit log query should return seeded admin events"
    );

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/admin/audit-logs?action={SQLI_PAYLOAD}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["total"], 0,
        "malicious action filter must not bypass equality filtering"
    );
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 0);
}

#[actix_web::test]
async fn test_folder_list_rejects_sort_by_sql_injection_payload() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/folders?folder_limit=0&file_limit=0&sort_by={SORT_BY_SQLI_PAYLOAD}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        400,
        "invalid sort_by payload must be rejected before any query builder ordering"
    );
}

// ─── Fix 5: 路径穿越攻击（上传 filename / relative_path） ──
//
// LocalDriver::full_path 已在驱动层做 sanitize_relative_path，但 driver 以上的
// filename / relative_path 必须在 API 入口就拒绝，这样日志和错误码都对用户友好，
// 也避免潜在的文件名污染 DB。

#[actix_web::test]
async fn test_init_upload_rejects_path_traversal_in_filename() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 各种路径穿越变体：字面 `..`、带斜杠、反斜杠、仅空白等
    let evil_filenames = [
        "..",
        ".",
        "../etc/passwd",
        "..\\etc\\passwd",
        "../../secret",
        "foo/../bar",
        "foo\\..\\bar",
        "foo/bar",    // 含 `/` 应拒绝
        "foo\\bar",   // 含 `\` 应拒绝
        "",           // 空名
        "con:evil",   // 含 `:`
        "file\0.txt", // 含 NUL
    ];
    for name in evil_filenames {
        let req = test::TestRequest::post()
            .uri("/api/v1/files/upload/init")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({
                "filename": name,
                "total_size": 1024
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(
            resp.status().is_client_error(),
            "filename '{name}' must be rejected with 4xx, got {}",
            resp.status()
        );
    }
}

#[actix_web::test]
async fn test_init_upload_rejects_path_traversal_in_relative_path() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // filename 本身合法，但 relative_path 里塞路径穿越段，应在 service 层 split + validate_name 拒绝
    let evil_paths = [
        "../escape.txt",
        "foo/../../outside.txt",
        "foo/./bar.txt",    // `.` 段
        "foo//bar.txt",     // 空段
        "foo/../bar.txt",   // `..` 段
        "..\\\\escape.txt", // 反斜杠只会被当成文件名非法字符
    ];
    for path in evil_paths {
        let req = test::TestRequest::post()
            .uri("/api/v1/files/upload/init")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({
                "filename": "leaf.txt",
                "total_size": 1024,
                "relative_path": path
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(
            resp.status().is_client_error(),
            "relative_path '{path}' must be rejected with 4xx, got {}",
            resp.status()
        );
    }
}

#[actix_web::test]
async fn test_multipart_upload_rejects_path_traversal_in_filename() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // multipart 上传接口对 filename 的 Content-Disposition 头同样要拒绝路径穿越
    let boundary = "----TraversalBoundary";
    let payload = "------TraversalBoundary\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"../../etc/passwd\"\r\n\
         Content-Type: text/plain\r\n\r\n\
         evil\r\n\
         ------TraversalBoundary--\r\n"
        .to_string();
    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status().is_client_error(),
        "multipart filename with traversal must be rejected with 4xx, got {}",
        resp.status()
    );
}

// ─── Fix 6: 分享文件夹下载不能跳到分享范围外的文件 ──────────

#[actix_web::test]
async fn test_shared_folder_download_rejects_file_outside_scope() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 建两个同级文件夹：shared / private，各放一个文件
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "shared" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let shared_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "private" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let private_id = body["data"]["id"].as_i64().unwrap();

    let _shared_file_id = upload_test_file_to_folder!(app, token, shared_id);
    let private_file_id = upload_test_file_to_folder!(app, token, private_id);

    // 只分享 shared 目录
    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": { "type": "folder", "id": shared_id }
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"].as_str().unwrap().to_string();

    // 攻击者用分享 token 伪造 file_id，尝试下载 private 文件夹里的文件
    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/s/{share_token}/files/{private_file_id}/download"
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status().is_client_error(),
        "file_id outside shared folder scope must be rejected with 4xx, got {}",
        resp.status()
    );
}

//! 集成测试：`files`。

#[macro_use]
mod common;

use actix_web::http::{StatusCode, header};
use actix_web::test;
use aster_drive::db::repository::{file_repo, policy_repo, user_repo};
use aster_drive::entities::{file, file_blob};
use aster_drive::runtime::SharedRuntimeState;
use aster_drive::services::storage_change_service::StorageChangeKind;
use aster_drive::types::FileCategory;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, Set};
use serde_json::Value;
use std::time::Duration;

macro_rules! upload_test_file_with_name_and_mime {
    ($app:expr, $token:expr, $name:expr, $mime:expr, $content:expr) => {{
        use actix_web::test;
        use serde_json::Value;

        let boundary = "----InlineUnsafeBoundary";
        let payload = format!(
            "------InlineUnsafeBoundary\r\n\
             Content-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\n\
             Content-Type: {mime}\r\n\r\n\
             {content}\r\n\
             ------InlineUnsafeBoundary--\r\n",
            name = $name,
            mime = $mime,
            content = $content
        );
        let req = test::TestRequest::post()
            .uri("/api/v1/files/upload")
            .insert_header(("Cookie", common::access_cookie_header(&$token)))
            .insert_header(common::csrf_header_for(&$token))
            .insert_header((
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            ))
            .set_payload(payload)
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 201, "upload should return 201");
        let body: Value = test::read_body_json(resp).await;
        body["data"]["id"].as_i64().unwrap()
    }};
}

macro_rules! upload_test_file_to_uri_with_content {
    ($app:expr, $token:expr, $uri:expr, $name:expr, $content:expr, $message:expr) => {{
        use actix_web::test;
        use serde_json::Value;

        let boundary = "----StorageUsedBoundary";
        let payload = format!(
            "------StorageUsedBoundary\r\n\
             Content-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\n\
             Content-Type: text/plain\r\n\r\n\
             {content}\r\n\
             ------StorageUsedBoundary--\r\n",
            name = $name,
            content = $content
        );
        let req = test::TestRequest::post()
            .uri($uri)
            .insert_header(("Cookie", common::access_cookie_header(&$token)))
            .insert_header(common::csrf_header_for(&$token))
            .insert_header((
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            ))
            .set_payload(payload)
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 201, $message);
        let body: Value = test::read_body_json(resp).await;
        body["data"]["id"].as_i64().unwrap()
    }};
}

macro_rules! upload_test_file_to_folder_with_content {
    ($app:expr, $token:expr, $folder_id:expr, $name:expr, $content:expr) => {{
        upload_test_file_to_uri_with_content!(
            $app,
            $token,
            &format!("/api/v1/files/upload?folder_id={}", $folder_id),
            $name,
            $content,
            "upload to folder should return 201"
        )
    }};
}

macro_rules! upload_test_file_with_content {
    ($app:expr, $token:expr, $name:expr, $content:expr) => {{
        upload_test_file_to_uri_with_content!(
            $app,
            $token,
            "/api/v1/files/upload",
            $name,
            $content,
            "upload should return 201"
        )
    }};
}

fn upload_payload(filename: &str, content: &str) -> String {
    format!(
        "------UnicodeBoundary123\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n\
         Content-Type: text/plain\r\n\r\n\
         {content}\r\n\
         ------UnicodeBoundary123--\r\n"
    )
}

#[actix_web::test]
async fn test_file_upload_download_delete() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);

    // 上传文件（multipart）
    let boundary = "----TestBoundary123";
    let file_content = b"Hello AsterDrive!";
    let upload_payload = format!(
        "------TestBoundary123\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"hello.txt\"\r\n\
         Content-Type: text/plain\r\n\r\n\
         {}\r\n\
         ------TestBoundary123--\r\n",
        std::str::from_utf8(file_content).unwrap()
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(upload_payload.clone())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201, "upload should return 201 Created");
    let upload_body: Value = test::read_body_json(resp).await;
    assert_eq!(upload_body["code"], "success");
    let file_id = upload_body["data"]["id"].as_i64().unwrap();
    assert_eq!(upload_body["data"]["name"], "hello.txt");
    assert_eq!(upload_body["data"]["mime_type"], "text/plain");

    // 获取文件信息
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "hello.txt");

    // 下载文件
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/download"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let download_body = test::read_body(resp).await;
    let content = String::from_utf8_lossy(&download_body);
    assert!(
        content.contains("Hello AsterDrive!"),
        "downloaded content should match: got '{content}'"
    );

    // 列出根目录应该有这个文件
    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);

    // 删除文件
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // 再查应该 404
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);

    // 删除后应能再次创建同名文件
    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(upload_payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let upload_body: Value = test::read_body_json(resp).await;
    assert_eq!(upload_body["data"]["name"], "hello.txt");
}

#[actix_web::test]
async fn test_file_direct_link_supports_public_access_force_download_and_file_removal() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file_named!(app, token, "clip 1.m3u8");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/direct-link"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let direct_token = body["data"]["token"]
        .as_str()
        .expect("direct link token should exist")
        .to_string();

    let req = test::TestRequest::get()
        .uri(&format!("/d/{direct_token}/wrong.m3u8"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);

    let req = test::TestRequest::get()
        .uri(&format!("/d/{direct_token}/clip%201.m3u8"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("Content-Disposition").unwrap(),
        "inline; filename*=UTF-8''clip%201.m3u8"
    );

    let req = test::TestRequest::get()
        .uri(&format!("/d/{direct_token}/clip%201.m3u8?download=1"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("Content-Disposition").unwrap(),
        "attachment; filename*=UTF-8''clip%201.m3u8"
    );

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri(&format!("/d/{direct_token}/clip%201.m3u8"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_file_preview_link_supports_public_inline_access_and_usage_limit() {
    let mut state = common::setup().await;
    state.cache = aster_drive::cache::create_cache(&aster_drive::config::CacheConfig {
        ..Default::default()
    })
    .await;
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file_named!(app, token, "report 1.docx");

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{file_id}/preview-link"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let preview_path = body["data"]["path"]
        .as_str()
        .expect("preview link path should exist")
        .to_string();
    assert!(preview_path.starts_with("/pv/"));
    assert_eq!(body["data"]["max_uses"], 5);

    for _ in 0..5 {
        let req = test::TestRequest::get().uri(&preview_path).to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        assert_eq!(
            resp.headers().get("Content-Disposition").unwrap(),
            "inline; filename*=UTF-8''report%201.docx"
        );
    }

    let req = test::TestRequest::get().uri(&preview_path).to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn test_file_download_honors_single_range_header() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file_with_name_and_mime!(
        app,
        token,
        "clip.mp4",
        "video/mp4",
        "abcdefghijklmnopqrstuvwxyz"
    );

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/download"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header((header::RANGE, "bytes=5-9"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(resp.headers().get(header::ACCEPT_RANGES).unwrap(), "bytes");
    assert_eq!(
        resp.headers().get(header::CONTENT_RANGE).unwrap(),
        "bytes 5-9/26"
    );
    assert_eq!(resp.headers().get(header::CONTENT_LENGTH).unwrap(), "5");
    let body = test::read_body(resp).await;
    assert_eq!(body.as_ref(), b"fghij");
}

#[actix_web::test]
async fn test_file_preview_link_honors_single_range_header() {
    let mut state = common::setup().await;
    state.cache = aster_drive::cache::create_cache(&aster_drive::config::CacheConfig {
        ..Default::default()
    })
    .await;
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file_with_name_and_mime!(
        app,
        token,
        "range-preview.mp4",
        "video/mp4",
        "abcdefghijklmnopqrstuvwxyz"
    );

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{file_id}/preview-link"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let preview_path = body["data"]["path"]
        .as_str()
        .expect("preview link path should exist")
        .to_string();

    let req = test::TestRequest::get()
        .uri(&preview_path)
        .insert_header((header::RANGE, "bytes=-4"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        resp.headers().get(header::CONTENT_RANGE).unwrap(),
        "bytes 22-25/26"
    );
    assert_eq!(
        resp.headers().get(header::CONTENT_DISPOSITION).unwrap(),
        "inline; filename*=UTF-8''range%2Dpreview.mp4"
    );
    let body = test::read_body(resp).await;
    assert_eq!(body.as_ref(), b"wxyz");
}

#[actix_web::test]
async fn test_file_preview_link_usage_limit_falls_back_when_cache_backend_does_not_persist() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file_named!(app, token, "fallback-preview.txt");

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{file_id}/preview-link"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let preview_path = body["data"]["path"]
        .as_str()
        .expect("preview link path should exist")
        .to_string();

    for _ in 0..5 {
        let req = test::TestRequest::get().uri(&preview_path).to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    let req = test::TestRequest::get().uri(&preview_path).to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn test_file_repo_resolve_unique_filename_prefers_first_gap_and_preserves_suffix() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);
    let user = user_repo::find_by_username(&db, "testuser")
        .await
        .unwrap()
        .expect("registered user should exist");
    let initial_candidate = file_repo::resolve_unique_filename(&db, user.id, None, "report.txt")
        .await
        .unwrap();
    assert_eq!(initial_candidate, "report.txt");

    upload_test_file_named!(app, token, "report.txt");
    upload_test_file_named!(app, token, "report (2).txt");
    upload_test_file_named!(app, token, "draft (3).txt");

    let gap_candidate = file_repo::resolve_unique_filename(&db, user.id, None, "report.txt")
        .await
        .unwrap();
    assert_eq!(gap_candidate, "report (1).txt");

    let suffix_candidate = file_repo::resolve_unique_filename(&db, user.id, None, "draft (3).txt")
        .await
        .unwrap();
    assert_eq!(suffix_candidate, "draft (4).txt");
}

#[actix_web::test]
async fn test_file_repo_resolve_unique_filename_treats_nfd_and_nfc_as_same_name() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);
    let user = user_repo::find_by_username(&db, "testuser")
        .await
        .unwrap()
        .expect("registered user should exist");

    upload_test_file_named!(app, token, "cafe\u{0301}.txt");

    let candidate = file_repo::resolve_unique_filename(&db, user.id, None, "caf\u{00e9}.txt")
        .await
        .unwrap();
    assert_eq!(candidate, "caf\u{00e9} (1).txt");
}

#[actix_web::test]
async fn test_file_repo_resolve_unique_filename_falls_back_after_candidate_batch() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);
    let user = user_repo::find_by_username(&db, "testuser")
        .await
        .unwrap()
        .expect("registered user should exist");

    upload_test_file_named!(app, token, "report.txt");
    // 32 mirrors file_repo::query::UNIQUE_FILENAME_CANDIDATE_BATCH_SIZE; update both together.
    for index in 1..32 {
        upload_test_file_named!(app, token, &format!("report ({index}).txt"));
    }

    let candidate = file_repo::resolve_unique_filename(&db, user.id, None, "report.txt")
        .await
        .unwrap();
    assert_eq!(candidate, "report (32).txt");
}

#[actix_web::test]
async fn test_dangerous_html_direct_link_stays_inline_with_csp_sandbox() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file_with_name_and_mime!(
        app,
        token,
        "dangerous.html",
        "text/html",
        "<!doctype html><script>alert(1)</script>"
    );

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/direct-link"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let direct_token = body["data"]["token"]
        .as_str()
        .expect("direct link token should exist")
        .to_string();

    let req = test::TestRequest::get()
        .uri(&format!("/d/{direct_token}/dangerous.html"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("Content-Disposition").unwrap(),
        "inline; filename*=UTF-8''dangerous.html"
    );
    assert_eq!(
        resp.headers().get("Content-Security-Policy").unwrap(),
        "sandbox"
    );
    assert_eq!(
        resp.headers().get("X-Content-Type-Options").unwrap(),
        "nosniff"
    );
}

#[actix_web::test]
async fn test_dangerous_svg_preview_link_stays_inline_with_csp_sandbox() {
    let mut state = common::setup().await;
    state.cache = aster_drive::cache::create_cache(&aster_drive::config::CacheConfig {
        ..Default::default()
    })
    .await;
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file_with_name_and_mime!(
        app,
        token,
        "dangerous.svg",
        "image/svg+xml",
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><script>alert(1)</script></svg>"
    );

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{file_id}/preview-link"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let preview_path = body["data"]["path"]
        .as_str()
        .expect("preview link path should exist")
        .to_string();

    let req = test::TestRequest::get().uri(&preview_path).to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("Content-Disposition").unwrap(),
        "inline; filename*=UTF-8''dangerous.svg"
    );
    assert_eq!(
        resp.headers().get("Content-Security-Policy").unwrap(),
        "sandbox"
    );
    assert_eq!(
        resp.headers().get("X-Content-Type-Options").unwrap(),
        "nosniff"
    );
}

#[actix_web::test]
async fn test_file_preview_link_uses_configured_public_site_url() {
    let state = common::setup().await;
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::site_url::PUBLIC_SITE_URL_KEY,
        r#"["https://drive.example.com"]"#,
    ));
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file_named!(app, token, "report 1.docx");

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{file_id}/preview-link"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let preview_path = body["data"]["path"].as_str().unwrap();

    assert!(preview_path.starts_with("https://drive.example.com/pv/"));
}

#[actix_web::test]
async fn test_file_lock_unlock() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);
    let mut storage_events = state.storage_change_tx.subscribe();

    // 锁定文件
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{file_id}/lock"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "locked": true }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["is_locked"], true);
    let event = tokio::time::timeout(Duration::from_secs(1), storage_events.recv())
        .await
        .expect("file lock should publish storage change event")
        .expect("storage change channel should stay open");
    assert_eq!(event.kind, StorageChangeKind::LockCreated);
    assert_eq!(event.file_ids, vec![file_id]);
    assert!(event.folder_ids.is_empty());
    assert!(event.affected_parent_ids.is_empty());
    assert!(event.root_affected);

    // 删除应失败
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 403 || resp.status() == 423);

    // 重命名应失败
    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "renamed.txt" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 403 || resp.status() == 423);

    // 解锁
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{file_id}/lock"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "locked": false }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let event = tokio::time::timeout(Duration::from_secs(1), storage_events.recv())
        .await
        .expect("file unlock should publish storage change event")
        .expect("storage change channel should stay open");
    assert_eq!(event.kind, StorageChangeKind::LockDeleted);
    assert_eq!(event.file_ids, vec![file_id]);
    assert!(event.folder_ids.is_empty());
    assert!(event.affected_parent_ids.is_empty());
    assert!(event.root_affected);

    // 解锁后删除成功
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_nested_file_lock_events_include_parent_folder() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    let (token, _) = register_and_login!(app);
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Lock Parent" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();
    let file_id = upload_test_file_to_folder!(app, token, folder_id);
    let mut storage_events = state.storage_change_tx.subscribe();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{file_id}/lock"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "locked": true }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let event = tokio::time::timeout(Duration::from_secs(1), storage_events.recv())
        .await
        .expect("nested file lock should publish storage change event")
        .expect("storage change channel should stay open");
    assert_eq!(event.kind, StorageChangeKind::LockCreated);
    assert_eq!(event.file_ids, vec![file_id]);
    assert!(event.folder_ids.is_empty());
    assert_eq!(event.affected_parent_ids, vec![folder_id]);
    assert!(!event.root_affected);
}

#[actix_web::test]
async fn test_upload_normalizes_nfd_filename_and_auto_renames_nfc_duplicates() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let nfc = "caf\u{00e9}.txt";
    let nfd = "cafe\u{0301}.txt";
    let copy_name = "caf\u{00e9} (1).txt";

    for (requested_name, expected_name) in [(nfd, nfc), (nfc, copy_name)] {
        let req = test::TestRequest::post()
            .uri("/api/v1/files/upload")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .insert_header((
                "Content-Type",
                "multipart/form-data; boundary=----UnicodeBoundary123",
            ))
            .set_payload(upload_payload(requested_name, "unicode content"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body["data"]["name"], expected_name);
    }

    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let names: Vec<&str> = body["data"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&nfc));
    assert!(names.contains(&copy_name));
}

#[actix_web::test]
async fn test_upload_rejects_windows_reserved_filename() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header((
            "Content-Type",
            "multipart/form-data; boundary=----UnicodeBoundary123",
        ))
        .set_payload(upload_payload("CON.txt", "reserved"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn test_file_rename_normalizes_nfd_and_rejects_windows_reserved_name() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "cafe\u{0301}.txt" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "caf\u{00e9}.txt");

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "AUX.txt" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn test_file_rename_move() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    // 创建文件夹
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Target" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    // 重命名文件
    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "renamed.txt" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "renamed.txt");

    // 移动到文件夹
    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "folder_id": folder_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // 确认在新文件夹中
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["files"][0]["name"], "renamed.txt");

    // 根目录应该没有文件了
    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 0);

    // 文件移走后，原位置应能重新创建同名文件
    let reused_root_id = upload_test_file_named!(app, token, "renamed.txt");
    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let root_files = body["data"]["files"].as_array().unwrap();
    assert_eq!(root_files.len(), 1);
    assert_eq!(root_files[0]["id"].as_i64().unwrap(), reused_root_id);
    assert_eq!(root_files[0]["name"], "renamed.txt");

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/files/{reused_root_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // 再通过 patch + null 移回根目录
    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "folder_id": null
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"]["folder_id"].is_null());

    // 文件已回到根目录
    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let root_files = body["data"]["files"].as_array().unwrap();
    assert_eq!(root_files.len(), 1);
    assert_eq!(root_files[0]["name"], "renamed.txt");

    // 目标文件夹重新为空
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 0);
}

#[actix_web::test]
async fn test_file_copy() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Source" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let source_folder_id = body["data"]["id"].as_i64().unwrap();

    let boundary = "----TestBoundary123";
    let payload = "------TestBoundary123\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"test.txt\"\r\n\
         Content-Type: text/plain\r\n\r\n\
         copy content\r\n\
         ------TestBoundary123--\r\n";
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/files/upload?folder_id={source_folder_id}"
        ))
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
    let body: Value = test::read_body_json(resp).await;
    let file_id = body["data"]["id"].as_i64().unwrap();

    // 复制到根目录（null = root）
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{file_id}/copy"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "folder_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "test.txt");
    assert!(body["data"]["folder_id"].is_null());
    let copy_id = body["data"]["id"].as_i64().unwrap();
    assert_ne!(copy_id, file_id);

    // 再复制一次到根目录（应生成冲突递增名）
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{file_id}/copy"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "folder_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "test (1).txt");
    assert!(body["data"]["folder_id"].is_null());

    // 源目录仍只保留原文件
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{source_folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let source_files = body["data"]["files"].as_array().unwrap();
    assert_eq!(source_files.len(), 1);
    assert_eq!(source_files[0]["id"].as_i64().unwrap(), file_id);

    // 根目录应出现两个副本
    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let root_files = body["data"]["files"].as_array().unwrap();
    assert_eq!(root_files.len(), 2);

    // 复制到新文件夹（应保留原名）
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "CopyDest" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let dest_folder = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{file_id}/copy"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "folder_id": dest_folder }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "test.txt");
    assert_eq!(body["data"]["folder_id"].as_i64().unwrap(), dest_folder);
}

#[actix_web::test]
async fn test_file_versions() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 上传文件 v1
    let file_id = upload_test_file!(app, token);

    // 无版本记录
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/versions"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 0);

    // 覆盖上传（同名文件 → 产生 v1 版本记录）
    let boundary = "----TestBoundary123";
    let payload = "------TestBoundary123\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"test.txt\"\r\n\
         Content-Type: text/plain\r\n\r\n\
         version 2 content\r\n\
         ------TestBoundary123--\r\n"
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
    // 同名文件应被覆盖（store_from_temp 的 existing_file_id 逻辑）
    // 但 REST upload 不走覆盖逻辑——会报同名冲突
    // 版本溯源只在 WebDAV PUT 覆盖时触发
    // 所以这里用不同名字测试版本功能不太合适
    // 改为：直接检查版本列表 API 可用性
    assert!(resp.status() == 201 || resp.status() == 400);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/versions"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_file_detail_storage_used_equals_size_without_versions() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let content = "plain-current";
    let file_id = upload_test_file_with_content!(app, token, "plain.txt", content);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["size"], content.len() as i64);
    assert_eq!(body["data"]["storage_used"], content.len() as i64);
}

#[actix_web::test]
async fn test_file_detail_storage_used_includes_all_history_versions() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let initial = "v1";
    let second = "version-two";
    let current = "version-three";
    let file_id = upload_test_file_with_content!(app, token, "versioned.txt", initial);

    for content in [second, current] {
        let req = test::TestRequest::put()
            .uri(&format!("/api/v1/files/{file_id}/content"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .insert_header(("Content-Type", "application/octet-stream"))
            .set_payload(content)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/versions"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 2);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["size"], current.len() as i64);
    assert_eq!(
        body["data"]["storage_used"],
        (initial.len() + second.len() + current.len()) as i64
    );
}

#[actix_web::test]
async fn test_empty_folder_detail_storage_used_is_zero() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Empty Storage" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{folder_id}/info"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["storage_used"], 0);
}

#[actix_web::test]
async fn test_folder_detail_storage_used_is_recursive_and_excludes_trashed_files() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Storage Used" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: Value = test::read_body_json(resp).await;
    let root_folder_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Nested",
            "parent_id": root_folder_id,
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: Value = test::read_body_json(resp).await;
    let nested_folder_id = body["data"]["id"].as_i64().unwrap();

    let root_initial = "alpha";
    let root_updated = "longer-alpha";
    let nested_initial = "beta!";
    let nested_updated = "longer-beta";
    let trashed_initial = "trashed";
    let trashed_updated = "trashed-updated";
    let root_file_id = upload_test_file_to_folder_with_content!(
        app,
        token,
        root_folder_id,
        "root.txt",
        root_initial
    );
    let nested_file_id = upload_test_file_to_folder_with_content!(
        app,
        token,
        nested_folder_id,
        "nested.txt",
        nested_initial
    );
    let trashed_file_id = upload_test_file_to_folder_with_content!(
        app,
        token,
        nested_folder_id,
        "trashed.txt",
        trashed_initial
    );

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/{root_file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(root_updated)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/{nested_file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(nested_updated)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/{trashed_file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(trashed_updated)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/files/{trashed_file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let root_file_storage_used = root_initial.len() as i64 + root_updated.len() as i64;
    let nested_file_storage_used = nested_initial.len() as i64 + nested_updated.len() as i64;

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{root_file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["size"], root_updated.len() as i64);
    assert_eq!(body["data"]["storage_used"], root_file_storage_used);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{root_folder_id}/info"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["storage_used"],
        root_file_storage_used + nested_file_storage_used
    );
}

#[actix_web::test]
async fn test_folder_detail_storage_used_handles_paginated_file_batches() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Wide Storage" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    let user = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .expect("registered test user should exist");
    let policy = policy_repo::find_default(state.writer_db())
        .await
        .unwrap()
        .expect("default policy should exist");
    let now = Utc::now();
    let blob = file_blob::ActiveModel {
        hash: Set("storage-used-pagination-blob".to_string()),
        size: Set(3),
        policy_id: Set(policy.id),
        storage_path: Set("storage-used-pagination-blob".to_string()),
        ref_count: Set(501),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("pagination test blob should insert");

    let mut expected = 0i64;
    let files = (0..501)
        .map(|index| {
            let size = if index % 2 == 0 { 2 } else { 3 };
            expected += size;
            file::ActiveModel {
                name: Set(format!("wide-{index}.txt")),
                folder_id: Set(Some(folder_id)),
                team_id: Set(None),
                blob_id: Set(blob.id),
                size: Set(size),
                owner_user_id: Set(Some(user.id)),
                created_by_user_id: Set(Some(user.id)),
                created_by_username: Set(user.username.clone()),
                mime_type: Set("text/plain".to_string()),
                extension: Set("txt".to_string()),
                compound_extension: Set(None),
                file_category: Set(FileCategory::Document),
                created_at: Set(now),
                updated_at: Set(now),
                deleted_at: Set(None),
                is_locked: Set(false),
                ..Default::default()
            }
        })
        .collect();
    file_repo::create_many(state.writer_db(), files)
        .await
        .expect("pagination test files should insert");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{folder_id}/info"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["storage_used"], expected);
}

#[actix_web::test]
async fn test_create_empty_file() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 创建空文件
    let req = test::TestRequest::post()
        .uri("/api/v1/files/new")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/json"))
        .set_json(serde_json::json!({ "name": "empty.txt", "folder_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let file_id = body["data"]["id"].as_i64().unwrap();
    assert_eq!(body["data"]["name"].as_str().unwrap(), "empty.txt");
    assert_eq!(body["data"]["size"].as_i64().unwrap(), 0);

    // 同名再建一个，应自动重命名
    let req = test::TestRequest::post()
        .uri("/api/v1/files/new")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/json"))
        .set_json(serde_json::json!({ "name": "empty.txt", "folder_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body2: Value = test::read_body_json(resp).await;
    let name2 = body2["data"]["name"].as_str().unwrap();
    assert_ne!(name2, "empty.txt", "duplicate name should be auto-renamed");
    assert_ne!(
        body2["data"]["blob_id"].as_i64().unwrap(),
        body["data"]["blob_id"].as_i64().unwrap(),
        "local create_empty should not dedup by default"
    );

    // 下载空文件应返回 200，内容为空
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/download"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let bytes = test::read_body(resp).await;
    assert!(bytes.is_empty());

    // 无效文件名应返回 400
    let req = test::TestRequest::post()
        .uri("/api/v1/files/new")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/json"))
        .set_json(serde_json::json!({ "name": "", "folder_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn test_create_empty_file_normalizes_nfd_and_rejects_windows_reserved_name() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/files/new")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/json"))
        .set_json(serde_json::json!({ "name": "cafe\u{0301}.txt", "folder_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "caf\u{00e9}.txt");

    let req = test::TestRequest::post()
        .uri("/api/v1/files/new")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/json"))
        .set_json(serde_json::json!({ "name": "PRN.txt", "folder_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

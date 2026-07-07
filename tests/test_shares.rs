//! 集成测试：`shares`。

#[macro_use]
mod common;

use actix_web::test;
use actix_web::{
    cookie::SameSite,
    http::{StatusCode, header},
};
use aster_drive::api::api_error_code::ApiErrorCode;
use aster_drive::config::operations::ARCHIVE_DOWNLOAD_SHARE_ENABLED_KEY;
use aster_drive::config::operations::SHARE_STREAM_SESSION_TTL_SECS_KEY;
use aster_drive::runtime::SharedRuntimeState;
use aster_drive::types::BackgroundTaskStatus;
use chrono::Utc;
use serde_json::Value;
use std::io::Cursor;

fn file_target(id: i64) -> Value {
    serde_json::json!({
        "type": "file",
        "id": id,
    })
}

fn folder_target(id: i64) -> Value {
    serde_json::json!({
        "type": "folder",
        "id": id,
    })
}

fn tiny_png() -> Vec<u8> {
    let mut buf = Cursor::new(Vec::new());
    let encoder = image::codecs::png::PngEncoder::new(&mut buf);
    image::ImageEncoder::write_image(encoder, &[255, 0, 0], 1, 1, image::ExtendedColorType::Rgb8)
        .unwrap();
    buf.into_inner()
}

macro_rules! upload_png {
    ($app:expr, $token:expr) => {{
        let png_bytes = tiny_png();
        let boundary = "----ShareThumbnailBound";
        let mut payload = Vec::new();
        payload.extend_from_slice(b"------ShareThumbnailBound\r\n");
        payload.extend_from_slice(
            b"Content-Disposition: form-data; name=\"file\"; filename=\"shared-thumb.png\"\r\n",
        );
        payload.extend_from_slice(b"Content-Type: image/png\r\n\r\n");
        payload.extend_from_slice(&png_bytes);
        payload.extend_from_slice(b"\r\n------ShareThumbnailBound--\r\n");

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
        let resp: actix_web::dev::ServiceResponse = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 201, "upload should return 201");
        let body: Value = test::read_body_json(resp).await;
        body["data"]["id"].as_i64().unwrap()
    }};
}

fn avatar_upload_payload() -> (String, Vec<u8>) {
    let boundary = "----AsterShareAvatarBoundary".to_string();
    let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        8,
        8,
        image::Rgba([255, 120, 0, 255]),
    ));
    let mut png = Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();

    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"file\"; filename=\"avatar.png\"\r\n\
             Content-Type: image/png\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(&png.into_inner());
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (boundary, body)
}

#[actix_web::test]
async fn test_shares_crud() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    // 创建分享
    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "target": file_target(file_id) }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"].as_str().unwrap().to_string();
    let share_id = body["data"]["id"].as_i64().unwrap();

    // 分页列出分享
    let req = test::TestRequest::get()
        .uri("/api/v1/shares?limit=1&offset=0")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(body["data"]["limit"], 1);
    assert_eq!(body["data"]["offset"], 0);
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["items"][0]["resource_name"], "test.txt");
    assert_eq!(body["data"]["items"][0]["resource_type"], "file");
    assert_eq!(body["data"]["items"][0]["status"], "active");
    assert_eq!(body["data"]["items"][0]["resource_deleted"], false);
    assert_eq!(body["data"]["items"][0]["remaining_downloads"], Value::Null);

    // 公开访问分享信息
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "test.txt");
    assert_eq!(body["data"]["mime_type"], "text/plain");
    assert!(body["data"]["size"].as_i64().unwrap() > 0);
    assert_eq!(body["data"]["shared_by"]["name"], "testuser");
    assert_eq!(body["data"]["shared_by"]["avatar"]["source"], "none");
    assert!(body["data"]["shared_by"].get("email").is_none());
    assert!(body["data"]["shared_by"].get("username").is_none());

    // 公开下载
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/download"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get(header::CONTENT_DISPOSITION).unwrap(),
        "attachment; filename*=UTF-8''test.txt"
    );

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/s/{share_token}/download?disposition=inline"
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get(header::CONTENT_DISPOSITION).unwrap(),
        "inline; filename*=UTF-8''test.txt"
    );

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/s/{share_token}/download?disposition=sideways"
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // 删除分享
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/shares/{share_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // 分享不再可访问
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_shared_thumbnail_returns_304_for_matching_if_none_match() {
    let state = common::setup().await;
    if state.writer_db().get_database_backend() == sea_orm::DbBackend::Sqlite {
        eprintln!("skipping real concurrent download counter test on SQLite single-writer pool");
        return;
    }
    let app = create_test_app!(state.clone());

    let (token, _) = register_and_login!(app);
    let file_id = upload_png!(app, token);

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "target": file_target(file_id) }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"].as_str().unwrap().to_string();

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/thumbnail"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 202);
    assert_eq!(
        resp.headers()
            .get("Retry-After")
            .and_then(|value| value.to_str().ok()),
        Some("2")
    );
    assert!(
        !resp.headers().contains_key(header::CONTENT_TYPE),
        "pending thumbnail response should not be JSON"
    );
    let body = test::read_body(resp).await;
    assert!(
        body.is_empty(),
        "pending thumbnail response should have an empty body"
    );

    let stats = aster_drive::services::task::drain(&state).await.unwrap();
    assert_eq!(stats.succeeded, 1);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/thumbnail"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let etag = resp
        .headers()
        .get("ETag")
        .and_then(|value| value.to_str().ok())
        .expect("shared thumbnail response should include ETag")
        .to_string();
    assert!(etag.contains("thumb-images-1-"));
    assert_eq!(
        resp.headers()
            .get("Cache-Control")
            .and_then(|value| value.to_str().ok()),
        Some("public, max-age=0, must-revalidate")
    );

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/thumbnail"))
        .insert_header(("If-None-Match", etag.as_str()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 304);
    assert_eq!(
        resp.headers()
            .get("ETag")
            .and_then(|value| value.to_str().ok()),
        Some(etag.as_str())
    );
    assert_eq!(
        resp.headers()
            .get("Cache-Control")
            .and_then(|value| value.to_str().ok()),
        Some("public, max-age=0, must-revalidate")
    );
}

#[actix_web::test]
async fn test_shared_folder_file_thumbnail_returns_202_until_background_generation_finishes() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    let (token, _) = register_and_login!(app);
    let file_id = upload_png!(app, token);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "shared-folder" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "folder_id": folder_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "target": folder_target(folder_id) }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"].as_str().unwrap().to_string();

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/s/{share_token}/files/{file_id}/thumbnail"
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 202);

    let stats = aster_drive::services::task::drain(&state).await.unwrap();
    assert_eq!(stats.succeeded, 1);

    let tasks =
        aster_drive::db::repository::background_task_repo::list_recent(state.writer_db(), 8)
            .await
            .unwrap();
    assert!(
        tasks
            .iter()
            .any(|task| task.status == BackgroundTaskStatus::Succeeded)
    );

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/s/{share_token}/files/{file_id}/thumbnail"
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("content-type").unwrap(), "image/webp");
}

#[actix_web::test]
async fn test_share_update_replaces_password_and_limits_without_changing_token() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": file_target(file_id),
            "password": "secret123",
            "max_downloads": 5
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_id = body["data"]["id"].as_i64().unwrap();
    let share_token = body["data"]["token"].as_str().unwrap().to_string();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/shares/{share_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "password": "new-secret",
            "expires_at": common::TEST_FUTURE_SHARE_EXPIRY_RFC3339,
            "max_downloads": 2
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["id"], share_id);
    assert_eq!(body["data"]["token"], share_token);
    assert!(body["data"].get("password").is_none());

    let req = test::TestRequest::get()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["items"][0]["token"], share_token);
    assert_eq!(body["data"]["items"][0]["has_password"], true);
    assert_eq!(body["data"]["items"][0]["max_downloads"], 2);
    assert_eq!(
        body["data"]["items"][0]["expires_at"],
        common::TEST_FUTURE_SHARE_EXPIRY_RFC3339
    );

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{share_token}/verify"))
        .set_json(serde_json::json!({ "password": "secret123" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 401 || resp.status() == 403);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{share_token}/verify"))
        .set_json(serde_json::json!({ "password": "new-secret" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/shares/{share_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "password": "",
            "expires_at": null,
            "max_downloads": 0
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["items"][0]["has_password"], false);
    assert_eq!(body["data"]["items"][0]["expires_at"], Value::Null);
    assert_eq!(body["data"]["items"][0]["max_downloads"], 0);
}

#[actix_web::test]
async fn test_share_password() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    // 创建带密码分享
    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": file_target(file_id),
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"].as_str().unwrap().to_string();

    // 公开访问 — 应显示 has_password=true
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["has_password"], true);

    let req = test::TestRequest::get()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["items"][0]["has_password"], true);
    assert_eq!(body["data"]["items"][0].get("password"), None);

    // 无密码下载 — 应被拦截（403）
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/download"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    // 验证密码
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{share_token}/verify"))
        .set_json(serde_json::json!({ "password": "secret123" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // 错误密码
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{share_token}/verify"))
        .set_json(serde_json::json!({ "password": "wrong" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 401 || resp.status() == 403);
}

#[actix_web::test]
async fn test_share_verify_cookie_scoped_and_secure_when_enabled() {
    let state = common::setup().await;
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::auth_runtime::AUTH_COOKIE_SECURE_KEY,
        "true",
    ));
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": file_target(file_id),
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"].as_str().unwrap().to_string();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{share_token}/verify"))
        .set_json(serde_json::json!({ "password": "secret123" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let expected_path = format!("/api/v1/s/{share_token}");
    let share_cookie = resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == format!("aster_share_{share_token}"))
        .expect("share verification cookie should be set");

    assert_eq!(share_cookie.path(), Some(expected_path.as_str()));
    assert_eq!(share_cookie.http_only(), Some(true));
    assert_eq!(share_cookie.same_site(), Some(SameSite::Lax));
    assert_eq!(share_cookie.secure(), Some(true));
}

#[actix_web::test]
async fn test_duplicate_active_share_rejected() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "target": file_target(file_id) }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "target": file_target(file_id) }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn test_share_batch_delete_removes_multiple_shares() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let file_one = upload_test_file_named!(app, token, "share-one.txt");
    let file_two = upload_test_file_named!(app, token, "share-two.txt");

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "target": file_target(file_one) }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_one_id = body["data"]["id"].as_i64().unwrap();
    let share_one_token = body["data"]["token"].as_str().unwrap().to_string();

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "target": file_target(file_two) }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_two_id = body["data"]["id"].as_i64().unwrap();
    let share_two_token = body["data"]["token"].as_str().unwrap().to_string();

    let req = test::TestRequest::post()
        .uri("/api/v1/shares/batch-delete")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "share_ids": [share_one_id, share_two_id]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["succeeded"], 2);
    assert_eq!(body["data"]["failed"], 0);

    let req = test::TestRequest::get()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total"], 0);

    for share_token in [share_one_token, share_two_token] {
        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/s/{share_token}"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);
    }
}

#[actix_web::test]
async fn test_share_batch_delete_preserves_partial_failures_for_foreign_and_missing_ids() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);
    let (owner_token, _) = register_and_login!(app);

    let owner_file = upload_test_file_named!(app, owner_token, "owner-share.txt");
    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "target": file_target(owner_file) }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let owner_share_id = body["data"]["id"].as_i64().unwrap();
    let owner_share_token = body["data"]["token"].as_str().unwrap().to_string();

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "shareother",
            "email": "shareother@example.com",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let _ = confirm_latest_contact_verification!(app, db, mail_sender);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "shareother",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let other_token =
        common::extract_cookie(&resp, "aster_access").expect("other access cookie missing");

    let other_file = upload_test_file_named!(app, other_token, "other-share.txt");
    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&other_token)))
        .insert_header(common::csrf_header_for(&other_token))
        .set_json(serde_json::json!({ "target": file_target(other_file) }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let other_share_id = body["data"]["id"].as_i64().unwrap();
    let other_share_token = body["data"]["token"].as_str().unwrap().to_string();

    let req = test::TestRequest::post()
        .uri("/api/v1/shares/batch-delete")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "share_ids": [owner_share_id, other_share_id, 999999]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["succeeded"], 1);
    assert_eq!(body["data"]["failed"], 2);
    let errors = body["data"]["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 2);
    assert!(
        errors
            .iter()
            .any(|item| item["entity_id"] == other_share_id)
    );
    assert!(errors.iter().any(|item| item["entity_id"] == 999999));

    let req = test::TestRequest::get()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total"], 0);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{owner_share_token}"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);

    let req = test::TestRequest::get()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&other_token)))
        .insert_header(common::csrf_header_for(&other_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(body["data"]["items"][0]["id"], other_share_id);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{other_share_token}"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_share_download_limit() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    // 创建限 1 次下载的分享
    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": file_target(file_id),
            "max_downloads": 1
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"].as_str().unwrap().to_string();

    // 第一次下载 OK
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/download"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // 第二次下载应被拒绝（403）
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/download"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403, "download limit should block");

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{share_token}/preview-link"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        200,
        "iframe preview-link should ignore exhausted download limit"
    );
    let body: Value = test::read_body_json(resp).await;
    let preview_path = body["data"]["path"].as_str().unwrap().to_string();

    for _ in 0..8 {
        let req = test::TestRequest::get()
            .uri(&preview_path)
            .insert_header((header::RANGE, "bytes=0-3"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            StatusCode::PARTIAL_CONTENT,
            "preview-link range resource should ignore exhausted download limit"
        );
        assert_eq!(
            resp.headers().get(header::CONTENT_RANGE).unwrap(),
            "bytes 0-3/12"
        );
        let body = test::read_body(resp).await;
        assert_eq!(body.as_ref(), b"test");
    }

    let req = test::TestRequest::get()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["items"][0]["status"], "exhausted");
    assert_eq!(body["data"]["items"][0]["download_count"], 1);
    assert_eq!(body["data"]["items"][0]["remaining_downloads"], 0);
}

#[actix_web::test]
async fn test_share_download_limit_counter_is_atomic_under_concurrency() {
    use aster_drive::db::repository::share_repo;
    use aster_drive::utils::raii::TempDirGuard;

    let temp_dir = std::env::temp_dir().join(format!(
        "asterdrive-share-download-race-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&temp_dir).expect("share race test db dir should be created");
    let _temp_dir_guard = TempDirGuard::new(temp_dir.clone(), "share download race test db");
    let database_url = format!("sqlite://{}?mode=rwc", temp_dir.join("shares.db").display());

    let state = common::setup_with_database_url(&database_url).await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": file_target(file_id),
            "max_downloads": 1
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_id = body["data"]["id"].as_i64().unwrap();

    let mut dbs = Vec::new();
    for _ in 0..32 {
        let cfg = aster_drive::config::DatabaseConfig {
            url: database_url.clone(),
            pool_size: 1,
            retry_count: 0,
        };
        dbs.push(
            aster_drive::db::connect_with_metrics(
                &cfg,
                aster_drive::metrics_core::NoopMetrics::arc(),
            )
            .await
            .expect("share race test connection should open"),
        );
    }

    let mut tasks = tokio::task::JoinSet::new();
    for db in dbs {
        tasks.spawn(async move { share_repo::increment_download_count(&db, share_id).await });
    }

    let mut reserved = 0;
    let mut rejected = 0;
    while let Some(result) = tasks.join_next().await {
        match result.unwrap().unwrap() {
            true => reserved += 1,
            false => rejected += 1,
        }
    }

    assert_eq!(
        reserved, 1,
        "only one concurrent request may reserve the slot"
    );
    assert_eq!(
        rejected, 31,
        "all other concurrent requests must be rejected"
    );

    let share = share_repo::find_by_id(state.writer_db(), share_id)
        .await
        .unwrap();
    assert_eq!(share.download_count, 1);
    assert_eq!(share.max_downloads, 1);
}

#[actix_web::test]
async fn test_share_stream_session_counts_once_across_ranges_and_survives_limit() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file_named!(app, token, "clip.mp4");

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": file_target(file_id),
            "max_downloads": 1
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"].as_str().unwrap().to_string();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{share_token}/stream-session"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let stream_path = body["data"]["path"].as_str().unwrap().to_string();

    let req = test::TestRequest::get()
        .uri(&stream_path)
        .insert_header(("Range", "bytes=0-3"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        resp.headers()
            .get("Content-Range")
            .and_then(|value| value.to_str().ok()),
        Some("bytes 0-3/12")
    );
    assert_eq!(test::read_body(resp).await.as_ref(), b"test");

    let req = test::TestRequest::get()
        .uri(&stream_path)
        .insert_header(("Range", "bytes=4-7"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(test::read_body(resp).await.as_ref(), b" con");

    let req = test::TestRequest::get()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["items"][0]["download_count"], 1);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{share_token}/stream-session"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "new stream sessions should be blocked after the limit is exhausted"
    );
}

#[actix_web::test]
async fn test_share_stream_session_uses_configured_ttl() {
    let state = common::setup().await;
    state.runtime_config.apply(common::system_config_model(
        SHARE_STREAM_SESSION_TTL_SECS_KEY,
        "3600",
    ));
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file_named!(app, token, "song.mp3");

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": file_target(file_id)
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"].as_str().unwrap().to_string();

    let before = Utc::now().timestamp();
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{share_token}/stream-session"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let after = Utc::now().timestamp();
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let expires_at = body["data"]["expires_at"].as_str().unwrap();
    let expires_at = chrono::DateTime::parse_from_rfc3339(expires_at)
        .unwrap()
        .timestamp();

    assert!(expires_at >= before + 3600);
    assert!(expires_at <= after + 3600);
}

#[actix_web::test]
async fn test_folder_share_archive_download_ticket_and_boundaries() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Shared Root" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let root_id = body["data"]["id"].as_i64().unwrap();
    let shared_file_id = upload_test_file_to_folder!(app, token, root_id);
    let outside_file_id = upload_test_file_named!(app, token, "outside-share.txt");

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "target": folder_target(root_id) }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"].as_str().unwrap().to_string();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{share_token}/archive-download"))
        .set_json(serde_json::json!({
            "file_ids": [shared_file_id],
            "folder_ids": [],
            "archive_name": "shared.zip"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let ticket = body["data"]["token"].as_str().unwrap();
    assert!(ticket.starts_with("st_"));
    assert_eq!(
        body["data"]["download_path"],
        format!("/api/v1/s/{share_token}/archive-download/{ticket}")
    );

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{share_token}/archive-download"))
        .set_json(serde_json::json!({
            "file_ids": [outside_file_id],
            "folder_ids": [],
            "archive_name": "outside.zip"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    state.runtime_config.apply(common::system_config_model(
        ARCHIVE_DOWNLOAD_SHARE_ENABLED_KEY,
        "false",
    ));
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{share_token}/archive-download"))
        .set_json(serde_json::json!({
            "file_ids": [shared_file_id],
            "folder_ids": [],
            "archive_name": "disabled.zip"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::ArchiveDownloadShareDisabled.as_ref()
    );

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/s/{share_token}/archive-download/{ticket}"
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::ArchiveDownloadShareDisabled.as_ref()
    );
}

#[actix_web::test]
async fn test_folder_share_archive_download_counts_against_download_limit() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Zip Limit" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();
    let file_id = upload_test_file_to_folder!(app, token, folder_id);

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": folder_target(folder_id),
            "max_downloads": 1
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"].as_str().unwrap().to_string();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{share_token}/archive-download"))
        .set_json(serde_json::json!({
            "file_ids": [file_id],
            "folder_ids": [],
            "archive_name": "first.zip"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let first_ticket = body["data"]["token"].as_str().unwrap().to_string();

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/s/{share_token}/archive-download/{first_ticket}"
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body = test::read_body(resp).await;
    assert!(!body.is_empty());

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/s/{share_token}/archive-download/{first_ticket}"
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::ShareDownloadLimitReached.as_ref()
    );

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{share_token}/archive-download"))
        .set_json(serde_json::json!({
            "file_ids": [file_id],
            "folder_ids": [],
            "archive_name": "second.zip"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::ShareDownloadLimitReached.as_ref()
    );

    let req = test::TestRequest::get()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["items"][0]["download_count"], 1);
    assert_eq!(body["data"]["items"][0]["remaining_downloads"], 0);
}

#[actix_web::test]
async fn test_folder_share_archive_download_rolls_back_when_client_drops_stream() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Zip Abort" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    let boundary = "----ShareArchiveAbortBoundary";
    let mut payload = format!(
        "--{boundary}\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"large-abort.bin\"\r\n\
         Content-Type: application/octet-stream\r\n\r\n"
    )
    .into_bytes();
    let mut seed = 0x1234_5678_u32;
    for _ in 0..(2 * 1024 * 1024) {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        payload.push((seed >> 24) as u8);
    }
    payload.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/upload?folder_id={folder_id}"))
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

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": folder_target(folder_id),
            "max_downloads": 1
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"].as_str().unwrap().to_string();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{share_token}/archive-download"))
        .set_json(serde_json::json!({
            "file_ids": [file_id],
            "folder_ids": [],
            "archive_name": "abort.zip"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let first_ticket = body["data"]["token"].as_str().unwrap().to_string();

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/s/{share_token}/archive-download/{first_ticket}"
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    drop(resp);

    let rollback_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let req = test::TestRequest::get()
            .uri("/api/v1/shares")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: Value = test::read_body_json(resp).await;
        if body["data"]["items"][0]["download_count"] == 0 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < rollback_deadline,
            "share archive stream abort should roll back download_count"
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{share_token}/archive-download"))
        .set_json(serde_json::json!({
            "file_ids": [file_id],
            "folder_ids": [],
            "archive_name": "after-abort.zip"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_folder_share_archive_download_rejects_passwordless_and_cross_share_tickets() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Secret Root" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let secret_root_id = body["data"]["id"].as_i64().unwrap();
    let secret_file_id = upload_test_file_to_folder!(app, token, secret_root_id);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Public Root" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let public_root_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": folder_target(secret_root_id),
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let secret_share_token = body["data"]["token"].as_str().unwrap().to_string();

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "target": folder_target(public_root_id) }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let public_share_token = body["data"]["token"].as_str().unwrap().to_string();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{secret_share_token}/archive-download"))
        .set_json(serde_json::json!({
            "file_ids": [secret_file_id],
            "folder_ids": [],
            "archive_name": "secret.zip"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{secret_share_token}/verify"))
        .set_json(serde_json::json!({ "password": "secret123" }))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let signed_cookie = common::extract_cookie(&resp, &format!("aster_share_{secret_share_token}"))
        .expect("share password cookie should be set");

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{secret_share_token}/archive-download"))
        .insert_header((
            "Cookie",
            format!("aster_share_{secret_share_token}={signed_cookie}"),
        ))
        .set_json(serde_json::json!({
            "file_ids": [secret_file_id],
            "folder_ids": [],
            "archive_name": "secret.zip"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let secret_ticket = body["data"]["token"].as_str().unwrap();

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/s/{public_share_token}/archive-download/{secret_ticket}"
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn test_password_protected_share_stream_session_requires_cookie() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file_named!(app, token, "private-clip.mp4");

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": file_target(file_id),
            "password": "secret"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"].as_str().unwrap().to_string();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{share_token}/stream-session"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{share_token}/verify"))
        .insert_header(("User-Agent", "AsterDrive Share Client/1.0"))
        .peer_addr("203.0.113.42:12345".parse().unwrap())
        .set_json(serde_json::json!({"password": "secret"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let signed_cookie = common::extract_cookie(&resp, &format!("aster_share_{share_token}"))
        .expect("password verification should set share cookie");
    let cookie_header = format!("aster_share_{share_token}={signed_cookie}");

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{share_token}/stream-session"))
        .insert_header(("Cookie", cookie_header.as_str()))
        .insert_header(("User-Agent", "AsterDrive Share Client/1.0"))
        .peer_addr("203.0.113.42:12345".parse().unwrap())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let stream_path = body["data"]["path"].as_str().unwrap().to_string();

    let req = test::TestRequest::get()
        .uri(&stream_path)
        .insert_header(("Range", "bytes=0-3"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let req = test::TestRequest::get()
        .uri(&stream_path)
        .insert_header(("Cookie", cookie_header))
        .insert_header(("User-Agent", "AsterDrive Share Client/1.0"))
        .peer_addr("203.0.113.42:12345".parse().unwrap())
        .insert_header(("Range", "bytes=0-3"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
}

#[actix_web::test]
async fn test_folder_share_stream_session_counts_once_across_ranges() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Video Folder" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();
    let file_id = upload_test_file_to_folder!(app, token, folder_id);

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": folder_target(folder_id),
            "max_downloads": 1
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"].as_str().unwrap().to_string();

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/s/{share_token}/files/{file_id}/stream-session"
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let stream_path = body["data"]["path"].as_str().unwrap().to_string();

    let req = test::TestRequest::get()
        .uri(&stream_path)
        .insert_header(("Range", "bytes=0-3"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(test::read_body(resp).await.as_ref(), b"test");

    let req = test::TestRequest::get()
        .uri(&stream_path)
        .insert_header(("Range", "bytes=5-11"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(test::read_body(resp).await.as_ref(), b"content");

    let req = test::TestRequest::get()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["items"][0]["download_count"], 1);

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/s/{share_token}/files/{file_id}/stream-session"
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn test_share_folder() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 创建文件夹
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Shared Folder" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    // 上传一个文件到该文件夹
    let file_id = upload_test_file_to_folder!(app, token, folder_id);

    // 分享文件夹
    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "target": folder_target(folder_id) }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"].as_str().unwrap().to_string();

    // 公开查看分享信息
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["share_type"], "folder");
    assert_eq!(body["data"]["shared_by"]["name"], "testuser");

    // 公开列出文件夹内容
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/content"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // 下载文件夹内文件
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/files/{file_id}/download"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_password_protected_share_preview_link_requires_cookie_but_does_not_increment_downloads()
 {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file_named!(app, token, "secret-report.docx");

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": file_target(file_id),
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"].as_str().unwrap().to_string();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{share_token}/preview-link"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{share_token}/verify"))
        .set_json(serde_json::json!({ "password": "secret123" }))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let signed_cookie = common::extract_cookie(&resp, &format!("aster_share_{share_token}"))
        .expect("share password cookie should be set");

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{share_token}/preview-link"))
        .insert_header((
            "Cookie",
            format!("aster_share_{share_token}={signed_cookie}"),
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let preview_path = body["data"]["path"].as_str().unwrap().to_string();

    let req = test::TestRequest::get().uri(&preview_path).to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("Content-Disposition").unwrap(),
        "inline; filename*=UTF-8''secret%2Dreport.docx"
    );

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["download_count"], 0);
}

#[actix_web::test]
async fn test_folder_share_file_preview_link_supports_public_inline_access() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Preview Folder" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    let file_id = upload_test_file_to_folder!(app, token, folder_id);

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": folder_target(folder_id),
            "max_downloads": 1
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"].as_str().unwrap().to_string();

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/files/{file_id}/download"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/files/{file_id}/download"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403, "download limit should block");

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/s/{share_token}/files/{file_id}/preview-link"
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        200,
        "folder file iframe preview-link should ignore exhausted download limit"
    );
    let body: Value = test::read_body_json(resp).await;
    let preview_path = body["data"]["path"].as_str().unwrap().to_string();

    let req = test::TestRequest::get().uri(&preview_path).to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("Content-Disposition").unwrap(),
        "inline; filename*=UTF-8''test%2Din%2Dfolder.txt"
    );
}

#[actix_web::test]
async fn test_public_share_info_prefers_display_name_and_exposes_gravatar_avatar() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/profile")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "display_name": "Test User"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::put()
        .uri("/api/v1/auth/profile/avatar/source")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "source": "gravatar"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let file_id = upload_test_file!(app, token);
    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "target": file_target(file_id) }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"].as_str().unwrap().to_string();

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["shared_by"]["name"], "Test User");
    assert_eq!(body["data"]["shared_by"]["avatar"]["source"], "gravatar");
    assert!(
        body["data"]["shared_by"]["avatar"]["url_512"]
            .as_str()
            .unwrap()
            .contains("gravatar.com/avatar/")
    );
    assert!(body["data"]["shared_by"].get("email").is_none());
    assert!(body["data"]["shared_by"].get("username").is_none());
}

#[actix_web::test]
async fn test_share_avatar_route_serves_uploaded_avatar_and_requires_password_cookie() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let (boundary, payload) = avatar_upload_payload();
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/profile/avatar/upload")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let file_id = upload_test_file!(app, token);
    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": file_target(file_id),
            "password": "secret123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"].as_str().unwrap().to_string();

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["shared_by"]["avatar"]["source"], "upload");
    assert_eq!(
        body["data"]["shared_by"]["avatar"]["url_512"],
        format!("/s/{share_token}/avatar/512?v=1")
    );

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/avatar/512"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{share_token}/verify"))
        .set_json(serde_json::json!({ "password": "secret123" }))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let signed_cookie = common::extract_cookie(&resp, &format!("aster_share_{share_token}"))
        .expect("share password cookie should be set");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/avatar/512"))
        .insert_header((
            "Cookie",
            format!("aster_share_{share_token}={signed_cookie}"),
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("content-type").unwrap(), "image/webp");
}

/// 伪造 cookie 不能绕过密码验证
#[actix_web::test]
async fn test_expired_share_public_endpoints_rejected() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": file_target(file_id),
            "expires_at": "2000-01-01T00:00:00Z"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"].as_str().unwrap();

    for path in [
        format!("/api/v1/s/{share_token}"),
        format!("/api/v1/s/{share_token}/download"),
        format!("/api/v1/s/{share_token}/thumbnail"),
    ] {
        let req = test::TestRequest::get().uri(&path).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(
            resp.status() == 403 || resp.status() == 404,
            "expired share path {path} should be rejected without 410, got {}",
            resp.status()
        );
        assert_eq!(
            resp.headers()
                .get("Cache-Control")
                .and_then(|value| value.to_str().ok()),
            Some("no-store, max-age=0")
        );
    }

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{share_token}/verify"))
        .set_json(serde_json::json!({ "password": "secret123" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 403 || resp.status() == 404,
        "expired share verify should be rejected without 410, got {}",
        resp.status()
    );
    assert_eq!(
        resp.headers()
            .get("Cache-Control")
            .and_then(|value| value.to_str().ok()),
        Some("no-store, max-age=0")
    );
}

#[actix_web::test]
async fn test_folder_share_deleted_child_resource_rejected() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Shared Root" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let root_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Child", "parent_id": root_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let child_id = body["data"]["id"].as_i64().unwrap();

    let child_file_id = upload_test_file_to_folder!(app, token, child_id);

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "target": folder_target(root_id) }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"].as_str().unwrap().to_string();

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/files/{child_file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/s/{share_token}/files/{child_file_id}/download"
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 404 || resp.status() == 403);

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{child_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/s/{share_token}/folders/{child_id}/content"
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 404 || resp.status() == 403);
}

#[actix_web::test]
async fn test_share_type_mismatch_rejected() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "target": file_target(file_id) }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let file_share_token = body["data"]["token"].as_str().unwrap();

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{file_share_token}/content"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_client_error());

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Folder Share" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "target": folder_target(folder_id) }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let folder_share_token = body["data"]["token"].as_str().unwrap();

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{folder_share_token}/download"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_client_error());
}

#[actix_web::test]
async fn test_share_forged_cookie_rejected() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    // 创建带密码分享
    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": file_target(file_id),
            "password": "secret"
        }))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"].as_str().unwrap().to_string();

    // 用伪造 cookie 尝试下载 → 应被拒绝
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/download"))
        .insert_header(("Cookie", format!("aster_share_{share_token}=forged_value")))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        403,
        "forged cookie should be rejected, got {}",
        resp.status()
    );

    // 用正确流程验证密码
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{share_token}/verify"))
        .insert_header(("User-Agent", "AsterDrive Share Client/1.0"))
        .peer_addr("203.0.113.42:12345".parse().unwrap())
        .set_json(serde_json::json!({"password": "secret"}))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // 提取签名 cookie
    let signed_cookie = common::extract_cookie(&resp, &format!("aster_share_{share_token}"))
        .expect("should get signed cookie");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/download"))
        .insert_header((
            "Cookie",
            format!("aster_share_{share_token}={signed_cookie}"),
        ))
        .insert_header(("User-Agent", "AsterDrive Share Client/2.0"))
        .peer_addr("203.0.113.42:12345".parse().unwrap())
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        403,
        "cookie verified for one user agent must not replay from another"
    );

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/download"))
        .insert_header((
            "Cookie",
            format!("aster_share_{share_token}={signed_cookie}"),
        ))
        .insert_header(("User-Agent", "AsterDrive Share Client/1.0"))
        .peer_addr("203.0.114.10:12345".parse().unwrap())
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        403,
        "cookie verified for one IPv4 /24 must not replay from another subnet"
    );

    // 用签名 cookie 下载 → 应成功
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/download"))
        .insert_header((
            "Cookie",
            format!("aster_share_{share_token}={signed_cookie}"),
        ))
        .insert_header(("User-Agent", "AsterDrive Share Client/1.0"))
        .peer_addr("203.0.113.99:12345".parse().unwrap())
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        200,
        "signed cookie should allow download, got {}",
        resp.status()
    );
}

#[actix_web::test]
async fn test_my_shares_list_deleted_and_expired_status() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let active_file_id = upload_test_file!(app, token);

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "target": file_target(active_file_id) }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Shared Deleted Folder" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let deleted_folder_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "target": folder_target(deleted_folder_id) }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{deleted_folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let expired_file_id = upload_test_file!(app, token);
    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": file_target(expired_file_id),
            "expires_at": "2000-01-01T00:00:00Z"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let expired_token = body["data"]["token"].as_str().unwrap().to_string();

    let req = test::TestRequest::get()
        .uri("/api/v1/shares?limit=10&offset=0")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let items = body["data"]["items"].as_array().unwrap();

    let deleted_item = items
        .iter()
        .find(|item| item["resource_name"] == "Shared Deleted Folder")
        .expect("deleted folder item");
    assert_eq!(deleted_item["resource_type"], "folder");
    assert_eq!(deleted_item["resource_deleted"], true);
    assert_eq!(deleted_item["status"], "deleted");

    let expired_item = items.iter().find(|item| item["token"] == expired_token);
    let expired_item = expired_item.expect("expired item should be present");
    assert_eq!(expired_item["resource_type"], "file");
    assert_eq!(expired_item["status"], "expired");
    assert_eq!(expired_item["resource_deleted"], false);
}

#[actix_web::test]
async fn test_purging_trash_removes_related_shares() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let standalone_file_id = upload_test_file!(app, token);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Purge Shared Folder" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();
    let nested_file_id = upload_test_file_to_folder!(app, token, folder_id);

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "target": file_target(standalone_file_id) }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let standalone_share_token = body["data"]["token"].as_str().unwrap().to_string();

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "target": folder_target(folder_id) }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_share_token = body["data"]["token"].as_str().unwrap().to_string();

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "target": file_target(nested_file_id) }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let nested_file_share_token = body["data"]["token"].as_str().unwrap().to_string();

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/files/{standalone_file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/shares?limit=10&offset=0")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total"], 3);
    let items = body["data"]["items"].as_array().unwrap();
    for share_token in [
        &standalone_share_token,
        &folder_share_token,
        &nested_file_share_token,
    ] {
        let item = items
            .iter()
            .find(|item| item["token"] == share_token.as_str())
            .expect("share should remain while resource is only in trash");
        assert_eq!(item["resource_deleted"], true);
        assert_eq!(item["status"], "deleted");
    }

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/trash/file/{standalone_file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/trash/folder/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/shares?limit=10&offset=0")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total"], 0);
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 0);

    for share_token in [
        standalone_share_token,
        folder_share_token,
        nested_file_share_token,
    ] {
        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/s/{share_token}"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);
    }
}

#[actix_web::test]
async fn test_share_folder_deep_scope_and_outside_access() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Root" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let root_id = body["data"]["id"].as_i64().unwrap();

    let mut parent_id = root_id;
    for name in ["A", "B", "C"] {
        let req = test::TestRequest::post()
            .uri("/api/v1/folders")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({ "name": name, "parent_id": parent_id }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        let body: Value = test::read_body_json(resp).await;
        parent_id = body["data"]["id"].as_i64().unwrap();
    }
    let deep_folder_id = parent_id;
    let deep_file_id = upload_test_file_to_folder!(app, token, deep_folder_id);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Outside" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let outside_folder_id = body["data"]["id"].as_i64().unwrap();
    let outside_file_id = upload_test_file_to_folder!(app, token, outside_folder_id);

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "target": folder_target(root_id) }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"].as_str().unwrap();

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/s/{share_token}/folders/{deep_folder_id}/content"
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/s/{share_token}/files/{deep_file_id}/download"
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/s/{share_token}/files/{outside_file_id}/download"
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/s/{share_token}/folders/{outside_folder_id}/content"
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn test_share_folder_subfolder_navigation() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 创建根文件夹
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Shared Root" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let root_id = body["data"]["id"].as_i64().unwrap();

    // 创建子文件夹
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Subfolder", "parent_id": root_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let sub_id = body["data"]["id"].as_i64().unwrap();

    // 上传文件到子文件夹
    let _file_id = upload_test_file_to_folder!(app, token, sub_id);

    // 分享根文件夹
    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "target": folder_target(root_id) }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"].as_str().unwrap().to_string();

    // 根目录内容应包含 Subfolder
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/content"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let folders = body["data"]["folders"].as_array().unwrap();
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0]["name"], "Subfolder");

    // 子文件夹内容应包含文件
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/folders/{sub_id}/content"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let files = body["data"]["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);

    // root 自身也能通过子文件夹接口访问
    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/s/{share_token}/folders/{root_id}/content"
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // 创建不相关文件夹 — 越权访问应被拒绝
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Outside" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let outside_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/s/{share_token}/folders/{outside_id}/content"
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        403,
        "accessing folder outside share scope should return 403, got {}",
        resp.status()
    );
}

#[actix_web::test]
async fn test_create_share_rejects_negative_max_downloads() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": file_target(file_id),
            "max_downloads": -1
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["msg"], "max_downloads cannot be negative");
}

//! 集成测试：`edit`。

#[macro_use]
mod common;
use aster_drive::runtime::SharedRuntimeState;

use actix_web::body::to_bytes;
use actix_web::test;
use serde_json::Value;

const OVER_LIMIT_BODY_SIZE: usize = 10 * 1024 * 1024 + 1;

fn snapshot_dir_tree(
    path: &std::path::Path,
) -> std::io::Result<std::collections::BTreeSet<String>> {
    fn walk(
        root: &std::path::Path,
        current: &std::path::Path,
        entries: &mut std::collections::BTreeSet<String>,
    ) -> std::io::Result<()> {
        for entry in std::fs::read_dir(current)? {
            let entry = entry?;
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                entries.insert(format!("{relative}/"));
                walk(root, &path, entries)?;
            } else {
                entries.insert(relative);
            }
        }
        Ok(())
    }

    let mut entries = std::collections::BTreeSet::new();
    if !path.exists() {
        return Ok(entries);
    }
    walk(path, path, &mut entries)?;
    Ok(entries)
}

// ── PUT /content 基本覆盖写入 ───────────────────────────────

#[actix_web::test]
async fn test_update_content_basic() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    // PUT /content 覆盖
    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/{file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload("updated content")
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200, "PUT content should return 200");

    // 应有 ETag header
    let etag = resp
        .headers()
        .get("ETag")
        .map(|v| v.to_str().unwrap().to_string());
    assert!(etag.is_some(), "response should have ETag header");

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "success");

    // 下载验证 ETag
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/download"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let dl_etag = resp
        .headers()
        .get("ETag")
        .map(|v| v.to_str().unwrap().to_string());
    assert!(dl_etag.is_some(), "download should have ETag");
    assert_eq!(etag, dl_etag, "ETag should match between PUT and GET");
}

#[actix_web::test]
async fn test_update_content_local_fast_path_avoids_global_temp_dir() {
    let state = common::setup().await;
    let temp_dir = state.config.server.temp_dir.clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);
    let temp_snapshot_before = snapshot_dir_tree(std::path::Path::new(&temp_dir)).unwrap();

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/{file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload("updated through local staging")
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let temp_snapshot_after = snapshot_dir_tree(std::path::Path::new(&temp_dir)).unwrap();
    assert_eq!(
        temp_snapshot_after, temp_snapshot_before,
        "local PUT /content should not create files in the global temp dir"
    );
}

#[actix_web::test]
async fn test_update_content_allows_body_larger_than_global_payload_limit() {
    let state = common::setup().await;
    let temp_dir = state.config.server.temp_dir.clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);
    let payload = vec![b'x'; OVER_LIMIT_BODY_SIZE];
    let temp_snapshot_before = snapshot_dir_tree(std::path::Path::new(&temp_dir)).unwrap();

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/{file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(payload.clone())
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["size"], payload.len() as i64);

    let temp_snapshot_after = snapshot_dir_tree(std::path::Path::new(&temp_dir)).unwrap();
    assert_eq!(temp_snapshot_after, temp_snapshot_before);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/download"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let downloaded = to_bytes(resp.into_body()).await.unwrap();
    assert_eq!(downloaded.as_ref(), payload.as_slice());
}

// ── PUT /content 自动创建版本 ───────────────────────────────

#[actix_web::test]
async fn test_update_content_creates_version() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    // 覆盖写入
    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/{file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload("v2")
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // 查版本列表
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/versions"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let versions = body["data"].as_array().unwrap();
    assert!(
        !versions.is_empty(),
        "should have at least 1 history version"
    );
}

#[actix_web::test]
async fn test_restore_single_history_version_recovers_original_content() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/{file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload("v2")
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/versions"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let versions = body["data"].as_array().unwrap();
    assert_eq!(versions.len(), 1, "should have exactly one history version");
    let version_id = versions[0]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/files/{file_id}/versions/{version_id}/restore"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/download"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body = to_bytes(resp.into_body()).await.unwrap();
    assert_eq!(body.as_ref(), b"test content");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/versions"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let versions = body["data"].as_array().unwrap();
    assert!(
        versions.is_empty(),
        "history should be empty after restoring v1"
    );
}

// ── ETag 乐观锁：正确 ETag 通过 ────────────────────────────

#[actix_web::test]
async fn test_download_if_none_match_returns_304() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/download"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let etag = resp
        .headers()
        .get("ETag")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/download"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("If-None-Match", etag.as_str()))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        304,
        "matching If-None-Match should return 304"
    );
    assert_eq!(
        resp.headers().get("ETag").and_then(|v| v.to_str().ok()),
        Some(etag.as_str())
    );
}

#[actix_web::test]
async fn test_update_content_etag_match() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    // 下载拿 ETag
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/download"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    let etag = resp
        .headers()
        .get("ETag")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    // 用正确 ETag 覆盖
    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/{file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .insert_header(("If-Match", etag.as_str()))
        .set_payload("updated with etag")
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200, "matching ETag should succeed");
}

// ── ETag 乐观锁：错误 ETag 返回 412 ────────────────────────

#[actix_web::test]
async fn test_update_content_etag_mismatch() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/{file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .insert_header(("If-Match", "\"wrong-etag\""))
        .set_payload("should fail")
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        412,
        "mismatched ETag should return 412, got {}",
        resp.status()
    );
}

// ── 悲观锁：被其他用户锁定时返回 423 ───────────────────────

#[actix_web::test]
async fn test_update_content_locked_by_other() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    // 用另一个 owner_id 直接插锁（模拟其他用户）
    {
        use sea_orm::{ActiveModelTrait, Set};
        let now = chrono::Utc::now();
        let lock = aster_drive::entities::resource_lock::ActiveModel {
            token: Set(format!("urn:uuid:{}", uuid::Uuid::new_v4())),
            entity_type: Set(aster_drive::types::EntityType::File),
            entity_id: Set(file_id),
            path: Set("/test.txt".to_string()),
            owner_id: Set(Some(99999)),
            owner_info: Set(None),
            timeout_at: Set(None),
            shared: Set(false),
            deep: Set(false),
            created_at: Set(now),
            ..Default::default()
        };
        lock.insert(&db).await.unwrap();
        aster_drive::services::files::lock::set_entity_locked(
            &db,
            aster_drive::types::EntityType::File,
            file_id,
            true,
        )
        .await
        .unwrap();
    }

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/{file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload("should fail")
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        423,
        "locked file should return 423, got {}",
        resp.status()
    );
}

// ── 悲观锁：锁持有者可以写入 ───────────────────────────────

#[actix_web::test]
async fn test_update_content_lock_owner_can_write() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    // 自己锁定
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{file_id}/lock"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({"locked": true}))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // 锁持有者覆盖
    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/{file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload("updated by lock owner")
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        200,
        "lock owner should be able to write, got {}",
        resp.status()
    );
}

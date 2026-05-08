//! 集成测试：`folders`。

#[macro_use]
mod common;

use actix_web::test;
use aster_drive::db::repository::{folder_repo, user_repo};
use serde_json::Value;

#[actix_web::test]
async fn test_folders_crud() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);

    // 列出根目录（应为空）
    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["folders"], Value::Array(vec![]));
    assert_eq!(body["data"]["files"], Value::Array(vec![]));

    // 创建文件夹
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Documents" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();
    assert_eq!(body["data"]["name"], "Documents");

    // 列出根目录（应有 1 个文件夹）
    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["folders"].as_array().unwrap().len(), 1);

    // 重命名文件夹
    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "My Docs" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "My Docs");

    // 删除文件夹
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_folder_lock_unlock() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);

    // 创建文件夹
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Locked Folder" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    // 锁定
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/folders/{folder_id}/lock"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "locked": true }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // 删除失败
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 403 || resp.status() == 423);

    // 重命名失败
    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Nope" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 403 || resp.status() == 423);

    // 解锁 → 删除成功
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/folders/{folder_id}/lock"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "locked": false }))
        .to_request();
    test::call_service(&app, req).await;

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_folder_create_normalizes_nfd_name_to_nfc() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "cafe\u{0301}" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "caf\u{00e9}");
}

#[actix_web::test]
async fn test_folder_create_rejects_windows_reserved_name() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "CON" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn test_folder_rename_normalizes_nfd_name_and_rejects_windows_reserved_name() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Workspace" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "cafe\u{0301}" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "caf\u{00e9}");

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "LPT1" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn test_folder_name_validation_returns_400() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Valid" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "bad/name" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn test_folder_repo_find_ancestors_returns_full_chain() {
    let state = common::setup().await;
    let db = state.db.clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Projects" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let projects_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "2026", "parent_id": projects_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let year_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Q1", "parent_id": year_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let quarter_id = body["data"]["id"].as_i64().unwrap();

    let user = user_repo::find_by_username(&db, "testuser")
        .await
        .unwrap()
        .expect("registered user should exist");
    let ancestors = folder_repo::find_ancestors(&db, user.id, quarter_id)
        .await
        .unwrap();

    assert_eq!(
        ancestors,
        vec![
            (projects_id, "Projects".to_string()),
            (year_id, "2026".to_string()),
            (quarter_id, "Q1".to_string()),
        ]
    );
}

#[actix_web::test]
async fn test_folder_list_items_are_lightweight_and_info_endpoint_returns_full_details() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Projects" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    let file_id = upload_test_file!(app, token);

    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let folder = &body["data"]["folders"][0];
    let file = &body["data"]["files"][0];
    assert_eq!(folder["id"], folder_id);
    assert!(folder["created_at"].is_null());
    assert!(folder["parent_id"].is_null());
    assert!(folder["policy_id"].is_null());
    assert!(folder.get("user_id").is_none());
    assert_eq!(file["id"], file_id);
    assert!(file["blob_id"].is_null());
    assert!(file["created_at"].is_null());
    assert!(file["folder_id"].is_null());
    assert!(file.get("user_id").is_none());

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{folder_id}/info"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["id"], folder_id);
    assert_eq!(body["data"]["name"], "Projects");
    assert!(body["data"]["created_at"].is_string());
    assert!(body["data"]["parent_id"].is_null());
    assert!(body["data"].get("user_id").is_none());
    assert!(body["data"]["owner_user_id"].as_i64().unwrap() > 0);
}

#[actix_web::test]
async fn test_folder_copy() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 创建源文件夹 + 里面放个文件
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Source" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let src_id = body["data"]["id"].as_i64().unwrap();

    let boundary = "----TestBoundary123";
    let payload = "------TestBoundary123\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"inside.txt\"\r\n\
         Content-Type: text/plain\r\n\r\n\
         folder content\r\n\
         ------TestBoundary123--\r\n";
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/upload?folder_id={src_id}"))
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

    // 复制文件夹到根目录（null = root，与根目录同名冲突时应递增）
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/folders/{src_id}/copy"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "Source (1)");
    assert!(body["data"]["parent_id"].is_null());
    let copy_id = body["data"]["id"].as_i64().unwrap();

    // 副本文件夹里应该有文件
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{copy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["files"][0]["name"], "inside.txt");
}

/// 测试多层嵌套文件夹复制（batch_duplicate_file_records）
#[actix_web::test]
async fn test_nested_folder_copy() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 创建 Source/A/B 三层嵌套，每层各一个文件
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Source" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let source_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "A", "parent_id": source_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let a_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "B", "parent_id": a_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let b_id = body["data"]["id"].as_i64().unwrap();

    upload_test_file_to_folder!(app, token, a_id);
    upload_test_file_to_folder!(app, token, b_id);

    // 复制顶层文件夹 A → 根目录（null = root，应保留原名）
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/folders/{a_id}/copy"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "A");
    assert!(body["data"]["parent_id"].is_null());
    let a_copy_id = body["data"]["id"].as_i64().unwrap();

    // A-copy 里应有 1 个文件 + 1 个子文件夹
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{a_copy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["files"].as_array().unwrap().len(),
        1,
        "A copy in root should have 1 file"
    );
    assert_eq!(
        body["data"]["folders"].as_array().unwrap().len(),
        1,
        "A-copy should have 1 subfolder"
    );

    // B-copy 里也应有 1 个文件
    let b_copy_id = body["data"]["folders"][0]["id"].as_i64().unwrap();
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{b_copy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["files"].as_array().unwrap().len(),
        1,
        "B-copy should have 1 file"
    );

    // 源文件夹和副本独立：删副本不影响源
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{a_copy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{a_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["files"].as_array().unwrap().len(),
        1,
        "original A should still have its file"
    );
}

#[actix_web::test]
async fn test_folder_copy_quota_failure_does_not_materialize_nested_descendants() {
    use sea_orm::{ActiveModelTrait, Set};

    let state = common::setup().await;
    let db = state.db.clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "QuotaSource" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let source_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Nested", "parent_id": source_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    upload_test_file_to_folder!(app, token, source_id);
    upload_test_file_to_folder!(app, token, source_id);

    let user = user_repo::find_by_username(&db, "testuser")
        .await
        .unwrap()
        .unwrap();
    let storage_used = user.storage_used;
    let mut user_active: aster_drive::entities::user::ActiveModel = user.into();
    user_active.storage_quota = Set(storage_used);
    user_active.update(&db).await.unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/folders/{source_id}/copy"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 507);
    let body: Value = test::read_body_json(resp).await;
    assert!(
        body["msg"].as_str().unwrap_or_default().contains("quota"),
        "quota error should be surfaced to the client"
    );

    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let root_folders = body["data"]["folders"].as_array().unwrap();

    if let Some(copy_folder) = root_folders
        .iter()
        .find(|folder| folder["name"].as_str() == Some("QuotaSource (1)"))
    {
        let copy_folder_id = copy_folder["id"].as_i64().unwrap();
        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/folders/{copy_folder_id}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: Value = test::read_body_json(resp).await;
        assert!(
            body["data"]["files"].as_array().unwrap().is_empty(),
            "quota failure should not leave copied files in the exposed copy shell"
        );
        assert!(
            body["data"]["folders"].as_array().unwrap().is_empty(),
            "quota failure should not expose nested descendant folders in the copy shell"
        );
    }

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{source_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 2);
    assert_eq!(body["data"]["folders"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["folders"][0]["name"], "Nested");
}

#[actix_web::test]
async fn test_folder_patch_can_move_to_root_with_null() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Parent" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let parent_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Child", "parent_id": parent_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let child_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/folders/{child_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"]["parent_id"].is_null());

    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let root_folders = body["data"]["folders"].as_array().unwrap();
    assert!(
        root_folders
            .iter()
            .any(|folder| folder["id"].as_i64() == Some(child_id)),
        "child folder should be moved back to root"
    );

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{parent_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["folders"].as_array().unwrap().len(), 0);
}

#[actix_web::test]
async fn test_folder_copy_preserves_policy_ids() {
    let state = common::setup().await;
    let db = state.db.clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Folder Copy Policy Root",
            "driver_type": "local",
            "base_path": "/tmp/test-folder-copy-policy-root",
            "max_file_size": 0,
            "is_default": false
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let root_policy_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Folder Copy Policy Child",
            "driver_type": "local",
            "base_path": "/tmp/test-folder-copy-policy-child",
            "max_file_size": 0,
            "is_default": false
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let child_policy_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "PolicySource" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let source_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Nested", "parent_id": source_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let nested_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/folders/{source_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "policy_id": root_policy_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/folders/{nested_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "policy_id": child_policy_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/folders/{source_id}/copy"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let copy_id = body["data"]["id"].as_i64().unwrap();

    let copied_root = folder_repo::find_by_id(&db, copy_id).await.unwrap();
    assert_eq!(copied_root.policy_id, Some(root_policy_id));

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{copy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let copied_nested_id = body["data"]["folders"][0]["id"].as_i64().unwrap();

    let copied_nested = folder_repo::find_by_id(&db, copied_nested_id)
        .await
        .unwrap();
    assert_eq!(copied_nested.policy_id, Some(child_policy_id));
}

#[actix_web::test]
async fn test_deleted_folder_name_can_be_reused_and_restore_rejects_active_conflict() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "restore-conflict-folder" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let deleted_folder_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{deleted_folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "restore-conflict-folder" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/trash/folder/{deleted_folder_id}/restore"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

//! 集成测试：`trash`。

#[macro_use]
mod common;
use aster_drive::runtime::SharedRuntimeState;

use actix_web::test;
use serde_json::Value;

fn write_temp_fixture(name: &str, contents: &str) -> String {
    let dir = format!("/tmp/asterdrive-trash-test-{}", uuid::Uuid::new_v4());
    std::fs::create_dir_all(&dir).unwrap();
    let path = format!("{dir}/{name}");
    std::fs::write(&path, contents).unwrap();
    path
}

#[actix_web::test]
async fn test_trash_restore_purge() {
    let state = common::setup().await;
    let mut rx = state.storage_change_bus.subscribe();
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);
    let upload_event = rx.recv().await.expect("upload should publish event");
    assert_eq!(
        upload_event.kind,
        aster_drive::services::events::storage_change::StorageChangeKind::FileCreated
    );
    assert!(upload_event.affects_quota);
    assert_eq!(
        upload_event.storage_delta,
        Some("test content".len() as i64)
    );

    // 软删除
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let trash_event = rx.recv().await.expect("soft delete should publish event");
    assert_eq!(
        trash_event.kind,
        aster_drive::services::events::storage_change::StorageChangeKind::FileTrashed
    );
    assert!(!trash_event.affects_quota);
    assert_eq!(trash_event.storage_delta, None);

    // 列出回收站
    let req = test::TestRequest::get()
        .uri("/api/v1/trash")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);
    let trash_file = &body["data"]["files"][0];
    assert!(trash_file["expires_at"].is_string());
    assert!(trash_file.get("deleted_at").is_none());

    // 恢复
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/trash/file/{file_id}/restore"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let restore_event = rx.recv().await.expect("restore should publish event");
    assert_eq!(
        restore_event.kind,
        aster_drive::services::events::storage_change::StorageChangeKind::FileRestoredFromTrash
    );
    assert!(!restore_event.affects_quota);
    assert_eq!(restore_event.storage_delta, None);

    // 文件可访问
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // 再次软删除 → purge 永久删除
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    test::call_service(&app, req).await;
    let _trash_again_event = rx.recv().await.expect("soft delete should publish event");

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/trash/file/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let purge_event = rx.recv().await.expect("purge should publish event");
    assert_eq!(
        purge_event.kind,
        aster_drive::services::events::storage_change::StorageChangeKind::FilePurged
    );
    assert!(purge_event.affects_quota);
    assert_eq!(
        purge_event.storage_delta,
        Some(-("test content".len() as i64))
    );

    // 回收站为空
    let req = test::TestRequest::get()
        .uri("/api/v1/trash")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 0);
}

#[actix_web::test]
async fn test_restore_file_rejects_active_name_conflict() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file_named!(app, token, "restore-conflict.txt");

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let replacement_id = upload_test_file_named!(app, token, "restore-conflict.txt");

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/trash/file/{file_id}/restore"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["msg"],
        "file 'restore-conflict.txt' already exists in this folder"
    );

    let req = test::TestRequest::get()
        .uri("/api/v1/trash")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["files"][0]["id"].as_i64().unwrap(), file_id);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{replacement_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_trash_purge_all() {
    use aster_drive::db::repository::user_repo;
    use aster_drive::services::events::storage_change::StorageChangeKind;

    let state = common::setup().await;
    let mut rx = state.storage_change_bus.subscribe();
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    // 上传两个文件
    let f1 = upload_test_file!(app, token);
    let _upload_f1_event = rx.recv().await.expect("first upload should publish event");
    // 第二个用不同名字
    let boundary = "----TestBoundary123";
    let payload = "------TestBoundary123\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"second.txt\"\r\n\
         Content-Type: text/plain\r\n\r\n\
         second\r\n\
         ------TestBoundary123--\r\n";
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
    let body: Value = test::read_body_json(resp).await;
    let f2 = body["data"]["id"].as_i64().unwrap();
    let _upload_f2_event = rx.recv().await.expect("second upload should publish event");

    // 软删除两个
    for fid in [f1, f2] {
        let req = test::TestRequest::delete()
            .uri(&format!("/api/v1/files/{fid}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .to_request();
        test::call_service(&app, req).await;
        let _trash_event = rx.recv().await.expect("soft delete should publish event");
    }

    // 回收站有 2 个
    let req = test::TestRequest::get()
        .uri("/api/v1/trash")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 2);
    let owner_before = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        owner_before.storage_used,
        "test content".len() as i64 + "second".len() as i64
    );

    // purge all creates a background task and does not synchronously scan/purge
    // the whole trash inside the HTTP request.
    let req = test::TestRequest::delete()
        .uri("/api/v1/trash")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let task_id = body["data"]["id"].as_i64().unwrap();
    assert_eq!(body["data"]["kind"], "trash_purge_all");
    assert_eq!(body["data"]["status"], "pending");
    assert_eq!(body["data"]["payload"]["kind"], "trash_purge_all");

    let req = test::TestRequest::delete()
        .uri("/api/v1/trash")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let duplicate_task_id = body["data"]["id"].as_i64().unwrap();
    assert_ne!(duplicate_task_id, task_id);
    assert_eq!(body["data"]["kind"], "trash_purge_all");
    assert_eq!(body["data"]["status"], "pending");

    // The request has only scheduled the work, so the trash is still present
    // until the dispatcher runs.
    let req = test::TestRequest::get()
        .uri("/api/v1/trash")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 2);

    let stats = aster_drive::services::task::drain(&state)
        .await
        .expect("trash purge task drain should succeed");
    assert_eq!(stats.succeeded, 2);
    let purge_all_event = rx
        .try_recv()
        .expect("purge-all task should publish one storage sync event");
    assert_eq!(purge_all_event.kind, StorageChangeKind::SyncRequired);
    assert!(purge_all_event.file_ids.is_empty());
    assert!(purge_all_event.folder_ids.is_empty());
    assert!(purge_all_event.affected_parent_ids.is_empty());
    assert!(purge_all_event.affects_quota);
    assert_eq!(
        purge_all_event.storage_delta,
        Some(-(("test content".len() + "second".len()) as i64))
    );
    assert!(
        rx.try_recv().is_err(),
        "trash purge-all should not publish one SSE event per purged entry"
    );

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/tasks/{task_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "succeeded");
    assert_eq!(body["data"]["result"]["kind"], "trash_purge_all");
    assert_eq!(body["data"]["result"]["purged"], 2);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/tasks/{duplicate_task_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "succeeded");
    assert_eq!(body["data"]["result"]["kind"], "trash_purge_all");
    assert_eq!(
        body["data"]["result"]["purged"], 0,
        "a duplicate empty-trash task should become a no-op after the first task drains the trash"
    );

    // 回收站为空
    let req = test::TestRequest::get()
        .uri("/api/v1/trash")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 0);
    let owner_after = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(owner_after.storage_used, 0);
}

#[actix_web::test]
async fn test_empty_trash_purge_all_task_succeeds_with_zero_result() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::delete()
        .uri("/api/v1/trash")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let task_id = body["data"]["id"].as_i64().unwrap();
    assert_eq!(body["data"]["kind"], "trash_purge_all");
    assert_eq!(body["data"]["status"], "pending");

    let stats = aster_drive::services::task::drain(&state)
        .await
        .expect("empty trash purge task drain should succeed");
    assert_eq!(stats.succeeded, 1);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/tasks/{task_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "succeeded");
    assert_eq!(body["data"]["result"]["kind"], "trash_purge_all");
    assert_eq!(body["data"]["result"]["purged"], 0);
    assert_eq!(body["data"]["progress_percent"], 100);
}

/// 测试嵌套文件夹的 purge：删除顶层文件夹后 purge，子文件夹和子文件都应被彻底清理
#[actix_web::test]
async fn test_purge_nested_folder_cleans_children() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 创建 parent/child 文件夹结构
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "parent" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let parent_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "child", "parent_id": parent_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let child_id = body["data"]["id"].as_i64().unwrap();

    // 在 child 内上传文件
    let file_id = upload_test_file_to_folder!(app, token, child_id);

    // 软删除顶层文件夹（会递归标记 child 和文件）
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{parent_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // purge 顶层文件夹
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/trash/folder/{parent_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // 回收站完全为空（子文件夹和子文件都已递归清理）
    let req = test::TestRequest::get()
        .uri("/api/v1/trash")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["folders"].as_array().unwrap().len(), 0);
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 0);

    // 子文件应已被硬删除（404）
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        404,
        "child file should be permanently deleted"
    );
}

/// 测试 purge_all 三层嵌套：所有子项都应被清理
#[actix_web::test]
async fn test_purge_all_nested_no_orphans() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    // 创建 A/B/C 三层嵌套
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "A" }))
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

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "C", "parent_id": b_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let c_id = body["data"]["id"].as_i64().unwrap();

    // 每层各上传一个文件
    upload_test_file_to_folder!(app, token, a_id);
    upload_test_file_to_folder!(app, token, b_id);
    let c_file_id = upload_test_file_to_folder!(app, token, c_id);

    // 根目录散文件
    let root_file_id = upload_test_file!(app, token);

    // 软删除 A + 散文件
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{a_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    test::call_service(&app, req).await;

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/files/{root_file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    test::call_service(&app, req).await;

    // purge all is scheduled and then executed by the background dispatcher.
    let req = test::TestRequest::delete()
        .uri("/api/v1/trash")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["kind"], "trash_purge_all");

    let stats = aster_drive::services::task::drain(&state)
        .await
        .expect("trash purge task drain should succeed");
    assert_eq!(stats.succeeded, 1);

    // 回收站完全为空
    let req = test::TestRequest::get()
        .uri("/api/v1/trash")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["folders"].as_array().unwrap().len(), 0);
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 0);

    // 最深层文件也应 404
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{c_file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

/// 测试嵌套文件夹软删除（batch soft_delete）：子文件夹和文件都应进入回收站
#[actix_web::test]
async fn test_soft_delete_nested_folder_marks_all_children() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 创建 A/B 两层嵌套
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "A" }))
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

    // 每层各上传一个文件
    let a_file = upload_test_file_to_folder!(app, token, a_id);
    let b_file = upload_test_file_to_folder!(app, token, b_id);

    // 软删除顶层文件夹
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{a_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // 子文件应不可访问（在回收站里）
    for fid in [a_file, b_file] {
        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/files/{fid}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            404,
            "file {fid} should be in trash (not accessible)"
        );
    }

    // 根目录应为空（所有内容已进回收站）
    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["folders"].as_array().unwrap().len(), 0);
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 0);

    // 回收站应有顶层文件夹
    let req = test::TestRequest::get()
        .uri("/api/v1/trash")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["folders"].as_array().unwrap().len(), 1);

    // 恢复后所有子项都回来
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/trash/folder/{a_id}/restore"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // A 文件夹里应有文件和子文件夹
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{a_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["folders"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);

    // B 文件夹里也应有文件
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{b_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);
}

#[actix_web::test]
async fn test_restore_file_moves_to_root_when_original_folder_is_deleted() {
    use aster_drive::db::repository::file_repo;

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "restore-parent" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    let file_id = upload_test_file_to_folder!(app, token, folder_id);

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/files/{file_id}"))
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

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/trash/file/{file_id}/restore"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let restored = file_repo::find_by_id(&db, file_id).await.unwrap();
    assert_eq!(restored.folder_id, None);
    assert!(restored.deleted_at.is_none());
}

#[actix_web::test]
async fn test_restore_folder_moves_to_root_when_parent_is_deleted() {
    use aster_drive::db::repository::{file_repo, folder_repo};

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "restore-root-parent" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let parent_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "restore-child", "parent_id": parent_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let child_id = body["data"]["id"].as_i64().unwrap();

    let child_file_id = upload_test_file_to_folder!(app, token, child_id);

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{child_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{parent_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/trash/folder/{child_id}/restore"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let restored_folder = folder_repo::find_by_id(&db, child_id).await.unwrap();
    assert_eq!(restored_folder.parent_id, None);
    assert!(restored_folder.deleted_at.is_none());

    let restored_file = file_repo::find_by_id(&db, child_file_id).await.unwrap();
    assert_eq!(restored_file.folder_id, Some(child_id));
    assert!(restored_file.deleted_at.is_none());
}

#[actix_web::test]
async fn test_cleanup_expired_falls_back_to_default_retention_for_invalid_config() {
    use aster_drive::db::repository::{config_repo, file_repo, folder_repo};
    use aster_drive::services::{files::file, files::folder, files::trash};
    use chrono::{Duration, Utc};
    use sea_orm::{ActiveModelTrait, Set};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "trashcleanup",
        "trashcleanup@example.com",
        "pass1234",
    )
    .await
    .unwrap();

    let root_path = write_temp_fixture("expired-root.txt", "expired root file");
    let root_file = file::store_from_temp(
        &state,
        user.id,
        file::StoreFromTempRequest::new(
            None,
            "expired-root.txt",
            &root_path,
            "expired root file".len() as i64,
        ),
    )
    .await
    .unwrap();

    let folder = folder::create(&state, user.id, "expired-folder", None)
        .await
        .unwrap();
    let nested_path = write_temp_fixture("expired-nested.txt", "expired nested file");
    let nested_file = file::store_from_temp(
        &state,
        user.id,
        file::StoreFromTempRequest::new(
            Some(folder.id),
            "expired-nested.txt",
            &nested_path,
            "expired nested file".len() as i64,
        ),
    )
    .await
    .unwrap();

    file::delete(&state, root_file.id, user.id).await.unwrap();
    folder::delete(&state, folder.id, user.id).await.unwrap();

    config_repo::upsert(
        state.writer_db(),
        "trash_retention_days",
        "not-a-number",
        user.id,
    )
    .await
    .unwrap();

    let expired_at = Utc::now() - Duration::days(8);

    let mut deleted_root: aster_drive::entities::file::ActiveModel =
        file_repo::find_by_id(state.writer_db(), root_file.id)
            .await
            .unwrap()
            .into();
    deleted_root.deleted_at = Set(Some(expired_at));
    deleted_root.update(state.writer_db()).await.unwrap();

    let mut deleted_nested: aster_drive::entities::file::ActiveModel =
        file_repo::find_by_id(state.writer_db(), nested_file.id)
            .await
            .unwrap()
            .into();
    deleted_nested.deleted_at = Set(Some(expired_at));
    deleted_nested.update(state.writer_db()).await.unwrap();

    let mut deleted_folder: aster_drive::entities::folder::ActiveModel =
        folder_repo::find_by_id(state.writer_db(), folder.id)
            .await
            .unwrap()
            .into();
    deleted_folder.deleted_at = Set(Some(expired_at));
    deleted_folder.update(state.writer_db()).await.unwrap();

    let purged = trash::cleanup_expired(&state).await.unwrap();
    assert_eq!(purged, 3);
    assert!(
        file_repo::find_by_id(state.writer_db(), root_file.id)
            .await
            .is_err()
    );
    assert!(
        file_repo::find_by_id(state.writer_db(), nested_file.id)
            .await
            .is_err()
    );
    assert!(
        folder_repo::find_by_id(state.writer_db(), folder.id)
            .await
            .is_err()
    );
}

#[actix_web::test]
async fn test_cleanup_expired_only_counts_top_level_deleted_folders() {
    use aster_drive::db::repository::folder_repo;
    use aster_drive::services::{files::folder, files::trash};
    use chrono::{Duration, Utc};
    use sea_orm::{ActiveModelTrait, Set};

    let state = common::setup().await;
    let user =
        common::create_test_account(&state, "trashnested", "trashnested@example.com", "pass1234")
            .await
            .unwrap();

    let parent = folder::create(&state, user.id, "expired-parent", None)
        .await
        .unwrap();
    let child = folder::create(&state, user.id, "expired-child", Some(parent.id))
        .await
        .unwrap();

    folder::delete(&state, parent.id, user.id).await.unwrap();

    let expired_at = Utc::now() - Duration::days(8);
    for folder_id in [parent.id, child.id] {
        let mut folder: aster_drive::entities::folder::ActiveModel =
            folder_repo::find_by_id(state.writer_db(), folder_id)
                .await
                .unwrap()
                .into();
        folder.deleted_at = Set(Some(expired_at));
        folder.update(state.writer_db()).await.unwrap();
    }

    let purged = trash::cleanup_expired(&state).await.unwrap();
    assert_eq!(
        purged, 1,
        "only the top-level expired folder should be counted"
    );
    assert!(
        folder_repo::find_by_id(state.writer_db(), parent.id)
            .await
            .is_err()
    );
    assert!(
        folder_repo::find_by_id(state.writer_db(), child.id)
            .await
            .is_err()
    );
}

#[actix_web::test]
async fn test_cleanup_expired_keeps_recently_deleted_items() {
    use aster_drive::db::repository::file_repo;
    use aster_drive::services::{files::file, files::trash};

    let state = common::setup().await;
    let user =
        common::create_test_account(&state, "trashrecent", "trashrecent@example.com", "pass1234")
            .await
            .unwrap();

    let temp_path = write_temp_fixture("recent-trash.txt", "recent trash");
    let file = file::store_from_temp(
        &state,
        user.id,
        file::StoreFromTempRequest::new(
            None,
            "recent-trash.txt",
            &temp_path,
            "recent trash".len() as i64,
        ),
    )
    .await
    .unwrap();

    file::delete(&state, file.id, user.id).await.unwrap();

    let purged = trash::cleanup_expired(&state).await.unwrap();
    assert_eq!(purged, 0);

    let trashed = file_repo::find_by_id(state.writer_db(), file.id)
        .await
        .unwrap();
    assert!(trashed.deleted_at.is_some());
}

#[actix_web::test]
async fn test_purge_all_processes_multiple_file_batches() {
    use aster_drive::services::{files::file, files::trash};

    let state = common::setup().await;
    let user =
        common::create_test_account(&state, "tbfiles", "trashbatchfiles@example.com", "pass1234")
            .await
            .unwrap();

    for idx in 0..120 {
        let file = file::create_empty(&state, user.id, None, &format!("batch-{idx}.txt"))
            .await
            .unwrap();
        file::delete(&state, file.id, user.id).await.unwrap();
    }

    let purged = trash::purge_all(&state, user.id).await.unwrap();
    assert_eq!(purged, 120);

    let trash = trash::list_trash(&state, user.id, 50, 0, 50, None)
        .await
        .unwrap();
    assert_eq!(trash.files_total, 0);
    assert_eq!(trash.folders_total, 0);
}

#[actix_web::test]
async fn test_purge_all_processes_multiple_folder_batches() {
    use aster_drive::services::{files::folder, files::trash};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "tbfolders",
        "trashbatchfolders@example.com",
        "pass1234",
    )
    .await
    .unwrap();

    for idx in 0..120 {
        let folder = folder::create(&state, user.id, &format!("batch-folder-{idx}"), None)
            .await
            .unwrap();
        folder::delete(&state, folder.id, user.id).await.unwrap();
    }

    let purged = trash::purge_all(&state, user.id).await.unwrap();
    assert_eq!(purged, 120);

    let trash = trash::list_trash(&state, user.id, 50, 0, 50, None)
        .await
        .unwrap();
    assert_eq!(trash.files_total, 0);
    assert_eq!(trash.folders_total, 0);
}

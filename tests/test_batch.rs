//! 集成测试：`batch`。

#[macro_use]
mod common;
use aster_drive::api::api_error_code::ApiErrorCode;
use aster_drive::entities::{file, folder};
use aster_drive::runtime::SharedRuntimeState;

use actix_web::test;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, sea_query::Expr};
use serde_json::Value;

fn upload_named_file(name: &str, content: &str, mime: &str, boundary: &str) -> String {
    format!(
        "--{boundary}\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\n\
         Content-Type: {mime}\r\n\r\n\
         {content}\r\n\
         --{boundary}--\r\n"
    )
}

#[actix_web::test]
async fn test_batch_delete_files() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let boundary = "----TestBoundary123";

    // Upload 3 files
    let mut file_ids = Vec::new();
    for name in ["file1.txt", "file2.txt", "file3.txt"] {
        let payload =
            upload_named_file(name, &format!("content of {name}"), "text/plain", boundary);
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
        file_ids.push(body["data"]["id"].as_i64().unwrap());
    }

    // Batch delete first two files
    let req = test::TestRequest::post()
        .uri("/api/v1/batch/delete")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [file_ids[0], file_ids[1]],
            "folder_ids": []
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "success");
    assert_eq!(body["data"]["succeeded"], 2);

    // 批量删除后应能重新创建同名文件
    for name in ["file1.txt", "file2.txt"] {
        let payload = upload_named_file(name, &format!("recreated {name}"), "text/plain", boundary);
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
        assert_eq!(resp.status(), 201, "recreating {name} should succeed");
    }
    assert_eq!(body["data"]["failed"], 0);

    // Third file should still be accessible
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{}", file_ids[2]))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_batch_delete_mixed() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let boundary = "----TestBoundary123";

    // Upload a file
    let payload = upload_named_file("mixed1.txt", "content1", "text/plain", boundary);
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
    let file_id = body["data"]["id"].as_i64().unwrap();

    // Create a folder
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "MixedFolder", "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    // Batch delete one file + one folder
    let req = test::TestRequest::post()
        .uri("/api/v1/batch/delete")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [file_id],
            "folder_ids": [folder_id]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "success");
    assert_eq!(body["data"]["succeeded"], 2);
    assert_eq!(body["data"]["failed"], 0);
}

#[actix_web::test]
async fn test_batch_move_files() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Source", "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let source_id = body["data"]["id"].as_i64().unwrap();

    let boundary = "----TestBoundary123";

    // Upload 2 files in source folder
    let mut file_ids = Vec::new();
    for name in ["move1.txt", "move2.txt"] {
        let payload =
            upload_named_file(name, &format!("content of {name}"), "text/plain", boundary);
        let req = test::TestRequest::post()
            .uri(&format!("/api/v1/files/upload?folder_id={source_id}"))
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
        file_ids.push(body["data"]["id"].as_i64().unwrap());
    }

    // Create target folder
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Target", "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let target_id = body["data"]["id"].as_i64().unwrap();

    // Batch move both files into target folder
    let req = test::TestRequest::post()
        .uri("/api/v1/batch/move")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": file_ids,
            "folder_ids": [],
            "target_folder_id": target_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "success");
    assert_eq!(body["data"]["succeeded"], 2);

    // Verify files are now in target folder
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{target_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 2);

    // Source folder should have no files now
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{source_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 0);

    // 批量移动后，原目录应能重新创建同名文件
    let payload = upload_named_file("move1.txt", "recreated move1", "text/plain", boundary);
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/upload?folder_id={source_id}"))
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

    // Batch move both files back to root (null = root)
    let req = test::TestRequest::post()
        .uri("/api/v1/batch/move")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": file_ids,
            "folder_ids": [],
            "target_folder_id": null
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "success");
    assert_eq!(body["data"]["succeeded"], 2);

    // Root should have the files again
    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 2);

    // Target folder should be empty again
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{target_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 0);
}

#[actix_web::test]
async fn test_batch_copy_files() {
    let state = common::setup().await;
    let storage_change_tx = state.storage_change_tx.clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Source", "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let source_id = body["data"]["id"].as_i64().unwrap();

    let boundary = "----TestBoundary123";

    // Upload 2 files in source folder
    let mut file_ids = Vec::new();
    for name in ["copy1.txt", "copy2.txt"] {
        let payload =
            upload_named_file(name, &format!("content of {name}"), "text/plain", boundary);
        let req = test::TestRequest::post()
            .uri(&format!("/api/v1/files/upload?folder_id={source_id}"))
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
        file_ids.push(body["data"]["id"].as_i64().unwrap());
    }

    // Batch copy both files to root (null = root)
    let mut rx = storage_change_tx.subscribe();
    let req = test::TestRequest::post()
        .uri("/api/v1/batch/copy")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": file_ids,
            "folder_ids": [],
            "target_folder_id": null
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "success");
    assert_eq!(body["data"]["succeeded"], 2);
    let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await
        .expect("should receive storage change event")
        .expect("storage change event channel should stay open");
    assert_eq!(
        event.kind,
        aster_drive::services::storage_change_service::StorageChangeKind::FileCreated
    );
    assert_eq!(event.file_ids.len(), 2);
    assert!(event.folder_ids.is_empty());
    assert!(event.root_affected);

    // Verify copies exist in root
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

    // Originals should still be in source folder
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{source_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 2);
}

#[actix_web::test]
async fn test_batch_move_preserves_per_item_conflict_reporting() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "SourceA", "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let source_a = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "SourceB", "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let source_b = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "TargetConflict", "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let target_id = body["data"]["id"].as_i64().unwrap();

    let boundary = "----TestBoundary123";
    let payload = upload_named_file("dup.txt", "same-name", "text/plain", boundary);
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/upload?folder_id={source_a}"))
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
    let file_a = body["data"]["id"].as_i64().unwrap();

    let payload = upload_named_file("dup.txt", "same-name", "text/plain", boundary);
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/upload?folder_id={source_b}"))
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
    let file_b = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/batch/move")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [file_a, file_b],
            "folder_ids": [],
            "target_folder_id": target_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["succeeded"], 1);
    assert_eq!(body["data"]["failed"], 1);
    assert_eq!(body["data"]["errors"][0]["entity_type"], "file");
    assert_eq!(body["data"]["errors"][0]["entity_id"], file_b);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{target_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let target_files = body["data"]["files"].as_array().unwrap();
    assert_eq!(target_files.len(), 1);
    assert_eq!(target_files[0]["name"], "dup.txt");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{source_b}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);
}

#[actix_web::test]
async fn test_batch_move_detects_target_conflicts_without_full_target_listing() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    macro_rules! create_folder {
        ($name:expr, $parent_id:expr) => {{
            let req = test::TestRequest::post()
                .uri("/api/v1/folders")
                .insert_header(("Cookie", common::access_cookie_header(&token)))
                .insert_header(common::csrf_header_for(&token))
                .set_json(serde_json::json!({ "name": $name, "parent_id": $parent_id }))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 201);
            let body: Value = test::read_body_json(resp).await;
            body["data"]["id"].as_i64().unwrap()
        }};
    }

    macro_rules! upload_file {
        ($folder_id:expr, $name:expr, $content:expr) => {{
            let boundary = "----TestBoundary123";
            let payload = upload_named_file($name, $content, "text/plain", boundary);
            let req = test::TestRequest::post()
                .uri(&format!("/api/v1/files/upload?folder_id={}", $folder_id))
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
            body["data"]["id"].as_i64().unwrap()
        }};
    }

    let source_id = create_folder!("ConflictSource", Value::Null);
    let target_id = create_folder!("ConflictTarget", Value::Null);

    for index in 0..20 {
        upload_file!(
            target_id,
            &format!("unrelated-{index}.txt"),
            &format!("unrelated {index}")
        );
        create_folder!(&format!("UnrelatedFolder{index}"), target_id);
    }

    let target_file_conflict = upload_file!(target_id, "Cafe\u{301}.txt", "existing target file");
    let target_folder_conflict = create_folder!("Cafe\u{301}Folder", target_id);
    file::Entity::update_many()
        .col_expr(file::Column::Name, Expr::value("Cafe\u{301}.txt"))
        .filter(file::Column::Id.eq(target_file_conflict))
        .exec(&db)
        .await
        .unwrap();
    folder::Entity::update_many()
        .col_expr(folder::Column::Name, Expr::value("Cafe\u{301}Folder"))
        .filter(folder::Column::Id.eq(target_folder_conflict))
        .exec(&db)
        .await
        .unwrap();

    let file_ok = upload_file!(source_id, "file-ok.txt", "move me");
    let file_conflict = upload_file!(source_id, "Café.txt", "keep me in source");
    let folder_ok = create_folder!("FolderOk", source_id);
    let folder_conflict = create_folder!("CaféFolder", source_id);

    let req = test::TestRequest::post()
        .uri("/api/v1/batch/move")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [file_ok, file_conflict],
            "folder_ids": [folder_ok, folder_conflict],
            "target_folder_id": target_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["succeeded"], 2);
    assert_eq!(body["data"]["failed"], 2);

    let errors = body["data"]["errors"].as_array().unwrap();
    assert!(errors.iter().any(|error| {
        error["entity_type"] == "file" && error["entity_id"].as_i64() == Some(file_conflict)
    }));
    assert!(errors.iter().any(|error| {
        error["entity_type"] == "folder" && error["entity_id"].as_i64() == Some(folder_conflict)
    }));

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{target_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let target_file_names: Vec<&str> = body["data"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["name"].as_str())
        .collect();
    let target_folder_names: Vec<&str> = body["data"]["folders"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["name"].as_str())
        .collect();
    assert!(target_file_names.contains(&"file-ok.txt"));
    assert!(target_file_names.contains(&"Cafe\u{301}.txt"));
    assert!(target_folder_names.contains(&"FolderOk"));
    assert!(target_folder_names.contains(&"Cafe\u{301}Folder"));
    assert!(target_file_names.contains(&"unrelated-19.txt"));
    assert!(target_folder_names.contains(&"UnrelatedFolder19"));

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{source_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let source_file_names: Vec<&str> = body["data"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["name"].as_str())
        .collect();
    let source_folder_names: Vec<&str> = body["data"]["folders"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["name"].as_str())
        .collect();
    assert!(!source_file_names.contains(&"file-ok.txt"));
    assert!(source_file_names.contains(&"Café.txt"));
    assert!(!source_folder_names.contains(&"FolderOk"));
    assert!(source_folder_names.contains(&"CaféFolder"));
}

#[actix_web::test]
async fn test_batch_copy_duplicate_ids_allocate_unique_names() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "CopyDupSource", "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let source_id = body["data"]["id"].as_i64().unwrap();

    let boundary = "----TestBoundary123";
    let payload = upload_named_file("repeat.txt", "repeat-content", "text/plain", boundary);
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/upload?folder_id={source_id}"))
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
        .uri("/api/v1/batch/copy")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [file_id, file_id],
            "folder_ids": [],
            "target_folder_id": null
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["succeeded"], 2);
    assert_eq!(body["data"]["failed"], 0);

    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let mut names: Vec<String> = body["data"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap().to_string())
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec!["repeat (1).txt".to_string(), "repeat.txt".to_string()]
    );
}

#[actix_web::test]
async fn test_batch_copy_duplicate_folder_ids_allocate_unique_names_and_preserve_descendants() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "CopyTree", "parent_id": null }))
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

    let boundary = "----TestBoundary123";
    for (folder_id, name, content) in [
        (source_id, "root.txt", "root-content"),
        (nested_id, "nested.txt", "nested-content"),
    ] {
        let payload = upload_named_file(name, content, "text/plain", boundary);
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
    }

    let req = test::TestRequest::post()
        .uri("/api/v1/batch/copy")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [],
            "folder_ids": [source_id, source_id],
            "target_folder_id": null
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["succeeded"], 2);
    assert_eq!(body["data"]["failed"], 0);

    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let root_folders = body["data"]["folders"].as_array().unwrap();
    let mut copy_folder_ids = Vec::new();
    for expected_name in ["CopyTree (1)", "CopyTree (2)"] {
        let folder = root_folders
            .iter()
            .find(|folder| folder["name"] == expected_name)
            .unwrap_or_else(|| panic!("missing copied folder '{expected_name}' in root listing"));
        copy_folder_ids.push(folder["id"].as_i64().unwrap());
    }

    for copy_folder_id in copy_folder_ids {
        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/folders/{copy_folder_id}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"]["files"][0]["name"], "root.txt");
        assert_eq!(body["data"]["folders"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"]["folders"][0]["name"], "Nested");

        let copied_nested_id = body["data"]["folders"][0]["id"].as_i64().unwrap();
        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/folders/{copied_nested_id}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"]["files"][0]["name"], "nested.txt");
        assert!(body["data"]["folders"].as_array().unwrap().is_empty());
    }
}

#[actix_web::test]
async fn test_batch_delete_preserves_partial_failures_for_locked_items() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let boundary = "----TestBoundary123";

    let payload = upload_named_file("delete-ok.txt", "delete-ok", "text/plain", boundary);
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
    let file_ok = body["data"]["id"].as_i64().unwrap();

    let payload = upload_named_file("delete-locked.txt", "delete-locked", "text/plain", boundary);
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
    let file_locked = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "DeleteFolderOk", "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_ok = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "DeleteFolderLocked", "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_locked = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{file_locked}/lock"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "locked": true }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/folders/{folder_locked}/lock"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "locked": true }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri("/api/v1/batch/delete")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [file_ok, file_locked],
            "folder_ids": [folder_ok, folder_locked]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["succeeded"], 2);
    assert_eq!(body["data"]["failed"], 2);
    assert_eq!(body["data"]["errors"][0]["entity_type"], "file");
    assert_eq!(body["data"]["errors"][0]["entity_id"], file_locked);
    assert_eq!(body["data"]["errors"][1]["entity_type"], "folder");
    assert_eq!(body["data"]["errors"][1]["entity_id"], folder_locked);

    let req = test::TestRequest::get()
        .uri("/api/v1/trash")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["folders"].as_array().unwrap().len(), 1);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_locked}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{folder_locked}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_batch_move_preserves_cycle_failures_for_folders() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "CycleA", "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_a = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "CycleB", "parent_id": folder_a }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_b = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "CycleC", "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_c = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/batch/move")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [],
            "folder_ids": [folder_a, folder_c],
            "target_folder_id": folder_b
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["succeeded"], 1);
    assert_eq!(body["data"]["failed"], 1);
    assert_eq!(body["data"]["errors"][0]["entity_type"], "folder");
    assert_eq!(body["data"]["errors"][0]["entity_id"], folder_a);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{folder_b}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let children = body["data"]["folders"].as_array().unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0]["name"], "CycleC");

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
            .any(|item| item["id"].as_i64() == Some(folder_a))
    );
    assert!(
        !root_folders
            .iter()
            .any(|item| item["id"].as_i64() == Some(folder_c))
    );
}

#[actix_web::test]
async fn test_batch_move_normalizes_descendants_of_selected_folders() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "MoveParent", "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let parent_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "MoveChild", "parent_id": parent_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let child_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "MoveTarget", "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let target_id = body["data"]["id"].as_i64().unwrap();

    let boundary = "----TestBoundary123";
    let payload = upload_named_file("nested.txt", "nested content", "text/plain", boundary);
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/upload?folder_id={child_id}"))
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
        .uri("/api/v1/batch/move")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [file_id],
            "folder_ids": [parent_id, child_id],
            "target_folder_id": target_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{target_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let target_folders = body["data"]["folders"].as_array().unwrap();
    let target_files = body["data"]["files"].as_array().unwrap();
    assert_eq!(target_folders.len(), 1);
    assert_eq!(target_folders[0]["id"], parent_id);
    assert_eq!(target_folders[0]["name"], "MoveParent");
    assert!(target_files.is_empty());

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{parent_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let parent_folders = body["data"]["folders"].as_array().unwrap();
    let parent_files = body["data"]["files"].as_array().unwrap();
    assert_eq!(parent_folders.len(), 1);
    assert_eq!(parent_folders[0]["id"], child_id);
    assert_eq!(parent_folders[0]["name"], "MoveChild");
    assert!(parent_files.is_empty());

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{child_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let child_files = body["data"]["files"].as_array().unwrap();
    assert_eq!(child_files.len(), 1);
    assert_eq!(child_files[0]["id"], file_id);
    assert_eq!(child_files[0]["name"], "nested.txt");
}

#[actix_web::test]
async fn test_batch_copy_preserves_partial_failures_for_quota() {
    use sea_orm::{ActiveModelTrait, Set};

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "QuotaSource", "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let source_id = body["data"]["id"].as_i64().unwrap();

    let boundary = "----TestBoundary123";

    let payload = upload_named_file("quota-a.txt", "quota-a-content", "text/plain", boundary);
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/upload?folder_id={source_id}"))
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
    let file_a = body["data"]["id"].as_i64().unwrap();

    let payload = upload_named_file("quota-b.txt", "quota-b-content", "text/plain", boundary);
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/upload?folder_id={source_id}"))
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
    let file_b = body["data"]["id"].as_i64().unwrap();

    let user = aster_drive::db::repository::user_repo::find_by_username(&db, "testuser")
        .await
        .unwrap()
        .unwrap();
    let file_a_model = aster_drive::db::repository::file_repo::find_by_id(&db, file_a)
        .await
        .unwrap();
    let current_used = user.storage_used;

    let mut updated_user: aster_drive::entities::user::ActiveModel = user.into();
    updated_user.storage_quota = Set(current_used + file_a_model.size);
    updated_user.update(&db).await.unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/batch/copy")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [file_a, file_b],
            "folder_ids": [],
            "target_folder_id": null
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["succeeded"], 1);
    assert_eq!(body["data"]["failed"], 1);
    assert_eq!(body["data"]["errors"][0]["entity_type"], "file");
    assert_eq!(body["data"]["errors"][0]["entity_id"], file_b);

    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let root_files = body["data"]["files"].as_array().unwrap();
    assert_eq!(root_files.len(), 1);
    assert_eq!(root_files[0]["name"], "quota-a.txt");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{source_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 2);
}

#[actix_web::test]
async fn test_batch_limit_allows_1000_items() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/batch/delete")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": (1..=1000).collect::<Vec<i64>>(),
            "folder_ids": []
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_batch_limit_rejects_over_1000_items() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/batch/delete")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": (1..=1001).collect::<Vec<i64>>(),
            "folder_ids": []
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["msg"], "batch size cannot exceed 1000 items");
}

#[actix_web::test]
async fn test_batch_empty_request() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // Send batch delete with empty arrays — validation should reject
    let req = test::TestRequest::post()
        .uri("/api/v1/batch/delete")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [],
            "folder_ids": []
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    let code = body["code"].as_str().expect("error code should be string");
    assert_eq!(code, ApiErrorCode::BadRequest.as_str());
}

#[actix_web::test]
async fn test_batch_move_rejects_non_positive_target_folder_id() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    let req = test::TestRequest::post()
        .uri("/api/v1/batch/move")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [file_id],
            "folder_ids": [],
            "target_folder_id": 0
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["msg"], "target_folder_id must be greater than 0");
}

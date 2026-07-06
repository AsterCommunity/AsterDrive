//! 集成测试：`batch`。

#[macro_use]
mod common;
use aster_drive::api::api_error_code::ApiErrorCode;
use aster_drive::db::repository::folder_repo;
use aster_drive::entities::{file, folder};
use aster_drive::runtime::SharedRuntimeState;

use actix_web::test;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set, sea_query::Expr};
use serde_json::Value;

macro_rules! register_user {
    ($app:expr, $db:expr, $mail_sender:expr, $username:expr, $email:expr) => {{
        let req = test::TestRequest::post()
            .uri("/api/v1/auth/register")
            .peer_addr("127.0.0.1:12345".parse().unwrap())
            .set_json(serde_json::json!({
                "username": $username,
                "email": $email,
                "password": "password123"
            }))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 201);
        let body: Value = test::read_body_json(resp).await;
        let user_id = body["data"]["id"].as_i64().unwrap();
        let _ = confirm_latest_contact_verification!($app, $db, $mail_sender);
        user_id
    }};
}

macro_rules! login_user {
    ($app:expr, $identifier:expr) => {{
        let req = test::TestRequest::post()
            .uri("/api/v1/auth/login")
            .peer_addr("127.0.0.1:12345".parse().unwrap())
            .set_json(serde_json::json!({
                "identifier": $identifier,
                "password": "password123"
            }))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 200);
        common::extract_cookie(&resp, "aster_access").unwrap()
    }};
}

macro_rules! create_team {
    ($app:expr, $token:expr, $name:expr) => {{
        let req = test::TestRequest::post()
            .uri("/api/v1/teams")
            .insert_header(("Cookie", common::access_cookie_header(&$token)))
            .insert_header(common::csrf_header_for(&$token))
            .set_json(serde_json::json!({ "name": $name }))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 201);
        let body: Value = test::read_body_json(resp).await;
        body["data"]["id"].as_i64().unwrap()
    }};
}

macro_rules! add_team_member {
    ($app:expr, $token:expr, $team_id:expr, $user_id:expr) => {{
        let req = test::TestRequest::post()
            .uri(&format!("/api/v1/teams/{}/members", $team_id))
            .insert_header(("Cookie", common::access_cookie_header(&$token)))
            .insert_header(common::csrf_header_for(&$token))
            .set_json(serde_json::json!({ "user_id": $user_id }))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 201);
    }};
}

macro_rules! create_local_policy {
    ($app:expr, $token:expr, $name:expr, $base_path:expr) => {{
        let req = test::TestRequest::post()
            .uri("/api/v1/admin/policies")
            .insert_header(("Cookie", common::access_cookie_header(&$token)))
            .insert_header(common::csrf_header_for(&$token))
            .set_json(serde_json::json!({
                "name": $name,
                "driver_type": "local",
                "base_path": $base_path,
                "max_file_size": 0,
                "is_default": false
            }))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 201);
        let body: Value = test::read_body_json(resp).await;
        body["data"]["id"].as_i64().unwrap()
    }};
}

macro_rules! set_folder_policy {
    ($app:expr, $token:expr, $folder_id:expr, $policy_id:expr) => {{
        let req = test::TestRequest::put()
            .uri(&format!("/api/v1/admin/folders/{}/policy", $folder_id))
            .insert_header(("Cookie", common::access_cookie_header(&$token)))
            .insert_header(common::csrf_header_for(&$token))
            .set_json(serde_json::json!({ "policy_id": $policy_id }))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 200);
    }};
}

macro_rules! create_personal_folder {
    ($app:expr, $token:expr, $name:expr, $parent_id:expr) => {{
        let req = test::TestRequest::post()
            .uri("/api/v1/folders")
            .insert_header(("Cookie", common::access_cookie_header(&$token)))
            .insert_header(common::csrf_header_for(&$token))
            .set_json(serde_json::json!({
                "name": $name,
                "parent_id": $parent_id
            }))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 201);
        let body: Value = test::read_body_json(resp).await;
        body["data"]["id"].as_i64().unwrap()
    }};
}

macro_rules! create_team_folder {
    ($app:expr, $token:expr, $team_id:expr, $name:expr, $parent_id:expr) => {{
        let req = test::TestRequest::post()
            .uri(&format!("/api/v1/teams/{}/folders", $team_id))
            .insert_header(("Cookie", common::access_cookie_header(&$token)))
            .insert_header(common::csrf_header_for(&$token))
            .set_json(serde_json::json!({
                "name": $name,
                "parent_id": $parent_id
            }))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 201);
        let body: Value = test::read_body_json(resp).await;
        body["data"]["id"].as_i64().unwrap()
    }};
}

macro_rules! upload_personal_file {
    ($app:expr, $token:expr, $folder_id:expr, $name:expr, $content:expr) => {{
        let boundary = "----WorkspaceTransferBoundary";
        let payload = upload_named_file($name, $content, "text/plain", boundary);
        let uri = match $folder_id {
            Some(folder_id) => format!("/api/v1/files/upload?folder_id={folder_id}"),
            None => "/api/v1/files/upload".to_string(),
        };
        let req = test::TestRequest::post()
            .uri(&uri)
            .insert_header(("Cookie", common::access_cookie_header(&$token)))
            .insert_header(common::csrf_header_for(&$token))
            .insert_header((
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            ))
            .set_payload(payload)
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 201);
        let body: Value = test::read_body_json(resp).await;
        body["data"]["id"].as_i64().unwrap()
    }};
}

macro_rules! upload_team_file {
    ($app:expr, $token:expr, $team_id:expr, $folder_id:expr, $name:expr, $content:expr) => {{
        let boundary = "----WorkspaceTransferTeamBoundary";
        let payload = upload_named_file($name, $content, "text/plain", boundary);
        let uri = match $folder_id {
            Some(folder_id) => {
                format!(
                    "/api/v1/teams/{}/files/upload?folder_id={folder_id}",
                    $team_id
                )
            }
            None => format!("/api/v1/teams/{}/files/upload", $team_id),
        };
        let req = test::TestRequest::post()
            .uri(&uri)
            .insert_header(("Cookie", common::access_cookie_header(&$token)))
            .insert_header(common::csrf_header_for(&$token))
            .insert_header((
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            ))
            .set_payload(payload)
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 201);
        let body: Value = test::read_body_json(resp).await;
        body["data"]["id"].as_i64().unwrap()
    }};
}

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
async fn test_workspace_transfer_copy_personal_file_to_team_root_resolves_name_conflict() {
    let state = common::setup().await;
    let storage_change_tx = state.storage_change_tx.clone();
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _user_id = register_user!(
        app,
        db,
        mail_sender,
        "xcpersonalteam",
        "xcopy-personal-team@example.com"
    );
    let token = login_user!(app, "xcpersonalteam");
    let team_id = create_team!(app, token, "Cross Copy Team");
    let source_folder_id =
        create_personal_folder!(app, token, "Personal Source", Option::<i64>::None);
    let file_id = upload_personal_file!(
        app,
        token,
        Some(source_folder_id),
        "conflict.txt",
        "personal body"
    );
    upload_team_file!(
        app,
        token,
        team_id,
        None::<i64>,
        "conflict.txt",
        "existing team body"
    );

    let mut rx = storage_change_tx.subscribe();
    let req = test::TestRequest::post()
        .uri("/api/v1/workspace-transfer/copy")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "source_workspace": { "kind": "personal" },
            "file_ids": [file_id],
            "folder_ids": [],
            "destination_workspace": { "kind": "team", "team_id": team_id },
            "target_folder_id": null
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["succeeded"], 1);
    assert_eq!(body["data"]["failed"], 0);

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await
        .expect("should receive destination storage change")
        .expect("storage change channel should stay open");
    assert_eq!(
        event.workspace,
        Some(
            aster_drive::services::storage_change_service::StorageChangeWorkspace::Team { team_id }
        )
    );
    assert_eq!(
        event.kind,
        aster_drive::services::storage_change_service::StorageChangeKind::FileCreated
    );
    assert_eq!(event.file_ids.len(), 1);
    assert!(event.root_affected);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let mut names: Vec<_> = body["data"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap().to_string())
        .collect();
    names.sort();
    assert_eq!(names, vec!["conflict (1).txt", "conflict.txt"]);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{source_folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);
}

#[actix_web::test]
async fn test_workspace_transfer_copy_team_folder_to_personal_root() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _user_id = register_user!(
        app,
        db,
        mail_sender,
        "xcteampersonal",
        "xcopy-team-personal@example.com"
    );
    let token = login_user!(app, "xcteampersonal");
    let team_id = create_team!(app, token, "Team To Personal");
    let source_id = create_team_folder!(app, token, team_id, "TeamFolder", Option::<i64>::None);
    let child_id = create_team_folder!(app, token, team_id, "Nested", Some(source_id));
    upload_team_file!(
        app,
        token,
        team_id,
        Some(child_id),
        "nested.txt",
        "nested body"
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/workspace-transfer/copy")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "source_workspace": { "kind": "team", "team_id": team_id },
            "file_ids": [],
            "folder_ids": [source_id],
            "destination_workspace": { "kind": "personal" },
            "target_folder_id": null
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["succeeded"], 1);

    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let copied_id = body["data"]["folders"]
        .as_array()
        .unwrap()
        .iter()
        .find(|folder| folder["name"] == "TeamFolder")
        .and_then(|folder| folder["id"].as_i64())
        .expect("copied folder should be in personal root");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{copied_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let copied_child_id = body["data"]["folders"][0]["id"].as_i64().unwrap();
    assert_eq!(body["data"]["folders"][0]["name"], "Nested");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{copied_child_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"][0]["name"], "nested.txt");
}

#[actix_web::test]
async fn test_workspace_transfer_copy_between_team_workspaces_to_target_folder() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _user_id = register_user!(
        app,
        db,
        mail_sender,
        "xcopyteamteam",
        "xcopy-team-team@example.com"
    );
    let token = login_user!(app, "xcopyteamteam");
    let source_team_id = create_team!(app, token, "Source Team");
    let dest_team_id = create_team!(app, token, "Destination Team");
    let source_folder_id = create_team_folder!(
        app,
        token,
        source_team_id,
        "SourceFolder",
        Option::<i64>::None
    );
    let target_folder_id = create_team_folder!(
        app,
        token,
        dest_team_id,
        "TargetFolder",
        Option::<i64>::None
    );
    let file_id = upload_team_file!(
        app,
        token,
        source_team_id,
        Some(source_folder_id),
        "team-a.txt",
        "team a body"
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/workspace-transfer/copy")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "source_workspace": { "kind": "team", "team_id": source_team_id },
            "file_ids": [file_id],
            "folder_ids": [],
            "destination_workspace": { "kind": "team", "team_id": dest_team_id },
            "target_folder_id": target_folder_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["succeeded"], 1);

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/teams/{dest_team_id}/folders/{target_folder_id}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"][0]["name"], "team-a.txt");
}

#[actix_web::test]
async fn test_workspace_transfer_copy_rejects_missing_destination_access() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _owner_id = register_user!(
        app,
        db,
        mail_sender,
        "xcopydestowner",
        "xcopy-dest-owner@example.com"
    );
    let member_id = register_user!(
        app,
        db,
        mail_sender,
        "xcopydestmember",
        "xcopy-dest-member@example.com"
    );
    let owner_token = login_user!(app, "xcopydestowner");
    let member_token = login_user!(app, "xcopydestmember");
    let source_team_id = create_team!(app, owner_token, "Readable Source Team");
    let dest_team_id = create_team!(app, owner_token, "Forbidden Destination Team");
    add_team_member!(app, owner_token, source_team_id, member_id);
    let file_id = upload_team_file!(
        app,
        owner_token,
        source_team_id,
        None::<i64>,
        "source.txt",
        "source body"
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/workspace-transfer/copy")
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .set_json(serde_json::json!({
            "source_workspace": { "kind": "team", "team_id": source_team_id },
            "file_ids": [file_id],
            "folder_ids": [],
            "destination_workspace": { "kind": "team", "team_id": dest_team_id },
            "target_folder_id": null
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn test_workspace_transfer_copy_rejects_missing_source_access() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _owner_id = register_user!(
        app,
        db,
        mail_sender,
        "xcsourceowner",
        "xcopy-source-owner@example.com"
    );
    let member_id = register_user!(
        app,
        db,
        mail_sender,
        "xcsourcemember",
        "xcopy-source-member@example.com"
    );
    let owner_token = login_user!(app, "xcsourceowner");
    let member_token = login_user!(app, "xcsourcemember");
    let source_team_id = create_team!(app, owner_token, "Forbidden Source Team");
    let dest_team_id = create_team!(app, owner_token, "Writable Destination Team");
    add_team_member!(app, owner_token, dest_team_id, member_id);
    let file_id = upload_team_file!(
        app,
        owner_token,
        source_team_id,
        None::<i64>,
        "private.txt",
        "private body"
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/workspace-transfer/copy")
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .set_json(serde_json::json!({
            "source_workspace": { "kind": "team", "team_id": source_team_id },
            "file_ids": [file_id],
            "folder_ids": [],
            "destination_workspace": { "kind": "team", "team_id": dest_team_id },
            "target_folder_id": null
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn test_workspace_transfer_copy_respects_destination_quota() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _user_id = register_user!(
        app,
        db,
        mail_sender,
        "xcopyquota",
        "xcopy-quota@example.com"
    );
    let token = login_user!(app, "xcopyquota");
    let team_id = create_team!(app, token, "Quota Destination Team");
    let source_folder_id = create_personal_folder!(app, token, "Quota Source", None::<i64>);
    let first_file_id = upload_personal_file!(
        app,
        token,
        Some(source_folder_id),
        "first.txt",
        "first-body"
    );
    let second_file_id = upload_personal_file!(
        app,
        token,
        Some(source_folder_id),
        "second.txt",
        "second-body"
    );
    let first_file = aster_drive::db::repository::file_repo::find_by_id(&db, first_file_id)
        .await
        .unwrap();
    let team = aster_drive::db::repository::team_repo::find_active_by_id(&db, team_id)
        .await
        .unwrap();
    let mut active: aster_drive::entities::team::ActiveModel = team.into();
    active.storage_quota = Set(first_file.size);
    active.update(&db).await.unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/workspace-transfer/copy")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "source_workspace": { "kind": "personal" },
            "file_ids": [first_file_id, second_file_id],
            "folder_ids": [],
            "destination_workspace": { "kind": "team", "team_id": team_id },
            "target_folder_id": null
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["succeeded"], 1);
    assert_eq!(body["data"]["failed"], 1);
    assert_eq!(body["data"]["errors"][0]["entity_id"], second_file_id);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let files = body["data"]["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["name"], "first.txt");
}

#[actix_web::test]
async fn test_workspace_transfer_copy_validates_request_boundaries() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    for payload in [
        serde_json::json!({
            "source_workspace": { "kind": "personal" },
            "file_ids": [],
            "folder_ids": [],
            "destination_workspace": { "kind": "personal" },
            "target_folder_id": null
        }),
        serde_json::json!({
            "source_workspace": { "kind": "team", "team_id": 0 },
            "file_ids": [1],
            "folder_ids": [],
            "destination_workspace": { "kind": "personal" },
            "target_folder_id": null
        }),
        serde_json::json!({
            "source_workspace": { "kind": "personal" },
            "file_ids": [1],
            "folder_ids": [],
            "destination_workspace": { "kind": "personal" },
            "target_folder_id": 0
        }),
        serde_json::json!({
            "source_workspace": { "kind": "personal" },
            "file_ids": [-1],
            "folder_ids": [],
            "destination_workspace": { "kind": "personal" },
            "target_folder_id": null
        }),
    ] {
        let req = test::TestRequest::post()
            .uri("/api/v1/workspace-transfer/copy")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(payload)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }
}

#[actix_web::test]
async fn test_workspace_transfer_copy_reports_source_scope_mismatch_per_item() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _user_id = register_user!(
        app,
        db,
        mail_sender,
        "xcsrcmismatch",
        "xcopy-source-mismatch@example.com"
    );
    let token = login_user!(app, "xcsrcmismatch");
    let source_folder_id = create_personal_folder!(app, token, "Mismatch Source", None::<i64>);
    let personal_file_id = upload_personal_file!(
        app,
        token,
        Some(source_folder_id),
        "personal-only.txt",
        "personal body"
    );
    let team_id = create_team!(app, token, "Declared Source Team");

    let req = test::TestRequest::post()
        .uri("/api/v1/workspace-transfer/copy")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "source_workspace": { "kind": "team", "team_id": team_id },
            "file_ids": [personal_file_id],
            "folder_ids": [],
            "destination_workspace": { "kind": "personal" },
            "target_folder_id": null
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["succeeded"], 0);
    assert_eq!(body["data"]["failed"], 1);
    assert_eq!(body["data"]["errors"][0]["entity_type"], "file");
    assert_eq!(body["data"]["errors"][0]["entity_id"], personal_file_id);

    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"]["files"].as_array().unwrap().is_empty());
}

#[actix_web::test]
async fn test_workspace_transfer_copy_rejects_target_folder_outside_destination_scope() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _user_id = register_user!(
        app,
        db,
        mail_sender,
        "xctargetscope",
        "xcopy-target-scope@example.com"
    );
    let token = login_user!(app, "xctargetscope");
    let source_folder_id = create_personal_folder!(app, token, "Target Scope Source", None::<i64>);
    let personal_target_id = create_personal_folder!(app, token, "Personal Target", None::<i64>);
    let file_id = upload_personal_file!(
        app,
        token,
        Some(source_folder_id),
        "target-scope.txt",
        "target body"
    );
    let team_id = create_team!(app, token, "Target Scope Team");

    let req = test::TestRequest::post()
        .uri("/api/v1/workspace-transfer/copy")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "source_workspace": { "kind": "personal" },
            "file_ids": [file_id],
            "folder_ids": [],
            "destination_workspace": { "kind": "team", "team_id": team_id },
            "target_folder_id": personal_target_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["succeeded"], 0);
    assert_eq!(body["data"]["failed"], 1);
    assert_eq!(body["data"]["errors"][0]["entity_type"], "file");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"]["files"].as_array().unwrap().is_empty());
}

#[actix_web::test]
async fn test_workspace_transfer_copy_duplicate_file_ids_allocate_unique_destination_names() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _user_id = register_user!(
        app,
        db,
        mail_sender,
        "xcdupefiles",
        "xcopy-duplicate-files@example.com"
    );
    let token = login_user!(app, "xcdupefiles");
    let team_id = create_team!(app, token, "Duplicate File Destination");
    let source_folder_id = create_personal_folder!(app, token, "Duplicate Source", None::<i64>);
    let file_id = upload_personal_file!(
        app,
        token,
        Some(source_folder_id),
        "repeat.txt",
        "repeat body"
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/workspace-transfer/copy")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "source_workspace": { "kind": "personal" },
            "file_ids": [file_id, file_id],
            "folder_ids": [],
            "destination_workspace": { "kind": "team", "team_id": team_id },
            "target_folder_id": null
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["succeeded"], 2);
    assert_eq!(body["data"]["failed"], 0);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
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
    assert_eq!(names, vec!["repeat (1).txt", "repeat.txt"]);
}

#[actix_web::test]
async fn test_workspace_transfer_copy_folder_name_conflict_preserves_descendants() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _user_id = register_user!(
        app,
        db,
        mail_sender,
        "xcfolderconflict",
        "xcopy-folder-conflict@example.com"
    );
    let token = login_user!(app, "xcfolderconflict");
    let team_id = create_team!(app, token, "Folder Conflict Source");
    let source_id = create_team_folder!(app, token, team_id, "ConflictFolder", None::<i64>);
    let nested_id = create_team_folder!(app, token, team_id, "Nested", Some(source_id));
    upload_team_file!(
        app,
        token,
        team_id,
        Some(nested_id),
        "inside.txt",
        "inside body"
    );
    create_personal_folder!(app, token, "ConflictFolder", None::<i64>);

    let req = test::TestRequest::post()
        .uri("/api/v1/workspace-transfer/copy")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "source_workspace": { "kind": "team", "team_id": team_id },
            "file_ids": [],
            "folder_ids": [source_id],
            "destination_workspace": { "kind": "personal" },
            "target_folder_id": null
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["succeeded"], 1);

    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let copied_id = body["data"]["folders"]
        .as_array()
        .unwrap()
        .iter()
        .find(|folder| folder["name"] == "ConflictFolder (1)")
        .and_then(|folder| folder["id"].as_i64())
        .expect("conflicting folder copy should get a unique name");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{copied_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let copied_nested_id = body["data"]["folders"][0]["id"].as_i64().unwrap();
    assert_eq!(body["data"]["folders"][0]["name"], "Nested");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{copied_nested_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"][0]["name"], "inside.txt");
}

#[actix_web::test]
async fn test_workspace_transfer_copy_preserves_policy_only_within_same_workspace() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _user_id = register_user!(
        app,
        db,
        mail_sender,
        "xcpolicyscope",
        "xcopy-policy-scope@example.com"
    );
    let token = login_user!(app, "xcpolicyscope");
    let root_policy_id = create_local_policy!(
        app,
        token,
        "Workspace Transfer Root Policy",
        "/tmp/test-workspace-transfer-root-policy"
    );
    let child_policy_id = create_local_policy!(
        app,
        token,
        "Workspace Transfer Child Policy",
        "/tmp/test-workspace-transfer-child-policy"
    );
    let source_id = create_personal_folder!(app, token, "PolicyFolder", Option::<i64>::None);
    let child_id = create_personal_folder!(app, token, "PolicyChild", Some(source_id));
    set_folder_policy!(app, token, source_id, root_policy_id);
    set_folder_policy!(app, token, child_id, child_policy_id);

    let req = test::TestRequest::post()
        .uri("/api/v1/workspace-transfer/copy")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "source_workspace": { "kind": "personal" },
            "file_ids": [],
            "folder_ids": [source_id],
            "destination_workspace": { "kind": "personal" },
            "target_folder_id": null
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["succeeded"], 1);
    assert_eq!(body["data"]["failed"], 0);

    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let same_workspace_copy_id = body["data"]["folders"]
        .as_array()
        .unwrap()
        .iter()
        .find(|folder| folder["name"] == "PolicyFolder (1)")
        .and_then(|folder| folder["id"].as_i64())
        .expect("same-workspace transfer copy should get a unique name");
    let copied_root = folder_repo::find_by_id(&db, same_workspace_copy_id)
        .await
        .unwrap();
    assert_eq!(copied_root.policy_id, Some(root_policy_id));

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{same_workspace_copy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let copied_child_id = body["data"]["folders"][0]["id"].as_i64().unwrap();
    let copied_child = folder_repo::find_by_id(&db, copied_child_id).await.unwrap();
    assert_eq!(copied_child.policy_id, Some(child_policy_id));

    let team_id = create_team!(app, token, "Policy Copy Destination");
    let req = test::TestRequest::post()
        .uri("/api/v1/workspace-transfer/copy")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "source_workspace": { "kind": "personal" },
            "file_ids": [],
            "folder_ids": [source_id],
            "destination_workspace": { "kind": "team", "team_id": team_id },
            "target_folder_id": null
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["succeeded"], 1);
    assert_eq!(body["data"]["failed"], 0);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let cross_workspace_copy_id = body["data"]["folders"]
        .as_array()
        .unwrap()
        .iter()
        .find(|folder| folder["name"] == "PolicyFolder")
        .and_then(|folder| folder["id"].as_i64())
        .expect("cross-workspace transfer copy should exist in team root");
    let copied_root = folder_repo::find_by_id(&db, cross_workspace_copy_id)
        .await
        .unwrap();
    assert_eq!(copied_root.policy_id, None);

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/teams/{team_id}/folders/{cross_workspace_copy_id}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let copied_child_id = body["data"]["folders"][0]["id"].as_i64().unwrap();
    let copied_child = folder_repo::find_by_id(&db, copied_child_id).await.unwrap();
    assert_eq!(copied_child.policy_id, None);
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

#[actix_web::test]
async fn test_batch_archive_download_ticket_respects_user_runtime_switch() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file_named!(app, token, "download-me.txt");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Archive Download Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let boundary = "----TeamArchiveDownloadBoundary";
    let payload = format!(
        "--{boundary}\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"team-download.txt\"\r\n\
         Content-Type: text/plain\r\n\r\n\
         team download content\r\n\
         --{boundary}--\r\n"
    );
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/files/upload"))
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
    let team_file_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/batch/archive-download")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [file_id],
            "folder_ids": [],
            "archive_name": "download.zip"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let ticket = body["data"]["token"].as_str().unwrap().to_string();
    assert!(ticket.starts_with("st_"));
    assert_eq!(
        body["data"]["download_path"],
        format!("/api/v1/batch/archive-download/{ticket}")
    );

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/batch/archive-download"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [team_file_id],
            "folder_ids": [],
            "archive_name": "team-download.zip"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let team_ticket = body["data"]["token"].as_str().unwrap().to_string();
    assert!(team_ticket.starts_with("st_"));
    assert_eq!(
        body["data"]["download_path"],
        format!("/api/v1/teams/{team_id}/batch/archive-download/{team_ticket}")
    );

    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::operations::ARCHIVE_DOWNLOAD_USER_ENABLED_KEY,
        "false",
    ));

    let req = test::TestRequest::post()
        .uri("/api/v1/batch/archive-download")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [file_id],
            "folder_ids": [],
            "archive_name": "download.zip"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::ArchiveDownloadUserDisabled.as_ref()
    );

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/batch/archive-download"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [team_file_id],
            "folder_ids": [],
            "archive_name": "team-download.zip"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::ArchiveDownloadUserDisabled.as_ref()
    );

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/batch/archive-download/{ticket}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::ArchiveDownloadUserDisabled.as_ref()
    );

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/teams/{team_id}/batch/archive-download/{team_ticket}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::ArchiveDownloadUserDisabled.as_ref()
    );
}

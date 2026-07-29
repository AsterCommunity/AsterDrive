//! 目录上传集成测试

#[macro_use]
mod common;

use actix_web::test;
use aster_drive::db::repository::folder_repo;
use aster_drive::runtime::SharedRuntimeState;
use serde_json::Value;

#[actix_web::test]
async fn test_direct_upload_with_relative_path_creates_nested_folders() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let boundary = "----DirUploadBoundary123";
    let payload = "------DirUploadBoundary123\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"hello.txt\"\r\n\
         Content-Type: text/plain\r\n\r\n\
         hello nested world\r\n\
         ------DirUploadBoundary123--\r\n";
    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload?relative_path=docs/guides/hello.txt")
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
    assert_eq!(body["data"]["name"], "hello.txt");

    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let docs_id = body["data"]["folders"]
        .as_array()
        .unwrap()
        .iter()
        .find(|folder| folder["name"] == "docs")
        .and_then(|folder| folder["id"].as_i64())
        .expect("docs folder should exist");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{docs_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let guides_id = body["data"]["folders"]
        .as_array()
        .unwrap()
        .iter()
        .find(|folder| folder["name"] == "guides")
        .and_then(|folder| folder["id"].as_i64())
        .expect("guides folder should exist");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{guides_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["files"][0]["name"], "hello.txt");

    let docs = folder_repo::find_by_id(&db, docs_id).await.unwrap();
    let guides = folder_repo::find_by_id(&db, guides_id).await.unwrap();
    assert_eq!(docs.created_by_username, "testuser");
    assert_eq!(guides.created_by_username, "testuser");
}

#[actix_web::test]
async fn test_init_upload_with_relative_path_reuses_existing_directories() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    for _ in 0..2 {
        let req = test::TestRequest::post()
            .uri("/api/v1/files/upload/init")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({
                "filename": "hello.txt",
                "total_size": 10_485_760,
                "relative_path": "docs/guides/hello.txt"
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let root_folders = body["data"]["folders"].as_array().unwrap();
    assert_eq!(root_folders.len(), 1);
    assert_eq!(root_folders[0]["name"], "docs");

    let docs_id = root_folders[0]["id"].as_i64().unwrap();
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{docs_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let child_folders = body["data"]["folders"].as_array().unwrap();
    assert_eq!(child_folders.len(), 1);
    assert_eq!(child_folders[0]["name"], "guides");

    let docs = folder_repo::find_by_id(&db, docs_id).await.unwrap();
    let guides_id = child_folders[0]["id"].as_i64().unwrap();
    let guides = folder_repo::find_by_id(&db, guides_id).await.unwrap();
    assert_eq!(docs.created_by_username, "testuser");
    assert_eq!(guides.created_by_username, "testuser");
}

#[actix_web::test]
async fn test_init_upload_with_relative_path_uses_parent_folder_policy() {
    use sea_orm::{ActiveModelTrait, Set};

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let policy_base_path = std::env::temp_dir()
        .join(format!(
            "test-relative-path-folder-policy-{}",
            uuid::Uuid::new_v4()
        ))
        .to_string_lossy()
        .into_owned();

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Tiny Folder Policy",
            "driver_type": "local",
            "base_path": policy_base_path,
            "max_file_size": 8,
            "is_default": false
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "policy-root" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    let mut folder: aster_drive_model::entities::folder::ActiveModel =
        folder_repo::find_by_id(&db, folder_id)
            .await
            .unwrap()
            .into();
    folder.policy_id = Set(Some(policy_id));
    folder.update(&db).await.unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload/init")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "filename": "ignored.txt",
            "folder_id": folder_id,
            "relative_path": "nested/too-large.txt",
            "total_size": 9
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["msg"], "file size 9 exceeds limit 8");
}

#[actix_web::test]
async fn test_relative_path_rejects_empty_segment() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload/init")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "filename": "bad.txt",
            "total_size": 10_485_760,
            "relative_path": "docs//bad.txt"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn test_chunked_upload_with_relative_path_and_auto_rename() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let relative_path = "docs/chunked.txt";
    let total_size = 10_485_760i64;

    for expected_name in ["chunked.txt", "chunked (1).txt"] {
        let req = test::TestRequest::post()
            .uri("/api/v1/files/upload/init")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({
                "filename": "chunked.txt",
                "total_size": total_size,
                "relative_path": relative_path
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body["data"]["mode"], "chunked");
        let upload_id = body["data"]["upload_id"].as_str().unwrap();
        let chunk_size = body["data"]["chunk_size"].as_i64().unwrap();
        let total_chunks = body["data"]["total_chunks"].as_i64().unwrap();

        for i in 0..total_chunks {
            let expected_chunk_size = if i == total_chunks - 1 {
                (total_size - chunk_size * (total_chunks - 1)) as usize
            } else {
                chunk_size as usize
            };
            let chunk_data = vec![b'A' + i as u8; expected_chunk_size];
            let req = test::TestRequest::put()
                .uri(&format!("/api/v1/files/upload/{upload_id}/{i}"))
                .insert_header(("Cookie", common::access_cookie_header(&token)))
                .insert_header(common::csrf_header_for(&token))
                .insert_header(("Content-Type", "application/octet-stream"))
                .set_payload(chunk_data)
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 200);
        }

        let req = test::TestRequest::post()
            .uri(&format!("/api/v1/files/upload/{upload_id}/complete"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
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
    let docs_id = body["data"]["folders"][0]["id"].as_i64().unwrap();

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{docs_id}"))
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
    assert!(names.contains(&"chunked.txt"));
    assert!(names.contains(&"chunked (1).txt"));
}

#[actix_web::test]
async fn test_relative_path_normalizes_nfd_segments_to_nfc() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let relative_path = urlencoding::encode("cafe\u{0301}/hello.txt");

    let boundary = "----DirUploadBoundaryNormalize";
    let payload = "------DirUploadBoundaryNormalize\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"hello.txt\"\r\n\
         Content-Type: text/plain\r\n\r\n\
         hello nested world\r\n\
         ------DirUploadBoundaryNormalize--\r\n";
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/files/upload?relative_path={relative_path}"
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

    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let root_folders = body["data"]["folders"].as_array().unwrap();
    assert_eq!(root_folders.len(), 1);
    assert_eq!(root_folders[0]["name"], "caf\u{00e9}");
}

#[actix_web::test]
async fn test_relative_path_rejects_windows_reserved_segment() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload/init")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "filename": "hello.txt",
            "total_size": 10_485_760,
            "relative_path": "docs/CON/hello.txt"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

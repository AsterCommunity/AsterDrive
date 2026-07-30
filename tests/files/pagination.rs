//! 集成测试：`pagination`。

use crate::common;

use actix_web::test;
use serde_json::Value;

/// Helper macro: create a folder in root or parent, return its ID
macro_rules! create_folder {
    ($app:expr, $token:expr, $name:expr) => {{
        let req = test::TestRequest::post()
            .uri("/api/v1/folders")
            .insert_header(("Cookie", common::access_cookie_header(&$token)))
            .insert_header(common::csrf_header_for(&$token))
            .set_json(serde_json::json!({ "name": $name }))
            .to_request();
        let resp: actix_web::dev::ServiceResponse = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 201);
        let body: Value = test::read_body_json(resp).await;
        body["data"]["id"].as_i64().unwrap()
    }};
    ($app:expr, $token:expr, $name:expr, $parent_id:expr) => {{
        let req = test::TestRequest::post()
            .uri("/api/v1/folders")
            .insert_header(("Cookie", common::access_cookie_header(&$token)))
            .insert_header(common::csrf_header_for(&$token))
            .set_json(serde_json::json!({ "name": $name, "parent_id": $parent_id }))
            .to_request();
        let resp: actix_web::dev::ServiceResponse = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 201);
        let body: Value = test::read_body_json(resp).await;
        body["data"]["id"].as_i64().unwrap()
    }};
}

#[actix_web::test]
async fn test_folder_list_includes_share_and_lock_status() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let folder_req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Shared Folder" }))
        .to_request();
    let folder_resp = test::call_service(&app, folder_req).await;
    assert_eq!(folder_resp.status(), 201);
    let folder_body: Value = test::read_body_json(folder_resp).await;
    let folder_id = folder_body["data"]["id"].as_i64().unwrap();

    let file_id = upload_test_file!(app, token);

    let lock_file_req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{file_id}/lock"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "locked": true }))
        .to_request();
    let lock_file_resp = test::call_service(&app, lock_file_req).await;
    assert_eq!(lock_file_resp.status(), 200);

    let lock_folder_req = test::TestRequest::post()
        .uri(&format!("/api/v1/folders/{folder_id}/lock"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "locked": true }))
        .to_request();
    let lock_folder_resp = test::call_service(&app, lock_folder_req).await;
    assert_eq!(lock_folder_resp.status(), 200);

    let share_file_req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": { "type": "file", "id": file_id }
        }))
        .to_request();
    let share_file_resp = test::call_service(&app, share_file_req).await;
    assert_eq!(share_file_resp.status(), 201);

    let share_folder_req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": { "type": "folder", "id": folder_id }
        }))
        .to_request();
    let share_folder_resp = test::call_service(&app, share_folder_req).await;
    assert_eq!(share_folder_resp.status(), 201);

    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;

    let folders = body["data"]["folders"].as_array().unwrap();
    let files = body["data"]["files"].as_array().unwrap();
    assert_eq!(folders.len(), 1);
    assert_eq!(files.len(), 1);
    assert_eq!(folders[0]["is_locked"], true);
    assert_eq!(folders[0]["is_shared"], true);
    assert_eq!(files[0]["is_locked"], true);
    assert_eq!(files[0]["is_shared"], true);
}

#[actix_web::test]
async fn test_folder_list_pagination_defaults() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // Create 3 folders and 5 files
    for i in 0..3 {
        create_folder!(app, token, format!("folder-{i:03}"));
    }
    for _ in 0..5 {
        upload_test_file!(app, token);
    }

    // Default request returns totals
    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["folders"].as_array().unwrap().len(), 3);
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 5);
    assert_eq!(body["data"]["folders_total"], 3);
    assert_eq!(body["data"]["files_total"], 5);
}

#[actix_web::test]
async fn test_folder_list_file_cursor_pagination() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // Create 8 files
    for _ in 0..8 {
        upload_test_file!(app, token);
    }

    // Page 1: file_limit=3, no cursor
    let req = test::TestRequest::get()
        .uri("/api/v1/folders?folder_limit=0&file_limit=3")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let page1_files = body["data"]["files"].as_array().unwrap();
    assert_eq!(page1_files.len(), 3);
    assert_eq!(body["data"]["files_total"], 8);
    // next_file_cursor must be set (more pages exist)
    assert!(!body["data"]["next_file_cursor"].is_null());

    let cursor_value = body["data"]["next_file_cursor"]["value"]
        .as_str()
        .unwrap()
        .to_string();
    let cursor_id = body["data"]["next_file_cursor"]["id"].as_i64().unwrap();
    let page1_ids: Vec<i64> = page1_files
        .iter()
        .map(|f| f["id"].as_i64().unwrap())
        .collect();

    // Page 2: use cursor
    let uri = format!(
        "/api/v1/folders?folder_limit=0&file_limit=3&file_after_value={}&file_after_id={}",
        urlencoding::encode(&cursor_value),
        cursor_id
    );
    let req = test::TestRequest::get()
        .uri(&uri)
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let page2_files = body["data"]["files"].as_array().unwrap();
    assert_eq!(page2_files.len(), 3);
    // No duplicates between pages
    for f in page2_files {
        let id = f["id"].as_i64().unwrap();
        assert!(!page1_ids.contains(&id), "duplicate file id {id} in page 2");
    }
    let cursor_value2 = body["data"]["next_file_cursor"]["value"]
        .as_str()
        .unwrap()
        .to_string();
    let cursor_id2 = body["data"]["next_file_cursor"]["id"].as_i64().unwrap();

    // Page 3: last page (2 files)
    let uri = format!(
        "/api/v1/folders?folder_limit=0&file_limit=3&file_after_value={}&file_after_id={}",
        urlencoding::encode(&cursor_value2),
        cursor_id2
    );
    let req = test::TestRequest::get()
        .uri(&uri)
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let page3_files = body["data"]["files"].as_array().unwrap();
    assert_eq!(page3_files.len(), 2);
    // Last page: next_file_cursor must be null
    assert!(body["data"]["next_file_cursor"].is_null());
}

#[actix_web::test]
async fn test_folder_list_file_limit_zero_skips_files() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    for i in 0..3 {
        create_folder!(app, token, format!("folder-{i:03}"));
    }
    for _ in 0..5 {
        upload_test_file!(app, token);
    }

    // file_limit=0 should return no files but still show files_total
    let req = test::TestRequest::get()
        .uri("/api/v1/folders?file_limit=0")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["folders"].as_array().unwrap().len(), 3);
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 0);
    assert_eq!(body["data"]["folders_total"], 3);
    assert_eq!(body["data"]["files_total"], 5);
}

#[actix_web::test]
async fn test_subfolder_pagination() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let parent_id = create_folder!(app, token, "parent");

    // Create 4 subfolders
    for i in 0..4 {
        create_folder!(app, token, format!("sub-{i}"), parent_id);
    }

    // Upload 6 files to parent
    for _ in 0..6 {
        upload_test_file_to_folder!(app, token, parent_id);
    }

    // Paginated list
    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/folders/{parent_id}?folder_limit=2&file_limit=3"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["folders"].as_array().unwrap().len(), 2);
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 3);
    assert_eq!(body["data"]["folders_total"], 4);
    assert_eq!(body["data"]["files_total"], 6);
}

#[actix_web::test]
async fn test_trash_pagination() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // Create and delete 4 folders
    for i in 0..4 {
        let id = create_folder!(app, token, format!("trash-folder-{i}"));
        let req = test::TestRequest::delete()
            .uri(&format!("/api/v1/folders/{id}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .to_request();
        let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    // Create and delete 5 files
    for _ in 0..5 {
        let id = upload_test_file!(app, token);
        let req = test::TestRequest::delete()
            .uri(&format!("/api/v1/files/{id}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .to_request();
        let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    // Default trash list with totals
    let req = test::TestRequest::get()
        .uri("/api/v1/trash")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["folders_total"], 4);
    assert_eq!(body["data"]["files_total"], 5);

    // Page 1: file_limit=3, should get next_file_cursor
    let req = test::TestRequest::get()
        .uri("/api/v1/trash?folder_limit=2&file_limit=3")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["folders"].as_array().unwrap().len(), 2);
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 3);
    assert_eq!(body["data"]["folders_total"], 4);
    assert_eq!(body["data"]["files_total"], 5);
    let cursor = &body["data"]["next_file_cursor"];
    assert!(
        cursor.is_object(),
        "should have next_file_cursor after page 1"
    );
    let after_expires_at = cursor["expires_at"].as_str().unwrap();
    let after_id = cursor["id"].as_i64().unwrap();

    // Page 2: use cursor, should get remaining 2 files and no more cursor
    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/trash?folder_limit=0&file_limit=3&file_after_expires_at={after_expires_at}&file_after_id={after_id}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 2);
    assert!(body["data"]["next_file_cursor"].is_null(), "no more pages");
}

#[actix_web::test]
async fn test_sort_by_name() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    for name in ["charlie.txt", "alpha.txt", "beta.txt"] {
        upload_test_file_named!(app, token, name);
    }

    // sort_by=name&sort_order=asc
    let req = test::TestRequest::get()
        .uri("/api/v1/folders?folder_limit=0&file_limit=10&sort_by=name&sort_order=asc")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let files = body["data"]["files"].as_array().unwrap();
    let names: Vec<&str> = files.iter().map(|f| f["name"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["alpha.txt", "beta.txt", "charlie.txt"]);

    // sort_by=name&sort_order=desc
    let req = test::TestRequest::get()
        .uri("/api/v1/folders?folder_limit=0&file_limit=10&sort_by=name&sort_order=desc")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let files = body["data"]["files"].as_array().unwrap();
    let names: Vec<&str> = files.iter().map(|f| f["name"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["charlie.txt", "beta.txt", "alpha.txt"]);
}

#[actix_web::test]
async fn test_sort_cursor_no_duplicates() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    for _ in 0..7 {
        upload_test_file!(app, token);
    }

    // Page 1 with sort_by=size
    let req = test::TestRequest::get()
        .uri("/api/v1/folders?folder_limit=0&file_limit=3&sort_by=size&sort_order=asc")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let page1_files = body["data"]["files"].as_array().unwrap();
    assert_eq!(page1_files.len(), 3);
    assert_eq!(body["data"]["files_total"], 7);
    let cursor = &body["data"]["next_file_cursor"];
    assert!(!cursor.is_null());

    let cursor_value = cursor["value"].as_str().unwrap();
    let cursor_id = cursor["id"].as_i64().unwrap();
    let page1_ids: Vec<i64> = page1_files
        .iter()
        .map(|f| f["id"].as_i64().unwrap())
        .collect();

    // Page 2
    let uri = format!(
        "/api/v1/folders?folder_limit=0&file_limit=3&sort_by=size&sort_order=asc&file_after_value={}&file_after_id={}",
        urlencoding::encode(cursor_value),
        cursor_id
    );
    let req = test::TestRequest::get()
        .uri(&uri)
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let page2_files = body["data"]["files"].as_array().unwrap();
    assert_eq!(page2_files.len(), 3);

    // No duplicates between pages
    let page2_ids: Vec<i64> = page2_files
        .iter()
        .map(|f| f["id"].as_i64().unwrap())
        .collect();
    for id in &page1_ids {
        assert!(
            !page2_ids.contains(id),
            "duplicate file id {id} across pages"
        );
    }

    // Page 3: last page (1 file)
    let cursor = &body["data"]["next_file_cursor"];
    assert!(!cursor.is_null());
    let cursor_value = cursor["value"].as_str().unwrap();
    let cursor_id = cursor["id"].as_i64().unwrap();
    let uri = format!(
        "/api/v1/folders?folder_limit=0&file_limit=3&sort_by=size&sort_order=asc&file_after_value={}&file_after_id={}",
        urlencoding::encode(cursor_value),
        cursor_id
    );
    let req = test::TestRequest::get()
        .uri(&uri)
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let page3_files = body["data"]["files"].as_array().unwrap();
    assert_eq!(page3_files.len(), 1);
    assert!(body["data"]["next_file_cursor"].is_null(), "no more pages");
}

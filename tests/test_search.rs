//! 集成测试：`search`。

#[macro_use]
mod common;

use actix_web::test;
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

async fn upload_search_file(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    token: &str,
    uri: &str,
    name: &str,
    content: &str,
    mime: &str,
) -> Value {
    let boundary = "----SearchUploadBoundary123";
    let payload = upload_named_file(name, content, mime, boundary);
    let req = test::TestRequest::post()
        .uri(uri)
        .insert_header(("Cookie", common::access_cookie_header(token)))
        .insert_header(common::csrf_header_for(token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 201, "upload failed for {name}");
    test::read_body_json(resp).await
}

#[actix_web::test]
async fn test_search_includes_share_and_lock_status() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let folder_req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Status Docs", "parent_id": null }))
        .to_request();
    let folder_resp = test::call_service(&app, folder_req).await;
    assert_eq!(folder_resp.status(), 201);
    let folder_body: Value = test::read_body_json(folder_resp).await;
    let folder_id = folder_body["data"]["id"].as_i64().unwrap();

    let boundary = "----TestBoundary123";
    let payload = upload_named_file("status-report.txt", "status", "text/plain", boundary);
    let upload_req = test::TestRequest::post()
        .uri("/api/v1/files/upload")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let upload_resp = test::call_service(&app, upload_req).await;
    assert_eq!(upload_resp.status(), 201);
    let upload_body: Value = test::read_body_json(upload_resp).await;
    let file_id = upload_body["data"]["id"].as_i64().unwrap();

    let lock_file_req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{file_id}/lock"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "locked": true }))
        .to_request();
    assert_eq!(test::call_service(&app, lock_file_req).await.status(), 200);

    let lock_folder_req = test::TestRequest::post()
        .uri(&format!("/api/v1/folders/{folder_id}/lock"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "locked": true }))
        .to_request();
    assert_eq!(
        test::call_service(&app, lock_folder_req).await.status(),
        200
    );

    let share_file_req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": { "type": "file", "id": file_id }
        }))
        .to_request();
    assert_eq!(test::call_service(&app, share_file_req).await.status(), 201);

    let share_folder_req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": { "type": "folder", "id": folder_id }
        }))
        .to_request();
    assert_eq!(
        test::call_service(&app, share_folder_req).await.status(),
        201
    );

    let file_search_req = test::TestRequest::get()
        .uri("/api/v1/search?q=status-report")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let file_search_resp = test::call_service(&app, file_search_req).await;
    assert_eq!(file_search_resp.status(), 200);
    let file_search_body: Value = test::read_body_json(file_search_resp).await;
    let files = file_search_body["data"]["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["is_locked"], true);
    assert_eq!(files[0]["is_shared"], true);
    assert!(files[0]["blob_id"].is_null());
    assert!(files[0]["created_at"].is_null());
    assert!(files[0]["folder_id"].is_null());
    assert!(files[0].get("user_id").is_none());

    let folder_search_req = test::TestRequest::get()
        .uri("/api/v1/search?type=folder&q=status")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let folder_search_resp = test::call_service(&app, folder_search_req).await;
    assert_eq!(folder_search_resp.status(), 200);
    let folder_search_body: Value = test::read_body_json(folder_search_resp).await;
    let folders = folder_search_body["data"]["folders"].as_array().unwrap();
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0]["is_locked"], true);
    assert_eq!(folders[0]["is_shared"], true);
    assert!(folders[0]["created_at"].is_null());
    assert!(folders[0]["parent_id"].is_null());
    assert!(folders[0]["policy_id"].is_null());
    assert!(folders[0].get("user_id").is_none());
}

#[actix_web::test]
async fn test_search_by_name() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let boundary = "----TestBoundary123";

    // Upload "report.pdf"
    let payload = upload_named_file("report.pdf", "pdf content", "application/pdf", boundary);
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

    // Upload "notes.txt"
    let payload = upload_named_file("notes.txt", "some notes", "text/plain", boundary);
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

    // Search for "rep" — should only match report.pdf
    let req = test::TestRequest::get()
        .uri("/api/v1/search?q=rep")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], 0);
    assert_eq!(body["data"]["total_files"], 1);
    let files = body["data"]["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["name"], "report.pdf");
}

#[actix_web::test]
async fn test_search_by_name_preserves_substring_and_short_query_behavior() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let boundary = "----TestBoundary123";
    let payload = upload_named_file("report.pdf", "pdf content", "application/pdf", boundary);
    let upload_req = test::TestRequest::post()
        .uri("/api/v1/files/upload")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let upload_resp = test::call_service(&app, upload_req).await;
    assert_eq!(upload_resp.status(), 201);
    let upload_body: Value = test::read_body_json(upload_resp).await;
    let file_id = upload_body["data"]["id"].as_i64().unwrap();

    let middle_substring_req = test::TestRequest::get()
        .uri("/api/v1/search?q=port")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let middle_substring_resp = test::call_service(&app, middle_substring_req).await;
    assert_eq!(middle_substring_resp.status(), 200);
    let middle_substring_body: Value = test::read_body_json(middle_substring_resp).await;
    assert_eq!(middle_substring_body["data"]["total_files"], 1);
    assert_eq!(
        middle_substring_body["data"]["files"][0]["name"],
        "report.pdf"
    );

    let short_query_req = test::TestRequest::get()
        .uri("/api/v1/search?q=r")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let short_query_resp = test::call_service(&app, short_query_req).await;
    assert_eq!(short_query_resp.status(), 200);
    let short_query_body: Value = test::read_body_json(short_query_resp).await;
    assert_eq!(short_query_body["data"]["total_files"], 1);
    assert_eq!(short_query_body["data"]["files"][0]["name"], "report.pdf");

    let rename_req = test::TestRequest::patch()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "ledger.pdf" }))
        .to_request();
    let rename_resp = test::call_service(&app, rename_req).await;
    assert_eq!(rename_resp.status(), 200);

    let renamed_search_req = test::TestRequest::get()
        .uri("/api/v1/search?q=ledge")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let renamed_search_resp = test::call_service(&app, renamed_search_req).await;
    assert_eq!(renamed_search_resp.status(), 200);
    let renamed_search_body: Value = test::read_body_json(renamed_search_resp).await;
    assert_eq!(renamed_search_body["data"]["total_files"], 1);
    assert_eq!(
        renamed_search_body["data"]["files"][0]["name"],
        "ledger.pdf"
    );

    let stale_name_req = test::TestRequest::get()
        .uri("/api/v1/search?q=report")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let stale_name_resp = test::call_service(&app, stale_name_req).await;
    assert_eq!(stale_name_resp.status(), 200);
    let stale_name_body: Value = test::read_body_json(stale_name_resp).await;
    assert_eq!(stale_name_body["data"]["total_files"], 0);
}

#[actix_web::test]
async fn test_search_rejects_invalid_type_and_dates() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let invalid_type_req = test::TestRequest::get()
        .uri("/api/v1/search?type=bogus")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let invalid_type_resp = test::call_service(&app, invalid_type_req).await;
    assert_eq!(invalid_type_resp.status(), 400);

    let invalid_date_req = test::TestRequest::get()
        .uri("/api/v1/search?created_after=not-a-date")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let invalid_date_resp = test::call_service(&app, invalid_date_req).await;
    assert_eq!(invalid_date_resp.status(), 400);

    let inverted_range_req = test::TestRequest::get()
        .uri(
            "/api/v1/search?created_after=2026-04-03T00:00:00Z&created_before=2026-04-02T00:00:00Z",
        )
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let inverted_range_resp = test::call_service(&app, inverted_range_req).await;
    assert_eq!(inverted_range_resp.status(), 400);
}

#[actix_web::test]
async fn test_search_by_mime_type() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let boundary = "----TestBoundary123";

    // Upload text file
    let payload = upload_named_file("doc.txt", "text content", "text/plain", boundary);
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

    // Upload PDF file
    let payload = upload_named_file("report.pdf", "pdf content", "application/pdf", boundary);
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

    // Search by MIME type — only PDF should match
    let req = test::TestRequest::get()
        .uri("/api/v1/search?mime_type=%20application/pdf%20")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], 0);
    assert_eq!(body["data"]["total_files"], 1);
    let files = body["data"]["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["mime_type"], "application/pdf");
}

#[actix_web::test]
async fn test_search_by_category_and_extensions() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    upload_search_file(
        &app,
        &token,
        "/api/v1/files/upload",
        "photo.JPG",
        "image",
        "image/jpeg",
    )
    .await;
    upload_search_file(
        &app,
        &token,
        "/api/v1/files/upload",
        "clip.mp4",
        "video",
        "video/mp4",
    )
    .await;
    upload_search_file(
        &app,
        &token,
        "/api/v1/files/upload",
        "song.mp3",
        "audio",
        "audio/mpeg",
    )
    .await;
    upload_search_file(
        &app,
        &token,
        "/api/v1/files/upload",
        "report.pdf",
        "pdf",
        "application/pdf",
    )
    .await;
    upload_search_file(
        &app,
        &token,
        "/api/v1/files/upload",
        "pdf-notes.txt",
        "not pdf",
        "text/plain",
    )
    .await;
    upload_search_file(
        &app,
        &token,
        "/api/v1/files/upload",
        "sheet.xlsx",
        "sheet",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    )
    .await;
    upload_search_file(
        &app,
        &token,
        "/api/v1/files/upload",
        "backup.tar.gz",
        "archive",
        "application/gzip",
    )
    .await;

    for (uri, expected_names) in [
        ("/api/v1/search?type=file&category=image", vec!["photo.JPG"]),
        ("/api/v1/search?type=file&category=video", vec!["clip.mp4"]),
        ("/api/v1/search?type=file&category=audio", vec!["song.mp3"]),
        (
            "/api/v1/search?type=file&category=document",
            vec!["pdf-notes.txt", "report.pdf"],
        ),
        (
            "/api/v1/search?type=file&extensions=pdf",
            vec!["report.pdf"],
        ),
        (
            "/api/v1/search?type=file&extensions=pdf,xlsx",
            vec!["report.pdf", "sheet.xlsx"],
        ),
        (
            "/api/v1/search?type=file&extensions=tar.gz",
            vec!["backup.tar.gz"],
        ),
        (
            "/api/v1/search?category=document&q=report",
            vec!["report.pdf"],
        ),
        (
            "/api/v1/search?category=document&mime_type=application/pdf",
            vec!["report.pdf"],
        ),
    ] {
        let req = test::TestRequest::get()
            .uri(uri)
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200, "unexpected status for {uri}");
        let body: Value = test::read_body_json(resp).await;
        let names: Vec<&str> = body["data"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|file| file["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, expected_names, "unexpected results for {uri}");
        assert_eq!(
            body["data"]["total_folders"], 0,
            "folders should be omitted for {uri}"
        );
    }

    let pdf_req = test::TestRequest::get()
        .uri("/api/v1/search?type=file&extensions=pdf")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let pdf_resp = test::call_service(&app, pdf_req).await;
    let pdf_body: Value = test::read_body_json(pdf_resp).await;
    let pdf = &pdf_body["data"]["files"][0];
    assert_eq!(pdf["extension"], "pdf");
    assert!(pdf["compound_extension"].is_null());
    assert_eq!(pdf["file_category"], "document");

    let archive_req = test::TestRequest::get()
        .uri("/api/v1/search?type=file&extensions=tar.gz")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let archive_resp = test::call_service(&app, archive_req).await;
    let archive_body: Value = test::read_body_json(archive_resp).await;
    assert_eq!(
        archive_body["data"]["files"][0]["compound_extension"],
        "tar.gz"
    );
    assert_eq!(archive_body["data"]["files"][0]["file_category"], "archive");
}

#[actix_web::test]
async fn test_search_category_combines_with_folder_scope_and_rename_updates_fields() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let folder_req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Scoped", "parent_id": null }))
        .to_request();
    let folder_resp = test::call_service(&app, folder_req).await;
    assert_eq!(folder_resp.status(), 201);
    let folder_body: Value = test::read_body_json(folder_resp).await;
    let folder_id = folder_body["data"]["id"].as_i64().unwrap();

    let outside = upload_search_file(
        &app,
        &token,
        "/api/v1/files/upload",
        "outside.pdf",
        "pdf",
        "application/pdf",
    )
    .await;
    let outside_id = outside["data"]["id"].as_i64().unwrap();
    upload_search_file(
        &app,
        &token,
        &format!("/api/v1/files/upload?folder_id={folder_id}"),
        "inside.pdf",
        "pdf",
        "application/pdf",
    )
    .await;

    let scoped_req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/search?type=file&category=document&folder_id={folder_id}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let scoped_resp = test::call_service(&app, scoped_req).await;
    assert_eq!(scoped_resp.status(), 200);
    let scoped_body: Value = test::read_body_json(scoped_resp).await;
    assert_eq!(scoped_body["data"]["total_files"], 1);
    assert_eq!(scoped_body["data"]["files"][0]["name"], "inside.pdf");

    let rename_req = test::TestRequest::patch()
        .uri(&format!("/api/v1/files/{outside_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "outside.mp4" }))
        .to_request();
    let rename_resp = test::call_service(&app, rename_req).await;
    assert_eq!(rename_resp.status(), 200);
    let rename_body: Value = test::read_body_json(rename_resp).await;
    assert_eq!(rename_body["data"]["extension"], "mp4");
    assert_eq!(rename_body["data"]["file_category"], "video");

    let video_req = test::TestRequest::get()
        .uri("/api/v1/search?type=file&category=video")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let video_resp = test::call_service(&app, video_req).await;
    assert_eq!(video_resp.status(), 200);
    let video_body: Value = test::read_body_json(video_resp).await;
    assert_eq!(video_body["data"]["total_files"], 1);
    assert_eq!(video_body["data"]["files"][0]["name"], "outside.mp4");
}

#[actix_web::test]
async fn test_search_rejects_invalid_file_type_filters() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    for uri in [
        "/api/v1/search?category=bogus",
        "/api/v1/search?extensions=",
        "/api/v1/search?extensions=pdf,,docx",
        "/api/v1/search?extensions=../pdf",
        "/api/v1/search?type=folder&category=image",
        "/api/v1/search?type=folder&extensions=pdf",
    ] {
        let req = test::TestRequest::get()
            .uri(uri)
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400, "unexpected status for {uri}");
    }
}

#[actix_web::test]
async fn test_search_folders() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // Create "Documents" folder
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Documents", "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    // Create "Photos" folder
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Photos", "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    // Search folders with q=doc — only "Documents" should match
    let req = test::TestRequest::get()
        .uri("/api/v1/search?type=folder&q=doc")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], 0);
    assert_eq!(body["data"]["total_folders"], 1);
    assert_eq!(body["data"]["total_files"], 0);
    let folders = body["data"]["folders"].as_array().unwrap();
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0]["name"], "Documents");
}

#[actix_web::test]
async fn test_search_excludes_deleted() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let boundary = "----TestBoundary123";

    // Upload a file
    let payload = upload_named_file("searchable.txt", "find me", "text/plain", boundary);
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

    // Verify file is searchable before deletion
    let req = test::TestRequest::get()
        .uri("/api/v1/search?q=searchable")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total_files"], 1);

    // Soft delete the file
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // Search again — deleted file should not appear
    let req = test::TestRequest::get()
        .uri("/api/v1/search?q=searchable")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], 0);
    assert_eq!(body["data"]["total_files"], 0);
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 0);
}

#[actix_web::test]
async fn test_search_only_own_files() {
    let state = common::setup().await;
    let db = state.db.clone();
    let mail_sender = state.mail_sender.clone();
    let runtime_config = state.runtime_config.clone();
    let app = create_test_app!(state);

    // Register user1 (first user = admin)
    let (token1, _) = register_and_login!(app);

    let boundary = "----TestBoundary123";

    // Upload file as user1
    let payload = upload_named_file("user1_report.txt", "user1 data", "text/plain", boundary);
    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload")
        .insert_header(("Cookie", common::access_cookie_header(&token1)))
        .insert_header(common::csrf_header_for(&token1))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    // Register user2 (non-admin)
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "user2",
            "email": "user2@example.com",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    common::flush_mail_outbox_with(&db, &runtime_config, &mail_sender).await;

    let _ = confirm_latest_contact_verification!(app, db, mail_sender);

    // Login as user2
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "user2",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let token2 = common::extract_cookie(&resp, "aster_access").unwrap();

    // Upload file as user2
    let payload = upload_named_file("user2_report.txt", "user2 data", "text/plain", boundary);
    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload")
        .insert_header(("Cookie", common::access_cookie_header(&token2)))
        .insert_header(common::csrf_header_for(&token2))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    // User1 searches for "report" — should only see own file
    let req = test::TestRequest::get()
        .uri("/api/v1/search?q=report")
        .insert_header(("Cookie", common::access_cookie_header(&token1)))
        .insert_header(common::csrf_header_for(&token1))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], 0);
    assert_eq!(body["data"]["total_files"], 1);
    let files = body["data"]["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["name"], "user1_report.txt");
}

//! 集成测试：压缩包只读预览。

#[macro_use]
mod common;

use std::io::{Cursor, Write};

use actix_web::http::StatusCode;
use actix_web::test;
use aster_drive::db::repository::property_repo;
use aster_drive::entities::background_task;
use aster_drive::types::{BackgroundTaskKind, BackgroundTaskStatus, EntityType};
use serde_json::Value;

fn create_stored_zip_bytes(entries: &[(&str, Option<&[u8]>)]) -> Vec<u8> {
    let cursor = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(cursor);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    for (path, content) in entries {
        match content {
            Some(bytes) => {
                zip.start_file(*path, options)
                    .expect("zip entry should start");
                zip.write_all(bytes).expect("zip entry should be writable");
            }
            None => {
                zip.add_directory(*path, options)
                    .expect("zip directory should be writable");
            }
        }
    }

    zip.finish().expect("zip writer should finish").into_inner()
}

fn create_stored_zip_bytes_with_raw_name(
    placeholder_name: &str,
    raw_name: &[u8],
    content: &[u8],
) -> Vec<u8> {
    assert_eq!(
        placeholder_name.len(),
        raw_name.len(),
        "test helper patches ZIP names in place"
    );
    let mut bytes = create_stored_zip_bytes(&[(placeholder_name, Some(content))]);
    patch_zip_entry_raw_name(&mut bytes, placeholder_name.as_bytes(), raw_name);
    bytes
}

fn patch_zip_entry_raw_name(bytes: &mut [u8], placeholder_name: &[u8], raw_name: &[u8]) {
    patch_zip_entry_raw_name_in_header(
        bytes,
        &[0x50, 0x4b, 0x03, 0x04],
        6,
        26,
        30,
        placeholder_name,
        raw_name,
    );
    patch_zip_entry_raw_name_in_header(
        bytes,
        &[0x50, 0x4b, 0x01, 0x02],
        8,
        28,
        46,
        placeholder_name,
        raw_name,
    );
}

fn patch_zip_entry_raw_name_in_header(
    bytes: &mut [u8],
    signature: &[u8; 4],
    flag_offset: usize,
    name_len_offset: usize,
    name_offset: usize,
    placeholder_name: &[u8],
    raw_name: &[u8],
) {
    let mut patched = false;
    for index in 0..bytes.len().saturating_sub(signature.len()) {
        if !bytes[index..].starts_with(signature) || index + name_offset > bytes.len() {
            continue;
        }
        let name_len = u16::from_le_bytes([
            bytes[index + name_len_offset],
            bytes[index + name_len_offset + 1],
        ]) as usize;
        let name_start = index + name_offset;
        let name_end = name_start + name_len;
        if name_end > bytes.len() || &bytes[name_start..name_end] != placeholder_name {
            continue;
        }

        assert_eq!(name_len, raw_name.len());
        bytes[name_start..name_end].copy_from_slice(raw_name);
        let flags =
            u16::from_le_bytes([bytes[index + flag_offset], bytes[index + flag_offset + 1]]);
        bytes[index + flag_offset..index + flag_offset + 2]
            .copy_from_slice(&(flags & !0x0800).to_le_bytes());
        patched = true;
        break;
    }

    assert!(patched, "ZIP entry header should be patched");
}

fn create_many_entry_zip_bytes(count: usize) -> Vec<u8> {
    let cursor = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(cursor);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    for index in 0..count {
        zip.start_file(format!("docs/file-{index:03}.txt"), options)
            .expect("zip entry should start");
        zip.write_all(format!("content-{index}").as_bytes())
            .expect("zip entry should be writable");
    }

    zip.finish().expect("zip writer should finish").into_inner()
}

fn create_many_long_entry_zip_bytes(count: usize) -> Vec<u8> {
    let cursor = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(cursor);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    for index in 0..count {
        zip.start_file(
            format!(
                "docs/file-{index:04}-archive-preview-manifest-cache-boundary-padding-alpha-beta-gamma-delta.txt"
            ),
            options,
        )
        .expect("zip entry should start");
        zip.write_all(b"x").expect("zip entry should be writable");
    }

    zip.finish().expect("zip writer should finish").into_inner()
}

async fn upload_bytes<S>(app: &S, token: &str, filename: &str, mime: &str, bytes: Vec<u8>) -> i64
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse,
            Error = actix_web::Error,
        >,
{
    upload_bytes_to_folder(app, token, None, filename, mime, bytes).await
}

async fn upload_bytes_to_folder<S>(
    app: &S,
    token: &str,
    folder_id: Option<i64>,
    filename: &str,
    mime: &str,
    bytes: Vec<u8>,
) -> i64
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse,
            Error = actix_web::Error,
        >,
{
    let boundary = "----ArchivePreviewBoundary";
    let uri = folder_id
        .map(|id| format!("/api/v1/files/upload?folder_id={id}"))
        .unwrap_or_else(|| "/api/v1/files/upload".to_string());
    let mut payload = Vec::new();
    payload.extend_from_slice(b"------ArchivePreviewBoundary\r\n");
    payload.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n")
            .as_bytes(),
    );
    payload.extend_from_slice(format!("Content-Type: {mime}\r\n\r\n").as_bytes());
    payload.extend_from_slice(&bytes);
    payload.extend_from_slice(b"\r\n------ArchivePreviewBoundary--\r\n");

    let req = test::TestRequest::post()
        .uri(&uri)
        .insert_header(("Cookie", common::access_cookie_header(token)))
        .insert_header(common::csrf_header_for(token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 201, "upload should return 201");
    let body: Value = test::read_body_json(resp).await;
    body["data"]["id"].as_i64().unwrap()
}

async fn create_folder<S>(app: &S, token: &str, name: &str, parent_id: Option<i64>) -> i64
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse,
            Error = actix_web::Error,
        >,
{
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(token)))
        .insert_header(common::csrf_header_for(token))
        .set_json(serde_json::json!({
            "name": name,
            "parent_id": parent_id
        }))
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 201, "folder create should return 201");
    let body: Value = test::read_body_json(resp).await;
    body["data"]["id"].as_i64().unwrap()
}

async fn enable_archive_preview(
    state: &aster_drive::runtime::PrimaryAppState,
    user_enabled: bool,
    share_enabled: bool,
) {
    for (key, value) in [
        ("archive_preview_enabled", "true"),
        (
            "archive_preview_user_enabled",
            if user_enabled { "true" } else { "false" },
        ),
        (
            "archive_preview_share_enabled",
            if share_enabled { "true" } else { "false" },
        ),
    ] {
        aster_drive::services::config_service::set(state, key, value, 1)
            .await
            .expect("archive preview config should update");
    }
}

async fn request_personal_archive_preview<S>(
    app: &S,
    token: &str,
    file_id: i64,
) -> actix_web::dev::ServiceResponse
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse,
            Error = actix_web::Error,
        >,
{
    request_personal_archive_preview_with_encoding(app, token, file_id, None).await
}

async fn request_personal_archive_preview_with_encoding<S>(
    app: &S,
    token: &str,
    file_id: i64,
    filename_encoding: Option<&str>,
) -> actix_web::dev::ServiceResponse
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse,
            Error = actix_web::Error,
        >,
{
    let uri = filename_encoding
        .map(|encoding| {
            format!("/api/v1/files/{file_id}/archive-preview?filename_encoding={encoding}")
        })
        .unwrap_or_else(|| format!("/api/v1/files/{file_id}/archive-preview"));
    let req = test::TestRequest::get()
        .uri(&uri)
        .insert_header(("Cookie", common::access_cookie_header(token)))
        .insert_header(common::csrf_header_for(token))
        .to_request();
    test::call_service(app, req).await
}

async fn archive_preview_tasks(
    state: &aster_drive::runtime::PrimaryAppState,
) -> Vec<background_task::Model> {
    let mut tasks =
        aster_drive::db::repository::background_task_repo::list_recent(state.writer_db(), 50)
            .await
            .expect("task list should load")
            .into_iter()
            .filter(|task| task.kind == BackgroundTaskKind::ArchivePreviewGenerate)
            .collect::<Vec<_>>();
    tasks.sort_by_key(|task| task.id);
    tasks
}

#[actix_web::test]
async fn test_archive_preview_default_disabled() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let file_id = upload_bytes(
        &app,
        &token,
        "bundle.zip",
        "application/zip",
        create_stored_zip_bytes(&[("docs/readme.txt", Some(b"hello".as_slice()))]),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/archive-preview"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["error"]["subcode"], "archive_preview.disabled");
}

#[actix_web::test]
async fn test_archive_preview_user_toggle_disabled_reports_subcode() {
    let state = common::setup().await;
    enable_archive_preview(&state, false, true).await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let file_id = upload_bytes(
        &app,
        &token,
        "bundle.zip",
        "application/zip",
        create_stored_zip_bytes(&[("docs/readme.txt", Some(b"hello".as_slice()))]),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/archive-preview"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["error"]["subcode"], "archive_preview.user_disabled");
}

#[actix_web::test]
async fn test_archive_preview_returns_manifest_and_caches_it() {
    let state = common::setup().await;
    enable_archive_preview(&state, true, false).await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let file_id = upload_bytes(
        &app,
        &token,
        "bundle.zip",
        "application/zip",
        create_stored_zip_bytes(&[
            ("docs/", None),
            ("docs/readme.txt", Some(b"hello".as_slice())),
            ("image.bin", Some(b"abc".as_slice())),
        ]),
    )
    .await;

    let resp = request_personal_archive_preview(&app, &token, file_id).await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    assert_eq!(
        resp.headers()
            .get("Retry-After")
            .and_then(|value| value.to_str().ok()),
        Some("2")
    );
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], 0);

    let tasks =
        aster_drive::db::repository::background_task_repo::list_recent(state.writer_db(), 10)
            .await
            .expect("task list should load");
    let task = tasks
        .iter()
        .find(|task| task.kind == BackgroundTaskKind::ArchivePreviewGenerate)
        .expect("archive preview task should be created");
    assert_eq!(task.status, BackgroundTaskStatus::Pending);

    aster_drive::services::task_service::drain(&state)
        .await
        .expect("archive preview task should drain");
    let tasks =
        aster_drive::db::repository::background_task_repo::list_recent(state.writer_db(), 10)
            .await
            .expect("task list should load");
    let task = tasks
        .iter()
        .find(|task| task.kind == BackgroundTaskKind::ArchivePreviewGenerate)
        .expect("archive preview task should exist");
    assert_eq!(task.status, BackgroundTaskStatus::Succeeded);
    assert_eq!(task.progress_current, 4);
    assert_eq!(task.progress_total, 4);
    assert_eq!(task.status_text.as_deref(), Some("Archive preview ready"));
    let task_result: Value = serde_json::from_str(
        task.result_json
            .as_ref()
            .expect("archive preview task should store result")
            .as_ref(),
    )
    .expect("archive preview task result should parse");
    assert_eq!(task_result["file_id"], file_id);
    assert_eq!(task_result["entry_count"], 3);
    assert_eq!(task_result["file_count"], 2);
    assert_eq!(task_result["directory_count"], 1);
    assert_eq!(task_result["truncated"], false);
    let steps: Value = serde_json::from_str(
        task.steps_json
            .as_ref()
            .expect("archive preview task should store steps")
            .as_ref(),
    )
    .expect("archive preview task steps should parse");
    let steps = steps.as_array().expect("task steps should be an array");
    assert_eq!(
        steps
            .iter()
            .map(|step| (
                step["key"].as_str().unwrap(),
                step["status"].as_str().unwrap()
            ))
            .collect::<Vec<_>>(),
        vec![
            ("waiting", "succeeded"),
            ("download_source", "succeeded"),
            ("scan_archive", "succeeded"),
            ("persist_manifest", "succeeded"),
        ]
    );

    let resp = request_personal_archive_preview(&app, &token, file_id).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("Cache-Control")
            .and_then(|value| value.to_str().ok()),
        Some("private, max-age=0, must-revalidate")
    );
    let etag = resp
        .headers()
        .get("ETag")
        .and_then(|value| value.to_str().ok())
        .expect("archive preview should include ETag")
        .to_string();
    let body: Value = test::read_body_json(resp).await;
    let data = &body["data"];
    assert_eq!(data["format"], "zip");
    assert_eq!(data["entry_count"], 3);
    assert_eq!(data["file_count"], 2);
    assert_eq!(data["directory_count"], 1);
    assert_eq!(data["truncated"], false);
    assert!(
        data["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["path"] == "docs/readme.txt"
                && entry["parent"] == "docs"
                && entry["kind"] == "file"
                && entry["size"] == 5)
    );

    let cached = property_repo::find_by_key(
        state.writer_db(),
        EntityType::File,
        file_id,
        "system.archive_preview",
        "zip_raw_manifest.v2",
    )
    .await
    .expect("cache lookup should succeed");
    assert!(
        cached.is_some(),
        "archive preview manifest should be cached"
    );

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/archive-preview"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("If-None-Match", etag.as_str()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
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
        Some("private, max-age=0, must-revalidate")
    );

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/archive-preview"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("If-None-Match", "*"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(
        resp.headers()
            .get("ETag")
            .and_then(|value| value.to_str().ok()),
        Some(etag.as_str())
    );

    let resp = request_personal_archive_preview(&app, &token, file_id).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("ETag")
            .and_then(|value| value.to_str().ok()),
        Some(etag.as_str())
    );
    let second_body: Value = test::read_body_json(resp).await;
    assert_eq!(second_body["data"]["entries"], data["entries"]);
}

#[actix_web::test]
async fn test_archive_preview_allows_display_only_names_that_extract_rejects() {
    let state = common::setup().await;
    enable_archive_preview(&state, true, false).await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let file_id = upload_bytes(
        &app,
        &token,
        "bundle.zip",
        "application/zip",
        create_stored_zip_bytes(&[("folder/name:with-colon.txt", Some(b"hello".as_slice()))]),
    )
    .await;

    let resp = request_personal_archive_preview(&app, &token, file_id).await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);

    aster_drive::services::task_service::drain(&state)
        .await
        .expect("archive preview task should drain");

    let resp = request_personal_archive_preview(&app, &token, file_id).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    let entries = body["data"]["entries"]
        .as_array()
        .expect("manifest entries should be an array");

    assert!(
        entries.iter().any(|entry| {
            entry["path"] == "folder/name:with-colon.txt"
                && entry["name"] == "name:with-colon.txt"
                && entry["parent"] == "folder"
        }),
        "preview should expose display-only archive entry names"
    );
}

#[actix_web::test]
async fn test_archive_preview_decodes_gb18030_names_and_reuses_raw_cache() {
    let state = common::setup().await;
    enable_archive_preview(&state, true, false).await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let file_id = upload_bytes(
        &app,
        &token,
        "legacy-gbk.zip",
        "application/zip",
        create_stored_zip_bytes_with_raw_name(
            "aaaaaaaaa.txt",
            b"\xb2\xe2\xca\xd4/\xce\xc4\xbc\xfe.txt",
            b"legacy payload",
        ),
    )
    .await;

    let resp =
        request_personal_archive_preview_with_encoding(&app, &token, file_id, Some("auto")).await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    aster_drive::services::task_service::drain(&state)
        .await
        .expect("archive preview task should drain");

    let resp =
        request_personal_archive_preview_with_encoding(&app, &token, file_id, Some("auto")).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    let entries = body["data"]["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["path"], "测试/文件.txt");
    assert_eq!(entries[0]["name"], "文件.txt");
    assert_eq!(entries[0]["parent"], "测试");
    assert_eq!(archive_preview_tasks(&state).await.len(), 1);

    let resp =
        request_personal_archive_preview_with_encoding(&app, &token, file_id, Some("cp437")).await;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "a generated raw preview should be re-decoded without another build task"
    );
    let body: Value = test::read_body_json(resp).await;
    assert_ne!(body["data"]["entries"][0]["path"], "测试/文件.txt");
    assert_eq!(archive_preview_tasks(&state).await.len(), 1);
}

#[actix_web::test]
async fn test_archive_preview_reuses_pending_task_for_repeated_requests() {
    let state = common::setup().await;
    enable_archive_preview(&state, true, false).await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let file_id = upload_bytes(
        &app,
        &token,
        "dedupe.zip",
        "application/zip",
        create_stored_zip_bytes(&[("dedupe.txt", Some(b"dedupe".as_slice()))]),
    )
    .await;

    let first = request_personal_archive_preview(&app, &token, file_id).await;
    assert_eq!(first.status(), StatusCode::ACCEPTED);
    let second = request_personal_archive_preview(&app, &token, file_id).await;
    assert_eq!(second.status(), StatusCode::ACCEPTED);

    let tasks = archive_preview_tasks(&state).await;
    assert_eq!(
        tasks.len(),
        1,
        "repeated cache-miss requests should reuse the pending task"
    );
    assert_eq!(tasks[0].status, BackgroundTaskStatus::Pending);
    assert!(tasks[0].display_name.contains(&format!("file #{file_id}")));

    aster_drive::services::task_service::drain(&state)
        .await
        .expect("archive preview task should drain");
    let tasks = archive_preview_tasks(&state).await;
    assert_eq!(
        tasks.len(),
        1,
        "successful generation should not create another task"
    );
    assert_eq!(tasks[0].status, BackgroundTaskStatus::Succeeded);

    let resp = request_personal_archive_preview(&app, &token, file_id).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let tasks = archive_preview_tasks(&state).await;
    assert_eq!(tasks.len(), 1, "cache hit should not enqueue a new task");
}

#[actix_web::test]
async fn test_archive_preview_limit_reduction_keeps_generated_cache() {
    let state = common::setup().await;
    enable_archive_preview(&state, true, false).await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let file_id = upload_bytes(
        &app,
        &token,
        "config-sensitive.zip",
        "application/zip",
        create_stored_zip_bytes(&[
            ("config-a.txt", Some(b"config-a".as_slice())),
            ("config-b.txt", Some(b"config-b".as_slice())),
        ]),
    )
    .await;

    let resp = request_personal_archive_preview(&app, &token, file_id).await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    aster_drive::services::task_service::drain(&state)
        .await
        .expect("archive preview task should drain");
    let resp = request_personal_archive_preview(&app, &token, file_id).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(archive_preview_tasks(&state).await.len(), 1);

    aster_drive::services::config_service::set(&state, "archive_preview_max_entries", "1", 1)
        .await
        .expect("archive preview entry limit should be reduced");

    let resp = request_personal_archive_preview(&app, &token, file_id).await;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "reduced preview limits should not invalidate an already generated manifest"
    );
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["entry_count"], 2);
    let tasks = archive_preview_tasks(&state).await;
    assert_eq!(
        tasks.len(),
        1,
        "cache hit should not enqueue a replacement task after limits are reduced"
    );
    assert_eq!(tasks[0].status, BackgroundTaskStatus::Succeeded);
}

#[actix_web::test]
async fn test_archive_preview_rejects_unsupported_type_and_source_limit() {
    let state = common::setup().await;
    enable_archive_preview(&state, true, false).await;
    aster_drive::services::config_service::set(&state, "archive_preview_max_source_bytes", "1", 1)
        .await
        .expect("archive preview source limit should update");
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let text_id = upload_bytes(
        &app,
        &token,
        "notes.txt",
        "text/plain",
        b"not a zip".to_vec(),
    )
    .await;
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{text_id}/archive-preview"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["error"]["subcode"], "archive_preview.unsupported_type");
    assert!(
        archive_preview_tasks(&state).await.is_empty(),
        "unsupported files should fail before task creation"
    );

    let mime_only_zip_id = upload_bytes(
        &app,
        &token,
        "archive-by-mime.bin",
        "application/zip",
        create_stored_zip_bytes(&[("payload.txt", Some(b"payload".as_slice()))]),
    )
    .await;
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{mime_only_zip_id}/archive-preview"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["error"]["subcode"], "archive_preview.unsupported_type");
    assert!(
        archive_preview_tasks(&state).await.is_empty(),
        "MIME-only ZIP files should fail before source limit checks or task creation"
    );

    let zip_id = upload_bytes(
        &app,
        &token,
        "too-large.zip",
        "application/zip",
        create_stored_zip_bytes(&[("payload.txt", Some(b"payload".as_slice()))]),
    )
    .await;
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{zip_id}/archive-preview"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["error"]["subcode"], "archive_preview.source_too_large");
    assert!(
        archive_preview_tasks(&state).await.is_empty(),
        "oversized sources should fail before task creation"
    );
}

#[actix_web::test]
async fn test_archive_preview_reports_invalid_archive_with_subcode() {
    let state = common::setup().await;
    enable_archive_preview(&state, true, false).await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let file_id = upload_bytes(
        &app,
        &token,
        "not-really.zip",
        "application/zip",
        b"not a real zip archive".to_vec(),
    )
    .await;

    let resp = request_personal_archive_preview(&app, &token, file_id).await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    aster_drive::services::task_service::drain(&state)
        .await
        .expect("archive preview task should drain");
    let resp = request_personal_archive_preview(&app, &token, file_id).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["error"]["subcode"], "archive_preview.invalid_archive");
}

#[actix_web::test]
async fn test_archive_preview_failed_task_is_reused_as_friendly_error_without_requeue() {
    let state = common::setup().await;
    enable_archive_preview(&state, true, false).await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let file_id = upload_bytes(
        &app,
        &token,
        "broken.zip",
        "application/zip",
        b"broken zip payload".to_vec(),
    )
    .await;

    let resp = request_personal_archive_preview(&app, &token, file_id).await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    aster_drive::services::task_service::drain(&state)
        .await
        .expect("archive preview task should drain");

    let tasks = archive_preview_tasks(&state).await;
    assert_eq!(tasks.len(), 1);
    let task = &tasks[0];
    assert_eq!(task.status, BackgroundTaskStatus::Failed);
    assert_eq!(task.failure_can_retry, Some(false));
    assert!(
        task.last_error
            .as_deref()
            .is_some_and(|error| error.contains("invalid archive"))
    );
    let steps: Value = serde_json::from_str(
        task.steps_json
            .as_ref()
            .expect("failed archive preview task should store steps")
            .as_ref(),
    )
    .expect("failed task steps should parse");
    assert!(
        steps
            .as_array()
            .unwrap()
            .iter()
            .any(|step| { step["key"] == "scan_archive" && step["status"] == "failed" }),
        "scan step should be marked failed"
    );

    let resp = request_personal_archive_preview(&app, &token, file_id).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["error"]["subcode"], "archive_preview.invalid_archive");
    assert_eq!(
        archive_preview_tasks(&state).await.len(),
        1,
        "known deterministic failure should not enqueue another identical task"
    );
}

#[actix_web::test]
async fn test_archive_preview_reports_scan_limit_rejection_with_subcode() {
    let state = common::setup().await;
    enable_archive_preview(&state, true, false).await;
    aster_drive::services::config_service::set(&state, "archive_preview_max_entries", "1", 1)
        .await
        .expect("archive preview entry limit should update");
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let file_id = upload_bytes(
        &app,
        &token,
        "many-entries.zip",
        "application/zip",
        create_stored_zip_bytes(&[
            ("first.txt", Some(b"first".as_slice())),
            ("second.txt", Some(b"second".as_slice())),
        ]),
    )
    .await;

    let resp = request_personal_archive_preview(&app, &token, file_id).await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    aster_drive::services::task_service::drain(&state)
        .await
        .expect("archive preview task should drain");
    let resp = request_personal_archive_preview(&app, &token, file_id).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["error"]["subcode"], "archive_preview.rejected");
}

#[actix_web::test]
async fn test_archive_preview_truncates_manifest_to_configured_limit() {
    let state = common::setup().await;
    enable_archive_preview(&state, true, false).await;
    aster_drive::services::config_service::set(
        &state,
        "archive_preview_max_manifest_bytes",
        "700",
        1,
    )
    .await
    .expect("archive preview manifest limit should update");
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let file_id = upload_bytes(
        &app,
        &token,
        "large-manifest.zip",
        "application/zip",
        create_many_entry_zip_bytes(20),
    )
    .await;

    let resp = request_personal_archive_preview(&app, &token, file_id).await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    aster_drive::services::task_service::drain(&state)
        .await
        .expect("archive preview task should drain");
    let resp = request_personal_archive_preview(&app, &token, file_id).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let data = &body["data"];
    assert_eq!(data["truncated"], true);
    assert_eq!(data["entry_count"], 20);
    assert!(
        data["entries"].as_array().unwrap().len() < 20,
        "manifest should keep counts but trim displayed entries"
    );
}

#[actix_web::test]
async fn test_archive_preview_caps_high_manifest_limit_to_cache_storage_limit() {
    let state = common::setup().await;
    enable_archive_preview(&state, true, false).await;
    aster_drive::services::config_service::set(
        &state,
        "archive_preview_max_manifest_bytes",
        "1048576",
        1,
    )
    .await
    .expect("archive preview manifest limit should update");
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let file_id = upload_bytes(
        &app,
        &token,
        "cache-boundary.zip",
        "application/zip",
        create_many_long_entry_zip_bytes(900),
    )
    .await;

    let resp = request_personal_archive_preview(&app, &token, file_id).await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    aster_drive::services::task_service::drain(&state)
        .await
        .expect("archive preview task should drain");
    let tasks = archive_preview_tasks(&state).await;
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].status, BackgroundTaskStatus::Succeeded);

    let resp = request_personal_archive_preview(&app, &token, file_id).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    let data = &body["data"];
    assert_eq!(data["truncated"], true);
    assert_eq!(data["entry_count"], 900);
    assert!(
        data["entries"].as_array().unwrap().len() < 900,
        "high configured manifest limit should still trim to the cacheable storage limit"
    );

    let cached = property_repo::find_by_key(
        state.writer_db(),
        EntityType::File,
        file_id,
        "system.archive_preview",
        "zip_raw_manifest.v2",
    )
    .await
    .expect("cache lookup should succeed")
    .expect("archive preview manifest should be cached");
    assert!(
        cached.value.expect("cache value should be present").len() <= 65_536,
        "cached archive preview wrapper must fit entity_property.value"
    );
}

#[actix_web::test]
async fn test_archive_preview_reports_manifest_limit_too_small_with_subcode() {
    let state = common::setup().await;
    enable_archive_preview(&state, true, false).await;
    aster_drive::services::config_service::set(
        &state,
        "archive_preview_max_manifest_bytes",
        "10",
        1,
    )
    .await
    .expect("archive preview manifest limit should update");
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let file_id = upload_bytes(
        &app,
        &token,
        "tiny-limit.zip",
        "application/zip",
        create_stored_zip_bytes(&[("payload.txt", Some(b"payload".as_slice()))]),
    )
    .await;

    let resp = request_personal_archive_preview(&app, &token, file_id).await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    aster_drive::services::task_service::drain(&state)
        .await
        .expect("archive preview task should drain");
    let resp = request_personal_archive_preview(&app, &token, file_id).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["error"]["subcode"],
        "archive_preview.manifest_too_large"
    );
}

#[actix_web::test]
async fn test_archive_preview_share_toggle_is_separate_from_user_toggle() {
    let state = common::setup().await;
    enable_archive_preview(&state, true, false).await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let file_id = upload_bytes(
        &app,
        &token,
        "shared.zip",
        "application/zip",
        create_stored_zip_bytes(&[("shared.txt", Some(b"shared".as_slice()))]),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "target": { "type": "file", "id": file_id } }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"].as_str().unwrap();

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/archive-preview"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["error"]["subcode"], "archive_preview.share_disabled");

    aster_drive::services::config_service::set(&state, "archive_preview_share_enabled", "true", 1)
        .await
        .expect("archive preview share config should update");
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/archive-preview"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    assert_eq!(
        resp.headers()
            .get("Retry-After")
            .and_then(|value| value.to_str().ok()),
        Some("2")
    );
    aster_drive::services::task_service::drain(&state)
        .await
        .expect("shared archive preview task should drain");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/archive-preview"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("Cache-Control")
            .and_then(|value| value.to_str().ok()),
        Some("public, max-age=0, must-revalidate")
    );
    let etag = resp
        .headers()
        .get("ETag")
        .and_then(|value| value.to_str().ok())
        .expect("shared archive preview should include ETag")
        .to_string();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["entries"][0]["path"], "shared.txt");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/archive-preview"))
        .insert_header(("If-None-Match", etag.as_str()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
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
async fn test_archive_preview_folder_share_file_uses_public_cache_and_etag() {
    let state = common::setup().await;
    enable_archive_preview(&state, true, true).await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let folder_id = create_folder(&app, &token, "Shared Folder", None).await;
    let file_id = upload_bytes_to_folder(
        &app,
        &token,
        Some(folder_id),
        "nested.zip",
        "application/zip",
        create_stored_zip_bytes(&[("nested.txt", Some(b"nested".as_slice()))]),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "target": { "type": "folder", "id": folder_id } }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"].as_str().unwrap();

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/s/{share_token}/files/{file_id}/archive-preview"
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    aster_drive::services::task_service::drain(&state)
        .await
        .expect("folder shared archive preview task should drain");

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/s/{share_token}/files/{file_id}/archive-preview"
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("Cache-Control")
            .and_then(|value| value.to_str().ok()),
        Some("public, max-age=0, must-revalidate")
    );
    let etag = resp
        .headers()
        .get("ETag")
        .and_then(|value| value.to_str().ok())
        .expect("folder share archive preview should include ETag")
        .to_string();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["entries"][0]["path"], "nested.txt");

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/s/{share_token}/files/{file_id}/archive-preview"
        ))
        .insert_header(("If-None-Match", etag.as_str()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(
        resp.headers()
            .get("ETag")
            .and_then(|value| value.to_str().ok()),
        Some(etag.as_str())
    );
}

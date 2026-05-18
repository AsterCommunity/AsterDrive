//! Background task integration tests

#[macro_use]
mod common;

use actix_web::{App, test, web};
use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};
use serde_json::Value;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use tokio::sync::broadcast;

use aster_drive::config::operations::{
    BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY_KEY, BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY_KEY,
};
use aster_drive::db::repository::background_task_repo;
use aster_drive::entities::background_task;
use aster_drive::services::task_service::{
    self, RuntimeSystemHealthComponent, RuntimeSystemHealthResult, RuntimeSystemHealthStatus,
    RuntimeTaskRunOutcome,
};
use aster_drive::types::{BackgroundTaskKind, BackgroundTaskStatus, StoredTaskPayload};

macro_rules! register_user {
    ($app:expr, $db:expr, $mail_sender:expr, $username:expr, $email:expr, $password:expr) => {{
        let req = test::TestRequest::post()
            .uri("/api/v1/auth/register")
            .peer_addr("127.0.0.1:12345".parse().unwrap())
            .set_json(serde_json::json!({
                "username": $username,
                "email": $email,
                "password": $password
            }))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 201);
        let _body: Value = test::read_body_json(resp).await;
        let _ = confirm_latest_contact_verification!($app, $db, $mail_sender);
    }};
}

macro_rules! login_user {
    ($app:expr, $identifier:expr, $password:expr) => {{
        let req = test::TestRequest::post()
            .uri("/api/v1/auth/login")
            .peer_addr("127.0.0.1:12345".parse().unwrap())
            .set_json(serde_json::json!({
                "identifier": $identifier,
                "password": $password
            }))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 200);
        common::extract_cookie(&resp, "aster_access").unwrap()
    }};
}

macro_rules! multipart_request {
    ($uri:expr, $token:expr, $filename:expr, $content:expr $(,)?) => {{
        let boundary = "----TaskBoundary123";
        let payload = format!(
            "------TaskBoundary123\r\n\
             Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n\
             Content-Type: text/plain\r\n\r\n\
             {content}\r\n\
             ------TaskBoundary123--\r\n",
            filename = $filename,
            content = $content,
        );

        test::TestRequest::post()
            .uri($uri)
            .insert_header(("Cookie", common::access_cookie_header(&$token)))
            .insert_header(common::csrf_header_for(&$token))
            .insert_header((
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            ))
            .set_payload(payload)
            .to_request()
    }};
}

fn zip_entry_names(bytes: &[u8]) -> Vec<String> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes.to_vec())).expect("zip archive should be readable");
    let mut names = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        names.push(
            archive
                .by_index(index)
                .expect("zip entry should exist")
                .name()
                .to_string(),
        );
    }
    names.sort();
    names
}

fn read_zip_entry_text(bytes: &[u8], name: &str) -> String {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes.to_vec())).expect("zip archive should be readable");
    let mut entry = archive.by_name(name).expect("zip entry should exist");
    let mut content = String::new();
    entry
        .read_to_string(&mut content)
        .expect("zip entry should be readable as utf-8 text");
    content
}

fn read_archive_download_path(body: &Value) -> String {
    body["data"]["token"]
        .as_str()
        .expect("ticket token should exist");
    body["data"]["download_path"]
        .as_str()
        .expect("download path should exist")
        .to_string()
}

fn read_task_result(body: &Value) -> Value {
    body["data"]["result"].clone()
}

fn read_task_steps(body: &Value) -> Vec<(String, String)> {
    body["data"]["steps"]
        .as_array()
        .expect("task steps should exist")
        .iter()
        .map(|step| {
            (
                step["key"]
                    .as_str()
                    .expect("task step key should exist")
                    .to_string(),
                step["status"]
                    .as_str()
                    .expect("task step status should exist")
                    .to_string(),
            )
        })
        .collect()
}

fn assert_task_steps(body: &Value, expected: &[(&str, &str)]) {
    let actual = read_task_steps(body);
    let expected = expected
        .iter()
        .map(|(key, status)| (key.to_string(), status.to_string()))
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

async fn drain_storage_change_events(
    rx: &mut tokio::sync::broadcast::Receiver<
        aster_drive::services::storage_change_service::StorageChangeEvent,
    >,
) {
    while let Ok(Ok(_)) | Ok(Err(broadcast::error::RecvError::Lagged(_))) =
        tokio::time::timeout(std::time::Duration::from_millis(10), rx.recv()).await
    {}
}

fn create_zip_bytes(entries: &[(&str, Option<&[u8]>)]) -> Vec<u8> {
    let cursor = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(cursor);
    let file_options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let dir_options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    for (path, content) in entries {
        match content {
            Some(bytes) => {
                zip.start_file(*path, file_options)
                    .expect("zip entry should start");
                zip.write_all(bytes).expect("zip entry should be writable");
            }
            None => {
                zip.add_directory(*path, dir_options)
                    .expect("zip directory should be writable");
            }
        }
    }

    zip.finish().expect("zip writer should finish").into_inner()
}

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

fn patch_zip_central_external_attrs(bytes: &mut [u8], path: &str, external_attrs: u32) {
    let path = path.as_bytes();
    let central_signature = [0x50, 0x4B, 0x01, 0x02];
    let mut patched = false;

    for index in 0..bytes.len().saturating_sub(central_signature.len()) {
        if !bytes[index..].starts_with(&central_signature) {
            continue;
        }
        let name_len = u16::from_le_bytes([bytes[index + 28], bytes[index + 29]]) as usize;
        let name_start = index + 46;
        let name_end = name_start + name_len;
        if name_end <= bytes.len() && &bytes[name_start..name_end] == path {
            bytes[index + 38..index + 42].copy_from_slice(&external_attrs.to_le_bytes());
            // Mark "version made by" as Unix so readers do not treat the entry as DOS-only.
            bytes[index + 5] = 3;
            patched = true;
            break;
        }
    }

    assert!(patched, "central directory entry should exist");
}

fn find_zip_local_header(bytes: &[u8], path: &str) -> usize {
    let path = path.as_bytes();
    let local_signature = [0x50, 0x4B, 0x03, 0x04];

    for index in 0..bytes.len().saturating_sub(local_signature.len()) {
        if !bytes[index..].starts_with(&local_signature) || index + 30 > bytes.len() {
            continue;
        }
        let name_len = u16::from_le_bytes([bytes[index + 26], bytes[index + 27]]) as usize;
        let name_start = index + 30;
        let name_end = name_start + name_len;
        if name_end <= bytes.len() && &bytes[name_start..name_end] == path {
            return index;
        }
    }

    panic!("local zip entry header should exist");
}

fn find_zip_central_header(bytes: &[u8], path: &str) -> usize {
    let path = path.as_bytes();
    let central_signature = [0x50, 0x4B, 0x01, 0x02];

    for index in 0..bytes.len().saturating_sub(central_signature.len()) {
        if !bytes[index..].starts_with(&central_signature) || index + 46 > bytes.len() {
            continue;
        }
        let name_len = u16::from_le_bytes([bytes[index + 28], bytes[index + 29]]) as usize;
        let name_start = index + 46;
        let name_end = name_start + name_len;
        if name_end <= bytes.len() && &bytes[name_start..name_end] == path {
            return index;
        }
    }

    panic!("central zip entry header should exist");
}

fn patch_zip_entry_general_purpose_flag(bytes: &mut [u8], path: &str, flag_mask: u16) {
    let local_header = find_zip_local_header(bytes, path);
    let local_flags = u16::from_le_bytes([bytes[local_header + 6], bytes[local_header + 7]]);
    bytes[local_header + 6..local_header + 8]
        .copy_from_slice(&(local_flags | flag_mask).to_le_bytes());

    let central_header = find_zip_central_header(bytes, path);
    let central_flags = u16::from_le_bytes([bytes[central_header + 8], bytes[central_header + 9]]);
    bytes[central_header + 8..central_header + 10]
        .copy_from_slice(&(central_flags | flag_mask).to_le_bytes());
}

fn patch_zip_entry_compression_method(bytes: &mut [u8], path: &str, method: u16) {
    let local_header = find_zip_local_header(bytes, path);
    bytes[local_header + 8..local_header + 10].copy_from_slice(&method.to_le_bytes());

    let central_header = find_zip_central_header(bytes, path);
    bytes[central_header + 10..central_header + 12].copy_from_slice(&method.to_le_bytes());
}

fn create_symlink_zip_bytes(path: &str, target: &str) -> Vec<u8> {
    let mut bytes = create_stored_zip_bytes(&[(path, Some(target.as_bytes()))]);
    patch_zip_central_external_attrs(&mut bytes, path, 0o120777_u32 << 16);
    bytes
}

fn create_special_file_zip_bytes(path: &str) -> Vec<u8> {
    let mut bytes = create_stored_zip_bytes(&[(path, Some(b""))]);
    patch_zip_central_external_attrs(&mut bytes, path, 0o060666_u32 << 16);
    bytes
}

fn create_encrypted_flag_zip_bytes(path: &str, content: &[u8]) -> Vec<u8> {
    let mut bytes = create_stored_zip_bytes(&[(path, Some(content))]);
    patch_zip_entry_general_purpose_flag(&mut bytes, path, 0x0001);
    bytes
}

fn create_unsupported_method_zip_bytes(path: &str, content: &[u8]) -> Vec<u8> {
    let mut bytes = create_stored_zip_bytes(&[(path, Some(content))]);
    patch_zip_entry_compression_method(&mut bytes, path, 12);
    bytes
}

#[cfg(unix)]
fn set_directory_writable(path: &Path, writable: bool) {
    use std::os::unix::fs::PermissionsExt;

    let mode = if writable { 0o755 } else { 0o555 };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .expect("test storage directory permissions should update");
}

#[cfg(not(unix))]
fn set_directory_writable(path: &Path, writable: bool) {
    let mut permissions = std::fs::metadata(path)
        .expect("test storage directory metadata should exist")
        .permissions();
    permissions.set_readonly(!writable);
    std::fs::set_permissions(path, permissions)
        .expect("test storage directory permissions should update");
}

fn set_storage_data_directories_writable(
    storage_root: &Path,
    writable: bool,
    writable_subtrees: &[&Path],
) {
    let mut pending = vec![storage_root.to_path_buf()];
    let mut directories = Vec::new();

    while let Some(current) = pending.pop() {
        let is_writable_subtree = writable_subtrees
            .iter()
            .any(|subtree| current == *subtree || current.starts_with(subtree));
        if current != storage_root && is_writable_subtree {
            continue;
        }
        directories.push(current.clone());

        let Ok(children) = std::fs::read_dir(&current) else {
            continue;
        };
        for child in children.flatten() {
            let Ok(file_type) = child.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                pending.push(child.path());
            }
        }
    }

    for directory in directories {
        set_directory_writable(&directory, writable);
    }
}

async fn run_failing_personal_archive_extract(
    archive_bytes: Vec<u8>,
    config_overrides: Vec<(&str, String)>,
) -> Value {
    run_failing_personal_archive_extract_with_options(archive_bytes, config_overrides, "1", false)
        .await
}

async fn run_failing_personal_archive_extract_with_options(
    archive_bytes: Vec<u8>,
    config_overrides: Vec<(&str, String)>,
    max_attempts: &str,
    assert_retry_rejected: bool,
) -> Value {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    aster_drive::services::config_service::set(
        &state,
        "background_task_max_attempts",
        max_attempts,
        1,
    )
    .await
    .expect("background task max attempts config should update");
    for (key, value) in config_overrides {
        aster_drive::services::config_service::set(&state, key, value.as_str(), 1)
            .await
            .expect("archive extract limit config should update");
    }

    let (token, _) = register_and_login!(app);
    let req = test::TestRequest::post()
        .uri("/api/v1/files/new")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "security-check.zip", "folder_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let archive_file_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/{archive_file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(archive_bytes)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{archive_file_id}/extract"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let task_id = body["data"]["id"].as_i64().unwrap();

    let stats = aster_drive::services::task_service::drain(&state)
        .await
        .expect("task drain should succeed");
    assert_eq!(stats.failed, 1);
    assert_eq!(stats.retried, 0);
    assert_eq!(stats.succeeded, 0);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/tasks/{task_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "failed");
    assert_eq!(body["data"]["attempt_count"], 1);
    assert_eq!(body["data"]["can_retry"], false);
    if assert_retry_rejected {
        let req = test::TestRequest::post()
            .uri(&format!("/api/v1/tasks/{task_id}/retry"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    let task_temp_dir =
        aster_drive::utils::paths::task_temp_dir(&state.config.server.temp_dir, task_id);
    assert!(
        !std::path::Path::new(&task_temp_dir).exists(),
        "failed extract task should cleanup temp dir"
    );

    body
}

fn create_zip_bytes_with_tampered_declared_size(
    path: &str,
    actual_content: &[u8],
    declared_size: u32,
) -> Vec<u8> {
    let mut bytes = create_zip_bytes(&[(path, Some(actual_content))]);
    let local_signature = [0x50, 0x4B, 0x03, 0x04];
    let central_signature = [0x50, 0x4B, 0x01, 0x02];
    let local_header = bytes
        .windows(local_signature.len())
        .position(|window| window == local_signature)
        .expect("local file header should exist");
    let central_header = bytes
        .windows(central_signature.len())
        .position(|window| window == central_signature)
        .expect("central directory header should exist");
    let declared_size_bytes = declared_size.to_le_bytes();
    bytes[local_header + 22..local_header + 26].copy_from_slice(&declared_size_bytes);
    bytes[central_header + 24..central_header + 28].copy_from_slice(&declared_size_bytes);
    bytes
}

fn healthy_system_health_result() -> RuntimeSystemHealthResult {
    RuntimeSystemHealthResult {
        status: RuntimeSystemHealthStatus::Healthy,
        components: vec![RuntimeSystemHealthComponent {
            name: "database".to_string(),
            status: RuntimeSystemHealthStatus::Healthy,
            message: "database ping succeeded".to_string(),
        }],
    }
}

fn utc_now_at_db_precision() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp_micros(Utc::now().timestamp_micros())
        .expect("current timestamp should fit in chrono range")
}

async fn insert_processing_task(
    state: &aster_drive::runtime::PrimaryAppState,
    processing_started_at: chrono::DateTime<chrono::Utc>,
    last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
) -> i64 {
    let now = Utc::now();
    background_task_repo::create(
        &state.db,
        background_task::ActiveModel {
            kind: Set(BackgroundTaskKind::ArchiveCompress),
            status: Set(BackgroundTaskStatus::Processing),
            creator_user_id: Set(None),
            team_id: Set(None),
            share_id: Set(None),
            display_name: Set("heartbeat-test".to_string()),
            payload_json: Set(StoredTaskPayload("{}".to_string())),
            result_json: Set(None),
            steps_json: Set(None),
            progress_current: Set(0),
            progress_total: Set(0),
            status_text: Set(None),
            attempt_count: Set(0),
            max_attempts: Set(1),
            next_run_at: Set(now),
            processing_token: Set(0),
            processing_started_at: Set(Some(processing_started_at)),
            last_heartbeat_at: Set(last_heartbeat_at),
            started_at: Set(Some(processing_started_at)),
            finished_at: Set(None),
            last_error: Set(None),
            failure_can_retry: Set(None),
            expires_at: Set(now + Duration::hours(1)),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .expect("processing task should be inserted")
    .id
}

async fn insert_pending_dispatch_task(
    state: &aster_drive::runtime::PrimaryAppState,
    kind: BackgroundTaskKind,
    max_attempts: i32,
) -> background_task::Model {
    let now = Utc::now();
    let (display_name, payload_json) = match kind {
        BackgroundTaskKind::ArchiveCompress => (
            "dispatch-archive.zip",
            serde_json::to_string(
                &aster_drive::services::task_service::ArchiveCompressTaskPayload {
                    file_ids: Vec::new(),
                    folder_ids: Vec::new(),
                    archive_name: "dispatch-archive.zip".to_string(),
                    target_folder_id: None,
                },
            )
            .expect("archive dispatch payload should serialize"),
        ),
        BackgroundTaskKind::SystemRuntime => (
            "dispatch-test",
            r#"{"task_name":"dispatch-test"}"#.to_string(),
        ),
        _ => panic!("unsupported dispatch test task kind"),
    };
    background_task_repo::create(
        &state.db,
        background_task::ActiveModel {
            kind: Set(kind),
            status: Set(BackgroundTaskStatus::Pending),
            creator_user_id: Set(match kind {
                BackgroundTaskKind::ArchiveCompress => Some(1),
                _ => None,
            }),
            team_id: Set(None),
            share_id: Set(None),
            display_name: Set(display_name.to_string()),
            payload_json: Set(StoredTaskPayload(payload_json)),
            result_json: Set(None),
            steps_json: Set(None),
            progress_current: Set(0),
            progress_total: Set(0),
            status_text: Set(None),
            attempt_count: Set(0),
            max_attempts: Set(max_attempts),
            next_run_at: Set(now - Duration::seconds(1)),
            processing_token: Set(0),
            processing_started_at: Set(None),
            last_heartbeat_at: Set(None),
            started_at: Set(None),
            finished_at: Set(None),
            last_error: Set(None),
            failure_can_retry: Set(None),
            expires_at: Set(now + Duration::hours(1)),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .expect("pending dispatch task should be inserted")
}

async fn insert_pending_lane_task(
    state: &aster_drive::runtime::PrimaryAppState,
    kind: BackgroundTaskKind,
    display_name: &str,
    payload_json: &str,
) -> background_task::Model {
    let now = Utc::now();
    background_task_repo::create(
        &state.db,
        background_task::ActiveModel {
            kind: Set(kind),
            status: Set(BackgroundTaskStatus::Pending),
            creator_user_id: Set(None),
            team_id: Set(None),
            share_id: Set(None),
            display_name: Set(display_name.to_string()),
            payload_json: Set(StoredTaskPayload(payload_json.to_string())),
            result_json: Set(None),
            steps_json: Set(None),
            progress_current: Set(0),
            progress_total: Set(0),
            status_text: Set(None),
            attempt_count: Set(0),
            max_attempts: Set(1),
            next_run_at: Set(now - Duration::seconds(1)),
            processing_token: Set(0),
            processing_started_at: Set(None),
            last_heartbeat_at: Set(None),
            lease_expires_at: Set(None),
            started_at: Set(None),
            finished_at: Set(None),
            last_error: Set(None),
            failure_can_retry: Set(None),
            expires_at: Set(now + Duration::hours(1)),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .expect("pending lane task should be inserted")
}

async fn assert_response_status(
    resp: actix_web::dev::ServiceResponse,
    expected: actix_web::http::StatusCode,
) -> actix_web::dev::ServiceResponse {
    let status = resp.status();
    if status != expected {
        let body = test::read_body(resp).await;
        panic!(
            "expected status {}, got {} with body: {}",
            expected,
            status,
            String::from_utf8_lossy(&body)
        );
    }
    resp
}

#[actix_web::test]
async fn test_processing_task_claimability_requires_explicit_lease_expiry() {
    let state = common::setup().await;
    let now = Utc::now();
    let stale_before = now - Duration::seconds(60);
    let processing_started_at = now - Duration::seconds(180);

    let fresh_heartbeat_without_lease = insert_processing_task(
        &state,
        processing_started_at,
        Some(now - Duration::seconds(5)),
    )
    .await;
    let no_heartbeat_without_lease =
        insert_processing_task(&state, processing_started_at, None).await;
    let stale_heartbeat_without_lease = insert_processing_task(
        &state,
        processing_started_at,
        Some(now - Duration::seconds(120)),
    )
    .await;

    let claimable = background_task_repo::list_claimable(&state.db, now, stale_before, 10)
        .await
        .expect("claimable task list should load");
    let ids = claimable.iter().map(|task| task.id).collect::<Vec<_>>();

    assert!(!ids.contains(&fresh_heartbeat_without_lease));
    assert!(!ids.contains(&no_heartbeat_without_lease));
    assert!(!ids.contains(&stale_heartbeat_without_lease));
}

#[actix_web::test]
async fn test_processing_task_claimability_prefers_explicit_lease_expiry_when_present() {
    let state = common::setup().await;
    let now = Utc::now();
    let stale_before = now - Duration::seconds(60);

    let fresh_lease = background_task_repo::create(
        &state.db,
        background_task::ActiveModel {
            kind: Set(BackgroundTaskKind::ArchiveCompress),
            status: Set(BackgroundTaskStatus::Processing),
            creator_user_id: Set(None),
            team_id: Set(None),
            share_id: Set(None),
            display_name: Set("fresh-lease".to_string()),
            payload_json: Set(StoredTaskPayload("{}".to_string())),
            result_json: Set(None),
            steps_json: Set(None),
            progress_current: Set(0),
            progress_total: Set(0),
            status_text: Set(None),
            attempt_count: Set(0),
            max_attempts: Set(1),
            next_run_at: Set(now - Duration::seconds(120)),
            processing_token: Set(1),
            processing_started_at: Set(Some(now - Duration::seconds(180))),
            last_heartbeat_at: Set(Some(now - Duration::seconds(120))),
            lease_expires_at: Set(Some(now + Duration::seconds(30))),
            started_at: Set(Some(now - Duration::seconds(180))),
            finished_at: Set(None),
            last_error: Set(None),
            expires_at: Set(now + Duration::hours(1)),
            created_at: Set(now - Duration::seconds(180)),
            updated_at: Set(now - Duration::seconds(120)),
            ..Default::default()
        },
    )
    .await
    .expect("fresh explicit lease task should be inserted");

    let expired_lease = background_task_repo::create(
        &state.db,
        background_task::ActiveModel {
            kind: Set(BackgroundTaskKind::ArchiveCompress),
            status: Set(BackgroundTaskStatus::Processing),
            creator_user_id: Set(None),
            team_id: Set(None),
            share_id: Set(None),
            display_name: Set("expired-lease".to_string()),
            payload_json: Set(StoredTaskPayload("{}".to_string())),
            result_json: Set(None),
            steps_json: Set(None),
            progress_current: Set(0),
            progress_total: Set(0),
            status_text: Set(None),
            attempt_count: Set(0),
            max_attempts: Set(1),
            next_run_at: Set(now - Duration::seconds(120)),
            processing_token: Set(1),
            processing_started_at: Set(Some(now - Duration::seconds(30))),
            last_heartbeat_at: Set(Some(now - Duration::seconds(5))),
            lease_expires_at: Set(Some(now - Duration::seconds(1))),
            started_at: Set(Some(now - Duration::seconds(30))),
            finished_at: Set(None),
            last_error: Set(None),
            expires_at: Set(now + Duration::hours(1)),
            created_at: Set(now - Duration::seconds(30)),
            updated_at: Set(now - Duration::seconds(5)),
            ..Default::default()
        },
    )
    .await
    .expect("expired explicit lease task should be inserted");

    let claimable = background_task_repo::list_claimable(&state.db, now, stale_before, 10)
        .await
        .expect("claimable task list should load");
    let ids = claimable.iter().map(|task| task.id).collect::<Vec<_>>();

    assert!(!ids.contains(&fresh_lease.id));
    assert!(ids.contains(&expired_lease.id));
}

#[actix_web::test]
async fn test_touch_heartbeat_refreshes_processing_task_liveness() {
    let state = common::setup().await;
    let now = Utc::now();
    let old_heartbeat = now - Duration::seconds(120);
    let task_id =
        insert_processing_task(&state, now - Duration::seconds(180), Some(old_heartbeat)).await;

    let touched_at = Utc::now();
    let touched = background_task_repo::touch_heartbeat(
        &state.db,
        task_id,
        0,
        touched_at,
        touched_at + Duration::seconds(60),
    )
    .await
    .expect("heartbeat touch should succeed");
    assert!(touched);

    let task = background_task_repo::find_by_id(&state.db, task_id)
        .await
        .expect("task should still exist");
    assert!(
        task.last_heartbeat_at
            .expect("heartbeat timestamp should exist")
            > old_heartbeat
    );
    assert!(
        task.lease_expires_at
            .expect("lease expiry should exist after heartbeat")
            > touched_at
    );
}

#[actix_web::test]
async fn test_processing_token_fences_stale_worker_updates_after_reclaim() {
    let state = common::setup().await;
    let now = Utc::now();
    let stale_before = now - Duration::seconds(60);
    let task = background_task_repo::create(
        &state.db,
        background_task::ActiveModel {
            kind: Set(BackgroundTaskKind::ArchiveCompress),
            status: Set(BackgroundTaskStatus::Processing),
            creator_user_id: Set(None),
            team_id: Set(None),
            share_id: Set(None),
            display_name: Set("fencing-test".to_string()),
            payload_json: Set(StoredTaskPayload("{}".to_string())),
            result_json: Set(None),
            steps_json: Set(None),
            progress_current: Set(0),
            progress_total: Set(0),
            status_text: Set(None),
            attempt_count: Set(0),
            max_attempts: Set(1),
            next_run_at: Set(now - Duration::seconds(120)),
            processing_token: Set(1),
            processing_started_at: Set(Some(now - Duration::seconds(180))),
            last_heartbeat_at: Set(Some(now - Duration::seconds(120))),
            lease_expires_at: Set(Some(now - Duration::seconds(1))),
            started_at: Set(Some(now - Duration::seconds(180))),
            finished_at: Set(None),
            last_error: Set(None),
            expires_at: Set(now + Duration::hours(1)),
            created_at: Set(now - Duration::seconds(180)),
            updated_at: Set(now - Duration::seconds(120)),
            ..Default::default()
        },
    )
    .await
    .expect("processing task with token should be inserted");

    let reclaimed = background_task_repo::try_claim(
        &state.db,
        task.id,
        task.processing_token,
        Utc::now(),
        stale_before,
        task.processing_token + 1,
        Utc::now() + Duration::seconds(60),
    )
    .await
    .expect("stale processing task should be reclaimable");
    assert!(reclaimed);

    let stale_touched = background_task_repo::touch_heartbeat(
        &state.db,
        task.id,
        task.processing_token,
        Utc::now(),
        Utc::now() + Duration::seconds(60),
    )
    .await
    .expect("stale worker heartbeat should be rejected");
    assert!(!stale_touched);

    let stale_progress = background_task_repo::mark_progress(
        &state.db,
        background_task_repo::TaskProgressUpdate {
            id: task.id,
            processing_token: task.processing_token,
            now: Utc::now(),
            lease_expires_at: Utc::now() + Duration::seconds(60),
            current: 1,
            total: 2,
            status_text: Some("stale update"),
            steps_json: None,
        },
    )
    .await
    .expect("stale worker progress update should be rejected");
    assert!(!stale_progress);

    let fresh_touched = background_task_repo::touch_heartbeat(
        &state.db,
        task.id,
        task.processing_token + 1,
        Utc::now(),
        Utc::now() + Duration::seconds(60),
    )
    .await
    .expect("current worker heartbeat should still succeed");
    assert!(fresh_touched);

    let stored = background_task_repo::find_by_id(&state.db, task.id)
        .await
        .expect("reclaimed task should still exist");
    assert_eq!(stored.status, BackgroundTaskStatus::Processing);
    assert_eq!(stored.processing_token, task.processing_token + 1);
    assert!(stored.lease_expires_at.is_some());
}

#[actix_web::test]
async fn test_dispatch_due_reclaims_stale_processing_task_with_new_token() {
    let state = common::setup().await;
    let now = Utc::now();
    let task = background_task_repo::create(
        &state.db,
        background_task::ActiveModel {
            kind: Set(BackgroundTaskKind::SystemRuntime),
            status: Set(BackgroundTaskStatus::Processing),
            creator_user_id: Set(None),
            team_id: Set(None),
            share_id: Set(None),
            display_name: Set("stale-dispatch-test".to_string()),
            payload_json: Set(StoredTaskPayload(
                r#"{"task_name":"stale-dispatch-test"}"#.to_string(),
            )),
            result_json: Set(None),
            steps_json: Set(None),
            progress_current: Set(0),
            progress_total: Set(0),
            status_text: Set(None),
            attempt_count: Set(0),
            max_attempts: Set(3),
            next_run_at: Set(now - Duration::seconds(120)),
            processing_token: Set(4),
            processing_started_at: Set(Some(now - Duration::seconds(180))),
            last_heartbeat_at: Set(Some(now - Duration::seconds(120))),
            lease_expires_at: Set(Some(now - Duration::seconds(1))),
            started_at: Set(Some(now - Duration::seconds(180))),
            finished_at: Set(None),
            last_error: Set(None),
            expires_at: Set(now + Duration::hours(1)),
            created_at: Set(now - Duration::seconds(180)),
            updated_at: Set(now - Duration::seconds(120)),
            ..Default::default()
        },
    )
    .await
    .expect("stale processing dispatch task should be inserted");

    let stats = task_service::dispatch_due(&state)
        .await
        .expect("dispatch should reclaim stale processing task");

    assert_eq!(stats.claimed, 1);
    assert_eq!(stats.retried, 0);
    assert_eq!(stats.failed, 1);
    assert_eq!(stats.succeeded, 0);

    let stored = background_task_repo::find_by_id(&state.db, task.id)
        .await
        .expect("reclaimed task should still exist");
    assert_eq!(stored.status, BackgroundTaskStatus::Failed);
    assert_eq!(stored.processing_token, 5);
    assert_eq!(stored.attempt_count, 1);
    assert_eq!(stored.failure_can_retry, Some(false));
    assert!(stored.processing_started_at.is_none());
    assert!(stored.last_heartbeat_at.is_none());
    assert!(stored.lease_expires_at.is_none());
    assert!(
        stored
            .last_error
            .as_deref()
            .expect("reclaimed stale task should record error")
            .contains("should not be dispatched")
    );
}

#[actix_web::test]
async fn test_dispatch_due_fast_continues_thumbnail_lane() {
    let state = common::setup().await;
    state.runtime_config.apply(common::system_config_model(
        BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY_KEY,
        "5",
    ));

    for index in 0..5 {
        insert_pending_lane_task(
            &state,
            BackgroundTaskKind::ThumbnailGenerate,
            &format!("thumbnail-fast-continue-{index}"),
            "{}",
        )
        .await;
    }

    let stats = task_service::dispatch_due(&state)
        .await
        .expect("dispatch should fast-continue thumbnail lane");

    assert_eq!(stats.claimed, 5);
    assert_eq!(stats.failed, 5);
    assert_eq!(stats.retried, 0);
    assert_eq!(stats.succeeded, 0);
}

#[actix_web::test]
async fn test_dispatch_due_fast_continues_archive_lane_with_backlog() {
    let state = common::setup().await;
    state.runtime_config.apply(common::system_config_model(
        BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY_KEY,
        "2",
    ));

    for index in 0..3 {
        insert_pending_lane_task(
            &state,
            BackgroundTaskKind::ArchiveCompress,
            &format!("archive-fast-continue-backlog-{index}"),
            "{}",
        )
        .await;
    }

    let stats = task_service::dispatch_due(&state)
        .await
        .expect("dispatch should fast-continue archive lane");

    assert_eq!(stats.claimed, 3);
    assert_eq!(stats.failed, 3);
    assert_eq!(stats.retried, 0);
    assert_eq!(stats.succeeded, 0);
}

#[actix_web::test]
async fn test_cleanup_expired_keeps_terminal_task_records() {
    let state = common::setup().await;
    let now = Utc::now();
    let task = background_task_repo::create(
        &state.db,
        background_task::ActiveModel {
            kind: Set(BackgroundTaskKind::ArchiveCompress),
            status: Set(BackgroundTaskStatus::Succeeded),
            creator_user_id: Set(None),
            team_id: Set(None),
            share_id: Set(None),
            display_name: Set("expired-task".to_string()),
            payload_json: Set(StoredTaskPayload("{}".to_string())),
            result_json: Set(None),
            steps_json: Set(None),
            progress_current: Set(1),
            progress_total: Set(1),
            status_text: Set(Some("finished".to_string())),
            attempt_count: Set(0),
            max_attempts: Set(1),
            next_run_at: Set(now - Duration::hours(2)),
            processing_started_at: Set(None),
            last_heartbeat_at: Set(None),
            started_at: Set(Some(now - Duration::hours(2))),
            finished_at: Set(Some(now - Duration::hours(2))),
            last_error: Set(None),
            expires_at: Set(now - Duration::hours(1)),
            created_at: Set(now - Duration::hours(2)),
            updated_at: Set(now - Duration::hours(2)),
            ..Default::default()
        },
    )
    .await
    .expect("expired task should be inserted");

    let task_temp_dir =
        aster_drive::utils::paths::task_temp_dir(&state.config.server.temp_dir, task.id);
    std::fs::create_dir_all(&task_temp_dir).expect("task temp dir should be created");
    std::fs::write(format!("{task_temp_dir}/artifact.tmp"), b"expired")
        .expect("task temp artifact should be written");

    let cleaned = task_service::cleanup_expired(&state)
        .await
        .expect("task cleanup should succeed");
    assert_eq!(cleaned, 1);
    assert!(
        !std::path::Path::new(&task_temp_dir).exists(),
        "expired task temp dir should be removed"
    );

    let stored = background_task_repo::find_by_id(&state.db, task.id)
        .await
        .expect("expired task record should still exist");
    assert_eq!(stored.status, BackgroundTaskStatus::Succeeded);

    let cleaned_again = task_service::cleanup_expired(&state)
        .await
        .expect("repeated task cleanup should still succeed");
    assert_eq!(cleaned_again, 0);
}

#[actix_web::test]
async fn test_record_runtime_task_run_persists_system_runtime_event() {
    let state = common::setup().await;
    aster_drive::services::config_service::set(&state, "background_task_max_attempts", "5", 1)
        .await
        .expect("background task max attempts config should update");
    let started_at = Utc::now() - Duration::seconds(2);
    let finished_at = Utc::now();

    let recorded = task_service::record_runtime_task_run(
        &state,
        "trash-cleanup",
        started_at,
        finished_at,
        &RuntimeTaskRunOutcome::succeeded(Some("cleaned up 3 expired trash entries".to_string())),
    )
    .await
    .expect("runtime task event should be recorded")
    .expect("runtime task event should not be quiet");

    assert_eq!(recorded.kind, BackgroundTaskKind::SystemRuntime);
    assert_eq!(recorded.status, BackgroundTaskStatus::Succeeded);
    assert_eq!(recorded.display_name, "Trash cleanup");
    assert_eq!(recorded.max_attempts, 1);
    assert!(recorded.lease_expires_at.is_none());
    assert_eq!(
        serde_json::from_str::<Value>(recorded.payload_json.as_ref())
            .expect("runtime payload should be valid json")["task_name"],
        "trash-cleanup"
    );
    let result = serde_json::from_str::<Value>(recorded.result_json.as_ref().unwrap().as_ref())
        .expect("runtime result should be valid json");
    assert_eq!(result["summary"], "cleaned up 3 expired trash entries");
    assert!(result["duration_ms"].as_i64().unwrap() >= 0);
}

#[actix_web::test]
async fn test_record_runtime_task_run_skips_quiet_outcome() {
    let state = common::setup().await;
    let started_at = Utc::now() - Duration::seconds(1);
    let finished_at = Utc::now();

    let recorded = task_service::record_runtime_task_run(
        &state,
        "background-task-dispatch",
        started_at,
        finished_at,
        &RuntimeTaskRunOutcome::quiet(),
    )
    .await
    .expect("quiet runtime task handling should succeed");

    assert!(recorded.is_none());
    let recent = background_task_repo::list_recent(&state.db, 10)
        .await
        .expect("recent background tasks should load");
    assert!(recent.is_empty());
}

#[actix_web::test]
async fn test_record_runtime_task_run_refreshes_latest_healthy_system_check() {
    let state = common::setup().await;
    let first_started_at = utc_now_at_db_precision() - Duration::seconds(6);
    let first_finished_at = utc_now_at_db_precision() - Duration::seconds(5);

    let first = task_service::record_runtime_task_run(
        &state,
        "system-health-check",
        first_started_at,
        first_finished_at,
        &RuntimeTaskRunOutcome::succeeded_with_system_health(
            Some("system healthy".to_string()),
            healthy_system_health_result(),
        ),
    )
    .await
    .expect("first system health event should be recorded")
    .expect("first healthy check should create a record");

    let second_started_at = utc_now_at_db_precision() - Duration::seconds(1);
    let second_finished_at = utc_now_at_db_precision();
    let second = task_service::record_runtime_task_run(
        &state,
        "system-health-check",
        second_started_at,
        second_finished_at,
        &RuntimeTaskRunOutcome::succeeded_with_system_health(
            Some("system healthy".to_string()),
            healthy_system_health_result(),
        ),
    )
    .await
    .expect("second system health event should refresh latest record")
    .expect("second healthy check should return the refreshed record");

    assert_eq!(second.id, first.id);
    assert_eq!(second.status, BackgroundTaskStatus::Succeeded);
    assert_eq!(second.started_at, Some(second_started_at));
    assert_eq!(second.finished_at, Some(second_finished_at));
    assert_eq!(second.updated_at, second_finished_at);

    let recent = background_task_repo::list_recent(&state.db, 10)
        .await
        .expect("recent background tasks should load");
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].id, first.id);
}

#[actix_web::test]
async fn test_record_runtime_task_run_keeps_health_failure_history_before_recovery() {
    let state = common::setup().await;
    let failed = task_service::record_runtime_task_run(
        &state,
        "system-health-check",
        Utc::now() - Duration::seconds(5),
        Utc::now() - Duration::seconds(4),
        &RuntimeTaskRunOutcome::failed_with_system_health(
            Some("cache degraded".to_string()),
            "cache=degraded: fallback active",
            RuntimeSystemHealthResult {
                status: RuntimeSystemHealthStatus::Degraded,
                components: vec![RuntimeSystemHealthComponent {
                    name: "cache".to_string(),
                    status: RuntimeSystemHealthStatus::Degraded,
                    message: "fallback active".to_string(),
                }],
            },
        ),
    )
    .await
    .expect("failed system health event should be recorded")
    .expect("failed system health should create a record");

    let recovered = task_service::record_runtime_task_run(
        &state,
        "system-health-check",
        Utc::now() - Duration::seconds(1),
        Utc::now(),
        &RuntimeTaskRunOutcome::succeeded_with_system_health(
            Some("system healthy".to_string()),
            healthy_system_health_result(),
        ),
    )
    .await
    .expect("recovered system health event should be recorded")
    .expect("recovery should create a new record after failure");

    assert_ne!(recovered.id, failed.id);
    assert_eq!(recovered.status, BackgroundTaskStatus::Succeeded);

    let recent = background_task_repo::list_recent(&state.db, 10)
        .await
        .expect("recent background tasks should load");
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].id, recovered.id);
    assert_eq!(recent[1].id, failed.id);
}

#[actix_web::test]
async fn test_dispatch_due_marks_manual_retryable_task_failed_without_auto_retry() {
    let state = common::setup().await;
    let task = insert_pending_dispatch_task(&state, BackgroundTaskKind::ArchiveCompress, 3).await;

    let stats = task_service::dispatch_due(&state)
        .await
        .expect("dispatch should succeed");

    assert_eq!(stats.claimed, 1);
    assert_eq!(stats.retried, 0);
    assert_eq!(stats.failed, 1);
    assert_eq!(stats.succeeded, 0);

    let stored = background_task_repo::find_by_id(&state.db, task.id)
        .await
        .expect("failed task should still exist");
    assert_eq!(stored.status, BackgroundTaskStatus::Failed);
    assert_eq!(stored.attempt_count, 1);
    assert_eq!(stored.failure_can_retry, Some(true));
    assert!(stored.finished_at.is_some());
    assert!(stored.processing_started_at.is_none());
    assert!(stored.last_heartbeat_at.is_none());
    assert!(stored.last_error.is_some());
}

#[actix_web::test]
async fn test_dispatch_due_marks_task_failed_after_max_attempts() {
    let state = common::setup().await;
    let task = insert_pending_dispatch_task(&state, BackgroundTaskKind::SystemRuntime, 1).await;

    let stats = task_service::dispatch_due(&state)
        .await
        .expect("dispatch should succeed");

    assert_eq!(stats.claimed, 1);
    assert_eq!(stats.retried, 0);
    assert_eq!(stats.failed, 1);
    assert_eq!(stats.succeeded, 0);

    let stored = background_task_repo::find_by_id(&state.db, task.id)
        .await
        .expect("failed task should still exist");
    assert_eq!(stored.status, BackgroundTaskStatus::Failed);
    assert_eq!(stored.attempt_count, 1);
    assert_eq!(stored.failure_can_retry, Some(false));
    assert!(stored.finished_at.is_some());
    assert!(stored.processing_started_at.is_none());
    assert!(stored.last_heartbeat_at.is_none());
    assert!(
        stored
            .last_error
            .as_deref()
            .expect("failed task should record last error")
            .contains("should not be dispatched")
    );
}

#[actix_web::test]
async fn test_personal_archive_stream_preserves_empty_folders() {
    let state = common::setup().await;
    let db = state.db.clone();
    let mail_sender = state.mail_sender.clone();
    let state = web::Data::new(state);
    let app = test::init_service(
        App::new()
            .app_data(web::PayloadConfig::new(10 * 1024 * 1024))
            .app_data(web::JsonConfig::default().limit(1024 * 1024))
            .app_data(web::Data::clone(&state))
            .configure(move |cfg| aster_drive::api::configure_primary(cfg, &db)),
    )
    .await;

    register_user!(
        app,
        state.db.clone(),
        mail_sender,
        "taskowner",
        "taskowner@example.com",
        "password123"
    );
    let token = login_user!(app, "taskowner", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "bundle", "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let bundle_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "docs", "parent_id": bundle_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let docs_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "empty", "parent_id": bundle_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = multipart_request!(
        &format!("/api/v1/files/upload?folder_id={docs_id}"),
        &token,
        "note.txt",
        "hello from archive task",
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/batch/archive-download")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [],
            "folder_ids": [bundle_id],
            "archive_name": "bundle-export"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let download_path = read_archive_download_path(&body);

    let req = test::TestRequest::get()
        .uri(&download_path)
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = assert_response_status(
        test::call_service(&app, req).await,
        actix_web::http::StatusCode::OK,
    )
    .await;
    assert_eq!(
        resp.headers()
            .get("Content-Type")
            .and_then(|value| value.to_str().ok()),
        Some("application/zip")
    );
    let zip_bytes = test::read_body(resp).await;
    let names = zip_entry_names(&zip_bytes);
    assert_eq!(
        names,
        vec![
            "bundle/",
            "bundle/docs/",
            "bundle/docs/note.txt",
            "bundle/empty/",
        ]
    );
    assert_eq!(
        read_zip_entry_text(&zip_bytes, "bundle/docs/note.txt"),
        "hello from archive task"
    );
}

#[actix_web::test]
async fn test_team_archive_stream_is_scoped_to_team_routes() {
    let state = common::setup().await;
    let db = state.db.clone();
    let mail_sender = state.mail_sender.clone();
    let state = web::Data::new(state);
    let app = test::init_service(
        App::new()
            .app_data(web::PayloadConfig::new(10 * 1024 * 1024))
            .app_data(web::JsonConfig::default().limit(1024 * 1024))
            .app_data(web::Data::clone(&state))
            .configure(move |cfg| aster_drive::api::configure_primary(cfg, &db)),
    )
    .await;

    register_user!(
        app,
        state.db.clone(),
        mail_sender,
        "teamowner",
        "teamowner@example.com",
        "password123"
    );
    let token = login_user!(app, "teamowner", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Ops Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = multipart_request!(
        &format!("/api/v1/teams/{team_id}/files/upload"),
        &token,
        "team.txt",
        "team archive payload",
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let file_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/batch/archive-download"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [file_id],
            "folder_ids": [],
            "archive_name": "ops-export"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let download_path = read_archive_download_path(&body);

    let req = test::TestRequest::get()
        .uri(&download_path)
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = assert_response_status(
        test::call_service(&app, req).await,
        actix_web::http::StatusCode::OK,
    )
    .await;
    let zip_bytes = test::read_body(resp).await;
    assert_eq!(zip_entry_names(&zip_bytes), vec!["team.txt"]);
    assert_eq!(
        read_zip_entry_text(&zip_bytes, "team.txt"),
        "team archive payload"
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/batch/archive-download")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [file_id],
            "folder_ids": [],
            "archive_name": "should-fail"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn test_personal_archive_compress_task_creates_workspace_file() {
    let state = common::setup().await;
    let db = state.db.clone();
    let mail_sender = state.mail_sender.clone();
    let state = web::Data::new(state);
    let app = test::init_service(
        App::new()
            .app_data(web::PayloadConfig::new(10 * 1024 * 1024))
            .app_data(web::JsonConfig::default().limit(1024 * 1024))
            .app_data(web::Data::clone(&state))
            .configure(move |cfg| aster_drive::api::configure_primary(cfg, &db)),
    )
    .await;

    aster_drive::services::config_service::set(
        state.get_ref(),
        "background_task_max_attempts",
        "5",
        1,
    )
    .await
    .expect("background task max attempts config should update");

    register_user!(
        app,
        state.db.clone(),
        mail_sender,
        "compressor",
        "compressor@example.com",
        "password123"
    );
    let token = login_user!(app, "compressor", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "bundle", "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let bundle_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "docs", "parent_id": bundle_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let docs_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "empty", "parent_id": bundle_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = multipart_request!(
        &format!("/api/v1/files/upload?folder_id={docs_id}"),
        &token,
        "note.txt",
        "hello from archive compress task",
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let archive_wakeup = state.background_task_dispatch_wakeup.notified();
    tokio::pin!(archive_wakeup);
    let req = test::TestRequest::post()
        .uri("/api/v1/batch/archive-compress")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [],
            "folder_ids": [bundle_id],
            "archive_name": "bundle-export"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let task_id = body["data"]["id"].as_i64().unwrap();
    assert_eq!(body["data"]["max_attempts"], 5);
    tokio::time::timeout(std::time::Duration::from_secs(1), &mut archive_wakeup)
        .await
        .expect("archive task creation should wake the dispatcher");
    assert_task_steps(
        &body,
        &[
            ("waiting", "active"),
            ("prepare_sources", "pending"),
            ("build_archive", "pending"),
            ("store_result", "pending"),
        ],
    );

    let stats = aster_drive::services::task_service::drain(state.get_ref())
        .await
        .expect("task drain should succeed");
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
    assert_task_steps(
        &body,
        &[
            ("waiting", "succeeded"),
            ("prepare_sources", "succeeded"),
            ("build_archive", "succeeded"),
            ("store_result", "succeeded"),
        ],
    );

    let result = read_task_result(&body);
    assert_eq!(result["kind"], "archive_compress");
    let archive_file_id = result["target_file_id"].as_i64().unwrap();
    assert_eq!(result["target_folder_id"], Value::Null);
    assert_eq!(result["target_path"], "/bundle-export.zip");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{archive_file_id}/download"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = assert_response_status(
        test::call_service(&app, req).await,
        actix_web::http::StatusCode::OK,
    )
    .await;
    let zip_bytes = test::read_body(resp).await;
    assert_eq!(
        zip_entry_names(&zip_bytes),
        vec![
            "bundle/",
            "bundle/docs/",
            "bundle/docs/note.txt",
            "bundle/empty/",
        ]
    );
    assert_eq!(
        read_zip_entry_text(&zip_bytes, "bundle/docs/note.txt"),
        "hello from archive compress task"
    );
}

#[actix_web::test]
async fn test_archive_compress_task_keeps_long_conflict_copy_name_within_limit() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let long_folder_name = "a".repeat(255);
    let initial_archive_name = format!("{}.zip", "a".repeat(251));
    let expected_copy_name = format!("{} (1).zip", "a".repeat(247));

    let req = test::TestRequest::post()
        .uri("/api/v1/files/new")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": initial_archive_name, "folder_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": long_folder_name, "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/batch/archive-compress")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [],
            "folder_ids": [folder_id]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let task_id = body["data"]["id"].as_i64().unwrap();

    let stats = aster_drive::services::task_service::drain(&state)
        .await
        .expect("task drain should succeed");
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

    let result = read_task_result(&body);
    assert_eq!(result["target_file_name"], expected_copy_name);
    assert_eq!(
        result["target_file_name"].as_str().unwrap().len(),
        255,
        "copy archive name should stay within common filesystem component limits"
    );
}

#[actix_web::test]
async fn test_archive_compress_task_rejects_expanded_selection_too_large() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    aster_drive::services::config_service::set(&state, "background_task_max_attempts", "1", 1)
        .await
        .expect("background task max attempts config should update");
    aster_drive::services::config_service::set(&state, "archive_build_max_entries", "2", 1)
        .await
        .expect("archive build max entries config should update");
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "many", "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    for index in 0..2 {
        let req = multipart_request!(
            &format!("/api/v1/files/upload?folder_id={folder_id}"),
            &token,
            &format!("file-{index}.txt"),
            "payload",
        );
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    let req = test::TestRequest::post()
        .uri("/api/v1/batch/archive-compress")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [],
            "folder_ids": [folder_id],
            "archive_name": "too-many"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let task_id = body["data"]["id"].as_i64().unwrap();

    let stats = aster_drive::services::task_service::drain(&state)
        .await
        .expect("task drain should succeed");
    assert_eq!(stats.failed, 1);
    assert_eq!(stats.succeeded, 0);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/tasks/{task_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(
        body["data"]["last_error"]
            .as_str()
            .expect("failed task should record last error")
            .contains("expands to 3 entries"),
        "expanded selection entry limit should be surfaced"
    );
}

#[actix_web::test]
async fn test_archive_compress_task_rejects_quota_before_building_archive() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    aster_drive::services::config_service::set(&state, "background_task_max_attempts", "1", 1)
        .await
        .expect("background task max attempts config should update");
    let (token, _) = register_and_login!(app);

    let req = multipart_request!("/api/v1/files/upload", &token, "source.txt", "payload");
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let file_id = body["data"]["id"].as_i64().unwrap();

    let owner = aster_drive::db::repository::user_repo::find_by_username(&state.db, "testuser")
        .await
        .expect("owner lookup should succeed")
        .expect("owner should exist");
    let mut owner_active = owner.into_active_model();
    owner_active.storage_quota = Set(owner_active.storage_used.clone().unwrap());
    owner_active
        .update(&state.db)
        .await
        .expect("owner quota should update");

    let req = test::TestRequest::post()
        .uri("/api/v1/batch/archive-compress")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [file_id],
            "folder_ids": [],
            "archive_name": "quota-fail"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let task_id = body["data"]["id"].as_i64().unwrap();

    let stats = aster_drive::services::task_service::drain(&state)
        .await
        .expect("task drain should succeed");
    assert_eq!(stats.failed, 1);
    assert_eq!(stats.succeeded, 0);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/tasks/{task_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "failed");
    assert_eq!(body["data"]["can_retry"], true);
    assert_task_steps(
        &body,
        &[
            ("waiting", "succeeded"),
            ("prepare_sources", "failed"),
            ("build_archive", "pending"),
            ("store_result", "pending"),
        ],
    );
    assert!(
        body["data"]["last_error"]
            .as_str()
            .expect("failed task should record last error")
            .contains("quota"),
        "quota fast-fail should be surfaced"
    );

    let task_temp_dir =
        aster_drive::utils::paths::task_temp_dir(&state.config.server.temp_dir, task_id);
    assert!(
        !std::path::Path::new(&task_temp_dir).exists(),
        "failed archive compress task should not leave temp dir"
    );
}

#[actix_web::test]
async fn test_archive_download_rejects_expanded_selection_source_too_large() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    aster_drive::services::config_service::set(
        &state,
        "archive_build_max_total_source_bytes",
        "4",
        1,
    )
    .await
    .expect("archive build source size config should update");
    let (token, _) = register_and_login!(app);

    let req = multipart_request!("/api/v1/files/upload", &token, "large.txt", "12345");
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let file_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/batch/archive-download")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [file_id],
            "folder_ids": [],
            "archive_name": "source-limit"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert!(
        body["msg"]
            .as_str()
            .expect("error response should have msg")
            .contains("source size"),
        "source-size limit should be surfaced"
    );
}

#[actix_web::test]
async fn test_archive_download_rejects_estimated_output_too_large() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    aster_drive::services::config_service::set(&state, "archive_build_max_temp_bytes", "100", 1)
        .await
        .expect("archive build temp size config should update");
    let (token, _) = register_and_login!(app);

    let req = multipart_request!("/api/v1/files/upload", &token, "tiny.txt", "1");
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let file_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/batch/archive-download")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [file_id],
            "folder_ids": [],
            "archive_name": "temp-limit"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert!(
        body["msg"]
            .as_str()
            .expect("error response should have msg")
            .contains("estimated output size"),
        "estimated output-size limit should be surfaced"
    );
}

#[actix_web::test]
async fn test_retry_task_reloads_max_attempts_from_runtime_config() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let owner = aster_drive::db::repository::user_repo::find_by_username(&state.db, "testuser")
        .await
        .expect("owner lookup should succeed")
        .expect("owner should exist");

    aster_drive::services::config_service::set(&state, "background_task_max_attempts", "4", 1)
        .await
        .expect("background task max attempts config should update");

    let now = Utc::now();
    let payload_json = serde_json::to_string(
        &aster_drive::services::task_service::ArchiveCompressTaskPayload {
            file_ids: Vec::new(),
            folder_ids: Vec::new(),
            archive_name: "retry-bundle.zip".to_string(),
            target_folder_id: None,
        },
    )
    .expect("archive payload should serialize");
    let task = background_task_repo::create(
        &state.db,
        background_task::ActiveModel {
            kind: Set(BackgroundTaskKind::ArchiveCompress),
            status: Set(BackgroundTaskStatus::Failed),
            creator_user_id: Set(Some(owner.id)),
            team_id: Set(None),
            share_id: Set(None),
            display_name: Set("retry-bundle.zip".to_string()),
            payload_json: Set(StoredTaskPayload(payload_json)),
            result_json: Set(None),
            steps_json: Set(None),
            progress_current: Set(0),
            progress_total: Set(0),
            status_text: Set(None),
            attempt_count: Set(1),
            max_attempts: Set(1),
            next_run_at: Set(now - Duration::seconds(1)),
            processing_token: Set(0),
            processing_started_at: Set(None),
            last_heartbeat_at: Set(None),
            lease_expires_at: Set(None),
            started_at: Set(Some(now - Duration::minutes(2))),
            finished_at: Set(Some(now - Duration::minutes(1))),
            last_error: Set(Some("transient failure".to_string())),
            failure_can_retry: Set(Some(true)),
            expires_at: Set(now + Duration::hours(1)),
            created_at: Set(now - Duration::minutes(2)),
            updated_at: Set(now - Duration::minutes(1)),
            ..Default::default()
        },
    )
    .await
    .expect("failed task should be inserted");

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/tasks/{}/retry", task.id))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let retry_wakeup = state.background_task_dispatch_wakeup.notified();
    tokio::pin!(retry_wakeup);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "pending");
    assert_eq!(body["data"]["attempt_count"], 0);
    assert_eq!(body["data"]["max_attempts"], 4);
    tokio::time::timeout(std::time::Duration::from_secs(1), &mut retry_wakeup)
        .await
        .expect("manual task retry should wake the dispatcher");

    let stored = background_task_repo::find_by_id(&state.db, task.id)
        .await
        .expect("retried task should still exist");
    assert_eq!(stored.status, BackgroundTaskStatus::Pending);
    assert_eq!(stored.attempt_count, 0);
    assert_eq!(stored.max_attempts, 4);
}

#[actix_web::test]
async fn test_team_archive_extract_task_creates_team_folder_tree() {
    let state = common::setup().await;
    let db = state.db.clone();
    let mail_sender = state.mail_sender.clone();
    let state = web::Data::new(state);
    let app = test::init_service(
        App::new()
            .app_data(web::PayloadConfig::new(10 * 1024 * 1024))
            .app_data(web::JsonConfig::default().limit(1024 * 1024))
            .app_data(web::Data::clone(&state))
            .configure(move |cfg| aster_drive::api::configure_primary(cfg, &db)),
    )
    .await;

    register_user!(
        app,
        state.db.clone(),
        mail_sender,
        "extractor",
        "extractor@example.com",
        "password123"
    );
    let token = login_user!(app, "extractor", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Archive Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/files/new"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "bundle.zip", "folder_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let archive_file_id = body["data"]["id"].as_i64().unwrap();

    let archive_bytes = create_zip_bytes(&[
        ("docs/", None),
        ("docs/note.txt", Some("team extract payload".as_bytes())),
        ("empty/", None),
    ]);
    let req = test::TestRequest::put()
        .uri(&format!(
            "/api/v1/teams/{team_id}/files/{archive_file_id}/content"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(archive_bytes)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/teams/{team_id}/files/{archive_file_id}/extract"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let task_id = body["data"]["id"].as_i64().unwrap();
    assert_task_steps(
        &body,
        &[
            ("waiting", "active"),
            ("download_source", "pending"),
            ("extract_archive", "pending"),
            ("import_result", "pending"),
        ],
    );

    let stats = aster_drive::services::task_service::drain(state.get_ref())
        .await
        .expect("task drain should succeed");
    assert_eq!(stats.succeeded, 1);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/tasks/{task_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "succeeded");
    assert_task_steps(
        &body,
        &[
            ("waiting", "succeeded"),
            ("download_source", "succeeded"),
            ("extract_archive", "succeeded"),
            ("import_result", "succeeded"),
        ],
    );

    let result = read_task_result(&body);
    assert_eq!(result["kind"], "archive_extract");
    let extracted_root_id = result["target_folder_id"].as_i64().unwrap();
    assert_eq!(result["target_folder_name"], "bundle");
    assert_eq!(result["target_path"], "/bundle");

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/teams/{team_id}/folders/{extracted_root_id}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let folders = body["data"]["folders"].as_array().unwrap();
    assert_eq!(folders.len(), 2);

    let docs_folder = folders
        .iter()
        .find(|folder| folder["name"] == "docs")
        .expect("docs folder should exist");
    let docs_folder_id = docs_folder["id"].as_i64().unwrap();

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/folders/{docs_folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let files = body["data"]["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["name"], "note.txt");
    let note_file_id = files[0]["id"].as_i64().unwrap();

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/teams/{team_id}/files/{note_file_id}/download"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = assert_response_status(
        test::call_service(&app, req).await,
        actix_web::http::StatusCode::OK,
    )
    .await;
    let file_bytes = test::read_body(resp).await;
    assert_eq!(String::from_utf8_lossy(&file_bytes), "team extract payload");
}

#[actix_web::test]
async fn test_archive_extract_task_publishes_single_storage_change_event() {
    let state = common::setup().await;
    let mut storage_events = state.storage_change_tx.subscribe();
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/files/new")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "bundle.zip", "folder_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let archive_file_id = body["data"]["id"].as_i64().unwrap();

    let archive_bytes = create_zip_bytes(&[
        ("docs/", None),
        ("docs/one.txt", Some("one".as_bytes())),
        ("docs/two.txt", Some("two".as_bytes())),
        ("three.txt", Some("three".as_bytes())),
    ]);
    let archive_size = i64::try_from(archive_bytes.len()).unwrap();
    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/{archive_file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(archive_bytes)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    drain_storage_change_events(&mut storage_events).await;

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{archive_file_id}/extract"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let stats = aster_drive::services::task_service::drain(&state)
        .await
        .expect("task drain should succeed");
    assert_eq!(stats.succeeded, 1);

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), storage_events.recv())
        .await
        .expect("archive extract should publish one storage event")
        .expect("storage change channel should stay open");
    assert_eq!(
        event.kind,
        aster_drive::services::storage_change_service::StorageChangeKind::FolderCreated
    );
    assert_eq!(event.file_ids.len(), 3);
    assert_eq!(event.folder_ids.len(), 2);
    assert_eq!(event.storage_delta, Some(11));
    assert!(event.affects_quota);

    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), storage_events.recv())
            .await
            .is_err(),
        "archive extract should coalesce per-file storage events"
    );

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me?fields=quota")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["storage_used"], archive_size + 11);
}

#[actix_web::test]
async fn test_archive_extract_empty_directories_publish_non_quota_storage_change_event() {
    let state = common::setup().await;
    let mut storage_events = state.storage_change_tx.subscribe();
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/files/new")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "empty-dirs.zip", "folder_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let archive_file_id = body["data"]["id"].as_i64().unwrap();

    let archive_bytes = create_zip_bytes(&[("alpha/", None), ("alpha/beta/", None)]);
    let archive_size = i64::try_from(archive_bytes.len()).unwrap();
    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/{archive_file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(archive_bytes)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    drain_storage_change_events(&mut storage_events).await;

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{archive_file_id}/extract"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let stats = aster_drive::services::task_service::drain(&state)
        .await
        .expect("task drain should succeed");
    assert_eq!(stats.succeeded, 1);

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), storage_events.recv())
        .await
        .expect("archive extract should publish one storage event")
        .expect("storage change channel should stay open");
    assert_eq!(
        event.kind,
        aster_drive::services::storage_change_service::StorageChangeKind::FolderCreated
    );
    assert!(event.file_ids.is_empty());
    assert_eq!(event.folder_ids.len(), 3);
    assert_eq!(event.storage_delta, Some(0));
    assert!(!event.affects_quota);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), storage_events.recv())
            .await
            .is_err(),
        "empty directory extract should publish only one storage event"
    );

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me?fields=quota")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["storage_used"], archive_size);
}

#[actix_web::test]
async fn test_archive_extract_task_keeps_long_conflict_folder_copy_name_within_limit() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let long_folder_name = "a".repeat(255);
    let expected_copy_name = format!("{} (1)", "a".repeat(251));

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": long_folder_name, "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/files/new")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "long-output.zip", "folder_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let archive_file_id = body["data"]["id"].as_i64().unwrap();

    let archive_bytes = create_zip_bytes(&[("payload.txt", Some(b"payload".as_slice()))]);
    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/{archive_file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(archive_bytes)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{archive_file_id}/extract"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target_folder_id": null,
            "output_folder_name": long_folder_name
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let task_id = body["data"]["id"].as_i64().unwrap();

    let stats = aster_drive::services::task_service::drain(&state)
        .await
        .expect("task drain should succeed");
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

    let result = read_task_result(&body);
    assert_eq!(result["target_folder_name"], expected_copy_name);
    assert_eq!(
        result["target_folder_name"].as_str().unwrap().len(),
        255,
        "copy folder name should stay within common filesystem component limits"
    );
}

#[actix_web::test]
async fn test_archive_extract_concurrent_root_name_conflicts_allocate_copy_names() {
    let state = common::setup().await;
    state.runtime_config.apply(common::system_config_model(
        BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY_KEY,
        "2",
    ));
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let mut task_ids = Vec::new();
    for index in 0..2 {
        let req = test::TestRequest::post()
            .uri("/api/v1/files/new")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({
                "name": format!("concurrent-extract-{index}.zip"),
                "folder_id": null,
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
        let body: Value = test::read_body_json(resp).await;
        let archive_file_id = body["data"]["id"].as_i64().unwrap();

        let payload = format!("payload {index}");
        let archive_bytes = create_zip_bytes(&[("payload.txt", Some(payload.as_bytes()))]);
        let req = test::TestRequest::put()
            .uri(&format!("/api/v1/files/{archive_file_id}/content"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .insert_header(("Content-Type", "application/octet-stream"))
            .set_payload(archive_bytes)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        let req = test::TestRequest::post()
            .uri(&format!("/api/v1/files/{archive_file_id}/extract"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({
                "target_folder_id": null,
                "output_folder_name": "concurrent-root",
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: Value = test::read_body_json(resp).await;
        task_ids.push(body["data"]["id"].as_i64().unwrap());
    }

    let stats = aster_drive::services::task_service::drain(&state)
        .await
        .expect("task drain should succeed");
    assert_eq!(stats.succeeded, 2);
    assert_eq!(stats.failed, 0);

    let mut target_names = Vec::new();
    for task_id in task_ids {
        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/tasks/{task_id}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body["data"]["status"], "succeeded");
        let result = read_task_result(&body);
        target_names.push(
            result["target_folder_name"]
                .as_str()
                .expect("extract task result should include target folder name")
                .to_string(),
        );
    }

    target_names.sort();
    assert_eq!(target_names, vec!["concurrent-root", "concurrent-root (1)"]);
}

#[actix_web::test]
async fn test_archive_extract_task_fails_before_staging_when_quota_is_insufficient() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    aster_drive::services::config_service::set(&state, "background_task_max_attempts", "1", 1)
        .await
        .expect("background task max attempts config should update");

    let (token, _) = register_and_login!(app);
    let archive_bytes = create_zip_bytes(&[(
        "payload.txt",
        Some("quota preflight should fail".as_bytes()),
    )]);

    let req = test::TestRequest::post()
        .uri("/api/v1/files/new")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "quota-check.zip", "folder_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let archive_file_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/{archive_file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(archive_bytes)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let owner = aster_drive::db::repository::user_repo::find_by_username(&state.db, "testuser")
        .await
        .expect("owner lookup should succeed")
        .expect("owner should exist");
    let quota_base = owner.storage_used;
    let mut owner_active = owner.into_active_model();
    owner_active.storage_quota = Set(quota_base
        .checked_add(8)
        .expect("quota adjustment should stay within i64"));
    owner_active
        .update(&state.db)
        .await
        .expect("owner quota should update");

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{archive_file_id}/extract"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let task_id = body["data"]["id"].as_i64().unwrap();

    let stats = aster_drive::services::task_service::drain(&state)
        .await
        .expect("task drain should succeed");
    assert_eq!(stats.failed, 1);
    assert_eq!(stats.succeeded, 0);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/tasks/{task_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "failed");
    assert_eq!(body["data"]["can_retry"], true);
    assert_task_steps(
        &body,
        &[
            ("waiting", "succeeded"),
            ("download_source", "succeeded"),
            ("extract_archive", "failed"),
            ("import_result", "pending"),
        ],
    );
    assert!(
        body["data"]["last_error"]
            .as_str()
            .expect("failed task should record last error")
            .contains("quota"),
        "quota preflight error should be surfaced"
    );

    let task_temp_dir =
        aster_drive::utils::paths::task_temp_dir(&state.config.server.temp_dir, task_id);
    assert!(
        !std::path::Path::new(&task_temp_dir).exists(),
        "failed extract task should cleanup temp dir"
    );
}

#[actix_web::test]
async fn test_archive_extract_task_fails_when_staging_limit_is_exceeded() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    aster_drive::services::config_service::set(&state, "background_task_max_attempts", "1", 1)
        .await
        .expect("background task max attempts config should update");

    let (token, _) = register_and_login!(app);
    let payload = vec![b'a'; 256];
    let archive_bytes = create_zip_bytes(&[("payload.txt", Some(&payload))]);
    let staging_limit = i64::try_from(archive_bytes.len())
        .expect("archive size should fit in i64")
        .checked_add(32)
        .expect("staging limit should fit in i64");
    aster_drive::services::config_service::set(
        &state,
        "archive_extract_max_staging_bytes",
        &staging_limit.to_string(),
        1,
    )
    .await
    .expect("archive extract staging limit config should update");

    let req = test::TestRequest::post()
        .uri("/api/v1/files/new")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "staging-check.zip", "folder_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let archive_file_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/{archive_file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(archive_bytes)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{archive_file_id}/extract"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let task_id = body["data"]["id"].as_i64().unwrap();

    let stats = aster_drive::services::task_service::drain(&state)
        .await
        .expect("task drain should succeed");
    assert_eq!(stats.failed, 1);
    assert_eq!(stats.succeeded, 0);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/tasks/{task_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "failed");
    assert_task_steps(
        &body,
        &[
            ("waiting", "succeeded"),
            ("download_source", "succeeded"),
            ("extract_archive", "failed"),
            ("import_result", "pending"),
        ],
    );
    assert!(
        body["data"]["last_error"]
            .as_str()
            .expect("failed task should record last error")
            .contains("staging requires"),
        "staging cap error should be surfaced"
    );

    let task_temp_dir =
        aster_drive::utils::paths::task_temp_dir(&state.config.server.temp_dir, task_id);
    assert!(
        !std::path::Path::new(&task_temp_dir).exists(),
        "failed extract task should cleanup temp dir"
    );
}

#[actix_web::test]
async fn test_archive_extract_task_rejects_entry_size_tampering() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    aster_drive::services::config_service::set(&state, "background_task_max_attempts", "1", 1)
        .await
        .expect("background task max attempts config should update");

    let (token, _) = register_and_login!(app);
    let payload = vec![b'x'; 64];
    let archive_bytes = create_zip_bytes_with_tampered_declared_size("payload.txt", &payload, 4);

    let req = test::TestRequest::post()
        .uri("/api/v1/files/new")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "tampered.zip", "folder_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let archive_file_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/{archive_file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(archive_bytes)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{archive_file_id}/extract"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let task_id = body["data"]["id"].as_i64().unwrap();

    let stats = aster_drive::services::task_service::drain(&state)
        .await
        .expect("task drain should succeed");
    assert_eq!(stats.failed, 1);
    assert_eq!(stats.succeeded, 0);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/tasks/{task_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "failed");
    assert_task_steps(
        &body,
        &[
            ("waiting", "succeeded"),
            ("download_source", "succeeded"),
            ("extract_archive", "failed"),
            ("import_result", "pending"),
        ],
    );
    assert!(
        body["data"]["last_error"]
            .as_str()
            .expect("failed task should record last error")
            .contains("declared size"),
        "size tampering should be surfaced"
    );

    let task_temp_dir =
        aster_drive::utils::paths::task_temp_dir(&state.config.server.temp_dir, task_id);
    assert!(
        !std::path::Path::new(&task_temp_dir).exists(),
        "failed extract task should cleanup temp dir"
    );
}

#[actix_web::test]
async fn test_archive_extract_task_rejects_too_many_entries() {
    let archive_bytes = create_stored_zip_bytes(&[
        ("one.txt", Some(b"one".as_slice())),
        ("two.txt", Some(b"two".as_slice())),
    ]);
    let body = run_failing_personal_archive_extract(
        archive_bytes,
        vec![("archive_extract_max_entries", "1".to_string())],
    )
    .await;

    assert!(
        body["data"]["last_error"]
            .as_str()
            .expect("failed task should record last error")
            .contains("entries"),
        "entry limit error should be surfaced"
    );
}

#[actix_web::test]
async fn test_archive_extract_task_rejects_too_many_files() {
    let archive_bytes = create_stored_zip_bytes(&[
        ("one.txt", Some(b"one".as_slice())),
        ("two.txt", Some(b"two".as_slice())),
    ]);
    let body = run_failing_personal_archive_extract(
        archive_bytes,
        vec![("archive_extract_max_files", "1".to_string())],
    )
    .await;

    assert!(
        body["data"]["last_error"]
            .as_str()
            .expect("failed task should record last error")
            .contains("files"),
        "file limit error should be surfaced"
    );
}

#[actix_web::test]
async fn test_archive_extract_task_rejects_too_many_directories() {
    let archive_bytes = create_stored_zip_bytes(&[
        ("one/file.txt", Some(b"one".as_slice())),
        ("two/file.txt", Some(b"two".as_slice())),
    ]);
    let body = run_failing_personal_archive_extract(
        archive_bytes,
        vec![("archive_extract_max_directories", "1".to_string())],
    )
    .await;

    assert!(
        body["data"]["last_error"]
            .as_str()
            .expect("failed task should record last error")
            .contains("directories"),
        "directory limit error should be surfaced"
    );
}

#[actix_web::test]
async fn test_archive_extract_task_rejects_duplicate_paths() {
    let archive_bytes = create_stored_zip_bytes(&[
        ("duplicate.txt", Some(b"first".as_slice())),
        ("./duplicate.txt", Some(b"second".as_slice())),
    ]);
    let body = run_failing_personal_archive_extract(archive_bytes, Vec::new()).await;

    assert!(
        body["data"]["last_error"]
            .as_str()
            .expect("failed task should record last error")
            .contains("duplicate entry path"),
        "duplicate path error should be surfaced"
    );
}

#[actix_web::test]
async fn test_archive_extract_task_rejects_unicode_normalized_duplicate_paths() {
    let archive_bytes = create_stored_zip_bytes(&[
        ("caf\u{00e9}.txt", Some(b"nfc".as_slice())),
        ("cafe\u{0301}.txt", Some(b"nfd".as_slice())),
    ]);
    let body = run_failing_personal_archive_extract(archive_bytes, Vec::new()).await;

    assert!(
        body["data"]["last_error"]
            .as_str()
            .expect("failed task should record last error")
            .contains("duplicate entry path"),
        "unicode-normalized duplicate path error should be surfaced"
    );
}

#[actix_web::test]
async fn test_archive_extract_task_rejects_file_directory_conflicts() {
    let archive_bytes = create_stored_zip_bytes(&[
        ("prefix", Some(b"file".as_slice())),
        ("prefix/child/", None),
    ]);
    let body = run_failing_personal_archive_extract(archive_bytes, Vec::new()).await;

    assert!(
        body["data"]["last_error"]
            .as_str()
            .expect("failed task should record last error")
            .contains("inside file entry"),
        "file/directory conflict error should be surfaced"
    );
}

#[actix_web::test]
async fn test_archive_extract_task_rejects_too_deep_paths() {
    let archive_bytes =
        create_stored_zip_bytes(&[("a/b/c/payload.txt", Some(b"nested".as_slice()))]);
    let body = run_failing_personal_archive_extract(
        archive_bytes,
        vec![("archive_extract_max_depth", "3".to_string())],
    )
    .await;

    assert!(
        body["data"]["last_error"]
            .as_str()
            .expect("failed task should record last error")
            .contains("depth"),
        "path depth error should be surfaced"
    );
}

#[actix_web::test]
async fn test_archive_extract_task_rejects_too_long_paths() {
    let archive_bytes = create_stored_zip_bytes(&[(
        "long-name/payload.txt",
        Some(b"path length check".as_slice()),
    )]);
    let body = run_failing_personal_archive_extract(
        archive_bytes,
        vec![("archive_extract_max_path_bytes", "8".to_string())],
    )
    .await;

    assert!(
        body["data"]["last_error"]
            .as_str()
            .expect("failed task should record last error")
            .contains("path length"),
        "path length error should be surfaced"
    );
}

#[actix_web::test]
async fn test_archive_extract_task_rejects_high_total_compression_ratio() {
    let first = vec![b'a'; 4096];
    let second = vec![b'b'; 4096];
    let archive_bytes =
        create_zip_bytes(&[("first.txt", Some(&first)), ("second.txt", Some(&second))]);
    let body = run_failing_personal_archive_extract(
        archive_bytes,
        vec![
            (
                "archive_extract_max_entry_compression_ratio",
                "1000".to_string(),
            ),
            ("archive_extract_max_compression_ratio", "2".to_string()),
        ],
    )
    .await;

    assert!(
        body["data"]["last_error"]
            .as_str()
            .expect("failed task should record last error")
            .contains("total compression ratio"),
        "total compression ratio error should be surfaced"
    );
}

#[actix_web::test]
async fn test_archive_extract_task_rejects_high_entry_compression_ratio() {
    let payload = vec![b'a'; 4096];
    let archive_bytes = create_zip_bytes(&[("payload.txt", Some(&payload))]);
    let body = run_failing_personal_archive_extract(
        archive_bytes,
        vec![(
            "archive_extract_max_entry_compression_ratio",
            "2".to_string(),
        )],
    )
    .await;

    assert!(
        body["data"]["last_error"]
            .as_str()
            .expect("failed task should record last error")
            .contains("compression ratio"),
        "compression ratio error should be surfaced"
    );
}

#[actix_web::test]
async fn test_archive_extract_security_validation_does_not_auto_or_manual_retry() {
    let payload = vec![b'a'; 4096];
    let archive_bytes = create_zip_bytes(&[("payload.txt", Some(&payload))]);
    let body = run_failing_personal_archive_extract_with_options(
        archive_bytes,
        vec![(
            "archive_extract_max_entry_compression_ratio",
            "2".to_string(),
        )],
        "3",
        true,
    )
    .await;

    assert_eq!(body["data"]["max_attempts"], 3);
    assert_eq!(body["data"]["attempt_count"], 1);
    assert_eq!(body["data"]["can_retry"], false);
}

#[actix_web::test]
async fn test_archive_extract_task_rejects_symlink_entries() {
    let archive_bytes = create_symlink_zip_bytes("link", "../target");
    let body = run_failing_personal_archive_extract(archive_bytes, Vec::new()).await;

    assert!(
        body["data"]["last_error"]
            .as_str()
            .expect("failed task should record last error")
            .contains("symbolic link"),
        "symlink error should be surfaced"
    );
}

#[actix_web::test]
async fn test_archive_extract_task_rejects_special_file_entries() {
    let archive_bytes = create_special_file_zip_bytes("device");
    let body = run_failing_personal_archive_extract(archive_bytes, Vec::new()).await;

    assert!(
        body["data"]["last_error"]
            .as_str()
            .expect("failed task should record last error")
            .contains("special file"),
        "special file error should be surfaced"
    );
}

#[actix_web::test]
async fn test_archive_extract_task_rejects_encrypted_entries() {
    let archive_bytes = create_encrypted_flag_zip_bytes("secret.txt", b"secret");
    let body = run_failing_personal_archive_extract(archive_bytes, Vec::new()).await;

    assert!(
        body["data"]["last_error"]
            .as_str()
            .expect("failed task should record last error")
            .contains("encrypted"),
        "encrypted entry error should be surfaced"
    );
}

#[actix_web::test]
async fn test_archive_extract_task_rejects_unsupported_compression_method() {
    let archive_bytes = create_unsupported_method_zip_bytes("payload.txt", b"payload");
    let body = run_failing_personal_archive_extract(archive_bytes, Vec::new()).await;
    let last_error = body["data"]["last_error"]
        .as_str()
        .expect("failed task should record last error");

    assert!(
        last_error.contains("unsupported compression method")
            || last_error.contains("unsupported compression"),
        "unsupported compression method error should be surfaced"
    );
}

#[actix_web::test]
async fn test_archive_extract_task_rejects_source_archive_larger_than_source_limit_before_download()
{
    let archive_bytes = create_stored_zip_bytes(&[("payload.txt", Some(b"payload".as_slice()))]);
    let body = run_failing_personal_archive_extract(
        archive_bytes,
        vec![("archive_extract_max_source_bytes", "1".to_string())],
    )
    .await;

    assert_task_steps(
        &body,
        &[
            ("waiting", "succeeded"),
            ("download_source", "failed"),
            ("extract_archive", "pending"),
            ("import_result", "pending"),
        ],
    );
    assert!(
        body["data"]["last_error"]
            .as_str()
            .expect("failed task should record last error")
            .contains("source archive size"),
        "source archive size error should be surfaced"
    );
}

#[actix_web::test]
async fn test_archive_extract_task_fails_when_downloaded_source_exceeds_declared_size() {
    let archive_bytes = create_stored_zip_bytes(&[("payload.txt", Some(b"payload".as_slice()))]);
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    aster_drive::services::config_service::set(&state, "background_task_max_attempts", "1", 1)
        .await
        .expect("background task max attempts config should update");
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/files/new")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "oversized-source.zip", "folder_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let archive_file_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/{archive_file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(archive_bytes)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let source_file =
        aster_drive::db::repository::file_repo::find_by_id(&state.db, archive_file_id)
            .await
            .expect("archive file should be loaded");
    let mut file_active = source_file.into_active_model();
    file_active.size = Set(1);
    file_active
        .update(&state.db)
        .await
        .expect("archive file declared size should update");

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{archive_file_id}/extract"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let task_id = body["data"]["id"].as_i64().unwrap();

    let stats = aster_drive::services::task_service::drain(&state)
        .await
        .expect("task drain should succeed");
    assert_eq!(stats.failed, 1);
    assert_eq!(stats.succeeded, 0);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/tasks/{task_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "failed");
    assert_task_steps(
        &body,
        &[
            ("waiting", "succeeded"),
            ("download_source", "failed"),
            ("extract_archive", "pending"),
            ("import_result", "pending"),
        ],
    );
    assert!(
        body["data"]["last_error"]
            .as_str()
            .expect("failed task should record last error")
            .contains("source archive expands beyond declared size"),
        "downloaded source size error should be surfaced"
    );
}

#[actix_web::test]
async fn test_archive_extract_task_cleans_created_root_after_import_failure() {
    let archive_bytes = create_stored_zip_bytes(&[
        ("first.txt", Some(b"first".as_slice())),
        ("second.txt", Some(b"second".as_slice())),
    ]);
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    aster_drive::services::config_service::set(&state, "background_task_max_attempts", "1", 1)
        .await
        .expect("background task max attempts config should update");
    let (token, _) = register_and_login!(app);
    let owner = aster_drive::db::repository::user_repo::find_by_username(&state.db, "testuser")
        .await
        .expect("owner lookup should succeed")
        .expect("owner should exist");

    let req = test::TestRequest::post()
        .uri("/api/v1/files/new")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "cleanup-check.zip", "folder_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let archive_file_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/{archive_file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(archive_bytes)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{archive_file_id}/extract"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "output_folder_name": "cleanup-root" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let task_id = body["data"]["id"].as_i64().unwrap();

    let storage_root = std::path::Path::new(&state.config.server.upload_temp_dir)
        .parent()
        .expect("test storage root should have a parent")
        .to_path_buf();
    let writable_subtrees = [
        std::path::Path::new(&state.config.server.temp_dir),
        std::path::Path::new(&state.config.server.upload_temp_dir),
    ];
    set_storage_data_directories_writable(&storage_root, false, &writable_subtrees);
    let stats = aster_drive::services::task_service::drain(&state).await;
    set_storage_data_directories_writable(&storage_root, true, &writable_subtrees);
    let stats = stats.expect("task drain should succeed");
    assert_eq!(stats.failed, 1);
    assert_eq!(stats.succeeded, 0);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/tasks/{task_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "failed");

    let cleanup_root = aster_drive::db::repository::folder_repo::find_by_name_in_parent(
        &state.db,
        owner.id,
        None,
        "cleanup-root",
    )
    .await
    .expect("folder lookup should succeed");
    assert!(
        cleanup_root.is_none(),
        "partially imported extract root should be purged after import failure"
    );
}

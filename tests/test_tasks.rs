//! Background task integration tests

#[macro_use]
mod common;

use actix_web::{App, test, web};
use async_trait::async_trait;
use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, PaginatorTrait, QueryFilter, Set,
};
use serde_json::Value;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::io::{AsyncRead, empty};
use tokio::sync::broadcast;

use aster_drive::api::api_error_code::ApiErrorCode;
use aster_drive::config::operations::{
    BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY_KEY, BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY_KEY,
    OFFLINE_DOWNLOAD_ARIA2_MAX_CONNECTION_PER_SERVER_KEY,
    OFFLINE_DOWNLOAD_ARIA2_REQUEST_TIMEOUT_SECS_KEY, OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY,
    OFFLINE_DOWNLOAD_ARIA2_RPC_URL_KEY, OFFLINE_DOWNLOAD_ARIA2_SPLIT_KEY,
    OFFLINE_DOWNLOAD_ENGINE_REGISTRY_JSON_KEY, OFFLINE_DOWNLOAD_MAX_FILE_SIZE_BYTES_KEY,
    OFFLINE_DOWNLOAD_MAX_MB_PER_SEC_KEY, OFFLINE_DOWNLOAD_REQUEST_TIMEOUT_SECS_KEY,
    OFFLINE_DOWNLOAD_TEMP_DIR_KEY,
};
use aster_drive::db::repository::{background_task_repo, file_repo, policy_repo};
use aster_drive::entities::{background_task, file_blob, storage_policy};
use aster_drive::runtime::SharedRuntimeState;
use aster_drive::services::task::{
    self, RuntimeTaskRunOutcome, SystemRuntimeTaskKind,
    types::{
        RuntimeSystemHealthComponent, RuntimeSystemHealthResult, RuntimeSystemHealthStatus,
        RuntimeTaskName, RuntimeTaskPayload,
    },
};
use aster_drive::storage::{BlobMetadata, StorageDriver};
use aster_drive::types::{BackgroundTaskKind, BackgroundTaskStatus, StoredTaskPayload};

const OLD_BACKGROUND_TASK_DISPLAY_NAME_LIMIT: usize = 255;
const EXPANDED_BACKGROUND_TASK_DISPLAY_NAME_LIMIT: usize = 512;
const ARIA2_TEST_IMAGE_TAG: &str = "latest";

struct Aria2TestContext {
    _container: testcontainers::ContainerAsync<testcontainers::GenericImage>,
    rpc_url: String,
    rpc_secret: String,
}

#[derive(Default)]
struct MetadataCountingDriver {
    metadata_calls: AtomicUsize,
}

impl MetadataCountingDriver {
    fn metadata_calls(&self) -> usize {
        self.metadata_calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl StorageDriver for MetadataCountingDriver {
    async fn put(&self, path: &str, _data: &[u8]) -> aster_drive::errors::Result<String> {
        Ok(path.to_string())
    }

    async fn get(&self, _path: &str) -> aster_drive::errors::Result<Vec<u8>> {
        Ok(Vec::new())
    }

    async fn get_stream(
        &self,
        _path: &str,
    ) -> aster_drive::errors::Result<Box<dyn AsyncRead + Unpin + Send>> {
        Ok(Box::new(empty()))
    }

    async fn delete(&self, _path: &str) -> aster_drive::errors::Result<()> {
        Ok(())
    }

    async fn exists(&self, _path: &str) -> aster_drive::errors::Result<bool> {
        Ok(true)
    }

    async fn metadata(&self, _path: &str) -> aster_drive::errors::Result<BlobMetadata> {
        self.metadata_calls.fetch_add(1, Ordering::SeqCst);
        Ok(BlobMetadata {
            size: 11,
            content_type: Some("application/octet-stream".to_string()),
        })
    }
}

macro_rules! register_user {
    ($app:expr, $db:expr, $mail_sender:expr, $username:expr, $email:expr, $password:expr) => {{
        common::create_test_account_via_api(
            &$app,
            &$db,
            &$mail_sender,
            $username,
            $email,
            $password,
        )
        .await;
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

fn zip_central_entry_flags(bytes: &[u8]) -> Vec<(String, u16)> {
    let central_signature = [0x50, 0x4B, 0x01, 0x02];
    let mut entries = Vec::new();
    let mut index = 0;

    while index + 46 <= bytes.len() {
        if !bytes[index..].starts_with(&central_signature) {
            index += 1;
            continue;
        }

        let flags = u16::from_le_bytes([bytes[index + 8], bytes[index + 9]]);
        let name_len = u16::from_le_bytes([bytes[index + 28], bytes[index + 29]]) as usize;
        let extra_len = u16::from_le_bytes([bytes[index + 30], bytes[index + 31]]) as usize;
        let comment_len = u16::from_le_bytes([bytes[index + 32], bytes[index + 33]]) as usize;
        let name_start = index + 46;
        let name_end = name_start + name_len;
        if name_end > bytes.len() {
            break;
        }

        entries.push((
            String::from_utf8_lossy(&bytes[name_start..name_end]).to_string(),
            flags,
        ));
        index = name_end + extra_len + comment_len;
    }

    entries
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

fn assert_expanded_task_display_name(body: &Value, expected: &str, context: &str) {
    let display_name = body["data"]["display_name"]
        .as_str()
        .expect("task response should include display_name");

    assert_eq!(display_name, expected);
    assert!(
        display_name.len() > OLD_BACKGROUND_TASK_DISPLAY_NAME_LIMIT,
        "{context} task display name should exceed the old {OLD_BACKGROUND_TASK_DISPLAY_NAME_LIMIT}-byte column limit"
    );
    assert!(
        display_name.len() <= EXPANDED_BACKGROUND_TASK_DISPLAY_NAME_LIMIT,
        "{context} task display name should fit the expanded {EXPANDED_BACKGROUND_TASK_DISPLAY_NAME_LIMIT}-byte column limit"
    );
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

async fn start_aria2_context(temp_dir: &str) -> Aria2TestContext {
    use testcontainers::{
        GenericImage, ImageExt,
        core::{IntoContainerPort, Mount},
        runners::AsyncRunner,
    };

    let host_temp_dir = std::fs::canonicalize(temp_dir).unwrap_or_else(|_| temp_dir.into());
    let rpc_secret = format!("asterdrive-{}", uuid::Uuid::new_v4().simple());
    let container = GenericImage::new("p3terx/aria2-pro", ARIA2_TEST_IMAGE_TAG)
        .with_exposed_port(IntoContainerPort::tcp(6800))
        .with_env_var("PUID", "0")
        .with_env_var("PGID", "0")
        .with_env_var("RPC_SECRET", rpc_secret.as_str())
        .with_mount(Mount::bind_mount(
            host_temp_dir.to_string_lossy().to_string(),
            temp_dir.to_string(),
        ))
        .start()
        .await
        .expect("failed to start aria2 test container");

    let port = container
        .get_host_port_ipv4(IntoContainerPort::tcp(6800))
        .await
        .expect("aria2 RPC port should resolve");
    let rpc_url = format!("http://127.0.0.1:{port}/jsonrpc");
    wait_for_aria2_rpc(&rpc_url, &rpc_secret).await;

    Aria2TestContext {
        _container: container,
        rpc_url,
        rpc_secret,
    }
}

async fn wait_for_aria2_rpc(rpc_url: &str, rpc_secret: &str) {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .expect("aria2 wait client should build");

    for _ in 0..60 {
        let response = client
            .post(rpc_url)
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": "asterdrive-test-wait",
                "method": "aria2.getVersion",
                "params": [format!("token:{rpc_secret}")]
            }))
            .send()
            .await;

        if let Ok(response) = response
            && response.status().is_success()
            && let Ok(body) = response.json::<Value>().await
            && body.get("result").is_some()
        {
            return;
        }

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    panic!("aria2 RPC did not become ready at {rpc_url}");
}

async fn drain_storage_change_events(
    rx: &mut tokio::sync::broadcast::Receiver<
        aster_drive::services::events::storage_change::StorageChangeEvent,
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
    run_failing_personal_archive_extract_with_filename(
        archive_bytes,
        "security-check.zip",
        config_overrides,
        max_attempts,
        assert_retry_rejected,
    )
    .await
}

async fn run_failing_personal_archive_extract_with_filename(
    archive_bytes: Vec<u8>,
    file_name: &str,
    config_overrides: Vec<(&str, String)>,
    max_attempts: &str,
    assert_retry_rejected: bool,
) -> Value {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    aster_drive::services::ops::config::set(
        &state,
        "background_task_max_attempts",
        max_attempts,
        1,
    )
    .await
    .expect("background task max attempts config should update");
    for (key, value) in config_overrides {
        aster_drive::services::ops::config::set(&state, key, value.as_str(), 1)
            .await
            .expect("archive extract limit config should update");
    }

    let (token, _) = register_and_login!(app);
    let req = test::TestRequest::post()
        .uri("/api/v1/files/new")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": file_name, "folder_id": null }))
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

    let stats = aster_drive::services::task::drain(&state)
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
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body["code"], ApiErrorCode::TaskRetryNotAllowed.as_str());
    }

    let task_temp_dir =
        aster_forge_utils::paths::task_temp_dir(&state.config.server.temp_dir, task_id);
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
            details: Vec::new(),
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
        state.writer_db(),
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
                &aster_drive::services::task::types::ArchiveCompressTaskPayload {
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
        state.writer_db(),
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
            runtime_json: Set(None),
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
        state.writer_db(),
        background_task::ActiveModel {
            kind: Set(kind),
            status: Set(BackgroundTaskStatus::Pending),
            creator_user_id: Set(None),
            team_id: Set(None),
            share_id: Set(None),
            display_name: Set(display_name.to_string()),
            payload_json: Set(StoredTaskPayload(payload_json.to_string())),
            result_json: Set(None),
            runtime_json: Set(None),
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
async fn test_offline_download_disabled_registry_rejects_task_creation() {
    let state = common::setup().await;
    aster_drive::services::ops::config::set(
        &state,
        OFFLINE_DOWNLOAD_ENGINE_REGISTRY_JSON_KEY,
        r#"{"version":1,"engines":[{"kind":"builtin","enabled":false},{"kind":"aria2","enabled":false}]}"#,
        1,
    )
    .await
    .expect("offline download registry should update");

    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let req = test::TestRequest::post()
        .uri("/api/v1/tasks/offline-download")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "url": "https://example.com/",
            "filename": "disabled.html"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let resp = assert_response_status(resp, actix_web::http::StatusCode::BAD_REQUEST).await;
    let body: Value = test::read_body_json(resp).await;
    assert!(
        body["msg"]
            .as_str()
            .is_some_and(|message| message.contains("no download engine is enabled")),
        "disabled offline download should return a clear validation error: {body}"
    );
    assert_eq!(
        background_task_repo::count_pending_or_retry(state.writer_db())
            .await
            .expect("background task count should load"),
        0
    );
}

#[actix_web::test]
async fn test_offline_download_aria2_engine_imports_example_com_e2e() {
    let state = common::setup().await;
    let aria2 = start_aria2_context(&state.config.server.temp_dir).await;

    for (key, value) in [
        (
            OFFLINE_DOWNLOAD_ENGINE_REGISTRY_JSON_KEY,
            r#"{"version":1,"engines":[{"kind":"aria2","enabled":true},{"kind":"builtin","enabled":false}]}"#,
        ),
        (OFFLINE_DOWNLOAD_ARIA2_RPC_URL_KEY, aria2.rpc_url.as_str()),
        (
            OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY,
            aria2.rpc_secret.as_str(),
        ),
        (OFFLINE_DOWNLOAD_ARIA2_REQUEST_TIMEOUT_SECS_KEY, "5"),
        (OFFLINE_DOWNLOAD_ARIA2_SPLIT_KEY, "1"),
        (OFFLINE_DOWNLOAD_ARIA2_MAX_CONNECTION_PER_SERVER_KEY, "1"),
        (OFFLINE_DOWNLOAD_MAX_FILE_SIZE_BYTES_KEY, "1048576"),
        (OFFLINE_DOWNLOAD_MAX_MB_PER_SEC_KEY, "0"),
        (OFFLINE_DOWNLOAD_REQUEST_TIMEOUT_SECS_KEY, "30"),
    ] {
        aster_drive::services::ops::config::set(&state, key, value, 1)
            .await
            .expect("offline download aria2 config should update");
    }

    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let req = test::TestRequest::post()
        .uri("/api/v1/tasks/offline-download")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "url": "https://example.com/",
            "filename": "aria2-example.html"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let created: Value = test::read_body_json(resp).await;
    let task_id = created["data"]["id"]
        .as_i64()
        .expect("created task id should exist");

    let stats = task::drain(&state)
        .await
        .expect("aria2 offline download task should drain");
    let task = background_task_repo::find_by_id(state.writer_db(), task_id)
        .await
        .expect("aria2 offline download task should load");
    // This E2E intentionally downloads from the public Internet through a real
    // aria2 container. Local TUN/VPN/proxy DNS setups can rewrite example.com to
    // private, loopback, documentation, or other reserved ranges; AsterDrive's
    // SSRF guard correctly rejects those as "blocked address", so the test may
    // fail in that environment even though the aria2 integration is fine.
    if stats.succeeded == 0
        && stats.failed == 1
        && task
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("blocked address"))
    {
        return;
    }
    assert_eq!(
        stats.succeeded, 1,
        "aria2 E2E should succeed unless the local network maps example.com to a blocked address; last_error={:?}",
        task.last_error
    );
    assert_eq!(
        stats.failed, 0,
        "aria2 E2E failed; last_error={:?}",
        task.last_error
    );
    assert_eq!(stats.retried, 0);
    assert_eq!(task.status, BackgroundTaskStatus::Succeeded);
    assert_eq!(
        task.display_name,
        "Import aria2-example.html from link via aria2"
    );
    let runtime_json = task
        .runtime_json
        .as_ref()
        .expect("aria2 task should persist runtime_json");
    let runtime: Value =
        serde_json::from_str(runtime_json.as_ref()).expect("runtime_json should parse");
    assert!(
        runtime["aria2"]["gid"]
            .as_str()
            .is_some_and(|gid| !gid.is_empty()),
        "aria2 GID should be persisted in runtime_json"
    );
    assert!(
        runtime["aria2"]["processing_token"]
            .as_i64()
            .is_some_and(|token| token > 0),
        "aria2 runtime_json should include the processing token"
    );

    let result: Value = serde_json::from_str(
        task.result_json
            .as_ref()
            .expect("aria2 task should persist result_json")
            .as_ref(),
    )
    .expect("offline download result should parse");
    assert_eq!(result["file_name"], "aria2-example.html");
    assert!(
        result["content_length"]
            .as_i64()
            .is_some_and(|length| length > 0)
    );
    assert!(
        result["sha256"]
            .as_str()
            .is_some_and(|sha256| sha256.len() == 64)
    );
    assert_eq!(result["download_engine"], "aria2");
    let file_id = result["file_id"]
        .as_i64()
        .expect("offline download result should include file_id");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/download"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let downloaded = test::read_body(resp).await;
    let downloaded = String::from_utf8_lossy(&downloaded);
    assert!(downloaded.contains("Example Domain"));

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/tasks/{task_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let task_info: Value = test::read_body_json(resp).await;
    assert_eq!(task_info["data"]["status"], "succeeded");
    assert_eq!(
        task_info["data"]["presentation"]["title"]["code"],
        "task_name_offline_download_source_with_engine"
    );
    assert_eq!(
        task_info["data"]["presentation"]["title"]["params"]["engine"],
        "aria2"
    );
    assert!(
        task_info["data"].get("runtime_json").is_none(),
        "runtime_json is internal state and should not leak through TaskInfo"
    );
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

    let claimable = background_task_repo::list_claimable(state.writer_db(), now, stale_before, 10)
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
        state.writer_db(),
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
        state.writer_db(),
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

    let claimable = background_task_repo::list_claimable(state.writer_db(), now, stale_before, 10)
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
        state.writer_db(),
        task_id,
        0,
        touched_at,
        touched_at + Duration::seconds(60),
    )
    .await
    .expect("heartbeat touch should succeed");
    assert!(touched);

    let task = background_task_repo::find_by_id(state.writer_db(), task_id)
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
        state.writer_db(),
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
        state.writer_db(),
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
        state.writer_db(),
        task.id,
        task.processing_token,
        Utc::now(),
        Utc::now() + Duration::seconds(60),
    )
    .await
    .expect("stale worker heartbeat should be rejected");
    assert!(!stale_touched);

    let stale_progress = background_task_repo::mark_progress(
        state.writer_db(),
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
        state.writer_db(),
        task.id,
        task.processing_token + 1,
        Utc::now(),
        Utc::now() + Duration::seconds(60),
    )
    .await
    .expect("current worker heartbeat should still succeed");
    assert!(fresh_touched);

    let stored = background_task_repo::find_by_id(state.writer_db(), task.id)
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
        state.writer_db(),
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

    let stats = task::dispatch_due(&state)
        .await
        .expect("dispatch should reclaim stale processing task");

    assert_eq!(stats.claimed, 1);
    assert_eq!(stats.retried, 0);
    assert_eq!(stats.failed, 1);
    assert_eq!(stats.succeeded, 0);

    let stored = background_task_repo::find_by_id(state.writer_db(), task.id)
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

    let stats = task::dispatch_due(&state)
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

    let stats = task::dispatch_due(&state)
        .await
        .expect("dispatch should fast-continue archive lane");

    assert_eq!(stats.claimed, 3);
    assert_eq!(stats.failed, 3);
    assert_eq!(stats.retried, 0);
    assert_eq!(stats.succeeded, 0);
}

#[actix_web::test]
async fn test_blob_maintenance_integrity_check_reports_missing_and_size_mismatch() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file_named!(app, token, "blob-integrity.txt");
    let file = file_repo::find_by_id(state.writer_db(), file_id)
        .await
        .expect("uploaded file should load");
    let healthy_blob = file_repo::find_blob_by_id(state.writer_db(), file.blob_id)
        .await
        .expect("uploaded blob should load");
    let now = Utc::now();
    let missing_blob = file_blob::ActiveModel {
        hash: Set("missing-integrity-hash".to_string()),
        size: Set(12),
        policy_id: Set(healthy_blob.policy_id),
        storage_path: Set("admin-maintenance/missing.bin".to_string()),
        thumbnail_path: Set(None),
        thumbnail_processor: Set(None),
        thumbnail_version: Set(None),
        ref_count: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("missing blob should insert");
    let mismatch_blob = file_blob::ActiveModel {
        hash: Set("mismatch-integrity-hash".to_string()),
        size: Set(999),
        policy_id: Set(healthy_blob.policy_id),
        storage_path: Set(healthy_blob.storage_path.clone()),
        thumbnail_path: Set(None),
        thumbnail_processor: Set(None),
        thumbnail_version: Set(None),
        ref_count: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("mismatch blob should insert");

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/file-blobs/maintenance")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action": "integrity_check",
            "blob_ids": [healthy_blob.id, missing_blob.id, mismatch_blob.id]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let task_id = body["data"]["id"].as_i64().unwrap();

    let stats = task::drain(&state)
        .await
        .expect("blob maintenance task should drain");
    assert_eq!(stats.succeeded, 1);
    assert_eq!(stats.failed, 0);

    let task = background_task_repo::find_by_id(state.writer_db(), task_id)
        .await
        .expect("task should load");
    assert_eq!(task.status, BackgroundTaskStatus::Succeeded);
    let result: Value = serde_json::from_str(task.result_json.unwrap().as_ref())
        .expect("blob maintenance result should parse");
    assert_eq!(result["action"], "integrity_check");
    assert_eq!(result["scanned_blobs"], 3);
    assert_eq!(result["checked_objects"], 3);
    assert_eq!(result["missing_objects"], 1);
    assert_eq!(result["size_mismatches"], 1);
    assert_eq!(result["ref_counts_fixed"], 0);
    assert_eq!(result["orphan_blobs_deleted"], 0);
}

#[actix_web::test]
async fn test_blob_maintenance_uses_uncached_driver_without_replacing_existing_handles() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file_named!(app, token, "blob-driver-release.txt");
    let file = file_repo::find_by_id(state.writer_db(), file_id)
        .await
        .expect("uploaded file should load");
    let blob = file_repo::find_blob_by_id(state.writer_db(), file.blob_id)
        .await
        .expect("uploaded blob should load");
    let policy = policy_repo::find_by_id(state.writer_db(), blob.policy_id)
        .await
        .expect("blob policy should load");
    let driver_before = state
        .driver_registry
        .get_driver(&policy)
        .expect("driver should resolve before maintenance");
    assert!(
        driver_before
            .exists(&blob.storage_path)
            .await
            .expect("old driver should read object before maintenance")
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/file-blobs/maintenance")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action": "integrity_check",
            "blob_ids": [blob.id]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let stats = task::drain(&state)
        .await
        .expect("blob maintenance task should drain");
    assert_eq!(stats.succeeded, 1);
    assert_eq!(stats.failed, 0);

    let driver_after = state
        .driver_registry
        .get_driver(&policy)
        .expect("driver should resolve after maintenance");
    assert!(
        std::sync::Arc::ptr_eq(&driver_before, &driver_after),
        "blob maintenance must not evict or replace an existing cached driver"
    );
    assert!(
        driver_before
            .exists(&blob.storage_path)
            .await
            .expect("old Arc handle should remain usable after registry invalidation"),
        "registry invalidation must not break in-flight users that already cloned the driver"
    );
    assert!(
        driver_after
            .exists(&blob.storage_path)
            .await
            .expect("cached driver should read object after maintenance"),
        "cached driver should keep working after maintenance"
    );
}

#[actix_web::test]
async fn test_blob_maintenance_reuses_existing_shared_driver_cache() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file_named!(app, token, "blob-driver-cache-hit.txt");
    let file = file_repo::find_by_id(state.writer_db(), file_id)
        .await
        .expect("uploaded file should load");
    let blob = file_repo::find_blob_by_id(state.writer_db(), file.blob_id)
        .await
        .expect("uploaded blob should load");
    let counting_driver = Arc::new(MetadataCountingDriver::default());
    state
        .driver_registry
        .insert_for_test(blob.policy_id, counting_driver.clone());

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/file-blobs/maintenance")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action": "integrity_check",
            "blob_ids": [blob.id]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let stats = task::drain(&state)
        .await
        .expect("blob maintenance task should drain");
    assert_eq!(stats.succeeded, 1);
    assert_eq!(stats.failed, 0);
    assert_eq!(
        counting_driver.metadata_calls(),
        1,
        "maintenance should reuse an already-cached shared driver Arc"
    );
}

#[actix_web::test]
async fn test_blob_maintenance_keeps_cached_policy_drivers_stable() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file_named!(app, token, "blob-driver-release-one-policy.txt");
    let file = file_repo::find_by_id(state.writer_db(), file_id)
        .await
        .expect("uploaded file should load");
    let blob = file_repo::find_blob_by_id(state.writer_db(), file.blob_id)
        .await
        .expect("uploaded blob should load");
    let touched_policy = policy_repo::find_by_id(state.writer_db(), blob.policy_id)
        .await
        .expect("blob policy should load");
    let untouched_policy_root = std::env::temp_dir().join(format!(
        "asterdrive-untouched-policy-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&untouched_policy_root)
        .expect("untouched policy root should be created");
    let now = Utc::now();
    let untouched_policy = storage_policy::ActiveModel {
        name: Set("Untouched maintenance policy".to_string()),
        driver_type: Set(aster_drive::types::DriverType::Local),
        endpoint: Set(String::new()),
        bucket: Set(String::new()),
        access_key: Set(String::new()),
        secret_key: Set(String::new()),
        base_path: Set(untouched_policy_root.to_string_lossy().into_owned()),
        remote_node_id: Set(None),
        max_file_size: Set(0),
        allowed_types: Set(aster_drive::types::StoredStoragePolicyAllowedTypes::empty()),
        options: Set(aster_drive::types::StoredStoragePolicyOptions::empty()),
        is_default: Set(false),
        chunk_size: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("untouched policy should insert");

    let touched_driver_before = state
        .driver_registry
        .get_driver(&touched_policy)
        .expect("touched driver should resolve before maintenance");
    let untouched_driver_before = state
        .driver_registry
        .get_driver(&untouched_policy)
        .expect("untouched driver should resolve before maintenance");

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/file-blobs/maintenance")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action": "integrity_check",
            "blob_ids": [blob.id]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let stats = task::drain(&state)
        .await
        .expect("blob maintenance task should drain");
    assert_eq!(stats.succeeded, 1);
    assert_eq!(stats.failed, 0);

    let touched_driver_after = state
        .driver_registry
        .get_driver(&touched_policy)
        .expect("touched driver should be recreated after maintenance");
    let untouched_driver_after = state
        .driver_registry
        .get_driver(&untouched_policy)
        .expect("untouched driver should still resolve after maintenance");

    assert!(
        std::sync::Arc::ptr_eq(&touched_driver_before, &touched_driver_after),
        "maintenance should leave existing cached drivers in place"
    );
    assert!(
        std::sync::Arc::ptr_eq(&untouched_driver_before, &untouched_driver_after),
        "maintenance must not evict drivers for policies it did not touch"
    );
}

#[actix_web::test]
async fn test_blob_maintenance_integrity_check_does_not_warm_shared_driver_cache() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file_named!(app, token, "blob-driver-cache-cold.txt");
    let file = file_repo::find_by_id(state.writer_db(), file_id)
        .await
        .expect("uploaded file should load");
    let blob = file_repo::find_blob_by_id(state.writer_db(), file.blob_id)
        .await
        .expect("uploaded blob should load");
    let policy = policy_repo::find_by_id(state.writer_db(), blob.policy_id)
        .await
        .expect("blob policy should load");
    state.driver_registry.invalidate(policy.id);
    assert!(
        !state.driver_registry.has_cached_driver_for_test(policy.id),
        "test must start with a cold shared driver cache"
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/file-blobs/maintenance")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action": "integrity_check",
            "blob_ids": [blob.id]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let stats = task::drain(&state)
        .await
        .expect("blob maintenance task should drain");
    assert_eq!(stats.succeeded, 1);
    assert_eq!(stats.failed, 0);
    assert!(
        !state.driver_registry.has_cached_driver_for_test(policy.id),
        "blob maintenance should use task-local drivers instead of warming the shared driver cache"
    );

    let driver_after = state
        .driver_registry
        .get_driver(&policy)
        .expect("driver should be built on demand after maintenance");
    let driver_again = state
        .driver_registry
        .get_driver(&policy)
        .expect("driver should be cached after explicit registry access");
    assert!(
        std::sync::Arc::ptr_eq(&driver_after, &driver_again),
        "maintenance should not pre-warm the shared cache; explicit access should create the first cached driver"
    );
}

#[actix_web::test]
async fn test_blob_maintenance_orphan_cleanup_does_not_warm_shared_driver_cache() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let policy = policy_repo::find_default(state.writer_db())
        .await
        .expect("default policy query should succeed")
        .expect("default policy should exist");
    let blob_path = format!("admin-maintenance/cache-cold-{}.txt", uuid::Uuid::new_v4());
    let driver = state
        .driver_registry
        .get_driver(&policy)
        .expect("driver should resolve for fixture setup");
    driver
        .put(&blob_path, b"orphan")
        .await
        .expect("orphan object should be written");
    let now = Utc::now();
    let blob = file_blob::ActiveModel {
        hash: Set(format!("orphan-cache-cold-{}", uuid::Uuid::new_v4())),
        size: Set(6),
        policy_id: Set(policy.id),
        storage_path: Set(blob_path),
        ref_count: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("orphan blob should insert");
    state.driver_registry.invalidate(policy.id);
    assert!(
        !state.driver_registry.has_cached_driver_for_test(policy.id),
        "test must start with a cold shared driver cache"
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/file-blobs/maintenance")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action": "orphan_cleanup",
            "blob_ids": [blob.id]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let stats = task::drain(&state)
        .await
        .expect("blob maintenance task should drain");
    assert_eq!(stats.succeeded, 1);
    assert_eq!(stats.failed, 0);
    assert!(
        !state.driver_registry.has_cached_driver_for_test(policy.id),
        "orphan cleanup should delete through task-local drivers, not the shared cache"
    );
    assert!(
        file_repo::find_blob_by_id(state.writer_db(), blob.id)
            .await
            .is_err(),
        "orphan blob row should be deleted"
    );
}

#[actix_web::test]
async fn test_blob_maintenance_empty_scan_keeps_untouched_cached_driver() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let policy = policy_repo::find_default(state.writer_db())
        .await
        .expect("default policy query should succeed")
        .expect("default policy should exist");
    let driver_before = state
        .driver_registry
        .get_driver(&policy)
        .expect("driver should resolve before empty maintenance");

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/file-blobs/maintenance")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action": "ref_count_reconcile"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let stats = task::drain(&state)
        .await
        .expect("empty blob maintenance task should drain");
    assert_eq!(stats.succeeded, 1);
    assert_eq!(stats.failed, 0);

    let driver_after = state
        .driver_registry
        .get_driver(&policy)
        .expect("driver should still resolve after empty maintenance");
    assert!(
        std::sync::Arc::ptr_eq(&driver_before, &driver_after),
        "empty maintenance should not evict storage drivers it did not touch"
    );
}

#[actix_web::test]
async fn test_blob_maintenance_ref_count_reconcile_fixes_current_and_version_refs() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file_named!(app, token, "blob-reconcile.txt");
    let file = file_repo::find_by_id(state.writer_db(), file_id)
        .await
        .expect("uploaded file should load");
    let mut blob = file_repo::find_blob_by_id(state.writer_db(), file.blob_id)
        .await
        .expect("uploaded blob should load")
        .into_active_model();
    blob.ref_count = Set(7);
    let blob = blob
        .update(state.writer_db())
        .await
        .expect("blob ref_count should update");

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/file-blobs/maintenance")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action": "ref_count_reconcile",
            "blob_ids": [blob.id]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let stats = task::drain(&state)
        .await
        .expect("blob maintenance task should drain");
    assert_eq!(stats.succeeded, 1);
    assert_eq!(stats.failed, 0);

    let fixed = file_repo::find_blob_by_id(state.writer_db(), blob.id)
        .await
        .expect("blob should remain");
    assert_eq!(fixed.ref_count, 1);
    let task = background_task::Entity::find()
        .filter(background_task::Column::Kind.eq(BackgroundTaskKind::BlobMaintenance))
        .one(state.writer_db())
        .await
        .expect("task query should succeed")
        .expect("blob maintenance task should exist");
    let result: Value = serde_json::from_str(task.result_json.unwrap().as_ref())
        .expect("blob maintenance result should parse");
    assert_eq!(result["action"], "ref_count_reconcile");
    assert_eq!(result["ref_counts_fixed"], 1);
    assert_eq!(result["orphan_blobs_deleted"], 0);
}

#[actix_web::test]
async fn test_blob_maintenance_ref_count_reconcile_without_targets_scans_all_blobs() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let first_file_id = upload_test_file_named!(app, token, "blob-reconcile-all-a.txt");
    let second_file_id = upload_test_file_named!(app, token, "blob-reconcile-all-b.txt");
    let first_file = file_repo::find_by_id(state.writer_db(), first_file_id)
        .await
        .expect("first uploaded file should load");
    let second_file = file_repo::find_by_id(state.writer_db(), second_file_id)
        .await
        .expect("second uploaded file should load");

    for blob_id in [first_file.blob_id, second_file.blob_id] {
        let mut blob = file_repo::find_blob_by_id(state.writer_db(), blob_id)
            .await
            .expect("uploaded blob should load")
            .into_active_model();
        blob.ref_count = Set(9);
        blob.update(state.writer_db())
            .await
            .expect("blob ref_count should update");
    }

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/file-blobs/maintenance")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action": "ref_count_reconcile"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let stats = task::drain(&state)
        .await
        .expect("blob maintenance task should drain");
    assert_eq!(stats.succeeded, 1);
    assert_eq!(stats.failed, 0);

    for blob_id in [first_file.blob_id, second_file.blob_id] {
        let fixed = file_repo::find_blob_by_id(state.writer_db(), blob_id)
            .await
            .expect("blob should remain");
        assert_eq!(fixed.ref_count, 1);
    }

    let task = background_task::Entity::find()
        .filter(background_task::Column::Kind.eq(BackgroundTaskKind::BlobMaintenance))
        .one(state.writer_db())
        .await
        .expect("task query should succeed")
        .expect("blob maintenance task should exist");
    let result: Value = serde_json::from_str(task.result_json.unwrap().as_ref())
        .expect("blob maintenance result should parse");
    assert_eq!(result["action"], "ref_count_reconcile");
    assert_eq!(result["scanned_blobs"], 2);
    assert_eq!(result["ref_counts_fixed"], 2);
}

#[actix_web::test]
async fn test_blob_maintenance_orphan_cleanup_removes_unreferenced_blob_only() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file_named!(app, token, "blob-orphan-keep.txt");
    let file = file_repo::find_by_id(state.writer_db(), file_id)
        .await
        .expect("uploaded file should load");
    let referenced_blob = file_repo::find_blob_by_id(state.writer_db(), file.blob_id)
        .await
        .expect("uploaded blob should load");
    let policy = state
        .policy_snapshot
        .get_policy_or_err(referenced_blob.policy_id)
        .expect("default policy should load");
    let driver = state
        .driver_registry
        .get_driver(&policy)
        .expect("driver should resolve");
    let orphan_path = "admin-maintenance/orphan-cleanup.txt";
    driver
        .put(orphan_path, b"orphan content")
        .await
        .expect("orphan object should be stored");
    let now = Utc::now();
    let orphan_blob = file_blob::ActiveModel {
        hash: Set("orphan-cleanup-hash".to_string()),
        size: Set(14),
        policy_id: Set(referenced_blob.policy_id),
        storage_path: Set(orphan_path.to_string()),
        thumbnail_path: Set(None),
        thumbnail_processor: Set(None),
        thumbnail_version: Set(None),
        ref_count: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("orphan blob should insert");

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/file-blobs/maintenance")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action": "orphan_cleanup",
            "blob_ids": [orphan_blob.id, referenced_blob.id]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let stats = task::drain(&state)
        .await
        .expect("blob maintenance task should drain");
    assert_eq!(stats.succeeded, 1);
    assert_eq!(stats.failed, 0);

    assert!(
        matches!(
            file_repo::find_blob_by_id(state.writer_db(), orphan_blob.id).await,
            Err(aster_drive::errors::AsterError::RecordNotFound(_))
        ),
        "orphan blob row should be deleted"
    );
    assert!(
        !driver
            .exists(orphan_path)
            .await
            .expect("orphan object existence should be checkable"),
        "orphan object should be deleted"
    );
    assert!(
        file_repo::find_blob_by_id(state.writer_db(), referenced_blob.id)
            .await
            .is_ok(),
        "referenced blob should remain"
    );

    let task = background_task::Entity::find()
        .filter(background_task::Column::Kind.eq(BackgroundTaskKind::BlobMaintenance))
        .one(state.writer_db())
        .await
        .expect("task query should succeed")
        .expect("blob maintenance task should exist");
    let result: Value = serde_json::from_str(task.result_json.unwrap().as_ref())
        .expect("blob maintenance result should parse");
    assert_eq!(result["action"], "orphan_cleanup");
    assert_eq!(result["ref_counts_fixed"], 0);
    assert_eq!(result["orphan_blobs_deleted"], 1);
    assert_eq!(result["skipped_blobs"], 1);
}

#[actix_web::test]
async fn test_cleanup_expired_keeps_terminal_task_records() {
    let state = common::setup().await;
    let now = Utc::now();
    let task = background_task_repo::create(
        state.writer_db(),
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
        aster_forge_utils::paths::task_temp_dir(&state.config.server.temp_dir, task.id);
    std::fs::create_dir_all(&task_temp_dir).expect("task temp dir should be created");
    std::fs::write(format!("{task_temp_dir}/artifact.tmp"), b"expired")
        .expect("task temp artifact should be written");

    let cleaned = task::cleanup_expired(&state)
        .await
        .expect("task cleanup should succeed");
    assert_eq!(cleaned, 1);
    assert!(
        !std::path::Path::new(&task_temp_dir).exists(),
        "expired task temp dir should be removed"
    );

    let stored = background_task_repo::find_by_id(state.writer_db(), task.id)
        .await
        .expect("expired task record should still exist");
    assert_eq!(stored.status, BackgroundTaskStatus::Succeeded);

    let cleaned_again = task::cleanup_expired(&state)
        .await
        .expect("repeated task cleanup should still succeed");
    assert_eq!(cleaned_again, 0);
}

#[actix_web::test]
async fn test_cleanup_expired_scans_offline_download_temp_dir() {
    let state = common::setup().await;
    let custom_temp_root = std::env::temp_dir().join(format!(
        "aster-drive-offline-task-cleanup-{}",
        aster_forge_utils::id::new_uuid()
    ));
    let custom_temp_root = custom_temp_root.to_string_lossy().to_string();
    state.runtime_config.apply(common::system_config_model(
        OFFLINE_DOWNLOAD_TEMP_DIR_KEY,
        &custom_temp_root,
    ));

    let now = Utc::now();
    let task = background_task_repo::create(
        state.writer_db(),
        background_task::ActiveModel {
            kind: Set(BackgroundTaskKind::OfflineDownload),
            status: Set(BackgroundTaskStatus::Failed),
            creator_user_id: Set(None),
            team_id: Set(None),
            share_id: Set(None),
            display_name: Set("expired-offline-download".to_string()),
            payload_json: Set(StoredTaskPayload("{}".to_string())),
            result_json: Set(None),
            steps_json: Set(None),
            progress_current: Set(0),
            progress_total: Set(0),
            status_text: Set(Some("failed".to_string())),
            attempt_count: Set(1),
            max_attempts: Set(1),
            next_run_at: Set(now - Duration::hours(2)),
            processing_started_at: Set(None),
            last_heartbeat_at: Set(None),
            started_at: Set(Some(now - Duration::hours(2))),
            finished_at: Set(Some(now - Duration::hours(2))),
            last_error: Set(Some("failed".to_string())),
            expires_at: Set(now - Duration::hours(1)),
            created_at: Set(now - Duration::hours(2)),
            updated_at: Set(now - Duration::hours(2)),
            ..Default::default()
        },
    )
    .await
    .expect("expired offline download task should be inserted");

    let task_temp_dir = aster_forge_utils::paths::task_temp_dir(&custom_temp_root, task.id);
    std::fs::create_dir_all(&task_temp_dir).expect("offline download temp dir should be created");
    std::fs::write(format!("{task_temp_dir}/source"), b"expired")
        .expect("offline download temp artifact should be written");

    let cleaned = task::cleanup_expired(&state)
        .await
        .expect("task cleanup should succeed");
    assert_eq!(cleaned, 1);
    assert!(
        !std::path::Path::new(&task_temp_dir).exists(),
        "offline download temp dir should be removed from custom root"
    );

    let _ = std::fs::remove_dir_all(&custom_temp_root);
}

#[actix_web::test]
async fn test_record_runtime_task_run_persists_system_runtime_event() {
    let state = common::setup().await;
    aster_drive::services::ops::config::set(&state, "background_task_max_attempts", "5", 1)
        .await
        .expect("background task max attempts config should update");
    let started_at = utc_now_at_db_precision() - Duration::seconds(2);
    let finished_at = utc_now_at_db_precision();

    let recorded = task::record_runtime_task_run(
        &state,
        SystemRuntimeTaskKind::TrashCleanup,
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
    assert_eq!(recorded.progress_current, 1);
    assert_eq!(recorded.progress_total, 1);
    assert!(recorded.steps_json.is_none());
    assert!(recorded.lease_expires_at.is_none());
    assert_eq!(recorded.started_at, Some(started_at));
    assert_eq!(recorded.finished_at, Some(finished_at));
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
async fn test_runtime_task_payload_keeps_known_and_legacy_task_names() {
    let known: RuntimeTaskPayload =
        serde_json::from_value(serde_json::json!({ "task_name": "trash-cleanup" }))
            .expect("known runtime payload should deserialize");
    assert_eq!(
        known.task_name,
        RuntimeTaskName::Known(SystemRuntimeTaskKind::TrashCleanup)
    );
    assert_eq!(known.task_name.display_name(), "Trash cleanup");

    let legacy_name = format!(
        "{}猫{}",
        "runtime-".repeat(64),
        "suffix-that-used-to-be-accepted"
    );
    let legacy: RuntimeTaskPayload =
        serde_json::from_value(serde_json::json!({ "task_name": legacy_name }))
            .expect("legacy runtime payload should deserialize");
    assert_eq!(
        legacy.task_name,
        RuntimeTaskName::Legacy(legacy_name.clone())
    );
    assert!(legacy.task_name.display_name().starts_with("runtime "));
    assert_eq!(
        serde_json::to_value(&legacy).expect("legacy payload should serialize")["task_name"],
        legacy_name
    );
}

#[actix_web::test]
async fn test_record_runtime_task_run_skips_quiet_outcome() {
    let state = common::setup().await;
    let started_at = Utc::now() - Duration::seconds(1);
    let finished_at = Utc::now();

    let recorded = task::record_runtime_task_run(
        &state,
        SystemRuntimeTaskKind::BackgroundTaskDispatch,
        started_at,
        finished_at,
        &RuntimeTaskRunOutcome::quiet(),
    )
    .await
    .expect("quiet runtime task handling should succeed");

    assert!(recorded.is_none());
    let recent = background_task_repo::list_recent(state.writer_db(), 10)
        .await
        .expect("recent background tasks should load");
    assert!(recent.is_empty());
}

#[actix_web::test]
async fn test_record_runtime_task_run_refreshes_latest_healthy_system_check() {
    let state = common::setup().await;
    let first_started_at = utc_now_at_db_precision() - Duration::seconds(6);
    let first_finished_at = utc_now_at_db_precision() - Duration::seconds(5);

    let first = task::record_runtime_task_run(
        &state,
        SystemRuntimeTaskKind::SystemHealthCheck,
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
    let second = task::record_runtime_task_run(
        &state,
        SystemRuntimeTaskKind::SystemHealthCheck,
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

    let recent = background_task_repo::list_recent(state.writer_db(), 10)
        .await
        .expect("recent background tasks should load");
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].id, first.id);
}

#[actix_web::test]
async fn test_record_runtime_task_run_keeps_health_failure_history_before_recovery() {
    let state = common::setup().await;
    let failed = task::record_runtime_task_run(
        &state,
        SystemRuntimeTaskKind::SystemHealthCheck,
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
                    details: Vec::new(),
                }],
            },
        ),
    )
    .await
    .expect("failed system health event should be recorded")
    .expect("failed system health should create a record");

    assert_eq!(failed.progress_current, 0);
    assert_eq!(failed.progress_total, 1);
    assert!(failed.steps_json.is_none());
    assert_eq!(failed.failure_can_retry, Some(false));

    let recovered = task::record_runtime_task_run(
        &state,
        SystemRuntimeTaskKind::SystemHealthCheck,
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

    let recent = background_task_repo::list_recent(state.writer_db(), 10)
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

    let stats = task::dispatch_due(&state)
        .await
        .expect("dispatch should succeed");

    assert_eq!(stats.claimed, 1);
    assert_eq!(stats.retried, 0);
    assert_eq!(stats.failed, 1);
    assert_eq!(stats.succeeded, 0);

    let stored = background_task_repo::find_by_id(state.writer_db(), task.id)
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

    let stats = task::dispatch_due(&state)
        .await
        .expect("dispatch should succeed");

    assert_eq!(stats.claimed, 1);
    assert_eq!(stats.retried, 0);
    assert_eq!(stats.failed, 1);
    assert_eq!(stats.succeeded, 0);

    let stored = background_task_repo::find_by_id(state.writer_db(), task.id)
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
    let db = state.writer_db().clone();
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
        state.writer_db().clone(),
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
    assert_eq!(
        resp.headers()
            .get("Content-Disposition")
            .and_then(|value| value.to_str().ok()),
        Some("attachment; filename*=UTF-8''bundle%2Dexport.zip")
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
async fn test_personal_archive_stream_marks_chinese_names_as_utf8() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
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
        state.writer_db().clone(),
        mail_sender,
        "utf8zip",
        "utf8zip@example.com",
        "password123"
    );
    let token = login_user!(app, "utf8zip", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "资料", "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    let req = multipart_request!(
        &format!("/api/v1/files/upload?folder_id={folder_id}"),
        &token,
        "说明.txt",
        "中文 archive payload",
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/batch/archive-download")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [],
            "folder_ids": [folder_id],
            "archive_name": "中文导出"
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
            .get("Content-Disposition")
            .and_then(|value| value.to_str().ok()),
        Some("attachment; filename*=UTF-8''%E4%B8%AD%E6%96%87%E5%AF%BC%E5%87%BA.zip")
    );
    let zip_bytes = test::read_body(resp).await;
    let names = zip_entry_names(&zip_bytes);
    assert_eq!(names, vec!["资料/", "资料/说明.txt"]);
    assert_eq!(
        read_zip_entry_text(&zip_bytes, "资料/说明.txt"),
        "中文 archive payload"
    );

    let flags = zip_central_entry_flags(&zip_bytes);
    assert_eq!(flags.len(), 2);
    for (name, flag) in flags {
        assert_ne!(
            flag & 0x0800,
            0,
            "ZIP entry {name} should set the UTF-8 filename flag"
        );
    }
}

#[actix_web::test]
async fn test_team_archive_stream_is_scoped_to_team_routes() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
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
        state.writer_db().clone(),
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
    let db = state.writer_db().clone();
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

    aster_drive::services::ops::config::set(
        state.get_ref(),
        "background_task_max_attempts",
        "5",
        1,
    )
    .await
    .expect("background task max attempts config should update");

    register_user!(
        app,
        state.writer_db().clone(),
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

    let stats = aster_drive::services::task::drain(state.get_ref())
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
async fn test_archive_compress_disabled_rejects_personal_task_without_creating_record() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    aster_drive::services::ops::config::set(&state, "archive_compress_enabled", "false", 1)
        .await
        .expect("archive compress enabled config should update");
    let (token, _) = register_and_login!(app);

    let req = multipart_request!("/api/v1/files/upload", &token, "source.txt", "payload");
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let file_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/batch/archive-compress")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [file_id],
            "folder_ids": [],
            "archive_name": "disabled"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "archive_compress.disabled");
    assert_eq!(
        background_task::Entity::find()
            .filter(background_task::Column::Kind.eq(BackgroundTaskKind::ArchiveCompress))
            .count(state.writer_db())
            .await
            .expect("archive compress task count should load"),
        0
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/batch/archive-compress")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [9_999_999],
            "folder_ids": [],
            "archive_name": "disabled-before-lookup"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "archive_compress.disabled");
    assert_eq!(
        background_task::Entity::find()
            .filter(background_task::Column::Kind.eq(BackgroundTaskKind::ArchiveCompress))
            .count(state.writer_db())
            .await
            .expect("archive compress task count should load"),
        0
    );
}

#[actix_web::test]
async fn test_archive_compress_disabled_rejects_team_task_without_creating_record() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
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

    aster_drive::services::ops::config::set(
        state.get_ref(),
        "archive_compress_enabled",
        "false",
        1,
    )
    .await
    .expect("archive compress enabled config should update");

    register_user!(
        app,
        state.writer_db().clone(),
        mail_sender,
        "teamcompressor",
        "teamcompressor@example.com",
        "password123"
    );
    let token = login_user!(app, "teamcompressor", "password123");

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

    let req = multipart_request!(
        &format!("/api/v1/teams/{team_id}/files/upload"),
        &token,
        "team-source.txt",
        "payload",
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let file_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/batch/archive-compress"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [file_id],
            "folder_ids": [],
            "archive_name": "team-disabled"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "archive_compress.disabled");
    assert_eq!(
        background_task::Entity::find()
            .filter(background_task::Column::Kind.eq(BackgroundTaskKind::ArchiveCompress))
            .count(state.writer_db())
            .await
            .expect("archive compress task count should load"),
        0
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
    let expected_display_name = format!("Compress {initial_archive_name}");

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
    assert_expanded_task_display_name(&body, &expected_display_name, "archive compress");
    let task_id = body["data"]["id"].as_i64().unwrap();

    let stats = aster_drive::services::task::drain(&state)
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
    aster_drive::services::ops::config::set(&state, "background_task_max_attempts", "1", 1)
        .await
        .expect("background task max attempts config should update");
    aster_drive::services::ops::config::set(&state, "archive_build_max_entries", "2", 1)
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

    let stats = aster_drive::services::task::drain(&state)
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
    aster_drive::services::ops::config::set(&state, "background_task_max_attempts", "1", 1)
        .await
        .expect("background task max attempts config should update");
    let (token, _) = register_and_login!(app);

    let req = multipart_request!("/api/v1/files/upload", &token, "source.txt", "payload");
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let file_id = body["data"]["id"].as_i64().unwrap();

    let owner =
        aster_drive::db::repository::user_repo::find_by_username(state.writer_db(), "testuser")
            .await
            .expect("owner lookup should succeed")
            .expect("owner should exist");
    let mut owner_active = owner.into_active_model();
    owner_active.storage_quota = Set(owner_active.storage_used.clone().unwrap());
    owner_active
        .update(state.writer_db())
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

    let stats = aster_drive::services::task::drain(&state)
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
        aster_forge_utils::paths::task_temp_dir(&state.config.server.temp_dir, task_id);
    assert!(
        !std::path::Path::new(&task_temp_dir).exists(),
        "failed archive compress task should not leave temp dir"
    );
}

#[actix_web::test]
async fn test_archive_download_rejects_expanded_selection_source_too_large() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    aster_drive::services::ops::config::set(&state, "archive_build_max_total_source_bytes", "4", 1)
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
    aster_drive::services::ops::config::set(&state, "archive_build_max_temp_bytes", "100", 1)
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
    let owner =
        aster_drive::db::repository::user_repo::find_by_username(state.writer_db(), "testuser")
            .await
            .expect("owner lookup should succeed")
            .expect("owner should exist");

    aster_drive::services::ops::config::set(&state, "background_task_max_attempts", "4", 1)
        .await
        .expect("background task max attempts config should update");

    let now = Utc::now();
    let payload_json = serde_json::to_string(
        &aster_drive::services::task::types::ArchiveCompressTaskPayload {
            file_ids: Vec::new(),
            folder_ids: Vec::new(),
            archive_name: "retry-bundle.zip".to_string(),
            target_folder_id: None,
        },
    )
    .expect("archive payload should serialize");
    let task = background_task_repo::create(
        state.writer_db(),
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

    let stored = background_task_repo::find_by_id(state.writer_db(), task.id)
        .await
        .expect("retried task should still exist");
    assert_eq!(stored.status, BackgroundTaskStatus::Pending);
    assert_eq!(stored.attempt_count, 0);
    assert_eq!(stored.max_attempts, 4);
}

#[actix_web::test]
async fn test_retry_task_rejects_non_failed_task_with_stable_code() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let owner =
        aster_drive::db::repository::user_repo::find_by_username(state.writer_db(), "testuser")
            .await
            .expect("owner lookup should succeed")
            .expect("owner should exist");

    let now = Utc::now();
    let payload_json = serde_json::to_string(
        &aster_drive::services::task::types::ArchiveCompressTaskPayload {
            file_ids: Vec::new(),
            folder_ids: Vec::new(),
            archive_name: "pending-task.zip".to_string(),
            target_folder_id: None,
        },
    )
    .expect("archive payload should serialize");
    let task = background_task_repo::create(
        state.writer_db(),
        background_task::ActiveModel {
            kind: Set(BackgroundTaskKind::ArchiveCompress),
            status: Set(BackgroundTaskStatus::Pending),
            creator_user_id: Set(Some(owner.id)),
            team_id: Set(None),
            share_id: Set(None),
            display_name: Set("pending-task.zip".to_string()),
            payload_json: Set(StoredTaskPayload(payload_json)),
            result_json: Set(None),
            steps_json: Set(None),
            progress_current: Set(0),
            progress_total: Set(0),
            status_text: Set(None),
            attempt_count: Set(0),
            max_attempts: Set(1),
            next_run_at: Set(now),
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
    .expect("pending task should be inserted");

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/tasks/{}/retry", task.id))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], ApiErrorCode::TaskRetryStatusConflict.as_str());
}

#[actix_web::test]
async fn test_retry_task_rejects_non_retryable_failure_with_stable_code() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let owner =
        aster_drive::db::repository::user_repo::find_by_username(state.writer_db(), "testuser")
            .await
            .expect("owner lookup should succeed")
            .expect("owner should exist");

    let now = Utc::now();
    let payload_json = serde_json::to_string(
        &aster_drive::services::task::types::ArchiveCompressTaskPayload {
            file_ids: Vec::new(),
            folder_ids: Vec::new(),
            archive_name: "non-retryable-task.zip".to_string(),
            target_folder_id: None,
        },
    )
    .expect("archive payload should serialize");
    let task = background_task_repo::create(
        state.writer_db(),
        background_task::ActiveModel {
            kind: Set(BackgroundTaskKind::ArchiveCompress),
            status: Set(BackgroundTaskStatus::Failed),
            creator_user_id: Set(Some(owner.id)),
            team_id: Set(None),
            share_id: Set(None),
            display_name: Set("non-retryable-task.zip".to_string()),
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
            last_error: Set(Some("validation failure".to_string())),
            failure_can_retry: Set(Some(false)),
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
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], ApiErrorCode::TaskRetryNotAllowed.as_str());
}

#[actix_web::test]
async fn test_team_archive_extract_task_creates_team_folder_tree() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
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
        state.writer_db().clone(),
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

    let stats = aster_drive::services::task::drain(state.get_ref())
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
    let mut storage_events = state.storage_change_bus.subscribe();
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

    let stats = aster_drive::services::task::drain(&state)
        .await
        .expect("task drain should succeed");
    assert_eq!(stats.succeeded, 1);

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), storage_events.recv())
        .await
        .expect("archive extract should publish one storage event")
        .expect("storage change channel should stay open");
    assert_eq!(
        event.kind,
        aster_drive::services::events::storage_change::StorageChangeKind::FolderCreated
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
async fn test_archive_extract_decodes_gb18030_names_by_default() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/files/new")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "legacy-gbk.zip", "folder_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let archive_file_id = body["data"]["id"].as_i64().unwrap();

    let archive_bytes = create_stored_zip_bytes_with_raw_name(
        "aaaaaaaaa.txt",
        b"\xb2\xe2\xca\xd4/\xce\xc4\xbc\xfe.txt",
        b"legacy payload",
    );
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

    let stats = aster_drive::services::task::drain(&state)
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
    let extracted_root_id = body["data"]["result"]["target_folder_id"].as_i64().unwrap();

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{extracted_root_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let folders = body["data"]["folders"].as_array().unwrap();
    let decoded_folder = folders
        .iter()
        .find(|folder| folder["name"] == "测试")
        .expect("decoded Chinese folder should exist");
    let decoded_folder_id = decoded_folder["id"].as_i64().unwrap();

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{decoded_folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let files = body["data"]["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["name"], "文件.txt");
}

#[actix_web::test]
async fn test_archive_extract_empty_directories_publish_non_quota_storage_change_event() {
    let state = common::setup().await;
    let mut storage_events = state.storage_change_bus.subscribe();
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

    let stats = aster_drive::services::task::drain(&state)
        .await
        .expect("task drain should succeed");
    assert_eq!(stats.succeeded, 1);

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), storage_events.recv())
        .await
        .expect("archive extract should publish one storage event")
        .expect("storage change channel should stay open");
    assert_eq!(
        event.kind,
        aster_drive::services::events::storage_change::StorageChangeKind::FolderCreated
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
    let archive_name = format!("{}.zip", "a".repeat(251));
    let expected_display_name = format!("Extract {archive_name}");

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
        .set_json(serde_json::json!({ "name": archive_name, "folder_id": null }))
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
    assert_expanded_task_display_name(&body, &expected_display_name, "archive extract");
    let task_id = body["data"]["id"].as_i64().unwrap();

    let stats = aster_drive::services::task::drain(&state)
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

    let stats = aster_drive::services::task::drain(&state)
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

    aster_drive::services::ops::config::set(&state, "background_task_max_attempts", "1", 1)
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

    let owner =
        aster_drive::db::repository::user_repo::find_by_username(state.writer_db(), "testuser")
            .await
            .expect("owner lookup should succeed")
            .expect("owner should exist");
    let quota_base = owner.storage_used;
    let mut owner_active = owner.into_active_model();
    owner_active.storage_quota = Set(quota_base
        .checked_add(8)
        .expect("quota adjustment should stay within i64"));
    owner_active
        .update(state.writer_db())
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

    let stats = aster_drive::services::task::drain(&state)
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
        aster_forge_utils::paths::task_temp_dir(&state.config.server.temp_dir, task_id);
    assert!(
        !std::path::Path::new(&task_temp_dir).exists(),
        "failed extract task should cleanup temp dir"
    );
}

#[actix_web::test]
async fn test_archive_extract_task_fails_when_staging_limit_is_exceeded() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    aster_drive::services::ops::config::set(&state, "background_task_max_attempts", "1", 1)
        .await
        .expect("background task max attempts config should update");

    let (token, _) = register_and_login!(app);
    let payload = vec![b'a'; 256];
    let archive_bytes = create_zip_bytes(&[("payload.txt", Some(&payload))]);
    let staging_limit = i64::try_from(archive_bytes.len())
        .expect("archive size should fit in i64")
        .checked_add(32)
        .expect("staging limit should fit in i64");
    aster_drive::services::ops::config::set(
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

    let stats = aster_drive::services::task::drain(&state)
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
        aster_forge_utils::paths::task_temp_dir(&state.config.server.temp_dir, task_id);
    assert!(
        !std::path::Path::new(&task_temp_dir).exists(),
        "failed extract task should cleanup temp dir"
    );
}

#[actix_web::test]
async fn test_archive_extract_task_rejects_entry_size_tampering() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    aster_drive::services::ops::config::set(&state, "background_task_max_attempts", "1", 1)
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

    let stats = aster_drive::services::task::drain(&state)
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
        aster_forge_utils::paths::task_temp_dir(&state.config.server.temp_dir, task_id);
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
async fn test_archive_extract_task_rejects_display_only_entry_names() {
    let archive_bytes = create_stored_zip_bytes(&[(
        "folder/name:with-colon.txt",
        Some(b"not importable".as_slice()),
    )]);
    let body = run_failing_personal_archive_extract(archive_bytes, Vec::new()).await;

    assert!(
        body["data"]["last_error"]
            .as_str()
            .expect("failed task should record last error")
            .contains("forbidden character ':'"),
        "extract must still reject archive entries that cannot become AsterDrive names"
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
    aster_drive::services::ops::config::set(&state, "background_task_max_attempts", "1", 1)
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
        aster_drive::db::repository::file_repo::find_by_id(state.writer_db(), archive_file_id)
            .await
            .expect("archive file should be loaded");
    let mut file_active = source_file.into_active_model();
    file_active.size = Set(1);
    file_active
        .update(state.writer_db())
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

    let stats = aster_drive::services::task::drain(&state)
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
    aster_drive::services::ops::config::set(&state, "background_task_max_attempts", "1", 1)
        .await
        .expect("background task max attempts config should update");
    let (token, _) = register_and_login!(app);
    let owner =
        aster_drive::db::repository::user_repo::find_by_username(state.writer_db(), "testuser")
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
    let stats = aster_drive::services::task::drain(&state).await;
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
        state.writer_db(),
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

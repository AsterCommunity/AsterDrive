use super::*;
use crate::storage::error::StorageErrorKind;
use crate::storage::remote_protocol::{
    PRESIGNED_AUTH_ACCESS_KEY_QUERY, PRESIGNED_AUTH_SIGNATURE_QUERY,
    PRESIGNED_RESPONSE_CACHE_CONTROL_QUERY, PRESIGNED_RESPONSE_CONTENT_DISPOSITION_QUERY,
    PRESIGNED_RESPONSE_CONTENT_TYPE_QUERY, RemoteStorageCapabilities,
};
use crate::storage::traits::driver::{PresignedDownloadOptions, StorageDriver};
use crate::storage::traits::extensions::{
    ListStorageDriver, PresignedStorageDriver, StreamUploadDriver,
};
use crate::storage::traits::multipart::MultipartStorageDriver;
use actix_web::{App, HttpResponse, HttpServer, web};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::AsyncReadExt;

struct TestHttpServer {
    base_url: String,
    handle: actix_web::dev::ServerHandle,
    task: tokio::task::JoinHandle<std::io::Result<()>>,
}

impl TestHttpServer {
    async fn stop(self) {
        self.handle.stop(true).await;
        let _ = self.task.await;
    }
}

fn build_policy(base_path: &str) -> storage_policy::Model {
    let now = chrono::Utc::now();
    storage_policy::Model {
        id: 1,
        name: "remote".to_string(),
        driver_type: crate::types::DriverType::Remote,
        endpoint: String::new(),
        bucket: String::new(),
        access_key: String::new(),
        secret_key: String::new(),
        base_path: base_path.to_string(),
        remote_node_id: Some(7),
        remote_storage_target_key: None,
        max_file_size: 0,
        allowed_types: crate::types::StoredStoragePolicyAllowedTypes::empty(),
        options: crate::types::StoredStoragePolicyOptions::empty(),
        is_default: false,
        chunk_size: 5_242_880,
        created_at: now,
        updated_at: now,
    }
}

fn build_follower(base_url: &str) -> managed_follower::Model {
    build_follower_with_capabilities(
        base_url,
        &serde_json::to_string(&RemoteStorageCapabilities::current())
            .expect("current capabilities should serialize"),
    )
}

fn build_follower_with_capabilities(
    base_url: &str,
    last_capabilities: &str,
) -> managed_follower::Model {
    let now = chrono::Utc::now();
    managed_follower::Model {
        id: 7,
        name: "follower".to_string(),
        base_url: base_url.to_string(),
        access_key: "access-key".to_string(),
        secret_key: "secret-key".to_string(),
        is_enabled: true,
        transport_mode: crate::types::RemoteNodeTransportMode::Direct,
        last_capabilities: last_capabilities.to_string(),
        last_error: String::new(),
        last_checked_at: None,
        tunnel_last_error: String::new(),
        tunnel_last_seen_at: None,
        created_at: now,
        updated_at: now,
    }
}

fn build_reverse_follower_with_capabilities(last_capabilities: &str) -> managed_follower::Model {
    let mut follower = build_follower_with_capabilities("", last_capabilities);
    follower.transport_mode = crate::types::RemoteNodeTransportMode::ReverseTunnel;
    follower
}

fn build_driver(base_url: &str, base_path: &str) -> RemoteDriver {
    RemoteDriver::new(&build_policy(base_path), &build_follower(base_url))
        .expect("remote driver should build")
}

fn build_target_scoped_driver(base_url: &str, base_path: &str, target_key: &str) -> RemoteDriver {
    let mut policy = build_policy(base_path);
    policy.remote_storage_target_key = Some(target_key.to_string());
    RemoteDriver::new(&policy, &build_follower(base_url)).expect("remote driver should build")
}

fn build_driver_with_capabilities_err(
    base_url: &str,
    base_path: &str,
    last_capabilities: &str,
) -> AsterError {
    match RemoteDriver::new(
        &build_policy(base_path),
        &build_follower_with_capabilities(base_url, last_capabilities),
    ) {
        Ok(_) => panic!("remote driver should reject capabilities"),
        Err(error) => error,
    }
}

async fn spawn_list_server(
    items: Vec<String>,
    seen_prefixes: Arc<Mutex<Vec<Option<String>>>>,
) -> TestHttpServer {
    async fn list_objects(
        query: web::Query<HashMap<String, String>>,
        items: web::Data<Vec<String>>,
        seen_prefixes: web::Data<Arc<Mutex<Vec<Option<String>>>>>,
    ) -> HttpResponse {
        let prefix = query.get("prefix").cloned();
        seen_prefixes
            .lock()
            .expect("seen_prefixes lock should not be poisoned")
            .push(prefix);
        HttpResponse::Ok().json(serde_json::json!({
            "code": "success",
            "msg": "",
            "data": { "items": items.get_ref() }
        }))
    }

    async fn metadata(path: web::Path<String>) -> HttpResponse {
        let size = match path.as_str() {
            "base/uploads/upload-1/parts/1" => 5u64,
            "base/uploads/upload-1/parts/2" => 7u64,
            _ => 0u64,
        };
        HttpResponse::Ok().json(serde_json::json!({
            "code": "success",
            "msg": "",
            "data": { "size": size, "content_type": null }
        }))
    }

    let listener =
        std::net::TcpListener::bind(("127.0.0.1", 0)).expect("test remote listener should bind");
    let addr = listener
        .local_addr()
        .expect("test remote listener should expose local addr");
    let items_for_server = items.clone();
    let seen_for_server = seen_prefixes.clone();
    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(items_for_server.clone()))
            .app_data(web::Data::new(seen_for_server.clone()))
            .route(
                "/api/v1/internal/storage/objects",
                web::get().to(list_objects),
            )
            .route(
                "/api/v1/internal/storage/objects/{path:.*}/metadata",
                web::get().to(metadata),
            )
    })
    .listen(listener)
    .expect("test remote server should listen")
    .run();
    let handle = server.handle();
    let task = tokio::spawn(server);

    TestHttpServer {
        base_url: format!("http://127.0.0.1:{}", addr.port()),
        handle,
        task,
    }
}

#[test]
fn new_trims_remote_policy_base_path() {
    let driver = build_driver("http://storage.example.com/", "/base/");

    assert_eq!(driver.base_path, "base");
    assert_eq!(driver.object_key("/files/a.txt"), "base/files/a.txt");
    assert_eq!(
        driver.strip_base_path("base/files/a.txt"),
        Some("files/a.txt")
    );
    assert_eq!(driver.strip_base_path("baseball/files/a.txt"), None);
}

#[tokio::test]
async fn list_paths_sends_scoped_prefix_and_strips_base_path() {
    let seen_prefixes = Arc::new(Mutex::new(Vec::new()));
    let server = spawn_list_server(
        vec![
            "base/files/b.txt".to_string(),
            "baseball/files/ignored.txt".to_string(),
            "base/files/a.txt".to_string(),
        ],
        seen_prefixes.clone(),
    )
    .await;
    let driver = build_driver(&server.base_url, "/base/");

    let listed = driver
        .list_paths(Some("files"))
        .await
        .expect("remote list should succeed");

    assert_eq!(
        listed,
        vec!["files/b.txt".to_string(), "files/a.txt".to_string()]
    );
    assert_eq!(
        *seen_prefixes
            .lock()
            .expect("seen_prefixes lock should not be poisoned"),
        vec![Some("base/files".to_string())]
    );

    server.stop().await;
}

#[tokio::test]
async fn presigned_urls_include_base_path_response_options_and_signature() {
    let driver = build_target_scoped_driver("http://storage.example.com/root/", "base", "rst-a");

    let download_url = driver
        .presigned_url(
            "folder/file name.txt",
            Duration::from_secs(60),
            PresignedDownloadOptions {
                download_name: None,
                require_download_name_match: false,
                response_cache_control: Some("private, max-age=60".to_string()),
                response_content_disposition: Some(
                    "attachment; filename=\"file name.txt\"".to_string(),
                ),
                response_content_type: Some("text/plain".to_string()),
            },
        )
        .await
        .expect("download presigned URL should build")
        .expect("remote driver should return URL");
    let parsed = reqwest::Url::parse(&download_url).expect("download URL should parse");
    let query = parsed.query_pairs().into_owned().collect::<HashMap<_, _>>();

    assert_eq!(
        parsed.path(),
        "/root/api/v1/internal/storage/objects/base/folder/file%20name.txt"
    );
    assert_eq!(
        query
            .get(crate::storage::remote_protocol::REMOTE_STORAGE_TARGET_KEY_QUERY)
            .map(String::as_str),
        Some("rst-a")
    );
    assert_eq!(
        query
            .get(PRESIGNED_RESPONSE_CACHE_CONTROL_QUERY)
            .map(String::as_str),
        Some("private, max-age=60")
    );
    assert_eq!(
        query
            .get(PRESIGNED_RESPONSE_CONTENT_DISPOSITION_QUERY)
            .map(String::as_str),
        Some("attachment; filename=\"file name.txt\"")
    );
    assert_eq!(
        query
            .get(PRESIGNED_RESPONSE_CONTENT_TYPE_QUERY)
            .map(String::as_str),
        Some("text/plain")
    );
    assert_eq!(
        query
            .get(PRESIGNED_AUTH_ACCESS_KEY_QUERY)
            .map(String::as_str),
        Some("access-key")
    );
    assert!(query.contains_key(PRESIGNED_AUTH_SIGNATURE_QUERY));

    let put_url = driver
        .presigned_put_url("upload.bin", Duration::from_secs(60))
        .await
        .expect("PUT presigned URL should build")
        .expect("remote driver should return URL");
    let parsed_put = reqwest::Url::parse(&put_url).expect("PUT URL should parse");
    assert_eq!(
        parsed_put.path(),
        "/root/api/v1/internal/storage/objects/base/upload.bin"
    );
    assert!(parsed_put.query_pairs().any(|(key, value)| key
        == crate::storage::remote_protocol::REMOTE_STORAGE_TARGET_KEY_QUERY
        && value == "rst-a"));
    assert!(
        parsed_put
            .query_pairs()
            .any(|(key, _)| key == PRESIGNED_AUTH_SIGNATURE_QUERY)
    );
}

#[tokio::test]
async fn reverse_tunnel_driver_rejects_presigned_browser_urls() {
    let remote_protocol = crate::runtime::PrimaryAppState::new_remote_protocol();
    let follower = build_reverse_follower_with_capabilities(
        r#"{
            "protocol_version":"v4",
            "min_supported_protocol_version":"v4",
            "supports_stream_upload":true,
            "supports_list":true,
            "supports_range_read":true
        }"#,
    );
    let driver = remote_protocol
        .driver_for_policy(&build_policy("base"), &follower)
        .expect("reverse tunnel driver should build");

    let download_error = driver
        .presigned_url(
            "file.txt",
            Duration::from_secs(60),
            PresignedDownloadOptions::default(),
        )
        .await
        .expect_err("reverse tunnel download presigned URL should be rejected");
    assert_eq!(
        download_error.storage_error_kind(),
        Some(StorageErrorKind::Unsupported)
    );
    assert!(download_error.message().contains("reverse tunnel"));

    let upload_error = driver
        .presigned_put_url("file.txt", Duration::from_secs(60))
        .await
        .expect_err("reverse tunnel upload presigned URL should be rejected");
    assert_eq!(
        upload_error.storage_error_kind(),
        Some(StorageErrorKind::Unsupported)
    );
    assert!(upload_error.message().contains("reverse tunnel"));

    let part_error = driver
        .presigned_upload_part_url("file.txt", "upload-1", 1, Duration::from_secs(60))
        .await
        .expect_err("reverse tunnel multipart presigned URL should be rejected");
    assert_eq!(
        part_error.storage_error_kind(),
        Some(StorageErrorKind::Unsupported)
    );
    assert!(part_error.message().contains("reverse tunnel"));
}

#[tokio::test]
async fn put_reader_rejects_negative_size_before_network_io() {
    let driver = build_driver("http://127.0.0.1:9", "base");

    let error = driver
        .put_reader("object.bin", Box::new(tokio::io::empty()), -1)
        .await
        .expect_err("negative size should be rejected locally");

    assert_eq!(
        error.storage_error_kind(),
        Some(StorageErrorKind::Precondition)
    );
    assert!(error.message().contains("must be non-negative"));
}

#[tokio::test]
async fn get_range_forwards_offset_and_length_to_remote_object_request() {
    async fn get_object(query: web::Query<HashMap<String, String>>) -> HttpResponse {
        assert_eq!(query.get("offset").map(String::as_str), Some("7"));
        assert_eq!(query.get("length").map(String::as_str), Some("5"));
        HttpResponse::Ok().body("world")
    }

    let listener =
        std::net::TcpListener::bind(("127.0.0.1", 0)).expect("test remote listener should bind");
    let addr = listener
        .local_addr()
        .expect("test remote listener should expose local addr");
    let server = HttpServer::new(move || {
        App::new().route(
            "/api/v1/internal/storage/objects/base/file.txt",
            web::get().to(get_object),
        )
    })
    .listen(listener)
    .expect("test remote server should listen")
    .run();
    let handle = server.handle();
    let task = tokio::spawn(server);
    let driver = build_driver(&format!("http://127.0.0.1:{}", addr.port()), "base");

    let mut reader = driver.get_range("file.txt", 7, Some(5)).await.unwrap();
    let mut body = Vec::new();
    reader.read_to_end(&mut body).await.unwrap();

    assert_eq!(body, b"world");
    handle.stop(true).await;
    let _ = task.await;
}

#[tokio::test]
async fn v4_remote_driver_rejects_v2_node_without_capacity_support() {
    let error = build_driver_with_capabilities_err(
        "http://127.0.0.1:9",
        "base",
        r#"{
            "protocol_version":"v2",
            "min_supported_protocol_version":"v2",
            "supports_stream_upload":true,
            "supports_list":true,
            "supports_range_read":true
        }"#,
    );
    assert_eq!(
        error.storage_error_kind(),
        Some(StorageErrorKind::Misconfigured)
    );
    assert!(
        error.message().contains("local supports v4-v5"),
        "unexpected error message: {}",
        error.message()
    );
}

#[test]
fn multipart_part_key_rejects_non_positive_part_numbers() {
    let err =
        RemoteDriver::multipart_part_key("upload-1", 0).expect_err("zero part number should fail");

    assert!(err.message().contains("part_number must be positive"));
}

#[tokio::test]
async fn list_uploaded_parts_sorts_and_deduplicates_numeric_part_keys() {
    let seen_prefixes = Arc::new(Mutex::new(Vec::new()));
    let server = spawn_list_server(
        vec![
            "base/uploads/upload-1/parts/2".to_string(),
            "base/uploads/upload-1/parts/not-a-number".to_string(),
            "base/uploads/upload-1/parts/1".to_string(),
            "base/uploads/upload-1/parts/2".to_string(),
        ],
        seen_prefixes.clone(),
    )
    .await;
    let driver = build_driver(&server.base_url, "base");

    let part_details = driver
        .list_uploaded_part_details("ignored.bin", "upload-1")
        .await
        .expect("remote parts should list");
    let parts = part_details
        .iter()
        .map(|part| part.part_number)
        .collect::<Vec<_>>();

    assert_eq!(parts, vec![1, 2]);
    assert_eq!(
        part_details
            .iter()
            .map(|part| part.size)
            .collect::<Vec<_>>(),
        vec![5, 7]
    );
    assert_eq!(
        *seen_prefixes
            .lock()
            .expect("seen_prefixes lock should not be poisoned"),
        vec![Some("base/uploads/upload-1/parts".to_string())]
    );

    server.stop().await;
}

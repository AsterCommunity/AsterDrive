use super::errors::{
    build_remote_status_error_from_parts, remote_api_error, remote_api_error_kind,
    remote_status_error_kind,
};
use super::*;
use crate::api::api_error_code::ApiErrorCode;
use crate::errors::AsterError;
use crate::storage::error::StorageErrorKind;
use crate::storage::traits::driver::PresignedDownloadOptions;
use crate::storage::{StorageCapacityInfo, StorageCapacityStatus};
use crate::types::DriverType;
use actix_web::{App, HttpRequest, HttpResponse, HttpServer, web};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::AsyncReadExt;

#[derive(Debug, Clone, PartialEq, Eq)]
struct LoggedRequest {
    method: String,
    path_and_query: String,
    access_key: Option<String>,
    content_length: Option<String>,
    body: Vec<u8>,
}

#[derive(Default)]
struct ProtocolLog {
    requests: Mutex<Vec<LoggedRequest>>,
}

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

fn log_request(req: &HttpRequest, body: &[u8], log: &web::Data<Arc<ProtocolLog>>) {
    log.requests
        .lock()
        .expect("protocol log lock should not be poisoned")
        .push(LoggedRequest {
            method: req.method().to_string(),
            path_and_query: req
                .uri()
                .path_and_query()
                .map(ToString::to_string)
                .unwrap_or_else(|| req.path().to_string()),
            access_key: req
                .headers()
                .get(INTERNAL_AUTH_ACCESS_KEY_HEADER)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string),
            content_length: req
                .headers()
                .get(reqwest::header::CONTENT_LENGTH.as_str())
                .and_then(|value| value.to_str().ok())
                .map(str::to_string),
            body: body.to_vec(),
        });
}

fn profile_json(target_key: &str) -> serde_json::Value {
    let now = chrono::Utc::now();
    serde_json::json!({
        "target_key": target_key,
        "name": "Local ingress",
        "driver_type": "local",
        "endpoint": "",
        "bucket": "",
        "base_path": "ingress-base",
        "is_default": true,
        "desired_revision": 3,
        "applied_revision": 2,
        "last_error": "",
        "created_at": now,
        "updated_at": now,
    })
}

async fn spawn_protocol_server() -> (TestHttpServer, Arc<ProtocolLog>) {
    async fn capabilities(req: HttpRequest, log: web::Data<Arc<ProtocolLog>>) -> HttpResponse {
        log_request(&req, &[], &log);
        HttpResponse::Ok().json(serde_json::json!({
            "code": "success",
            "msg": "",
            "data": RemoteStorageCapabilities::current()
        }))
    }

    async fn list_objects(
        req: HttpRequest,
        query: web::Query<HashMap<String, String>>,
        log: web::Data<Arc<ProtocolLog>>,
    ) -> HttpResponse {
        log_request(&req, &[], &log);
        let prefix = query.get("prefix").cloned().unwrap_or_default();
        HttpResponse::Ok().json(serde_json::json!({
            "code": "success",
            "msg": "",
            "data": { "items": [format!("{prefix}/one.bin"), format!("{prefix}/two.bin")] }
        }))
    }

    async fn capacity(req: HttpRequest, log: web::Data<Arc<ProtocolLog>>) -> HttpResponse {
        log_request(&req, &[], &log);
        HttpResponse::Ok().json(serde_json::json!({
            "code": "success",
            "msg": "",
            "data": {
                "capacity": StorageCapacityInfo {
                    status: StorageCapacityStatus::Supported,
                    total_bytes: Some(1_000),
                    available_bytes: Some(600),
                    used_bytes: Some(400),
                    source: "remote_test".to_string(),
                    observed_at: chrono::Utc::now(),
                }
            }
        }))
    }

    async fn put_object(
        req: HttpRequest,
        body: web::Bytes,
        log: web::Data<Arc<ProtocolLog>>,
    ) -> HttpResponse {
        log_request(&req, &body, &log);
        HttpResponse::NoContent().finish()
    }

    async fn get_object(
        req: HttpRequest,
        path: web::Path<String>,
        log: web::Data<Arc<ProtocolLog>>,
    ) -> HttpResponse {
        log_request(&req, &[], &log);
        if path.as_str() == "stream.bin" {
            HttpResponse::Ok().body("streamed")
        } else {
            HttpResponse::Ok().body("downloaded")
        }
    }

    async fn head_object(
        req: HttpRequest,
        path: web::Path<String>,
        log: web::Data<Arc<ProtocolLog>>,
    ) -> HttpResponse {
        log_request(&req, &[], &log);
        if path.as_str() == "missing.bin" {
            HttpResponse::NotFound().finish()
        } else {
            HttpResponse::Ok().finish()
        }
    }

    async fn delete_object(req: HttpRequest, log: web::Data<Arc<ProtocolLog>>) -> HttpResponse {
        log_request(&req, &[], &log);
        HttpResponse::NoContent().finish()
    }

    async fn metadata(req: HttpRequest, log: web::Data<Arc<ProtocolLog>>) -> HttpResponse {
        log_request(&req, &[], &log);
        HttpResponse::Ok().json(serde_json::json!({
            "code": "success",
            "msg": "",
            "data": { "size": 42, "content_type": "text/plain" }
        }))
    }

    async fn binding(
        req: HttpRequest,
        body: web::Bytes,
        log: web::Data<Arc<ProtocolLog>>,
    ) -> HttpResponse {
        log_request(&req, &body, &log);
        HttpResponse::NoContent().finish()
    }

    async fn list_profiles(req: HttpRequest, log: web::Data<Arc<ProtocolLog>>) -> HttpResponse {
        log_request(&req, &[], &log);
        HttpResponse::Ok().json(serde_json::json!({
            "code": "success",
            "msg": "",
            "data": [profile_json("profile-a")]
        }))
    }

    async fn create_profile(
        req: HttpRequest,
        body: web::Bytes,
        log: web::Data<Arc<ProtocolLog>>,
    ) -> HttpResponse {
        log_request(&req, &body, &log);
        HttpResponse::Ok().json(serde_json::json!({
            "code": "success",
            "msg": "",
            "data": profile_json("created-profile")
        }))
    }

    async fn update_profile(
        req: HttpRequest,
        body: web::Bytes,
        log: web::Data<Arc<ProtocolLog>>,
    ) -> HttpResponse {
        log_request(&req, &body, &log);
        HttpResponse::Ok().json(serde_json::json!({
            "code": "success",
            "msg": "",
            "data": profile_json("updated-profile")
        }))
    }

    async fn delete_profile(req: HttpRequest, log: web::Data<Arc<ProtocolLog>>) -> HttpResponse {
        log_request(&req, &[], &log);
        HttpResponse::NoContent().finish()
    }

    async fn compose(
        req: HttpRequest,
        body: web::Bytes,
        log: web::Data<Arc<ProtocolLog>>,
    ) -> HttpResponse {
        log_request(&req, &body, &log);
        HttpResponse::Ok().json(serde_json::json!({
            "code": "success",
            "msg": "",
            "data": { "bytes_written": 6 }
        }))
    }

    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("remote protocol test listener should bind");
    let addr = listener
        .local_addr()
        .expect("remote protocol test listener should expose addr");
    let log = Arc::new(ProtocolLog::default());
    let log_for_server = log.clone();
    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(log_for_server.clone()))
            .route(
                "/api/v1/internal/storage/capabilities",
                web::get().to(capabilities),
            )
            .route(
                "/api/v1/internal/storage/objects",
                web::get().to(list_objects),
            )
            .route("/api/v1/internal/storage/capacity", web::get().to(capacity))
            .route(
                "/api/v1/internal/storage/objects/{key:.*}/metadata",
                web::get().to(metadata),
            )
            .route(
                "/api/v1/internal/storage/objects/{key:.*}",
                web::put().to(put_object),
            )
            .route(
                "/api/v1/internal/storage/objects/{key:.*}",
                web::get().to(get_object),
            )
            .route(
                "/api/v1/internal/storage/objects/{key:.*}",
                web::head().to(head_object),
            )
            .route(
                "/api/v1/internal/storage/objects/{key:.*}",
                web::delete().to(delete_object),
            )
            .route("/api/v1/internal/storage/binding", web::put().to(binding))
            .route(
                "/api/v1/internal/storage/targets",
                web::get().to(list_profiles),
            )
            .route(
                "/api/v1/internal/storage/targets",
                web::post().to(create_profile),
            )
            .route(
                "/api/v1/internal/storage/targets/{key:.*}",
                web::patch().to(update_profile),
            )
            .route(
                "/api/v1/internal/storage/targets/{key:.*}",
                web::delete().to(delete_profile),
            )
            .route("/api/v1/internal/storage/compose", web::post().to(compose))
    })
    .listen(listener)
    .expect("remote protocol test server should listen")
    .run();
    let handle = server.handle();
    let task = tokio::spawn(server);

    (
        TestHttpServer {
            base_url: format!("http://127.0.0.1:{}", addr.port()),
            handle,
            task,
        },
        log,
    )
}

#[test]
fn remote_api_error_kind_maps_auth_codes() {
    assert_eq!(
        remote_api_error_kind(ApiErrorCode::CredentialsFailed),
        Some(StorageErrorKind::Auth)
    );
    assert_eq!(
        remote_api_error_kind(ApiErrorCode::TokenExpired),
        Some(StorageErrorKind::Auth)
    );
    assert_eq!(
        remote_api_error_kind(ApiErrorCode::TokenMissing),
        Some(StorageErrorKind::Auth)
    );
    assert_eq!(
        remote_api_error_kind(ApiErrorCode::MfaFailed),
        Some(StorageErrorKind::Auth)
    );
}

#[test]
fn remote_api_error_kind_maps_unsupported_driver() {
    assert_eq!(
        remote_api_error_kind(ApiErrorCode::UnsupportedDriver),
        Some(StorageErrorKind::Unsupported)
    );
    assert_eq!(
        remote_api_error_kind(ApiErrorCode::StorageOperationUnsupported),
        Some(StorageErrorKind::Unsupported)
    );
}

#[test]
fn remote_api_error_kind_maps_storage_and_http_error_codes() {
    assert_eq!(
        remote_api_error_kind(ApiErrorCode::BadRequest),
        Some(StorageErrorKind::Misconfigured)
    );
    assert_eq!(
        remote_api_error_kind(ApiErrorCode::StorageObjectNotFound),
        Some(StorageErrorKind::NotFound)
    );
    assert_eq!(
        remote_api_error_kind(ApiErrorCode::RemoteStorageTargetNotFound),
        Some(StorageErrorKind::NotFound)
    );
    assert_eq!(
        remote_api_error_kind(ApiErrorCode::StorageRateLimited),
        Some(StorageErrorKind::RateLimited)
    );
    assert_eq!(
        remote_api_error_kind(ApiErrorCode::StoragePermissionDenied),
        Some(StorageErrorKind::Permission)
    );
    assert_eq!(
        remote_api_error_kind(ApiErrorCode::StoragePreconditionFailed),
        Some(StorageErrorKind::Precondition)
    );
    assert_eq!(
        remote_api_error_kind(ApiErrorCode::StorageTransientFailure),
        Some(StorageErrorKind::Transient)
    );
    assert_eq!(
        remote_api_error_kind(ApiErrorCode::StorageDriverError),
        Some(StorageErrorKind::Unknown)
    );
}

#[test]
fn remote_status_error_kind_maps_rate_limit_and_server_errors() {
    assert_eq!(
        remote_status_error_kind(reqwest::StatusCode::TOO_MANY_REQUESTS),
        StorageErrorKind::RateLimited
    );
    assert_eq!(
        remote_status_error_kind(reqwest::StatusCode::SERVICE_UNAVAILABLE),
        StorageErrorKind::Transient
    );
    assert_eq!(
        remote_status_error_kind(reqwest::StatusCode::BAD_REQUEST),
        StorageErrorKind::Misconfigured
    );
    assert_eq!(
        remote_status_error_kind(reqwest::StatusCode::UNAUTHORIZED),
        StorageErrorKind::Auth
    );
    assert_eq!(
        remote_status_error_kind(reqwest::StatusCode::FORBIDDEN),
        StorageErrorKind::Permission
    );
    assert_eq!(
        remote_status_error_kind(reqwest::StatusCode::NOT_FOUND),
        StorageErrorKind::NotFound
    );
    assert_eq!(
        remote_status_error_kind(reqwest::StatusCode::CONFLICT),
        StorageErrorKind::Precondition
    );
    assert_eq!(
        remote_status_error_kind(reqwest::StatusCode::METHOD_NOT_ALLOWED),
        StorageErrorKind::Unsupported
    );
    assert_eq!(
        remote_status_error_kind(reqwest::StatusCode::IM_A_TEAPOT),
        StorageErrorKind::Unknown
    );
}

#[test]
fn remote_api_error_maps_storage_quota_exceeded() {
    let err = remote_api_error(
        ApiErrorCode::StorageQuotaExceeded,
        "put remote storage object: quota exceeded",
    )
    .expect("quota error should map");
    assert!(matches!(err, AsterError::StorageQuotaExceeded(_)));
    assert_eq!(err.message(), "put remote storage object: quota exceeded");
}

#[test]
fn s3_ingress_profile_create_debug_redacts_credentials() {
    let request = RemoteCreateS3StorageTargetRequest {
        name: "s3".to_string(),
        endpoint: "https://s3.example.com".to_string(),
        bucket: "bucket-a".to_string(),
        access_key: "plain-access-key".to_string(),
        secret_key: "plain-secret-key".to_string(),
        base_path: "ingress".to_string(),
        is_default: true,
    };

    let rendered = format!("{request:?}");
    assert!(rendered.contains("access_key"));
    assert!(rendered.contains("secret_key"));
    assert!(rendered.contains("<redacted>"));
    assert!(!rendered.contains("plain-access-key"));
    assert!(!rendered.contains("plain-secret-key"));
}

#[test]
fn ingress_profile_update_debug_redacts_optional_credentials() {
    let request = RemoteUpdateStorageTargetRequest {
        name: Some("s3".to_string()),
        driver_type: Some(DriverType::S3),
        endpoint: Some("https://s3.example.com".to_string()),
        bucket: Some("bucket-a".to_string()),
        access_key: Some("plain-access-key".to_string()),
        secret_key: Some("plain-secret-key".to_string()),
        base_path: Some("ingress".to_string()),
        is_default: Some(true),
    };

    let rendered = format!("{request:?}");
    assert!(rendered.contains("access_key"));
    assert!(rendered.contains("secret_key"));
    assert!(rendered.contains("<redacted>"));
    assert!(!rendered.contains("plain-access-key"));
    assert!(!rendered.contains("plain-secret-key"));
}

#[test]
fn storage_target_path_encodes_path_separators_inside_target_key() {
    let client = RemoteStorageClient::new("http://storage.example.com", "ak", "sk")
        .expect("remote client should build");

    assert_eq!(
        client.storage_target_path(" a/b "),
        "/api/v1/internal/storage/targets/a%2Fb"
    );
}

#[test]
fn remote_client_new_validates_required_fields_and_scheme() {
    assert!(RemoteStorageClient::new("", "ak", "sk").is_err());
    assert!(RemoteStorageClient::new("http://storage.example.com", "  ", "sk").is_err());
    assert!(RemoteStorageClient::new("http://storage.example.com", "ak", "  ").is_err());

    let err = match RemoteStorageClient::new("ftp://storage.example.com", "ak", "sk") {
        Ok(_) => panic!("non-http scheme should fail"),
        Err(error) => error,
    };
    assert!(err.message().contains("must use http/https"));
}

#[test]
fn remote_presigned_url_normalizes_base_url_and_rejects_invalid_expiry() {
    let client =
        RemoteStorageClient::new("http://storage.example.com/root/?q=1#frag", " ak ", "sk")
            .expect("remote client should build");

    let url = client
        .presigned_url(
            "folder/file name.txt",
            Duration::from_secs(60),
            PresignedDownloadOptions {
                response_cache_control: Some("private".to_string()),
                response_content_disposition: Some("attachment".to_string()),
                response_content_type: Some("text/plain".to_string()),
            },
        )
        .expect("presigned URL should build");
    let parsed = reqwest::Url::parse(&url).expect("presigned URL should parse");
    let query = parsed.query_pairs().into_owned().collect::<HashMap<_, _>>();

    assert_eq!(
        parsed.path(),
        "/root/api/v1/internal/storage/objects/folder/file%20name.txt"
    );
    assert_eq!(
        query.get("aster_access_key").map(String::as_str),
        Some("ak")
    );
    assert_eq!(
        query.get("response-cache-control").map(String::as_str),
        Some("private")
    );
    assert!(query.contains_key("aster_signature"));

    let target_url = client
        .with_policy_context(Some("rst-primary"), 4096)
        .presigned_put_url("object.bin", Duration::from_secs(60))
        .expect("target-scoped presigned URL should build");
    let target_query = reqwest::Url::parse(&target_url)
        .expect("target URL should parse")
        .query_pairs()
        .into_owned()
        .collect::<HashMap<_, _>>();
    assert_eq!(
        target_query
            .get(REMOTE_STORAGE_TARGET_KEY_QUERY)
            .map(String::as_str),
        Some("rst-primary")
    );
    assert_eq!(
        target_query
            .get(REMOTE_POLICY_MAX_FILE_SIZE_QUERY)
            .map(String::as_str),
        Some("4096")
    );
    assert!(target_query.contains_key(PRESIGNED_AUTH_SIGNATURE_QUERY));

    for max_file_size in [0, -1] {
        let url = client
            .with_policy_context(Some("rst-primary"), max_file_size)
            .presigned_put_url("object.bin", Duration::from_secs(60))
            .expect("non-positive policy max should not block URL signing");
        let query = reqwest::Url::parse(&url)
            .expect("target URL should parse")
            .query_pairs()
            .into_owned()
            .collect::<HashMap<_, _>>();
        assert_eq!(
            query
                .get(REMOTE_STORAGE_TARGET_KEY_QUERY)
                .map(String::as_str),
            Some("rst-primary")
        );
        assert_eq!(
            query
                .get(REMOTE_POLICY_MAX_FILE_SIZE_QUERY)
                .map(String::as_str),
            Some("0")
        );
        assert!(query.contains_key(PRESIGNED_AUTH_SIGNATURE_QUERY));
    }

    let zero = client
        .presigned_put_url("object.bin", Duration::ZERO)
        .expect_err("zero expiry should fail");
    assert_eq!(
        zero.storage_error_kind(),
        Some(StorageErrorKind::Precondition)
    );
    assert!(zero.message().contains("must be positive"));

    let overflow = client
        .presigned_put_url("object.bin", Duration::from_secs(u64::MAX))
        .expect_err("oversized expiry should fail");
    assert_eq!(
        overflow.storage_error_kind(),
        Some(StorageErrorKind::Precondition)
    );
    assert!(overflow.message().contains("exceeds i64 range"));
}

#[test]
fn not_found_record_error_uses_contextual_remote_message() {
    let body = serde_json::json!({
        "code": "not_found",
        "msg": "remote_storage_target 'profile-a'",
    })
    .to_string();
    let err = build_remote_status_error_from_parts(
        reqwest::StatusCode::NOT_FOUND,
        &body,
        "update remote remote storage target",
        false,
    );

    assert!(matches!(err, AsterError::RecordNotFound(_)));
    assert_eq!(
        err.message(),
        "update remote remote storage target: remote_storage_target 'profile-a'"
    );
}

#[test]
fn remote_status_error_preserves_known_api_codes() {
    let body = serde_json::json!({
        "code": "storage.permission",
        "msg": "denied",
        "error": {
            "retryable": false
        }
    })
    .to_string();
    let err = build_remote_status_error_from_parts(
        reqwest::StatusCode::FORBIDDEN,
        &body,
        "get remote storage object",
        false,
    );

    assert_eq!(err.storage_error_kind(), Some(StorageErrorKind::Permission));
    assert_eq!(
        err.api_error_code_override(),
        Some(ApiErrorCode::StoragePermission)
    );
    assert_eq!(err.message(), "get remote storage object: denied");

    let precondition = build_remote_status_error_from_parts(
        reqwest::StatusCode::PRECONDITION_FAILED,
        &serde_json::json!({
            "code": "storage.precondition",
            "msg": "stale revision",
            "error": {
                "retryable": false
            }
        })
        .to_string(),
        "sync remote binding state",
        false,
    );
    assert!(matches!(precondition, AsterError::PreconditionFailed(_)));
    assert_eq!(
        precondition.api_error_code_override(),
        Some(ApiErrorCode::StoragePrecondition)
    );

    let plain = build_remote_status_error_from_parts(
        reqwest::StatusCode::METHOD_NOT_ALLOWED,
        "method blocked",
        "compose remote storage objects",
        false,
    );
    assert_eq!(
        plain.storage_error_kind(),
        Some(StorageErrorKind::Unsupported)
    );
    assert_eq!(
        plain.message(),
        "compose remote storage objects: method blocked"
    );
}

#[tokio::test]
async fn remote_client_object_profile_and_compose_paths_roundtrip() {
    let (server, log) = spawn_protocol_server().await;
    let client = RemoteStorageClient::new(&server.base_url, " access-key ", "secret-key")
        .expect("remote client should build");

    let capabilities = client
        .probe_capabilities()
        .await
        .expect("capabilities should load");
    assert_eq!(capabilities.protocol_version, "v5");
    assert_eq!(capabilities.min_supported_protocol_version, "v4");
    assert!(capabilities.features.object_get);
    assert!(capabilities.features.object_head);
    assert!(capabilities.features.range_get);
    assert!(capabilities.features.accept_ranges_header);
    assert!(capabilities.features.browser_presigned_cors);
    assert!(
        capabilities
            .browser_cors
            .allowed_headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case("range"))
    );
    assert!(
        capabilities
            .browser_cors
            .exposed_headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case("ETag"))
    );
    assert_eq!(
        capabilities
            .browser_cors
            .exposed_headers
            .iter()
            .filter(|header| header.eq_ignore_ascii_case("ETag"))
            .count(),
        1
    );
    assert!(capabilities.supports_list);
    assert!(capabilities.supports_range_read);
    assert!(capabilities.supports_stream_upload);
    assert!(capabilities.supports_capacity);

    client
        .put_bytes("folder/file name?.txt", b"upload")
        .await
        .expect("put bytes should succeed");
    client
        .put_reader(
            "stream-upload.bin",
            Box::new(std::io::Cursor::new(b"stream".to_vec())),
            6,
        )
        .await
        .expect("put reader should succeed");

    assert_eq!(
        client
            .get_bytes("download.bin")
            .await
            .expect("get bytes should succeed"),
        b"downloaded"
    );
    let mut stream = client
        .get_stream("stream.bin", Some(7), Some(5))
        .await
        .expect("get stream should succeed");
    let mut streamed = Vec::new();
    stream.read_to_end(&mut streamed).await.unwrap();
    assert_eq!(streamed, b"streamed");

    assert!(client.exists("exists.bin").await.unwrap());
    assert!(!client.exists("missing.bin").await.unwrap());

    let metadata = client.metadata("meta.bin").await.unwrap();
    assert_eq!(metadata.size, 42);
    assert_eq!(metadata.content_type.as_deref(), Some("text/plain"));

    let listed = client.list_paths(Some("prefix")).await.unwrap();
    assert_eq!(listed, vec!["prefix/one.bin", "prefix/two.bin"]);

    let capacity = client.capacity_info().await.unwrap();
    assert_eq!(capacity.status, StorageCapacityStatus::Supported);
    assert_eq!(capacity.total_bytes, Some(1_000));
    assert_eq!(capacity.available_bytes, Some(600));
    assert_eq!(capacity.used_bytes, Some(400));
    assert_eq!(capacity.source, "remote_test");

    client
        .sync_binding(&RemoteBindingSyncRequest {
            name: "Follower A".to_string(),
            is_enabled: true,
        })
        .await
        .expect("binding sync should succeed");

    let profiles = client.list_storage_targets().await.unwrap();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].target_key, "profile-a");

    let created = client
        .create_storage_target(&RemoteCreateStorageTargetRequest::Local(
            RemoteCreateLocalStorageTargetRequest {
                name: "Managed local".to_string(),
                base_path: "ingress-base".to_string(),
                is_default: true,
            },
        ))
        .await
        .expect("profile create should succeed");
    assert_eq!(created.target_key, "created-profile");

    let updated = client
        .update_storage_target(
            "profile/a",
            &RemoteUpdateStorageTargetRequest {
                name: Some("Updated".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect("profile update should succeed");
    assert_eq!(updated.target_key, "updated-profile");

    client
        .delete_storage_target("profile/a")
        .await
        .expect("profile delete should succeed");
    client
        .delete("delete.bin")
        .await
        .expect("object delete should succeed");

    let composed = client
        .compose_objects(
            "target.bin",
            vec!["part-a".to_string(), "part-b".to_string()],
            6,
        )
        .await
        .expect("compose should succeed");
    assert_eq!(composed.bytes_written, 6);

    let requests = log
        .requests
        .lock()
        .expect("protocol log lock should not be poisoned")
        .clone();
    assert!(
        requests
            .iter()
            .all(|request| request.access_key.as_deref() == Some("access-key")),
        "all signed requests should use trimmed access key: {requests:?}"
    );
    assert!(requests.iter().any(|request| {
        request.method == "PUT"
            && request
                .path_and_query
                .contains("/objects/folder/file%20name%3F.txt")
            && request.content_length.as_deref() == Some("6")
            && request.body == b"upload"
    }));
    assert!(requests.iter().any(|request| {
        request.method == "GET"
            && request
                .path_and_query
                .contains("/objects/stream.bin?offset=7&length=5")
    }));
    assert!(requests.iter().any(|request| {
        request.method == "GET" && request.path_and_query == "/api/v1/internal/storage/capacity"
    }));
    assert!(requests.iter().any(|request| {
        request.method == "PATCH" && request.path_and_query.contains("/targets/profile%2Fa")
    }));
    assert!(requests.iter().any(|request| {
        let path = request
            .path_and_query
            .split_once('?')
            .map_or(request.path_and_query.as_str(), |(path, _)| path);
        request.method == "POST"
            && path.ends_with("/compose")
            && serde_json::from_slice::<serde_json::Value>(&request.body)
                .expect("compose request body should be JSON")["expected_size"]
                == 6
    }));

    server.stop().await;
}

#[tokio::test]
async fn remote_client_maps_envelope_errors_and_missing_data() {
    async fn capabilities_missing_data() -> HttpResponse {
        HttpResponse::Ok().json(serde_json::json!({
            "code": "success",
            "msg": "",
            "data": null
        }))
    }

    async fn list_error() -> HttpResponse {
        HttpResponse::Ok().json(serde_json::json!({
            "code": "storage.permission",
            "msg": "list denied"
        }))
    }

    async fn metadata_invalid_json() -> HttpResponse {
        HttpResponse::Ok().body("not json")
    }

    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("remote protocol error listener should bind");
    let addr = listener
        .local_addr()
        .expect("remote protocol error listener should expose addr");
    let server = HttpServer::new(move || {
        App::new()
            .route(
                "/api/v1/internal/storage/capabilities",
                web::get().to(capabilities_missing_data),
            )
            .route(
                "/api/v1/internal/storage/objects",
                web::get().to(list_error),
            )
            .route(
                "/api/v1/internal/storage/objects/{key:.*}/metadata",
                web::get().to(metadata_invalid_json),
            )
    })
    .listen(listener)
    .expect("remote protocol error server should listen")
    .run();
    let handle = server.handle();
    let task = tokio::spawn(server);
    let client = RemoteStorageClient::new(
        &format!("http://127.0.0.1:{}", addr.port()),
        "access-key",
        "secret-key",
    )
    .expect("remote client should build");

    let capabilities_error = client
        .probe_capabilities()
        .await
        .expect_err("missing capabilities data should fail");
    assert_eq!(
        capabilities_error.storage_error_kind(),
        Some(StorageErrorKind::Misconfigured)
    );
    assert!(
        capabilities_error
            .message()
            .contains("capabilities response missing data")
    );

    let list_error = client
        .list_paths(None)
        .await
        .expect_err("remote list envelope error should fail");
    assert_eq!(
        list_error.storage_error_kind(),
        Some(StorageErrorKind::Permission)
    );
    assert!(list_error.message().contains("list denied"));

    let metadata_error = client
        .metadata("bad.bin")
        .await
        .expect_err("invalid metadata JSON should fail");
    assert_eq!(
        metadata_error.storage_error_kind(),
        Some(StorageErrorKind::Misconfigured)
    );
    assert!(
        metadata_error
            .message()
            .contains("decode remote storage metadata")
    );

    handle.stop(true).await;
    let _ = task.await;
}

#[tokio::test]
async fn remote_client_probe_rejects_v2_capabilities_without_capacity_support() {
    async fn capabilities_v2() -> HttpResponse {
        HttpResponse::Ok().json(serde_json::json!({
            "code": "success",
            "msg": "",
            "data": {
                "protocol_version": "v2",
                "min_supported_protocol_version": "v2",
                "features": RemoteStorageFeatureFlags::current(),
                "browser_cors": RemoteStorageBrowserCorsContract::current(),
                "limits": RemoteStorageProtocolLimits::default(),
                "supports_list": true,
                "supports_range_read": true,
                "supports_stream_upload": true
            }
        }))
    }

    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("v2 capabilities listener should bind");
    let addr = listener
        .local_addr()
        .expect("v2 capabilities listener addr should resolve");
    let server = HttpServer::new(move || {
        App::new().route(
            "/api/v1/internal/storage/capabilities",
            web::get().to(capabilities_v2),
        )
    })
    .listen(listener)
    .expect("v2 capabilities server should listen")
    .run();
    let handle = server.handle();
    let task = tokio::spawn(server);
    let client = RemoteStorageClient::new(
        &format!("http://127.0.0.1:{}", addr.port()),
        "access-key",
        "secret-key",
    )
    .expect("remote client should build");

    let capabilities = client
        .probe_capabilities()
        .await
        .expect_err("v4 client should reject v2 capabilities");
    assert_eq!(
        capabilities.storage_error_kind(),
        Some(StorageErrorKind::Misconfigured)
    );
    assert!(
        capabilities.message().contains("local supports v4-v5"),
        "unexpected error message: {}",
        capabilities.message()
    );

    handle.stop(true).await;
    let _ = task.await;
}

#[tokio::test]
async fn remote_client_capacity_maps_missing_data_and_unsupported_errors() {
    async fn capacity_missing_data() -> HttpResponse {
        HttpResponse::Ok().json(serde_json::json!({
            "code": "success",
            "msg": "",
        }))
    }

    async fn capacity_unsupported() -> HttpResponse {
        HttpResponse::BadRequest().json(serde_json::json!({
            "code": "storage.operation_unsupported",
            "msg": "capacity unsupported"
        }))
    }

    let missing_listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("missing capacity listener should bind");
    let missing_addr = missing_listener
        .local_addr()
        .expect("missing capacity listener addr should resolve");
    let missing_server = HttpServer::new(move || {
        App::new().route(
            "/api/v1/internal/storage/capacity",
            web::get().to(capacity_missing_data),
        )
    })
    .listen(missing_listener)
    .expect("missing capacity server should listen")
    .run();
    let missing_handle = missing_server.handle();
    let missing_task = tokio::spawn(missing_server);
    let missing_client = RemoteStorageClient::new(
        &format!("http://127.0.0.1:{}", missing_addr.port()),
        "access-key",
        "secret-key",
    )
    .expect("missing client should build");

    let missing_error = missing_client
        .capacity_info()
        .await
        .expect_err("missing capacity data should fail");
    assert_eq!(
        missing_error.storage_error_kind(),
        Some(StorageErrorKind::Misconfigured)
    );
    assert!(
        missing_error
            .message()
            .contains("capacity response missing data")
    );
    missing_handle.stop(true).await;
    let _ = missing_task.await;

    let unsupported_listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("unsupported capacity listener should bind");
    let unsupported_addr = unsupported_listener
        .local_addr()
        .expect("unsupported capacity listener addr should resolve");
    let unsupported_server = HttpServer::new(move || {
        App::new().route(
            "/api/v1/internal/storage/capacity",
            web::get().to(capacity_unsupported),
        )
    })
    .listen(unsupported_listener)
    .expect("unsupported capacity server should listen")
    .run();
    let unsupported_handle = unsupported_server.handle();
    let unsupported_task = tokio::spawn(unsupported_server);
    let unsupported_client = RemoteStorageClient::new(
        &format!("http://127.0.0.1:{}", unsupported_addr.port()),
        "access-key",
        "secret-key",
    )
    .expect("unsupported client should build");

    let unsupported_error = unsupported_client
        .capacity_info()
        .await
        .expect_err("remote unsupported should fail");
    assert_eq!(
        unsupported_error.storage_error_kind(),
        Some(StorageErrorKind::Unsupported)
    );
    assert!(unsupported_error.message().contains("capacity unsupported"));
    unsupported_handle.stop(true).await;
    let _ = unsupported_task.await;
}

#[test]
fn capabilities_validation_rejects_incompatible_protocol_versions() {
    let capabilities = RemoteStorageCapabilities {
        protocol_version: "v1".to_string(),
        min_supported_protocol_version: "v1".to_string(),
        ..RemoteStorageCapabilities::current()
    };

    let error = capabilities
        .validate_protocol("test remote node")
        .expect_err("v1 discovery should be incompatible with local protocol");

    assert_eq!(
        error.storage_error_kind(),
        Some(StorageErrorKind::Misconfigured)
    );
    assert!(
        error.message().contains("protocol incompatible"),
        "unexpected error message: {}",
        error.message()
    );
    assert!(
        error.message().contains("local supports v4-v5"),
        "unexpected error message: {}",
        error.message()
    );
}

#[test]
fn capabilities_validation_rejects_v2_remote_nodes() {
    let capabilities = RemoteStorageCapabilities {
        protocol_version: "v2".to_string(),
        min_supported_protocol_version: "v2".to_string(),
        supports_capacity: false,
        ..RemoteStorageCapabilities::current()
    };

    let error = capabilities
        .validate_protocol("test remote node")
        .expect_err("v2 discovery should be incompatible with v4");
    assert!(
        error.message().contains("local supports v4-v5"),
        "unexpected error message: {}",
        error.message()
    );
}

#[test]
fn managed_ingress_capabilities_accept_unknown_driver_ids() {
    let capabilities: RemoteStorageCapabilities = serde_json::from_value(serde_json::json!({
        "protocol_version": "v5",
        "min_supported_protocol_version": "v4",
        "managed_ingress": {
            "enabled": true,
            "driver_types": ["local", "plugin.example.archive"]
        }
    }))
    .expect("unknown managed ingress driver ids should stay wire-compatible");

    let managed_ingress = capabilities
        .managed_ingress
        .as_ref()
        .expect("managed ingress capabilities should decode");
    assert!(managed_ingress.supports_known_driver(DriverType::Local));
    assert!(!managed_ingress.supports_known_driver(DriverType::S3));
    assert_eq!(
        managed_ingress.driver_types[0].as_known_driver_type(),
        Some(DriverType::Local)
    );
    assert_eq!(managed_ingress.driver_types[1].as_known_driver_type(), None);
    assert_eq!(
        managed_ingress.driver_types[1].as_str(),
        "plugin.example.archive"
    );
}

#[test]
fn managed_ingress_capabilities_require_enabled_and_matching_driver() {
    let disabled: RemoteStorageCapabilities = serde_json::from_value(serde_json::json!({
        "protocol_version": "v5",
        "min_supported_protocol_version": "v4",
        "managed_ingress": {
            "enabled": false,
            "driver_types": ["local", "s3"]
        }
    }))
    .expect("disabled managed ingress capabilities should decode");
    assert!(
        !disabled
            .effective_remote_storage_targets()
            .supports_known_driver(DriverType::Local)
    );

    let enabled_without_driver_types: RemoteStorageCapabilities =
        serde_json::from_value(serde_json::json!({
            "protocol_version": "v5",
            "min_supported_protocol_version": "v4",
            "managed_ingress": {
                "enabled": true
            }
        }))
        .expect("missing managed ingress driver_types should decode as empty");
    assert!(
        !enabled_without_driver_types
            .effective_remote_storage_targets()
            .supports_known_driver(DriverType::Local)
    );

    let enabled_with_unknown_only: RemoteStorageCapabilities =
        serde_json::from_value(serde_json::json!({
            "protocol_version": "v5",
            "min_supported_protocol_version": "v4",
            "managed_ingress": {
                "enabled": true,
                "driver_types": ["plugin.example.archive"]
            }
        }))
        .expect("unknown-only managed ingress capabilities should decode");
    assert!(
        !enabled_with_unknown_only
            .effective_remote_storage_targets()
            .supports_known_driver(DriverType::Local)
    );
}

#[test]
fn managed_ingress_capabilities_serialize_known_driver_ids_as_strings() {
    let capabilities = RemoteStorageCapabilities::current()
        .with_remote_storage_target_driver_types(vec![DriverType::Local, DriverType::S3]);

    let value =
        serde_json::to_value(&capabilities).expect("managed ingress capabilities should serialize");
    assert_eq!(
        value["managed_ingress"]["driver_types"],
        serde_json::json!(["local", "s3"])
    );
    assert_eq!(value["managed_ingress"]["enabled"], serde_json::json!(true));

    let roundtripped: RemoteStorageCapabilities =
        serde_json::from_value(value).expect("serialized capabilities should roundtrip");
    let effective = roundtripped.effective_remote_storage_targets();
    assert!(effective.supports_known_driver(DriverType::Local));
    assert!(effective.supports_known_driver(DriverType::S3));
    assert!(!effective.supports_known_driver(DriverType::Remote));
}

#[test]
fn missing_managed_ingress_capabilities_default_only_for_legacy_v4() {
    let legacy_v4: RemoteStorageCapabilities = serde_json::from_value(serde_json::json!({
        "protocol_version": "v4",
        "min_supported_protocol_version": "v4"
    }))
    .expect("legacy v4 capabilities without managed ingress should decode");

    let effective_legacy = legacy_v4.effective_remote_storage_targets();
    assert!(effective_legacy.supports_known_driver(DriverType::Local));
    assert!(effective_legacy.supports_known_driver(DriverType::S3));
    assert!(!effective_legacy.supports_known_driver(DriverType::Remote));

    let unknown = RemoteStorageCapabilities::unknown();
    assert!(
        !unknown
            .effective_remote_storage_targets()
            .supports_known_driver(DriverType::Local)
    );

    let empty = RemoteStorageCapabilities::from_stored_json("{}");
    assert!(
        !empty
            .effective_remote_storage_targets()
            .supports_known_driver(DriverType::Local)
    );

    let v5_without_field: RemoteStorageCapabilities = serde_json::from_value(serde_json::json!({
        "protocol_version": "v5",
        "min_supported_protocol_version": "v4"
    }))
    .expect("v5 capabilities without managed ingress should decode conservatively");
    assert!(
        !v5_without_field
            .effective_remote_storage_targets()
            .supports_known_driver(DriverType::Local)
    );

    let v4_with_null_field: RemoteStorageCapabilities = serde_json::from_value(serde_json::json!({
        "protocol_version": "v4",
        "min_supported_protocol_version": "v4",
        "managed_ingress": null
    }))
    .expect("legacy v4 capabilities with explicit null managed_ingress should decode");
    assert!(
        v4_with_null_field
            .effective_remote_storage_targets()
            .supports_known_driver(DriverType::Local)
    );

    let v4_with_explicit_empty_field: RemoteStorageCapabilities =
        serde_json::from_value(serde_json::json!({
            "protocol_version": "v4",
            "min_supported_protocol_version": "v4",
            "managed_ingress": {
                "enabled": false,
                "driver_types": []
            }
        }))
        .expect("legacy v4 capabilities with explicit managed_ingress should decode");
    assert!(
        !v4_with_explicit_empty_field
            .effective_remote_storage_targets()
            .supports_known_driver(DriverType::Local)
    );
}

#[test]
fn capabilities_validation_blocks_remote_presigned_download_without_browser_range_cors() {
    let mut capabilities = RemoteStorageCapabilities::current();
    capabilities.browser_cors.allowed_headers = vec!["content-type".to_string()];
    capabilities.browser_cors.exposed_headers =
        vec!["Accept-Ranges".to_string(), "Content-Length".to_string()];
    let options = crate::types::StoragePolicyOptions {
        remote_download_strategy: Some(crate::types::RemoteDownloadStrategy::Presigned),
        ..Default::default()
    };

    let error = capabilities
        .validate_for_remote_policy(7, 42, &options)
        .expect_err("missing Range/CORS headers should block presigned remote download");

    assert_eq!(
        error.storage_error_kind(),
        Some(StorageErrorKind::Misconfigured)
    );
    assert!(
        error
            .message()
            .contains("browser CORS contract is incomplete"),
        "unexpected error message: {}",
        error.message()
    );
    assert!(error.message().contains("allowed_headers missing range"));
    assert!(
        error
            .message()
            .contains("exposed_headers missing Content-Range")
    );
}

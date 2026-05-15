use super::errors::{
    build_remote_status_error_from_parts, remote_api_error, remote_api_error_kind,
    remote_status_error_kind,
};
use super::*;
use crate::api::error_code::ErrorCode;
use crate::errors::AsterError;
use crate::storage::driver::PresignedDownloadOptions;
use crate::storage::error::StorageErrorKind;
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

fn profile_json(profile_key: &str) -> serde_json::Value {
    let now = chrono::Utc::now();
    serde_json::json!({
        "profile_key": profile_key,
        "name": "Local ingress",
        "driver_type": "local",
        "endpoint": "",
        "bucket": "",
        "base_path": "ingress-base",
        "max_file_size": 1024,
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
            "code": 0,
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
            "code": 0,
            "msg": "",
            "data": { "items": [format!("{prefix}/one.bin"), format!("{prefix}/two.bin")] }
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
            "code": 0,
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
            "code": 0,
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
            "code": 0,
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
            "code": 0,
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
            "code": 0,
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
                "/api/v1/internal/storage/ingress-profiles",
                web::get().to(list_profiles),
            )
            .route(
                "/api/v1/internal/storage/ingress-profiles",
                web::post().to(create_profile),
            )
            .route(
                "/api/v1/internal/storage/ingress-profiles/{key:.*}",
                web::patch().to(update_profile),
            )
            .route(
                "/api/v1/internal/storage/ingress-profiles/{key:.*}",
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
        remote_api_error_kind(ErrorCode::AuthFailed as i32),
        Some(StorageErrorKind::Auth)
    );
    assert_eq!(
        remote_api_error_kind(ErrorCode::TokenExpired as i32),
        Some(StorageErrorKind::Auth)
    );
}

#[test]
fn remote_api_error_kind_maps_unsupported_driver() {
    assert_eq!(
        remote_api_error_kind(ErrorCode::UnsupportedDriver as i32),
        Some(StorageErrorKind::Unsupported)
    );
    assert_eq!(
        remote_api_error_kind(ErrorCode::StorageOperationUnsupported as i32),
        Some(StorageErrorKind::Unsupported)
    );
}

#[test]
fn remote_api_error_kind_maps_storage_and_http_error_codes() {
    assert_eq!(
        remote_api_error_kind(ErrorCode::BadRequest as i32),
        Some(StorageErrorKind::Misconfigured)
    );
    assert_eq!(
        remote_api_error_kind(ErrorCode::StorageObjectNotFound as i32),
        Some(StorageErrorKind::NotFound)
    );
    assert_eq!(
        remote_api_error_kind(ErrorCode::StorageRateLimited as i32),
        Some(StorageErrorKind::RateLimited)
    );
    assert_eq!(
        remote_api_error_kind(ErrorCode::StoragePermissionDenied as i32),
        Some(StorageErrorKind::Permission)
    );
    assert_eq!(
        remote_api_error_kind(ErrorCode::StoragePreconditionFailed as i32),
        Some(StorageErrorKind::Precondition)
    );
    assert_eq!(
        remote_api_error_kind(ErrorCode::StorageTransientFailure as i32),
        Some(StorageErrorKind::Transient)
    );
    assert_eq!(
        remote_api_error_kind(ErrorCode::StorageDriverError as i32),
        Some(StorageErrorKind::Unknown)
    );
    assert_eq!(remote_api_error_kind(999_999), None);
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
        ErrorCode::StorageQuotaExceeded as i32,
        "put remote storage object: quota exceeded",
    )
    .expect("quota error should map");
    assert!(matches!(err, AsterError::StorageQuotaExceeded(_)));
    assert_eq!(err.message(), "put remote storage object: quota exceeded");
}

#[test]
fn s3_ingress_profile_create_debug_redacts_credentials() {
    let request = RemoteCreateS3IngressProfileRequest {
        name: "s3".to_string(),
        endpoint: "https://s3.example.com".to_string(),
        bucket: "bucket-a".to_string(),
        access_key: "plain-access-key".to_string(),
        secret_key: "plain-secret-key".to_string(),
        base_path: "ingress".to_string(),
        max_file_size: 1024,
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
    let request = RemoteUpdateIngressProfileRequest {
        name: Some("s3".to_string()),
        driver_type: Some(DriverType::S3),
        endpoint: Some("https://s3.example.com".to_string()),
        bucket: Some("bucket-a".to_string()),
        access_key: Some("plain-access-key".to_string()),
        secret_key: Some("plain-secret-key".to_string()),
        base_path: Some("ingress".to_string()),
        max_file_size: Some(1024),
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
fn ingress_profile_url_encodes_path_separators_inside_profile_key() {
    let client = RemoteStorageClient::new("http://storage.example.com", "ak", "sk")
        .expect("remote client should build");

    let url = client
        .ingress_profile_url(" a/b ")
        .expect("profile URL should build");

    assert_eq!(
        url.path(),
        "/api/v1/internal/storage/ingress-profiles/a%2Fb"
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
        "code": ErrorCode::NotFound as i32,
        "msg": "managed_ingress_profile 'profile-a'",
    })
    .to_string();
    let err = build_remote_status_error_from_parts(
        reqwest::StatusCode::NOT_FOUND,
        &body,
        "update remote ingress profile",
        false,
    );

    assert!(matches!(err, AsterError::RecordNotFound(_)));
    assert_eq!(
        err.message(),
        "update remote ingress profile: managed_ingress_profile 'profile-a'"
    );
}

#[test]
fn remote_status_error_preserves_subcodes_and_plain_body_context() {
    let body = serde_json::json!({
        "code": ErrorCode::StoragePermissionDenied as i32,
        "msg": "denied",
        "error": {
            "internal_code": "E031",
            "subcode": "storage.remote_permission",
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
    assert_eq!(err.api_error_subcode(), Some("storage.remote_permission"));
    assert_eq!(err.message(), "get remote storage object: denied");

    let precondition = build_remote_status_error_from_parts(
        reqwest::StatusCode::PRECONDITION_FAILED,
        &serde_json::json!({
            "code": ErrorCode::PreconditionFailed as i32,
            "msg": "stale revision",
            "error": {
                "internal_code": "E060",
                "subcode": "remote.revision_stale",
                "retryable": false
            }
        })
        .to_string(),
        "sync remote binding state",
        false,
    );
    assert!(matches!(precondition, AsterError::PreconditionFailed(_)));
    assert_eq!(
        precondition.api_error_subcode(),
        Some("remote.revision_stale")
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
    assert_eq!(capabilities.protocol_version, "v2");
    assert_eq!(capabilities.min_supported_protocol_version, "v2");
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
    assert!(capabilities.supports_list);
    assert!(capabilities.supports_range_read);
    assert!(capabilities.supports_stream_upload);

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

    client
        .sync_binding(&RemoteBindingSyncRequest {
            name: "Follower A".to_string(),
            is_enabled: true,
        })
        .await
        .expect("binding sync should succeed");

    let profiles = client.list_ingress_profiles().await.unwrap();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].profile_key, "profile-a");

    let created = client
        .create_ingress_profile(&RemoteCreateIngressProfileRequest::Local(
            RemoteCreateLocalIngressProfileRequest {
                name: "Managed local".to_string(),
                base_path: "ingress-base".to_string(),
                max_file_size: 1024,
                is_default: true,
            },
        ))
        .await
        .expect("profile create should succeed");
    assert_eq!(created.profile_key, "created-profile");

    let updated = client
        .update_ingress_profile(
            "profile/a",
            &RemoteUpdateIngressProfileRequest {
                name: Some("Updated".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect("profile update should succeed");
    assert_eq!(updated.profile_key, "updated-profile");

    client
        .delete_ingress_profile("profile/a")
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
        request.method == "PATCH"
            && request
                .path_and_query
                .contains("/ingress-profiles/profile%2Fa")
    }));
    assert!(requests.iter().any(|request| {
        request.method == "POST"
            && request.path_and_query.ends_with("/compose")
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
            "code": 0,
            "msg": "",
            "data": null
        }))
    }

    async fn list_error() -> HttpResponse {
        HttpResponse::Ok().json(serde_json::json!({
            "code": ErrorCode::StoragePermissionDenied as i32,
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

#[test]
fn capabilities_validation_rejects_incompatible_protocol_versions() {
    let capabilities = RemoteStorageCapabilities {
        protocol_version: "v1".to_string(),
        min_supported_protocol_version: "v1".to_string(),
        ..RemoteStorageCapabilities::current()
    };

    let error = capabilities
        .validate_protocol("test remote node")
        .expect_err("v1 discovery should be incompatible with v2 local protocol");

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
        error.message().contains("local supports v2-v2"),
        "unexpected error message: {}",
        error.message()
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

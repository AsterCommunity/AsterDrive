//! 远端存储集成测试。

#[macro_use]
mod common;

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use actix_web::dev::Service;
use actix_web::{App, HttpServer, test, web};
use aster_drive::api::api_error_code::ApiErrorCode;
use aster_drive::db::repository::{
    file_repo, follower_enrollment_session_repo, managed_follower_repo, master_binding_repo,
    policy_repo, upload_session_part_repo, upload_session_repo, user_repo,
};
use aster_drive::entities::{follower_enrollment_session, storage_policy};
use aster_drive::runtime::SharedRuntimeState;
use aster_drive::services::{
    auth::local, files::file, files::folder, remote::master_binding, remote::remote_node,
    remote::storage_target, storage_policy::policy, upload_service,
};
use aster_drive::storage::remote_protocol::tunnel::server::{
    REMOTE_TUNNEL_BODY_LIMIT, REMOTE_TUNNEL_COMPLETE_PATH, REMOTE_TUNNEL_JSON_LIMIT,
    REMOTE_TUNNEL_POLL_PATH, RemoteTunnelResponse,
};
use aster_drive::storage::remote_protocol::{
    INTERNAL_AUTH_ACCESS_KEY_HEADER, INTERNAL_AUTH_NONCE_HEADER, INTERNAL_AUTH_SIGNATURE_HEADER,
    INTERNAL_AUTH_TIMESTAMP_HEADER, INTERNAL_STORAGE_BASE_PATH,
    INTERNAL_STORAGE_MIN_SUPPORTED_PROTOCOL_VERSION_LABEL, INTERNAL_STORAGE_PROTOCOL_VERSION_LABEL,
    RemoteBindingSyncRequest, RemoteCreateLocalStorageTargetRequest,
    RemoteCreateS3StorageTargetRequest, RemoteCreateStorageTargetRequest,
    RemoteStorageCapabilities, RemoteStorageClient, RemoteStorageComposeRequest,
    RemoteUpdateStorageTargetRequest, sign_internal_request, sign_presigned_request,
};
use aster_drive::types::{
    DriverType, RemoteDownloadStrategy, RemoteNodeTransportMode, RemoteUploadStrategy,
    StoragePolicyOptions, StoredStoragePolicyAllowedTypes, serialize_storage_policy_options,
};
use bytes::Bytes;
use chrono::{Duration as ChronoDuration, Utc};
use futures::TryStreamExt;
use futures::future::join_all;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;

struct TestHttpServer {
    base_url: String,
    handle: actix_web::dev::ServerHandle,
    task: tokio::task::JoinHandle<std::io::Result<()>>,
}

struct TestReverseTunnelHttpWorker {
    shutdown: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

struct RawHttpResponse {
    status: u16,
    headers: std::collections::HashMap<String, String>,
    body: Vec<u8>,
    trailing: Vec<u8>,
}

impl TestHttpServer {
    async fn stop(self) {
        self.handle.stop(true).await;
        let _ = self.task.await;
    }
}

async fn spawn_internal_storage_server(
    state: aster_drive::runtime::FollowerAppState,
) -> TestHttpServer {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("test internal storage listener should bind");
    spawn_internal_storage_server_on_listener(state, listener).await
}

async fn spawn_internal_storage_server_on_listener(
    state: aster_drive::runtime::FollowerAppState,
    listener: std::net::TcpListener,
) -> TestHttpServer {
    let addr = listener
        .local_addr()
        .expect("test internal storage listener should expose local addr");
    let state_for_server = state.clone();
    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state_for_server.clone()))
            .service(
                web::scope("/api/v1").service(aster_drive::api::routes::internal_storage::routes()),
            )
    })
    .listen(listener)
    .expect("test internal storage server should listen")
    .run();
    let handle = server.handle();
    let task = tokio::spawn(server);

    TestHttpServer {
        base_url: format!("http://127.0.0.1:{}", addr.port()),
        handle,
        task,
    }
}

fn follower_view_with_server_port(
    state: &aster_drive::runtime::PrimaryAppState,
    port: u16,
) -> aster_drive::runtime::FollowerAppState {
    let mut config = (*state.config).clone();
    config.server.port = port;

    aster_drive::runtime::FollowerAppState {
        db_handles: state.db_handles.clone(),
        driver_registry: state.driver_registry.clone(),
        runtime_config: state.runtime_config.clone(),
        policy_snapshot: state.policy_snapshot.clone(),
        config: Arc::new(config),
        cache: state.cache.clone(),
        metrics: state.metrics.clone(),
    }
}

async fn spawn_reverse_tunnel_primary_server(
    state: aster_drive::runtime::PrimaryAppState,
) -> TestHttpServer {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("test reverse tunnel primary listener should bind");
    let addr = listener
        .local_addr()
        .expect("test reverse tunnel primary listener should expose local addr");
    let state_for_server = state.clone();
    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::PayloadConfig::new(REMOTE_TUNNEL_JSON_LIMIT))
            .app_data(web::JsonConfig::default().limit(REMOTE_TUNNEL_JSON_LIMIT))
            .app_data(web::Data::new(state_for_server.clone()))
            .service(
                web::scope("/api/v1").service(aster_drive::api::routes::remote_tunnel::routes()),
            )
    })
    .listen(listener)
    .expect("test reverse tunnel primary server should listen")
    .run();
    let handle = server.handle();
    let task = tokio::spawn(server);

    TestHttpServer {
        base_url: format!("http://127.0.0.1:{}", addr.port()),
        handle,
        task,
    }
}

async fn spawn_reverse_tunnel_poll_only_primary_server(
    state: aster_drive::runtime::PrimaryAppState,
) -> TestHttpServer {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("test reverse tunnel poll-only primary listener should bind");
    let addr = listener
        .local_addr()
        .expect("test reverse tunnel poll-only primary listener should expose local addr");
    let state_for_server = state.clone();
    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::PayloadConfig::new(REMOTE_TUNNEL_JSON_LIMIT))
            .app_data(web::JsonConfig::default().limit(REMOTE_TUNNEL_JSON_LIMIT))
            .app_data(web::Data::new(state_for_server.clone()))
            .service(
                web::scope("/api/v1/internal/remote-tunnel")
                    .route(
                        "/poll",
                        web::post().to(aster_drive::api::routes::remote_tunnel::poll_remote_tunnel),
                    )
                    .route(
                        "/complete",
                        web::post()
                            .to(aster_drive::api::routes::remote_tunnel::complete_remote_tunnel),
                    ),
            )
    })
    .listen(listener)
    .expect("test reverse tunnel poll-only primary server should listen")
    .run();
    let handle = server.handle();
    let task = tokio::spawn(server);

    TestHttpServer {
        base_url: format!("http://127.0.0.1:{}", addr.port()),
        handle,
        task,
    }
}

async fn spawn_capabilities_server(capabilities: serde_json::Value) -> TestHttpServer {
    async fn capabilities_handler(
        capabilities: web::Data<serde_json::Value>,
    ) -> actix_web::HttpResponse {
        actix_web::HttpResponse::Ok().json(serde_json::json!({
            "code": "success",
            "msg": "",
            "data": capabilities.get_ref().clone(),
        }))
    }

    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("test capabilities listener should bind");
    let addr = listener
        .local_addr()
        .expect("test capabilities listener should expose local addr");
    let capabilities_for_server = capabilities.clone();
    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(capabilities_for_server.clone()))
            .route(
                "/api/v1/internal/storage/capabilities",
                web::get().to(capabilities_handler),
            )
    })
    .listen(listener)
    .expect("test capabilities server should listen")
    .run();
    let handle = server.handle();
    let task = tokio::spawn(server);

    TestHttpServer {
        base_url: format!("http://127.0.0.1:{}", addr.port()),
        handle,
        task,
    }
}

async fn spawn_counting_internal_storage_server(
    state: aster_drive::runtime::FollowerAppState,
) -> (TestHttpServer, Arc<AtomicUsize>) {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("test internal storage listener should bind");
    let addr = listener
        .local_addr()
        .expect("test internal storage listener should expose local addr");
    let state_for_server = state.clone();
    let request_count = Arc::new(AtomicUsize::new(0));
    let request_count_for_server = request_count.clone();
    let server = HttpServer::new(move || {
        let request_count = request_count_for_server.clone();
        App::new()
            .wrap_fn(move |req, srv| {
                request_count.fetch_add(1, Ordering::Relaxed);
                srv.call(req)
            })
            .app_data(web::Data::new(state_for_server.clone()))
            .service(
                web::scope("/api/v1").service(aster_drive::api::routes::internal_storage::routes()),
            )
    })
    .listen(listener)
    .expect("counting internal storage server should listen")
    .run();
    let handle = server.handle();
    let task = tokio::spawn(server);

    (
        TestHttpServer {
            base_url: format!("http://127.0.0.1:{}", addr.port()),
            handle,
            task,
        },
        request_count,
    )
}

async fn start_test_reverse_tunnel_worker(
    primary_state: aster_drive::runtime::PrimaryAppState,
    remote_node: aster_drive::entities::managed_follower::Model,
    follower_base_url: String,
) -> (CancellationToken, tokio::task::JoinHandle<()>) {
    let shutdown = CancellationToken::new();
    let worker_shutdown = shutdown.clone();
    let handle = tokio::spawn(async move {
        let client = reqwest::Client::new();
        loop {
            let poll_result = tokio::select! {
                biased;
                _ = worker_shutdown.cancelled() => break,
                result = aster_drive::storage::remote_protocol::tunnel::server::poll(
                    &primary_state,
                    &remote_node,
                ) => result,
            };
            let poll = poll_result.expect("reverse tunnel poll should succeed");
            let Some(request) = poll.request else {
                continue;
            };

            let response =
                execute_test_reverse_tunnel_request(&client, &follower_base_url, request).await;
            aster_drive::storage::remote_protocol::tunnel::server::complete(
                &primary_state,
                &remote_node,
                response,
            )
            .await
            .expect("reverse tunnel completion should succeed");
        }
    });
    (shutdown, handle)
}

async fn execute_test_reverse_tunnel_request(
    client: &reqwest::Client,
    follower_base_url: &str,
    request: aster_drive::storage::remote_protocol::tunnel::server::RemoteTunnelRequest,
) -> RemoteTunnelResponse {
    let method = reqwest::Method::from_bytes(request.method.as_bytes())
        .expect("reverse tunnel test request method should parse");
    let response = client
        .request(
            method,
            format!("{}{}", follower_base_url, request.path_and_query),
        )
        .headers(reverse_tunnel_test_headers(request.headers))
        .body(request.body)
        .send()
        .await
        .expect("reverse tunnel test local request should succeed");
    aster_drive::storage::remote_protocol::tunnel::server::tunnel_response_from_reqwest(
        request.request_id,
        response,
    )
    .await
    .expect("reverse tunnel test response should convert")
}

fn reverse_tunnel_test_headers(headers: Vec<(String, String)>) -> reqwest::header::HeaderMap {
    let mut map = reqwest::header::HeaderMap::new();
    for (name, value) in headers {
        let name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
            .expect("reverse tunnel test header name should parse");
        let value = reqwest::header::HeaderValue::from_str(&value)
            .expect("reverse tunnel test header value should parse");
        map.append(name, value);
    }
    map
}

async fn stop_test_reverse_tunnel_worker(
    shutdown: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
) {
    shutdown.cancel();
    match tokio::time::timeout(Duration::from_secs(7), handle).await {
        Ok(join_result) => join_result.expect("reverse tunnel test worker task failed"),
        Err(_) => panic!("reverse tunnel test worker did not shut down"),
    }
}

async fn start_production_reverse_tunnel_worker(
    follower_state: aster_drive::runtime::FollowerAppState,
) -> TestReverseTunnelHttpWorker {
    let shutdown = CancellationToken::new();
    let worker_shutdown = shutdown.clone();
    let handle = tokio::spawn(async move {
        aster_drive::storage::remote_protocol::tunnel::client::run_follower_tunnel_worker(
            web::Data::new(follower_state),
            worker_shutdown,
        )
        .await;
    });

    TestReverseTunnelHttpWorker { shutdown, handle }
}

async fn stop_test_reverse_tunnel_http_worker(worker: TestReverseTunnelHttpWorker) {
    stop_test_reverse_tunnel_worker(worker.shutdown, worker.handle).await;
}

async fn send_signed_tunnel_json_raw(
    client: &reqwest::Client,
    remote_node: &aster_drive::entities::managed_follower::Model,
    master_url: &str,
    path: &str,
    value: serde_json::Value,
) -> (reqwest::StatusCode, serde_json::Value) {
    let body = serde_json::to_vec(&value).expect("tunnel JSON body should encode");
    let url = format!("{master_url}{path}");
    let response = client
        .post(url)
        .headers(signed_tunnel_http_headers(
            remote_node,
            "POST",
            path,
            Some(u64::try_from(body.len()).expect("tunnel test body length should fit u64")),
        ))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body)
        .send()
        .await
        .expect("signed tunnel HTTP request should send");
    let status = response.status();
    let body = response
        .bytes()
        .await
        .expect("signed tunnel HTTP response body should read");
    let value: serde_json::Value =
        serde_json::from_slice(&body).expect("signed tunnel response should be JSON");
    (status, value)
}

fn signed_tunnel_http_headers(
    remote_node: &aster_drive::entities::managed_follower::Model,
    method: &str,
    path: &str,
    content_length: Option<u64>,
) -> reqwest::header::HeaderMap {
    let timestamp = Utc::now().timestamp();
    let nonce = uuid::Uuid::new_v4().to_string();
    let signature = sign_internal_request(
        &remote_node.secret_key,
        method,
        path,
        timestamp,
        &nonce,
        content_length,
    );
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::HeaderName::from_static(INTERNAL_AUTH_ACCESS_KEY_HEADER),
        reqwest::header::HeaderValue::from_str(&remote_node.access_key)
            .expect("access key header value should parse"),
    );
    headers.insert(
        reqwest::header::HeaderName::from_static(INTERNAL_AUTH_TIMESTAMP_HEADER),
        reqwest::header::HeaderValue::from_str(&timestamp.to_string())
            .expect("timestamp header value should parse"),
    );
    headers.insert(
        reqwest::header::HeaderName::from_static(INTERNAL_AUTH_NONCE_HEADER),
        reqwest::header::HeaderValue::from_str(&nonce).expect("nonce header value should parse"),
    );
    headers.insert(
        reqwest::header::HeaderName::from_static(INTERNAL_AUTH_SIGNATURE_HEADER),
        reqwest::header::HeaderValue::from_str(&signature)
            .expect("signature header value should parse"),
    );
    if let Some(content_length) = content_length {
        headers.insert(
            reqwest::header::CONTENT_LENGTH,
            reqwest::header::HeaderValue::from_str(&content_length.to_string())
                .expect("content length header value should parse"),
        );
    }
    headers
}

async fn create_remote_policy(
    state: &aster_drive::runtime::PrimaryAppState,
    remote_node_id: i64,
    name: &str,
    base_path: &str,
) -> storage_policy::Model {
    create_remote_policy_with_options(
        state,
        remote_node_id,
        name,
        base_path,
        StoragePolicyOptions::default(),
        5_242_880,
    )
    .await
}

async fn create_remote_policy_with_options(
    state: &aster_drive::runtime::PrimaryAppState,
    remote_node_id: i64,
    name: &str,
    base_path: &str,
    options: StoragePolicyOptions,
    chunk_size: i64,
) -> storage_policy::Model {
    let now = Utc::now();
    let policy = policy_repo::create(
        state.writer_db(),
        storage_policy::ActiveModel {
            name: Set(name.to_string()),
            driver_type: Set(DriverType::Remote),
            endpoint: Set(String::new()),
            bucket: Set(String::new()),
            access_key: Set(String::new()),
            secret_key: Set(String::new()),
            base_path: Set(base_path.to_string()),
            remote_node_id: Set(Some(remote_node_id)),
            max_file_size: Set(0),
            allowed_types: Set(StoredStoragePolicyAllowedTypes::empty()),
            options: Set(serialize_storage_policy_options(&options)
                .expect("remote policy options should serialize")),
            is_default: Set(false),
            chunk_size: Set(chunk_size),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .expect("remote policy should be created");

    state
        .policy_snapshot
        .reload(state.writer_db())
        .await
        .expect("policy snapshot should reload after creating remote policy");

    policy
}

async fn create_remote_policy_via_service_with_options(
    state: &aster_drive::runtime::PrimaryAppState,
    remote_node_id: i64,
    name: &str,
    base_path: &str,
    target_key: &str,
    options: StoragePolicyOptions,
    chunk_size: i64,
) -> storage_policy::Model {
    let created = policy::create(
        state,
        policy::CreateStoragePolicyInput {
            name: name.to_string(),
            connection: policy::StoragePolicyConnectionInput {
                driver_type: DriverType::Remote,
                endpoint: String::new(),
                bucket: String::new(),
                access_key: String::new(),
                secret_key: String::new(),
                base_path: base_path.to_string(),
                remote_node_id: Some(remote_node_id),
                remote_storage_target_key: Some(target_key.to_string()),
                options: Default::default(),
            },
            max_file_size: 0,
            chunk_size: Some(chunk_size),
            is_default: false,
            allowed_types: None,
            options: Some(options),
            remote_storage_target_key: Some(target_key.to_string()),
            application_config: Default::default(),
        },
    )
    .await
    .expect("remote policy should be created through service");

    policy_repo::find_by_id(state.writer_db(), created.id)
        .await
        .expect("created remote policy should be queryable")
}

async fn seed_remote_capabilities(
    state: &aster_drive::runtime::PrimaryAppState,
    remote_node_id: i64,
    capabilities: RemoteStorageCapabilities,
) {
    let mut remote_node: aster_drive::entities::managed_follower::ActiveModel =
        managed_follower_repo::find_by_id(state.writer_db(), remote_node_id)
            .await
            .expect("remote node should exist before seeding capabilities")
            .into();
    remote_node.last_capabilities =
        Set(serde_json::to_string(&capabilities).expect("remote capabilities should serialize"));
    remote_node.last_error = Set(String::new());
    remote_node.last_checked_at = Set(Some(Utc::now()));
    remote_node.updated_at = Set(Utc::now());
    remote_node
        .update(state.writer_db())
        .await
        .expect("remote node capabilities should update");
    state
        .driver_registry
        .reload_managed_followers(state.writer_db())
        .await
        .expect("driver registry should reload seeded remote capabilities");
    state.driver_registry.invalidate_all();
}

async fn set_policy_max_file_size(
    state: &aster_drive::runtime::PrimaryAppState,
    policy: &storage_policy::Model,
    max_file_size: i64,
) {
    let mut active: storage_policy::ActiveModel = policy.clone().into();
    active.max_file_size = Set(max_file_size);
    active.updated_at = Set(Utc::now());
    active
        .update(state.writer_db())
        .await
        .expect("policy max_file_size should update");
    state
        .policy_snapshot
        .reload(state.writer_db())
        .await
        .expect("policy snapshot should reload after updating max_file_size");
    state.driver_registry.invalidate(policy.id);
}

async fn wait_for_remote_probe(
    state: &aster_drive::runtime::PrimaryAppState,
    node_id: i64,
) -> remote_node::RemoteNodeInfo {
    mark_remote_node_enrollment_completed(state, node_id).await;
    for attempt in 0..20 {
        match remote_node::test_connection(state, node_id).await {
            Ok(info) => return info,
            Err(error) if attempt < 19 => {
                tracing::debug!(attempt, node_id, "remote probe not ready yet: {error}");
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(error) => panic!("remote probe should eventually succeed: {error}"),
        }
    }

    unreachable!("remote probe retry loop should return or panic")
}

async fn wait_for_tunnel_error_persisted(
    state: &aster_drive::runtime::PrimaryAppState,
    node_id: i64,
    expected: &str,
) {
    for attempt in 0..20 {
        let remote_node = managed_follower_repo::find_by_id(state.writer_db(), node_id)
            .await
            .expect("remote node should be queryable while waiting for tunnel error");
        if remote_node.tunnel_last_error.contains(expected) {
            return;
        }
        if attempt < 19 {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    let remote_node = managed_follower_repo::find_by_id(state.writer_db(), node_id)
        .await
        .expect("remote node should be queryable after waiting for tunnel error");
    panic!(
        "expected persisted tunnel error to contain '{expected}', got '{}'",
        remote_node.tunnel_last_error
    );
}

async fn wait_for_stream_lane(
    state: &aster_drive::runtime::PrimaryAppState,
    node: &aster_drive::entities::managed_follower::Model,
) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        let wait = state
            .remote_protocol
            .tunnel_registry()
            .wait_for_stream_lane(node);
        if tokio::time::timeout(remaining, wait)
            .await
            .is_ok_and(|result| result.is_ok())
        {
            return;
        }
    }
    panic!(
        "expected reverse tunnel stream lane for remote node #{} to become available",
        node.id
    );
}

async fn mark_remote_node_enrollment_completed(
    state: &aster_drive::runtime::PrimaryAppState,
    node_id: i64,
) {
    if follower_enrollment_session_repo::has_completed_for_managed_follower(
        state.writer_db(),
        node_id,
    )
    .await
    .expect("remote node enrollment completion check should succeed")
    {
        return;
    }

    let now = Utc::now();
    follower_enrollment_session_repo::create(
        state.writer_db(),
        follower_enrollment_session::ActiveModel {
            managed_follower_id: Set(node_id),
            token_hash: Set(format!("test-token-{node_id}-{}", uuid::Uuid::new_v4())),
            ack_token_hash: Set(format!("test-ack-token-{node_id}-{}", uuid::Uuid::new_v4())),
            expires_at: Set(now + ChronoDuration::minutes(30)),
            redeemed_at: Set(Some(now)),
            acked_at: Set(Some(now)),
            invalidated_at: Set(None),
            created_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .expect("remote node enrollment should be marked completed");
}

async fn create_managed_local_ingress_for_binding(
    provider_state: &aster_drive::runtime::PrimaryAppState,
    access_key: &str,
    base_path: &str,
) {
    let binding = master_binding_repo::find_by_access_key(provider_state.writer_db(), access_key)
        .await
        .expect("provider master binding lookup should succeed")
        .expect("provider master binding should exist");
    storage_target::create(
        &provider_state.follower_view(),
        &binding,
        RemoteCreateStorageTargetRequest::Local(RemoteCreateLocalStorageTargetRequest {
            name: format!("Managed {base_path}"),
            base_path: base_path.to_string(),
            is_default: true,
        }),
    )
    .await
    .expect("provider managed remote storage target should be created");
}

async fn setup_reverse_tunnel_ingress_profile_target(
    transport_mode: RemoteNodeTransportMode,
) -> (
    aster_drive::runtime::PrimaryAppState,
    TestHttpServer,
    aster_drive::runtime::PrimaryAppState,
    aster_drive::entities::managed_follower::Model,
    String,
    CancellationToken,
    tokio::task::JoinHandle<()>,
) {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;
    let provider_server = spawn_internal_storage_server(provider_state.follower_view()).await;

    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: format!("managed-ingress-{transport_mode:?}").to_lowercase(),
            base_url: String::new(),
            transport_mode,
            is_enabled: true,
        },
    )
    .await
    .expect("reverse tunnel ingress target node should be created without base_url");
    let consumer_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), consumer_node.id)
            .await
            .expect("reverse tunnel ingress target node should be queryable");

    let (provider_binding, _) = master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "reverse-managed-ingress".to_string(),
            master_url: "http://master.example.com".to_string(),
            access_key: consumer_node_model.access_key.clone(),
            secret_key: consumer_node_model.secret_key.clone(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider binding should be created for reverse managed ingress");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    mark_remote_node_enrollment_completed(&consumer_state, consumer_node.id).await;
    seed_remote_capabilities(
        &consumer_state,
        consumer_node.id,
        RemoteStorageCapabilities::current()
            .with_remote_storage_target_driver_types(vec![DriverType::Local, DriverType::S3]),
    )
    .await;

    let (tunnel_shutdown, tunnel_handle) = start_test_reverse_tunnel_worker(
        consumer_state.clone(),
        consumer_node_model.clone(),
        provider_server.base_url.clone(),
    )
    .await;

    let listed = storage_target::list_remote(&consumer_state, consumer_node.id)
        .await
        .expect("reverse tunnel remote storage target list should use tunnel transport");
    assert!(
        listed.is_empty(),
        "fresh reverse tunnel managed ingress target should start without profiles"
    );

    assert_eq!(provider_binding.access_key, consumer_node_model.access_key);

    (
        provider_state,
        provider_server,
        consumer_state,
        consumer_node_model,
        provider_binding.storage_namespace,
        tunnel_shutdown,
        tunnel_handle,
    )
}

#[actix_web::test]
async fn test_remote_ingress_profiles_use_reverse_tunnel_without_base_url() {
    let (
        provider_state,
        provider_server,
        consumer_state,
        consumer_node_model,
        storage_namespace,
        tunnel_shutdown,
        tunnel_handle,
    ) = setup_reverse_tunnel_ingress_profile_target(RemoteNodeTransportMode::ReverseTunnel).await;

    let created = storage_target::create_remote(
        &consumer_state,
        consumer_node_model.id,
        RemoteCreateStorageTargetRequest::Local(RemoteCreateLocalStorageTargetRequest {
            name: "Reverse Landing".to_string(),
            base_path: "reverse-landing".to_string(),
            is_default: true,
        }),
    )
    .await
    .expect("reverse tunnel remote storage target create should not require base_url");
    assert_eq!(created.name, "Reverse Landing");
    assert!(created.is_default);

    let listed = storage_target::list_remote(&consumer_state, consumer_node_model.id)
        .await
        .expect("reverse tunnel remote storage target list should not require base_url");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].target_key, created.target_key);

    let updated = storage_target::update_remote(
        &consumer_state,
        consumer_node_model.id,
        &created.target_key,
        RemoteUpdateStorageTargetRequest {
            name: Some("Reverse Landing Updated".to_string()),
            base_path: Some("reverse-landing-updated".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("reverse tunnel remote storage target update should not require base_url");
    assert_eq!(updated.name, "Reverse Landing Updated");
    assert_eq!(updated.base_path, "reverse-landing-updated");

    let client = RemoteStorageClient::new(
        &provider_server.base_url,
        &consumer_node_model.access_key,
        &consumer_node_model.secret_key,
    )
    .expect("managed ingress direct client should build for verification");
    client
        .put_bytes("reverse-managed-ingress.bin", b"reverse managed ingress")
        .await
        .expect("updated reverse tunnel remote storage target should accept remote writes");
    let stored_path = Path::new(
        &provider_state
            .config
            .server
            .follower
            .remote_storage_target_local_root,
    )
    .join("reverse-landing-updated")
    .join(storage_namespace)
    .join("reverse-managed-ingress.bin");
    assert_eq!(
        tokio::fs::read(&stored_path)
            .await
            .expect("reverse tunnel managed ingress payload should be stored"),
        b"reverse managed ingress"
    );

    storage_target::delete_remote(&consumer_state, consumer_node_model.id, &created.target_key)
        .await
        .expect("reverse tunnel remote storage target delete should not require base_url");
    let listed_after_delete = storage_target::list_remote(&consumer_state, consumer_node_model.id)
        .await
        .expect("reverse tunnel remote storage target list after delete should succeed");
    assert!(listed_after_delete.is_empty());

    stop_test_reverse_tunnel_worker(tunnel_shutdown, tunnel_handle).await;
    provider_server.stop().await;
}

#[actix_web::test]
async fn test_remote_ingress_profiles_auto_empty_base_url_uses_reverse_tunnel() {
    let (
        _provider_state,
        provider_server,
        consumer_state,
        consumer_node_model,
        _storage_namespace,
        tunnel_shutdown,
        tunnel_handle,
    ) = setup_reverse_tunnel_ingress_profile_target(RemoteNodeTransportMode::Auto).await;

    let profile = storage_target::create_remote(
        &consumer_state,
        consumer_node_model.id,
        RemoteCreateStorageTargetRequest::Local(RemoteCreateLocalStorageTargetRequest {
            name: "Auto Tunnel Landing".to_string(),
            base_path: "auto-tunnel-landing".to_string(),
            is_default: true,
        }),
    )
    .await
    .expect("auto remote storage target create should use reverse tunnel when base_url is empty");

    let listed = storage_target::list_remote(&consumer_state, consumer_node_model.id)
        .await
        .expect("auto remote storage target list should use reverse tunnel when base_url is empty");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].target_key, profile.target_key);

    stop_test_reverse_tunnel_worker(tunnel_shutdown, tunnel_handle).await;
    provider_server.stop().await;
}

#[tokio::test]
async fn test_remote_ingress_profile_create_rejects_driver_missing_from_capabilities() {
    let consumer_state = common::setup().await;
    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "managed-ingress-capability-create-node".to_string(),
            base_url: "http://127.0.0.1:9".to_string(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("consumer remote node should be created");
    mark_remote_node_enrollment_completed(&consumer_state, consumer_node.id).await;
    seed_remote_capabilities(
        &consumer_state,
        consumer_node.id,
        RemoteStorageCapabilities::current()
            .with_remote_storage_target_driver_types(vec![DriverType::Local]),
    )
    .await;

    let error = storage_target::create_remote(
        &consumer_state,
        consumer_node.id,
        RemoteCreateStorageTargetRequest::S3(RemoteCreateS3StorageTargetRequest {
            name: "Unsupported S3".to_string(),
            endpoint: "https://s3.example.com".to_string(),
            bucket: "bucket".to_string(),
            access_key: "access".to_string(),
            secret_key: "secret".to_string(),
            base_path: "unsupported-s3".to_string(),
            is_default: true,
        }),
    )
    .await
    .expect_err("primary should reject S3 when remote capabilities only declare local ingress");

    assert_eq!(
        error.api_error_code(),
        ApiErrorCode::ManagedIngressDriverUnsupported
    );
    assert!(
        error
            .message()
            .contains("does not declare remote storage target support for the s3 driver"),
        "unexpected error message: {}",
        error.message()
    );
}

#[tokio::test]
async fn test_remote_ingress_profile_update_rejects_driver_missing_from_capabilities() {
    let consumer_state = common::setup().await;
    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "managed-ingress-capability-update-node".to_string(),
            base_url: "http://127.0.0.1:9".to_string(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("consumer remote node should be created");
    mark_remote_node_enrollment_completed(&consumer_state, consumer_node.id).await;
    seed_remote_capabilities(
        &consumer_state,
        consumer_node.id,
        RemoteStorageCapabilities::current()
            .with_remote_storage_target_driver_types(vec![DriverType::Local]),
    )
    .await;

    let error = storage_target::update_remote(
        &consumer_state,
        consumer_node.id,
        "profile-a",
        RemoteUpdateStorageTargetRequest {
            driver_type: Some(DriverType::S3),
            endpoint: Some("https://s3.example.com".to_string()),
            bucket: Some("bucket".to_string()),
            access_key: Some("access".to_string()),
            secret_key: Some("secret".to_string()),
            base_path: Some("unsupported-s3".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect_err(
        "primary should reject update to S3 when remote capabilities only declare local ingress",
    );

    assert_eq!(
        error.api_error_code(),
        ApiErrorCode::ManagedIngressDriverUnsupported
    );
    assert!(
        error
            .message()
            .contains("does not declare remote storage target support for the s3 driver"),
        "unexpected error message: {}",
        error.message()
    );
}

#[tokio::test]
async fn test_remote_ingress_profile_update_without_driver_change_keeps_remote_authoritative() {
    let consumer_state = common::setup().await;
    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "managed-ingress-capability-no-driver-update-node".to_string(),
            base_url: "http://127.0.0.1:9".to_string(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("consumer remote node should be created");
    mark_remote_node_enrollment_completed(&consumer_state, consumer_node.id).await;
    seed_remote_capabilities(
        &consumer_state,
        consumer_node.id,
        RemoteStorageCapabilities::current().with_remote_storage_target_driver_types(vec![]),
    )
    .await;

    let error = storage_target::update_remote(
        &consumer_state,
        consumer_node.id,
        "profile-a",
        RemoteUpdateStorageTargetRequest {
            name: Some("Rename Only".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect_err("rename-only update should still be sent to the remote follower");

    assert_ne!(
        error.api_error_code(),
        ApiErrorCode::ManagedIngressDriverUnsupported
    );
    assert!(
        error.message().contains("remote storage request failed"),
        "unexpected error message: {}",
        error.message()
    );
}

#[actix_web::test]
async fn test_remote_ingress_profile_driver_descriptors_follow_remote_capabilities() {
    let consumer_state = common::setup().await;
    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "managed-ingress-driver-descriptor-node".to_string(),
            base_url: "http://127.0.0.1:9".to_string(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("consumer remote node should be created");
    mark_remote_node_enrollment_completed(&consumer_state, consumer_node.id).await;
    seed_remote_capabilities(
        &consumer_state,
        consumer_node.id,
        RemoteStorageCapabilities::current()
            .with_remote_storage_target_driver_types(vec![DriverType::Local]),
    )
    .await;

    let app = create_test_app!(consumer_state.clone());
    let (token, _) = register_and_login!(app);
    for path in [
        format!(
            "/api/v1/admin/remote-nodes/{}/storage-target-drivers",
            consumer_node.id
        ),
        format!(
            "/api/v1/admin/remote-nodes/{}/ingress-profile-drivers",
            consumer_node.id
        ),
    ] {
        let req = test::TestRequest::get()
            .uri(&path)
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
        let body: serde_json::Value = test::read_body_json(resp).await;
        let descriptors = body["data"]
            .as_array()
            .expect("driver descriptors response should contain data array");
        assert_eq!(descriptors.len(), 1);
        assert_eq!(descriptors[0]["driver_type"], "local");
        assert_eq!(
            descriptors[0]["fields"]
                .as_array()
                .expect("local descriptor fields should be an array")
                .iter()
                .map(|field| field["name"].as_str().unwrap_or_default())
                .collect::<Vec<_>>(),
            vec!["base_path", "is_default"]
        );
    }
}

async fn write_temp_upload_file(
    state: &aster_drive::runtime::PrimaryAppState,
    name: &str,
    data: &[u8],
) -> PathBuf {
    let path = Path::new(&state.config.server.temp_dir).join(name);
    tokio::fs::create_dir_all(&state.config.server.temp_dir)
        .await
        .expect("test temp dir should exist");
    tokio::fs::write(&path, data)
        .await
        .expect("test temp upload file should be written");
    path
}

fn build_test_png() -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(4, 4, image::Rgba([255, 0, 0, 255]));
    let mut cursor = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut cursor, image::ImageFormat::Png)
        .expect("test png should encode");
    cursor.into_inner()
}

async fn collect_download_body(
    outcome: aster_drive::services::files::file::DownloadOutcome,
) -> Vec<u8> {
    match outcome {
        aster_drive::services::files::file::DownloadOutcome::Stream(streamed) => streamed
            .body
            .try_fold(Vec::new(), |mut acc, chunk| async move {
                acc.extend_from_slice(&chunk);
                Ok(acc)
            })
            .await
            .expect("download stream should succeed"),
        other => panic!("expected streaming remote download, got {other:?}"),
    }
}

async fn put_presigned_bytes(url: &str, data: Vec<u8>) -> reqwest::Response {
    reqwest::Client::new()
        .put(url)
        .body(data)
        .send()
        .await
        .expect("presigned upload request should succeed")
}

async fn send_raw_http_request(base_url: &str, request: &[u8]) -> RawHttpResponse {
    let parsed = reqwest::Url::parse(base_url).expect("base URL should parse");
    let host = parsed.host_str().expect("base URL should contain host");
    let port = parsed
        .port_or_known_default()
        .expect("base URL should contain port");
    let mut stream = TcpStream::connect((host, port))
        .await
        .expect("raw HTTP test stream should connect");
    stream
        .write_all(request)
        .await
        .expect("raw HTTP request should be written");
    stream
        .shutdown()
        .await
        .expect("raw HTTP request stream should shutdown write half");

    let mut raw_response = Vec::new();
    stream
        .read_to_end(&mut raw_response)
        .await
        .expect("raw HTTP response should be readable");
    parse_raw_http_response(&raw_response)
}

fn parse_raw_http_response(raw: &[u8]) -> RawHttpResponse {
    let header_end = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("raw HTTP response should contain header terminator");
    let header_text =
        std::str::from_utf8(&raw[..header_end]).expect("raw HTTP response headers should be utf-8");
    let mut lines = header_text.split("\r\n");
    let status_line = lines
        .next()
        .expect("raw HTTP response should contain status line");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .expect("raw HTTP status line should contain status code")
        .parse::<u16>()
        .expect("raw HTTP status code should parse");

    let mut headers = std::collections::HashMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    let body_start = header_end + 4;
    let body_len = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or_else(|| raw.len().saturating_sub(body_start));
    let body_end = body_start.saturating_add(body_len).min(raw.len());

    RawHttpResponse {
        status,
        headers,
        body: raw[body_start..body_end].to_vec(),
        trailing: raw[body_end..].to_vec(),
    }
}

fn build_multipart_payload(filename: &str, data: &[u8]) -> (String, Vec<u8>) {
    let boundary = format!("----AsterRemoteBoundary{}", uuid::Uuid::new_v4().simple());
    let mut payload = Vec::new();
    payload.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    payload.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n")
            .as_bytes(),
    );
    payload.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
    payload.extend_from_slice(data);
    payload.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (boundary, payload)
}

fn snapshot_dir_tree(path: &Path) -> std::io::Result<std::collections::BTreeSet<String>> {
    fn walk(
        root: &Path,
        current: &Path,
        entries: &mut std::collections::BTreeSet<String>,
    ) -> std::io::Result<()> {
        for entry in std::fs::read_dir(current)? {
            let entry = entry?;
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                entries.insert(format!("{relative}/"));
                walk(root, &path, entries)?;
            } else {
                entries.insert(relative);
            }
        }
        Ok(())
    }

    let mut entries = std::collections::BTreeSet::new();
    if !path.exists() {
        return Ok(entries);
    }
    walk(path, path, &mut entries)?;
    Ok(entries)
}

fn snapshot_temp_roots(
    roots: &[String],
) -> std::io::Result<std::collections::BTreeMap<String, std::collections::BTreeSet<String>>> {
    let mut snapshots = std::collections::BTreeMap::new();
    for root in roots {
        snapshots.insert(root.clone(), snapshot_dir_tree(Path::new(root))?);
    }
    Ok(snapshots)
}

fn provider_object_path(
    ingress_base_path: &str,
    storage_namespace: &str,
    remote_base_path: &str,
    storage_path: &str,
) -> PathBuf {
    let mut relative = PathBuf::from(storage_namespace.trim_matches('/'));
    if !remote_base_path.trim_matches('/').is_empty() {
        relative.push(remote_base_path.trim_matches('/'));
    }
    relative.push(storage_path.trim_start_matches('/'));
    Path::new(ingress_base_path).join(relative)
}

fn managed_ingress_object_path(
    provider_state: &aster_drive::runtime::PrimaryAppState,
    profile_base_path: &str,
    storage_namespace: &str,
    remote_base_path: &str,
    storage_path: &str,
) -> PathBuf {
    let ingress_base_path = Path::new(
        &provider_state
            .config
            .server
            .follower
            .remote_storage_target_local_root,
    )
    .join(profile_base_path);
    provider_object_path(
        ingress_base_path
            .to_str()
            .expect("managed ingress base path should be valid utf-8"),
        storage_namespace,
        remote_base_path,
        storage_path,
    )
}

async fn latest_audit_log(
    db: &sea_orm::DatabaseConnection,
    action: aster_drive::types::AuditAction,
) -> aster_drive::entities::audit_log::Model {
    aster_drive::entities::audit_log::Entity::find()
        .filter(aster_drive::entities::audit_log::Column::Action.eq(action))
        .order_by_desc(aster_drive::entities::audit_log::Column::Id)
        .one(db)
        .await
        .expect("audit log query should succeed")
        .expect("audit log entry should exist")
}

fn audit_details(entry: &aster_drive::entities::audit_log::Model) -> serde_json::Value {
    serde_json::from_str(
        entry
            .details
            .as_deref()
            .expect("audit details should exist"),
    )
    .expect("audit details should parse")
}

async fn remote_driver_for_policy_connection_test_objects(
    provider_state: &aster_drive::runtime::PrimaryAppState,
    profile_base_path: &str,
    storage_namespace: &str,
    remote_base_path: &str,
) -> Vec<String> {
    let base_dir = managed_ingress_object_path(
        provider_state,
        profile_base_path,
        storage_namespace,
        remote_base_path,
        "",
    );
    snapshot_dir_tree(&base_dir)
        .expect("managed ingress test object directory should be readable")
        .into_iter()
        .collect()
}

fn path_and_query_from_url(url: &str) -> String {
    let parsed = reqwest::Url::parse(url).expect("test URL should parse");
    match parsed.query() {
        Some(query) => format!("{}?{query}", parsed.path()),
        None => parsed.path().to_string(),
    }
}

fn rewrite_path_query_param(path_and_query: &str, key: &str, value: Option<&str>) -> String {
    let mut parsed = reqwest::Url::parse(&format!("http://example.invalid{path_and_query}"))
        .expect("test path and query should parse");
    let existing_pairs: Vec<(String, String)> = parsed
        .query_pairs()
        .into_owned()
        .filter(|(existing_key, _)| existing_key != key)
        .collect();
    parsed.set_query(None);
    {
        let mut query = parsed.query_pairs_mut();
        for (existing_key, existing_value) in existing_pairs {
            query.append_pair(&existing_key, &existing_value);
        }
        if let Some(value) = value {
            query.append_pair(key, value);
        }
    }
    path_and_query_from_url(parsed.as_str())
}

struct BrowserPresignedCorsFixture {
    provider_state: aster_drive::runtime::PrimaryAppState,
    presigned_path: String,
}

async fn setup_browser_presigned_cors_fixture(
    label: &str,
    master_url: &str,
) -> BrowserPresignedCorsFixture {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;

    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: format!("{label}-node"),
            base_url: "http://provider.example.com".to_string(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("consumer remote node should be created");
    let consumer_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), consumer_node.id)
            .await
            .expect("consumer remote node should be queryable");

    master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: format!("{label}-binding"),
            master_url: master_url.to_string(),
            access_key: consumer_node_model.access_key.clone(),
            secret_key: consumer_node_model.secret_key.clone(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider master binding should be created");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(
        &provider_state,
        &consumer_node_model.access_key,
        &consumer_node_model.access_key,
    )
    .await;
    seed_remote_capabilities(
        &consumer_state,
        consumer_node.id,
        RemoteStorageCapabilities::current(),
    )
    .await;

    let remote_policy = create_remote_policy_with_options(
        &consumer_state,
        consumer_node.id,
        &format!("Remote Presigned {label} Policy"),
        &format!("{label}-base"),
        StoragePolicyOptions {
            remote_upload_strategy: Some(RemoteUploadStrategy::Presigned),
            ..Default::default()
        },
        1024,
    )
    .await;

    let app = create_test_app!(consumer_state.clone());
    let _ = register_and_login!(app);
    let user = user_repo::find_by_username(consumer_state.writer_db(), "testuser")
        .await
        .expect("test user lookup should succeed")
        .expect("test user should exist");
    let folder = folder::create(&consumer_state, user.id, &format!("{label}-folder"), None)
        .await
        .expect("remote folder should be created");
    common::bind_policy_to_folder(&consumer_state, folder.id, remote_policy.id).await;

    let init = upload_service::init_upload(
        &consumer_state,
        user.id,
        &format!("{label}.bin"),
        32,
        Some(folder.id),
        None,
    )
    .await
    .expect("remote presigned upload should initialize");

    BrowserPresignedCorsFixture {
        provider_state,
        presigned_path: path_and_query_from_url(
            &init
                .presigned_url
                .expect("presigned mode should return presigned_url"),
        ),
    }
}

#[tokio::test]
async fn test_remote_storage_target_handles_remote_writes_without_legacy_binding_policy() {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;
    let provider_server = spawn_internal_storage_server(provider_state.follower_view()).await;
    let managed_root = PathBuf::from(
        &provider_state
            .config
            .server
            .follower
            .remote_storage_target_local_root,
    );
    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "managed-ingress-node".to_string(),
            base_url: provider_server.base_url.clone(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("consumer remote node should be created");
    let consumer_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), consumer_node.id)
            .await
            .expect("consumer remote node should be queryable");

    let (provider_binding, _) = master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "managed-ingress-binding".to_string(),
            master_url: "http://master.example.com".to_string(),
            access_key: consumer_node_model.access_key.clone(),
            secret_key: consumer_node_model.secret_key.clone(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider binding should be created");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(
        &provider_state,
        &consumer_node_model.access_key,
        &consumer_node_model.access_key,
    )
    .await;

    wait_for_remote_probe(&consumer_state, consumer_node.id).await;

    let profile = storage_target::create_remote(
        &consumer_state,
        consumer_node.id,
        RemoteCreateStorageTargetRequest::Local(RemoteCreateLocalStorageTargetRequest {
            name: "Managed Local".to_string(),
            base_path: "managed-a".to_string(),
            is_default: true,
        }),
    )
    .await
    .expect("managed remote storage target should be created through primary");
    assert!(profile.is_default);
    assert_eq!(profile.applied_revision, profile.desired_revision);

    let client = RemoteStorageClient::new(
        &provider_server.base_url,
        &consumer_node_model.access_key,
        &consumer_node_model.secret_key,
    )
    .expect("managed ingress client should build");
    client
        .put_bytes("managed-ingress.bin", b"managed ingress payload")
        .await
        .expect("managed ingress write should not depend on legacy binding policy fields");

    let stored_path = Path::new(
        &provider_state
            .config
            .server
            .follower
            .remote_storage_target_local_root,
    )
    .join("managed-a")
    .join(provider_binding.storage_namespace)
    .join("managed-ingress.bin");
    let stored = tokio::fs::read(&stored_path)
        .await
        .expect("managed ingress payload should land under managed ingress root");
    assert_eq!(stored, b"managed ingress payload");

    provider_server.stop().await;
    let _ = tokio::fs::remove_dir_all(managed_root.join("managed-a")).await;
    let _ = tokio::fs::remove_dir(&managed_root).await;
}

#[tokio::test]
async fn test_remote_policy_uses_selected_remote_storage_target_key() {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;
    let provider_server = spawn_internal_storage_server(provider_state.follower_view()).await;
    let managed_root = PathBuf::from(
        &provider_state
            .config
            .server
            .follower
            .remote_storage_target_local_root,
    );
    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "selected-target-node".to_string(),
            base_url: provider_server.base_url.clone(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("consumer remote node should be created");
    let consumer_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), consumer_node.id)
            .await
            .expect("consumer remote node should be queryable");

    let (provider_binding, _) = master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "selected-target-binding".to_string(),
            master_url: "http://master.example.com".to_string(),
            access_key: consumer_node_model.access_key.clone(),
            secret_key: consumer_node_model.secret_key.clone(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider binding should be created");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(
        &provider_state,
        &consumer_node_model.access_key,
        "default-target",
    )
    .await;
    wait_for_remote_probe(&consumer_state, consumer_node.id).await;

    let selected_target = storage_target::create_remote(
        &consumer_state,
        consumer_node.id,
        RemoteCreateStorageTargetRequest::Local(RemoteCreateLocalStorageTargetRequest {
            name: "Selected Target".to_string(),
            base_path: "selected-target".to_string(),
            is_default: false,
        }),
    )
    .await
    .expect("selected remote storage target should be created through primary");
    assert!(!selected_target.is_default);

    let remote_policy = create_remote_policy_via_service_with_options(
        &consumer_state,
        consumer_node.id,
        "Selected Target Policy",
        "policy-prefix",
        &selected_target.target_key,
        StoragePolicyOptions::default(),
        5_242_880,
    )
    .await;
    assert_eq!(
        remote_policy.remote_storage_target_key.as_deref(),
        Some(selected_target.target_key.as_str())
    );

    let driver = consumer_state
        .driver_registry()
        .get_driver(&remote_policy)
        .expect("remote policy driver should resolve");
    driver
        .put("selected.bin", b"selected target payload")
        .await
        .expect("remote policy should write through selected target");

    let selected_path = managed_ingress_object_path(
        &provider_state,
        "selected-target",
        &provider_binding.storage_namespace,
        "policy-prefix",
        "selected.bin",
    );
    let default_path = managed_ingress_object_path(
        &provider_state,
        "default-target",
        &provider_binding.storage_namespace,
        "policy-prefix",
        "selected.bin",
    );
    assert_eq!(
        tokio::fs::read(&selected_path)
            .await
            .expect("selected target object should exist"),
        b"selected target payload"
    );
    assert!(
        tokio::fs::metadata(&default_path).await.is_err(),
        "selected policy object must not be written to the binding default target"
    );

    provider_server.stop().await;
    let _ = tokio::fs::remove_dir_all(managed_root.join("selected-target")).await;
    let _ = tokio::fs::remove_dir_all(managed_root.join("default-target")).await;
    let _ = tokio::fs::remove_dir(&managed_root).await;
}

#[tokio::test]
async fn test_follower_internal_storage_records_object_audit_logs() {
    let provider_state = common::setup().await;
    let provider_server = spawn_internal_storage_server(provider_state.follower_view()).await;
    let (binding, _) = master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "object-audit-binding".to_string(),
            master_url: "http://master.example.com".to_string(),
            access_key: "object-audit-ak".to_string(),
            secret_key: "object-audit-sk".to_string(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider binding should be created");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(&provider_state, &binding.access_key, "object-audit")
        .await;

    let client = RemoteStorageClient::new(
        &provider_server.base_url,
        &binding.access_key,
        &binding.secret_key,
    )
    .expect("remote storage client should build");
    client
        .put_bytes("audit-object.bin", b"audit payload")
        .await
        .expect("object write should succeed");
    let write_entry = latest_audit_log(
        provider_state.writer_db(),
        aster_drive::types::AuditAction::FollowerObjectWrite,
    )
    .await;
    assert_eq!(write_entry.user_id, 0);
    assert_eq!(write_entry.entity_name.as_deref(), Some("audit-object.bin"));
    let write_details = audit_details(&write_entry);
    assert_eq!(write_details["binding_id"], binding.id);
    assert_eq!(write_details["object_key"], "audit-object.bin");
    assert_eq!(write_details["size"], 13);
    assert!(
        write_details["storage_path"]
            .as_str()
            .expect("storage path should be string")
            .ends_with("/audit-object.bin")
    );

    let downloaded = client
        .get_bytes("audit-object.bin")
        .await
        .expect("object read should succeed");
    assert_eq!(downloaded, b"audit payload");
    let read_entry = latest_audit_log(
        provider_state.writer_db(),
        aster_drive::types::AuditAction::FollowerObjectRead,
    )
    .await;
    assert_eq!(read_entry.entity_name.as_deref(), Some("audit-object.bin"));
    let read_details = audit_details(&read_entry);
    assert_eq!(read_details["binding_id"], binding.id);
    assert_eq!(read_details["object_key"], "audit-object.bin");
    assert_eq!(read_details["size"], 13);
    assert_eq!(read_details["partial"], false);

    client
        .delete("audit-object.bin")
        .await
        .expect("object delete should succeed");
    let delete_entry = latest_audit_log(
        provider_state.writer_db(),
        aster_drive::types::AuditAction::FollowerObjectDelete,
    )
    .await;
    assert_eq!(
        delete_entry.entity_name.as_deref(),
        Some("audit-object.bin")
    );
    let delete_details = audit_details(&delete_entry);
    assert_eq!(delete_details["binding_id"], binding.id);
    assert_eq!(delete_details["object_key"], "audit-object.bin");

    client
        .put_bytes("audit-part-a.bin", b"abc")
        .await
        .expect("first compose part write should succeed");
    client
        .put_bytes("audit-part-b.bin", b"def")
        .await
        .expect("second compose part write should succeed");
    let composed = client
        .compose_objects(
            "audit-composed.bin",
            vec![
                "audit-part-a.bin".to_string(),
                "audit-part-b.bin".to_string(),
            ],
            6,
        )
        .await
        .expect("object compose should succeed");
    assert_eq!(composed.bytes_written, 6);
    let compose_entry = latest_audit_log(
        provider_state.writer_db(),
        aster_drive::types::AuditAction::FollowerObjectCompose,
    )
    .await;
    assert_eq!(
        compose_entry.entity_name.as_deref(),
        Some("audit-composed.bin")
    );
    let compose_details = audit_details(&compose_entry);
    assert_eq!(compose_details["binding_id"], binding.id);
    assert_eq!(compose_details["object_key"], "audit-composed.bin");
    assert_eq!(compose_details["size"], 6);
    assert_eq!(compose_details["bytes_written"], 6);
    assert_eq!(
        compose_details["parts"],
        serde_json::json!(["audit-part-a.bin", "audit-part-b.bin"])
    );
    assert!(
        compose_details["storage_path"]
            .as_str()
            .expect("compose storage path should be string")
            .ends_with("/audit-composed.bin")
    );

    provider_server.stop().await;
}

#[tokio::test]
async fn test_follower_internal_storage_records_binding_and_profile_audit_logs() {
    let provider_state = common::setup().await;
    let provider_server = spawn_internal_storage_server(provider_state.follower_view()).await;
    let (binding, _) = master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "profile-audit-binding".to_string(),
            master_url: "http://master.example.com".to_string(),
            access_key: "profile-audit-ak".to_string(),
            secret_key: "profile-audit-sk".to_string(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider binding should be created");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");

    let client = RemoteStorageClient::new(
        &provider_server.base_url,
        &binding.access_key,
        &binding.secret_key,
    )
    .expect("remote storage client should build");
    client
        .sync_binding(&RemoteBindingSyncRequest {
            name: "profile-audit-renamed".to_string(),
            is_enabled: false,
        })
        .await
        .expect("binding sync should succeed");
    let sync_entry = latest_audit_log(
        provider_state.writer_db(),
        aster_drive::types::AuditAction::FollowerBindingSync,
    )
    .await;
    assert_eq!(sync_entry.entity_id, Some(binding.id));
    assert_eq!(
        sync_entry.entity_name.as_deref(),
        Some("profile-audit-renamed")
    );
    let sync_details = audit_details(&sync_entry);
    assert_eq!(sync_details["binding_id"], binding.id);
    assert_eq!(sync_details["name"], "profile-audit-renamed");
    assert_eq!(sync_details["is_enabled"], false);
    assert!(sync_details.get("access_key").is_none());

    client
        .sync_binding(&RemoteBindingSyncRequest {
            name: "profile-audit-renamed".to_string(),
            is_enabled: true,
        })
        .await
        .expect("binding should be re-enabled for profile operations");

    let profile = client
        .create_storage_target(&RemoteCreateStorageTargetRequest::Local(
            RemoteCreateLocalStorageTargetRequest {
                name: "Profile Audit Local".to_string(),
                base_path: "profile-audit-a".to_string(),
                is_default: true,
            },
        ))
        .await
        .expect("remote storage target create should succeed");
    let create_entry = latest_audit_log(
        provider_state.writer_db(),
        aster_drive::types::AuditAction::FollowerIngressProfileCreate,
    )
    .await;
    assert_eq!(
        create_entry.entity_name.as_deref(),
        Some(profile.target_key.as_str())
    );
    let create_details = audit_details(&create_entry);
    assert_eq!(create_details["binding_id"], binding.id);
    assert_eq!(create_details["target_key"], profile.target_key);
    assert_eq!(create_details["driver_type"], "local");
    assert_eq!(create_details["is_default"], true);

    let updated = client
        .update_storage_target(
            &profile.target_key,
            &RemoteUpdateStorageTargetRequest {
                name: Some("Profile Audit Updated".to_string()),
                base_path: Some("profile-audit-b".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect("remote storage target update should succeed");
    let update_entry = latest_audit_log(
        provider_state.writer_db(),
        aster_drive::types::AuditAction::FollowerIngressProfileUpdate,
    )
    .await;
    assert_eq!(
        update_entry.entity_name.as_deref(),
        Some(updated.target_key.as_str())
    );
    let update_details = audit_details(&update_entry);
    assert_eq!(update_details["binding_id"], binding.id);
    assert_eq!(update_details["target_key"], updated.target_key);
    assert_eq!(update_details["driver_type"], "local");
    assert_eq!(update_details["is_default"], true);

    client
        .delete_storage_target(&profile.target_key)
        .await
        .expect("remote storage target delete should succeed");
    let delete_entry = latest_audit_log(
        provider_state.writer_db(),
        aster_drive::types::AuditAction::FollowerIngressProfileDelete,
    )
    .await;
    assert_eq!(
        delete_entry.entity_name.as_deref(),
        Some(profile.target_key.as_str())
    );
    let delete_details = audit_details(&delete_entry);
    assert_eq!(delete_details["binding_id"], binding.id);
    assert_eq!(delete_details["target_key"], profile.target_key);
    assert_eq!(delete_details["driver_type"], "local");
    assert_eq!(delete_details["is_default"], true);

    provider_server.stop().await;
}

#[tokio::test]
async fn test_remote_storage_target_api_isolates_multiple_primary_bindings() {
    let provider_state = common::setup().await;
    let provider_server = spawn_internal_storage_server(provider_state.follower_view()).await;

    let (binding_a, _) = master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "primary-a".to_string(),
            master_url: "http://primary-a.example.com".to_string(),
            access_key: "managed-ak-a".to_string(),
            secret_key: "managed-sk-a".to_string(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider binding a should be created");
    let (binding_b, _) = master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "primary-b".to_string(),
            master_url: "http://primary-b.example.com".to_string(),
            access_key: "managed-ak-b".to_string(),
            secret_key: "managed-sk-b".to_string(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider binding b should be created");
    assert_ne!(binding_a.storage_namespace, binding_b.storage_namespace);

    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");

    let client_a =
        RemoteStorageClient::new(&provider_server.base_url, "managed-ak-a", "managed-sk-a")
            .expect("remote storage client should build");
    let client_b =
        RemoteStorageClient::new(&provider_server.base_url, "managed-ak-b", "managed-sk-b")
            .expect("remote storage client should build");

    for (client, name) in [(&client_a, "Managed A"), (&client_b, "Managed B")] {
        let profile = client
            .create_storage_target(&RemoteCreateStorageTargetRequest::Local(
                RemoteCreateLocalStorageTargetRequest {
                    name: name.to_string(),
                    base_path: "shared-profile".to_string(),
                    is_default: true,
                },
            ))
            .await
            .expect("managed remote storage target should be created for its binding");
        assert!(profile.is_default);
        assert_eq!(profile.base_path, "shared-profile");
    }

    client_a
        .put_bytes("same.bin", b"payload-from-a")
        .await
        .expect("binding a should write object");
    client_b
        .put_bytes("same.bin", b"payload-from-b")
        .await
        .expect("binding b should write object");

    let path_a = managed_ingress_object_path(
        &provider_state,
        "shared-profile",
        &binding_a.storage_namespace,
        "",
        "same.bin",
    );
    let path_b = managed_ingress_object_path(
        &provider_state,
        "shared-profile",
        &binding_b.storage_namespace,
        "",
        "same.bin",
    );
    assert_eq!(
        tokio::fs::read(&path_a)
            .await
            .expect("binding a object should exist"),
        b"payload-from-a"
    );
    assert_eq!(
        tokio::fs::read(&path_b)
            .await
            .expect("binding b object should exist"),
        b"payload-from-b"
    );
    assert_eq!(
        client_a
            .get_bytes("same.bin")
            .await
            .expect("binding a should read its object"),
        b"payload-from-a"
    );
    assert_eq!(
        client_b
            .get_bytes("same.bin")
            .await
            .expect("binding b should read its object"),
        b"payload-from-b"
    );
    assert_eq!(
        client_a
            .list_paths(None)
            .await
            .expect("binding a list should succeed"),
        vec!["same.bin".to_string()]
    );
    assert_eq!(
        client_a
            .list_paths(Some("."))
            .await
            .expect("binding a root prefix list should succeed"),
        vec!["same.bin".to_string()]
    );
    assert_eq!(
        client_b
            .list_paths(None)
            .await
            .expect("binding b list should succeed"),
        vec!["same.bin".to_string()]
    );

    let profile_a = storage_target::create(
        &provider_state.follower_view(),
        &binding_a,
        RemoteCreateStorageTargetRequest::Local(RemoteCreateLocalStorageTargetRequest {
            name: "Second A".to_string(),
            base_path: "secondary-a".to_string(),
            is_default: false,
        }),
    )
    .await
    .expect("binding a should allow additional scoped profiles");
    let profiles_b = client_b
        .list_storage_targets()
        .await
        .expect("binding b profiles should list");
    assert_eq!(profile_a.base_path, "secondary-a");
    assert_eq!(profiles_b.len(), 1);
    assert_eq!(profiles_b[0].base_path, "shared-profile");

    provider_server.stop().await;
}

#[actix_web::test]
async fn test_internal_storage_presigned_put_rejects_payload_exceeding_ingress_limit() {
    let provider_state = common::setup().await;
    let provider_default_policy = provider_state
        .policy_snapshot
        .system_default_policy()
        .expect("provider default ingress policy should exist");
    set_policy_max_file_size(&provider_state, &provider_default_policy, 8).await;

    let access_key = "limit-access-key";
    let secret_key = "limit-secret-key";
    master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "limit-binding".to_string(),
            master_url: "http://master.example.com".to_string(),
            access_key: access_key.to_string(),
            secret_key: secret_key.to_string(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider binding should be created");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(&provider_state, access_key, access_key).await;

    let follower_app = test::init_service(
        App::new()
            .app_data(web::Data::new(provider_state.follower_view()))
            .service(
                web::scope("/api/v1").service(aster_drive::api::routes::internal_storage::routes()),
            ),
    )
    .await;

    let path = "/api/v1/internal/storage/objects/too-large.bin";
    let request_target = format!("{path}?max_file_size=8");
    let expires_at = Utc::now().timestamp() + 300;
    let signature =
        sign_presigned_request(secret_key, "PUT", &request_target, access_key, expires_at);
    let req = test::TestRequest::put()
        .uri(&format!(
            "{request_target}&aster_access_key={access_key}&aster_expires={expires_at}&aster_signature={signature}"
        ))
        .insert_header((
            actix_web::http::header::CONTENT_TYPE,
            "application/octet-stream",
        ))
        .insert_header((actix_web::http::header::CONTENT_LENGTH, "16"))
        .set_payload(vec![b'x'; 16])
        .to_request();
    let resp = test::call_service(&follower_app, req).await;

    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "file.too_large");
    assert_eq!(body["msg"], "object size 16 exceeds limit 8");
}

#[actix_web::test]
async fn test_internal_storage_presigned_put_ignores_bytes_beyond_declared_content_length() {
    let provider_state = common::setup().await;

    let access_key = "declared-length-access-key";
    let secret_key = "declared-length-secret-key";
    master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "declared-length-binding".to_string(),
            master_url: "http://master.example.com".to_string(),
            access_key: access_key.to_string(),
            secret_key: secret_key.to_string(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider binding should be created");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(&provider_state, access_key, access_key).await;
    let binding = master_binding_repo::find_by_access_key(provider_state.writer_db(), access_key)
        .await
        .expect("provider binding lookup should succeed")
        .expect("provider binding should exist");

    let provider_server = spawn_internal_storage_server(provider_state.follower_view()).await;
    let parsed_server_url =
        reqwest::Url::parse(&provider_server.base_url).expect("provider base_url should parse");
    let host = parsed_server_url
        .host_str()
        .expect("provider base_url should contain host");
    let port = parsed_server_url
        .port_or_known_default()
        .expect("provider base_url should contain port");
    let object_key = "declared-length-only.bin";
    let path = format!("/api/v1/internal/storage/objects/{object_key}");
    let signed_path = format!("{path}?max_file_size=0");
    let expires_at = Utc::now().timestamp() + 300;
    let signature = sign_presigned_request(secret_key, "PUT", &signed_path, access_key, expires_at);
    let request_target = format!(
        "{signed_path}&aster_access_key={access_key}&aster_expires={expires_at}&aster_signature={signature}"
    );
    let mut request = format!(
        "PUT {request_target} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/octet-stream\r\nContent-Length: 4\r\nConnection: close\r\n\r\n"
    )
    .into_bytes();
    request.extend_from_slice(b"testignored-trailing-bytes");

    let response = send_raw_http_request(&provider_server.base_url, &request).await;
    let expected_etag = format!("\"{}\"", hex::encode(Sha256::digest(b"test")));
    assert_eq!(response.status, actix_web::http::StatusCode::OK.as_u16());
    assert_eq!(response.headers.get("etag"), Some(&expected_etag));
    let response_body: serde_json::Value = serde_json::from_slice(&response.body)
        .expect("raw HTTP success response body should be valid json");
    assert_eq!(response_body["code"], "success");
    assert!(
        response.trailing.is_empty(),
        "connection-close request should not emit a second HTTP response"
    );

    let storage_path = managed_ingress_object_path(
        &provider_state,
        access_key,
        &binding.storage_namespace,
        "",
        object_key,
    );
    let stored = tokio::fs::read(&storage_path)
        .await
        .expect("provider should store presigned upload object");
    assert_eq!(stored, b"test");
    let metadata = tokio::fs::metadata(&storage_path)
        .await
        .expect("provider object metadata should be readable");
    assert_eq!(metadata.len(), 4);

    provider_server.stop().await;
}

#[actix_web::test]
async fn test_internal_storage_compose_rejects_expected_size_exceeding_ingress_limit() {
    let provider_state = common::setup().await;
    let provider_default_policy = provider_state
        .policy_snapshot
        .system_default_policy()
        .expect("provider default ingress policy should exist");
    set_policy_max_file_size(&provider_state, &provider_default_policy, 8).await;

    let access_key = "compose-limit-access-key";
    let secret_key = "compose-limit-secret-key";
    master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "compose-limit-binding".to_string(),
            master_url: "http://master.example.com".to_string(),
            access_key: access_key.to_string(),
            secret_key: secret_key.to_string(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider binding should be created");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(&provider_state, access_key, access_key).await;

    let follower_app = test::init_service(
        App::new()
            .app_data(web::Data::new(provider_state.follower_view()))
            .service(
                web::scope("/api/v1").service(aster_drive::api::routes::internal_storage::routes()),
            ),
    )
    .await;

    let body = serde_json::to_vec(&RemoteStorageComposeRequest {
        target_key: "assembled.bin".to_string(),
        part_keys: vec!["part-1".to_string(), "part-2".to_string()],
        expected_size: 16,
    })
    .expect("compose request body should serialize");
    let path = "/api/v1/internal/storage/compose?max_file_size=8";
    let timestamp = Utc::now().timestamp();
    let nonce = "compose-limit-test";
    let signature = sign_internal_request(
        secret_key,
        "POST",
        path,
        timestamp,
        nonce,
        Some(u64::try_from(body.len()).expect("compose body length should fit u64")),
    );
    let req = test::TestRequest::post()
        .uri(path)
        .insert_header((actix_web::http::header::CONTENT_TYPE, "application/json"))
        .insert_header((
            actix_web::http::header::CONTENT_LENGTH,
            body.len().to_string(),
        ))
        .insert_header(("x-aster-access-key", access_key))
        .insert_header(("x-aster-timestamp", timestamp.to_string()))
        .insert_header(("x-aster-nonce", nonce))
        .insert_header(("x-aster-signature", signature))
        .set_payload(body)
        .to_request();
    let resp = test::call_service(&follower_app, req).await;

    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "file.too_large");
    assert_eq!(body["msg"], "composed object size 16 exceeds limit 8");
}

#[actix_web::test]
async fn test_remote_node_connection_failure_returns_error_and_persists_last_error() {
    let state = common::setup().await;
    let node = remote_node::create(
        &state,
        remote_node::CreateRemoteNodeInput {
            name: "broken-remote".to_string(),
            base_url: "http://127.0.0.1:9".to_string(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("broken remote node should be created");
    mark_remote_node_enrollment_completed(&state, node.id).await;

    let error = remote_node::test_connection(&state, node.id)
        .await
        .expect_err("connection test should surface probe failures");
    assert_eq!(error.code(), "E031");

    let stored = managed_follower_repo::find_by_id(state.writer_db(), node.id)
        .await
        .expect("remote node should still exist after failed probe");
    assert!(!stored.last_error.is_empty());
}

#[actix_web::test]
async fn test_remote_node_failed_probe_preserves_cached_capabilities() {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;
    let provider_server = spawn_internal_storage_server(provider_state.follower_view()).await;

    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "capability-cache-target".to_string(),
            base_url: provider_server.base_url.clone(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("consumer remote node should be created");
    let consumer_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), consumer_node.id)
            .await
            .expect("consumer remote node should be queryable");

    let (provider_binding, _) = master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "capability-cache-access".to_string(),
            master_url: "http://master.example.com".to_string(),
            access_key: consumer_node_model.access_key.clone(),
            secret_key: consumer_node_model.secret_key.clone(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider master binding should be created");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(
        &provider_state,
        &provider_binding.access_key,
        &provider_binding.access_key,
    )
    .await;

    wait_for_remote_probe(&consumer_state, consumer_node.id).await;
    let cached_capabilities =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), consumer_node.id)
            .await
            .expect("remote node should remain queryable after successful probe")
            .last_capabilities;
    assert_ne!(cached_capabilities, "{}");

    provider_server.stop().await;

    let error = remote_node::test_connection(&consumer_state, consumer_node.id)
        .await
        .expect_err("stopped provider should make probe fail");
    assert_eq!(error.code(), "E031");

    let stored = managed_follower_repo::find_by_id(consumer_state.writer_db(), consumer_node.id)
        .await
        .expect("remote node should remain queryable after failed probe");
    assert!(!stored.last_error.is_empty());
    assert_eq!(stored.last_capabilities, cached_capabilities);
}

#[actix_web::test]
async fn test_remote_node_probe_rejects_incompatible_protocol_version() {
    let state = common::setup().await;
    let capabilities_server = spawn_capabilities_server(serde_json::json!({
        "protocol_version": "v1",
        "min_supported_protocol_version": "v1",
        "supports_list": true,
        "supports_range_read": true,
        "supports_stream_upload": true,
    }))
    .await;
    let node = remote_node::create(
        &state,
        remote_node::CreateRemoteNodeInput {
            name: "old-protocol-target".to_string(),
            base_url: capabilities_server.base_url.clone(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("remote node should be created");
    mark_remote_node_enrollment_completed(&state, node.id).await;

    let error = remote_node::test_connection(&state, node.id)
        .await
        .expect_err("incompatible remote protocol should fail probe");
    assert_eq!(error.code(), "E031");
    assert_eq!(
        error.storage_error_kind(),
        Some(aster_drive::storage::StorageErrorKind::Misconfigured)
    );
    assert!(error.message().contains("protocol incompatible"));
    let expected_protocol_range = format!(
        "local supports {INTERNAL_STORAGE_MIN_SUPPORTED_PROTOCOL_VERSION_LABEL}-{INTERNAL_STORAGE_PROTOCOL_VERSION_LABEL}"
    );
    assert!(error.message().contains(&expected_protocol_range));

    let stored = managed_follower_repo::find_by_id(state.writer_db(), node.id)
        .await
        .expect("remote node should remain queryable");
    assert!(stored.last_error.contains("protocol incompatible"));
    assert_eq!(stored.last_capabilities, "{}");

    capabilities_server.stop().await;
}

#[actix_web::test]
async fn test_remote_node_probe_rejects_presigned_download_when_range_cors_missing() {
    let state = common::setup().await;
    let mut capabilities = RemoteStorageCapabilities::current();
    capabilities.browser_cors.allowed_headers = vec!["content-type".to_string()];
    capabilities.browser_cors.exposed_headers =
        vec!["Accept-Ranges".to_string(), "Content-Length".to_string()];
    let capabilities_server = spawn_capabilities_server(
        serde_json::to_value(&capabilities).expect("capabilities should serialize"),
    )
    .await;

    let node = remote_node::create(
        &state,
        remote_node::CreateRemoteNodeInput {
            name: "missing-range-cors-target".to_string(),
            base_url: capabilities_server.base_url.clone(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("remote node should be created");
    mark_remote_node_enrollment_completed(&state, node.id).await;
    create_remote_policy_with_options(
        &state,
        node.id,
        "Remote Presigned Download Needs Range CORS",
        "base",
        StoragePolicyOptions {
            remote_download_strategy: Some(RemoteDownloadStrategy::Presigned),
            ..Default::default()
        },
        5_242_880,
    )
    .await;

    let error = remote_node::test_connection(&state, node.id)
        .await
        .expect_err("missing Range/CORS contract should fail probe for presigned download policy");
    assert_eq!(error.code(), "E031");
    assert_eq!(
        error.storage_error_kind(),
        Some(aster_drive::storage::StorageErrorKind::Misconfigured)
    );
    assert!(
        error
            .message()
            .contains("browser CORS contract is incomplete")
    );
    assert!(error.message().contains("allowed_headers missing range"));
    assert!(
        error
            .message()
            .contains("exposed_headers missing Content-Range")
    );

    let stored = managed_follower_repo::find_by_id(state.writer_db(), node.id)
        .await
        .expect("remote node should remain queryable");
    assert!(
        stored
            .last_error
            .contains("browser CORS contract is incomplete")
    );
    let expected_protocol_marker =
        format!("\"protocol_version\":\"{INTERNAL_STORAGE_PROTOCOL_VERSION_LABEL}\"");
    assert!(stored.last_capabilities.contains(&expected_protocol_marker));

    capabilities_server.stop().await;
}

async fn create_internal_hmac_binding(
    provider_state: &aster_drive::runtime::PrimaryAppState,
    label: &str,
) -> (String, String) {
    let access_key = format!("{label}-access-key");
    let secret_key = format!("{label}-secret-key");
    master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: format!("{label}-binding"),
            master_url: "http://master.example.com".to_string(),
            access_key: access_key.clone(),
            secret_key: secret_key.clone(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider binding should be created");
    (access_key, secret_key)
}

async fn setup_internal_hmac_binding_state(
    label: &str,
) -> (aster_drive::runtime::PrimaryAppState, String, String) {
    let mut provider_state = common::setup().await;
    provider_state.cache = aster_drive::cache::create_cache(&aster_drive::config::CacheConfig {
        backend: "memory".to_string(),
        redis_url: String::new(),
        default_ttl: 60,
    })
    .await;
    let (access_key, secret_key) = create_internal_hmac_binding(&provider_state, label).await;
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    (provider_state, access_key, secret_key)
}

#[actix_web::test]
async fn test_internal_storage_capabilities_probe_does_not_require_ingress_profile() {
    let provider_state = common::setup().await;
    let access_key = "capabilities-no-ingress-access-key";
    let secret_key = "capabilities-no-ingress-secret-key";

    master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "capabilities-no-ingress-binding".to_string(),
            master_url: "http://master.example.com".to_string(),
            access_key: access_key.to_string(),
            secret_key: secret_key.to_string(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider binding should be created");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");

    let follower_app = test::init_service(
        App::new()
            .app_data(web::Data::new(provider_state.follower_view()))
            .service(
                web::scope("/api/v1").service(aster_drive::api::routes::internal_storage::routes()),
            ),
    )
    .await;

    let path = "/api/v1/internal/storage/capabilities";
    let timestamp = Utc::now().timestamp();
    let nonce = "capabilities-no-ingress-test";
    let signature = sign_internal_request(secret_key, "GET", path, timestamp, nonce, None);
    let req = test::TestRequest::get()
        .uri(path)
        .insert_header(("x-aster-access-key", access_key))
        .insert_header(("x-aster-timestamp", timestamp.to_string()))
        .insert_header(("x-aster-nonce", nonce))
        .insert_header(("x-aster-signature", signature))
        .to_request();
    let resp = test::call_service(&follower_app, req).await;

    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "success");
    assert_eq!(
        body["data"]["protocol_version"],
        INTERNAL_STORAGE_PROTOCOL_VERSION_LABEL
    );
    assert_eq!(
        body["data"]["min_supported_protocol_version"],
        INTERNAL_STORAGE_MIN_SUPPORTED_PROTOCOL_VERSION_LABEL
    );
    assert_eq!(body["data"]["features"]["object_get"], true);
    assert_eq!(body["data"]["features"]["object_head"], true);
    assert_eq!(body["data"]["features"]["browser_presigned_cors"], true);
    assert_eq!(body["data"]["supports_range_read"], true);
    assert_eq!(body["data"]["managed_ingress"]["enabled"], true);
    assert_eq!(
        body["data"]["managed_ingress"]["driver_types"],
        serde_json::json!(["local", "s3"])
    );
}

#[actix_web::test]
async fn test_internal_storage_rejects_replayed_hmac_nonce() {
    let (provider_state, access_key, secret_key) =
        setup_internal_hmac_binding_state("capabilities-replay").await;
    let follower_app = test::init_service(
        App::new()
            .app_data(web::Data::new(provider_state.follower_view()))
            .service(
                web::scope("/api/v1").service(aster_drive::api::routes::internal_storage::routes()),
            ),
    )
    .await;

    let path = "/api/v1/internal/storage/capabilities";
    let timestamp = Utc::now().timestamp();
    let nonce = "capabilities-replay-test";
    let signature = sign_internal_request(&secret_key, "GET", path, timestamp, nonce, None);
    let signed_request = || {
        test::TestRequest::get()
            .uri(path)
            .insert_header(("x-aster-access-key", access_key.as_str()))
            .insert_header(("x-aster-timestamp", timestamp.to_string()))
            .insert_header(("x-aster-nonce", nonce))
            .insert_header(("x-aster-signature", signature.as_str()))
            .to_request()
    };

    let first = test::call_service(&follower_app, signed_request()).await;
    assert_eq!(first.status(), actix_web::http::StatusCode::OK);

    let replay = test::call_service(&follower_app, signed_request()).await;
    assert_eq!(replay.status(), actix_web::http::StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = test::read_body_json(replay).await;
    assert_eq!(body["code"], "auth.token_invalid");
    assert_eq!(body["msg"], "internal auth nonce has already been used");
}

#[actix_web::test]
async fn test_internal_storage_invalid_signature_does_not_consume_hmac_nonce() {
    let (provider_state, access_key, secret_key) =
        setup_internal_hmac_binding_state("capabilities-bad-signature").await;
    let follower_app = test::init_service(
        App::new()
            .app_data(web::Data::new(provider_state.follower_view()))
            .service(
                web::scope("/api/v1").service(aster_drive::api::routes::internal_storage::routes()),
            ),
    )
    .await;

    let path = "/api/v1/internal/storage/capabilities";
    let timestamp = Utc::now().timestamp();
    let nonce = "capabilities-bad-signature-test";
    let bad_req = test::TestRequest::get()
        .uri(path)
        .insert_header(("x-aster-access-key", access_key.as_str()))
        .insert_header(("x-aster-timestamp", timestamp.to_string()))
        .insert_header(("x-aster-nonce", nonce))
        .insert_header(("x-aster-signature", "00"))
        .to_request();
    let bad_resp = test::call_service(&follower_app, bad_req).await;

    assert_eq!(bad_resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = test::read_body_json(bad_resp).await;
    assert_eq!(body["code"], "auth.credentials_failed");
    assert_eq!(body["msg"], "internal auth signature mismatch");

    let signature = sign_internal_request(&secret_key, "GET", path, timestamp, nonce, None);
    let valid_req = test::TestRequest::get()
        .uri(path)
        .insert_header(("x-aster-access-key", access_key.as_str()))
        .insert_header(("x-aster-timestamp", timestamp.to_string()))
        .insert_header(("x-aster-nonce", nonce))
        .insert_header(("x-aster-signature", signature))
        .to_request();
    let valid_resp = test::call_service(&follower_app, valid_req).await;

    assert_eq!(valid_resp.status(), actix_web::http::StatusCode::OK);
}

#[actix_web::test]
async fn test_internal_storage_hmac_nonce_is_scoped_by_access_key() {
    let (provider_state, access_key_a, secret_key_a) =
        setup_internal_hmac_binding_state("capabilities-nonce-scope-a").await;
    let (access_key_b, secret_key_b) =
        create_internal_hmac_binding(&provider_state, "capabilities-nonce-scope-b").await;
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");

    let follower_app = test::init_service(
        App::new()
            .app_data(web::Data::new(provider_state.follower_view()))
            .service(
                web::scope("/api/v1").service(aster_drive::api::routes::internal_storage::routes()),
            ),
    )
    .await;

    let path = "/api/v1/internal/storage/capabilities";
    let timestamp = Utc::now().timestamp();
    let nonce = "capabilities-shared-nonce-test";
    let signature_a = sign_internal_request(&secret_key_a, "GET", path, timestamp, nonce, None);
    let signature_b = sign_internal_request(&secret_key_b, "GET", path, timestamp, nonce, None);
    let request = |access_key: &str, signature: &str| {
        test::TestRequest::get()
            .uri(path)
            .insert_header(("x-aster-access-key", access_key))
            .insert_header(("x-aster-timestamp", timestamp.to_string()))
            .insert_header(("x-aster-nonce", nonce))
            .insert_header(("x-aster-signature", signature))
            .to_request()
    };

    let first_a = test::call_service(&follower_app, request(&access_key_a, &signature_a)).await;
    assert_eq!(first_a.status(), actix_web::http::StatusCode::OK);

    let first_b = test::call_service(&follower_app, request(&access_key_b, &signature_b)).await;
    assert_eq!(first_b.status(), actix_web::http::StatusCode::OK);

    let replay_a = test::call_service(&follower_app, request(&access_key_a, &signature_a)).await;
    assert_eq!(replay_a.status(), actix_web::http::StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn test_internal_storage_rejects_concurrent_replayed_hmac_nonce() {
    let (provider_state, access_key, secret_key) =
        setup_internal_hmac_binding_state("capabilities-concurrent-replay").await;
    let provider_server = spawn_internal_storage_server(provider_state.follower_view()).await;

    let path = "/api/v1/internal/storage/capabilities";
    let url = Arc::new(format!("{}{}", provider_server.base_url, path));
    let timestamp = Utc::now().timestamp();
    let timestamp_header = Arc::new(timestamp.to_string());
    let nonce = Arc::new("capabilities-concurrent-replay-test".to_string());
    let signature = Arc::new(sign_internal_request(
        &secret_key,
        "GET",
        path,
        timestamp,
        nonce.as_str(),
        None,
    ));
    let access_key = Arc::new(access_key);
    let client = reqwest::Client::new();

    let responses = join_all((0..8).map(|_| {
        let client = client.clone();
        let url = url.clone();
        let access_key = access_key.clone();
        let timestamp_header = timestamp_header.clone();
        let nonce = nonce.clone();
        let signature = signature.clone();
        async move {
            client
                .get(url.as_str())
                .header("x-aster-access-key", access_key.as_str())
                .header("x-aster-timestamp", timestamp_header.as_str())
                .header("x-aster-nonce", nonce.as_str())
                .header("x-aster-signature", signature.as_str())
                .send()
                .await
        }
    }))
    .await;
    provider_server.stop().await;

    let statuses: Vec<_> = responses
        .into_iter()
        .map(|result| {
            result
                .expect("concurrent internal request should complete")
                .status()
        })
        .collect();
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == reqwest::StatusCode::OK)
            .count(),
        1
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == reqwest::StatusCode::UNAUTHORIZED)
            .count(),
        7
    );
}

#[actix_web::test]
async fn test_remote_storage_end_to_end_via_internal_api() {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;
    let provider_server = spawn_internal_storage_server(provider_state.follower_view()).await;

    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "provider-target".to_string(),
            base_url: provider_server.base_url.clone(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("consumer remote node should be created");
    let consumer_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), consumer_node.id)
            .await
            .expect("consumer remote node should be queryable");

    let (provider_binding, _) = master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "consumer-access".to_string(),
            master_url: "http://master.example.com".to_string(),
            access_key: consumer_node_model.access_key.clone(),
            secret_key: consumer_node_model.secret_key.clone(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider master binding should be created");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(
        &provider_state,
        &provider_binding.access_key,
        &provider_binding.access_key,
    )
    .await;

    let probed = wait_for_remote_probe(&consumer_state, consumer_node.id).await;
    assert_eq!(
        probed.capabilities.protocol_version,
        INTERNAL_STORAGE_PROTOCOL_VERSION_LABEL
    );
    assert_eq!(
        probed.capabilities.min_supported_protocol_version,
        INTERNAL_STORAGE_MIN_SUPPORTED_PROTOCOL_VERSION_LABEL
    );
    assert!(probed.capabilities.features.object_get);
    assert!(probed.capabilities.features.object_head);
    assert!(probed.capabilities.features.range_get);
    assert!(probed.capabilities.features.accept_ranges_header);
    assert!(probed.capabilities.features.browser_presigned_cors);
    assert!(probed.capabilities.supports_list);
    assert!(probed.capabilities.supports_range_read);
    assert!(probed.capabilities.supports_stream_upload);

    let remote_policy = create_remote_policy(
        &consumer_state,
        consumer_node.id,
        "Remote Test Policy",
        "consumer-base",
    )
    .await;

    let user = local::register(
        &consumer_state,
        "remoteuser",
        "remoteuser@example.com",
        "pass1234",
    )
    .await
    .expect("consumer test user should be created");

    let folder = folder::create(&consumer_state, user.id, "remote-folder", None)
        .await
        .expect("remote folder should be created");
    common::bind_policy_to_folder(&consumer_state, folder.id, remote_policy.id).await;

    let upload_bytes = b"hello remote storage over internal api";
    let upload_path = write_temp_upload_file(
        &consumer_state,
        &format!("remote-upload-{}.txt", uuid::Uuid::new_v4()),
        upload_bytes,
    )
    .await;
    let upload_path_string = upload_path.to_string_lossy().into_owned();

    let created = file::store_from_temp(
        &consumer_state,
        user.id,
        file::StoreFromTempRequest::new(
            Some(folder.id),
            "remote.txt",
            &upload_path_string,
            i64::try_from(upload_bytes.len()).expect("upload size should fit i64"),
        ),
    )
    .await
    .expect("remote file upload should succeed");
    aster_drive::utils::cleanup_temp_file(&upload_path_string).await;

    let created_file = file_repo::find_by_id(consumer_state.writer_db(), created.id)
        .await
        .expect("uploaded file should be queryable");
    let created_blob = file_repo::find_blob_by_id(consumer_state.writer_db(), created_file.blob_id)
        .await
        .expect("uploaded blob should be queryable");

    assert!(created_blob.hash.starts_with("remote-"));
    assert!(created_blob.storage_path.starts_with("files/"));

    let remote_driver = consumer_state
        .driver_registry
        .get_driver(&remote_policy)
        .expect("remote policy driver should resolve");
    assert!(
        remote_driver
            .exists(&created_blob.storage_path)
            .await
            .expect("remote HEAD should succeed")
    );

    let listed_paths = remote_driver
        .as_list()
        .expect("remote driver should support list")
        .list_paths(None)
        .await
        .expect("remote list should succeed");
    assert!(
        listed_paths.contains(&created_blob.storage_path),
        "remote list should include uploaded blob path"
    );

    let provider_uploaded_path = managed_ingress_object_path(
        &provider_state,
        &provider_binding.access_key,
        &provider_binding.storage_namespace,
        &remote_policy.base_path,
        &created_blob.storage_path,
    );
    let provider_uploaded_bytes = tokio::fs::read(&provider_uploaded_path)
        .await
        .expect("provider-side object should exist on local ingress storage");
    assert_eq!(provider_uploaded_bytes, upload_bytes);

    let downloaded_bytes = collect_download_body(
        file::download(&consumer_state, created_file.id, user.id, None)
            .await
            .expect("remote file download should succeed"),
    )
    .await;
    assert_eq!(downloaded_bytes, upload_bytes);

    file::delete(&consumer_state, created_file.id, user.id)
        .await
        .expect("remote file soft delete should succeed");
    file::purge(&consumer_state, created_file.id, user.id)
        .await
        .expect("remote file purge should succeed");

    assert!(
        !remote_driver
            .exists(&created_blob.storage_path)
            .await
            .expect("remote HEAD after purge should succeed")
    );
    assert!(
        tokio::fs::metadata(&provider_uploaded_path).await.is_err(),
        "provider-side object should be deleted after purge"
    );

    let empty_file = file::create_empty(
        &consumer_state,
        user.id,
        Some(folder.id),
        "empty-remote.txt",
    )
    .await
    .expect("remote empty file should be created");
    let empty_file = file_repo::find_by_id(consumer_state.writer_db(), empty_file.id)
        .await
        .expect("empty remote file should exist");
    let empty_blob = file_repo::find_blob_by_id(consumer_state.writer_db(), empty_file.blob_id)
        .await
        .expect("empty remote blob should exist");

    assert!(empty_blob.hash.starts_with("remote-"));
    assert!(empty_blob.storage_path.starts_with("files/"));
    assert!(
        remote_driver
            .exists(&empty_blob.storage_path)
            .await
            .expect("remote HEAD for empty blob should succeed")
    );

    let provider_empty_path = managed_ingress_object_path(
        &provider_state,
        &provider_binding.access_key,
        &provider_binding.storage_namespace,
        &remote_policy.base_path,
        &empty_blob.storage_path,
    );
    let empty_meta = tokio::fs::metadata(&provider_empty_path)
        .await
        .expect("provider-side empty object should exist");
    assert_eq!(empty_meta.len(), 0);

    file::purge(&consumer_state, empty_file.id, user.id)
        .await
        .expect("empty remote file purge should succeed");
    assert!(
        tokio::fs::metadata(&provider_empty_path).await.is_err(),
        "provider-side empty object should be deleted after purge"
    );

    provider_server.stop().await;
}

#[actix_web::test]
async fn test_remote_storage_end_to_end_via_reverse_tunnel() {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;
    let provider_server = spawn_internal_storage_server(provider_state.follower_view()).await;

    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "reverse-provider-target".to_string(),
            base_url: String::new(),
            transport_mode: RemoteNodeTransportMode::ReverseTunnel,
            is_enabled: true,
        },
    )
    .await
    .expect("reverse remote node should be created without base_url");
    let consumer_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), consumer_node.id)
            .await
            .expect("reverse remote node should be queryable");

    let (provider_binding, _) = master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "reverse-consumer-access".to_string(),
            master_url: "http://master.example.com".to_string(),
            access_key: consumer_node_model.access_key.clone(),
            secret_key: consumer_node_model.secret_key.clone(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider master binding should be created for reverse node");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(
        &provider_state,
        &provider_binding.access_key,
        &provider_binding.access_key,
    )
    .await;
    mark_remote_node_enrollment_completed(&consumer_state, consumer_node.id).await;

    let (tunnel_shutdown, tunnel_handle) = start_test_reverse_tunnel_worker(
        consumer_state.clone(),
        consumer_node_model.clone(),
        provider_server.base_url.clone(),
    )
    .await;

    let probed = remote_node::test_connection(&consumer_state, consumer_node.id)
        .await
        .expect("reverse tunnel remote node should probe through tunnel");
    assert_eq!(
        probed.transport_mode,
        RemoteNodeTransportMode::ReverseTunnel
    );
    assert_eq!(
        probed.tunnel.status,
        aster_drive::storage::remote_protocol::tunnel::server::RemoteTunnelOnlineStatus::Online
    );
    assert_eq!(
        probed.capabilities.protocol_version,
        INTERNAL_STORAGE_PROTOCOL_VERSION_LABEL
    );

    let remote_policy = create_remote_policy(
        &consumer_state,
        consumer_node.id,
        "Reverse Tunnel Remote Policy",
        "reverse-base",
    )
    .await;
    let remote_driver = consumer_state
        .driver_registry
        .get_driver(&remote_policy)
        .expect("reverse remote policy driver should resolve");

    remote_driver
        .put("files/reverse.txt", b"reverse tunnel payload")
        .await
        .expect("reverse remote PUT should succeed");
    assert!(
        remote_driver
            .exists("files/reverse.txt")
            .await
            .expect("reverse remote HEAD should succeed")
    );
    let downloaded = remote_driver
        .get("files/reverse.txt")
        .await
        .expect("reverse remote GET should succeed");
    assert_eq!(downloaded, b"reverse tunnel payload");

    let mut stream = remote_driver
        .get_range("files/reverse.txt", 8, Some(6))
        .await
        .expect("reverse remote range GET should succeed");
    let mut range = Vec::new();
    stream
        .read_to_end(&mut range)
        .await
        .expect("reverse remote range body should read");
    assert_eq!(range, b"tunnel");

    let streamed_payload = b"reverse tunnel buffered stream upload".to_vec();
    let stream_driver = remote_driver
        .as_stream_upload()
        .expect("reverse remote driver should expose bounded stream upload");
    stream_driver
        .put_reader(
            "files/streamed.txt",
            Box::new(std::io::Cursor::new(streamed_payload.clone())),
            i64::try_from(streamed_payload.len()).expect("test payload length should fit i64"),
        )
        .await
        .expect("reverse buffered stream upload should succeed");
    assert_eq!(
        remote_driver
            .get("files/streamed.txt")
            .await
            .expect("reverse streamed upload should be readable"),
        streamed_payload
    );

    let listed_paths = remote_driver
        .as_list()
        .expect("reverse remote driver should support list")
        .list_paths(Some("files"))
        .await
        .expect("reverse remote list should succeed");
    assert!(listed_paths.contains(&"files/reverse.txt".to_string()));
    assert!(listed_paths.contains(&"files/streamed.txt".to_string()));

    remote_driver
        .delete("files/reverse.txt")
        .await
        .expect("reverse remote DELETE should succeed");
    assert!(
        !remote_driver
            .exists("files/reverse.txt")
            .await
            .expect("reverse remote HEAD after delete should succeed")
    );

    let provider_streamed_path = managed_ingress_object_path(
        &provider_state,
        &provider_binding.access_key,
        &provider_binding.storage_namespace,
        &remote_policy.base_path,
        "files/streamed.txt",
    );
    assert_eq!(
        tokio::fs::read(&provider_streamed_path)
            .await
            .expect("provider should receive reverse streamed object"),
        b"reverse tunnel buffered stream upload"
    );

    stop_test_reverse_tunnel_worker(tunnel_shutdown, tunnel_handle).await;
    provider_server.stop().await;
}

#[actix_web::test]
async fn test_reverse_tunnel_handles_concurrent_requests_across_polls() {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;
    let provider_server = spawn_internal_storage_server(provider_state.follower_view()).await;

    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "reverse-concurrent-target".to_string(),
            base_url: String::new(),
            transport_mode: RemoteNodeTransportMode::ReverseTunnel,
            is_enabled: true,
        },
    )
    .await
    .expect("reverse remote node should be created without base_url");
    let consumer_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), consumer_node.id)
            .await
            .expect("reverse remote node should be queryable");

    let (provider_binding, _) = master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "reverse-concurrent-access".to_string(),
            master_url: "http://master.example.com".to_string(),
            access_key: consumer_node_model.access_key.clone(),
            secret_key: consumer_node_model.secret_key.clone(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider master binding should be created for reverse node");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(
        &provider_state,
        &provider_binding.access_key,
        &provider_binding.access_key,
    )
    .await;
    mark_remote_node_enrollment_completed(&consumer_state, consumer_node.id).await;
    seed_remote_capabilities(
        &consumer_state,
        consumer_node.id,
        RemoteStorageCapabilities::current(),
    )
    .await;

    let (tunnel_shutdown, tunnel_handle) = start_test_reverse_tunnel_worker(
        consumer_state.clone(),
        consumer_node_model,
        provider_server.base_url.clone(),
    )
    .await;

    let remote_policy = create_remote_policy(
        &consumer_state,
        consumer_node.id,
        "Reverse Concurrent Remote Policy",
        "reverse-concurrent-base",
    )
    .await;
    let remote_driver = consumer_state
        .driver_registry
        .get_driver(&remote_policy)
        .expect("reverse remote policy driver should resolve");

    let writes = (0..8)
        .map(|index| {
            let driver = remote_driver.clone();
            tokio::spawn(async move {
                let path = format!("files/concurrent-{index}.txt");
                let payload = format!("reverse concurrent payload {index}");
                driver
                    .put(&path, payload.as_bytes())
                    .await
                    .expect("concurrent reverse tunnel PUT should succeed");
                let stored = driver
                    .get(&path)
                    .await
                    .expect("concurrent reverse tunnel GET should succeed");
                assert_eq!(stored, payload.as_bytes());
            })
        })
        .collect::<Vec<_>>();
    for result in join_all(writes).await {
        result.expect("concurrent reverse tunnel task should join");
    }

    let listed_paths = remote_driver
        .as_list()
        .expect("reverse remote driver should support list")
        .list_paths(Some("files"))
        .await
        .expect("reverse remote list should succeed after concurrent writes");
    for index in 0..8 {
        assert!(listed_paths.contains(&format!("files/concurrent-{index}.txt")));
    }

    stop_test_reverse_tunnel_worker(tunnel_shutdown, tunnel_handle).await;
    provider_server.stop().await;
}

#[actix_web::test]
async fn test_reverse_tunnel_e2e_over_http_with_follower_worker() {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;
    let provider_listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("provider listener should bind for production tunnel worker test");
    let provider_port = provider_listener
        .local_addr()
        .expect("provider listener should expose local addr")
        .port();
    let provider_server = spawn_internal_storage_server_on_listener(
        follower_view_with_server_port(&provider_state, provider_port),
        provider_listener,
    )
    .await;
    let primary_server = spawn_reverse_tunnel_primary_server(consumer_state.clone()).await;

    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "reverse-http-provider-target".to_string(),
            base_url: String::new(),
            transport_mode: RemoteNodeTransportMode::ReverseTunnel,
            is_enabled: true,
        },
    )
    .await
    .expect("reverse HTTP remote node should be created without base_url");
    let consumer_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), consumer_node.id)
            .await
            .expect("reverse HTTP remote node should be queryable");

    let (provider_binding, _) = master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "reverse-http-consumer-access".to_string(),
            master_url: primary_server.base_url.clone(),
            access_key: consumer_node_model.access_key.clone(),
            secret_key: consumer_node_model.secret_key.clone(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider master binding should be created for HTTP reverse node");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(
        &provider_state,
        &provider_binding.access_key,
        &provider_binding.access_key,
    )
    .await;
    mark_remote_node_enrollment_completed(&consumer_state, consumer_node.id).await;

    let tunnel_worker = start_production_reverse_tunnel_worker(follower_view_with_server_port(
        &provider_state,
        provider_port,
    ))
    .await;
    wait_for_stream_lane(&consumer_state, &consumer_node_model).await;

    let probed = remote_node::test_connection(&consumer_state, consumer_node.id)
        .await
        .expect("HTTP reverse tunnel remote node should probe through production worker");
    assert_eq!(
        probed.tunnel.status,
        aster_drive::storage::remote_protocol::tunnel::server::RemoteTunnelOnlineStatus::Online
    );
    assert_eq!(
        probed.capabilities.protocol_version,
        INTERNAL_STORAGE_PROTOCOL_VERSION_LABEL
    );

    let remote_policy = create_remote_policy(
        &consumer_state,
        consumer_node.id,
        "Reverse HTTP Tunnel Remote Policy",
        "reverse-http-base",
    )
    .await;
    let remote_driver = consumer_state
        .driver_registry
        .get_driver(&remote_policy)
        .expect("HTTP reverse remote policy driver should resolve");

    let payload = vec![b'x'; 1024 * 1024 + 64 * 1024];
    remote_driver
        .put("files/http-large.bin", &payload)
        .await
        .expect("HTTP reverse tunnel PUT above global JSON limit should succeed");
    assert_eq!(
        remote_driver
            .get("files/http-large.bin")
            .await
            .expect("HTTP reverse tunnel GET above global JSON limit should succeed"),
        payload
    );

    let provider_object = managed_ingress_object_path(
        &provider_state,
        &provider_binding.access_key,
        &provider_binding.storage_namespace,
        &remote_policy.base_path,
        "files/http-large.bin",
    );
    assert_eq!(
        tokio::fs::metadata(&provider_object)
            .await
            .expect("provider should receive HTTP tunnel object")
            .len(),
        1024 * 1024 + 64 * 1024
    );

    stop_test_reverse_tunnel_http_worker(tunnel_worker).await;
    primary_server.stop().await;
    provider_server.stop().await;
}

#[actix_web::test]
async fn test_reverse_tunnel_production_worker_falls_back_to_poll_when_stream_unavailable() {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;
    let provider_listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("provider listener should bind for poll fallback test");
    let provider_port = provider_listener
        .local_addr()
        .expect("provider listener should expose local addr")
        .port();
    let provider_server = spawn_internal_storage_server_on_listener(
        follower_view_with_server_port(&provider_state, provider_port),
        provider_listener,
    )
    .await;
    let primary_server =
        spawn_reverse_tunnel_poll_only_primary_server(consumer_state.clone()).await;

    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "reverse-poll-fallback-target".to_string(),
            base_url: String::new(),
            transport_mode: RemoteNodeTransportMode::ReverseTunnel,
            is_enabled: true,
        },
    )
    .await
    .expect("reverse poll fallback node should be created without base_url");
    let consumer_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), consumer_node.id)
            .await
            .expect("reverse poll fallback node should be queryable");

    let (provider_binding, _) = master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "reverse-poll-fallback-access".to_string(),
            master_url: primary_server.base_url.clone(),
            access_key: consumer_node_model.access_key.clone(),
            secret_key: consumer_node_model.secret_key.clone(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider binding should be created for poll fallback reverse node");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload for poll fallback test");
    create_managed_local_ingress_for_binding(
        &provider_state,
        &provider_binding.access_key,
        &provider_binding.access_key,
    )
    .await;
    mark_remote_node_enrollment_completed(&consumer_state, consumer_node.id).await;

    let tunnel_worker = start_production_reverse_tunnel_worker(follower_view_with_server_port(
        &provider_state,
        provider_port,
    ))
    .await;

    let probed = remote_node::test_connection(&consumer_state, consumer_node.id)
        .await
        .expect("poll fallback should probe through production worker");
    assert_eq!(
        probed.tunnel.status,
        aster_drive::storage::remote_protocol::tunnel::server::RemoteTunnelOnlineStatus::Online
    );
    assert_eq!(
        probed.capabilities.protocol_version,
        INTERNAL_STORAGE_PROTOCOL_VERSION_LABEL
    );
    assert!(
        !consumer_state
            .remote_protocol
            .tunnel_registry()
            .has_stream_lane(&consumer_node_model),
        "poll-only primary server must not register stream lanes"
    );

    let remote_policy = create_remote_policy(
        &consumer_state,
        consumer_node.id,
        "Reverse Poll Fallback Remote Policy",
        "reverse-poll-fallback-base",
    )
    .await;
    let remote_driver = consumer_state
        .driver_registry
        .get_driver(&remote_policy)
        .expect("poll fallback remote policy driver should resolve");

    remote_driver
        .put("files/fallback.txt", b"poll fallback payload")
        .await
        .expect("poll fallback reverse tunnel PUT should succeed");
    assert_eq!(
        remote_driver
            .get("files/fallback.txt")
            .await
            .expect("poll fallback reverse tunnel GET should succeed"),
        b"poll fallback payload"
    );

    let provider_object = managed_ingress_object_path(
        &provider_state,
        &provider_binding.access_key,
        &provider_binding.storage_namespace,
        &remote_policy.base_path,
        "files/fallback.txt",
    );
    assert_eq!(
        tokio::fs::read(&provider_object)
            .await
            .expect("provider should receive poll fallback object"),
        b"poll fallback payload"
    );

    stop_test_reverse_tunnel_http_worker(tunnel_worker).await;
    primary_server.stop().await;
    provider_server.stop().await;
}

#[actix_web::test]
async fn test_reverse_tunnel_follower_worker_rejects_non_storage_paths() {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;
    let provider_listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("provider listener should bind for tunnel path guard test");
    let provider_port = provider_listener
        .local_addr()
        .expect("provider listener should expose local addr")
        .port();
    let provider_server = spawn_internal_storage_server_on_listener(
        follower_view_with_server_port(&provider_state, provider_port),
        provider_listener,
    )
    .await;
    let primary_server = spawn_reverse_tunnel_primary_server(consumer_state.clone()).await;

    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "reverse-path-guard-target".to_string(),
            base_url: String::new(),
            transport_mode: RemoteNodeTransportMode::ReverseTunnel,
            is_enabled: true,
        },
    )
    .await
    .expect("reverse node should be created for path guard test");
    let consumer_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), consumer_node.id)
            .await
            .expect("reverse node should be queryable for path guard test");

    master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "reverse-path-guard-access".to_string(),
            master_url: primary_server.base_url.clone(),
            access_key: consumer_node_model.access_key.clone(),
            secret_key: consumer_node_model.secret_key.clone(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider binding should be created for path guard test");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload for path guard test");
    mark_remote_node_enrollment_completed(&consumer_state, consumer_node.id).await;

    let tunnel_worker = start_production_reverse_tunnel_worker(follower_view_with_server_port(
        &provider_state,
        provider_port,
    ))
    .await;

    let response = consumer_state
        .remote_protocol
        .tunnel_registry()
        .send(
            &consumer_node_model,
            http::Method::GET,
            "/api/v1/admin/users".to_string(),
            None,
            Vec::new(),
            Bytes::new(),
        )
        .await
        .expect("forbidden tunnel target should return a tunnel response");
    assert_eq!(response.status, http::StatusCode::FORBIDDEN);
    assert_eq!(
        response.body.as_ref(),
        b"reverse tunnel can only proxy internal storage paths"
    );
    let info = remote_node::get(&consumer_state, consumer_node.id)
        .await
        .expect("remote node info should load after forbidden tunnel target");
    assert!(
        info.tunnel
            .last_error
            .contains("reverse tunnel can only proxy internal storage paths"),
        "forbidden tunnel target should be visible in tunnel diagnostics, got '{}'",
        info.tunnel.last_error
    );

    stop_test_reverse_tunnel_http_worker(tunnel_worker).await;
    primary_server.stop().await;
    provider_server.stop().await;
}

#[actix_web::test]
async fn test_reverse_tunnel_policy_connection_test_uses_tunnel_registry() {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;
    let provider_server = spawn_internal_storage_server(provider_state.follower_view()).await;

    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "reverse-policy-test-target".to_string(),
            base_url: String::new(),
            transport_mode: RemoteNodeTransportMode::ReverseTunnel,
            is_enabled: true,
        },
    )
    .await
    .expect("reverse remote node should be created without base_url");
    let consumer_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), consumer_node.id)
            .await
            .expect("reverse remote node should be queryable");

    let (provider_binding, _) = master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "reverse-policy-test-access".to_string(),
            master_url: "http://master.example.com".to_string(),
            access_key: consumer_node_model.access_key.clone(),
            secret_key: consumer_node_model.secret_key.clone(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider master binding should be created for reverse node");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(
        &provider_state,
        &provider_binding.access_key,
        &provider_binding.access_key,
    )
    .await;
    mark_remote_node_enrollment_completed(&consumer_state, consumer_node.id).await;
    seed_remote_capabilities(
        &consumer_state,
        consumer_node.id,
        RemoteStorageCapabilities::current(),
    )
    .await;

    let (tunnel_shutdown, tunnel_handle) = start_test_reverse_tunnel_worker(
        consumer_state.clone(),
        consumer_node_model,
        provider_server.base_url.clone(),
    )
    .await;

    policy::test_connection_params(
        &consumer_state,
        policy::TestDraftStoragePolicyConnectionInput {
            policy_id: None,
            connection: policy::StoragePolicyConnectionInput {
                driver_type: DriverType::Remote,
                endpoint: String::new(),
                bucket: String::new(),
                access_key: String::new(),
                secret_key: String::new(),
                base_path: "reverse-policy-test".to_string(),
                remote_node_id: Some(consumer_node.id),
                remote_storage_target_key: None,
                options: Default::default(),
            },
        },
    )
    .await
    .expect("reverse tunnel policy connection test should use tunnel transport");

    let test_objects = remote_driver_for_policy_connection_test_objects(
        &provider_state,
        &provider_binding.access_key,
        &provider_binding.storage_namespace,
        "reverse-policy-test",
    )
    .await;
    assert!(
        test_objects.is_empty(),
        "connection test should clean up tunneled test object, found {test_objects:?}"
    );

    stop_test_reverse_tunnel_worker(tunnel_shutdown, tunnel_handle).await;
    provider_server.stop().await;
}

#[actix_web::test]
async fn test_reverse_tunnel_http_rejects_access_key_mismatch() {
    let state = common::setup().await;
    let primary_server = spawn_reverse_tunnel_primary_server(state.clone()).await;
    let node_a = remote_node::create(
        &state,
        remote_node::CreateRemoteNodeInput {
            name: "reverse-http-auth-a".to_string(),
            base_url: String::new(),
            transport_mode: RemoteNodeTransportMode::ReverseTunnel,
            is_enabled: true,
        },
    )
    .await
    .expect("node a should be created");
    let node_b = remote_node::create(
        &state,
        remote_node::CreateRemoteNodeInput {
            name: "reverse-http-auth-b".to_string(),
            base_url: String::new(),
            transport_mode: RemoteNodeTransportMode::ReverseTunnel,
            is_enabled: true,
        },
    )
    .await
    .expect("node b should be created");
    let node_a_model = managed_follower_repo::find_by_id(state.writer_db(), node_a.id)
        .await
        .expect("node a should be queryable");
    let node_b_model = managed_follower_repo::find_by_id(state.writer_db(), node_b.id)
        .await
        .expect("node b should be queryable");

    let client = reqwest::Client::new();
    let (status, body) = send_signed_tunnel_json_raw(
        &client,
        &node_a_model,
        &primary_server.base_url,
        REMOTE_TUNNEL_POLL_PATH,
        serde_json::json!({ "access_key": node_b_model.access_key }),
    )
    .await;

    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
    assert!(
        body["msg"]
            .as_str()
            .unwrap_or_default()
            .contains("does not match signed credentials"),
        "unexpected mismatch response: {body}"
    );

    primary_server.stop().await;
}

#[actix_web::test]
async fn test_reverse_tunnel_completion_from_wrong_node_does_not_consume_pending_request() {
    let state = common::setup().await;
    let node_a = remote_node::create(
        &state,
        remote_node::CreateRemoteNodeInput {
            name: "reverse-wrong-complete-a".to_string(),
            base_url: String::new(),
            transport_mode: RemoteNodeTransportMode::ReverseTunnel,
            is_enabled: true,
        },
    )
    .await
    .expect("node a should be created");
    let node_b = remote_node::create(
        &state,
        remote_node::CreateRemoteNodeInput {
            name: "reverse-wrong-complete-b".to_string(),
            base_url: String::new(),
            transport_mode: RemoteNodeTransportMode::ReverseTunnel,
            is_enabled: true,
        },
    )
    .await
    .expect("node b should be created");
    let node_a_model = managed_follower_repo::find_by_id(state.writer_db(), node_a.id)
        .await
        .expect("node a should be queryable");
    let node_b_model = managed_follower_repo::find_by_id(state.writer_db(), node_b.id)
        .await
        .expect("node b should be queryable");

    let poll_state = state.clone();
    let poll_node = node_a_model.clone();
    let poll_handle = tokio::spawn(async move {
        aster_drive::storage::remote_protocol::tunnel::server::poll(&poll_state, &poll_node)
            .await
            .expect("node a poll should succeed")
    });
    tokio::task::yield_now().await;

    let send_handle = {
        let registry = state.remote_protocol.tunnel_registry().clone();
        let node = node_a_model.clone();
        tokio::spawn(async move {
            registry
                .send(
                    &node,
                    http::Method::GET,
                    format!("{INTERNAL_STORAGE_BASE_PATH}/capabilities"),
                    None,
                    Vec::new(),
                    Bytes::new(),
                )
                .await
        })
    };
    let poll = poll_handle.await.expect("poll task should join");
    let request = poll
        .request
        .expect("node a poll should receive dispatched request");

    let wrong_error = aster_drive::storage::remote_protocol::tunnel::server::complete(
        &state,
        &node_b_model,
        RemoteTunnelResponse {
            request_id: request.request_id.clone(),
            status: 200,
            headers: Vec::new(),
            body: b"wrong node".to_vec(),
        },
    )
    .await
    .expect_err("wrong node must not complete another node's pending request");
    assert!(
        wrong_error
            .message()
            .contains("does not belong to this remote node"),
        "unexpected wrong-node error: {wrong_error}"
    );

    aster_drive::storage::remote_protocol::tunnel::server::complete(
        &state,
        &node_a_model,
        RemoteTunnelResponse {
            request_id: request.request_id,
            status: 200,
            headers: Vec::new(),
            body: serde_json::to_vec(&serde_json::json!({
            "code": "success",
            "msg": "",
            "data": RemoteStorageCapabilities::current(),
            }))
            .expect("capabilities response should serialize"),
        },
    )
    .await
    .expect("owner node should still be able to complete pending request");
    let response = send_handle
        .await
        .expect("reverse tunnel send task should join")
        .expect("reverse tunnel send should complete after owner response");
    assert_eq!(response.status.as_u16(), 200);
}

#[actix_web::test]
async fn test_reverse_tunnel_records_offline_error_and_clears_on_poll() {
    let state = common::setup().await;
    let node = remote_node::create(
        &state,
        remote_node::CreateRemoteNodeInput {
            name: "reverse-offline-error".to_string(),
            base_url: String::new(),
            transport_mode: RemoteNodeTransportMode::ReverseTunnel,
            is_enabled: true,
        },
    )
    .await
    .expect("reverse node should be created");
    let node_model = managed_follower_repo::find_by_id(state.writer_db(), node.id)
        .await
        .expect("reverse node should be queryable");

    let error = state
        .remote_protocol
        .tunnel_registry()
        .send(
            &node_model,
            http::Method::GET,
            format!("{INTERNAL_STORAGE_BASE_PATH}/capabilities"),
            None,
            Vec::new(),
            Bytes::new(),
        )
        .await
        .expect_err("offline reverse tunnel send should fail");
    assert!(error.message().contains("reverse tunnel is offline"));

    let info = remote_node::get(&state, node.id)
        .await
        .expect("remote node info should load");
    assert!(
        info.tunnel.last_error.contains("reverse tunnel is offline"),
        "runtime tunnel error should be visible immediately, got '{}'",
        info.tunnel.last_error
    );
    wait_for_tunnel_error_persisted(&state, node.id, "reverse tunnel is offline").await;

    let poll_state = state.clone();
    let poll_node = node_model.clone();
    let poll_handle = tokio::spawn(async move {
        aster_drive::storage::remote_protocol::tunnel::server::poll(&poll_state, &poll_node)
            .await
            .expect("reverse tunnel poll should clear stale error")
    });
    tokio::task::yield_now().await;
    let send_handle = {
        let registry = state.remote_protocol.tunnel_registry().clone();
        let node = node_model.clone();
        tokio::spawn(async move {
            registry
                .send(
                    &node,
                    http::Method::GET,
                    format!("{INTERNAL_STORAGE_BASE_PATH}/capabilities"),
                    None,
                    Vec::new(),
                    Bytes::new(),
                )
                .await
        })
    };
    let polled = poll_handle.await.expect("poll task should join");
    let request = polled
        .request
        .expect("poll should receive the stale-error clearing request");
    aster_drive::storage::remote_protocol::tunnel::server::complete(
        &state,
        &node_model,
        RemoteTunnelResponse {
            request_id: request.request_id,
            status: 200,
            headers: Vec::new(),
            body: serde_json::to_vec(&serde_json::json!({
            "code": "success",
            "msg": "",
            "data": RemoteStorageCapabilities::current(),
            }))
            .expect("capabilities response should serialize"),
        },
    )
    .await
    .expect("successful tunnel completion should clear stale error");
    send_handle
        .await
        .expect("send task should join")
        .expect("send should complete after success response");
    let cleared = remote_node::get(&state, node.id)
        .await
        .expect("remote node info should load after poll");
    assert_eq!(cleared.tunnel.last_error, "");
}

#[actix_web::test]
async fn test_reverse_tunnel_polls_do_not_touch_updated_at() {
    let state = common::setup().await;
    let node = remote_node::create(
        &state,
        remote_node::CreateRemoteNodeInput {
            name: "reverse-poll-updated-at".to_string(),
            base_url: String::new(),
            transport_mode: RemoteNodeTransportMode::ReverseTunnel,
            is_enabled: true,
        },
    )
    .await
    .expect("reverse node should be created");
    let before = managed_follower_repo::find_by_id(state.writer_db(), node.id)
        .await
        .expect("reverse node should be queryable before poll");
    tokio::time::sleep(Duration::from_millis(2)).await;

    let poll_state = state.clone();
    let poll_node = before.clone();
    let poll_handle = tokio::spawn(async move {
        aster_drive::storage::remote_protocol::tunnel::server::poll(&poll_state, &poll_node)
            .await
            .expect("reverse tunnel poll should succeed")
    });
    tokio::task::yield_now().await;
    let send_handle = {
        let registry = state.remote_protocol.tunnel_registry().clone();
        let node = before.clone();
        tokio::spawn(async move {
            registry
                .send(
                    &node,
                    http::Method::GET,
                    format!("{INTERNAL_STORAGE_BASE_PATH}/capabilities"),
                    None,
                    Vec::new(),
                    Bytes::new(),
                )
                .await
        })
    };
    let polled = poll_handle.await.expect("poll task should join");
    let request = polled
        .request
        .expect("poll should receive the updated_at test request");
    aster_drive::storage::remote_protocol::tunnel::server::complete(
        &state,
        &before,
        RemoteTunnelResponse {
            request_id: request.request_id,
            status: 200,
            headers: Vec::new(),
            body: serde_json::to_vec(&serde_json::json!({
                "code": "success",
                "msg": "",
                "data": RemoteStorageCapabilities::current(),
            }))
            .expect("capabilities response should serialize"),
        },
    )
    .await
    .expect("tunnel completion should finish updated_at test request");
    send_handle
        .await
        .expect("send task should join")
        .expect("send should complete after updated_at test response");

    let after = managed_follower_repo::find_by_id(state.writer_db(), node.id)
        .await
        .expect("reverse node should be queryable after poll");
    assert_eq!(
        after.updated_at, before.updated_at,
        "tunnel heartbeat should not change configuration updated_at"
    );
    assert!(after.tunnel_last_seen_at.is_some());
}

#[actix_web::test]
async fn test_reverse_tunnel_completion_rejects_body_above_cap() {
    let state = common::setup().await;
    let node = remote_node::create(
        &state,
        remote_node::CreateRemoteNodeInput {
            name: "reverse-too-large-complete".to_string(),
            base_url: String::new(),
            transport_mode: RemoteNodeTransportMode::ReverseTunnel,
            is_enabled: true,
        },
    )
    .await
    .expect("node should be created");
    let node_model = managed_follower_repo::find_by_id(state.writer_db(), node.id)
        .await
        .expect("node should be queryable");

    let error = aster_drive::storage::remote_protocol::tunnel::server::complete(
        &state,
        &node_model,
        RemoteTunnelResponse {
            request_id: "too-large".to_string(),
            status: 200,
            headers: Vec::new(),
            body: vec![0; REMOTE_TUNNEL_BODY_LIMIT + 1],
        },
    )
    .await
    .expect_err("oversized tunnel response must be rejected before pending lookup");
    assert!(
        error
            .message()
            .contains("reverse tunnel response body exceeds"),
        "unexpected oversized response error: {error}"
    );
}

#[actix_web::test]
async fn test_reverse_tunnel_route_accepts_payload_above_global_json_limit() {
    const {
        assert!(REMOTE_TUNNEL_JSON_LIMIT > 1024 * 1024);
    }

    let state = common::setup().await;
    let primary_server = spawn_reverse_tunnel_primary_server(state.clone()).await;
    let node = remote_node::create(
        &state,
        remote_node::CreateRemoteNodeInput {
            name: "reverse-json-limit".to_string(),
            base_url: String::new(),
            transport_mode: RemoteNodeTransportMode::ReverseTunnel,
            is_enabled: true,
        },
    )
    .await
    .expect("node should be created");
    let node_model = managed_follower_repo::find_by_id(state.writer_db(), node.id)
        .await
        .expect("node should be queryable");

    let client = reqwest::Client::new();
    let (status, response) = send_signed_tunnel_json_raw(
        &client,
        &node_model,
        &primary_server.base_url,
        REMOTE_TUNNEL_COMPLETE_PATH,
        serde_json::json!({
            "request_id": "not-pending",
            "status": 200,
            "headers": [],
            "body": vec![b'a'; 1024 * 1024 + 1],
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
    assert!(
        response["msg"]
            .as_str()
            .unwrap_or_default()
            .contains("no longer pending"),
        "large complete should reach handler instead of JSON limit: {response}"
    );

    primary_server.stop().await;
}

#[actix_web::test]
async fn test_reverse_tunnel_remote_policy_rejects_presigned_strategies() {
    let state = common::setup().await;
    let node = remote_node::create(
        &state,
        remote_node::CreateRemoteNodeInput {
            name: "reverse-presigned-rejected".to_string(),
            base_url: String::new(),
            transport_mode: RemoteNodeTransportMode::ReverseTunnel,
            is_enabled: true,
        },
    )
    .await
    .expect("reverse remote node should be created");

    let make_input = |options| policy::CreateStoragePolicyInput {
        name: "Reverse Presigned Rejected".to_string(),
        connection: policy::StoragePolicyConnectionInput {
            driver_type: DriverType::Remote,
            endpoint: String::new(),
            bucket: String::new(),
            access_key: String::new(),
            secret_key: String::new(),
            base_path: "reverse-presigned".to_string(),
            remote_node_id: Some(node.id),
            remote_storage_target_key: None,
            options: Default::default(),
        },
        max_file_size: 0,
        chunk_size: Some(5_242_880),
        is_default: false,
        allowed_types: None,
        options: Some(options),
        remote_storage_target_key: None,
        application_config: Default::default(),
    };

    let upload_error = policy::create(
        &state,
        make_input(StoragePolicyOptions {
            remote_upload_strategy: Some(RemoteUploadStrategy::Presigned),
            ..Default::default()
        }),
    )
    .await
    .expect_err("reverse tunnel should reject presigned remote upload policies");
    assert_eq!(
        upload_error.api_error_code(),
        ApiErrorCode::PolicyRemoteNodeTransferStrategyUnsupported
    );
    assert!(
        upload_error
            .message()
            .contains("reverse tunnel remote nodes do not support presigned")
    );

    let download_error = policy::create(
        &state,
        make_input(StoragePolicyOptions {
            remote_download_strategy: Some(RemoteDownloadStrategy::Presigned),
            ..Default::default()
        }),
    )
    .await
    .expect_err("reverse tunnel should reject presigned remote download policies");
    assert_eq!(
        download_error.api_error_code(),
        ApiErrorCode::PolicyRemoteNodeTransferStrategyUnsupported
    );
    assert!(
        download_error
            .message()
            .contains("reverse tunnel remote nodes do not support presigned")
    );
}

#[actix_web::test]
async fn test_auto_empty_url_remote_policy_rejects_presigned_strategies() {
    let state = common::setup().await;
    let node = remote_node::create(
        &state,
        remote_node::CreateRemoteNodeInput {
            name: "auto-empty-presigned-rejected".to_string(),
            base_url: String::new(),
            transport_mode: RemoteNodeTransportMode::Auto,
            is_enabled: true,
        },
    )
    .await
    .expect("auto remote node should be created without base_url");

    let error = policy::create(
        &state,
        policy::CreateStoragePolicyInput {
            name: "Auto Empty Presigned Rejected".to_string(),
            connection: policy::StoragePolicyConnectionInput {
                driver_type: DriverType::Remote,
                endpoint: String::new(),
                bucket: String::new(),
                access_key: String::new(),
                secret_key: String::new(),
                base_path: "auto-empty-presigned".to_string(),
                remote_node_id: Some(node.id),
                remote_storage_target_key: None,
                options: Default::default(),
            },
            max_file_size: 0,
            chunk_size: Some(5_242_880),
            is_default: false,
            allowed_types: None,
            options: Some(StoragePolicyOptions {
                remote_upload_strategy: Some(RemoteUploadStrategy::Presigned),
                ..Default::default()
            }),
            remote_storage_target_key: None,
            application_config: Default::default(),
        },
    )
    .await
    .expect_err(
        "auto node without base_url resolves to reverse tunnel and should reject presigned",
    );
    assert_eq!(
        error.api_error_code(),
        ApiErrorCode::PolicyRemoteNodeTransferStrategyUnsupported
    );
    assert!(
        error
            .message()
            .contains("reverse tunnel remote nodes do not support presigned")
    );
}

#[actix_web::test]
async fn test_reverse_tunnel_remote_policy_update_rejects_presigned_strategies() {
    let state = common::setup().await;
    let node = remote_node::create(
        &state,
        remote_node::CreateRemoteNodeInput {
            name: "reverse-presigned-update-rejected".to_string(),
            base_url: String::new(),
            transport_mode: RemoteNodeTransportMode::ReverseTunnel,
            is_enabled: true,
        },
    )
    .await
    .expect("reverse remote node should be created");
    seed_remote_capabilities(&state, node.id, RemoteStorageCapabilities::current()).await;
    let policy = create_remote_policy(
        &state,
        node.id,
        "Reverse Presigned Update Rejected",
        "reverse-presigned-update",
    )
    .await;

    let upload_error = policy::update(
        &state,
        policy.id,
        policy::UpdateStoragePolicyInput {
            options: Some(StoragePolicyOptions {
                remote_upload_strategy: Some(RemoteUploadStrategy::Presigned),
                ..Default::default()
            }),
            ..Default::default()
        },
    )
    .await
    .expect_err("reverse tunnel should reject presigned upload policy updates");
    assert_eq!(
        upload_error.api_error_code(),
        ApiErrorCode::PolicyRemoteNodeTransferStrategyUnsupported
    );
    assert!(
        upload_error
            .message()
            .contains("reverse tunnel remote nodes do not support presigned")
    );

    let download_error = policy::update(
        &state,
        policy.id,
        policy::UpdateStoragePolicyInput {
            options: Some(StoragePolicyOptions {
                remote_download_strategy: Some(RemoteDownloadStrategy::Presigned),
                ..Default::default()
            }),
            ..Default::default()
        },
    )
    .await
    .expect_err("reverse tunnel should reject presigned download policy updates");
    assert_eq!(
        download_error.api_error_code(),
        ApiErrorCode::PolicyRemoteNodeTransferStrategyUnsupported
    );
    assert!(
        download_error
            .message()
            .contains("reverse tunnel remote nodes do not support presigned")
    );
}

#[actix_web::test]
async fn test_remote_node_update_rejects_reverse_tunnel_when_referenced_policy_uses_presigned() {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;
    let provider_server = spawn_internal_storage_server(provider_state.follower_view()).await;

    let node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "direct-presigned-update-target".to_string(),
            base_url: provider_server.base_url.clone(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("direct remote node should be created");
    seed_remote_capabilities(
        &consumer_state,
        node.id,
        RemoteStorageCapabilities::current(),
    )
    .await;
    let _policy = create_remote_policy_with_options(
        &consumer_state,
        node.id,
        "Direct Presigned Before Reverse Switch",
        "direct-presigned-before-switch",
        StoragePolicyOptions {
            remote_upload_strategy: Some(RemoteUploadStrategy::Presigned),
            ..Default::default()
        },
        5_242_880,
    )
    .await;

    let error = remote_node::update(
        &consumer_state,
        node.id,
        remote_node::UpdateRemoteNodeInput {
            base_url: Some(String::new()),
            transport_mode: Some(RemoteNodeTransportMode::ReverseTunnel),
            ..Default::default()
        },
    )
    .await
    .expect_err("node with presigned policies must not switch to reverse tunnel");
    assert!(
        error.message().contains("uses presigned"),
        "unexpected reverse switch error: {error}"
    );

    let still_direct = managed_follower_repo::find_by_id(consumer_state.writer_db(), node.id)
        .await
        .expect("remote node should still exist");
    assert_eq!(still_direct.transport_mode, RemoteNodeTransportMode::Direct);
    assert_eq!(still_direct.base_url, provider_server.base_url);

    provider_server.stop().await;
}

#[actix_web::test]
async fn test_remote_presigned_download_redirects_to_follower() {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;
    let provider_server = spawn_internal_storage_server(provider_state.follower_view()).await;

    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "provider-target".to_string(),
            base_url: provider_server.base_url.clone(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("consumer remote node should be created");
    let consumer_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), consumer_node.id)
            .await
            .expect("consumer remote node should be queryable");

    master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "consumer-access".to_string(),
            master_url: "http://master.example.com".to_string(),
            access_key: consumer_node_model.access_key.clone(),
            secret_key: consumer_node_model.secret_key.clone(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider master binding should be created");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(
        &provider_state,
        &consumer_node_model.access_key,
        &consumer_node_model.access_key,
    )
    .await;

    wait_for_remote_probe(&consumer_state, consumer_node.id).await;

    let remote_policy = create_remote_policy_with_options(
        &consumer_state,
        consumer_node.id,
        "Remote Presigned Download Policy",
        "presigned-download-base",
        StoragePolicyOptions {
            remote_download_strategy: Some(RemoteDownloadStrategy::Presigned),
            ..Default::default()
        },
        1024,
    )
    .await;

    let app = create_test_app!(consumer_state.clone());
    let (token, _) = register_and_login!(app);
    let user = user_repo::find_by_username(consumer_state.writer_db(), "testuser")
        .await
        .expect("test user lookup should succeed")
        .expect("test user should exist");
    let folder = folder::create(&consumer_state, user.id, "remote-presigned-download", None)
        .await
        .expect("remote folder should be created");
    common::bind_policy_to_folder(&consumer_state, folder.id, remote_policy.id).await;

    let body = b"hello remote presigned download".to_vec();
    let upload_path = write_temp_upload_file(
        &consumer_state,
        &format!("remote-presigned-download-{}.txt", uuid::Uuid::new_v4()),
        &body,
    )
    .await;
    let upload_path_string = upload_path.to_string_lossy().into_owned();
    let created = file::store_from_temp(
        &consumer_state,
        user.id,
        file::StoreFromTempRequest::new(
            Some(folder.id),
            "presigned-download.txt",
            &upload_path_string,
            i64::try_from(body.len()).expect("body length should fit i64"),
        ),
    )
    .await
    .expect("remote file upload should succeed");
    aster_drive::utils::cleanup_temp_file(&upload_path_string).await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{}/download", created.id))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
    assert_eq!(
        resp.headers()
            .get(actix_web::http::header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
    let location = resp
        .headers()
        .get(actix_web::http::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .expect("download redirect should contain Location")
        .to_string();
    assert!(
        location.starts_with(&provider_server.base_url),
        "remote download should redirect to the follower"
    );
    assert!(
        location.contains("response-content-disposition="),
        "presigned URL should preserve attachment filename"
    );
    assert!(
        location.contains("response-cache-control="),
        "presigned URL should preserve cache-control"
    );

    let response = reqwest::Client::new()
        .get(&location)
        .header(reqwest::header::ORIGIN, "http://master.example.com")
        .send()
        .await
        .expect("presigned remote download request should succeed");
    assert!(response.status().is_success());
    assert_eq!(
        response
            .headers()
            .get(reqwest::header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|value| value.to_str().ok()),
        Some("http://master.example.com")
    );
    assert_eq!(
        response
            .headers()
            .get(reqwest::header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
            .and_then(|value| value.to_str().ok()),
        Some("true")
    );
    assert!(
        response
            .headers()
            .get(reqwest::header::ACCESS_CONTROL_EXPOSE_HEADERS)
            .and_then(|value| value.to_str().ok())
            .expect("presigned remote download should expose response headers")
            .contains("Content-Disposition")
    );
    assert_eq!(
        response
            .headers()
            .get(reqwest::header::CONTENT_DISPOSITION)
            .and_then(|value| value.to_str().ok()),
        Some("attachment; filename*=UTF-8''presigned%2Ddownload.txt")
    );
    assert_eq!(
        response
            .headers()
            .get(reqwest::header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("private, max-age=0, must-revalidate")
    );
    assert_eq!(
        response
            .bytes()
            .await
            .expect("presigned remote download body should be readable")
            .as_ref(),
        body
    );

    provider_server.stop().await;
}

#[actix_web::test]
async fn test_disabling_remote_node_syncs_follower_binding_and_blocks_remote_use() {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;
    let provider_server = spawn_internal_storage_server(provider_state.follower_view()).await;

    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "provider-target".to_string(),
            base_url: provider_server.base_url.clone(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("consumer remote node should be created");
    let consumer_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), consumer_node.id)
            .await
            .expect("consumer remote node should be queryable");

    master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "consumer-access".to_string(),
            master_url: "http://master.example.com".to_string(),
            access_key: consumer_node_model.access_key.clone(),
            secret_key: consumer_node_model.secret_key.clone(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider master binding should be created");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(
        &provider_state,
        &consumer_node_model.access_key,
        &consumer_node_model.access_key,
    )
    .await;

    wait_for_remote_probe(&consumer_state, consumer_node.id).await;

    remote_node::update(
        &consumer_state,
        consumer_node.id,
        remote_node::UpdateRemoteNodeInput {
            is_enabled: Some(false),
            ..Default::default()
        },
    )
    .await
    .expect("disabling remote node should succeed");

    let provider_binding = master_binding_repo::find_by_access_key(
        provider_state.writer_db(),
        &consumer_node_model.access_key,
    )
    .await
    .expect("provider binding lookup should succeed")
    .expect("provider binding should still exist");
    assert!(!provider_binding.is_enabled);

    let forbidden_client = RemoteStorageClient::new(
        &provider_server.base_url,
        &consumer_node_model.access_key,
        &consumer_node_model.secret_key,
    )
    .expect("manual remote client should initialize");
    let probe_error = forbidden_client
        .probe_capabilities()
        .await
        .expect_err("disabled binding should reject signed internal requests");
    assert_eq!(probe_error.code(), "E060");
    assert!(probe_error.message().contains("master binding is disabled"));

    let create_error = policy::create(
        &consumer_state,
        policy::CreateStoragePolicyInput {
            name: "Disabled Remote Policy".to_string(),
            connection: policy::StoragePolicyConnectionInput {
                driver_type: DriverType::Remote,
                endpoint: String::new(),
                bucket: String::new(),
                access_key: String::new(),
                secret_key: String::new(),
                base_path: String::new(),
                remote_node_id: Some(consumer_node.id),
                remote_storage_target_key: None,
                options: Default::default(),
            },
            max_file_size: 0,
            chunk_size: None,
            is_default: false,
            allowed_types: Some(Vec::new()),
            options: Some(StoragePolicyOptions::default()),
            remote_storage_target_key: None,
            application_config: Default::default(),
        },
    )
    .await
    .expect_err("disabled remote nodes should not be bindable to remote policies");
    assert_eq!(create_error.code(), "E005");
    assert_eq!(
        create_error.api_error_code(),
        ApiErrorCode::PolicyRemoteNodeDisabled
    );
    assert!(create_error.message().contains("is disabled"));

    let remote_policy = create_remote_policy(
        &consumer_state,
        consumer_node.id,
        "Disabled Remote Policy",
        "disabled-base",
    )
    .await;
    let driver_error = match consumer_state.driver_registry.get_driver(&remote_policy) {
        Ok(_) => panic!("disabled remote nodes should not resolve into remote drivers"),
        Err(error) => error,
    };
    assert_eq!(driver_error.code(), "E060");
    assert!(driver_error.message().contains("is disabled"));

    provider_server.stop().await;
}

#[actix_web::test]
async fn test_saved_remote_node_connection_endpoint_returns_precondition_failed_for_disabled_binding()
 {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;
    let provider_server = spawn_internal_storage_server(provider_state.follower_view()).await;

    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "provider-target".to_string(),
            base_url: provider_server.base_url.clone(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("consumer remote node should be created");
    let consumer_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), consumer_node.id)
            .await
            .expect("consumer remote node should be queryable");

    master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "consumer-access".to_string(),
            master_url: "http://master.example.com".to_string(),
            access_key: consumer_node_model.access_key.clone(),
            secret_key: consumer_node_model.secret_key.clone(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider master binding should be created");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(
        &provider_state,
        &consumer_node_model.access_key,
        &consumer_node_model.access_key,
    )
    .await;

    wait_for_remote_probe(&consumer_state, consumer_node.id).await;

    master_binding::sync_from_primary(
        &provider_state.follower_view(),
        &consumer_node_model.access_key,
        master_binding::SyncMasterBindingInput {
            name: "consumer-access".to_string(),
            is_enabled: false,
        },
    )
    .await
    .expect("provider binding should disable cleanly");

    let app = create_test_app!(consumer_state.clone());
    let (admin_token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/admin/remote-nodes/{}/test",
            consumer_node.id
        ))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 412);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(
        body["msg"]
            .as_str()
            .unwrap()
            .contains("master binding is disabled")
    );

    provider_server.stop().await;
}

#[actix_web::test]
async fn test_disabled_remote_nodes_skip_network_during_health_checks() {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;
    let (provider_server, request_count) =
        spawn_counting_internal_storage_server(provider_state.follower_view()).await;

    let remote_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "disabled-health-check-target".to_string(),
            base_url: provider_server.base_url.clone(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: false,
        },
    )
    .await
    .expect("disabled remote node should be created");

    let stats = remote_node::run_health_tests(&consumer_state)
        .await
        .expect("health checks should finish");

    assert_eq!(stats.checked, 0);
    assert_eq!(stats.healthy, 0);
    assert_eq!(stats.failed, 0);
    assert_eq!(stats.skipped, 1);
    assert_eq!(
        request_count.load(Ordering::Relaxed),
        0,
        "disabled remote nodes should not send health-check traffic",
    );

    let remote_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), remote_node.id)
            .await
            .expect("disabled remote node should remain queryable");
    assert_eq!(remote_node_model.last_checked_at, None);

    provider_server.stop().await;
}

#[actix_web::test]
async fn test_pending_remote_nodes_skip_network_during_health_checks() {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;
    let (provider_server, request_count) =
        spawn_counting_internal_storage_server(provider_state.follower_view()).await;

    let remote_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "pending-health-check-target".to_string(),
            base_url: provider_server.base_url.clone(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("pending remote node should be created");

    let stats = remote_node::run_health_tests(&consumer_state)
        .await
        .expect("health checks should finish");

    assert_eq!(stats.checked, 0);
    assert_eq!(stats.healthy, 0);
    assert_eq!(stats.failed, 0);
    assert_eq!(stats.skipped, 1);
    assert_eq!(
        request_count.load(Ordering::Relaxed),
        0,
        "pending remote nodes should not send health-check traffic",
    );

    let remote_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), remote_node.id)
            .await
            .expect("pending remote node should remain queryable");
    assert_eq!(remote_node_model.last_checked_at, None);
    assert_eq!(
        remote_node_model.last_error, "",
        "pending remote nodes should not record probe failures",
    );

    provider_server.stop().await;
}

#[actix_web::test]
async fn test_pending_remote_node_connection_test_requires_completed_enrollment_before_network() {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;
    let (provider_server, request_count) =
        spawn_counting_internal_storage_server(provider_state.follower_view()).await;

    let remote_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "pending-connection-test-target".to_string(),
            base_url: provider_server.base_url.clone(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("pending remote node should be created");

    let error = remote_node::test_connection(&consumer_state, remote_node.id)
        .await
        .expect_err("pending remote node should reject connection tests");

    assert_eq!(
        error.message(),
        remote_node::REMOTE_NODE_ENROLLMENT_REQUIRED_MESSAGE,
    );
    assert_eq!(
        error.http_status(),
        actix_web::http::StatusCode::PRECONDITION_FAILED,
    );
    assert_eq!(
        request_count.load(Ordering::Relaxed),
        0,
        "pending remote node connection tests should not send network traffic",
    );

    let remote_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), remote_node.id)
            .await
            .expect("pending remote node should remain queryable");
    assert_eq!(remote_node_model.last_checked_at, None);
    assert_eq!(remote_node_model.last_error, "");

    provider_server.stop().await;
}

#[actix_web::test]
async fn test_remote_ingress_profiles_require_completed_enrollment_before_network() {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;
    let (provider_server, request_count) =
        spawn_counting_internal_storage_server(provider_state.follower_view()).await;

    let remote_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "pending-ingress-profile-target".to_string(),
            base_url: provider_server.base_url.clone(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("pending remote node should be created");

    let error = storage_target::list_remote(&consumer_state, remote_node.id)
        .await
        .expect_err("pending remote node should reject remote remote storage target reads");

    assert_eq!(
        error.message(),
        remote_node::REMOTE_NODE_ENROLLMENT_REQUIRED_MESSAGE,
    );
    assert_eq!(
        error.http_status(),
        actix_web::http::StatusCode::PRECONDITION_FAILED,
    );
    assert_eq!(
        request_count.load(Ordering::Relaxed),
        0,
        "pending remote remote storage target reads should not send network traffic",
    );

    provider_server.stop().await;
}

#[actix_web::test]
async fn test_health_checks_only_touch_enabled_remote_nodes_in_mixed_sets() {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;
    let (enabled_server, enabled_request_count) =
        spawn_counting_internal_storage_server(provider_state.follower_view()).await;
    let (disabled_server, disabled_request_count) =
        spawn_counting_internal_storage_server(provider_state.follower_view()).await;

    let enabled_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "enabled-health-check-target".to_string(),
            base_url: enabled_server.base_url.clone(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("enabled remote node should be created");
    let enabled_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), enabled_node.id)
            .await
            .expect("enabled remote node should be queryable");

    master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "enabled-health-check-access".to_string(),
            master_url: "http://master.example.com".to_string(),
            access_key: enabled_node_model.access_key.clone(),
            secret_key: enabled_node_model.secret_key.clone(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider binding for enabled node should be created");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(
        &provider_state,
        &enabled_node_model.access_key,
        &enabled_node_model.access_key,
    )
    .await;
    mark_remote_node_enrollment_completed(&consumer_state, enabled_node.id).await;

    let disabled_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "disabled-health-check-target".to_string(),
            base_url: disabled_server.base_url.clone(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: false,
        },
    )
    .await
    .expect("disabled remote node should be created");

    let stats = remote_node::run_health_tests(&consumer_state)
        .await
        .expect("mixed health checks should finish");

    assert_eq!(stats.checked, 1);
    assert_eq!(stats.healthy, 1);
    assert_eq!(stats.failed, 0);
    assert_eq!(stats.skipped, 1);
    assert_eq!(
        enabled_request_count.load(Ordering::Relaxed),
        2,
        "enabled remote node should sync binding and probe capabilities",
    );
    assert_eq!(
        disabled_request_count.load(Ordering::Relaxed),
        0,
        "disabled remote node should not send any health-check traffic",
    );

    let enabled_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), enabled_node.id)
            .await
            .expect("enabled remote node should remain queryable");
    let disabled_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), disabled_node.id)
            .await
            .expect("disabled remote node should remain queryable");
    assert!(enabled_node_model.last_checked_at.is_some());
    assert_eq!(disabled_node_model.last_checked_at, None);

    enabled_server.stop().await;
    disabled_server.stop().await;
}

#[actix_web::test]
async fn test_reverse_tunnel_remote_nodes_are_checked_by_health_tests_without_base_url() {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;
    let (provider_server, request_count) =
        spawn_counting_internal_storage_server(provider_state.follower_view()).await;

    let remote_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "reverse-health-check-target".to_string(),
            base_url: String::new(),
            transport_mode: RemoteNodeTransportMode::ReverseTunnel,
            is_enabled: true,
        },
    )
    .await
    .expect("reverse remote node should be created");
    let remote_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), remote_node.id)
            .await
            .expect("reverse remote node should be queryable");

    let (provider_binding, _) = master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "reverse-health-check-access".to_string(),
            master_url: "http://master.example.com".to_string(),
            access_key: remote_node_model.access_key.clone(),
            secret_key: remote_node_model.secret_key.clone(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider binding for reverse health check should be created");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(
        &provider_state,
        &provider_binding.access_key,
        &provider_binding.access_key,
    )
    .await;
    mark_remote_node_enrollment_completed(&consumer_state, remote_node.id).await;

    let (tunnel_shutdown, tunnel_handle) = start_test_reverse_tunnel_worker(
        consumer_state.clone(),
        remote_node_model,
        provider_server.base_url.clone(),
    )
    .await;

    let stats = remote_node::run_health_tests(&consumer_state)
        .await
        .expect("reverse health checks should finish through tunnel");

    assert_eq!(stats.checked, 1);
    assert_eq!(stats.healthy, 1);
    assert_eq!(stats.failed, 0);
    assert_eq!(stats.skipped, 0);
    assert_eq!(
        request_count.load(Ordering::Relaxed),
        2,
        "reverse tunnel health check should sync binding and probe capabilities",
    );

    let checked_node =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), remote_node.id)
            .await
            .expect("reverse remote node should remain queryable");
    assert!(checked_node.last_checked_at.is_some());
    assert_eq!(checked_node.last_error, "");

    stop_test_reverse_tunnel_worker(tunnel_shutdown, tunnel_handle).await;
    provider_server.stop().await;
}

#[actix_web::test]
async fn test_thumbnail_endpoint_returns_precondition_failed_when_remote_node_disabled() {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;
    let provider_server = spawn_internal_storage_server(provider_state.follower_view()).await;

    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "provider-target".to_string(),
            base_url: provider_server.base_url.clone(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("consumer remote node should be created");
    let consumer_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), consumer_node.id)
            .await
            .expect("consumer remote node should be queryable");

    master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "consumer-access".to_string(),
            master_url: "http://master.example.com".to_string(),
            access_key: consumer_node_model.access_key.clone(),
            secret_key: consumer_node_model.secret_key.clone(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider master binding should be created");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(
        &provider_state,
        &consumer_node_model.access_key,
        &consumer_node_model.access_key,
    )
    .await;

    wait_for_remote_probe(&consumer_state, consumer_node.id).await;

    let remote_policy = create_remote_policy(
        &consumer_state,
        consumer_node.id,
        "Remote Thumb Policy",
        "thumb-base",
    )
    .await;

    let app = create_test_app!(consumer_state.clone());
    let (token, _) = register_and_login!(app);
    let user = user_repo::find_by_username(consumer_state.writer_db(), "testuser")
        .await
        .expect("test user lookup should succeed")
        .expect("test user should exist");

    let folder = folder::create(&consumer_state, user.id, "remote-thumbs", None)
        .await
        .expect("remote folder should be created");
    common::bind_policy_to_folder(&consumer_state, folder.id, remote_policy.id).await;

    let png_bytes = build_test_png();
    let upload_path = write_temp_upload_file(
        &consumer_state,
        &format!("remote-thumb-{}.png", uuid::Uuid::new_v4()),
        &png_bytes,
    )
    .await;
    let upload_path_string = upload_path.to_string_lossy().into_owned();
    let created = file::store_from_temp(
        &consumer_state,
        user.id,
        file::StoreFromTempRequest::new(
            Some(folder.id),
            "thumb-source.png",
            &upload_path_string,
            i64::try_from(png_bytes.len()).expect("png size should fit i64"),
        ),
    )
    .await
    .expect("remote thumbnail source upload should succeed");
    aster_drive::utils::cleanup_temp_file(&upload_path_string).await;

    let created_file = file_repo::find_by_id(consumer_state.writer_db(), created.id)
        .await
        .expect("uploaded file should be queryable");
    let created_blob = file_repo::find_blob_by_id(consumer_state.writer_db(), created_file.blob_id)
        .await
        .expect("uploaded blob should be queryable");
    aster_drive::services::media::processing::generate_and_store_thumbnail(
        &consumer_state,
        &created_blob,
        &created_file.name,
        &created_file.mime_type,
    )
    .await
    .expect("thumbnail should generate while remote node is enabled");

    remote_node::update(
        &consumer_state,
        consumer_node.id,
        remote_node::UpdateRemoteNodeInput {
            is_enabled: Some(false),
            ..Default::default()
        },
    )
    .await
    .expect("disabling remote node should succeed");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{}/thumbnail", created.id))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 412);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["msg"].as_str().unwrap().contains("remote node #"));
    assert!(body["msg"].as_str().unwrap().contains("is disabled"));

    provider_server.stop().await;
}

#[actix_web::test]
async fn test_remote_presigned_upload_writes_directly_to_provider() {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;
    let provider_server = spawn_internal_storage_server(provider_state.follower_view()).await;

    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "provider-target".to_string(),
            base_url: provider_server.base_url.clone(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("consumer remote node should be created");
    let consumer_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), consumer_node.id)
            .await
            .expect("consumer remote node should be queryable");

    let (provider_binding, _) = master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "consumer-access".to_string(),
            master_url: "http://master.example.com".to_string(),
            access_key: consumer_node_model.access_key.clone(),
            secret_key: consumer_node_model.secret_key.clone(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider master binding should be created");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(
        &provider_state,
        &consumer_node_model.access_key,
        &consumer_node_model.access_key,
    )
    .await;

    wait_for_remote_probe(&consumer_state, consumer_node.id).await;

    let remote_policy = create_remote_policy_with_options(
        &consumer_state,
        consumer_node.id,
        "Remote Presigned Policy",
        "presigned-base",
        StoragePolicyOptions {
            remote_upload_strategy: Some(RemoteUploadStrategy::Presigned),
            ..Default::default()
        },
        1024,
    )
    .await;

    let app = create_test_app!(consumer_state.clone());
    let _ = register_and_login!(app);
    let user = user_repo::find_by_username(consumer_state.writer_db(), "testuser")
        .await
        .expect("test user lookup should succeed")
        .expect("test user should exist");
    let folder = folder::create(&consumer_state, user.id, "remote-presigned", None)
        .await
        .expect("remote folder should be created");
    common::bind_policy_to_folder(&consumer_state, folder.id, remote_policy.id).await;

    let body = b"presigned-remote-upload".to_vec();
    let init = upload_service::init_upload(
        &consumer_state,
        user.id,
        "presigned.bin",
        i64::try_from(body.len()).expect("body length should fit i64"),
        Some(folder.id),
        None,
    )
    .await
    .expect("remote presigned upload should initialize");
    assert_eq!(init.mode, aster_drive::types::UploadMode::Presigned);

    let upload_id = init
        .upload_id
        .expect("presigned mode should return upload id");
    let presigned_url = init
        .presigned_url
        .clone()
        .expect("presigned mode should return presigned_url");
    let response = put_presigned_bytes(&presigned_url, body.clone()).await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert!(
        response.headers().get(reqwest::header::ETAG).is_some(),
        "remote presigned upload should return ETag header"
    );
    let session = upload_session_repo::find_by_id(consumer_state.writer_db(), &upload_id)
        .await
        .expect("upload session should exist");
    let temp_key = session
        .object_temp_key
        .clone()
        .expect("remote presigned temp key should exist");
    let uploaded_temp_path = managed_ingress_object_path(
        &provider_state,
        &consumer_node_model.access_key,
        &provider_binding.storage_namespace,
        "",
        &format!("{}/{}", remote_policy.base_path.trim_matches('/'), temp_key),
    );
    let uploaded_temp = tokio::fs::read(&uploaded_temp_path)
        .await
        .expect("provider temp object should exist after presigned PUT");
    assert_eq!(uploaded_temp, body);
    let remote_driver = consumer_state
        .driver_registry
        .get_driver(&remote_policy)
        .expect("remote driver should be available");
    let remote_metadata = remote_driver
        .metadata(&temp_key)
        .await
        .expect("remote metadata should see uploaded temp object");
    assert_eq!(
        remote_metadata.size,
        u64::try_from(body.len()).expect("body length should fit u64")
    );

    let temp_dir = aster_drive::utils::paths::upload_temp_dir(
        &consumer_state.config.server.upload_temp_dir,
        &upload_id,
    );
    assert!(
        !tokio::fs::try_exists(&temp_dir)
            .await
            .expect("temp dir existence should be readable"),
        "single-file remote presigned upload should not create local chunk temp dir"
    );

    let created = upload_service::complete_upload(&consumer_state, &upload_id, user.id, None)
        .await
        .expect("remote presigned upload should complete");
    let created_file = file_repo::find_by_id(consumer_state.writer_db(), created.id)
        .await
        .expect("uploaded file should be queryable");
    let created_blob = file_repo::find_blob_by_id(consumer_state.writer_db(), created_file.blob_id)
        .await
        .expect("uploaded blob should be queryable");
    assert_ne!(
        created_blob.storage_path, temp_key,
        "completed remote presigned uploads must be copied away from the still-valid PUT key"
    );
    assert!(
        created_blob.hash.starts_with("remote-"),
        "remote presigned uploads should keep remote opaque blob identity"
    );
    assert!(
        !remote_driver
            .exists(&temp_key)
            .await
            .expect("remote temp key existence should be queryable"),
        "remote presigned temp key should be removed after final copy"
    );

    let stored_path = managed_ingress_object_path(
        &provider_state,
        &consumer_node_model.access_key,
        &provider_binding.storage_namespace,
        &remote_policy.base_path,
        &created_blob.storage_path,
    );
    let stored = tokio::fs::read(&stored_path)
        .await
        .expect("provider should receive direct presigned upload");
    assert_eq!(stored, body);

    provider_server.stop().await;
}

#[actix_web::test]
async fn test_force_delete_policy_cleans_late_remote_presigned_put_e2e() {
    use aster_drive::db::repository::background_task_repo;
    use aster_drive::entities::background_task;
    use aster_drive::services::task_service;
    use aster_drive::types::{BackgroundTaskKind, BackgroundTaskStatus};
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;
    let provider_server = spawn_internal_storage_server(provider_state.follower_view()).await;

    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "late-presigned-provider".to_string(),
            base_url: provider_server.base_url.clone(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("consumer remote node should be created");
    let consumer_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), consumer_node.id)
            .await
            .expect("consumer remote node should be queryable");

    let (provider_binding, _) = master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "late-presigned-consumer-access".to_string(),
            master_url: "http://master.example.com".to_string(),
            access_key: consumer_node_model.access_key.clone(),
            secret_key: consumer_node_model.secret_key.clone(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider master binding should be created");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(
        &provider_state,
        &consumer_node_model.access_key,
        &consumer_node_model.access_key,
    )
    .await;
    wait_for_remote_probe(&consumer_state, consumer_node.id).await;

    let remote_policy = create_remote_policy_with_options(
        &consumer_state,
        consumer_node.id,
        "Late Remote Presigned Cleanup",
        "late-presigned-base",
        StoragePolicyOptions {
            remote_upload_strategy: Some(RemoteUploadStrategy::Presigned),
            ..Default::default()
        },
        1024,
    )
    .await;

    let app = create_test_app!(consumer_state.clone());
    let _ = register_and_login!(app);
    let user = user_repo::find_by_username(consumer_state.writer_db(), "testuser")
        .await
        .expect("test user lookup should succeed")
        .expect("test user should exist");
    let folder = folder::create(&consumer_state, user.id, "late-remote-presigned", None)
        .await
        .expect("remote folder should be created");
    common::bind_policy_to_folder(&consumer_state, folder.id, remote_policy.id).await;

    let body = b"late remote presigned write after force delete".to_vec();
    let init = upload_service::init_upload(
        &consumer_state,
        user.id,
        "late-remote.bin",
        i64::try_from(body.len()).expect("body length should fit i64"),
        Some(folder.id),
        None,
    )
    .await
    .expect("remote presigned upload should initialize");
    assert_eq!(init.mode, aster_drive::types::UploadMode::Presigned);
    let upload_id = init
        .upload_id
        .expect("presigned mode should return upload id");
    let presigned_url = init
        .presigned_url
        .clone()
        .expect("presigned mode should return presigned_url");
    let session = upload_session_repo::find_by_id(consumer_state.writer_db(), &upload_id)
        .await
        .expect("upload session should exist");
    let temp_key = session
        .object_temp_key
        .clone()
        .expect("remote presigned temp key should exist");
    let provider_temp_path = managed_ingress_object_path(
        &provider_state,
        &consumer_node_model.access_key,
        &provider_binding.storage_namespace,
        "",
        &format!("{}/{}", remote_policy.base_path.trim_matches('/'), temp_key),
    );
    assert!(
        !tokio::fs::try_exists(&provider_temp_path)
            .await
            .expect("provider temp path existence should be readable"),
        "temp object should not exist before the late PUT"
    );

    policy::delete(&consumer_state, remote_policy.id, true)
        .await
        .expect("force deleting remote policy with pending presigned session should succeed");
    assert!(
        policy_repo::find_by_id(consumer_state.writer_db(), remote_policy.id)
            .await
            .is_err(),
        "remote policy should be deleted"
    );
    assert!(
        upload_session_repo::find_by_id(consumer_state.writer_db(), &upload_id)
            .await
            .is_err(),
        "force delete should remove the remote upload session before the old URL expires"
    );

    let response = put_presigned_bytes(&presigned_url, body.clone()).await;
    assert_eq!(
        response.status(),
        reqwest::StatusCode::OK,
        "old remote presigned URL should still accept PUT until it expires"
    );
    let uploaded = tokio::fs::read(&provider_temp_path)
        .await
        .expect("late PUT should create an orphan provider temp object");
    assert_eq!(uploaded, body);

    let cleanup_task = background_task::Entity::find()
        .filter(background_task::Column::Kind.eq(BackgroundTaskKind::StoragePolicyTempCleanup))
        .one(consumer_state.writer_db())
        .await
        .expect("cleanup task query should succeed")
        .expect("force delete should schedule delayed cleanup");
    assert_eq!(cleanup_task.status, BackgroundTaskStatus::Pending);
    let mut due_task: background_task::ActiveModel = cleanup_task.clone().into();
    due_task.next_run_at = Set(Utc::now() - ChronoDuration::seconds(1));
    due_task.update(consumer_state.writer_db()).await.unwrap();

    let stats = task_service::dispatch_due(&consumer_state)
        .await
        .expect("cleanup task dispatch should succeed");
    assert_eq!(stats.claimed, 1);
    assert_eq!(stats.succeeded, 1);
    assert!(
        !tokio::fs::try_exists(&provider_temp_path)
            .await
            .expect("provider temp path existence should be readable after cleanup"),
        "delayed cleanup should delete the late remote temp object"
    );
    let stored_task = background_task_repo::find_by_id(consumer_state.writer_db(), cleanup_task.id)
        .await
        .expect("cleanup task should remain queryable");
    assert_eq!(stored_task.status, BackgroundTaskStatus::Succeeded);

    provider_server.stop().await;
}

#[actix_web::test]
async fn test_remote_relay_stream_direct_upload_e2e() {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;
    let provider_server = spawn_internal_storage_server(provider_state.follower_view()).await;

    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "provider-target".to_string(),
            base_url: provider_server.base_url.clone(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("consumer remote node should be created");
    let consumer_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), consumer_node.id)
            .await
            .expect("consumer remote node should be queryable");

    let (provider_binding, _) = master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "consumer-access".to_string(),
            master_url: "http://master.example.com".to_string(),
            access_key: consumer_node_model.access_key.clone(),
            secret_key: consumer_node_model.secret_key.clone(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider master binding should be created");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(
        &provider_state,
        &consumer_node_model.access_key,
        &consumer_node_model.access_key,
    )
    .await;

    wait_for_remote_probe(&consumer_state, consumer_node.id).await;

    let remote_policy = create_remote_policy_with_options(
        &consumer_state,
        consumer_node.id,
        "Remote Relay Direct Policy",
        "relay-direct-base",
        StoragePolicyOptions {
            remote_upload_strategy: Some(RemoteUploadStrategy::RelayStream),
            ..Default::default()
        },
        1024,
    )
    .await;

    let user = local::register(
        &consumer_state,
        "remrelaydir",
        "remote-relay-direct@example.com",
        "pass1234",
    )
    .await
    .expect("consumer test user should be created");
    let folder = folder::create(&consumer_state, user.id, "remote-relay-direct", None)
        .await
        .expect("remote folder should be created");
    common::bind_policy_to_folder(&consumer_state, folder.id, remote_policy.id).await;

    let login = local::login(&consumer_state, "remrelaydir", "pass1234", None, None)
        .await
        .expect("consumer login should succeed");
    let login = common::expect_authenticated_login(login);
    common::seed_csrf_token(&login.access_token);

    let body = b"remote relay stream direct".to_vec();
    let init = upload_service::init_upload(
        &consumer_state,
        user.id,
        "relay-direct.bin",
        i64::try_from(body.len()).expect("body length should fit i64"),
        Some(folder.id),
        None,
    )
    .await
    .expect("remote relay direct upload should initialize");
    assert_eq!(init.mode, aster_drive::types::UploadMode::Direct);

    let temp_roots = vec![
        consumer_state.config.server.temp_dir.clone(),
        consumer_state.config.server.upload_temp_dir.clone(),
    ];
    let temp_snapshot_before = snapshot_temp_roots(&temp_roots).unwrap();
    let app = create_test_app!(consumer_state.clone());

    let (boundary, payload) = build_multipart_payload("relay-direct.bin", &body);
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/files/upload?folder_id={}&declared_size={}",
            folder.id,
            body.len()
        ))
        .insert_header(("Cookie", common::access_cookie_header(&login.access_token)))
        .insert_header(common::csrf_header_for(&login.access_token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let response_body: serde_json::Value = test::read_body_json(resp).await;
    let file_id = response_body["data"]["id"]
        .as_i64()
        .expect("upload response should contain file id");

    let temp_snapshot_after = snapshot_temp_roots(&temp_roots).unwrap();
    assert_eq!(
        temp_snapshot_after, temp_snapshot_before,
        "remote relay direct upload should not create local temp files or upload temp dirs"
    );

    let created_file = file_repo::find_by_id(consumer_state.writer_db(), file_id)
        .await
        .expect("uploaded file should be queryable");
    let created_blob = file_repo::find_blob_by_id(consumer_state.writer_db(), created_file.blob_id)
        .await
        .expect("uploaded blob should be queryable");
    assert!(created_blob.hash.starts_with("remote-"));
    assert!(created_blob.storage_path.starts_with("files/"));

    let provider_path = managed_ingress_object_path(
        &provider_state,
        &consumer_node_model.access_key,
        &provider_binding.storage_namespace,
        &remote_policy.base_path,
        &created_blob.storage_path,
    );
    let stored = tokio::fs::read(&provider_path)
        .await
        .expect("provider should receive direct relay upload");
    assert_eq!(stored, body);

    provider_server.stop().await;
}

#[actix_web::test]
async fn test_remote_presigned_upload_browser_cors_follows_bound_master_origin() {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;

    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "provider-target".to_string(),
            base_url: "http://provider.example.com".to_string(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("consumer remote node should be created");
    let consumer_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), consumer_node.id)
            .await
            .expect("consumer remote node should be queryable");

    master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "consumer-access".to_string(),
            master_url: "http://localhost:3000".to_string(),
            access_key: consumer_node_model.access_key.clone(),
            secret_key: consumer_node_model.secret_key.clone(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider master binding should be created");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(
        &provider_state,
        &consumer_node_model.access_key,
        &consumer_node_model.access_key,
    )
    .await;
    seed_remote_capabilities(
        &consumer_state,
        consumer_node.id,
        RemoteStorageCapabilities::current(),
    )
    .await;

    let remote_policy = create_remote_policy_with_options(
        &consumer_state,
        consumer_node.id,
        "Remote Presigned Browser CORS Policy",
        "browser-cors-base",
        StoragePolicyOptions {
            remote_upload_strategy: Some(RemoteUploadStrategy::Presigned),
            ..Default::default()
        },
        1024,
    )
    .await;

    let app = create_test_app!(consumer_state.clone());
    let _ = register_and_login!(app);
    let user = user_repo::find_by_username(consumer_state.writer_db(), "testuser")
        .await
        .expect("test user lookup should succeed")
        .expect("test user should exist");
    let folder = folder::create(&consumer_state, user.id, "remote-browser-cors", None)
        .await
        .expect("remote folder should be created");
    common::bind_policy_to_folder(&consumer_state, folder.id, remote_policy.id).await;

    let body = b"presigned-browser-cors".to_vec();
    let init = upload_service::init_upload(
        &consumer_state,
        user.id,
        "presigned-browser.bin",
        i64::try_from(body.len()).expect("body length should fit i64"),
        Some(folder.id),
        None,
    )
    .await
    .expect("remote presigned upload should initialize");
    let presigned_url = init
        .presigned_url
        .expect("presigned mode should return presigned_url");
    let presigned_path = path_and_query_from_url(&presigned_url);

    let follower_app = test::init_service(
        App::new()
            .app_data(web::Data::new(provider_state.follower_view()))
            .service(
                web::scope("/api/v1").service(aster_drive::api::routes::internal_storage::routes()),
            ),
    )
    .await;

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri(&presigned_path)
        .insert_header(("Origin", "http://localhost:3000"))
        .insert_header(("Access-Control-Request-Method", "PUT"))
        .insert_header(("Access-Control-Request-Headers", "content-type"))
        .to_request();
    let resp = test::call_service(&follower_app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NO_CONTENT);
    assert_eq!(
        resp.headers()
            .get(actix_web::http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:3000")
    );
    assert_eq!(
        resp.headers()
            .get(actix_web::http::header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
            .and_then(|value| value.to_str().ok()),
        Some("true")
    );
    assert_eq!(
        resp.headers()
            .get(actix_web::http::header::ACCESS_CONTROL_ALLOW_METHODS)
            .and_then(|value| value.to_str().ok()),
        Some("GET, PUT, OPTIONS")
    );
    assert_eq!(
        resp.headers()
            .get(actix_web::http::header::ACCESS_CONTROL_ALLOW_HEADERS)
            .and_then(|value| value.to_str().ok()),
        Some("content-type, range")
    );
    let vary = resp
        .headers()
        .get(actix_web::http::header::VARY)
        .and_then(|value| value.to_str().ok())
        .expect("preflight response should include Vary");
    assert!(vary.contains("Origin"));
    assert!(vary.contains("Access-Control-Request-Method"));
    assert!(vary.contains("Access-Control-Request-Headers"));

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri(&presigned_path)
        .insert_header(("Origin", "http://evil.example.com"))
        .insert_header(("Access-Control-Request-Method", "PUT"))
        .insert_header(("Access-Control-Request-Headers", "content-type"))
        .to_request();
    let resp = test::call_service(&follower_app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri(&presigned_path)
        .insert_header(("Origin", "http://localhost:3000"))
        .insert_header(("Access-Control-Request-Method", "DELETE"))
        .insert_header(("Access-Control-Request-Headers", "content-type"))
        .to_request();
    let resp = test::call_service(&follower_app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri(&presigned_path)
        .insert_header(("Origin", "http://localhost:3000"))
        .insert_header(("Access-Control-Request-Method", "PUT"))
        .insert_header(("Access-Control-Request-Headers", "content-type, x-extra"))
        .to_request();
    let resp = test::call_service(&follower_app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);

    let req = test::TestRequest::put()
        .uri(&presigned_path)
        .insert_header(("Origin", "http://evil.example.com"))
        .insert_header((
            actix_web::http::header::CONTENT_TYPE,
            "application/octet-stream",
        ))
        .insert_header((
            actix_web::http::header::CONTENT_LENGTH,
            body.len().to_string(),
        ))
        .set_payload(body.clone())
        .to_request();
    let resp = test::call_service(&follower_app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);

    let req = test::TestRequest::put()
        .uri(&presigned_path)
        .insert_header(("Origin", "http://localhost:3000"))
        .insert_header((
            actix_web::http::header::CONTENT_TYPE,
            "application/octet-stream",
        ))
        .insert_header((
            actix_web::http::header::CONTENT_LENGTH,
            body.len().to_string(),
        ))
        .set_payload(body)
        .to_request();
    let resp = test::call_service(&follower_app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get(actix_web::http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:3000")
    );
    assert_eq!(
        resp.headers()
            .get(actix_web::http::header::ACCESS_CONTROL_EXPOSE_HEADERS)
            .and_then(|value| value.to_str().ok()),
        Some("ETag")
    );
    let vary = resp
        .headers()
        .get(actix_web::http::header::VARY)
        .and_then(|value| value.to_str().ok())
        .expect("actual PUT response should include Vary");
    assert!(vary.contains("Origin"));
    assert!(
        resp.headers().contains_key(actix_web::http::header::ETAG),
        "browser PUT should expose ETag header"
    );
}

#[actix_web::test]
async fn test_remote_presigned_download_browser_cors_allows_get() {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;
    let provider_server = spawn_internal_storage_server(provider_state.follower_view()).await;

    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "provider-target".to_string(),
            base_url: provider_server.base_url.clone(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("consumer remote node should be created");
    let consumer_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), consumer_node.id)
            .await
            .expect("consumer remote node should be queryable");

    master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "consumer-access".to_string(),
            master_url: "http://localhost:3000".to_string(),
            access_key: consumer_node_model.access_key.clone(),
            secret_key: consumer_node_model.secret_key.clone(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider master binding should be created");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(
        &provider_state,
        &consumer_node_model.access_key,
        &consumer_node_model.access_key,
    )
    .await;
    seed_remote_capabilities(
        &consumer_state,
        consumer_node.id,
        RemoteStorageCapabilities::current(),
    )
    .await;

    let remote_policy = create_remote_policy_with_options(
        &consumer_state,
        consumer_node.id,
        "Remote Presigned Browser Download CORS Policy",
        "browser-download-cors-base",
        StoragePolicyOptions {
            remote_download_strategy: Some(RemoteDownloadStrategy::Presigned),
            ..Default::default()
        },
        1024,
    )
    .await;

    let app = create_test_app!(consumer_state.clone());
    let _ = register_and_login!(app);
    let user = user_repo::find_by_username(consumer_state.writer_db(), "testuser")
        .await
        .expect("test user lookup should succeed")
        .expect("test user should exist");
    let folder = folder::create(
        &consumer_state,
        user.id,
        "remote-browser-download-cors",
        None,
    )
    .await
    .expect("remote folder should be created");
    common::bind_policy_to_folder(&consumer_state, folder.id, remote_policy.id).await;

    let body = b"presigned-download-browser-cors".to_vec();
    let upload_path = write_temp_upload_file(
        &consumer_state,
        &format!(
            "remote-presigned-browser-download-cors-{}.txt",
            uuid::Uuid::new_v4()
        ),
        &body,
    )
    .await;
    let upload_path_string = upload_path.to_string_lossy().into_owned();
    let created = file::store_from_temp(
        &consumer_state,
        user.id,
        file::StoreFromTempRequest::new(
            Some(folder.id),
            "presigned-browser-download.txt",
            &upload_path_string,
            i64::try_from(body.len()).expect("body length should fit i64"),
        ),
    )
    .await
    .expect("remote file upload should succeed");
    aster_drive::utils::cleanup_temp_file(&upload_path_string).await;

    let download_result = file::download(&consumer_state, created.id, user.id, None)
        .await
        .expect("remote presigned download should resolve");
    let presigned_path = match download_result {
        file::DownloadOutcome::PresignedRedirect { url, .. } => path_and_query_from_url(&url),
        other => panic!("expected remote presigned download, got {other:?}"),
    };

    let follower_app = test::init_service(
        App::new()
            .app_data(web::Data::new(provider_state.follower_view()))
            .service(
                web::scope("/api/v1").service(aster_drive::api::routes::internal_storage::routes()),
            ),
    )
    .await;

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri(&presigned_path)
        .insert_header(("Origin", "http://localhost:3000"))
        .insert_header(("Access-Control-Request-Method", "GET"))
        .insert_header(("Access-Control-Request-Headers", "range"))
        .to_request();
    let resp = test::call_service(&follower_app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NO_CONTENT);
    assert_eq!(
        resp.headers()
            .get(actix_web::http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:3000")
    );
    assert_eq!(
        resp.headers()
            .get(actix_web::http::header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
            .and_then(|value| value.to_str().ok()),
        Some("true")
    );
    assert_eq!(
        resp.headers()
            .get(actix_web::http::header::ACCESS_CONTROL_ALLOW_METHODS)
            .and_then(|value| value.to_str().ok()),
        Some("GET, PUT, OPTIONS")
    );
    assert_eq!(
        resp.headers()
            .get(actix_web::http::header::ACCESS_CONTROL_ALLOW_HEADERS)
            .and_then(|value| value.to_str().ok()),
        Some("content-type, range")
    );

    let req = test::TestRequest::get()
        .uri(&presigned_path)
        .insert_header(("Origin", "http://localhost:3000"))
        .to_request();
    let resp = test::call_service(&follower_app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get(actix_web::http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:3000")
    );
    assert_eq!(
        resp.headers()
            .get(actix_web::http::header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
            .and_then(|value| value.to_str().ok()),
        Some("true")
    );
    let exposed_headers = resp
        .headers()
        .get(actix_web::http::header::ACCESS_CONTROL_EXPOSE_HEADERS)
        .and_then(|value| value.to_str().ok())
        .expect("actual GET response should expose download headers");
    assert!(exposed_headers.contains("Content-Disposition"));
    assert!(exposed_headers.contains("Content-Length"));
    assert!(exposed_headers.contains("Content-Range"));
    assert!(exposed_headers.contains("Content-Type"));
    assert!(exposed_headers.contains("Accept-Ranges"));
    let vary = resp
        .headers()
        .get(actix_web::http::header::VARY)
        .and_then(|value| value.to_str().ok())
        .expect("actual GET response should include Vary");
    assert!(vary.contains("Origin"));
    let downloaded = test::read_body(resp).await;
    assert_eq!(downloaded.as_ref(), body);

    provider_server.stop().await;
}

#[actix_web::test]
async fn test_internal_storage_get_honors_range_header() {
    let provider_state = common::setup().await;
    let access_key = "range-header-access-key";
    let secret_key = "range-header-secret-key";
    let object_key = "range-header.bin";
    let body = b"0123456789abcdef";

    master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "range-header-binding".to_string(),
            master_url: "http://master.example.com".to_string(),
            access_key: access_key.to_string(),
            secret_key: secret_key.to_string(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider binding should be created");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(&provider_state, access_key, access_key).await;
    let binding = master_binding_repo::find_by_access_key(provider_state.writer_db(), access_key)
        .await
        .expect("provider binding lookup should succeed")
        .expect("provider binding should exist");
    let storage_path = managed_ingress_object_path(
        &provider_state,
        access_key,
        &binding.storage_namespace,
        "",
        object_key,
    );
    tokio::fs::create_dir_all(
        storage_path
            .parent()
            .expect("provider object path should have parent"),
    )
    .await
    .expect("provider object parent should be created");
    tokio::fs::write(&storage_path, body)
        .await
        .expect("provider object should be written");

    let follower_app = test::init_service(
        App::new()
            .app_data(web::Data::new(provider_state.follower_view()))
            .service(
                web::scope("/api/v1").service(aster_drive::api::routes::internal_storage::routes()),
            ),
    )
    .await;

    let path = format!("/api/v1/internal/storage/objects/{object_key}");
    let timestamp = Utc::now().timestamp();
    let nonce = "range-header-test";
    let signature = sign_internal_request(secret_key, "GET", &path, timestamp, nonce, None);
    let req = test::TestRequest::get()
        .uri(&path)
        .insert_header((actix_web::http::header::RANGE, "bytes=4-8"))
        .insert_header(("x-aster-access-key", access_key))
        .insert_header(("x-aster-timestamp", timestamp.to_string()))
        .insert_header(("x-aster-nonce", nonce))
        .insert_header(("x-aster-signature", signature))
        .to_request();
    let resp = test::call_service(&follower_app, req).await;

    assert_eq!(resp.status(), actix_web::http::StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        resp.headers()
            .get(actix_web::http::header::CONTENT_RANGE)
            .and_then(|value| value.to_str().ok()),
        Some("bytes 4-8/16")
    );
    assert_eq!(
        resp.headers()
            .get(actix_web::http::header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok()),
        Some("5")
    );
    assert_eq!(
        resp.headers()
            .get(actix_web::http::header::ACCEPT_RANGES)
            .and_then(|value| value.to_str().ok()),
        Some("bytes")
    );
    let downloaded = test::read_body(resp).await;
    assert_eq!(downloaded.as_ref(), b"45678");
}

#[actix_web::test]
async fn test_remote_relay_stream_chunked_upload_e2e() {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;
    let provider_server = spawn_internal_storage_server(provider_state.follower_view()).await;

    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "provider-target".to_string(),
            base_url: provider_server.base_url.clone(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("consumer remote node should be created");
    let consumer_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), consumer_node.id)
            .await
            .expect("consumer remote node should be queryable");

    let (provider_binding, _) = master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "consumer-access".to_string(),
            master_url: "http://master.example.com".to_string(),
            access_key: consumer_node_model.access_key.clone(),
            secret_key: consumer_node_model.secret_key.clone(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider master binding should be created");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(
        &provider_state,
        &consumer_node_model.access_key,
        &consumer_node_model.access_key,
    )
    .await;

    wait_for_remote_probe(&consumer_state, consumer_node.id).await;

    let remote_policy = create_remote_policy_with_options(
        &consumer_state,
        consumer_node.id,
        "Remote Relay Chunked Policy",
        "relay-chunked-base",
        StoragePolicyOptions {
            remote_upload_strategy: Some(RemoteUploadStrategy::RelayStream),
            ..Default::default()
        },
        4,
    )
    .await;

    let user = local::register(
        &consumer_state,
        "remrelaych",
        "remote-relay-chunked@example.com",
        "pass1234",
    )
    .await
    .expect("consumer test user should be created");
    let login = local::login(&consumer_state, "remrelaych", "pass1234", None, None)
        .await
        .expect("consumer test user should log in");
    let login = common::expect_authenticated_login(login);
    common::seed_csrf_token(&login.access_token);
    let folder = folder::create(&consumer_state, user.id, "remote-relay-chunked", None)
        .await
        .expect("remote folder should be created");
    common::bind_policy_to_folder(&consumer_state, folder.id, remote_policy.id).await;

    let body = b"remote-relay-chunked-upload".to_vec();
    let init = upload_service::init_upload(
        &consumer_state,
        user.id,
        "relay-chunked.bin",
        i64::try_from(body.len()).expect("body length should fit i64"),
        Some(folder.id),
        None,
    )
    .await
    .expect("remote relay chunked upload should initialize");
    assert_eq!(init.mode, aster_drive::types::UploadMode::Chunked);
    assert_eq!(init.chunk_size, Some(4));

    let app = create_test_app!(consumer_state.clone());
    let upload_id = init
        .upload_id
        .clone()
        .expect("chunked mode should return upload id");
    let session = upload_session_repo::find_by_id(consumer_state.writer_db(), &upload_id)
        .await
        .expect("upload session should exist");
    let remote_multipart_id = session
        .object_multipart_id
        .clone()
        .expect("remote relay multipart upload id should be stored");
    let chunk_size = usize::try_from(
        init.chunk_size
            .expect("chunked mode should return chunk size"),
    )
    .expect("chunk size should fit usize");
    let total_chunks = usize::try_from(
        init.total_chunks
            .expect("chunked mode should return total_chunks"),
    )
    .expect("total chunks should fit usize");

    let temp_dir = aster_drive::utils::paths::upload_temp_dir(
        &consumer_state.config.server.upload_temp_dir,
        &upload_id,
    );
    let assembled_path = aster_drive::utils::paths::upload_assembled_path(
        &consumer_state.config.server.upload_temp_dir,
        &upload_id,
    );
    assert!(
        !tokio::fs::try_exists(&temp_dir)
            .await
            .expect("temp dir existence should be readable"),
        "remote relay multipart should not create local upload temp dir"
    );
    assert!(
        upload_session_part_repo::list_by_upload(consumer_state.writer_db(), &upload_id)
            .await
            .expect("multipart parts should be queryable")
            .is_empty()
    );

    let oversized_init = upload_service::init_upload(
        &consumer_state,
        user.id,
        "relay-chunked-oversized.bin",
        5,
        Some(folder.id),
        None,
    )
    .await
    .expect("remote relay oversized upload should initialize");
    let oversized_upload_id = oversized_init
        .upload_id
        .expect("chunked mode should return upload id");
    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/upload/{oversized_upload_id}/0"))
        .insert_header(("Cookie", common::access_cookie_header(&login.access_token)))
        .insert_header(common::csrf_header_for(&login.access_token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(b"12345".to_vec())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::PAYLOAD_TOO_LARGE
    );
    let error_body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(error_body["code"], "upload.chunk_too_large");
    assert_eq!(error_body["error"]["retryable"], false);
    assert!(error_body["error"].get("internal_code").is_none());
    assert!(error_body["error"].get("subcode").is_none());
    assert!(
        upload_session_part_repo::list_by_upload(consumer_state.writer_db(), &oversized_upload_id)
            .await
            .expect("multipart parts should be queryable")
            .is_empty(),
        "oversized remote relay chunk must release the claimed part row"
    );
    let oversized_progress =
        upload_service::get_progress(&consumer_state, &oversized_upload_id, user.id)
            .await
            .expect("remote relay oversized progress should be queryable");
    assert!(oversized_progress.chunks_on_disk.is_empty());

    let first_chunk_end = std::cmp::min(chunk_size, body.len());
    let first_chunk = body[..first_chunk_end].to_vec();
    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/upload/{upload_id}/0"))
        .insert_header(("Cookie", common::access_cookie_header(&login.access_token)))
        .insert_header(common::csrf_header_for(&login.access_token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(first_chunk.clone())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let first: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(first["data"]["received_count"], 1);

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/upload/{upload_id}/0"))
        .insert_header(("Cookie", common::access_cookie_header(&login.access_token)))
        .insert_header(common::csrf_header_for(&login.access_token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(first_chunk.clone())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let duplicate: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(duplicate["data"]["received_count"], 1);

    let progress = upload_service::get_progress(&consumer_state, &upload_id, user.id)
        .await
        .expect("remote relay upload progress should be queryable");
    assert_eq!(progress.chunks_on_disk, vec![0]);

    let parts = upload_session_part_repo::list_by_upload(consumer_state.writer_db(), &upload_id)
        .await
        .expect("remote relay multipart parts should be queryable");
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].part_number, 1);
    assert_eq!(parts[0].size, i64::try_from(first_chunk.len()).unwrap());

    for chunk_number in 1..total_chunks {
        let start = chunk_number * chunk_size;
        let end = std::cmp::min(start + chunk_size, body.len());
        let req = test::TestRequest::put()
            .uri(&format!(
                "/api/v1/files/upload/{upload_id}/{}",
                i32::try_from(chunk_number).expect("chunk number should fit i32")
            ))
            .insert_header(("Cookie", common::access_cookie_header(&login.access_token)))
            .insert_header(common::csrf_header_for(&login.access_token))
            .insert_header(("Content-Type", "application/octet-stream"))
            .set_payload(body[start..end].to_vec())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    }

    let progress = upload_service::get_progress(&consumer_state, &upload_id, user.id)
        .await
        .expect("completed remote relay upload progress should be queryable");
    assert_eq!(
        progress.chunks_on_disk,
        (0..i32::try_from(total_chunks).expect("chunk count should fit i32")).collect::<Vec<_>>()
    );

    let created = upload_service::complete_upload(&consumer_state, &upload_id, user.id, None)
        .await
        .expect("remote relay multipart upload should complete");
    let created_file = file_repo::find_by_id(consumer_state.writer_db(), created.id)
        .await
        .expect("uploaded file should be queryable");
    let created_blob = file_repo::find_blob_by_id(consumer_state.writer_db(), created_file.blob_id)
        .await
        .expect("uploaded blob should be queryable");
    assert_eq!(created_blob.storage_path, format!("files/{upload_id}"));

    let stored_path = managed_ingress_object_path(
        &provider_state,
        &consumer_node_model.access_key,
        &provider_binding.storage_namespace,
        &remote_policy.base_path,
        &created_blob.storage_path,
    );
    let stored = tokio::fs::read(&stored_path)
        .await
        .expect("provider should receive remote relay multipart upload");
    assert_eq!(stored, body);

    assert!(
        !tokio::fs::try_exists(&temp_dir)
            .await
            .expect("temp dir existence should be readable"),
        "remote relay multipart should never create local chunk temp dir"
    );
    assert!(
        !tokio::fs::try_exists(&assembled_path)
            .await
            .expect("assembled path existence should be readable"),
        "remote relay multipart should never create local assembled temp file"
    );

    let first_part_path = managed_ingress_object_path(
        &provider_state,
        &consumer_node_model.access_key,
        &provider_binding.storage_namespace,
        &remote_policy.base_path,
        &format!("uploads/{remote_multipart_id}/parts/1"),
    );
    assert!(
        !tokio::fs::try_exists(&first_part_path)
            .await
            .expect("part path existence should be readable"),
        "remote relay multipart compose should cleanup follower temp parts"
    );

    provider_server.stop().await;
}

#[actix_web::test]
async fn test_remote_presigned_upload_browser_cors_accepts_master_url_with_path_and_port() {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;

    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "provider-target".to_string(),
            base_url: "http://provider.example.com".to_string(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("consumer remote node should be created");
    let consumer_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), consumer_node.id)
            .await
            .expect("consumer remote node should be queryable");

    master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "consumer-access".to_string(),
            master_url: "http://localhost:3000/admin/settings/site".to_string(),
            access_key: consumer_node_model.access_key.clone(),
            secret_key: consumer_node_model.secret_key.clone(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider master binding should be created");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(
        &provider_state,
        &consumer_node_model.access_key,
        &consumer_node_model.access_key,
    )
    .await;
    seed_remote_capabilities(
        &consumer_state,
        consumer_node.id,
        RemoteStorageCapabilities::current(),
    )
    .await;

    let remote_policy = create_remote_policy_with_options(
        &consumer_state,
        consumer_node.id,
        "Remote Presigned Browser Origin Path Policy",
        "browser-origin-path-base",
        StoragePolicyOptions {
            remote_upload_strategy: Some(RemoteUploadStrategy::Presigned),
            ..Default::default()
        },
        1024,
    )
    .await;

    let app = create_test_app!(consumer_state.clone());
    let _ = register_and_login!(app);
    let user = user_repo::find_by_username(consumer_state.writer_db(), "testuser")
        .await
        .expect("test user lookup should succeed")
        .expect("test user should exist");
    let folder = folder::create(&consumer_state, user.id, "remote-browser-origin-path", None)
        .await
        .expect("remote folder should be created");
    common::bind_policy_to_folder(&consumer_state, folder.id, remote_policy.id).await;

    let init = upload_service::init_upload(
        &consumer_state,
        user.id,
        "presigned-origin-path.bin",
        32,
        Some(folder.id),
        None,
    )
    .await
    .expect("remote presigned upload should initialize");
    let presigned_path = path_and_query_from_url(
        &init
            .presigned_url
            .expect("presigned mode should return presigned_url"),
    );

    let follower_app = test::init_service(
        App::new()
            .app_data(web::Data::new(provider_state.follower_view()))
            .service(
                web::scope("/api/v1").service(aster_drive::api::routes::internal_storage::routes()),
            ),
    )
    .await;

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri(&presigned_path)
        .insert_header(("Origin", "http://localhost:3000"))
        .insert_header(("Access-Control-Request-Method", "PUT"))
        .insert_header(("Access-Control-Request-Headers", "content-type"))
        .to_request();
    let resp = test::call_service(&follower_app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NO_CONTENT);
    assert_eq!(
        resp.headers()
            .get(actix_web::http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:3000")
    );
}

#[actix_web::test]
async fn test_remote_presigned_upload_browser_cors_rejects_disabled_binding() {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;

    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "provider-target".to_string(),
            base_url: "http://provider.example.com".to_string(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("consumer remote node should be created");
    let consumer_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), consumer_node.id)
            .await
            .expect("consumer remote node should be queryable");

    let binding = master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "consumer-access".to_string(),
            master_url: "http://localhost:3000".to_string(),
            access_key: consumer_node_model.access_key.clone(),
            secret_key: consumer_node_model.secret_key.clone(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider master binding should be created")
    .0;
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(
        &provider_state,
        &binding.access_key,
        &binding.access_key,
    )
    .await;
    seed_remote_capabilities(
        &consumer_state,
        consumer_node.id,
        RemoteStorageCapabilities::current(),
    )
    .await;

    let remote_policy = create_remote_policy_with_options(
        &consumer_state,
        consumer_node.id,
        "Remote Presigned Disabled Binding Policy",
        "browser-disabled-binding-base",
        StoragePolicyOptions {
            remote_upload_strategy: Some(RemoteUploadStrategy::Presigned),
            ..Default::default()
        },
        1024,
    )
    .await;

    let app = create_test_app!(consumer_state.clone());
    let _ = register_and_login!(app);
    let user = user_repo::find_by_username(consumer_state.writer_db(), "testuser")
        .await
        .expect("test user lookup should succeed")
        .expect("test user should exist");
    let folder = folder::create(
        &consumer_state,
        user.id,
        "remote-browser-disabled-binding",
        None,
    )
    .await
    .expect("remote folder should be created");
    common::bind_policy_to_folder(&consumer_state, folder.id, remote_policy.id).await;

    let init = upload_service::init_upload(
        &consumer_state,
        user.id,
        "presigned-disabled-binding.bin",
        32,
        Some(folder.id),
        None,
    )
    .await
    .expect("remote presigned upload should initialize");
    let presigned_path = path_and_query_from_url(
        &init
            .presigned_url
            .expect("presigned mode should return presigned_url"),
    );

    let follower_state = provider_state.follower_view();
    master_binding::sync_from_primary(
        &follower_state,
        &binding.access_key,
        master_binding::SyncMasterBindingInput {
            name: binding.name.clone(),
            is_enabled: false,
        },
    )
    .await
    .expect("binding should be disabled");

    let follower_app = test::init_service(
        App::new()
            .app_data(web::Data::new(provider_state.follower_view()))
            .service(
                web::scope("/api/v1").service(aster_drive::api::routes::internal_storage::routes()),
            ),
    )
    .await;

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri(&presigned_path)
        .insert_header(("Origin", "http://localhost:3000"))
        .insert_header(("Access-Control-Request-Method", "PUT"))
        .insert_header(("Access-Control-Request-Headers", "content-type"))
        .to_request();
    let resp = test::call_service(&follower_app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn test_remote_presigned_upload_browser_cors_passthrough_without_origin_header() {
    let fixture = setup_browser_presigned_cors_fixture(
        "provider-browser-no-origin-space",
        "http://localhost:3000",
    )
    .await;
    let follower_app = test::init_service(
        App::new()
            .app_data(web::Data::new(fixture.provider_state.follower_view()))
            .service(
                web::scope("/api/v1").service(aster_drive::api::routes::internal_storage::routes()),
            ),
    )
    .await;

    let body = b"presigned-no-origin".to_vec();
    let req = test::TestRequest::put()
        .uri(&fixture.presigned_path)
        .insert_header((
            actix_web::http::header::CONTENT_TYPE,
            "application/octet-stream",
        ))
        .insert_header((
            actix_web::http::header::CONTENT_LENGTH,
            body.len().to_string(),
        ))
        .set_payload(body)
        .to_request();
    let resp = test::call_service(&follower_app, req).await;

    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    assert!(
        !resp
            .headers()
            .contains_key(actix_web::http::header::ACCESS_CONTROL_ALLOW_ORIGIN),
        "requests without Origin should bypass browser CORS decoration"
    );
    assert!(
        !resp
            .headers()
            .contains_key(actix_web::http::header::ACCESS_CONTROL_EXPOSE_HEADERS),
        "non-browser requests should not receive expose-headers decoration"
    );
    assert!(
        resp.headers().contains_key(actix_web::http::header::ETAG),
        "plain presigned PUT should still return ETag"
    );
}

#[actix_web::test]
async fn test_remote_presigned_upload_browser_cors_missing_access_key_passthroughs_to_auth_error() {
    let fixture = setup_browser_presigned_cors_fixture(
        "provider-browser-missing-access-key-space",
        "http://localhost:3000",
    )
    .await;
    let presigned_path =
        rewrite_path_query_param(&fixture.presigned_path, "aster_access_key", None);
    let follower_app = test::init_service(
        App::new()
            .app_data(web::Data::new(fixture.provider_state.follower_view()))
            .service(
                web::scope("/api/v1").service(aster_drive::api::routes::internal_storage::routes()),
            ),
    )
    .await;

    let req = test::TestRequest::put()
        .uri(&presigned_path)
        .insert_header(("Origin", "http://localhost:3000"))
        .insert_header((
            actix_web::http::header::CONTENT_TYPE,
            "application/octet-stream",
        ))
        .insert_header((actix_web::http::header::CONTENT_LENGTH, "4"))
        .set_payload(b"test".as_slice())
        .to_request();
    let resp = test::call_service(&follower_app, req).await;

    assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);
    assert!(
        !resp
            .headers()
            .contains_key(actix_web::http::header::ACCESS_CONTROL_ALLOW_ORIGIN),
        "middleware should leave missing access_key requests to auth layer"
    );
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "auth.token_invalid");
    assert_eq!(body["msg"], "missing query parameter 'aster_access_key'");
}

#[actix_web::test]
async fn test_remote_presigned_upload_browser_cors_invalid_origin_header_returns_bad_request() {
    let fixture = setup_browser_presigned_cors_fixture(
        "provider-browser-invalid-origin-space",
        "http://localhost:3000",
    )
    .await;
    let follower_app = test::init_service(
        App::new()
            .app_data(web::Data::new(fixture.provider_state.follower_view()))
            .service(
                web::scope("/api/v1").service(aster_drive::api::routes::internal_storage::routes()),
            ),
    )
    .await;

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri(&fixture.presigned_path)
        .insert_header(("Origin", "http://localhost:3000/admin"))
        .insert_header(("Access-Control-Request-Method", "PUT"))
        .insert_header(("Access-Control-Request-Headers", "content-type"))
        .to_request();
    let err = test::try_call_service(&follower_app, req)
        .await
        .expect_err("invalid Origin header should be rejected before routing");
    let resp = err.error_response();

    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
    assert!(
        !resp
            .headers()
            .contains_key(actix_web::http::header::ACCESS_CONTROL_ALLOW_ORIGIN),
        "invalid Origin should fail before emitting CORS allow headers"
    );
    let body = actix_web::body::to_bytes(resp.into_body())
        .await
        .expect("bad request body should be readable");
    let body: serde_json::Value =
        serde_json::from_slice(&body).expect("bad request body should be valid json");
    assert_eq!(body["code"], "validation.request_origin_invalid");
    assert_eq!(
        body["msg"],
        "CORS origin must not include a path: 'http://localhost:3000/admin'"
    );
}

#[actix_web::test]
async fn test_remote_presigned_upload_browser_cors_invalid_preflight_header_name_returns_bad_request()
 {
    let fixture = setup_browser_presigned_cors_fixture(
        "provider-browser-invalid-header-name-space",
        "http://localhost:3000",
    )
    .await;
    let follower_app = test::init_service(
        App::new()
            .app_data(web::Data::new(fixture.provider_state.follower_view()))
            .service(
                web::scope("/api/v1").service(aster_drive::api::routes::internal_storage::routes()),
            ),
    )
    .await;

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri(&fixture.presigned_path)
        .insert_header(("Origin", "http://localhost:3000"))
        .insert_header(("Access-Control-Request-Method", "PUT"))
        .insert_header(("Access-Control-Request-Headers", "content-type, @bad"))
        .to_request();
    let err = test::try_call_service(&follower_app, req)
        .await
        .expect_err("invalid preflight header name should be rejected");
    let resp = err.error_response();

    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
    let body = actix_web::body::to_bytes(resp.into_body())
        .await
        .expect("bad request body should be readable");
    let body: serde_json::Value =
        serde_json::from_slice(&body).expect("bad request body should be valid json");
    assert_eq!(body["code"], "validation.request_header_value_invalid");
    assert_eq!(body["msg"], "invalid Access-Control-Request-Headers");
}

#[actix_web::test]
async fn test_remote_presigned_upload_browser_cors_keeps_headers_on_expired_signature() {
    let fixture = setup_browser_presigned_cors_fixture(
        "provider-browser-expired-signature-space",
        "http://localhost:3000",
    )
    .await;
    let expired_path =
        rewrite_path_query_param(&fixture.presigned_path, "aster_expires", Some("1"));
    let follower_app = test::init_service(
        App::new()
            .app_data(web::Data::new(fixture.provider_state.follower_view()))
            .service(
                web::scope("/api/v1").service(aster_drive::api::routes::internal_storage::routes()),
            ),
    )
    .await;

    let req = test::TestRequest::put()
        .uri(&expired_path)
        .insert_header(("Origin", "http://localhost:3000"))
        .insert_header((
            actix_web::http::header::CONTENT_TYPE,
            "application/octet-stream",
        ))
        .insert_header((actix_web::http::header::CONTENT_LENGTH, "4"))
        .set_payload(b"test".as_slice())
        .to_request();
    let resp = test::call_service(&follower_app, req).await;

    assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);
    assert_eq!(
        resp.headers()
            .get(actix_web::http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:3000")
    );
    assert_eq!(
        resp.headers()
            .get(actix_web::http::header::ACCESS_CONTROL_EXPOSE_HEADERS)
            .and_then(|value| value.to_str().ok()),
        Some("ETag")
    );
    let vary = resp
        .headers()
        .get(actix_web::http::header::VARY)
        .and_then(|value| value.to_str().ok())
        .expect("expired presigned response should include Vary");
    assert!(vary.contains("Origin"));
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "auth.token_invalid");
    assert_eq!(body["msg"], "remote presigned URL has expired");
}

#[actix_web::test]
async fn test_remote_presigned_upload_browser_cors_keeps_headers_on_signature_mismatch() {
    let fixture = setup_browser_presigned_cors_fixture(
        "provider-browser-signature-mismatch-space",
        "http://localhost:3000",
    )
    .await;
    let bad_signature_path =
        rewrite_path_query_param(&fixture.presigned_path, "aster_signature", Some("deadbeef"));
    let follower_app = test::init_service(
        App::new()
            .app_data(web::Data::new(fixture.provider_state.follower_view()))
            .service(
                web::scope("/api/v1").service(aster_drive::api::routes::internal_storage::routes()),
            ),
    )
    .await;

    let req = test::TestRequest::put()
        .uri(&bad_signature_path)
        .insert_header(("Origin", "http://localhost:3000"))
        .insert_header((
            actix_web::http::header::CONTENT_TYPE,
            "application/octet-stream",
        ))
        .insert_header((actix_web::http::header::CONTENT_LENGTH, "4"))
        .set_payload(b"test".as_slice())
        .to_request();
    let resp = test::call_service(&follower_app, req).await;

    assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);
    assert_eq!(
        resp.headers()
            .get(actix_web::http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:3000")
    );
    assert_eq!(
        resp.headers()
            .get(actix_web::http::header::ACCESS_CONTROL_EXPOSE_HEADERS)
            .and_then(|value| value.to_str().ok()),
        Some("ETag")
    );
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "auth.credentials_failed");
    assert_eq!(body["msg"], "remote presigned signature mismatch");
}

#[actix_web::test]
async fn test_remote_presigned_upload_browser_cors_accepts_master_url_with_default_https_port() {
    let fixture = setup_browser_presigned_cors_fixture(
        "provider-browser-default-port-space",
        " HTTPS://LOCALHOST:443/admin/settings/site ",
    )
    .await;
    let follower_app = test::init_service(
        App::new()
            .app_data(web::Data::new(fixture.provider_state.follower_view()))
            .service(
                web::scope("/api/v1").service(aster_drive::api::routes::internal_storage::routes()),
            ),
    )
    .await;

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri(&fixture.presigned_path)
        .insert_header(("Origin", "https://localhost"))
        .insert_header(("Access-Control-Request-Method", "PUT"))
        .insert_header(("Access-Control-Request-Headers", "content-type"))
        .to_request();
    let resp = test::call_service(&follower_app, req).await;

    assert_eq!(resp.status(), actix_web::http::StatusCode::NO_CONTENT);
    assert_eq!(
        resp.headers()
            .get(actix_web::http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|value| value.to_str().ok()),
        Some("https://localhost")
    );
}

#[actix_web::test]
async fn test_remote_presigned_multipart_upload_composes_on_provider_without_assembled_temp() {
    let provider_state = common::setup().await;
    let consumer_state = common::setup().await;
    let provider_server = spawn_internal_storage_server(provider_state.follower_view()).await;

    let consumer_node = remote_node::create(
        &consumer_state,
        remote_node::CreateRemoteNodeInput {
            name: "provider-target".to_string(),
            base_url: provider_server.base_url.clone(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("consumer remote node should be created");
    let consumer_node_model =
        managed_follower_repo::find_by_id(consumer_state.writer_db(), consumer_node.id)
            .await
            .expect("consumer remote node should be queryable");

    let (provider_binding, _) = master_binding::upsert_from_enrollment(
        provider_state.writer_db(),
        master_binding::UpsertMasterBindingInput {
            name: "consumer-access".to_string(),
            master_url: "http://master.example.com".to_string(),
            access_key: consumer_node_model.access_key.clone(),
            secret_key: consumer_node_model.secret_key.clone(),
            is_enabled: true,
        },
    )
    .await
    .expect("provider master binding should be created");
    provider_state
        .driver_registry
        .reload_master_bindings(provider_state.writer_db())
        .await
        .expect("provider binding registry should reload");
    create_managed_local_ingress_for_binding(
        &provider_state,
        &consumer_node_model.access_key,
        &consumer_node_model.access_key,
    )
    .await;

    wait_for_remote_probe(&consumer_state, consumer_node.id).await;

    let remote_policy = create_remote_policy_with_options(
        &consumer_state,
        consumer_node.id,
        "Remote Presigned Multipart Policy",
        "presigned-multipart-base",
        StoragePolicyOptions {
            remote_upload_strategy: Some(RemoteUploadStrategy::Presigned),
            ..Default::default()
        },
        4,
    )
    .await;

    let app = create_test_app!(consumer_state.clone());
    let _ = register_and_login!(app);
    let user = user_repo::find_by_username(consumer_state.writer_db(), "testuser")
        .await
        .expect("test user lookup should succeed")
        .expect("test user should exist");
    let folder = folder::create(&consumer_state, user.id, "remote-presigned-multipart", None)
        .await
        .expect("remote folder should be created");
    common::bind_policy_to_folder(&consumer_state, folder.id, remote_policy.id).await;

    let body = b"multipart-remote-upload".to_vec();
    let init = upload_service::init_upload(
        &consumer_state,
        user.id,
        "multipart.bin",
        i64::try_from(body.len()).expect("body length should fit i64"),
        Some(folder.id),
        None,
    )
    .await
    .expect("remote presigned multipart upload should initialize");
    assert_eq!(
        init.mode,
        aster_drive::types::UploadMode::PresignedMultipart
    );

    let upload_id = init
        .upload_id
        .clone()
        .expect("presigned multipart mode should return upload id");
    let session = upload_session_repo::find_by_id(consumer_state.writer_db(), &upload_id)
        .await
        .expect("upload session should exist");
    let remote_multipart_id = session
        .object_multipart_id
        .clone()
        .expect("remote multipart upload id should be stored");
    let chunk_size = usize::try_from(
        init.chunk_size
            .expect("presigned multipart mode should return chunk size"),
    )
    .expect("chunk size should fit usize");
    let total_chunks = usize::try_from(
        init.total_chunks
            .expect("presigned multipart mode should return total_chunks"),
    )
    .expect("total_chunks should fit usize");

    let requested_parts =
        (1..=i32::try_from(total_chunks).expect("chunk count should fit i32")).collect::<Vec<_>>();
    let urls = upload_service::presign_parts(&consumer_state, &upload_id, user.id, requested_parts)
        .await
        .expect("presign multipart part URLs should succeed");

    let mut completed_parts = Vec::with_capacity(total_chunks);
    for part_number in 1..=total_chunks {
        let start = (part_number - 1) * chunk_size;
        let end = std::cmp::min(start + chunk_size, body.len());
        let part_body = body[start..end].to_vec();
        let url = urls
            .get(&i32::try_from(part_number).expect("part number should fit i32"))
            .expect("part URL should exist");
        let response = put_presigned_bytes(url, part_body).await;
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|value| value.to_str().ok())
            .expect("multipart part upload should return ETag")
            .trim_matches('"')
            .to_string();
        completed_parts.push((
            i32::try_from(part_number).expect("part number should fit i32"),
            etag,
        ));
    }

    let progress = upload_service::get_progress(&consumer_state, &upload_id, user.id)
        .await
        .expect("multipart upload progress should be queryable");
    assert_eq!(
        progress.chunks_on_disk,
        (1..=i32::try_from(total_chunks).expect("chunk count should fit i32")).collect::<Vec<_>>()
    );

    let temp_dir = aster_drive::utils::paths::upload_temp_dir(
        &consumer_state.config.server.upload_temp_dir,
        &upload_id,
    );
    let assembled_path = aster_drive::utils::paths::upload_assembled_path(
        &consumer_state.config.server.upload_temp_dir,
        &upload_id,
    );
    assert!(
        !tokio::fs::try_exists(&temp_dir)
            .await
            .expect("temp dir existence should be readable"),
        "remote presigned multipart upload should not create local chunk temp dir"
    );
    assert!(
        !tokio::fs::try_exists(&assembled_path)
            .await
            .expect("assembled path existence should be readable"),
        "remote presigned multipart upload should not create local assembled temp file"
    );

    let created = upload_service::complete_upload(
        &consumer_state,
        &upload_id,
        user.id,
        Some(completed_parts),
    )
    .await
    .expect("remote presigned multipart upload should complete");
    let created_file = file_repo::find_by_id(consumer_state.writer_db(), created.id)
        .await
        .expect("uploaded file should be queryable");
    let created_blob = file_repo::find_blob_by_id(consumer_state.writer_db(), created_file.blob_id)
        .await
        .expect("uploaded blob should be queryable");
    assert!(
        created_blob.hash.starts_with("remote-"),
        "remote presigned multipart uploads should keep remote opaque blob identity"
    );

    let stored_path = managed_ingress_object_path(
        &provider_state,
        &consumer_node_model.access_key,
        &provider_binding.storage_namespace,
        &remote_policy.base_path,
        &created_blob.storage_path,
    );
    let stored = tokio::fs::read(&stored_path)
        .await
        .expect("provider should receive composed multipart upload");
    assert_eq!(stored, body);

    let first_part_path = managed_ingress_object_path(
        &provider_state,
        &consumer_node_model.access_key,
        &provider_binding.storage_namespace,
        &remote_policy.base_path,
        &format!("uploads/{remote_multipart_id}/parts/1"),
    );
    assert!(
        !tokio::fs::try_exists(&first_part_path)
            .await
            .expect("part path existence should be readable"),
        "remote multipart compose should cleanup follower temp parts"
    );

    provider_server.stop().await;
}

//! Real multi-primary acceptance tests backed by PostgreSQL and Redis.

use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

use aster_forge_test::postgres::{PostgresTestContainer, PostgresTestDatabase};
use aster_forge_test::process::{TestProcess, available_loopback_port};
use aster_forge_test::redis::RedisTestContainer;
use aster_forge_test::smtp::SmtpTestContainer;
use aster_forge_test::suite::TestContainerSuite;
use chrono::Utc;
use futures::{SinkExt, StreamExt};
use migration::Migrator;
use reqwest::header::SET_COOKIE;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde_json::{Value, json};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::protocol::Message as WsMessage;
use tokio_util::sync::CancellationToken;

use aster_drive::storage::remote_protocol::tunnel::server::{
    REMOTE_TUNNEL_CONNECT_PATH, REMOTE_TUNNEL_PROXY_PATH_PREFIX, RemoteTunnelStreamFrame,
    RemoteTunnelStreamFrameKind, decode_stream_frame, encode_stream_frame,
};
use aster_drive::storage::remote_protocol::{
    INTERNAL_AUTH_ACCESS_KEY_HEADER, INTERNAL_AUTH_NONCE_HEADER, INTERNAL_AUTH_SIGNATURE_HEADER,
    INTERNAL_AUTH_TIMESTAMP_HEADER, INTERNAL_STORAGE_BASE_PATH, RemoteStorageCapabilities,
    sign_internal_request,
};

const RUNTIME_LEASE_ID: &str = "aster_drive.background_tasks";
const ADMIN_PASSWORD: &str = "AsterDrive-E2E-Password-399!";
const SHARED_SECRET: &str = "asterdrive399abcdef0123456789abcdef0123456789abcdef0123456789abcd";
const INTERNAL_PROXY_SECRET: &str =
    "asterdrive399proxyabcdef0123456789abcdef0123456789abcdef012345";

struct SharedServices {
    _postgres: PostgresTestContainer,
    database: PostgresTestDatabase,
    redis: RedisTestContainer,
    smtp: SmtpTestContainer,
    database_url: String,
    redis_url: String,
    config_topic: String,
}

impl SharedServices {
    async fn start() -> Self {
        let postgres = PostgresTestContainer::start(test_suite()).await;
        let smtp = SmtpTestContainer::start(test_suite()).await;
        smtp.clear_messages().await;
        let database_name = format!("asterdrive_multi_primary_{}", uuid::Uuid::new_v4().simple());
        let test_database = postgres.create_database(&database_name).await;
        let database_url = test_database.url().to_string();
        let database = test_database.connect().await;
        Migrator::up(&database, None)
            .await
            .expect("apply migrations to isolated multi-primary database");
        let now = Utc::now();
        aster_drive::db::repository::policy_repo::create(
            &database,
            aster_drive::entities::storage_policy::ActiveModel {
                name: Set("E2E Shared Object Storage".to_string()),
                driver_type: Set(aster_drive::types::DriverType::S3),
                endpoint: Set("http://127.0.0.1:9000".to_string()),
                bucket: Set("asterdrive-e2e".to_string()),
                access_key: Set("e2e-access".to_string()),
                secret_key: Set("e2e-secret".to_string()),
                base_path: Set(String::new()),
                max_file_size: Set(0),
                allowed_types: Set(aster_drive::types::StoredStoragePolicyAllowedTypes::empty()),
                options: Set(aster_drive::types::StoredStoragePolicyOptions::empty()),
                is_default: Set(true),
                chunk_size: Set(5_242_880),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .expect("create shared E2E storage policy");
        aster_drive::services::storage_policy::policy::ensure_policy_groups_seeded(&database)
            .await
            .expect("seed default E2E storage policy group");
        seed_runtime_config(&database, smtp.smtp_address().port()).await;
        database
            .close()
            .await
            .expect("close isolated database seed connection");

        let redis = RedisTestContainer::start(test_suite()).await;

        Self {
            database_url,
            database: test_database,
            redis_url: redis.url().to_string(),
            _postgres: postgres,
            redis,
            smtp,
            config_topic: format!(
                "aster_drive.multi_primary_e2e.{}",
                uuid::Uuid::new_v4().simple()
            ),
        }
    }

    async fn connect_database(&self) -> DatabaseConnection {
        self.database.connect().await
    }

    async fn cleanup_database(&self) {
        self.database.cleanup().await;
    }
}

async fn seed_runtime_config(database: &DatabaseConnection, smtp_port: u16) {
    aster_drive::db::repository::config_repo::ensure_defaults_with_env(
        database,
        &|_| None::<String>,
    )
    .await
    .expect("seed runtime config defaults");

    let values = [
        (
            aster_drive::config::definitions::MAIL_OUTBOX_DISPATCH_INTERVAL_SECS_KEY,
            "1".to_string(),
        ),
        (
            aster_drive::config::definitions::BACKGROUND_TASK_DISPATCH_INTERVAL_SECS_KEY,
            "1".to_string(),
        ),
        (
            aster_drive::config::definitions::BACKGROUND_TASK_DISPATCH_IDLE_MAX_INTERVAL_SECS_KEY,
            "1".to_string(),
        ),
        (
            aster_drive::config::definitions::REMOTE_NODE_HEALTH_TEST_INTERVAL_SECS_KEY,
            "1".to_string(),
        ),
        (
            aster_drive::config::definitions::MAIL_SMTP_HOST_KEY,
            "127.0.0.1".to_string(),
        ),
        (
            aster_drive::config::definitions::MAIL_SMTP_PORT_KEY,
            smtp_port.to_string(),
        ),
        (
            aster_drive::config::definitions::MAIL_FROM_ADDRESS_KEY,
            "asterdrive-e2e@example.com".to_string(),
        ),
        (
            aster_drive::config::definitions::MAIL_FROM_NAME_KEY,
            "AsterDrive E2E".to_string(),
        ),
        (
            aster_drive::config::definitions::MAIL_SECURITY_KEY,
            "false".to_string(),
        ),
    ];
    for (key, value) in values {
        aster_drive::db::repository::config_repo::upsert_with_actor(database, key, &value, None)
            .await
            .unwrap_or_else(|error| panic!("seed runtime config {key}: {error}"));
    }
}

fn test_suite() -> &'static TestContainerSuite {
    static SUITE: OnceLock<TestContainerSuite> = OnceLock::new();
    SUITE.get_or_init(|| TestContainerSuite::new("asterdrive-multi-primary"))
}

fn e2e_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

struct ServerProcess {
    port: u16,
    process: TestProcess,
}

impl ServerProcess {
    fn spawn(name: &str, services: &SharedServices) -> Self {
        let port = available_loopback_port();
        let mut command = Command::new(env!("CARGO_BIN_EXE_aster_drive"));
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("ASTER__") {
                command.env_remove(key);
            }
        }
        command
            .env("ASTER__DEPLOYMENT__PROFILE", "cluster")
            .env(
                "ASTER__DEPLOYMENT__INTERNAL_ENDPOINT",
                format!("http://127.0.0.1:{port}"),
            )
            .env(
                "ASTER__DEPLOYMENT__INTERNAL_PROXY_SECRET",
                INTERNAL_PROXY_SECRET,
            )
            .env("ASTER__SERVER__HOST", "127.0.0.1")
            .env("ASTER__SERVER__PORT", port.to_string())
            .env("ASTER__SERVER__WORKERS", "1")
            .env("ASTER__DATABASE__URL", &services.database_url)
            .env("ASTER__DATABASE__POOL_SIZE", "5")
            .env("ASTER__CACHE__BACKEND", "redis")
            .env("ASTER__CACHE__ENDPOINT", &services.redis_url)
            .env("ASTER__CONFIG_SYNC__BACKEND", "redis")
            .env("ASTER__CONFIG_SYNC__ENDPOINT", &services.redis_url)
            .env("ASTER__CONFIG_SYNC__TOPIC", &services.config_topic)
            .env("ASTER__AUTH__JWT_SECRET", SHARED_SECRET)
            .env("ASTER__AUTH__SHARE_COOKIE_SECRET", SHARED_SECRET)
            .env("ASTER__AUTH__DIRECT_LINK_SECRET", SHARED_SECRET)
            .env("ASTER__AUTH__MFA_SECRET_KEY", SHARED_SECRET)
            .env("ASTER__AUTH__STORAGE_CREDENTIAL_SECRET_KEY", SHARED_SECRET)
            .env("ASTER__AUTH__BOOTSTRAP_INSECURE_COOKIES", "true")
            .env("ASTER__LOGGING__LEVEL", "warn");
        let process = TestProcess::spawn(name, &mut command);

        Self { port, process }
    }

    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn name(&self) -> &str {
        self.process.name()
    }

    fn terminate(&mut self) {
        self.process.terminate();
    }

    #[cfg(unix)]
    fn terminate_gracefully(&mut self) {
        assert!(
            self.process.terminate_gracefully(Duration::from_secs(20)),
            "primary {} did not stop after SIGTERM\n{}",
            self.name(),
            self.diagnostics()
        );
    }

    fn assert_running(&mut self) {
        self.process.assert_running();
    }

    fn diagnostics(&self) -> String {
        self.process.diagnostics()
    }
}

async fn wait_for_health(client: &reqwest::Client, server: &mut ServerProcess) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
    loop {
        server.assert_running();
        if let Ok(response) = client
            .get(format!("{}/health", server.base_url()))
            .send()
            .await
            && response.status().is_success()
        {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "primary {} did not become healthy\n{}",
                server.name(),
                server.diagnostics()
            );
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn setup_and_login(client: &reqwest::Client, server: &ServerProcess) -> String {
    let setup_response = client
        .post(format!("{}/api/v1/auth/setup", server.base_url()))
        .json(&json!({
            "username": "admin",
            "email": "admin@example.com",
            "password": ADMIN_PASSWORD,
        }))
        .send()
        .await
        .expect("send initial admin setup request");
    let setup_status = setup_response.status();
    let setup_body = setup_response.text().await.expect("read setup response");
    assert_eq!(
        setup_status.as_u16(),
        201,
        "admin setup failed: {setup_body}"
    );

    let login_response = client
        .post(format!("{}/api/v1/auth/login", server.base_url()))
        .json(&json!({
            "identifier": "admin",
            "password": ADMIN_PASSWORD,
        }))
        .send()
        .await
        .expect("send admin login request");
    let login_status = login_response.status();
    let access_token = cookie_value(&login_response, "aster_access");
    let login_body = login_response.text().await.expect("read login response");
    assert!(
        login_status.is_success(),
        "admin login failed with {login_status}: {login_body}"
    );
    access_token.unwrap_or_else(|| panic!("login response omitted aster_access: {login_body}"))
}

fn cookie_value(response: &reqwest::Response, name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    response
        .headers()
        .get_all(SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(|value| {
            value
                .strip_prefix(&prefix)
                .and_then(|value| value.split(';').next())
                .map(str::to_string)
        })
}

async fn set_registration_enabled(
    client: &reqwest::Client,
    server: &ServerProcess,
    access_token: &str,
    enabled: bool,
) {
    let response = client
        .put(format!(
            "{}/api/v1/admin/config/auth_allow_user_registration",
            server.base_url()
        ))
        .bearer_auth(access_token)
        .json(&json!({ "value": enabled.to_string() }))
        .send()
        .await
        .expect("send runtime config mutation");
    let status = response.status();
    let body = response
        .text()
        .await
        .expect("read config mutation response");
    assert!(
        status.is_success(),
        "config mutation failed with {status}: {body}"
    );
}

async fn registration_enabled(client: &reqwest::Client, server: &ServerProcess) -> bool {
    let response = client
        .post(format!("{}/api/v1/auth/check", server.base_url()))
        .send()
        .await
        .expect("send public runtime config probe");
    let status = response.status();
    let body: Value = response.json().await.expect("decode public config probe");
    assert!(status.is_success(), "public config probe failed: {body}");
    body["data"]["allow_user_registration"]
        .as_bool()
        .expect("allow_user_registration should be boolean")
}

async fn wait_for_registration_enabled(
    client: &reqwest::Client,
    server: &mut ServerProcess,
    expected: bool,
    timeout: Duration,
) {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        server.assert_running();
        if registration_enabled(client, server).await == expected {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "primary {} did not reconcile allow_user_registration={expected}\n{}",
                server.name(),
                server.diagnostics()
            );
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn wait_for_runtime_lease(
    database: &DatabaseConnection,
    server: &mut ServerProcess,
) -> aster_forge_db::runtime_lease::Model {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        server.assert_running();
        if let Some(lease) = aster_forge_db::runtime_lease::Entity::find_by_id(RUNTIME_LEASE_ID)
            .one(database)
            .await
            .expect("query runtime lease")
        {
            return lease;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "primary {} did not acquire runtime lease\n{}",
                server.name(),
                server.diagnostics()
            );
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn load_single_runtime_lease(
    database: &DatabaseConnection,
) -> aster_forge_db::runtime_lease::Model {
    let leases = aster_forge_db::runtime_lease::Entity::find()
        .all(database)
        .await
        .expect("list runtime leases");
    assert_eq!(leases.len(), 1, "only one runtime lease row may exist");
    let lease = leases
        .into_iter()
        .next()
        .expect("runtime lease should exist");
    assert_eq!(lease.lease_id, RUNTIME_LEASE_ID);
    lease
}

async fn assert_single_live_runtime_lease(
    database: &DatabaseConnection,
) -> aster_forge_db::runtime_lease::Model {
    let lease = load_single_runtime_lease(database).await;
    assert!(
        lease.expires_at > Utc::now(),
        "active runtime lease must not be expired"
    );
    lease
}

async fn wait_for_new_runtime_owner(
    database: &DatabaseConnection,
    server: &mut ServerProcess,
    previous_owner: &str,
    timeout: Duration,
) -> aster_forge_db::runtime_lease::Model {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        server.assert_running();
        if let Some(lease) = aster_forge_db::runtime_lease::Entity::find_by_id(RUNTIME_LEASE_ID)
            .one(database)
            .await
            .expect("query runtime lease during takeover")
            && lease.owner_id != previous_owner
        {
            assert!(
                lease.expires_at > Utc::now(),
                "new runtime lease owner must publish a live expiration"
            );
            return lease;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "primary {} did not take over the runtime lease\n{}",
                server.name(),
                server.diagnostics()
            );
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn create_trash_purge_task(
    client: &reqwest::Client,
    server: &ServerProcess,
    access_token: &str,
) -> i64 {
    let response = client
        .delete(format!("{}/api/v1/trash", server.base_url()))
        .bearer_auth(access_token)
        .send()
        .await
        .expect("create trash purge task");
    let status = response.status();
    let body: Value = response.json().await.expect("decode trash purge response");
    assert!(status.is_success(), "trash purge request failed: {body}");
    body["data"]["id"]
        .as_i64()
        .expect("trash purge response should include task id")
}

async fn wait_for_background_task(
    database: &DatabaseConnection,
    task_id: i64,
    server_a: &mut ServerProcess,
    server_b: &mut ServerProcess,
    timeout: Duration,
) -> aster_drive::entities::background_task::Model {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        server_a.assert_running();
        server_b.assert_running();
        let task = aster_drive::entities::background_task::Entity::find_by_id(task_id)
            .one(database)
            .await
            .expect("query background task")
            .expect("background task should exist");
        if task.status.is_terminal() {
            return task;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "background task {task_id} did not finish: {:?}\n{}\n{}",
                task.status,
                server_a.diagnostics(),
                server_b.diagnostics()
            );
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn wait_for_mail_outbox_sent(
    database: &DatabaseConnection,
    outbox_id: i64,
    services: &SharedServices,
    server_a: &mut ServerProcess,
    server_b: &mut ServerProcess,
) -> aster_forge_db::mail_outbox::Model {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        server_a.assert_running();
        server_b.assert_running();
        let row = aster_forge_db::mail_outbox::Entity::find_by_id(outbox_id)
            .one(database)
            .await
            .expect("query mail outbox row")
            .expect("mail outbox row should exist");
        let messages = services.smtp.message_count().await;
        if row.status == aster_forge_mail::MailOutboxStatus::Sent && messages >= 1 {
            return row;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "mail outbox row {outbox_id} did not send: status={:?}, accepted={}\n{}\n{}",
                row.status,
                messages,
                server_a.diagnostics(),
                server_b.diagnostics()
            );
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn scheduled_runtime_records(
    database: &DatabaseConnection,
    task_name: &str,
) -> Vec<aster_drive::entities::background_task::Model> {
    aster_drive::entities::background_task::Entity::find()
        .filter(
            aster_drive::entities::background_task::Column::Kind
                .eq(aster_drive::types::BackgroundTaskKind::SystemRuntime),
        )
        .all(database)
        .await
        .expect("query scheduled runtime records")
        .into_iter()
        .filter(|task| {
            task.dedupe_key.is_some()
                && serde_json::from_str::<Value>(task.payload_json.as_ref())
                    .ok()
                    .and_then(|payload| payload["task_name"].as_str().map(str::to_string))
                    .as_deref()
                    == Some(task_name)
        })
        .collect()
}

struct SyntheticTunnelFollower {
    shutdown: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

impl SyntheticTunnelFollower {
    async fn stop(self) {
        self.shutdown.cancel();
        tokio::time::timeout(Duration::from_secs(5), self.task)
            .await
            .expect("synthetic tunnel follower should stop before timeout")
            .expect("synthetic tunnel follower task should join");
    }
}

async fn seed_reverse_tunnel_node(
    database: &DatabaseConnection,
) -> aster_drive::entities::managed_follower::Model {
    let now = Utc::now();
    let node = aster_drive::entities::managed_follower::ActiveModel {
        name: Set("multi-primary reverse tunnel follower".to_string()),
        base_url: Set(String::new()),
        access_key: Set(format!("e2e-access-{}", uuid::Uuid::new_v4().simple())),
        secret_key: Set(format!("e2e-secret-{}", uuid::Uuid::new_v4().simple())),
        is_enabled: Set(true),
        transport_mode: Set(aster_drive::types::RemoteNodeTransportMode::ReverseTunnel),
        last_capabilities: Set("{}".to_string()),
        last_error: Set(String::new()),
        last_checked_at: Set(None),
        tunnel_last_error: Set(String::new()),
        tunnel_last_seen_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(database)
    .await
    .expect("insert reverse tunnel E2E remote node");

    aster_drive::entities::follower_enrollment_session::ActiveModel {
        managed_follower_id: Set(node.id),
        token_hash: Set(format!("e2e-token-{}", uuid::Uuid::new_v4().simple())),
        ack_token_hash: Set(format!("e2e-ack-{}", uuid::Uuid::new_v4().simple())),
        expires_at: Set(now + chrono::Duration::minutes(30)),
        redeemed_at: Set(Some(now)),
        acked_at: Set(Some(now)),
        invalidated_at: Set(None),
        created_at: Set(now),
        ..Default::default()
    }
    .insert(database)
    .await
    .expect("mark reverse tunnel E2E enrollment complete");
    node
}

async fn defer_remote_node_health_tests(database: &DatabaseConnection) {
    aster_drive::db::repository::config_repo::upsert_with_actor(
        database,
        aster_drive::config::definitions::REMOTE_NODE_HEALTH_TEST_INTERVAL_SECS_KEY,
        "3600",
        None,
    )
    .await
    .expect("defer automatic remote-node health tests during tunnel routing E2E");
}

fn spawn_synthetic_tunnel_follower(
    primary_url: String,
    remote_node: aster_drive::entities::managed_follower::Model,
) -> SyntheticTunnelFollower {
    let shutdown = CancellationToken::new();
    let worker_shutdown = shutdown.clone();
    let task = tokio::spawn(async move {
        while !worker_shutdown.is_cancelled() {
            let result = run_synthetic_tunnel_connection(
                &primary_url,
                &remote_node,
                worker_shutdown.clone(),
            )
            .await;
            if worker_shutdown.is_cancelled() {
                break;
            }
            if let Err(error) = result {
                tracing::debug!("synthetic reverse tunnel reconnecting after: {error}");
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    });
    SyntheticTunnelFollower { shutdown, task }
}

async fn run_synthetic_tunnel_connection(
    primary_url: &str,
    remote_node: &aster_drive::entities::managed_follower::Model,
    shutdown: CancellationToken,
) -> Result<(), String> {
    let ws_url = format!(
        "{}{}",
        primary_url.replacen("http://", "ws://", 1),
        REMOTE_TUNNEL_CONNECT_PATH
    );
    let timestamp = Utc::now().timestamp();
    let nonce = uuid::Uuid::new_v4().to_string();
    let signature = sign_internal_request(
        &remote_node.secret_key,
        "GET",
        REMOTE_TUNNEL_CONNECT_PATH,
        timestamp,
        &nonce,
        None,
    );
    let mut request = ws_url
        .into_client_request()
        .map_err(|error| format!("build synthetic tunnel websocket request: {error}"))?;
    let headers = request.headers_mut();
    headers.insert(
        INTERNAL_AUTH_ACCESS_KEY_HEADER,
        HeaderValue::from_str(&remote_node.access_key)
            .map_err(|error| format!("set synthetic tunnel access key: {error}"))?,
    );
    headers.insert(
        INTERNAL_AUTH_TIMESTAMP_HEADER,
        HeaderValue::from_str(&timestamp.to_string())
            .map_err(|error| format!("set synthetic tunnel timestamp: {error}"))?,
    );
    headers.insert(
        INTERNAL_AUTH_NONCE_HEADER,
        HeaderValue::from_str(&nonce)
            .map_err(|error| format!("set synthetic tunnel nonce: {error}"))?,
    );
    headers.insert(
        INTERNAL_AUTH_SIGNATURE_HEADER,
        HeaderValue::from_str(&signature)
            .map_err(|error| format!("set synthetic tunnel signature: {error}"))?,
    );

    let (socket, _) = connect_async(request)
        .await
        .map_err(|error| format!("connect synthetic tunnel websocket: {error}"))?;
    let (mut writer, mut reader) = socket.split();
    loop {
        let message = tokio::select! {
            _ = shutdown.cancelled() => return Ok(()),
            message = reader.next() => message,
        };
        let Some(message) = message else {
            return Ok(());
        };
        match message.map_err(|error| format!("read synthetic tunnel websocket: {error}"))? {
            WsMessage::Binary(bytes) => {
                let start = decode_stream_frame(bytes)
                    .map_err(|error| format!("decode synthetic tunnel frame: {error}"))?;
                if start.kind != RemoteTunnelStreamFrameKind::RequestStart {
                    return Err(format!(
                        "synthetic tunnel expected request_start, got {:?}",
                        start.kind
                    ));
                }
                drain_synthetic_request_body(&start.request_id, &mut reader, &mut writer).await?;
                send_synthetic_capabilities_response(&start.request_id, &mut writer).await?;
            }
            WsMessage::Ping(bytes) => writer
                .send(WsMessage::Pong(bytes))
                .await
                .map_err(|error| format!("send synthetic tunnel pong: {error}"))?,
            WsMessage::Close(_) => return Ok(()),
            _ => {}
        }
    }
}

async fn drain_synthetic_request_body<R, W>(
    request_id: &str,
    reader: &mut R,
    writer: &mut W,
) -> Result<(), String>
where
    R: futures::Stream<Item = Result<WsMessage, tokio_tungstenite::tungstenite::Error>> + Unpin,
    W: futures::Sink<WsMessage, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    loop {
        let message = reader
            .next()
            .await
            .ok_or_else(|| "synthetic tunnel closed before request_end".to_string())?
            .map_err(|error| format!("read synthetic tunnel request body: {error}"))?;
        match message {
            WsMessage::Binary(bytes) => {
                let frame = decode_stream_frame(bytes)
                    .map_err(|error| format!("decode synthetic request body frame: {error}"))?;
                if frame.request_id != request_id {
                    return Err("synthetic tunnel received interleaved request".to_string());
                }
                match frame.kind {
                    RemoteTunnelStreamFrameKind::RequestBody => {}
                    RemoteTunnelStreamFrameKind::RequestEnd => return Ok(()),
                    RemoteTunnelStreamFrameKind::Error => return Ok(()),
                    other => {
                        return Err(format!(
                            "synthetic tunnel received unexpected request frame {other:?}"
                        ));
                    }
                }
            }
            WsMessage::Ping(bytes) => writer
                .send(WsMessage::Pong(bytes))
                .await
                .map_err(|error| format!("send synthetic tunnel body pong: {error}"))?,
            WsMessage::Close(_) => {
                return Err("synthetic tunnel closed before request_end".to_string());
            }
            _ => {}
        }
    }
}

async fn send_synthetic_capabilities_response<W>(
    request_id: &str,
    writer: &mut W,
) -> Result<(), String>
where
    W: futures::Sink<WsMessage, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let body = serde_json::to_vec(&json!({
        "code": "success",
        "msg": "",
        "data": RemoteStorageCapabilities::current(),
    }))
    .map_err(|error| format!("encode synthetic tunnel capabilities: {error}"))?;
    let frames = [
        RemoteTunnelStreamFrame {
            kind: RemoteTunnelStreamFrameKind::ResponseStart,
            request_id: request_id.to_string(),
            method: None,
            path_and_query: None,
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            content_length: Some(body.len() as u64),
            status: Some(200),
            message: None,
            body: bytes::Bytes::new(),
        },
        RemoteTunnelStreamFrame {
            kind: RemoteTunnelStreamFrameKind::ResponseBody,
            request_id: request_id.to_string(),
            method: None,
            path_and_query: None,
            headers: Vec::new(),
            content_length: None,
            status: None,
            message: None,
            body: bytes::Bytes::copy_from_slice(&body[..body.len() / 2]),
        },
        RemoteTunnelStreamFrame {
            kind: RemoteTunnelStreamFrameKind::ResponseBody,
            request_id: request_id.to_string(),
            method: None,
            path_and_query: None,
            headers: Vec::new(),
            content_length: None,
            status: None,
            message: None,
            body: bytes::Bytes::copy_from_slice(&body[body.len() / 2..]),
        },
        RemoteTunnelStreamFrame {
            kind: RemoteTunnelStreamFrameKind::ResponseEnd,
            request_id: request_id.to_string(),
            method: None,
            path_and_query: None,
            headers: Vec::new(),
            content_length: None,
            status: None,
            message: None,
            body: bytes::Bytes::new(),
        },
    ];
    for frame in frames {
        writer
            .send(WsMessage::Binary(encode_stream_frame(&frame).map_err(
                |error| format!("encode synthetic response frame: {error}"),
            )?))
            .await
            .map_err(|error| format!("send synthetic response frame: {error}"))?;
    }
    Ok(())
}

async fn wait_for_tunnel_owner(
    database: &DatabaseConnection,
    remote_node_id: i64,
    expected_endpoint: &str,
    server: &mut ServerProcess,
    timeout: Duration,
) -> aster_drive::entities::remote_tunnel_owner::Model {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        server.assert_running();
        if let Some(owner) =
            aster_drive::entities::remote_tunnel_owner::Entity::find_by_id(remote_node_id)
                .one(database)
                .await
                .expect("query reverse tunnel owner directory")
            && owner.internal_endpoint == expected_endpoint
            && owner.lease_expires_at > Utc::now()
        {
            return owner;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "reverse tunnel owner did not become {expected_endpoint}\n{}",
                server.diagnostics()
            );
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn test_remote_node_through(
    client: &reqwest::Client,
    server: &ServerProcess,
    access_token: &str,
    remote_node_id: i64,
) -> Value {
    let response = client
        .post(format!(
            "{}/api/v1/admin/remote-nodes/{remote_node_id}/test",
            server.base_url()
        ))
        .bearer_auth(access_token)
        .send()
        .await
        .expect("send reverse tunnel remote-node probe");
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .expect("decode reverse tunnel remote-node probe");
    assert!(
        status.is_success(),
        "reverse tunnel probe through {} failed with {status}: {body}\n{}",
        server.name(),
        server.diagnostics()
    );
    body
}

async fn stale_fencing_proxy_response(
    client: &reqwest::Client,
    server: &ServerProcess,
    remote_node_id: i64,
    stale_fencing_token: &str,
) -> (reqwest::StatusCode, String) {
    let mut url = reqwest::Url::parse(&format!(
        "{}{}{remote_node_id}",
        server.base_url(),
        format!("{REMOTE_TUNNEL_PROXY_PATH_PREFIX}/")
    ))
    .expect("build stale fencing proxy URL");
    url.query_pairs_mut()
        .append_pair("method", "GET")
        .append_pair(
            "path_and_query",
            &format!("{INTERNAL_STORAGE_BASE_PATH}/capabilities"),
        )
        .append_pair("fencing_token", stale_fencing_token)
        .append_pair("headers", "W10");
    let request_target = format!("{}?{}", url.path(), url.query().unwrap_or_default());
    let timestamp = Utc::now().timestamp();
    let nonce = uuid::Uuid::new_v4().to_string();
    let signature = sign_internal_request(
        INTERNAL_PROXY_SECRET,
        "POST",
        &request_target,
        timestamp,
        &nonce,
        Some(0),
    );
    let response = client
        .post(url)
        .header(INTERNAL_AUTH_ACCESS_KEY_HEADER, "stale-e2e-runtime")
        .header(INTERNAL_AUTH_TIMESTAMP_HEADER, timestamp.to_string())
        .header(INTERNAL_AUTH_NONCE_HEADER, nonce)
        .header(INTERNAL_AUTH_SIGNATURE_HEADER, signature)
        .header(reqwest::header::CONTENT_LENGTH, "0")
        .body(Vec::new())
        .send()
        .await
        .expect("send stale fencing proxy request");
    let status = response.status();
    let body = response
        .text()
        .await
        .expect("read stale fencing proxy response");
    (status, body)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Docker and two real AsterDrive primary processes"]
async fn config_sync_propagates_and_reconciles_after_redis_outage() {
    let _guard = e2e_lock().lock().await;
    let services = SharedServices::start().await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build E2E HTTP client");
    let mut primary_a = ServerProcess::spawn("primary-a", &services);
    wait_for_health(&client, &mut primary_a).await;
    let access_token = setup_and_login(&client, &primary_a).await;

    let mut primary_b = ServerProcess::spawn("primary-b", &services);
    wait_for_health(&client, &mut primary_b).await;
    assert!(registration_enabled(&client, &primary_b).await);
    tokio::time::sleep(Duration::from_secs(1)).await;

    set_registration_enabled(&client, &primary_a, &access_token, false).await;
    wait_for_registration_enabled(&client, &mut primary_b, false, Duration::from_secs(15)).await;

    services.redis.stop().await;
    let database = services.connect_database().await;
    aster_drive::db::repository::config_repo::upsert(
        &database,
        "auth_allow_user_registration",
        "true",
        1,
    )
    .await
    .expect("update authoritative config while notification transport is offline");
    tokio::time::sleep(Duration::from_millis(750)).await;
    assert!(
        !registration_enabled(&client, &primary_b).await,
        "primary B must keep its old snapshot until reconnect reconciliation"
    );

    services.redis.restart().await;
    wait_for_registration_enabled(&client, &mut primary_a, true, Duration::from_secs(60)).await;
    wait_for_registration_enabled(&client, &mut primary_b, true, Duration::from_secs(60)).await;

    primary_a.terminate();
    primary_b.terminate();
    database
        .close()
        .await
        .expect("close config E2E database connection");
    services.cleanup_database().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Docker and two real AsterDrive primary processes"]
async fn scheduler_has_one_owner_and_standby_takes_over_after_owner_crash() {
    let _guard = e2e_lock().lock().await;
    let services = SharedServices::start().await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build E2E HTTP client");
    let database = services.connect_database().await;

    let mut primary_a = ServerProcess::spawn("primary-a", &services);
    wait_for_health(&client, &mut primary_a).await;
    let initial_lease = wait_for_runtime_lease(&database, &mut primary_a).await;

    let mut primary_b = ServerProcess::spawn("primary-b", &services);
    wait_for_health(&client, &mut primary_b).await;
    tokio::time::sleep(Duration::from_secs(12)).await;
    let renewed_lease = assert_single_live_runtime_lease(&database).await;
    assert_eq!(
        renewed_lease.owner_id, initial_lease.owner_id,
        "standby primary must not steal a live lease"
    );
    assert!(
        renewed_lease.last_renewed_at > initial_lease.last_renewed_at,
        "active owner should renew while both primaries are healthy"
    );

    primary_a.terminate();
    let lease_after_crash = load_single_runtime_lease(&database).await;
    assert_eq!(lease_after_crash.owner_id, initial_lease.owner_id);

    let deadline = tokio::time::Instant::now() + Duration::from_secs(50);
    loop {
        primary_b.assert_running();
        let lease = load_single_runtime_lease(&database).await;
        if lease.owner_id != initial_lease.owner_id {
            assert!(
                lease.expires_at > Utc::now(),
                "new runtime lease owner must publish a live expiration"
            );
            assert!(
                lease.last_renewed_at >= lease_after_crash.expires_at,
                "standby must acquire only after the crashed owner's lease expires"
            );
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "primary B did not take over the expired scheduler lease\n{}",
                primary_b.diagnostics()
            );
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    primary_b.terminate();
    database
        .close()
        .await
        .expect("close scheduler E2E database connection");
    services.cleanup_database().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Docker and two real AsterDrive primary processes"]
async fn scheduled_mail_dispatch_has_one_firing_and_one_delivery_across_primaries() {
    let _guard = e2e_lock().lock().await;
    let services = SharedServices::start().await;
    let database = services.connect_database().await;
    let now = Utc::now();
    let payload = aster_drive::services::mail::template::MailTemplatePayload::user_invitation(
        "e2e@example.com",
        "https://drive.example.com/invite/e2e",
        "AsterDrive E2E",
        "1 hour",
    )
    .to_stored()
    .expect("serialize E2E mail payload");
    let outbox = aster_forge_db::create_mail_outbox_row(
        &database,
        aster_forge_db::MailOutboxCreate {
            template_code: aster_forge_mail::MailTemplateCode::UserInvitation,
            to_address: "e2e@example.com".to_string(),
            to_name: Some("E2E Recipient".to_string()),
            payload_json: payload,
            next_attempt_at: now,
            now,
        },
    )
    .await
    .expect("create E2E mail outbox row");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build E2E HTTP client");
    let mut primary_a = ServerProcess::spawn("primary-a", &services);
    wait_for_health(&client, &mut primary_a).await;
    let mut primary_b = ServerProcess::spawn("primary-b", &services);
    wait_for_health(&client, &mut primary_b).await;

    let sent = wait_for_mail_outbox_sent(
        &database,
        outbox.id,
        &services,
        &mut primary_a,
        &mut primary_b,
    )
    .await;
    assert_eq!(
        sent.payload_json.as_ref(),
        aster_forge_mail::StoredMailPayload::CLEARED_JSON
    );
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(services.smtp.message_count().await, 1);

    let records = scheduled_runtime_records(&database, "mail-outbox-dispatch").await;
    assert_eq!(
        records.len(),
        1,
        "one scheduled firing must create one history row"
    );
    assert!(records[0].dedupe_key.is_some());

    primary_a.terminate();
    primary_b.terminate();
    database
        .close()
        .await
        .expect("close mail E2E database connection");
    services.cleanup_database().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Docker and two real AsterDrive primary processes"]
async fn background_task_claim_is_fenced_across_primaries() {
    let _guard = e2e_lock().lock().await;
    let services = SharedServices::start().await;
    let database = services.connect_database().await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build E2E HTTP client");

    let mut primary_a = ServerProcess::spawn("primary-a", &services);
    wait_for_health(&client, &mut primary_a).await;
    let access_token = setup_and_login(&client, &primary_a).await;
    let mut primary_b = ServerProcess::spawn("primary-b", &services);
    wait_for_health(&client, &mut primary_b).await;

    let task_id = create_trash_purge_task(&client, &primary_a, &access_token).await;
    let task = wait_for_background_task(
        &database,
        task_id,
        &mut primary_a,
        &mut primary_b,
        Duration::from_secs(30),
    )
    .await;
    assert_eq!(
        task.status,
        aster_drive::types::BackgroundTaskStatus::Succeeded
    );
    assert_eq!(task.processing_token, 1);
    assert_eq!(task.attempt_count, 0);
    assert!(task.processing_started_at.is_none());
    assert!(task.lease_expires_at.is_none());
    assert!(
        !aster_drive::db::repository::background_task_repo::mark_succeeded(
            &database,
            aster_drive::db::repository::background_task_repo::TaskSuccessUpdate {
                id: task.id,
                processing_token: 0,
                result_json: None,
                steps_json: None,
                current: 1,
                total: 1,
                status_text: Some("stale worker completion"),
                finished_at: Utc::now(),
                expires_at: Utc::now() + chrono::Duration::hours(1),
            },
        )
        .await
        .expect("stale processing token write should execute without database error"),
        "a stale worker token must not overwrite the completed task"
    );

    primary_a.terminate();
    primary_b.terminate();
    database
        .close()
        .await
        .expect("close background task E2E database connection");
    services.cleanup_database().await;
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Docker and two real AsterDrive primary processes"]
async fn graceful_primary_shutdown_releases_lease_for_standby() {
    let _guard = e2e_lock().lock().await;
    let services = SharedServices::start().await;
    let database = services.connect_database().await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build E2E HTTP client");

    let mut primary_a = ServerProcess::spawn("primary-a", &services);
    wait_for_health(&client, &mut primary_a).await;
    let initial_lease = wait_for_runtime_lease(&database, &mut primary_a).await;
    let mut primary_b = ServerProcess::spawn("primary-b", &services);
    wait_for_health(&client, &mut primary_b).await;

    primary_a.terminate_gracefully();
    let takeover = wait_for_new_runtime_owner(
        &database,
        &mut primary_b,
        &initial_lease.owner_id,
        Duration::from_secs(20),
    )
    .await;
    assert_ne!(takeover.owner_id, initial_lease.owner_id);

    primary_b.terminate();
    database
        .close()
        .await
        .expect("close graceful shutdown E2E database connection");
    services.cleanup_database().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Docker, two real AsterDrive primaries, and a synthetic tunnel follower"]
async fn reverse_tunnel_request_hitting_non_owner_primary_streams_through_owner() {
    let _guard = e2e_lock().lock().await;
    let services = SharedServices::start().await;
    let database = services.connect_database().await;
    defer_remote_node_health_tests(&database).await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .expect("build reverse tunnel E2E HTTP client");

    let mut primary_a = ServerProcess::spawn("primary-a", &services);
    wait_for_health(&client, &mut primary_a).await;
    let access_token = setup_and_login(&client, &primary_a).await;
    let mut primary_b = ServerProcess::spawn("primary-b", &services);
    wait_for_health(&client, &mut primary_b).await;
    let remote_node = seed_reverse_tunnel_node(&database).await;

    let follower = spawn_synthetic_tunnel_follower(primary_a.base_url(), remote_node.clone());
    let owner = wait_for_tunnel_owner(
        &database,
        remote_node.id,
        &primary_a.base_url(),
        &mut primary_a,
        Duration::from_secs(15),
    )
    .await;
    assert_eq!(owner.remote_node_id, remote_node.id);

    let probe = test_remote_node_through(&client, &primary_b, &access_token, remote_node.id).await;
    assert_eq!(
        probe["data"]["capabilities"]["protocol_version"],
        RemoteStorageCapabilities::current().protocol_version
    );
    assert_eq!(probe["data"]["tunnel"]["status"], "online");

    follower.stop().await;
    primary_a.terminate();
    primary_b.terminate();
    database
        .close()
        .await
        .expect("close reverse tunnel routing E2E database connection");
    services.cleanup_database().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Docker, two real AsterDrive primaries, and a synthetic tunnel follower"]
async fn reverse_tunnel_owner_failover_fences_stale_primary() {
    let _guard = e2e_lock().lock().await;
    let services = SharedServices::start().await;
    let database = services.connect_database().await;
    defer_remote_node_health_tests(&database).await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .expect("build reverse tunnel failover E2E HTTP client");

    let mut primary_a = ServerProcess::spawn("primary-a", &services);
    wait_for_health(&client, &mut primary_a).await;
    let access_token = setup_and_login(&client, &primary_a).await;
    let mut primary_b = ServerProcess::spawn("primary-b", &services);
    wait_for_health(&client, &mut primary_b).await;
    let remote_node = seed_reverse_tunnel_node(&database).await;

    let follower_a = spawn_synthetic_tunnel_follower(primary_a.base_url(), remote_node.clone());
    let owner_a = wait_for_tunnel_owner(
        &database,
        remote_node.id,
        &primary_a.base_url(),
        &mut primary_a,
        Duration::from_secs(15),
    )
    .await;
    primary_a.terminate();
    follower_a.stop().await;
    let follower_b = spawn_synthetic_tunnel_follower(primary_b.base_url(), remote_node.clone());
    let owner_b = wait_for_tunnel_owner(
        &database,
        remote_node.id,
        &primary_b.base_url(),
        &mut primary_b,
        Duration::from_secs(65),
    )
    .await;
    assert_ne!(owner_b.fencing_token, owner_a.fencing_token);

    let probe = test_remote_node_through(&client, &primary_b, &access_token, remote_node.id).await;
    assert_eq!(probe["data"]["tunnel"]["status"], "online");
    let (stale_status, stale_body) =
        stale_fencing_proxy_response(&client, &primary_b, remote_node.id, &owner_a.fencing_token)
            .await;
    assert!(
        !stale_status.is_success() && stale_body.contains("fencing token is stale"),
        "stale owner token should be rejected, got {stale_status}: {stale_body}"
    );

    follower_b.stop().await;
    primary_b.terminate();
    database
        .close()
        .await
        .expect("close reverse tunnel failover E2E database connection");
    services.cleanup_database().await;
}

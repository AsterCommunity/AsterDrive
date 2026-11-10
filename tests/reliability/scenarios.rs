//! Named checkpoint process controller and reference workflows.

use std::{path::PathBuf, process::Command, sync::OnceLock, time::Duration};

use aster_drive::db::repository::{background_task_repo, config_repo, policy_repo};
use aster_drive_migration::Migrator;
use aster_drive_model::{
    entities::{background_task, file, file_blob, storage_policy, upload_session},
    types::{BackgroundTaskStatus, StoredStoragePolicyAllowedTypes, StoredStoragePolicyConfig},
};
use aster_forge_test::{
    postgres::{PostgresTestContainer, PostgresTestDatabase},
    process::{TestProcess, available_loopback_port},
    redis::RedisTestContainer,
    suite::TestContainerSuite,
    temp::TestTempDir,
};
use chrono::Utc;
use sea_orm::{ColumnTrait, Database, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{
    controller::{CheckpointController, Selection},
    invariants,
};

const SECRET: &str = "asterdrive522abcdef0123456789abcdef0123456789abcdef0123456789abcd";
const PASSWORD: &str = "AsterDrive-Reliability-522!";
const TIMEOUT: Duration = Duration::from_secs(120);

struct DurableState {
    dir: TestTempDir,
    db: DatabaseConnection,
    url: String,
    _database: Option<PostgresTestDatabase>,
    _postgres: Option<PostgresTestContainer>,
    redis: Option<RedisTestContainer>,
}

impl DurableState {
    async fn create() -> Self {
        let dir = TestTempDir::new("reliability");
        let backend =
            std::env::var("ASTER_TEST_DATABASE_BACKEND").unwrap_or_else(|_| "sqlite".into());
        assert!(
            matches!(backend.as_str(), "sqlite" | "postgres"),
            "unsupported reliability database backend"
        );
        let suite = suite();
        let postgres = if backend == "postgres" {
            Some(PostgresTestContainer::start(suite).await)
        } else {
            None
        };
        let database = if let Some(postgres) = &postgres {
            Some(
                postgres
                    .create_database(&format!("reliability_{}", uuid::Uuid::new_v4().simple()))
                    .await,
            )
        } else {
            None
        };
        let url = database
            .as_ref()
            .map(|db| db.url().to_string())
            .unwrap_or_else(|| format!("sqlite://{}?mode=rwc", dir.join("state.sqlite").display()));
        let db = Database::connect(&url)
            .await
            .expect("connect durable test database");
        Migrator::up(&db, None)
            .await
            .expect("migrate durable test database");
        let config = aster_drive_storage::encode_storage_policy_config(
            aster_drive_storage::ConnectorConfigEnvelope::new(
                aster_drive_storage::ConnectorId::declared("asterdrive.storage.local"),
                1,
                json!({"base_path":dir.join("objects"), "content_dedup":true}),
            ),
            aster_drive_storage::StoragePolicyBehaviorConfig::default(),
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("objects")).unwrap();
        policy_repo::create(
            &db,
            storage_policy::ActiveModel {
                name: Set("Reliability local storage".into()),
                connector_id: Set("asterdrive.storage.local".into()),
                storage_config: Set(StoredStoragePolicyConfig::from(config)),
                is_default: Set(true),
                max_file_size: Set(0),
                allowed_types: Set(StoredStoragePolicyAllowedTypes::empty()),
                chunk_size: Set(2048),
                created_at: Set(Utc::now()),
                updated_at: Set(Utc::now()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        aster_drive::services::storage_policy::policy::ensure_policy_groups_seeded(&db)
            .await
            .unwrap();
        config_repo::ensure_defaults_with_env(&db, &|_| None::<String>)
            .await
            .unwrap();
        for (key, value) in [
            ("background_task_dispatch_interval_secs", "1"),
            ("background_task_dispatch_idle_max_interval_secs", "1"),
            ("archive_compress_enabled", "true"),
        ] {
            config_repo::upsert_with_actor(&db, key, value, None)
                .await
                .unwrap();
        }
        let redis = if postgres.is_some() {
            Some(RedisTestContainer::start(suite).await)
        } else {
            None
        };
        Self {
            dir,
            db,
            url,
            _database: database,
            _postgres: postgres,
            redis,
        }
    }
}

fn suite() -> &'static TestContainerSuite {
    static SUITE: OnceLock<TestContainerSuite> = OnceLock::new();
    SUITE.get_or_init(|| TestContainerSuite::new("asterdrive-reliability"))
}

pub(super) struct Server {
    process: TestProcess,
    pub(super) url: String,
    artifact: PathBuf,
}

impl Server {
    fn spawn(
        state: &DurableState,
        evidence: &Evidence,
        generation: u32,
        checkpoint: Option<&str>,
    ) -> Self {
        let port = available_loopback_port();
        let mut command = Command::new(env!("CARGO_BIN_EXE_reliability_fixture"));
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("ASTER__")
                || key.to_string_lossy().starts_with("ASTER_RELIABILITY_")
            {
                command.env_remove(key);
            }
        }
        command
            .args(["serve", "all"])
            .env("ASTER__DEPLOYMENT__PROFILE", "single")
            .env("ASTER__SERVER__HOST", "127.0.0.1")
            .env("ASTER__SERVER__PORT", port.to_string())
            .env("ASTER__SERVER__WORKERS", "2")
            .env("ASTER__SERVER__UPLOAD_TEMP_DIR", state.dir.join("uploads"))
            .env("ASTER__SERVER__TEMP_DIR", state.dir.join("tasks"))
            .env("ASTER__SERVER__UPLOAD_TEMP_MIN_FREE_BYTES", "0")
            .env("ASTER__DATABASE__URL", &state.url)
            .env("ASTER__LOGGING__LEVEL", "warn")
            .env("ASTER__AUTH__BOOTSTRAP_INSECURE_COOKIES", "true");
        for key in [
            "JWT_SECRET",
            "SHARE_COOKIE_SECRET",
            "DIRECT_LINK_SECRET",
            "MFA_SECRET_KEY",
            "STORAGE_CREDENTIAL_SECRET_KEY",
        ] {
            command.env(format!("ASTER__AUTH__{key}"), SECRET);
        }
        if let Some(redis) = &state.redis {
            command
                .env("ASTER__CACHE__BACKEND", "redis")
                .env("ASTER__CACHE__ENDPOINT", redis.url())
                .env("ASTER__CONFIG_SYNC__BACKEND", "redis")
                .env("ASTER__CONFIG_SYNC__ENDPOINT", redis.url())
                .env(
                    "ASTER__CONFIG_SYNC__TOPIC",
                    state.dir.path().file_name().unwrap(),
                );
        }
        if let Some(checkpoint) = checkpoint {
            command
                .env("ASTER_RELIABILITY_CHECKPOINT", checkpoint)
                .env("ASTER_RELIABILITY_GENERATION", generation.to_string())
                .env(
                    "ASTER_RELIABILITY_SOCKET",
                    state.dir.join(format!("ipc-{generation}")),
                );
        }
        evidence.write(
            &format!("generation-{generation}-spawn.json"),
            &json!({"generation":generation,"armed":checkpoint.is_some()}),
        );
        Self {
            process: TestProcess::spawn(&format!("reliability-{generation}"), &mut command),
            url: format!("http://127.0.0.1:{port}"),
            artifact: evidence.root.join(format!("generation-{generation}.log")),
        }
    }

    async fn ready(&mut self, client: &aster_forge_http::HttpClient) {
        let result = tokio::time::timeout(TIMEOUT, async {
            loop {
                // TestProcess's panic includes raw logs. Persist only redacted logs below.
                let status = client
                    .get(format!("{}/health/ready", self.url))
                    .send()
                    .await;
                if status.is_ok_and(|r| r.status().is_success()) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await;
        assert!(
            result.is_ok(),
            "readiness timeout; see redacted process artifact"
        );
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.process.terminate();
        let logs = invariants::redact_logs(
            &self.process.diagnostics(),
            &[SECRET, PASSWORD, super::minio::SECRET],
        );
        if let Err(error) = std::fs::write(&self.artifact, logs) {
            eprintln!("cannot persist redacted process artifact: {error}");
        }
    }
}

struct Evidence {
    root: PathBuf,
}

impl Evidence {
    fn new(selection: &Selection, checkpoint: &str) -> Self {
        let root = std::env::var_os("ASTER_RELIABILITY_ARTIFACT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/reliability"))
            .join(format!(
                "{}-{}-{}",
                selection.seed,
                checkpoint,
                uuid::Uuid::new_v4().simple()
            ));
        std::fs::create_dir_all(&root).unwrap();
        let evidence = Self { root };
        evidence.write("replay.json", &json!({"seed":selection.seed,"checkpoint":checkpoint,"head":option_env!("ASTER_BUILD_REVISION")}));
        evidence
    }

    fn write(&self, name: &str, value: &impl serde::Serialize) {
        std::fs::write(
            self.root.join(name),
            serde_json::to_vec_pretty(value).unwrap(),
        )
        .unwrap();
    }

    async fn inspect(&self, name: &str, state: &DurableState, token: &str) -> invariants::Snapshot {
        let snapshot =
            invariants::inspect(&state.db, &[SECRET, PASSWORD, token, super::minio::SECRET]).await;
        self.write(name, &snapshot);
        snapshot
    }
}

fn client() -> aster_forge_http::HttpClient {
    aster_forge_http::HttpClient::builder()
        .no_proxy()
        .timeout(Duration::from_secs(110))
        .build()
        .unwrap()
}

pub(super) async fn data(request: aster_forge_http::HttpRequestBuilder) -> Value {
    let response = request.send().await.expect("fixture HTTP request failed");
    let status = response.status();
    // Product bodies can contain tokens/URLs. Never print them in assertions.
    assert!(
        status.is_success(),
        "fixture HTTP request returned {status}"
    );
    response
        .json::<Value>()
        .await
        .expect("decode fixture response")["data"]
        .clone()
}

async fn login(client: &aster_forge_http::HttpClient, server: &Server) -> String {
    data(
        client
            .post(format!("{}/api/v1/auth/setup", server.url))
            .json(
                &json!({"username":"admin","email":"reliability@example.com","password":PASSWORD}),
            ),
    )
    .await;
    let response = client
        .post(format!("{}/api/v1/auth/login", server.url))
        .json(&json!({"identifier":"admin","password":PASSWORD}))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success(), "fixture login failed");
    response
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|h| h.to_str().ok())
        .find_map(|cookie| {
            cookie
                .strip_prefix("aster_access=")
                .and_then(|c| c.split(';').next())
                .map(str::to_owned)
        })
        .expect("fixture access cookie missing")
}

async fn init(
    client: &aster_forge_http::HttpClient,
    server: &Server,
    token: &str,
    payload: &[u8],
) -> String {
    let result = data(
        client
            .post(format!("{}/api/v1/files/upload/init", server.url))
            .bearer_auth(token)
            .json(&json!({"filename":"source.bin","total_size":payload.len()})),
    )
    .await;
    assert_eq!(result["mode"], "chunked");
    result["upload_id"].as_str().unwrap().into()
}

fn chunk(
    client: &aster_forge_http::HttpClient,
    server: &Server,
    token: &str,
    upload: &str,
    number: usize,
    payload: &[u8],
) -> aster_forge_http::HttpRequestBuilder {
    client
        .put(format!(
            "{}/api/v1/files/upload/{upload}/{number}",
            server.url
        ))
        .bearer_auth(token)
        .header("Content-Type", "application/octet-stream")
        .body(payload.to_vec())
}

async fn upload_chunks(
    client: &aster_forge_http::HttpClient,
    server: &Server,
    token: &str,
    upload: &str,
    payload: &[u8],
) {
    for (number, bytes) in payload.chunks(2048).enumerate() {
        data(chunk(client, server, token, upload, number, bytes)).await;
    }
}

async fn retry_completion(
    client: &aster_forge_http::HttpClient,
    server: &Server,
    token: &str,
    upload: &str,
) -> Value {
    tokio::time::timeout(TIMEOUT, async {
        loop {
            let response = complete(client, server, token, upload)
                .send()
                .await
                .unwrap();
            let status = response.status();
            let body: Value = response.json().await.unwrap();
            if body["code"] == "upload.assembling" {
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }
            assert!(
                status.is_success() && !body["data"].is_null(),
                "upload recovery failed with {status}; body omitted for redaction"
            );
            break body["data"].clone();
        }
    })
    .await
    .expect("upload assembly recovery timeout")
}

fn complete(
    client: &aster_forge_http::HttpClient,
    server: &Server,
    token: &str,
    upload: &str,
) -> aster_forge_http::HttpRequestBuilder {
    client
        .post(format!(
            "{}/api/v1/files/upload/{upload}/complete",
            server.url
        ))
        .bearer_auth(token)
        .json(&json!({}))
}

async fn download(
    client: &aster_forge_http::HttpClient,
    server: &Server,
    token: &str,
    id: i64,
) -> Vec<u8> {
    let response = client
        .get(format!("{}/api/v1/files/{id}/download", server.url))
        .bearer_auth(token)
        .send()
        .await
        .unwrap();
    assert!(
        response.status().is_success(),
        "completed file must be readable"
    );
    response.bytes().await.unwrap().to_vec()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "real SIGKILL/restart; run make reliability-e2e"]
async fn crash_recovery() {
    let selection = Selection::from_env();
    let object_storage =
        std::env::var("ASTER_RELIABILITY_STORAGE").unwrap_or_else(|_| "local".into());
    assert!(
        matches!(object_storage.as_str(), "local" | "minio"),
        "unknown reliability storage"
    );
    assert!(
        object_storage != "minio" || selection.checkpoints.iter().all(|c| c.starts_with("task.")),
        "MinIO applies to archive checkpoints; offset staging requires local storage"
    );
    for checkpoint in &selection.checkpoints {
        let evidence = Evidence::new(&selection, checkpoint);
        let state = DurableState::create().await;
        let mut controller = CheckpointController::bind(&state.dir.join("ipc-1"), "1", checkpoint);
        let client = client();
        let mut server = Server::spawn(&state, &evidence, 1, Some(checkpoint));
        server.ready(&client).await;
        let token = login(&client, &server).await;
        let mut minio = if object_storage == "minio" {
            Some(super::minio::Minio::start().await)
        } else {
            None
        };
        let payload = selection.payload();
        let upload = init(&client, &server, &token, &payload).await;
        let request = if checkpoint.ends_with("after_staging_sync_before_receipt_transaction") {
            chunk(&client, &server, &token, &upload, 0, &payload[..2048])
        } else {
            upload_chunks(&client, &server, &token, &upload, &payload).await;
            complete(&client, &server, &token, &upload)
        };
        let mut task_id = None;
        let pending = if checkpoint.starts_with("task.") {
            let source = data(request).await;
            if let Some(minio) = &mut minio {
                minio.route_archives(&client, &server, &token).await;
            }
            let task = data(client.post(format!("{}/api/v1/batch/archive-compress", server.url)).bearer_auth(&token)
                .json(&json!({"file_ids":[source["id"]],"folder_ids":[],"archive_name":"durable.zip"}))).await;
            task_id = Some(task["id"].as_i64().unwrap());
            None
        } else {
            Some(tokio::spawn(async move { request.send().await }))
        };
        let reached = controller.wait(TIMEOUT).await;
        evidence.write("checkpoint.json", &json!({"generation":1,"checkpoint":checkpoint,"reached":reached.is_ok(),"error":reached.as_ref().err()}));
        assert!(
            reached.is_ok(),
            "checkpoint evidence missing; see redacted artifacts"
        );
        let before = evidence.inspect("before-kill.json", &state, &token).await;
        before.assert_valid();
        // TestProcess::terminate is Child::kill (SIGKILL on Unix), followed by wait.
        server.process.terminate();
        evidence.write(
            "killed.json",
            &json!({"generation":1,"signal":"SIGKILL","checkpoint_confirmed":true}),
        );
        controller.assert_no_duplicate().await;
        if let Some(pending) = pending {
            let _connection_closed = pending.await.unwrap();
        }
        let claimed = if let Some(id) = task_id {
            Some(
                background_task::Entity::find_by_id(id)
                    .one(&state.db)
                    .await
                    .unwrap()
                    .unwrap(),
            )
        } else {
            None
        };
        drop(server);
        let mut recovery_controller = task_id.map(|_| {
            CheckpointController::bind(
                &state.dir.join("ipc-2"),
                "2",
                "task.after_claim_before_handler",
            )
        });
        let mut restarted = Server::spawn(
            &state,
            &evidence,
            2,
            task_id.map(|_| "task.after_claim_before_handler"),
        );
        restarted.ready(&client).await;
        if let Some(claimed) = claimed {
            recover_task(
                RecoveryContext {
                    state: &state,
                    client: &client,
                    restarted: &restarted,
                    token: &token,
                    payload: &payload,
                    evidence: &evidence,
                },
                &claimed,
                recovery_controller.as_mut().unwrap(),
            )
            .await;
        } else {
            if checkpoint.ends_with("after_staging_sync_before_receipt_transaction") {
                let session = upload_session::Entity::find_by_id(upload.clone())
                    .one(&state.db)
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(session.received_count, 0, "synced bytes are not a receipt");
                upload_chunks(&client, &restarted, &token, &upload, &payload).await;
                upload_chunks(&client, &restarted, &token, &upload, &payload).await;
                let session = upload_session::Entity::find_by_id(upload.clone())
                    .one(&state.db)
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    session.received_count, 2,
                    "range retries cannot double-count receipts"
                );
            }
            let file = retry_completion(&client, &restarted, &token, &upload).await;
            let repeated = data(complete(&client, &restarted, &token, &upload)).await;
            assert_eq!(
                file["id"], repeated["id"],
                "completed-session retry must reuse product result"
            );
            assert_eq!(
                download(&client, &restarted, &token, file["id"].as_i64().unwrap()).await,
                payload
            );
            let after = evidence
                .inspect("after-recovery.json", &state, &token)
                .await;
            after.assert_valid();
            assert_eq!(after.files, 1);
            assert_eq!(after.quota_used, i64::try_from(payload.len()).unwrap());
            assert_local_objects_tracked(&state).await;
            assert!(
                !state.dir.join("uploads").join(&upload).exists(),
                "completed upload retry must clean staging state"
            );
        }
        restarted.ready(&client).await;
        if let Some(minio) = &minio {
            minio.assert_objects_tracked(&state.db).await;
        }
        data(
            client
                .get(format!("{}/api/v1/folders", restarted.url))
                .bearer_auth(&token),
        )
        .await;
        evidence.write(
            "complete.json",
            &json!({"checkpoint":checkpoint,"seed":selection.seed,"recovered":true}),
        );
        drop(restarted);
        state.db.close().await.unwrap();
        if let Some(database) = state._database {
            database.cleanup().await;
        }
    }
}

async fn assert_local_objects_tracked(state: &DurableState) {
    let blobs = file_blob::Entity::find().all(&state.db).await.unwrap();
    let mut directories = vec![state.dir.join("objects")];
    while let Some(dir) = directories.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                directories.push(entry.path());
                continue;
            }
            let path = entry.path();
            assert!(
                blobs
                    .iter()
                    .any(|b| b.storage_path.as_ref().is_some_and(|p| state
                        .dir
                        .join("objects")
                        .join(p)
                        == path)),
                "untracked local object after recovery"
            );
        }
    }
}

struct RecoveryContext<'a> {
    state: &'a DurableState,
    client: &'a aster_forge_http::HttpClient,
    restarted: &'a Server,
    token: &'a str,
    payload: &'a [u8],
    evidence: &'a Evidence,
}

async fn recover_task(
    context: RecoveryContext<'_>,
    claimed: &background_task::Model,
    controller: &mut CheckpointController,
) {
    let RecoveryContext {
        state,
        client,
        restarted,
        token,
        payload,
        evidence,
    } = context;
    assert_eq!(claimed.status, BackgroundTaskStatus::Processing);
    // Probe the real claim CAS before expiry, without artificially expiring the lease.
    let now = Utc::now();
    assert!(
        !background_task_repo::try_claim(
            &state.db,
            claimed.id,
            claimed.processing_token,
            now,
            now - chrono::Duration::seconds(60),
            claimed.processing_token + 1,
            now + chrono::Duration::seconds(60)
        )
        .await
        .unwrap()
    );
    controller
        .wait(TIMEOUT)
        .await
        .expect("reclaimed owner did not reach checkpoint");
    let reclaimed = background_task::Entity::find_by_id(claimed.id)
        .one(&state.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reclaimed.status, BackgroundTaskStatus::Processing);
    assert_eq!(reclaimed.processing_token, claimed.processing_token + 1);
    // The archive owner remains blocked here while a different dispatcher lane runs.
    let independent = data(
        client
            .delete(format!("{}/api/v1/trash", restarted.url))
            .bearer_auth(token),
    )
    .await;
    let independent_id = independent["id"].as_i64().unwrap();
    tokio::time::timeout(TIMEOUT, async {
        loop {
            let task = background_task::Entity::find_by_id(independent_id)
                .one(&state.db)
                .await
                .unwrap()
                .unwrap();
            if task.status == BackgroundTaskStatus::Succeeded {
                break;
            }
            assert_ne!(
                task.status,
                BackgroundTaskStatus::Failed,
                "unrelated lane failed"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("unrelated dispatcher lane stalled behind archive recovery");
    let now = Utc::now();
    assert!(
        !background_task_repo::mark_progress(
            &state.db,
            background_task_repo::TaskProgressUpdate {
                id: claimed.id,
                processing_token: claimed.processing_token,
                now,
                lease_expires_at: now + chrono::Duration::seconds(60),
                current: 999,
                total: 999,
                status_text: None,
                steps_json: None
            }
        )
        .await
        .unwrap()
    );
    assert!(
        !background_task_repo::set_runtime_json(
            &state.db,
            claimed.id,
            claimed.processing_token,
            Some("{}"),
            now
        )
        .await
        .unwrap()
    );
    assert!(
        !background_task_repo::mark_retry(
            &state.db,
            claimed.id,
            claimed.processing_token,
            99,
            now,
            "stale execution",
            None
        )
        .await
        .unwrap()
    );
    assert!(
        !background_task_repo::mark_succeeded(
            &state.db,
            background_task_repo::TaskSuccessUpdate {
                id: claimed.id,
                processing_token: claimed.processing_token,
                result_json: Some("{}"),
                steps_json: None,
                current: 999,
                total: 999,
                status_text: None,
                finished_at: now,
                expires_at: now + chrono::Duration::hours(1),
            }
        )
        .await
        .unwrap()
    );
    assert!(
        !background_task_repo::mark_failed(
            &state.db,
            background_task_repo::TaskFailureUpdate {
                id: claimed.id,
                processing_token: claimed.processing_token,
                attempt_count: 99,
                last_error: "stale execution",
                finished_at: now,
                expires_at: now + chrono::Duration::hours(1),
                steps_json: None,
                failure_can_retry: false,
            }
        )
        .await
        .unwrap()
    );
    controller.release().await;
    tokio::time::timeout(TIMEOUT, async {
        loop {
            let task = background_task::Entity::find_by_id(claimed.id)
                .one(&state.db)
                .await
                .unwrap()
                .unwrap();
            if task.status == BackgroundTaskStatus::Succeeded {
                assert_eq!(task.processing_token, claimed.processing_token + 1);
                break;
            }
            assert!(
                !matches!(
                    task.status,
                    BackgroundTaskStatus::Failed | BackgroundTaskStatus::Canceled
                ),
                "archive recovery entered terminal failure"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("archive task reclaim/finish timeout");
    let files = file::Entity::find()
        .filter(file::Column::Name.eq("durable.zip"))
        .all(&state.db)
        .await
        .unwrap();
    assert_eq!(
        files.len(),
        1,
        "archive recovery must create one durable result"
    );
    let zip = download(client, restarted, token, files[0].id).await;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).unwrap();
    let mut contents = Vec::new();
    std::io::Read::read_to_end(&mut archive.by_index(0).unwrap(), &mut contents).unwrap();
    assert_eq!(Sha256::digest(&contents), Sha256::digest(payload));
    let after = evidence.inspect("after-recovery.json", state, token).await;
    after.assert_valid();
    assert_eq!(after.files, 2);
    assert_local_objects_tracked(state).await;
}

//! 上传集成测试（分片 + presigned）

#[macro_use]
mod common;

use actix_web::test;
use aster_drive::api::api_error_code::ApiErrorCode;
use aster_drive::db::repository::policy_repo;
use aster_drive::runtime::SharedRuntimeState;
use aster_drive::services::auth::local;
use aster_drive::types::UploadSessionKind;
use serde_json::Value;
use testcontainers::{GenericImage, ImageExt, runners::AsyncRunner};
use tokio::task::JoinSet;
use tokio::time::{Duration, timeout};

const TEST_CHUNK_SIZE: usize = 5_242_880;
const PERSONAL_FINALIZATION_CONCURRENCY: usize = 8;
const TEAM_FINALIZATION_CONCURRENCY: usize = 4;
const RUSTFS_TEST_IMAGE_TAG: &str = "1.0.0-alpha.90";

async fn hold_personal_quota_row(
    state: &aster_drive::runtime::PrimaryAppState,
    user_id: i64,
) -> Option<sea_orm::DatabaseTransaction> {
    use aster_drive::db::repository::user_repo;
    if state.writer_db().get_database_backend() == sea_orm::DbBackend::Sqlite {
        return None;
    }
    let txn = aster_forge_db::transaction::begin(state.writer_db())
        .await
        .unwrap();
    user_repo::lock_by_id(&txn, user_id).await.unwrap();
    Some(txn)
}

async fn hold_team_quota_row(
    state: &aster_drive::runtime::PrimaryAppState,
    team_id: i64,
) -> Option<sea_orm::DatabaseTransaction> {
    use aster_drive::db::repository::team_repo;
    if state.writer_db().get_database_backend() == sea_orm::DbBackend::Sqlite {
        return None;
    }
    let txn = aster_forge_db::transaction::begin(state.writer_db())
        .await
        .unwrap();
    team_repo::lock_by_id(&txn, team_id).await.unwrap();
    Some(txn)
}

async fn assert_tasks_wait_for_quota_guard<T: Send + 'static>(tasks: &mut JoinSet<T>) {
    assert!(
        timeout(Duration::from_millis(100), tasks.join_next())
            .await
            .is_err(),
        "a finalization task crossed the quota-row lock before the guard was released"
    );
}

fn assert_upload_error_contract(body: &Value, expected_code: &str) {
    assert_eq!(body["code"], expected_code);
    assert_eq!(body["error"]["retryable"], false);
    assert!(body["error"].get("internal_code").is_none());
    assert!(body["error"].get("subcode").is_none());
}

fn new_test_upload_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

async fn reload_policy_snapshot(state: &aster_drive::runtime::PrimaryAppState) {
    state
        .policy_snapshot
        .reload(state.writer_db())
        .await
        .unwrap();
}

async fn set_default_local_content_dedup(
    state: &aster_drive::runtime::PrimaryAppState,
    enabled: bool,
) {
    use aster_drive::db::repository::policy_repo;
    use sea_orm::{ActiveModelTrait, Set};

    let policy = policy_repo::find_default(state.writer_db())
        .await
        .unwrap()
        .expect("default policy should exist in test setup");
    let mut active: aster_drive::entities::storage_policy::ActiveModel = policy.into();
    active.options = Set(aster_drive::types::StoredStoragePolicyOptions::from(
        if enabled {
            r#"{"content_dedup":true}"#
        } else {
            "{}"
        }
        .to_string(),
    ));
    active.update(state.writer_db()).await.unwrap();
    reload_policy_snapshot(state).await;
}

async fn upload_same_content_direct_and_chunked(
    state: &aster_drive::runtime::PrimaryAppState,
    user_id: i64,
) -> (
    aster_drive::entities::file_blob::Model,
    aster_drive::entities::file_blob::Model,
) {
    use aster_drive::db::repository::file_repo;
    use aster_drive::services::{files::file, files::upload};

    let pattern = b"same content across direct and chunked upload paths\n";
    let content = pattern.repeat((10_485_760 / pattern.len()) + 1);
    let content = &content[..10_485_760];
    let temp_path = aster_forge_utils::paths::temp_file_path(
        &state.config.server.temp_dir,
        &uuid::Uuid::new_v4().to_string(),
    );
    tokio::fs::create_dir_all(&state.config.server.temp_dir)
        .await
        .unwrap();
    tokio::fs::write(&temp_path, content).await.unwrap();

    let direct_file = file::store_from_temp(
        state,
        user_id,
        file::StoreFromTempRequest::new(None, "same-direct.txt", &temp_path, content.len() as i64),
    )
    .await
    .unwrap();

    let init = upload::init_upload(
        state,
        user_id,
        "same-chunked.txt",
        content.len() as i64,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(init.mode, aster_drive::types::UploadMode::Chunked);

    let upload_id = init.upload_id.unwrap();
    let total_chunks = init.total_chunks.unwrap();
    let chunk_size = init.chunk_size.unwrap() as usize;
    for chunk_number in 0..total_chunks {
        let start = chunk_number as usize * chunk_size;
        let end = ((chunk_number as usize + 1) * chunk_size).min(content.len());
        let chunk = &content[start..end];
        upload::upload_chunk(state, &upload_id, chunk_number, user_id, chunk)
            .await
            .unwrap();
    }
    let chunked_file = upload::complete_upload(state, &upload_id, user_id, None)
        .await
        .unwrap();

    let direct_blob = file_repo::find_blob_by_id(state.writer_db(), direct_file.blob_id)
        .await
        .unwrap();
    let chunked_blob = file_repo::find_blob_by_id(state.writer_db(), chunked_file.blob_id)
        .await
        .unwrap();

    let _ = tokio::fs::remove_file(&temp_path).await;
    (direct_blob, chunked_blob)
}

struct UploadSessionSpec<'a> {
    upload_id: &'a str,
    status: aster_drive::types::UploadSessionStatus,
    expires_at: chrono::DateTime<chrono::Utc>,
    total_chunks: i32,
    received_count: i32,
    session_kind: aster_drive::types::UploadSessionKind,
    policy_id: Option<i64>,
    object_temp_key: Option<&'a str>,
    object_multipart_id: Option<&'a str>,
    file_id: Option<i64>,
}

impl<'a> UploadSessionSpec<'a> {
    fn new(
        upload_id: &'a str,
        status: aster_drive::types::UploadSessionStatus,
        expires_at: chrono::DateTime<chrono::Utc>,
        session_kind: UploadSessionKind,
    ) -> Self {
        Self {
            upload_id,
            status,
            expires_at,
            total_chunks: 0,
            received_count: 0,
            session_kind,
            policy_id: None,
            object_temp_key: None,
            object_multipart_id: None,
            file_id: None,
        }
    }

    fn chunks(mut self, total_chunks: i32, received_count: i32) -> Self {
        self.total_chunks = total_chunks;
        self.received_count = received_count;
        self
    }

    fn policy(mut self, policy_id: i64) -> Self {
        self.policy_id = Some(policy_id);
        self
    }

    fn object_upload(
        mut self,
        object_temp_key: Option<&'a str>,
        object_multipart_id: Option<&'a str>,
    ) -> Self {
        self.object_temp_key = object_temp_key;
        self.object_multipart_id = object_multipart_id;
        self
    }

    fn file_id(mut self, file_id: i64) -> Self {
        self.file_id = Some(file_id);
        self
    }
}

async fn create_upload_session(
    state: &aster_drive::runtime::PrimaryAppState,
    user_id: i64,
    spec: UploadSessionSpec<'_>,
) {
    use aster_drive::db::repository::{policy_repo, upload_session_repo};
    use sea_orm::Set;

    let policy = policy_repo::find_default(state.writer_db())
        .await
        .unwrap()
        .expect("default policy should exist in test setup");
    let policy_id = spec.policy_id.unwrap_or(policy.id);
    let now = chrono::Utc::now();
    upload_session_repo::create(
        state.writer_db(),
        aster_drive::entities::upload_session::ActiveModel {
            id: Set(spec.upload_id.to_string()),
            user_id: Set(user_id),
            team_id: Set(None),
            frontend_client_id: Set(None),
            filename: Set("manual-upload.bin".to_string()),
            total_size: Set(10),
            chunk_size: Set(5),
            total_chunks: Set(spec.total_chunks),
            received_count: Set(spec.received_count),
            folder_id: Set(None),
            policy_id: Set(policy_id),
            status: Set(spec.status),
            session_kind: Set(spec.session_kind),
            object_temp_key: Set(spec.object_temp_key.map(str::to_string)),
            object_multipart_id: Set(spec.object_multipart_id.map(str::to_string)),
            provider_session_ciphertext: Set(None),
            file_id: Set(spec.file_id),
            created_at: Set(now),
            expires_at: Set(spec.expires_at),
            updated_at: Set(now),
        },
    )
    .await
    .unwrap();
}

#[actix_web::test]
async fn test_upload_session_try_create_reports_id_conflict() {
    use aster_drive::db::repository::{policy_repo, upload_session_repo};
    use sea_orm::Set;

    let state = common::setup().await;
    let user =
        common::create_test_account(&state, "trycreateuser", "trycreate@test.com", "password123")
            .await
            .unwrap();
    let policy = policy_repo::find_default(state.writer_db())
        .await
        .unwrap()
        .expect("default policy should exist");
    let upload_id = new_test_upload_id();
    let now = chrono::Utc::now();

    let build_model = || aster_drive::entities::upload_session::ActiveModel {
        id: Set(upload_id.clone()),
        user_id: Set(user.id),
        team_id: Set(None),
        frontend_client_id: Set(None),
        filename: Set("try-create.bin".to_string()),
        total_size: Set(1),
        chunk_size: Set(1),
        total_chunks: Set(1),
        received_count: Set(0),
        folder_id: Set(None),
        policy_id: Set(policy.id),
        status: Set(aster_drive::types::UploadSessionStatus::Uploading),
        session_kind: Set(aster_drive::types::UploadSessionKind::OffsetStaging),
        object_temp_key: Set(None),
        object_multipart_id: Set(None),
        provider_session_ciphertext: Set(None),
        file_id: Set(None),
        created_at: Set(now),
        expires_at: Set(now + chrono::Duration::hours(1)),
        updated_at: Set(now),
    };

    assert!(
        upload_session_repo::try_create(state.writer_db(), build_model())
            .await
            .unwrap()
    );
    assert!(
        !upload_session_repo::try_create(state.writer_db(), build_model())
            .await
            .unwrap()
    );
}

#[actix_web::test]
async fn test_upload_session_try_create_preserves_non_id_unique_conflict() {
    use aster_drive::db::repository::{policy_repo, upload_session_repo};
    use sea_orm::{ConnectionTrait, Set};

    let state = common::setup().await;
    state
        .writer_db()
        .execute_unprepared(
            "CREATE UNIQUE INDEX uq_upload_sessions_filename_test ON upload_sessions (filename)",
        )
        .await
        .unwrap();
    let user = common::create_test_account(
        &state,
        "trycreateuniq",
        "trycreateuniq@test.com",
        "password123",
    )
    .await
    .unwrap();
    let policy = policy_repo::find_default(state.writer_db())
        .await
        .unwrap()
        .expect("default policy should exist");
    let filename = format!("try-create-unique-{}.bin", new_test_upload_id());
    let now = chrono::Utc::now();

    let build_model = |upload_id: String| aster_drive::entities::upload_session::ActiveModel {
        id: Set(upload_id),
        user_id: Set(user.id),
        team_id: Set(None),
        frontend_client_id: Set(None),
        filename: Set(filename.clone()),
        total_size: Set(1),
        chunk_size: Set(1),
        total_chunks: Set(1),
        received_count: Set(0),
        folder_id: Set(None),
        policy_id: Set(policy.id),
        status: Set(aster_drive::types::UploadSessionStatus::Uploading),
        session_kind: Set(aster_drive::types::UploadSessionKind::OffsetStaging),
        object_temp_key: Set(None),
        object_multipart_id: Set(None),
        provider_session_ciphertext: Set(None),
        file_id: Set(None),
        created_at: Set(now),
        expires_at: Set(now + chrono::Duration::hours(1)),
        updated_at: Set(now),
    };

    assert!(
        upload_session_repo::try_create(state.writer_db(), build_model(new_test_upload_id()))
            .await
            .unwrap()
    );
    let err = upload_session_repo::try_create(state.writer_db(), build_model(new_test_upload_id()))
        .await
        .expect_err("non-id unique conflict should not be treated as id retry");
    assert_eq!(err.code(), "E002");
}

async fn create_dead_remote_policy(
    state: &aster_drive::runtime::PrimaryAppState,
) -> aster_drive::entities::storage_policy::Model {
    use aster_drive::db::repository::managed_follower_repo;
    use aster_drive::entities::{managed_follower, storage_policy};
    use aster_drive::types::{
        DriverType, StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions,
    };
    use sea_orm::Set;

    let now = chrono::Utc::now();
    let remote_node = managed_follower_repo::create(
        state.writer_db(),
        managed_follower::ActiveModel {
            name: Set(format!("dead-remote-{}", uuid::Uuid::new_v4())),
            base_url: Set("http://127.0.0.1:9".to_string()),
            access_key: Set("dead-remote-ak".to_string()),
            secret_key: Set("dead-remote-sk".to_string()),
            is_enabled: Set(true),
            last_capabilities: Set(serde_json::to_string(
                &aster_drive::storage::remote_protocol::RemoteStorageCapabilities::current(),
            )
            .expect("current remote capabilities should serialize")),
            last_error: Set(String::new()),
            last_checked_at: Set(Some(now)),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let policy = policy_repo::create(
        state.writer_db(),
        storage_policy::ActiveModel {
            name: Set(format!("dead-remote-policy-{}", uuid::Uuid::new_v4())),
            driver_type: Set(DriverType::Remote),
            endpoint: Set(String::new()),
            bucket: Set(String::new()),
            access_key: Set(String::new()),
            secret_key: Set(String::new()),
            base_path: Set("dead-remote".to_string()),
            remote_node_id: Set(Some(remote_node.id)),
            max_file_size: Set(0),
            allowed_types: Set(StoredStoragePolicyAllowedTypes::empty()),
            options: Set(StoredStoragePolicyOptions::empty()),
            is_default: Set(false),
            chunk_size: Set(5),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    state
        .policy_snapshot
        .reload(state.writer_db())
        .await
        .expect("policy snapshot should reload after creating dead remote policy");
    state
        .driver_registry
        .reload_managed_followers(state.writer_db())
        .await
        .expect("driver registry should reload managed followers after creating dead remote node");
    state.driver_registry.invalidate(policy.id);

    policy
}

fn s3_test_client(endpoint: &str) -> aws_sdk_s3::Client {
    let credentials =
        aws_credential_types::Credentials::new("rustfsadmin", "rustfsadmin123", None, None, "test");
    let config = aws_sdk_s3::Config::builder()
        .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
        .region(aws_sdk_s3::config::Region::new("us-east-1"))
        .credentials_provider(credentials)
        .endpoint_url(endpoint)
        .force_path_style(true)
        .build();
    aws_sdk_s3::Client::from_conf(config)
}

async fn try_create_s3_bucket(endpoint: &str, bucket: &str) -> std::result::Result<(), String> {
    use aws_sdk_s3::error::ProvideErrorMetadata;

    let client = s3_test_client(endpoint);
    if let Err(err) = client.create_bucket().bucket(bucket).send().await {
        let code = err
            .as_service_error()
            .and_then(|service_err| service_err.code());
        if matches!(
            code,
            Some("BucketAlreadyOwnedByYou") | Some("BucketAlreadyExists")
        ) {
            return Ok(());
        }
        return Err(err.to_string());
    }
    Ok(())
}

async fn wait_for_s3_bucket(endpoint: &str, bucket: &str) {
    let mut last_err: Option<String> = None;
    let ready = tokio::time::timeout(std::time::Duration::from_secs(45), async {
        loop {
            match tokio::time::timeout(
                std::time::Duration::from_secs(3),
                try_create_s3_bucket(endpoint, bucket),
            )
            .await
            {
                Ok(Ok(())) => break,
                Ok(Err(err)) => last_err = Some(err),
                Err(_) => {
                    last_err = Some("create_bucket attempt timed out".to_string());
                }
            }
            // 这里只是 readiness probe 的退避间隔；真正的同步条件是上面的 create_bucket 成功。
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    })
    .await;

    if ready.is_err() {
        panic!(
            "timed out waiting for S3 bucket {bucket} at {endpoint}: {}",
            last_err.unwrap_or_else(|| "unknown error".to_string())
        );
    }
}

async fn s3_object_exists(client: &aws_sdk_s3::Client, bucket: &str, key: &str) -> bool {
    match client.head_object().bucket(bucket).key(key).send().await {
        Ok(_) => true,
        Err(error)
            if error
                .as_service_error()
                .map(|service_error| service_error.is_not_found())
                == Some(true) =>
        {
            false
        }
        Err(error) => panic!("S3 head_object for {key} failed unexpectedly: {error}"),
    }
}

async fn wait_for_s3_multipart_abort(
    client: &aws_sdk_s3::Client,
    bucket: &str,
    key: &str,
    upload_id: &str,
) {
    use aws_sdk_s3::error::ProvideErrorMetadata;

    let ready = tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            match client
                .list_parts()
                .bucket(bucket)
                .key(key)
                .upload_id(upload_id)
                .send()
                .await
            {
                Ok(response) => {
                    tracing::debug!(
                        upload_id,
                        bucket,
                        key,
                        part_count = response.parts().len(),
                        "multipart upload still exists while waiting for abort"
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
                Err(error) => {
                    let code = error.code();
                    let http_status = error
                        .raw_response()
                        .map(|response| response.status().as_u16());
                    if matches!(code, Some("NoSuchUpload" | "NotFound")) || http_status == Some(404)
                    {
                        break;
                    }

                    tracing::debug!(
                        upload_id,
                        bucket,
                        key,
                        error = %error,
                        "multipart abort is not visible yet"
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        }
    })
    .await;

    if ready.is_err() {
        panic!("timed out waiting for aborted multipart upload {upload_id} at {bucket}/{key}");
    }
}

fn snapshot_dir_tree(
    path: &std::path::Path,
) -> std::io::Result<std::collections::BTreeSet<String>> {
    fn walk(
        root: &std::path::Path,
        current: &std::path::Path,
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
        snapshots.insert(
            root.clone(),
            snapshot_dir_tree(std::path::Path::new(&root))?,
        );
    }
    Ok(snapshots)
}

async fn create_s3_policy(
    state: &aster_drive::runtime::PrimaryAppState,
    name: &str,
    endpoint: &str,
    bucket: &str,
    options: &str,
    chunk_size: i64,
) -> aster_drive::entities::storage_policy::Model {
    use chrono::Utc;
    use sea_orm::Set;

    let now = Utc::now();
    let policy = aster_drive::db::repository::policy_repo::create(
        state.writer_db(),
        aster_drive::entities::storage_policy::ActiveModel {
            name: Set(name.to_string()),
            driver_type: Set(aster_drive::types::DriverType::S3),
            endpoint: Set(endpoint.to_string()),
            bucket: Set(bucket.to_string()),
            access_key: Set("rustfsadmin".to_string()),
            secret_key: Set("rustfsadmin123".to_string()),
            base_path: Set("uploads".to_string()),
            max_file_size: Set(0),
            allowed_types: Set(aster_drive::types::StoredStoragePolicyAllowedTypes::empty()),
            options: Set(aster_drive::types::StoredStoragePolicyOptions::from(
                options.to_string(),
            )),
            is_default: Set(false),
            chunk_size: Set(chunk_size),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    state
        .policy_snapshot
        .reload(state.writer_db())
        .await
        .unwrap();
    state.driver_registry.invalidate(policy.id);
    policy
}

async fn create_s3_default_policy(
    state: &aster_drive::runtime::PrimaryAppState,
    user_id: i64,
    name: &str,
    endpoint: &str,
    bucket: &str,
    options: &str,
    chunk_size: i64,
) -> aster_drive::entities::storage_policy::Model {
    let policy = create_s3_policy(state, name, endpoint, bucket, options, chunk_size).await;

    let group = aster_drive::services::storage_policy::policy::create_group(
        state,
        aster_drive::services::storage_policy::policy::CreateStoragePolicyGroupInput {
            name: format!("S3 Test Group · {}", policy.id),
            description: Some(format!("Single-policy S3 group for policy #{}", policy.id)),
            is_enabled: true,
            is_default: false,
            items: vec![
                aster_drive::services::storage_policy::policy::StoragePolicyGroupItemInput {
                    policy_id: policy.id,
                    priority: 1,
                    min_file_size: 0,
                    max_file_size: 0,
                },
            ],
        },
    )
    .await
    .unwrap();

    aster_drive::services::user::account::update(
        state,
        aster_drive::services::user::account::UpdateUserInput {
            id: user_id,
            email_verified: None,
            role: None,
            status: None,
            must_change_password: None,
            storage_quota: None,
            policy_group_id: Some(group.id),
        },
    )
    .await
    .unwrap();

    policy
}

fn build_multipart_payload(filename: &str, data: &[u8]) -> (String, Vec<u8>) {
    let boundary = format!("----AsterTestBoundary{}", uuid::Uuid::new_v4().simple());
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

fn build_malformed_multipart_headers_over_parser_buffer() -> (String, Vec<u8>) {
    let boundary = format!(
        "----AsterMalformedBoundary{}",
        uuid::Uuid::new_v4().simple()
    );
    let mut payload = Vec::new();
    payload.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    payload.extend_from_slice(b"X-Long: ");
    payload.extend_from_slice(&vec![b'a'; 70 * 1024]);
    (boundary, payload)
}

async fn store_temp_file_in_personal_space(
    state: &aster_drive::runtime::PrimaryAppState,
    user_id: i64,
    filename: &str,
    data: &[u8],
) -> i64 {
    let temp_path = aster_forge_utils::paths::temp_file_path(
        &state.config.server.temp_dir,
        &format!("download-test-{}", uuid::Uuid::new_v4()),
    );
    tokio::fs::create_dir_all(&state.config.server.temp_dir)
        .await
        .unwrap();
    tokio::fs::write(&temp_path, data).await.unwrap();
    let file = aster_drive::services::files::file::store_from_temp(
        state,
        user_id,
        aster_drive::services::files::file::StoreFromTempRequest::new(
            None,
            filename,
            &temp_path,
            data.len() as i64,
        ),
    )
    .await
    .unwrap();
    let _ = tokio::fs::remove_file(&temp_path).await;
    file.id
}

async fn prepare_chunked_upload_for_personal_scope(
    state: &aster_drive::runtime::PrimaryAppState,
    user_id: i64,
    filename: &str,
    fill: u8,
) -> String {
    use aster_drive::services::files::upload;

    let total_size = i64::try_from(TEST_CHUNK_SIZE * 2).unwrap();
    let init = upload::init_upload(state, user_id, filename, total_size, None, None)
        .await
        .unwrap();
    assert_eq!(init.mode, aster_drive::types::UploadMode::Chunked);
    let upload_id = init.upload_id.unwrap();
    let chunk = vec![fill; TEST_CHUNK_SIZE];
    for chunk_number in 0..2 {
        upload::upload_chunk(state, &upload_id, chunk_number, user_id, &chunk)
            .await
            .unwrap();
    }
    upload_id
}

async fn prepare_chunked_upload_for_team_scope(
    state: &aster_drive::runtime::PrimaryAppState,
    team_id: i64,
    user_id: i64,
    filename: &str,
    fill: u8,
) -> String {
    use aster_drive::services::files::upload;

    let total_size = i64::try_from(TEST_CHUNK_SIZE * 2).unwrap();
    let init =
        upload::init_upload_for_team(state, team_id, user_id, filename, total_size, None, None)
            .await
            .unwrap();
    assert_eq!(init.mode, aster_drive::types::UploadMode::Chunked);
    let upload_id = init.upload_id.unwrap();
    let chunk = vec![fill; TEST_CHUNK_SIZE];
    for chunk_number in 0..2 {
        upload::upload_chunk_for_team(state, team_id, &upload_id, chunk_number, user_id, &chunk)
            .await
            .unwrap();
    }
    upload_id
}

#[tokio::test]
async fn test_concurrent_store_from_temp_same_name_auto_renames() {
    use aster_drive::db::repository::file_repo;
    use aster_drive::services::files::file;
    use std::sync::Arc;

    let state = Arc::new(common::setup().await);
    let user = common::create_test_account(
        state.as_ref(),
        "raceuser",
        "concurrent-store@test.com",
        "password123",
    )
    .await
    .unwrap();

    let temp_path_1 = aster_forge_utils::paths::temp_file_path(
        &state.config.server.temp_dir,
        &format!("concurrent-store-{}", uuid::Uuid::new_v4()),
    );
    let temp_path_2 = aster_forge_utils::paths::temp_file_path(
        &state.config.server.temp_dir,
        &format!("concurrent-store-{}", uuid::Uuid::new_v4()),
    );
    tokio::fs::create_dir_all(&state.config.server.temp_dir)
        .await
        .unwrap();
    tokio::fs::write(&temp_path_1, b"first concurrent upload")
        .await
        .unwrap();
    tokio::fs::write(&temp_path_2, b"second concurrent upload")
        .await
        .unwrap();

    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let state_1 = Arc::clone(&state);
    let state_2 = Arc::clone(&state);
    let barrier_1 = Arc::clone(&barrier);
    let barrier_2 = Arc::clone(&barrier);
    let name_1 = "race.txt".to_string();
    let name_2 = name_1.clone();
    let path_1 = temp_path_1.clone();
    let path_2 = temp_path_2.clone();

    let (first, second) = tokio::join!(
        async move {
            barrier_1.wait().await;
            file::store_from_temp(
                &state_1,
                user.id,
                file::StoreFromTempRequest::new(
                    None,
                    &name_1,
                    &path_1,
                    i64::try_from(b"first concurrent upload".len()).unwrap(),
                ),
            )
            .await
        },
        async move {
            barrier_2.wait().await;
            file::store_from_temp(
                &state_2,
                user.id,
                file::StoreFromTempRequest::new(
                    None,
                    &name_2,
                    &path_2,
                    i64::try_from(b"second concurrent upload".len()).unwrap(),
                ),
            )
            .await
        }
    );

    let first = first.unwrap();
    let second = second.unwrap();
    let first = file_repo::find_by_id(state.writer_db(), first.id)
        .await
        .unwrap();
    let second = file_repo::find_by_id(state.writer_db(), second.id)
        .await
        .unwrap();

    let mut names = vec![first.name, second.name];
    names.sort();
    assert_eq!(
        names,
        vec!["race (1).txt".to_string(), "race.txt".to_string()],
        "concurrent same-name uploads should succeed and auto-rename one side",
    );

    let _ = tokio::fs::remove_file(&temp_path_1).await;
    let _ = tokio::fs::remove_file(&temp_path_2).await;
}

#[tokio::test]
async fn test_concurrent_store_from_temp_same_owner_serializes_quota_row() {
    use aster_drive::db::repository::{file_repo, user_repo};
    use aster_drive::services::files::file;
    use std::sync::Arc;

    let state = Arc::new(common::setup().await);
    let user = common::create_test_account(
        state.as_ref(),
        "quota-lock-user",
        "quota-lock-user@test.com",
        "password123",
    )
    .await
    .unwrap();
    let payloads = [b"alpha payload".as_slice(), b"beta payload".as_slice()];
    let mut paths = Vec::new();
    for payload in payloads {
        let path = aster_forge_utils::paths::temp_file_path(
            &state.config.server.temp_dir,
            &format!("quota-lock-{}", uuid::Uuid::new_v4()),
        );
        tokio::fs::create_dir_all(&state.config.server.temp_dir)
            .await
            .unwrap();
        tokio::fs::write(&path, payload).await.unwrap();
        paths.push(path);
    }

    let quota_guard = hold_personal_quota_row(state.as_ref(), user.id).await;
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let mut tasks = JoinSet::new();
    for (index, path) in paths.iter().cloned().enumerate() {
        let state = Arc::clone(&state);
        let barrier = Arc::clone(&barrier);
        let filename = format!("quota-lock-{index}.txt");
        let size = i64::try_from(payloads[index].len()).unwrap();
        tasks.spawn(async move {
            barrier.wait().await;
            file::store_from_temp(
                &state,
                user.id,
                file::StoreFromTempRequest::new(None, &filename, &path, size),
            )
            .await
        });
    }

    if let Some(quota_guard) = quota_guard {
        assert_tasks_wait_for_quota_guard(&mut tasks).await;
        aster_forge_db::transaction::commit(quota_guard)
            .await
            .unwrap();
    }

    let mut file_ids = Vec::new();
    while let Some(result) = tasks.join_next().await {
        file_ids.push(result.unwrap().unwrap().id);
    }
    assert_eq!(file_ids.len(), 2);
    for file_id in file_ids {
        file_repo::find_by_id(state.writer_db(), file_id)
            .await
            .expect("both concurrent file transactions should commit");
    }
    let owner = user_repo::find_by_id(state.writer_db(), user.id)
        .await
        .unwrap();
    assert_eq!(
        owner.storage_used,
        i64::try_from(payloads.iter().map(|payload| payload.len()).sum::<usize>()).unwrap()
    );

    for path in paths {
        let _ = tokio::fs::remove_file(path).await;
    }
}

#[actix_web::test]
async fn test_chunked_upload_flow() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 1. 初始化分片上传（10KB 文件，chunk_size=5MB → 直传模式）
    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload/init")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "filename": "chunked.txt",
            "total_size": 10240
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    // 小文件可能返回 direct 模式
    let mode = body["data"]["mode"].as_str().unwrap();
    assert!(
        mode == "direct" || mode == "chunked",
        "mode should be direct or chunked, got {mode}"
    );

    if mode == "chunked" {
        let upload_id = body["data"]["upload_id"].as_str().unwrap().to_string();
        let total_chunks = body["data"]["total_chunks"].as_i64().unwrap();

        // 2. 上传分片
        for i in 0..total_chunks {
            let chunk_data = vec![b'A'; 5120]; // 5KB per chunk
            let req = test::TestRequest::put()
                .uri(&format!("/api/v1/files/upload/{upload_id}/{i}"))
                .insert_header(("Cookie", common::access_cookie_header(&token)))
                .insert_header(common::csrf_header_for(&token))
                .insert_header(("Content-Type", "application/octet-stream"))
                .set_payload(chunk_data)
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 200, "chunk {i} upload failed");
        }

        // 3. 查看进度
        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/files/upload/{upload_id}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        // 4. 完成上传
        let req = test::TestRequest::post()
            .uri(&format!("/api/v1/files/upload/{upload_id}/complete"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body["data"]["name"], "chunked.txt");
    }
}

#[actix_web::test]
async fn test_chunk_upload_endpoint_streams_and_rejects_oversized_chunk_with_413() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload/init")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "filename": "oversized-chunk.bin",
            "total_size": TEST_CHUNK_SIZE + 1
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["mode"], "chunked");
    let upload_id = body["data"]["upload_id"].as_str().unwrap().to_string();

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/upload/{upload_id}/0"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(vec![b'x'; TEST_CHUNK_SIZE + 1])
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::PAYLOAD_TOO_LARGE
    );
    let body: Value = test::read_body_json(resp).await;
    assert_upload_error_contract(&body, "upload.chunk_too_large");
}

#[actix_web::test]
async fn test_chunk_upload_endpoint_keeps_duplicate_size_validation() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload/init")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "filename": "duplicate-size.bin",
            "total_size": TEST_CHUNK_SIZE + 1
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let upload_id = body["data"]["upload_id"].as_str().unwrap().to_string();

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/upload/{upload_id}/0"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(vec![b'a'; TEST_CHUNK_SIZE])
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/upload/{upload_id}/0"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(b"short".to_vec())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_client_error());
    let body: Value = test::read_body_json(resp).await;
    assert_upload_error_contract(&body, "upload.chunk_size_mismatch");
}

#[actix_web::test]
async fn test_recoverable_upload_sessions_endpoint_lists_active_sessions() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let frontend_client_id = "11111111-1111-4111-8111-111111111111";

    let total_size = TEST_CHUNK_SIZE + 1;
    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload/init")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "filename": "recoverable.bin",
            "total_size": total_size,
            "frontend_client_id": frontend_client_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["mode"], "chunked");
    let upload_id = body["data"]["upload_id"].as_str().unwrap().to_string();

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/files/upload/sessions?frontend_client_id={frontend_client_id}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let sessions = body["data"].as_array().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["upload_id"], upload_id);
    assert_eq!(sessions[0]["mode"], "chunked");
    assert_eq!(sessions[0]["status"], "uploading");
    assert_eq!(sessions[0]["filename"], "recoverable.bin");
    assert_eq!(
        sessions[0]["total_size"].as_i64().unwrap(),
        total_size as i64
    );
    assert_eq!(sessions[0]["folder_id"], Value::Null);
    assert_eq!(sessions[0]["chunks_on_disk"].as_array().unwrap().len(), 0);

    let req = test::TestRequest::get()
        .uri(
            "/api/v1/files/upload/sessions?frontend_client_id=22222222-2222-4222-8222-222222222222",
        )
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"].as_array().unwrap().len(),
        0,
        "sessions from a different frontend client should be hidden"
    );
}

#[actix_web::test]
async fn test_recoverable_upload_sessions_endpoint_filters_by_frontend_client() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let client_a = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let client_b = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";

    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload/init")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "filename": "client-a.bin",
            "total_size": TEST_CHUNK_SIZE + 1,
            "frontend_client_id": client_a
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let upload_a = body["data"]["upload_id"].as_str().unwrap().to_string();

    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload/init")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "filename": "client-b.bin",
            "total_size": TEST_CHUNK_SIZE + 1,
            "frontend_client_id": client_b
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let upload_b = body["data"]["upload_id"].as_str().unwrap().to_string();

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/files/upload/sessions?frontend_client_id={client_a}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let sessions = body["data"].as_array().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["upload_id"], upload_a);

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/files/upload/sessions?frontend_client_id={client_b}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let sessions = body["data"].as_array().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["upload_id"], upload_b);

    let req = test::TestRequest::get()
        .uri("/api/v1/files/upload/sessions")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"].as_array().unwrap().len(),
        2,
        "legacy callers without a frontend filter keep the old owner-wide behavior"
    );
}

#[actix_web::test]
async fn test_init_upload_validates_filename_and_total_size() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload/init")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "filename": "",
            "total_size": 1024
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload/init")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "filename": "valid.bin",
            "total_size": 1024,
            "frontend_client_id": "11111111111141118111111111111111"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload/init")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "filename": "valid.bin",
            "total_size": 0
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["mode"], "direct");
    assert!(body["data"]["upload_id"].is_null());

    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload/init")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "filename": "negative.bin",
            "total_size": -1
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["msg"], "total_size cannot be negative");
}

#[actix_web::test]
async fn test_malformed_multipart_headers_returns_upload_error_contract() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let (boundary, payload) = build_malformed_multipart_headers_over_parser_buffer();
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
    let status = resp.status();
    let body_bytes = test::read_body(resp).await;
    assert_eq!(
        status,
        actix_web::http::StatusCode::BAD_REQUEST,
        "unexpected status body: {}",
        String::from_utf8_lossy(&body_bytes)
    );
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_upload_error_contract(&body, ApiErrorCode::UploadFieldReadFailed.as_str());
}

#[actix_web::test]
async fn test_empty_file_upload_flow_uses_direct_and_creates_file() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload/init")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "filename": "empty-upload.txt",
            "total_size": 0
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["mode"], "direct");
    assert!(body["data"]["upload_id"].is_null());

    let (boundary, payload) = build_multipart_payload("empty-upload.txt", b"");
    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload?declared_size=0")
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
    assert_eq!(body["data"]["name"], "empty-upload.txt");
    assert_eq!(body["data"]["size"], 0);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/download"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let bytes = test::read_body(resp).await;
    assert!(bytes.is_empty());
}

#[actix_web::test]
async fn test_file_upload_init_upload_normalizes_nfd_filename_and_rejects_windows_reserved_name() {
    use aster_drive::db::repository::upload_session_repo;
    use aster_drive::services::files::upload;

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "unicodeupload",
        "unicodeupload@test.com",
        "password123",
    )
    .await
    .unwrap();

    let init = upload::init_upload(&state, user.id, "cafe\u{0301}.txt", 10_485_760, None, None)
        .await
        .unwrap();
    assert_eq!(init.mode, aster_drive::types::UploadMode::Chunked);
    let upload_id = init
        .upload_id
        .expect("chunked upload should return upload_id");
    let session = upload_session_repo::find_by_id(state.writer_db(), &upload_id)
        .await
        .unwrap();
    assert_eq!(session.filename, "caf\u{00e9}.txt");

    let err = match upload::init_upload(&state, user.id, "COM1.txt", 10_485_760, None, None).await {
        Ok(_) => panic!("COM1.txt should be rejected"),
        Err(err) => err,
    };
    assert_eq!(err.code(), "E005");
}

#[actix_web::test]
async fn test_update_storage_used_is_atomic_under_concurrency() {
    use aster_drive::db::repository::user_repo;

    let state = common::setup().await;
    let user = common::create_test_account(&state, "quotauser", "quota@test.com", "password123")
        .await
        .unwrap();

    let mut tasks = JoinSet::new();
    for _ in 0..32 {
        let db = state.writer_db().clone();
        let user_id = user.id;
        tasks.spawn(async move { user_repo::update_storage_used(&db, user_id, 1).await });
    }

    while let Some(result) = tasks.join_next().await {
        result.unwrap().unwrap();
    }

    let updated = user_repo::find_by_id(state.writer_db(), user.id)
        .await
        .unwrap();
    assert_eq!(updated.storage_used, 32);

    let mut tasks = JoinSet::new();
    for _ in 0..40 {
        let db = state.writer_db().clone();
        let user_id = user.id;
        tasks.spawn(async move { user_repo::update_storage_used(&db, user_id, -1).await });
    }

    while let Some(result) = tasks.join_next().await {
        result.unwrap().unwrap();
    }

    let updated = user_repo::find_by_id(state.writer_db(), user.id)
        .await
        .unwrap();
    assert_eq!(
        updated.storage_used, 0,
        "storage_used should not go below zero"
    );
}

/// 验证 update_storage_used 在并发场景下，超过 quota 的请求会被 SQL CAS 拒绝。
///
/// 历史漏洞：check_quota 是 SELECT-then-compare，并发请求可同时通过预检后超额提交。
/// 修复后：update_storage_used 在 SQL WHERE 子句中加 `storage_used + delta <= storage_quota`，
/// 真正的 race winner 才能成功，loser 收到 storage_quota_exceeded。
#[actix_web::test]
async fn test_concurrent_quota_overrun_is_rejected_by_cas() {
    use aster_drive::db::repository::user_repo;
    use aster_drive::entities::user;
    use sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, Set};

    let state = common::setup().await;
    let registered =
        common::create_test_account(&state, "quotaov", "quotaov@test.com", "password123")
            .await
            .unwrap();

    // 设 quota = 100 字节，并发提交 20 个 +10 字节请求（总需求 200，超额一倍）
    let model = user::Entity::find_by_id(registered.id)
        .one(state.writer_db())
        .await
        .unwrap()
        .unwrap();
    let mut active = model.into_active_model();
    active.storage_quota = Set(100);
    active.update(state.writer_db()).await.unwrap();

    let mut tasks = JoinSet::new();
    for _ in 0..20 {
        let db = state.writer_db().clone();
        let user_id = registered.id;
        tasks.spawn(async move { user_repo::update_storage_used(&db, user_id, 10).await });
    }

    let mut succeeded = 0;
    let mut quota_exceeded = 0;
    let mut other_errors = 0;
    while let Some(result) = tasks.join_next().await {
        match result.unwrap() {
            Ok(()) => succeeded += 1,
            Err(e) if e.code() == "E032" => quota_exceeded += 1, // StorageQuotaExceeded
            Err(_) => other_errors += 1,
        }
    }

    assert_eq!(other_errors, 0, "should only see Ok or quota_exceeded");
    assert_eq!(succeeded, 10, "exactly 10 requests should fit in quota=100");
    assert_eq!(quota_exceeded, 10, "remaining 10 must be rejected");

    let final_state = user_repo::find_by_id(state.writer_db(), registered.id)
        .await
        .unwrap();
    assert_eq!(final_state.storage_used, 100, "must not exceed quota");
}

/// 验证 check_quota 对 i64 加法溢出有防护（之前会 wrap 成负数反而通过校验）
#[actix_web::test]
async fn test_check_quota_rejects_integer_overflow() {
    use aster_drive::db::repository::user_repo;
    use aster_drive::entities::user;
    use sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, Set};

    let state = common::setup().await;
    let registered = common::create_test_account(&state, "ovflu", "ovflu@test.com", "password123")
        .await
        .unwrap();

    // 把 storage_used 调到接近 i64::MAX，再传一个会触发溢出的 delta
    let model = user::Entity::find_by_id(registered.id)
        .one(state.writer_db())
        .await
        .unwrap()
        .unwrap();
    let mut active = model.into_active_model();
    active.storage_used = Set(i64::MAX - 100);
    active.storage_quota = Set(0); // 不限，证明检查的是溢出本身而非配额
    active.update(state.writer_db()).await.unwrap();

    // 不限配额下，i64 加法溢出本来会 wrap 成负数通过 check，现在必须明确拒绝
    let result = user_repo::check_quota(state.writer_db(), registered.id, 200).await;
    let err = result.expect_err("overflow must be rejected");
    assert_eq!(err.code(), "E004", "expected internal_error for overflow");
}

#[actix_web::test]
async fn test_s3_relay_stream_download_e2e() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let me: Value = test::read_body_json(resp).await;
    let user_id = me["data"]["id"].as_i64().expect("user id should exist");

    let container = GenericImage::new("rustfs/rustfs", RUSTFS_TEST_IMAGE_TAG)
        .with_exposed_port(testcontainers::core::IntoContainerPort::tcp(9000))
        .with_env_var("RUSTFS_ACCESS_KEY", "rustfsadmin")
        .with_env_var("RUSTFS_SECRET_KEY", "rustfsadmin123")
        .start()
        .await
        .expect("failed to start rustfs container");

    let port = container.get_host_port_ipv4(9000).await.unwrap();
    let endpoint = format!("http://127.0.0.1:{port}");
    let bucket = "download-relay";
    wait_for_s3_bucket(&endpoint, bucket).await;

    create_s3_default_policy(
        &state,
        user_id,
        "S3 Relay Download",
        &endpoint,
        bucket,
        r#"{"object_storage_download_strategy":"relay_stream"}"#,
        TEST_CHUNK_SIZE as i64,
    )
    .await;

    let file_id = store_temp_file_in_personal_space(
        &state,
        user_id,
        "relay report.txt",
        b"hello relay download",
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/download"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("Content-Disposition")
            .unwrap()
            .to_str()
            .unwrap(),
        "attachment; filename*=UTF-8''relay%20report.txt"
    );
    let body = test::read_body(resp).await;
    assert_eq!(body.as_ref(), b"hello relay download");
}

#[actix_web::test]
async fn test_s3_presigned_download_redirects_and_share_counts() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let me: Value = test::read_body_json(resp).await;
    let user_id = me["data"]["id"].as_i64().expect("user id should exist");

    let container = GenericImage::new("rustfs/rustfs", RUSTFS_TEST_IMAGE_TAG)
        .with_exposed_port(testcontainers::core::IntoContainerPort::tcp(9000))
        .with_env_var("RUSTFS_ACCESS_KEY", "rustfsadmin")
        .with_env_var("RUSTFS_SECRET_KEY", "rustfsadmin123")
        .start()
        .await
        .expect("failed to start rustfs container");

    let port = container.get_host_port_ipv4(9000).await.unwrap();
    let endpoint = format!("http://127.0.0.1:{port}");
    let bucket = "download-presigned";
    wait_for_s3_bucket(&endpoint, bucket).await;

    create_s3_default_policy(
        &state,
        user_id,
        "S3 Presigned Download",
        &endpoint,
        bucket,
        r#"{"object_storage_download_strategy":"presigned"}"#,
        TEST_CHUNK_SIZE as i64,
    )
    .await;

    let file_name = "presigned report.txt";
    let file_data = b"hello presigned download";
    let file_id = store_temp_file_in_personal_space(&state, user_id, file_name, file_data).await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/download"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers().get("Cache-Control").unwrap(),
        "no-store",
        "presigned redirect should not be cached"
    );
    let location = resp
        .headers()
        .get("Location")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        location.contains("response-content-disposition="),
        "presigned URL should preserve attachment filename"
    );

    let response = reqwest::get(&location).await.unwrap();
    assert!(response.status().is_success());
    assert_eq!(
        response
            .headers()
            .get(reqwest::header::CONTENT_DISPOSITION)
            .unwrap()
            .to_str()
            .unwrap(),
        "attachment; filename*=UTF-8''presigned%20report.txt"
    );
    assert_eq!(response.bytes().await.unwrap().as_ref(), file_data);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/direct-link"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let direct_token = body["data"]["token"]
        .as_str()
        .expect("direct link token should exist")
        .to_string();

    let req = test::TestRequest::get()
        .uri(&format!("/d/{direct_token}/presigned%20report.txt"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers().get("Cache-Control").unwrap(),
        "no-store",
        "direct-link inline presigned redirect should not be cached"
    );
    let direct_inline_location = resp
        .headers()
        .get("Location")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        direct_inline_location.contains("response-content-disposition="),
        "direct-link inline presigned URL should preserve inline filename"
    );
    let direct_inline_response = reqwest::get(&direct_inline_location).await.unwrap();
    assert!(direct_inline_response.status().is_success());
    assert_eq!(
        direct_inline_response
            .headers()
            .get(reqwest::header::CONTENT_DISPOSITION)
            .unwrap()
            .to_str()
            .unwrap(),
        "inline; filename*=UTF-8''presigned%20report.txt"
    );
    assert_eq!(
        direct_inline_response.bytes().await.unwrap().as_ref(),
        file_data
    );

    let req = test::TestRequest::get()
        .uri(&format!(
            "/d/{direct_token}/presigned%20report.txt?download=1"
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers().get("Cache-Control").unwrap(),
        "no-store",
        "direct-link presigned redirect should not be cached"
    );
    let direct_location = resp
        .headers()
        .get("Location")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        direct_location.contains("response-content-disposition="),
        "direct-link presigned URL should preserve attachment filename"
    );

    let direct_response = reqwest::get(&direct_location).await.unwrap();
    assert!(direct_response.status().is_success());
    assert_eq!(
        direct_response
            .headers()
            .get(reqwest::header::CONTENT_DISPOSITION)
            .unwrap()
            .to_str()
            .unwrap(),
        "attachment; filename*=UTF-8''presigned%20report.txt"
    );
    assert_eq!(direct_response.bytes().await.unwrap().as_ref(), file_data);

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": { "type": "file", "id": file_id }
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"]
        .as_str()
        .expect("share token should exist")
        .to_string();

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/download"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 302);
    let shared_location = resp
        .headers()
        .get("Location")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        shared_location.contains("response-content-disposition="),
        "shared presigned URL should preserve attachment filename"
    );
    let shared_response = reqwest::get(&shared_location).await.unwrap();
    assert!(shared_response.status().is_success());
    assert_eq!(shared_response.bytes().await.unwrap().as_ref(), file_data);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["download_count"], 1);
}

#[actix_web::test]
async fn test_chunked_upload_offset_staging_preserves_content() {
    use aster_drive::db::repository::{file_repo, upload_session_part_repo};
    use aster_drive::services::files::upload;

    let state = common::setup().await;
    let user = common::create_test_account(&state, "streamuser", "stream@test.com", "password123")
        .await
        .unwrap();

    let init = upload::init_upload(&state, user.id, "streamed.txt", 10_485_760, None, None)
        .await
        .unwrap();
    assert_eq!(init.mode, aster_drive::types::UploadMode::Chunked);

    let upload_id = init.upload_id.unwrap();
    let chunk0 = vec![b'A'; TEST_CHUNK_SIZE];
    let chunk1 = vec![b'B'; TEST_CHUNK_SIZE];

    let resp0 = upload::upload_chunk(&state, &upload_id, 0, user.id, &chunk0)
        .await
        .unwrap();
    assert_eq!(resp0.received_count, 1);
    let resp1 = upload::upload_chunk(&state, &upload_id, 1, user.id, &chunk1)
        .await
        .unwrap();
    assert_eq!(resp1.received_count, 2);

    let staging_path = upload::test_support::offset_staging_file_path(
        &state.config.server.upload_temp_dir,
        &upload_id,
    );
    let staging_metadata = tokio::fs::metadata(&staging_path).await.unwrap();
    assert_eq!(staging_metadata.len(), 10_485_760);
    let staged = tokio::fs::read(&staging_path).await.unwrap();
    assert_eq!(staged, [chunk0.as_slice(), chunk1.as_slice()].concat());

    let receipts = upload_session_part_repo::list_all_by_upload(state.writer_db(), &upload_id)
        .await
        .unwrap();
    assert_eq!(receipts.len(), 2);
    assert!(receipts.iter().enumerate().all(|(index, receipt)| {
        receipt.part_number == index as i32 + 1
            && receipt.etag == upload::test_support::offset_staging_receipt_etag()
            && receipt.size == TEST_CHUNK_SIZE as i64
    }));
    for chunk_number in 0..2 {
        assert!(
            tokio::fs::metadata(aster_forge_utils::paths::upload_chunk_path(
                &state.config.server.upload_temp_dir,
                &upload_id,
                chunk_number,
            ))
            .await
            .is_err(),
            "offset staging should not create legacy chunk marker files"
        );
    }

    let temp_dir =
        aster_forge_utils::paths::upload_temp_dir(&state.config.server.upload_temp_dir, &upload_id);
    tokio::fs::write(format!("{temp_dir}/chunk_0"), b"legacy payload")
        .await
        .unwrap();
    tokio::fs::write(format!("{temp_dir}/assembled"), b"legacy assembled payload")
        .await
        .unwrap();

    let file = upload::complete_upload(&state, &upload_id, user.id, None)
        .await
        .unwrap();
    assert_eq!(file.name, "streamed.txt");

    let blob = file_repo::find_blob_by_id(state.writer_db(), file.blob_id)
        .await
        .unwrap();
    let policy =
        aster_drive::db::repository::policy_repo::find_by_id(state.writer_db(), blob.policy_id)
            .await
            .unwrap();
    let driver = state.driver_registry.get_driver(&policy).unwrap();
    let stored = driver.get(&blob.storage_path).await.unwrap();

    assert_eq!(stored, [chunk0.as_slice(), chunk1.as_slice()].concat());
    assert_eq!(blob.size, stored.len() as i64);
}

#[actix_web::test]
async fn test_concurrent_distinct_chunks_write_to_their_staging_offsets() {
    use aster_drive::services::files::upload;

    let state = std::sync::Arc::new(common::setup().await);
    let user = common::create_test_account(
        &state,
        "offsetconcurrent",
        "offset-concurrent@test.com",
        "password123",
    )
    .await
    .unwrap();
    let init = upload::init_upload(
        &state,
        user.id,
        "offset-concurrent.bin",
        10_485_760,
        None,
        None,
    )
    .await
    .unwrap();
    let upload_id = init.upload_id.unwrap();
    let first = vec![b'A'; TEST_CHUNK_SIZE];
    let second = vec![b'B'; TEST_CHUNK_SIZE];
    let _write_rendezvous = upload::test_support::install_distinct_chunk_rendezvous(&upload_id, 2);

    let (first_result, second_result) =
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            tokio::join!(
                upload::upload_chunk(&state, &upload_id, 0, user.id, &first),
                upload::upload_chunk(&state, &upload_id, 1, user.id, &second),
            )
        })
        .await
        .expect("distinct chunk writes must enter their critical sections concurrently");
    first_result.unwrap();
    second_result.unwrap();

    let staging_path = upload::test_support::offset_staging_file_path(
        &state.config.server.upload_temp_dir,
        &upload_id,
    );
    let staged = tokio::fs::read(staging_path).await.unwrap();
    assert_eq!(staged, [first.as_slice(), second.as_slice()].concat());

    let progress = upload::get_progress(&state, &upload_id, user.id)
        .await
        .unwrap();
    assert_eq!(progress.received_count, 2);
    assert_eq!(progress.chunks_on_disk, vec![0, 1]);
}

#[actix_web::test]
async fn test_concurrent_duplicate_staged_chunk_keeps_one_complete_payload() {
    use aster_drive::services::files::upload;

    let state = std::sync::Arc::new(common::setup().await);
    let user = common::create_test_account(
        &state,
        "offsetduplicate",
        "offset-duplicate@test.com",
        "password123",
    )
    .await
    .unwrap();
    let init = upload::init_upload(
        &state,
        user.id,
        "offset-duplicate.bin",
        10_485_760,
        None,
        None,
    )
    .await
    .unwrap();
    let upload_id = init.upload_id.unwrap();
    let first = vec![b'A'; TEST_CHUNK_SIZE];
    let second = vec![b'B'; TEST_CHUNK_SIZE];
    let exclusion = upload::test_support::install_same_chunk_exclusion_observer(&upload_id);

    let (first_result, second_result) =
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            tokio::join!(
                upload::upload_chunk(&state, &upload_id, 0, user.id, &first),
                upload::upload_chunk(&state, &upload_id, 0, user.id, &second),
            )
        })
        .await
        .expect("duplicate chunk uploads must finish while the exclusion observer is installed");
    first_result.unwrap();
    second_result.unwrap();
    assert_eq!(
        exclusion.entry_count(),
        2,
        "both duplicate requests must reach the staging write lock entry"
    );
    assert!(
        !exclusion.overlap_observed(),
        "duplicate chunk uploads entered the staging write critical section together"
    );

    let staging_path = upload::test_support::offset_staging_file_path(
        &state.config.server.upload_temp_dir,
        &upload_id,
    );
    let staged = tokio::fs::read(staging_path).await.unwrap();
    let first_range = &staged[..TEST_CHUNK_SIZE];
    assert!(
        first_range == first.as_slice() || first_range == second.as_slice(),
        "concurrent duplicate writes must preserve one whole winner payload"
    );
    assert_eq!(
        upload::get_progress(&state, &upload_id, user.id)
            .await
            .unwrap()
            .received_count,
        1
    );
}

#[actix_web::test]
async fn test_retry_overwrites_uncommitted_partial_staging_range() {
    use aster_drive::services::files::upload;
    use tokio::io::{AsyncSeekExt, AsyncWriteExt};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "offsetretry",
        "offset-retry@test.com",
        "password123",
    )
    .await
    .unwrap();
    let init = upload::init_upload(&state, user.id, "offset-retry.bin", 10_485_760, None, None)
        .await
        .unwrap();
    let upload_id = init.upload_id.unwrap();
    let staging_path = upload::test_support::offset_staging_file_path(
        &state.config.server.upload_temp_dir,
        &upload_id,
    );

    let mut staging = tokio::fs::OpenOptions::new()
        .write(true)
        .open(&staging_path)
        .await
        .unwrap();
    staging.seek(std::io::SeekFrom::Start(0)).await.unwrap();
    staging.write_all(&vec![b'X'; 1024 * 1024]).await.unwrap();
    staging.flush().await.unwrap();
    drop(staging);

    let committed = vec![b'A'; TEST_CHUNK_SIZE];
    upload::upload_chunk(&state, &upload_id, 0, user.id, &committed)
        .await
        .unwrap();

    let staged = tokio::fs::read(staging_path).await.unwrap();
    assert_eq!(&staged[..TEST_CHUNK_SIZE], committed.as_slice());
    assert_eq!(
        upload::get_progress(&state, &upload_id, user.id)
            .await
            .unwrap()
            .chunks_on_disk,
        vec![0]
    );
}

#[actix_web::test]
async fn test_retry_repairs_staged_chunk_receipt_without_double_counting() {
    use aster_drive::db::repository::{upload_session_part_repo, upload_session_repo};
    use aster_drive::entities::upload_session;
    use aster_drive::services::files::upload;
    use sea_orm::{ActiveModelTrait, Set};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "offsetreceipt",
        "offset-receipt@test.com",
        "password123",
    )
    .await
    .unwrap();
    let init = upload::init_upload(
        &state,
        user.id,
        "offset-receipt.bin",
        10_485_760,
        None,
        None,
    )
    .await
    .unwrap();
    let upload_id = init.upload_id.unwrap();
    let committed = vec![b'A'; TEST_CHUNK_SIZE];

    upload::upload_chunk(&state, &upload_id, 0, user.id, &committed)
        .await
        .unwrap();
    upload_session_part_repo::delete_by_upload_and_part(state.writer_db(), &upload_id, 1)
        .await
        .unwrap();
    let mut session: upload_session::ActiveModel =
        upload_session_repo::find_by_id(state.writer_db(), &upload_id)
            .await
            .unwrap()
            .into();
    session.received_count = Set(0);
    session.update(state.writer_db()).await.unwrap();

    let repaired = upload::upload_chunk(&state, &upload_id, 0, user.id, &committed)
        .await
        .unwrap();
    assert_eq!(repaired.received_count, 1);
    let duplicate = upload::upload_chunk(&state, &upload_id, 0, user.id, &committed)
        .await
        .unwrap();
    assert_eq!(duplicate.received_count, 1);
}

#[actix_web::test]
async fn test_retry_with_existing_receipt_keeps_committed_staging_range() {
    use aster_drive::services::files::upload;

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "receiptretry",
        "offset-receipt-retry@test.com",
        "password123",
    )
    .await
    .unwrap();
    let init = upload::init_upload(
        &state,
        user.id,
        "offset-receipt-retry.bin",
        10_485_760,
        None,
        None,
    )
    .await
    .unwrap();
    let upload_id = init.upload_id.unwrap();
    let first = vec![b'A'; TEST_CHUNK_SIZE];
    let replacement = vec![b'B'; TEST_CHUNK_SIZE];

    upload::upload_chunk(&state, &upload_id, 0, user.id, &first)
        .await
        .unwrap();
    let retried = upload::upload_chunk(&state, &upload_id, 0, user.id, &replacement)
        .await
        .unwrap();
    assert_eq!(retried.received_count, 1);

    let staging_path = upload::test_support::offset_staging_file_path(
        &state.config.server.upload_temp_dir,
        &upload_id,
    );
    let staged = tokio::fs::read(staging_path).await.unwrap();
    assert_eq!(&staged[..TEST_CHUNK_SIZE], first.as_slice());
}

#[actix_web::test]
async fn test_offset_staging_rejects_corrupted_receipt_on_retry_and_complete() {
    use aster_drive::db::repository::upload_session_part_repo;
    use aster_drive::services::files::upload;
    use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "corruptreceipt",
        "corrupt-receipt@test.com",
        "password123",
    )
    .await
    .unwrap();
    let init = upload::init_upload(
        &state,
        user.id,
        "corrupt-receipt.bin",
        10_485_760,
        None,
        None,
    )
    .await
    .unwrap();
    let upload_id = init.upload_id.unwrap();
    let chunk0 = vec![b'A'; TEST_CHUNK_SIZE];
    let chunk1 = vec![b'B'; TEST_CHUNK_SIZE];
    upload::upload_chunk(&state, &upload_id, 0, user.id, &chunk0)
        .await
        .unwrap();
    upload::upload_chunk(&state, &upload_id, 1, user.id, &chunk1)
        .await
        .unwrap();

    let mut receipt =
        upload_session_part_repo::find_by_upload_and_part(state.writer_db(), &upload_id, 1)
            .await
            .unwrap()
            .unwrap()
            .into_active_model();
    receipt.etag = Set("corrupted-receipt".to_string());
    receipt.update(state.writer_db()).await.unwrap();

    let retry_error = match upload::upload_chunk(&state, &upload_id, 0, user.id, &chunk0).await {
        Ok(_) => panic!("corrupted local receipt must reject a retry"),
        Err(error) => error,
    };
    assert!(retry_error.message().contains("receipt is corrupted"));

    let mut receipt =
        upload_session_part_repo::find_by_upload_and_part(state.writer_db(), &upload_id, 1)
            .await
            .unwrap()
            .unwrap()
            .into_active_model();
    receipt.etag = Set(upload::test_support::offset_staging_receipt_etag().to_string());
    receipt.size = Set(1);
    receipt.update(state.writer_db()).await.unwrap();

    let progress_error = match upload::get_progress(&state, &upload_id, user.id).await {
        Ok(_) => panic!("corrupted local receipt size must reject progress recovery"),
        Err(error) => error,
    };
    assert!(progress_error.message().contains("receipt is corrupted"));

    let complete_error = upload::complete_upload(&state, &upload_id, user.id, None)
        .await
        .unwrap_err();
    assert!(complete_error.message().contains("receipt is invalid"));
}

#[actix_web::test]
async fn test_direct_and_chunked_upload_do_not_dedup_local_by_default() {
    let state = common::setup().await;
    let user =
        common::create_test_account(&state, "compareuser", "compare@test.com", "password123")
            .await
            .unwrap();

    let (direct_blob, chunked_blob) = upload_same_content_direct_and_chunked(&state, user.id).await;

    assert_ne!(direct_blob.id, chunked_blob.id);
    assert_ne!(direct_blob.hash, chunked_blob.hash);
    assert_eq!(direct_blob.size, chunked_blob.size);
    assert_eq!(direct_blob.ref_count, 1);
    assert_eq!(chunked_blob.ref_count, 1);
}

#[actix_web::test]
async fn test_direct_and_chunked_upload_share_blob_when_local_dedup_enabled() {
    let state = common::setup().await;
    set_default_local_content_dedup(&state, true).await;
    let user =
        common::create_test_account(&state, "compareuser", "compare@test.com", "password123")
            .await
            .unwrap();

    let (direct_blob, chunked_blob) = upload_same_content_direct_and_chunked(&state, user.id).await;

    assert_eq!(direct_blob.id, chunked_blob.id);
    assert_eq!(direct_blob.hash, chunked_blob.hash);
    assert_eq!(direct_blob.size, chunked_blob.size);
    assert_eq!(direct_blob.ref_count, 2);
}

#[actix_web::test]
async fn test_concurrent_chunked_dedup_complete_reuses_blob_without_overwrite() {
    use aster_drive::db::repository::file_repo;
    use aster_drive::services::files::upload;

    let state = common::setup().await;
    set_default_local_content_dedup(&state, true).await;
    let user = common::create_test_account(
        &state,
        "chunkeddedupuser",
        "chunkeddedup@test.com",
        "password123",
    )
    .await
    .unwrap();

    let pattern = b"concurrent chunked dedup payload\n";
    let content = pattern.repeat((10_485_760 / pattern.len()) + 1);
    let content = content[..10_485_760].to_vec();
    let mut upload_ids = Vec::new();

    for name in ["dedup-a.bin", "dedup-b.bin"] {
        let init = upload::init_upload(&state, user.id, name, content.len() as i64, None, None)
            .await
            .unwrap();
        let upload_id = init.upload_id.unwrap();
        let total_chunks = init.total_chunks.unwrap();
        let chunk_size = init.chunk_size.unwrap() as usize;
        for chunk_number in 0..total_chunks {
            let start = chunk_number as usize * chunk_size;
            let end = ((chunk_number as usize + 1) * chunk_size).min(content.len());
            upload::upload_chunk(
                &state,
                &upload_id,
                chunk_number,
                user.id,
                &content[start..end],
            )
            .await
            .unwrap();
        }
        upload_ids.push(upload_id);
    }

    let mut tasks = JoinSet::new();
    for upload_id in upload_ids {
        let state = state.clone();
        tasks.spawn(async move {
            upload::complete_upload(&state, &upload_id, user.id, None)
                .await
                .unwrap()
        });
    }

    let first = tasks.join_next().await.unwrap().unwrap();
    let second = tasks.join_next().await.unwrap().unwrap();
    let first_blob = file_repo::find_blob_by_id(state.writer_db(), first.blob_id)
        .await
        .unwrap();
    let second_blob = file_repo::find_blob_by_id(state.writer_db(), second.blob_id)
        .await
        .unwrap();
    let policy = policy_repo::find_by_id(state.writer_db(), first_blob.policy_id)
        .await
        .unwrap();
    let driver = state.driver_registry.get_driver(&policy).unwrap();

    assert_eq!(first_blob.id, second_blob.id);
    assert_eq!(first_blob.ref_count, 2);
    assert_eq!(driver.get(&first_blob.storage_path).await.unwrap(), content);
}

#[actix_web::test]
async fn test_concurrent_chunked_complete_same_user_commits_all_quota_and_sessions() {
    use aster_drive::db::repository::{upload_session_repo, user_repo};
    use aster_drive::services::files::upload;

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "chunkqlock",
        "chunked-quota-lock@test.com",
        "password123",
    )
    .await
    .unwrap();
    let mut upload_ids = Vec::new();
    for index in 0..PERSONAL_FINALIZATION_CONCURRENCY {
        upload_ids.push(
            prepare_chunked_upload_for_personal_scope(
                &state,
                user.id,
                &format!("chunked-lock-{index}.bin"),
                b'A' + u8::try_from(index).unwrap(),
            )
            .await,
        );
    }

    let quota_guard = hold_personal_quota_row(&state, user.id).await;
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(upload_ids.len()));
    let mut tasks = JoinSet::new();
    for upload_id in upload_ids.clone() {
        let state = state.clone();
        let barrier = barrier.clone();
        tasks.spawn(async move {
            barrier.wait().await;
            upload::complete_upload(&state, &upload_id, user.id, None).await
        });
    }

    if let Some(quota_guard) = quota_guard {
        assert_tasks_wait_for_quota_guard(&mut tasks).await;
        aster_forge_db::transaction::commit(quota_guard)
            .await
            .unwrap();
    }

    let mut files = Vec::new();
    while let Some(result) = tasks.join_next().await {
        files.push(result.unwrap().unwrap());
    }
    assert_eq!(files.len(), PERSONAL_FINALIZATION_CONCURRENCY);
    let mut file_ids: Vec<i64> = files.iter().map(|file| file.id).collect();
    file_ids.sort_unstable();
    file_ids.dedup();
    assert_eq!(file_ids.len(), PERSONAL_FINALIZATION_CONCURRENCY);

    let expected_size =
        i64::try_from(TEST_CHUNK_SIZE * 2 * PERSONAL_FINALIZATION_CONCURRENCY).unwrap();
    let owner = user_repo::find_by_id(state.writer_db(), user.id)
        .await
        .unwrap();
    assert_eq!(owner.storage_used, expected_size);
    for upload_id in upload_ids {
        let session = upload_session_repo::find_by_id(state.writer_db(), &upload_id)
            .await
            .unwrap();
        assert_eq!(
            session.status,
            aster_drive::types::UploadSessionStatus::Completed
        );
        assert!(session.file_id.is_some());
    }
}

#[actix_web::test]
async fn test_concurrent_chunked_complete_same_user_quota_boundary_rolls_back_loser() {
    use aster_drive::db::repository::{file_repo, upload_session_repo, user_repo};
    use aster_drive::entities::user;
    use aster_drive::services::files::upload;
    use sea_orm::{ActiveModelTrait, EntityTrait, PaginatorTrait, Set};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "chunkqrace",
        "chunked-quota-race@test.com",
        "password123",
    )
    .await
    .unwrap();
    let upload_ids = vec![
        prepare_chunked_upload_for_personal_scope(
            &state,
            user.id,
            "chunked-quota-race-a.bin",
            b'C',
        )
        .await,
        prepare_chunked_upload_for_personal_scope(
            &state,
            user.id,
            "chunked-quota-race-b.bin",
            b'D',
        )
        .await,
    ];

    let single_file_size = i64::try_from(TEST_CHUNK_SIZE * 2).unwrap();
    let mut active: user::ActiveModel = user_repo::find_by_id(state.writer_db(), user.id)
        .await
        .unwrap()
        .into();
    active.storage_quota = Set(single_file_size);
    active.update(state.writer_db()).await.unwrap();

    let quota_guard = hold_personal_quota_row(&state, user.id).await;
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(upload_ids.len()));
    let mut tasks = JoinSet::new();
    for upload_id in upload_ids.clone() {
        let state = state.clone();
        let barrier = barrier.clone();
        tasks.spawn(async move {
            barrier.wait().await;
            upload::complete_upload(&state, &upload_id, user.id, None).await
        });
    }

    if let Some(quota_guard) = quota_guard {
        assert_tasks_wait_for_quota_guard(&mut tasks).await;
        aster_forge_db::transaction::commit(quota_guard)
            .await
            .unwrap();
    }

    let mut succeeded = Vec::new();
    let mut quota_errors = 0;
    while let Some(result) = tasks.join_next().await {
        match result.unwrap() {
            Ok(file) => succeeded.push(file),
            Err(error) if error.code() == "E032" => quota_errors += 1,
            Err(error) => panic!("unexpected concurrent completion error: {error}"),
        }
    }
    assert_eq!(succeeded.len(), 1);
    assert_eq!(quota_errors, 1);

    let owner = user_repo::find_by_id(state.writer_db(), user.id)
        .await
        .unwrap();
    assert_eq!(owner.storage_used, single_file_size);
    assert_eq!(
        file_repo::find_by_folder(state.writer_db(), user.id, None)
            .await
            .unwrap()
            .into_iter()
            .filter(|file| file.name.starts_with("chunked-quota-race-"))
            .count(),
        1,
        "quota loser must roll back its file row"
    );
    assert_eq!(
        aster_drive::entities::file_blob::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        1,
        "quota loser must roll back its blob row"
    );

    let mut completed = 0;
    let mut failed = 0;
    for upload_id in upload_ids {
        let session = upload_session_repo::find_by_id(state.writer_db(), &upload_id)
            .await
            .unwrap();
        match session.status {
            aster_drive::types::UploadSessionStatus::Completed => {
                completed += 1;
                assert!(session.file_id.is_some());
            }
            aster_drive::types::UploadSessionStatus::Failed => {
                failed += 1;
                assert_eq!(session.file_id, None);
            }
            status => panic!("unexpected upload session status: {status:?}"),
        }
    }
    assert_eq!((completed, failed), (1, 1));
}

#[actix_web::test]
async fn test_concurrent_chunked_complete_same_team_serializes_team_quota() {
    use aster_drive::db::repository::{team_repo, upload_session_repo};
    use aster_drive::services::{files::upload, workspace::team};

    let state = common::setup().await;
    let owner = common::create_test_account(
        &state,
        "teamqlock",
        "team-chunked-lock@test.com",
        "password123",
    )
    .await
    .unwrap();
    let team = team::create_team(
        &state,
        owner.id,
        team::CreateTeamInput {
            name: "Chunked quota lock team".to_string(),
            description: None,
        },
    )
    .await
    .unwrap();
    let mut upload_ids = Vec::new();
    for index in 0..TEAM_FINALIZATION_CONCURRENCY {
        upload_ids.push(
            prepare_chunked_upload_for_team_scope(
                &state,
                team.id,
                owner.id,
                &format!("team-chunked-lock-{index}.bin"),
                b'E' + u8::try_from(index).unwrap(),
            )
            .await,
        );
    }

    let quota_guard = hold_team_quota_row(&state, team.id).await;
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(upload_ids.len()));
    let mut tasks = JoinSet::new();
    for upload_id in upload_ids.clone() {
        let state = state.clone();
        let barrier = barrier.clone();
        tasks.spawn(async move {
            barrier.wait().await;
            upload::complete_upload_for_team(&state, team.id, &upload_id, owner.id, None).await
        });
    }
    if let Some(quota_guard) = quota_guard {
        assert_tasks_wait_for_quota_guard(&mut tasks).await;
        aster_forge_db::transaction::commit(quota_guard)
            .await
            .unwrap();
    }
    while let Some(result) = tasks.join_next().await {
        result.unwrap().unwrap();
    }

    let team_after = team_repo::find_by_id(state.writer_db(), team.id)
        .await
        .unwrap();
    assert_eq!(
        team_after.storage_used,
        i64::try_from(TEST_CHUNK_SIZE * 2 * TEAM_FINALIZATION_CONCURRENCY).unwrap()
    );
    let mut file_ids = Vec::new();
    for upload_id in upload_ids {
        let session = upload_session_repo::find_by_id(state.writer_db(), &upload_id)
            .await
            .unwrap();
        assert_eq!(
            session.status,
            aster_drive::types::UploadSessionStatus::Completed
        );
        let file_id = session
            .file_id
            .expect("completed session must reference a file");
        file_ids.push(file_id);
        assert_eq!(session.team_id, Some(team.id));
    }
    file_ids.sort_unstable();
    file_ids.dedup();
    assert_eq!(file_ids.len(), TEAM_FINALIZATION_CONCURRENCY);
}

#[actix_web::test]
async fn test_concurrent_chunked_complete_same_team_quota_boundary_rolls_back_loser() {
    use aster_drive::db::repository::{file_repo, team_repo, upload_session_repo, version_repo};
    use aster_drive::entities::team as team_entity;
    use aster_drive::services::{files::upload, workspace::team};
    use sea_orm::{ActiveModelTrait, EntityTrait, PaginatorTrait, Set};

    let state = common::setup().await;
    let owner = common::create_test_account(
        &state,
        "teamqrace",
        "team-chunked-quota-race@test.com",
        "password123",
    )
    .await
    .unwrap();
    let team = team::create_team(
        &state,
        owner.id,
        team::CreateTeamInput {
            name: "Chunked quota race team".to_string(),
            description: None,
        },
    )
    .await
    .unwrap();
    let upload_ids = vec![
        prepare_chunked_upload_for_team_scope(
            &state,
            team.id,
            owner.id,
            "team-chunked-quota-race-a.bin",
            b'G',
        )
        .await,
        prepare_chunked_upload_for_team_scope(
            &state,
            team.id,
            owner.id,
            "team-chunked-quota-race-b.bin",
            b'H',
        )
        .await,
    ];

    let single_file_size = i64::try_from(TEST_CHUNK_SIZE * 2).unwrap();
    let mut active: team_entity::ActiveModel = team_repo::find_by_id(state.writer_db(), team.id)
        .await
        .unwrap()
        .into();
    active.storage_quota = Set(single_file_size);
    active.update(state.writer_db()).await.unwrap();

    let quota_guard = hold_team_quota_row(&state, team.id).await;
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(upload_ids.len()));
    let mut tasks = JoinSet::new();
    for upload_id in upload_ids.clone() {
        let state = state.clone();
        let barrier = barrier.clone();
        tasks.spawn(async move {
            barrier.wait().await;
            upload::complete_upload_for_team(&state, team.id, &upload_id, owner.id, None).await
        });
    }

    if let Some(quota_guard) = quota_guard {
        assert_tasks_wait_for_quota_guard(&mut tasks).await;
        aster_forge_db::transaction::commit(quota_guard)
            .await
            .unwrap();
    }

    let mut succeeded = 0;
    let mut quota_errors = 0;
    while let Some(result) = tasks.join_next().await {
        match result.unwrap() {
            Ok(_) => succeeded += 1,
            Err(error) if error.code() == "E032" => quota_errors += 1,
            Err(error) => panic!("unexpected team completion error: {error}"),
        }
    }
    assert_eq!((succeeded, quota_errors), (1, 1));

    let team_after = team_repo::find_by_id(state.writer_db(), team.id)
        .await
        .unwrap();
    assert_eq!(team_after.storage_used, single_file_size);
    let live_files = file_repo::find_by_team_folder(state.writer_db(), team.id, None)
        .await
        .unwrap()
        .into_iter()
        .filter(|file| file.name.starts_with("team-chunked-quota-race-"))
        .collect::<Vec<_>>();
    assert_eq!(live_files.len(), 1);
    let completed_file = &live_files[0];
    assert!(completed_file.deleted_at.is_none());
    assert_eq!(
        aster_drive::entities::file_blob::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        1
    );

    let statuses = futures::future::join_all(
        upload_ids
            .iter()
            .map(|upload_id| upload_session_repo::find_by_id(state.writer_db(), upload_id)),
    )
    .await;
    let mut completed = 0;
    let mut failed = 0;
    for session in statuses {
        let session = session.unwrap();
        assert_eq!(session.team_id, Some(team.id));
        match session.status {
            aster_drive::types::UploadSessionStatus::Completed => {
                let file_id = session
                    .file_id
                    .expect("completed session must reference a file");
                assert_eq!(file_id, completed_file.id);
                let versions = version_repo::find_by_file_id(state.writer_db(), file_id)
                    .await
                    .unwrap();
                assert!(versions.is_empty());
                completed += 1;
            }
            aster_drive::types::UploadSessionStatus::Failed => {
                assert!(session.file_id.is_none());
                failed += 1;
            }
            status => panic!("unexpected upload session status: {status:?}"),
        }
    }
    assert_eq!((completed, failed), (1, 1));
    assert_eq!(
        aster_drive::entities::file_version::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        0
    );
}

#[actix_web::test]
async fn test_local_direct_upload_with_declared_size_avoids_global_temp_dirs_and_reuses_blob() {
    use aster_drive::db::repository::{file_repo, policy_repo};

    let state = common::setup().await;
    set_default_local_content_dedup(&state, true).await;
    let db = state.writer_db().clone();
    let driver_registry = state.driver_registry.clone();
    let temp_roots = vec![
        state.config.server.temp_dir.clone(),
        state.config.server.upload_temp_dir.clone(),
    ];
    let temp_snapshot_before = snapshot_temp_roots(&temp_roots).unwrap();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let data = b"hello local direct dedup";
    let (boundary, payload) = build_multipart_payload("local-a.txt", data);
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/files/upload?declared_size={}",
            data.len()
        ))
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

    let (boundary2, payload2) = build_multipart_payload("local-b.txt", data);
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/files/upload?declared_size={}",
            data.len()
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary2}"),
        ))
        .set_payload(payload2)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let file_id2 = body["data"]["id"].as_i64().unwrap();

    let temp_snapshot_after = snapshot_temp_roots(&temp_roots).unwrap();
    assert_eq!(
        temp_snapshot_after, temp_snapshot_before,
        "local direct upload fast path should not touch global temp/upload temp dirs"
    );

    let file = file_repo::find_by_id(&db, file_id).await.unwrap();
    let file2 = file_repo::find_by_id(&db, file_id2).await.unwrap();
    let blob = file_repo::find_blob_by_id(&db, file.blob_id).await.unwrap();
    let blob2 = file_repo::find_blob_by_id(&db, file2.blob_id)
        .await
        .unwrap();

    assert_eq!(blob.id, blob2.id);
    assert_eq!(blob.hash, blob2.hash);
    assert_eq!(blob.ref_count, 2);

    let policy = policy_repo::find_by_id(&db, blob.policy_id).await.unwrap();
    let driver = driver_registry.get_driver(&policy).unwrap();
    let stored = driver.get(&blob.storage_path).await.unwrap();
    assert_eq!(stored, data);
}

#[actix_web::test]
async fn test_chunked_upload_cancel() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 初始化大文件上传（强制 chunked 模式）
    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload/init")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "filename": "big.bin",
            "total_size": 10_485_760  // 10MB → 超过 chunk_size(5MB) → chunked
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;

    if let Some(upload_id) = body["data"]["upload_id"].as_str() {
        // 取消上传
        let req = test::TestRequest::delete()
            .uri(&format!("/api/v1/files/upload/{upload_id}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        // 再查进度应该 404
        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/files/upload/{upload_id}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status() == 404 || resp.status() == 410);
    }
}

/// 测试 init_upload：Local 策略下不返回 presigned
#[actix_web::test]
async fn test_init_upload_local_never_presigned() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload/init")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "filename": "test.bin",
            "total_size": 1024
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let mode = body["data"]["mode"].as_str().unwrap();
    assert_ne!(
        mode, "presigned",
        "local storage should never use presigned"
    );
    assert!(body["data"]["presigned_url"].is_null());
}

#[tokio::test]
async fn test_chunked_init_persists_explicit_offset_staging_kind() {
    use aster_drive::db::repository::upload_session_repo;
    use aster_drive::services::files::upload;
    use aster_drive::types::{UploadMode, UploadSessionKind};

    let state = common::setup().await;
    let user = common::create_test_account(&state, "kindinit", "kind-init@test.com", "password123")
        .await
        .unwrap();
    let response = upload::init_upload(
        &state,
        user.id,
        "kind-init.bin",
        10 * 1024 * 1024,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(response.mode, UploadMode::Chunked);
    let session =
        upload_session_repo::find_by_id(state.writer_db(), response.upload_id.as_deref().unwrap())
            .await
            .unwrap();
    assert_eq!(session.session_kind, UploadSessionKind::OffsetStaging);
}

#[tokio::test]
async fn test_explicit_session_kind_rejects_incompatible_fields() {
    use aster_drive::services::files::upload;
    use aster_drive::types::{UploadSessionKind, UploadSessionStatus};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "kindcorrupt",
        "kind-corrupt@test.com",
        "password123",
    )
    .await
    .unwrap();
    let upload_id = new_test_upload_id();
    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            &upload_id,
            UploadSessionStatus::Uploading,
            chrono::Utc::now() + chrono::Duration::hours(1),
            UploadSessionKind::ProviderRelayMultipart,
        )
        .chunks(1, 0),
    )
    .await;

    let error = match upload::get_progress(&state, &upload_id, user.id).await {
        Ok(_) => panic!("provider multipart kind without multipart id must be rejected"),
        Err(error) => error,
    };
    assert_eq!(error.code(), "E057");
    assert!(error.message().contains("multipart fields"));
}

#[tokio::test]
async fn test_relay_kind_rejects_missing_object_temp_key() {
    use aster_drive::services::files::upload;
    use aster_drive::types::UploadSessionStatus;

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "relaykindbad",
        "relay-kind-corrupt@test.com",
        "password123",
    )
    .await
    .unwrap();
    let policy = create_dead_remote_policy(&state).await;
    let upload_id = new_test_upload_id();
    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            &upload_id,
            UploadSessionStatus::Uploading,
            chrono::Utc::now() + chrono::Duration::hours(1),
            UploadSessionKind::ProviderRelayMultipart,
        )
        .chunks(1, 0)
        .policy(policy.id)
        .object_upload(None, Some("multipart")),
    )
    .await;

    let error = match upload::get_progress(&state, &upload_id, user.id).await {
        Ok(_) => panic!("relay session without temp key must be rejected"),
        Err(error) => error,
    };
    assert_eq!(error.code(), "E057");
    assert!(error.message().contains("temporary object fields"));
}

#[actix_web::test]
async fn test_relay_chunk_endpoint_rejects_missing_object_temp_key() {
    use aster_drive::types::{UploadSessionKind, UploadSessionStatus};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "relayendpoint",
        "relay-endpoint@test.com",
        "password123",
    )
    .await
    .unwrap();
    let (access_token, _) =
        local::issue_tokens_for_session(&state, user.id, user.session_version, None, None)
            .await
            .unwrap();
    let upload_id = new_test_upload_id();
    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            &upload_id,
            UploadSessionStatus::Uploading,
            chrono::Utc::now() + chrono::Duration::hours(1),
            UploadSessionKind::ProviderRelayMultipart,
        )
        .chunks(1, 0)
        .object_upload(None, Some("multipart")),
    )
    .await;

    let app = create_test_app!(state.clone());
    let request = test::TestRequest::put()
        .uri(&format!("/api/v1/files/upload/{upload_id}/0"))
        .insert_header(("Authorization", format!("Bearer {access_token}")))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(b"1234567890".to_vec())
        .to_request();
    let response = test::call_service(&app, request).await;
    assert!(!response.status().is_success());
    let body: Value = test::read_body_json(response).await;
    assert_upload_error_contract(&body, ApiErrorCode::UploadSessionCorrupted.as_str());
}

#[tokio::test]
async fn test_relay_progress_rejects_zero_and_out_of_range_part_numbers() {
    use aster_drive::db::repository::upload_session_part_repo;
    use aster_drive::services::files::upload;
    use aster_drive::types::{UploadSessionKind, UploadSessionStatus};

    for (index, part_number) in [0, 3].into_iter().enumerate() {
        let state = common::setup().await;
        let user = common::create_test_account(
            &state,
            &format!("relayrange{index}"),
            &format!("relay-range{index}@test.com"),
            "password123",
        )
        .await
        .unwrap();
        let upload_id = new_test_upload_id();
        create_upload_session(
            &state,
            user.id,
            UploadSessionSpec::new(
                &upload_id,
                UploadSessionStatus::Uploading,
                chrono::Utc::now() + chrono::Duration::hours(1),
                UploadSessionKind::ProviderRelayMultipart,
            )
            .chunks(2, 1)
            .object_upload(Some("files/temp"), Some("multipart")),
        )
        .await;
        upload_session_part_repo::upsert_part(
            state.writer_db(),
            &upload_id,
            part_number,
            "etag",
            5,
        )
        .await
        .unwrap();

        let error = match upload::get_progress(&state, &upload_id, user.id).await {
            Ok(_) => panic!("invalid provider part number must be rejected"),
            Err(error) => error,
        };
        assert!(error.message().contains("out of range"));
    }
}

#[tokio::test]
async fn test_explicit_staging_kind_does_not_fall_back_when_file_is_missing() {
    use aster_drive::db::repository::upload_session_part_repo;
    use aster_drive::services::files::upload;
    use aster_drive::types::{UploadSessionKind, UploadSessionStatus};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "missingstaging",
        "missing-staging@test.com",
        "password123",
    )
    .await
    .unwrap();
    let upload_id = new_test_upload_id();
    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            &upload_id,
            UploadSessionStatus::Uploading,
            chrono::Utc::now() + chrono::Duration::hours(1),
            UploadSessionKind::OffsetStaging,
        )
        .chunks(1, 1),
    )
    .await;
    upload_session_part_repo::upsert_part(
        state.writer_db(),
        &upload_id,
        1,
        upload::test_support::offset_staging_receipt_etag(),
        10,
    )
    .await
    .unwrap();

    let error = upload::complete_upload(&state, &upload_id, user.id, None)
        .await
        .expect_err("missing explicit staging file must fail completion");
    assert!(error.message().contains("stat chunk staging file"));
}

#[tokio::test]
async fn test_presigned_session_kind_rejects_server_chunk_put() {
    use actix_web::FromRequest;
    use aster_drive::services::files::upload;
    use aster_drive::types::{UploadSessionKind, UploadSessionStatus};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "kindpresigned",
        "kind-presigned@test.com",
        "password123",
    )
    .await
    .unwrap();
    let upload_id = new_test_upload_id();
    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            &upload_id,
            UploadSessionStatus::Uploading,
            chrono::Utc::now() + chrono::Duration::hours(1),
            UploadSessionKind::ProviderPresignedMultipart,
        )
        .chunks(1, 0)
        .object_upload(Some("files/temp"), Some("multipart")),
    )
    .await;

    let error = match upload::upload_chunk(&state, &upload_id, 0, user.id, b"12345").await {
        Ok(_) => panic!("presigned session must not enter server chunk path"),
        Err(error) => error,
    };
    assert_eq!(error.code(), "E057");
    assert!(error.message().contains("presigned"));

    let payload_upload_id = new_test_upload_id();
    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            &payload_upload_id,
            UploadSessionStatus::Uploading,
            chrono::Utc::now() + chrono::Duration::hours(1),
            UploadSessionKind::ProviderPresignedMultipart,
        )
        .chunks(1, 0)
        .object_upload(Some("files/temp"), Some("multipart")),
    )
    .await;
    let (request, mut dev_payload) = actix_web::test::TestRequest::default()
        .set_payload(b"12345".to_vec())
        .to_http_parts();
    let payload = actix_web::web::Payload::from_request(&request, &mut dev_payload)
        .await
        .expect("test payload should extract");
    let error =
        match upload::upload_chunk_payload(&state, &payload_upload_id, 0, user.id, payload).await {
            Ok(_) => panic!("presigned payload PUT must be rejected"),
            Err(error) => error,
        };
    assert_eq!(error.code(), "E057");
    assert!(error.message().contains("presigned"));
}

/// 并发上传同一分片不会导致 received_count 多算（TOCTOU 修复验证）
#[tokio::test]
async fn test_concurrent_chunk_upload_idempotent() {
    use aster_drive::services::files::upload;
    use std::sync::Arc;

    let state = Arc::new(common::setup().await);
    let user = common::create_test_account(
        state.as_ref(),
        "testuser",
        "test@example.com",
        "password123",
    )
    .await
    .unwrap();
    let upload_id = new_test_upload_id();
    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            &upload_id,
            aster_drive::types::UploadSessionStatus::Uploading,
            chrono::Utc::now() + chrono::Duration::hours(1),
            UploadSessionKind::OffsetStaging,
        )
        .chunks(2, 0),
    )
    .await;
    tokio::fs::create_dir_all(aster_forge_utils::paths::upload_temp_dir(
        &state.config.server.upload_temp_dir,
        &upload_id,
    ))
    .await
    .unwrap();
    let staging_path = upload::test_support::offset_staging_file_path(
        &state.config.server.upload_temp_dir,
        &upload_id,
    );
    let staging_file = tokio::fs::File::create(&staging_path).await.unwrap();
    staging_file.set_len(10).await.unwrap();

    let chunk_data = b"12345".to_vec();
    let state1 = Arc::clone(&state);
    let state2 = Arc::clone(&state);
    let upload_id1 = upload_id.clone();
    let upload_id2 = upload_id.clone();
    let chunk1 = chunk_data.clone();
    let chunk2 = chunk_data.clone();
    let concurrency = upload::test_support::install_same_chunk_exclusion_observer(&upload_id);

    let (first, second) = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        tokio::join!(
            upload::upload_chunk(&state1, &upload_id1, 0, user.id, &chunk1),
            upload::upload_chunk(&state2, &upload_id2, 0, user.id, &chunk2),
        )
    })
    .await
    .expect("duplicate requests must rendezvous at the staging write lock entry");

    let first = first.unwrap();
    let second = second.unwrap();
    let final_progress = upload::get_progress(&state, &upload_id, user.id)
        .await
        .unwrap();

    assert_eq!(
        concurrency.entry_count(),
        2,
        "both duplicate requests must reach the staging write lock entry"
    );
    assert!(
        !concurrency.overlap_observed(),
        "same-chunk writes must remain mutually exclusive"
    );
    assert!(
        [first.received_count, second.received_count].contains(&1),
        "at least one concurrent upload should observe received_count=1"
    );
    assert_eq!(
        final_progress.received_count, 1,
        "duplicate concurrent chunk upload should not increment count twice"
    );

    let receipts = aster_drive::db::repository::upload_session_part_repo::list_all_by_upload(
        state.writer_db(),
        &upload_id,
    )
    .await
    .unwrap();
    assert_eq!(
        receipts.len(),
        1,
        "duplicate chunk must persist one receipt"
    );
    assert_eq!(receipts[0].part_number, 1);
    assert_eq!(
        receipts[0].etag,
        upload::test_support::offset_staging_receipt_etag()
    );
    assert_eq!(receipts[0].size, chunk_data.len() as i64);
    let staged = tokio::fs::read(&staging_path).await.unwrap();
    assert_eq!(&staged[..chunk_data.len()], chunk_data.as_slice());

    drop(concurrency);
    let third = upload::upload_chunk(&state, &upload_id, 0, user.id, &chunk_data)
        .await
        .unwrap();
    assert_eq!(
        third.received_count, 1,
        "sequential duplicate should remain idempotent after concurrent uploads"
    );
}

#[tokio::test]
async fn test_upload_session_part_upsert_updates_existing_row_without_duplicates() {
    use aster_drive::db::repository::{upload_session_part_repo, upload_session_repo};

    let state = common::setup().await;
    let user = common::create_test_account(&state, "partuser", "part@test.com", "password123")
        .await
        .unwrap();
    let upload_id = new_test_upload_id();

    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            &upload_id,
            aster_drive::types::UploadSessionStatus::Uploading,
            chrono::Utc::now() + chrono::Duration::hours(1),
            UploadSessionKind::OffsetStaging,
        )
        .chunks(2, 0),
    )
    .await;

    let first =
        upload_session_part_repo::upsert_part(state.writer_db(), &upload_id, 1, "etag-1", 5)
            .await
            .unwrap();
    assert!(first.inserted);
    assert_eq!(first.model.etag, "etag-1");
    assert_eq!(first.model.size, 5);

    let second =
        upload_session_part_repo::upsert_part(state.writer_db(), &upload_id, 1, "etag-2", 7)
            .await
            .unwrap();
    assert!(!second.inserted);
    assert_eq!(second.model.etag, "etag-2");
    assert_eq!(second.model.size, 7);

    let parts = upload_session_part_repo::list_by_upload(state.writer_db(), &upload_id)
        .await
        .unwrap();
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].part_number, 1);
    assert_eq!(parts[0].etag, "etag-2");
    assert_eq!(parts[0].size, 7);

    let session = upload_session_repo::find_by_id(state.writer_db(), &upload_id)
        .await
        .unwrap();
    assert_eq!(session.received_count, 0);
}

#[actix_web::test]
async fn test_upload_chunk_rejects_wrong_chunk_size() {
    use aster_drive::services::files::upload;

    let state = common::setup().await;
    let user = common::create_test_account(&state, "sizeuser", "size@test.com", "password123")
        .await
        .unwrap();

    let init = upload::init_upload(&state, user.id, "size-check.bin", 10_485_760, None, None)
        .await
        .unwrap();
    assert_eq!(init.mode, aster_drive::types::UploadMode::Chunked);

    let upload_id = init.upload_id.unwrap();
    let err = match upload::upload_chunk(&state, &upload_id, 0, user.id, b"short").await {
        Ok(_) => panic!("wrong-sized chunk upload should fail"),
        Err(err) => err,
    };
    assert_eq!(err.code(), "E056");
    assert!(err.message().contains("size mismatch"));

    let progress = upload::get_progress(&state, &upload_id, user.id)
        .await
        .unwrap();
    assert_eq!(progress.received_count, 0);
    assert!(progress.chunks_on_disk.is_empty());
}

#[actix_web::test]
async fn test_complete_upload_is_idempotent_after_completion() {
    use aster_drive::db::repository::{upload_session_repo, user_repo};
    use aster_drive::services::files::upload;

    let state = common::setup().await;
    let user = common::create_test_account(&state, "idemuser", "idem@test.com", "password123")
        .await
        .unwrap();

    let init = upload::init_upload(&state, user.id, "idempotent.txt", 10_485_760, None, None)
        .await
        .unwrap();
    assert_eq!(init.mode, aster_drive::types::UploadMode::Chunked);
    assert_eq!(init.total_chunks, Some(2));

    let upload_id = init.upload_id.unwrap();
    let chunk0 = vec![b'A'; TEST_CHUNK_SIZE];
    let chunk1 = vec![b'B'; TEST_CHUNK_SIZE];
    upload::upload_chunk(&state, &upload_id, 0, user.id, &chunk0)
        .await
        .unwrap();
    upload::upload_chunk(&state, &upload_id, 1, user.id, &chunk1)
        .await
        .unwrap();

    let first = upload::complete_upload(&state, &upload_id, user.id, None)
        .await
        .unwrap();
    let user_after_first = user_repo::find_by_id(state.writer_db(), user.id)
        .await
        .unwrap();
    let second = upload::complete_upload(&state, &upload_id, user.id, None)
        .await
        .unwrap();
    let user_after_second = user_repo::find_by_id(state.writer_db(), user.id)
        .await
        .unwrap();

    assert_eq!(second.id, first.id);
    assert_eq!(second.blob_id, first.blob_id);
    assert_eq!(second.name, "idempotent.txt");
    assert_eq!(user_after_first.storage_used, 10_485_760);
    assert_eq!(
        user_after_second.storage_used, user_after_first.storage_used,
        "completed retry must not charge quota twice"
    );

    let session = upload_session_repo::find_by_id(state.writer_db(), &upload_id)
        .await
        .unwrap();
    assert_eq!(
        session.status,
        aster_drive::types::UploadSessionStatus::Completed
    );
    assert_eq!(session.file_id, Some(first.id));
}

#[actix_web::test]
async fn test_complete_chunked_upload_quota_failure_does_not_complete_session_or_charge_quota() {
    use aster_drive::db::repository::{upload_session_repo, user_repo};
    use aster_drive::entities::user;
    use aster_drive::services::files::upload;
    use aster_drive::types::UploadSessionStatus;
    use sea_orm::{ActiveModelTrait, Set};

    let state = common::setup().await;
    let user =
        common::create_test_account(&state, "quotaend", "quota-complete@test.com", "password123")
            .await
            .unwrap();

    let init = upload::init_upload(
        &state,
        user.id,
        "quota-finalize.txt",
        10_485_760,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(init.mode, aster_drive::types::UploadMode::Chunked);
    assert_eq!(init.total_chunks, Some(2));

    let upload_id = init.upload_id.unwrap();
    let chunk0 = vec![b'A'; TEST_CHUNK_SIZE];
    let chunk1 = vec![b'B'; TEST_CHUNK_SIZE];
    upload::upload_chunk(&state, &upload_id, 0, user.id, &chunk0)
        .await
        .unwrap();
    upload::upload_chunk(&state, &upload_id, 1, user.id, &chunk1)
        .await
        .unwrap();

    let mut active: user::ActiveModel = user_repo::find_by_id(state.writer_db(), user.id)
        .await
        .unwrap()
        .into();
    active.storage_quota = Set(1);
    active.update(state.writer_db()).await.unwrap();

    let err = upload::complete_upload(&state, &upload_id, user.id, None)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "E032");

    let session = upload_session_repo::find_by_id(state.writer_db(), &upload_id)
        .await
        .unwrap();
    assert_eq!(session.status, UploadSessionStatus::Failed);
    assert_eq!(session.file_id, None);

    let user_after = user_repo::find_by_id(state.writer_db(), user.id)
        .await
        .unwrap();
    assert_eq!(user_after.storage_used, 0);
}

#[actix_web::test]
async fn test_complete_upload_marks_session_failed_after_missing_chunk_receipt() {
    use aster_drive::db::repository::{upload_session_part_repo, upload_session_repo};
    use aster_drive::services::files::upload;

    let state = common::setup().await;
    let user = common::create_test_account(&state, "faileduser", "failed@test.com", "password123")
        .await
        .unwrap();

    let init = upload::init_upload(&state, user.id, "broken.txt", 10_485_760, None, None)
        .await
        .unwrap();
    assert_eq!(init.mode, aster_drive::types::UploadMode::Chunked);

    let upload_id = init.upload_id.unwrap();
    let chunk0 = vec![b'A'; TEST_CHUNK_SIZE];
    let chunk1 = vec![b'B'; TEST_CHUNK_SIZE];
    upload::upload_chunk(&state, &upload_id, 0, user.id, &chunk0)
        .await
        .unwrap();
    upload::upload_chunk(&state, &upload_id, 1, user.id, &chunk1)
        .await
        .unwrap();

    upload_session_part_repo::delete_by_upload_and_part(state.writer_db(), &upload_id, 2)
        .await
        .unwrap();

    let err = upload::complete_upload(&state, &upload_id, user.id, None)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "E057");
    assert!(err.message().contains("receipt count mismatch"));

    let session = upload_session_repo::find_by_id(state.writer_db(), &upload_id)
        .await
        .unwrap();
    assert_eq!(
        session.status,
        aster_drive::types::UploadSessionStatus::Failed
    );
    assert_eq!(session.file_id, None);

    let retry_err = upload::complete_upload(&state, &upload_id, user.id, None)
        .await
        .unwrap_err();
    assert_eq!(retry_err.code(), "E057");
    assert!(retry_err.message().contains("previously"));

    let next_init =
        upload::init_upload(&state, user.id, "after-broken.txt", 10_485_760, None, None)
            .await
            .unwrap();
    let next_upload_id = next_init.upload_id.unwrap();
    upload::upload_chunk(&state, &next_upload_id, 0, user.id, &chunk0)
        .await
        .unwrap();
    upload::upload_chunk(&state, &next_upload_id, 1, user.id, &chunk1)
        .await
        .unwrap();

    let next_file = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        upload::complete_upload(&state, &next_upload_id, user.id, None),
    )
    .await
    .expect("failed staging validation must not block the next upload")
    .unwrap();
    assert_eq!(next_file.name, "after-broken.txt");
}

#[actix_web::test]
async fn test_complete_upload_rejects_truncated_offset_staging_file() {
    use aster_drive::db::repository::upload_session_repo;
    use aster_drive::services::files::upload;

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "truncatedstaging",
        "truncated-staging@test.com",
        "password123",
    )
    .await
    .unwrap();
    let init = upload::init_upload(
        &state,
        user.id,
        "truncated-staging.bin",
        10_485_760,
        None,
        None,
    )
    .await
    .unwrap();
    let upload_id = init.upload_id.unwrap();
    let chunk0 = vec![b'A'; TEST_CHUNK_SIZE];
    let chunk1 = vec![b'B'; TEST_CHUNK_SIZE];
    upload::upload_chunk(&state, &upload_id, 0, user.id, &chunk0)
        .await
        .unwrap();
    upload::upload_chunk(&state, &upload_id, 1, user.id, &chunk1)
        .await
        .unwrap();

    let staging_path = upload::test_support::offset_staging_file_path(
        &state.config.server.upload_temp_dir,
        &upload_id,
    );
    tokio::fs::OpenOptions::new()
        .write(true)
        .open(&staging_path)
        .await
        .unwrap()
        .set_len(10_485_759)
        .await
        .unwrap();

    let error = upload::complete_upload(&state, &upload_id, user.id, None)
        .await
        .unwrap_err();
    assert_eq!(error.code(), "E057");
    assert!(error.message().contains("staging file size mismatch"));
    assert_eq!(
        upload_session_repo::find_by_id(state.writer_db(), &upload_id)
            .await
            .unwrap()
            .status,
        aster_drive::types::UploadSessionStatus::Failed
    );
}

#[actix_web::test]
async fn test_complete_upload_keeps_presigned_multipart_session_retryable_after_storage_error() {
    use aster_drive::db::repository::upload_session_repo;
    use aster_drive::services::files::upload;
    use aster_drive::types::UploadSessionStatus;

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "presigretry",
        "presigned-retry@test.com",
        "password123",
    )
    .await
    .unwrap();
    let remote_policy = create_dead_remote_policy(&state).await;
    let upload_id = new_test_upload_id();

    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            &upload_id,
            UploadSessionStatus::Presigned,
            chrono::Utc::now() + chrono::Duration::hours(1),
            UploadSessionKind::RemotePresignedMultipart,
        )
        .chunks(2, 0)
        .policy(remote_policy.id)
        .object_upload(
            Some("upload/data/files/presigned-retry-temp"),
            Some("presigned-retry-multipart"),
        ),
    )
    .await;

    let parts = Some(vec![
        (1, "\"etag-1\"".to_string()),
        (2, "\"etag-2\"".to_string()),
    ]);
    let err = upload::complete_upload(&state, &upload_id, user.id, parts.clone())
        .await
        .unwrap_err();
    assert_eq!(err.code(), "E031");

    let session = upload_session_repo::find_by_id(state.writer_db(), &upload_id)
        .await
        .unwrap();
    assert_eq!(session.status, UploadSessionStatus::Presigned);
    assert_eq!(session.file_id, None);

    let retry_err = upload::complete_upload(&state, &upload_id, user.id, parts)
        .await
        .unwrap_err();
    assert_eq!(retry_err.code(), "E031");

    let session_after_retry = upload_session_repo::find_by_id(state.writer_db(), &upload_id)
        .await
        .unwrap();
    assert_eq!(session_after_retry.status, UploadSessionStatus::Presigned);
    assert_eq!(session_after_retry.file_id, None);
}

#[actix_web::test]
async fn test_complete_offset_staging_stream_relay_remains_retryable_after_storage_error() {
    use aster_drive::db::repository::upload_session_repo;
    use aster_drive::services::files::upload;
    use aster_drive::types::UploadSessionStatus;

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "rstagingretry",
        "remote-staging-retry@test.com",
        "password123",
    )
    .await
    .unwrap();
    let remote_policy = create_dead_remote_policy(&state).await;
    let upload_id = new_test_upload_id();
    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            &upload_id,
            UploadSessionStatus::Uploading,
            chrono::Utc::now() + chrono::Duration::hours(1),
            UploadSessionKind::OffsetStaging,
        )
        .chunks(2, 0)
        .policy(remote_policy.id),
    )
    .await;

    let temp_dir =
        aster_forge_utils::paths::upload_temp_dir(&state.config.server.upload_temp_dir, &upload_id);
    tokio::fs::create_dir_all(&temp_dir).await.unwrap();
    let staging_path = upload::test_support::offset_staging_file_path(
        &state.config.server.upload_temp_dir,
        &upload_id,
    );
    let staging = tokio::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&staging_path)
        .await
        .unwrap();
    staging.set_len(10).await.unwrap();

    upload::upload_chunk(&state, &upload_id, 0, user.id, b"12345")
        .await
        .unwrap();
    upload::upload_chunk(&state, &upload_id, 1, user.id, b"67890")
        .await
        .unwrap();

    let error = upload::complete_upload(&state, &upload_id, user.id, None)
        .await
        .unwrap_err();
    assert_eq!(error.code(), "E031");
    let session = upload_session_repo::find_by_id(state.writer_db(), &upload_id)
        .await
        .unwrap();
    assert_eq!(session.status, UploadSessionStatus::Uploading);
    assert_eq!(session.file_id, None);
    assert!(std::path::Path::new(&staging_path).exists());

    let retry_error = upload::complete_upload(&state, &upload_id, user.id, None)
        .await
        .unwrap_err();
    assert_eq!(retry_error.code(), "E031");
}

#[actix_web::test]
async fn test_file_upload_complete_rejects_assembling_session() {
    use aster_drive::services::files::upload;

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "assemblinguser",
        "assembling@test.com",
        "password123",
    )
    .await
    .unwrap();
    let upload_id = new_test_upload_id();
    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            &upload_id,
            aster_drive::types::UploadSessionStatus::Assembling,
            chrono::Utc::now() + chrono::Duration::hours(1),
            UploadSessionKind::OffsetStaging,
        )
        .chunks(2, 2),
    )
    .await;

    let err = upload::complete_upload(&state, &upload_id, user.id, None)
        .await
        .unwrap_err();
    assert!(err.message().contains("being processed"));
}

#[actix_web::test]
async fn test_file_upload_complete_completed_without_file_id_returns_refresh_hint() {
    use aster_drive::services::files::upload;

    let state = common::setup().await;
    let user =
        common::create_test_account(&state, "completeuser", "complete@test.com", "password123")
            .await
            .unwrap();
    let upload_id = new_test_upload_id();
    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            &upload_id,
            aster_drive::types::UploadSessionStatus::Completed,
            chrono::Utc::now() + chrono::Duration::hours(1),
            UploadSessionKind::OffsetStaging,
        )
        .chunks(0, 0),
    )
    .await;

    let err = upload::complete_upload(&state, &upload_id, user.id, None)
        .await
        .unwrap_err();
    assert!(err.message().contains("file_id not found"));
    assert!(err.message().contains("refresh"));
}

#[actix_web::test]
async fn test_file_upload_complete_presigned_multipart_requires_parts() {
    use aster_drive::services::files::upload;

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "partsuser",
        "multipartparts@test.com",
        "password123",
    )
    .await
    .unwrap();
    let upload_id = new_test_upload_id();
    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            &upload_id,
            aster_drive::types::UploadSessionStatus::Presigned,
            chrono::Utc::now() + chrono::Duration::hours(1),
            UploadSessionKind::ProviderPresignedMultipart,
        )
        .chunks(2, 0)
        .object_upload(Some("files/temp-key"), Some("multipart-id")),
    )
    .await;

    let err = upload::complete_upload(&state, &upload_id, user.id, None)
        .await
        .unwrap_err();
    assert!(err.message().contains("parts required"));
}

#[actix_web::test]
async fn test_file_upload_get_progress_lists_and_sorts_staging_receipts() {
    use aster_drive::db::repository::upload_session_part_repo;
    use aster_drive::services::files::upload;

    let state = common::setup().await;
    let user =
        common::create_test_account(&state, "progressuser", "progress@test.com", "password123")
            .await
            .unwrap();
    let upload_id = new_test_upload_id();
    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            &upload_id,
            aster_drive::types::UploadSessionStatus::Uploading,
            chrono::Utc::now() + chrono::Duration::hours(1),
            UploadSessionKind::OffsetStaging,
        )
        .chunks(2, 2),
    )
    .await;

    for part_number in [2, 1] {
        upload_session_part_repo::upsert_part(
            state.writer_db(),
            &upload_id,
            part_number,
            upload::test_support::offset_staging_receipt_etag(),
            5,
        )
        .await
        .unwrap();
    }

    let progress = upload::get_progress(&state, &upload_id, user.id)
        .await
        .unwrap();
    assert_eq!(progress.upload_id, upload_id);
    assert_eq!(progress.received_count, 2);
    assert_eq!(progress.total_chunks, 2);
    assert_eq!(progress.chunks_on_disk, vec![0, 1]);
}

#[actix_web::test]
async fn test_file_upload_get_progress_uses_db_parts_for_terminal_relay_multipart_sessions() {
    use aster_drive::db::repository::{upload_session_part_repo, upload_session_repo};
    use aster_drive::services::files::upload;
    use aster_drive::types::UploadSessionStatus;
    use chrono::Utc;
    use sea_orm::Set;

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "relayprog",
        "relay-progress@test.com",
        "password123",
    )
    .await
    .unwrap();
    let relay_policy = create_s3_default_policy(
        &state,
        user.id,
        "Test S3 Relay Progress",
        "http://127.0.0.1:9000",
        "unused-progress-bucket",
        r#"{"object_storage_upload_strategy":"relay_stream"}"#,
        5_242_880,
    )
    .await;

    for (status_name, status) in [
        ("assembling", UploadSessionStatus::Assembling),
        ("completed", UploadSessionStatus::Completed),
        ("failed", UploadSessionStatus::Failed),
    ] {
        let upload_id = new_test_upload_id();
        let now = Utc::now();
        upload_session_repo::create(
            state.writer_db(),
            aster_drive::entities::upload_session::ActiveModel {
                id: Set(upload_id.clone()),
                user_id: Set(user.id),
                team_id: Set(None),
                frontend_client_id: Set(None),
                filename: Set("relay-progress.bin".to_string()),
                total_size: Set(15),
                chunk_size: Set(5),
                total_chunks: Set(3),
                received_count: Set(2),
                folder_id: Set(None),
                policy_id: Set(relay_policy.id),
                status: Set(status),
                session_kind: Set(aster_drive::types::UploadSessionKind::ProviderRelayMultipart),
                object_temp_key: Set(Some(format!("files/{upload_id}"))),
                object_multipart_id: Set(Some(format!("multipart-{status_name}"))),
                provider_session_ciphertext: Set(None),
                file_id: Set(None),
                created_at: Set(now),
                expires_at: Set(now + chrono::Duration::hours(1)),
                updated_at: Set(now),
            },
        )
        .await
        .unwrap();

        upload_session_part_repo::upsert_part(state.writer_db(), &upload_id, 1, "etag-1", 5)
            .await
            .unwrap();
        upload_session_part_repo::upsert_part(state.writer_db(), &upload_id, 3, "etag-3", 5)
            .await
            .unwrap();

        let progress = upload::get_progress(&state, &upload_id, user.id)
            .await
            .unwrap();
        assert_eq!(progress.status, status);
        assert_eq!(progress.received_count, 2);
        assert_eq!(progress.total_chunks, 3);
        assert_eq!(progress.chunks_on_disk, vec![0, 2]);
    }
}

#[actix_web::test]
async fn test_sqlite_reader_routes_do_not_wait_for_busy_writer_pool() {
    use aster_forge_utils::raii::TempDirGuard;
    use sea_orm::{ConnectionTrait, TransactionTrait};

    let temp_dir =
        std::env::temp_dir().join(format!("asterdrive-reader-routes-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).expect("sqlite temp dir should be created");
    let _temp_dir_guard = TempDirGuard::new(temp_dir.clone(), "sqlite reader route test db");
    let database_url = format!("sqlite://{}?mode=rwc", temp_dir.join("reader.db").display());
    let state = common::setup_with_database_url(&database_url).await;
    assert!(
        state.sqlite_read_write_split(),
        "file SQLite test state should enable reader/writer split"
    );

    let app = create_test_app!(state.clone());
    let user =
        common::create_test_account(&state, "readerroute", "readerroute@test.com", "password123")
            .await
            .unwrap();
    let (access_token, _) =
        local::issue_tokens_for_session(&state, user.id, user.session_version, None, None)
            .await
            .unwrap();
    let upload_id = new_test_upload_id();
    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            &upload_id,
            aster_drive::types::UploadSessionStatus::Uploading,
            chrono::Utc::now() + chrono::Duration::hours(1),
            UploadSessionKind::OffsetStaging,
        )
        .chunks(3, 2),
    )
    .await;

    let txn = state.writer_db().begin().await.unwrap();
    txn.execute_unprepared("UPDATE users SET updated_at = updated_at WHERE id = -1")
        .await
        .unwrap();

    let folder_req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Authorization", format!("Bearer {access_token}")))
        .to_request();
    let folder_resp = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        test::call_service(&app, folder_req),
    )
    .await
    .expect("folder listing should not wait for the busy writer pool");
    assert_eq!(folder_resp.status(), 200);

    let progress_req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/upload/{upload_id}"))
        .insert_header(("Authorization", format!("Bearer {access_token}")))
        .to_request();
    let progress_resp = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        test::call_service(&app, progress_req),
    )
    .await
    .expect("upload progress should not wait for the busy writer pool");
    assert_eq!(progress_resp.status(), 200);
    let progress_body: Value = test::read_body_json(progress_resp).await;
    assert_eq!(progress_body["data"]["upload_id"], upload_id);
    assert_eq!(progress_body["data"]["received_count"], 2);

    txn.rollback().await.unwrap();
}

#[actix_web::test]
async fn test_file_upload_presign_parts_rejects_non_multipart_session() {
    use aster_drive::services::files::upload;

    let state = common::setup().await;
    let user =
        common::create_test_account(&state, "presignuser", "presign@test.com", "password123")
            .await
            .unwrap();
    let upload_id = new_test_upload_id();
    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            &upload_id,
            aster_drive::types::UploadSessionStatus::Presigned,
            chrono::Utc::now() + chrono::Duration::hours(1),
            UploadSessionKind::ProviderPresignedSingle,
        )
        .chunks(1, 0)
        .object_upload(Some("files/temp-key"), None),
    )
    .await;

    let err = upload::presign_parts(&state, &upload_id, user.id, vec![1])
        .await
        .unwrap_err();
    assert!(err.message().contains("not a multipart upload session"));
}

#[actix_web::test]
async fn test_file_upload_presign_parts_validates_part_number_batch() {
    use aster_drive::services::files::upload;

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "presignbatch",
        "presignbatch@test.com",
        "password123",
    )
    .await
    .unwrap();
    let upload_id = new_test_upload_id();
    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            &upload_id,
            aster_drive::types::UploadSessionStatus::Presigned,
            chrono::Utc::now() + chrono::Duration::hours(1),
            UploadSessionKind::ProviderPresignedMultipart,
        )
        .chunks(3, 0)
        .object_upload(Some("files/temp-key"), Some("multipart-id")),
    )
    .await;

    let err = upload::presign_parts(&state, &upload_id, user.id, vec![])
        .await
        .unwrap_err();
    assert_eq!(err.api_error_code(), ApiErrorCode::UploadPartNumbersEmpty);

    let err = upload::presign_parts(&state, &upload_id, user.id, vec![0])
        .await
        .unwrap_err();
    assert_eq!(
        err.api_error_code(),
        ApiErrorCode::UploadPartNumberOutOfRange
    );

    let too_many = (1..=65).collect();
    let err = upload::presign_parts(&state, &upload_id, user.id, too_many)
        .await
        .unwrap_err();
    assert_eq!(err.api_error_code(), ApiErrorCode::UploadPartNumbersTooMany);
}

#[actix_web::test]
async fn test_file_upload_cleanup_expired_removes_local_sessions_only() {
    use aster_drive::db::repository::upload_session_repo;
    use aster_drive::services::files::upload;

    let state = common::setup().await;
    let user =
        common::create_test_account(&state, "cleanupuser", "cleanup@test.com", "password123")
            .await
            .unwrap();

    let expired_id = new_test_upload_id();
    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            &expired_id,
            aster_drive::types::UploadSessionStatus::Uploading,
            chrono::Utc::now() - chrono::Duration::minutes(5),
            UploadSessionKind::OffsetStaging,
        )
        .chunks(2, 1),
    )
    .await;

    let completed_id = new_test_upload_id();
    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            &completed_id,
            aster_drive::types::UploadSessionStatus::Completed,
            chrono::Utc::now() - chrono::Duration::minutes(5),
            UploadSessionKind::OffsetStaging,
        )
        .chunks(0, 0)
        .file_id(123),
    )
    .await;

    let assembling_id = new_test_upload_id();
    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            &assembling_id,
            aster_drive::types::UploadSessionStatus::Assembling,
            chrono::Utc::now() - chrono::Duration::minutes(5),
            UploadSessionKind::OffsetStaging,
        )
        .chunks(2, 2),
    )
    .await;

    let expired_dir = aster_forge_utils::paths::upload_temp_dir(
        &state.config.server.upload_temp_dir,
        &expired_id,
    );
    tokio::fs::create_dir_all(&expired_dir).await.unwrap();
    tokio::fs::write(format!("{expired_dir}/chunk_0"), b"temp")
        .await
        .unwrap();
    let assembling_dir = aster_forge_utils::paths::upload_temp_dir(
        &state.config.server.upload_temp_dir,
        &assembling_id,
    );
    tokio::fs::create_dir_all(&assembling_dir).await.unwrap();
    tokio::fs::write(format!("{assembling_dir}/chunk_0"), b"temp")
        .await
        .unwrap();

    let cleaned = upload::cleanup_expired(&state).await.unwrap();
    assert_eq!(cleaned, 1);
    assert!(
        upload_session_repo::find_by_id(state.writer_db(), &expired_id)
            .await
            .is_err()
    );
    assert!(
        upload_session_repo::find_by_id(state.writer_db(), &completed_id)
            .await
            .is_ok()
    );
    assert!(
        upload_session_repo::find_by_id(state.writer_db(), &assembling_id)
            .await
            .is_ok()
    );
    assert!(
        !std::path::Path::new(&expired_dir).exists(),
        "expired temp dir should be removed"
    );
    assert!(
        std::path::Path::new(&assembling_dir).exists(),
        "assembling temp dir must not be removed while completion is in progress"
    );
    aster_forge_utils::fs::cleanup_temp_dir(&assembling_dir).await;
}

#[actix_web::test]
async fn test_file_upload_cleanup_expired_keeps_remote_sessions_when_storage_is_unavailable() {
    use aster_drive::db::repository::upload_session_repo;
    use aster_drive::services::files::upload;
    use aster_drive::types::UploadSessionStatus;

    let state = common::setup().await;
    let user =
        common::create_test_account(&state, "cleanrem", "cleanup-remote@test.com", "password123")
            .await
            .unwrap();
    let remote_policy = create_dead_remote_policy(&state).await;

    let upload_id = new_test_upload_id();
    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            &upload_id,
            UploadSessionStatus::Uploading,
            chrono::Utc::now() - chrono::Duration::minutes(5),
            UploadSessionKind::RemoteRelayMultipart,
        )
        .chunks(2, 1)
        .policy(remote_policy.id)
        .object_upload(
            Some("upload/data/files/cleanup-remote-temp"),
            Some("cleanup-remote-multipart"),
        ),
    )
    .await;

    let expired_dir =
        aster_forge_utils::paths::upload_temp_dir(&state.config.server.upload_temp_dir, &upload_id);
    tokio::fs::create_dir_all(&expired_dir).await.unwrap();
    tokio::fs::write(format!("{expired_dir}/chunk_0"), b"temp")
        .await
        .unwrap();

    let cleaned = upload::cleanup_expired(&state).await.unwrap();
    assert_eq!(cleaned, 0);

    let session = upload_session_repo::find_by_id(state.writer_db(), &upload_id)
        .await
        .unwrap();
    assert_eq!(session.status, UploadSessionStatus::Uploading);
    assert!(
        !std::path::Path::new(&expired_dir).exists(),
        "expired temp dir should still be removed when session is kept for retry"
    );
}

#[tokio::test]
async fn test_cancel_upload_aborts_presigned_multipart_session_on_rustfs() {
    use aster_drive::db::repository::upload_session_repo;
    use aster_drive::services::files::upload;
    use aws_sdk_s3::error::ProvideErrorMetadata;
    use testcontainers::{GenericImage, ImageExt, runners::AsyncRunner};

    let container = GenericImage::new("rustfs/rustfs", RUSTFS_TEST_IMAGE_TAG)
        .with_exposed_port(testcontainers::core::IntoContainerPort::tcp(9000))
        .with_env_var("RUSTFS_ACCESS_KEY", "rustfsadmin")
        .with_env_var("RUSTFS_SECRET_KEY", "rustfsadmin123")
        .start()
        .await
        .expect("failed to start rustfs container");

    let port = container.get_host_port_ipv4(9000).await.unwrap();
    let endpoint = format!("http://127.0.0.1:{port}");
    let bucket = "test-cancel-presigned-multipart";
    wait_for_s3_bucket(&endpoint, bucket).await;

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "cancelmpuser",
        "cancel-multipart@test.com",
        "password123",
    )
    .await
    .unwrap();
    let policy = create_s3_default_policy(
        &state,
        user.id,
        "Cancel Multipart RustFS Policy",
        &endpoint,
        bucket,
        r#"{"object_storage_upload_strategy":"presigned"}"#,
        TEST_CHUNK_SIZE as i64,
    )
    .await;

    let part1 = vec![b'A'; TEST_CHUNK_SIZE];
    let data_len = i64::try_from(TEST_CHUNK_SIZE + 11).unwrap();
    let init = upload::init_upload(
        &state,
        user.id,
        "cancel-multipart.bin",
        data_len,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        init.mode,
        aster_drive::types::UploadMode::PresignedMultipart
    );
    assert_eq!(init.total_chunks, Some(2));
    let upload_id = init.upload_id.clone().unwrap();

    let session = upload_session_repo::find_by_id(state.writer_db(), &upload_id)
        .await
        .unwrap();
    let temp_key = session
        .object_temp_key
        .clone()
        .expect("multipart session should store temp key");
    let multipart_id = session
        .object_multipart_id
        .clone()
        .expect("multipart session should store multipart id");
    let object_key =
        aster_drive::storage::object_key::join_key_prefix(&policy.base_path, &temp_key);

    let urls = upload::presign_parts(&state, &upload_id, user.id, vec![1])
        .await
        .unwrap();
    let part_url = urls.get(&1).expect("part 1 URL missing");
    let client = reqwest::Client::new();
    let response = client
        .put(part_url)
        .header(reqwest::header::CONTENT_LENGTH, part1.len())
        .body(part1)
        .send()
        .await
        .unwrap();
    assert!(
        response.status().is_success(),
        "multipart part upload failed: {}",
        response.status()
    );

    let progress = upload::get_progress(&state, &upload_id, user.id)
        .await
        .unwrap();
    assert_eq!(
        progress.status,
        aster_drive::types::UploadSessionStatus::Presigned
    );
    assert_eq!(progress.chunks_on_disk, vec![1]);

    let temp_dir =
        aster_forge_utils::paths::upload_temp_dir(&state.config.server.upload_temp_dir, &upload_id);
    tokio::fs::create_dir_all(&temp_dir).await.unwrap();
    tokio::fs::write(format!("{temp_dir}/chunk_0"), b"temp")
        .await
        .unwrap();

    upload::cancel_upload(&state, &upload_id, user.id)
        .await
        .unwrap();

    assert!(
        upload_session_repo::find_by_id(state.writer_db(), &upload_id)
            .await
            .is_err(),
        "canceled multipart upload session should be deleted immediately"
    );
    assert!(
        !std::path::Path::new(&temp_dir).exists(),
        "canceled multipart upload should clean local temp data"
    );

    let s3_client = s3_test_client(&endpoint);
    wait_for_s3_multipart_abort(&s3_client, bucket, &object_key, &multipart_id).await;

    let err = s3_client
        .list_parts()
        .bucket(bucket)
        .key(&object_key)
        .upload_id(&multipart_id)
        .send()
        .await
        .unwrap_err();
    let code = err.code();
    let http_status = err
        .raw_response()
        .map(|response| response.status().as_u16());
    assert!(
        matches!(code, Some("NoSuchUpload" | "NotFound")) || http_status == Some(404),
        "expected aborted multipart upload to be gone, got error: {err}"
    );
}

#[actix_web::test]
async fn test_cancel_upload_keeps_remote_session_when_object_cleanup_is_unavailable() {
    use aster_drive::db::repository::upload_session_repo;
    use aster_drive::services::files::upload;
    use aster_drive::types::UploadSessionStatus;

    let state = common::setup().await;
    let user =
        common::create_test_account(&state, "cancelrem", "cancel-remote@test.com", "password123")
            .await
            .unwrap();
    let remote_policy = create_dead_remote_policy(&state).await;
    let upload_id = new_test_upload_id();
    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            &upload_id,
            UploadSessionStatus::Uploading,
            chrono::Utc::now() + chrono::Duration::hours(1),
            UploadSessionKind::RemotePresignedSingle,
        )
        .chunks(2, 0)
        .policy(remote_policy.id)
        .object_upload(Some("upload/data/files/cancel-remote-temp"), None),
    )
    .await;

    let temp_dir =
        aster_forge_utils::paths::upload_temp_dir(&state.config.server.upload_temp_dir, &upload_id);
    tokio::fs::create_dir_all(&temp_dir).await.unwrap();
    tokio::fs::write(format!("{temp_dir}/chunk_0"), b"temp")
        .await
        .unwrap();

    let canceled_at = chrono::Utc::now();
    upload::cancel_upload(&state, &upload_id, user.id)
        .await
        .unwrap();
    let cancel_completed_at = chrono::Utc::now();

    let session = upload_session_repo::find_by_id(state.writer_db(), &upload_id)
        .await
        .unwrap();
    assert_eq!(session.status, UploadSessionStatus::Failed);
    assert!(
        session.expires_at > canceled_at,
        "cancel should defer cleanup instead of deleting the session when remote cleanup is blocked"
    );
    assert!(
        session.expires_at <= cancel_completed_at + chrono::Duration::seconds(20),
        "deferred cancel cleanup grace window should stay short"
    );
    assert!(
        !std::path::Path::new(&temp_dir).exists(),
        "cancel should still clean local temp data when remote cleanup is blocked"
    );
}

#[actix_web::test]
async fn test_upload_chunk_returns_session_expired_for_failed_multipart_session() {
    use aster_drive::db::repository::upload_session_part_repo;
    use aster_drive::services::files::upload;
    use aster_drive::types::UploadSessionStatus;

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "failedchunkuser",
        "failed-chunk@test.com",
        "password123",
    )
    .await
    .unwrap();
    let upload_id = new_test_upload_id();
    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            &upload_id,
            UploadSessionStatus::Failed,
            chrono::Utc::now() + chrono::Duration::hours(1),
            UploadSessionKind::ProviderRelayMultipart,
        )
        .chunks(2, 0)
        .object_upload(Some("files/temp-key"), Some("multipart-id")),
    )
    .await;

    let err = match upload::upload_chunk(&state, &upload_id, 0, user.id, &[b'x'; 5]).await {
        Ok(_) => panic!("expected failed session to reject late chunk"),
        Err(err) => err,
    };
    assert_eq!(err.code(), "E055");
    assert!(err.message().contains("canceled or failed"));
    assert!(
        upload_session_part_repo::find_by_upload_and_part(state.writer_db(), &upload_id, 1)
            .await
            .unwrap()
            .is_none(),
        "failed session should reject late multipart chunks before claiming part rows"
    );
}

/// S3 presigned upload 端到端测试（需要 testcontainers + rustfs）
#[tokio::test]
async fn test_presigned_upload_s3_e2e() {
    use aster_drive::services::files::upload;
    use testcontainers::{GenericImage, ImageExt, runners::AsyncRunner};

    // 启动 rustfs 容器
    let container = GenericImage::new("rustfs/rustfs", RUSTFS_TEST_IMAGE_TAG)
        .with_exposed_port(testcontainers::core::IntoContainerPort::tcp(9000))
        .with_env_var("RUSTFS_ACCESS_KEY", "rustfsadmin")
        .with_env_var("RUSTFS_SECRET_KEY", "rustfsadmin123")
        .start()
        .await
        .expect("failed to start rustfs container");

    let port = container.get_host_port_ipv4(9000).await.unwrap();
    let endpoint = format!("http://127.0.0.1:{port}");
    let bucket = "test-presigned";

    wait_for_s3_bucket(&endpoint, bucket).await;

    // 创建 state（内存 SQLite）
    let state = common::setup().await;

    let user = common::create_test_account(&state, "s3user", "s3@test.com", "pass1234")
        .await
        .unwrap();
    let s3_policy = create_s3_default_policy(
        &state,
        user.id,
        "Test S3 Presigned",
        &endpoint,
        bucket,
        r#"{"object_storage_upload_strategy":"presigned"}"#,
        5_242_880,
    )
    .await;

    // 1. init_upload → 应返回 presigned 模式
    let data = b"hello presigned world!";
    let init = upload::init_upload(&state, user.id, "hello.txt", data.len() as i64, None, None)
        .await
        .unwrap();
    assert_eq!(init.mode, aster_drive::types::UploadMode::Presigned);
    assert!(init.presigned_url.is_some());
    assert!(init.upload_id.is_some());

    let presigned_url = init.presigned_url.unwrap();
    let upload_id = init.upload_id.unwrap();

    // 2. PUT 到 presigned URL（模拟客户端直传）
    let client = reqwest::Client::new();
    let resp = client
        .put(&presigned_url)
        .header("Content-Type", "application/octet-stream")
        .body(data.to_vec())
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_success(),
        "S3 presigned PUT failed: {}",
        resp.status()
    );

    // 3. complete → 服务端 hash + dedup + 建记录
    let file = upload::complete_upload(&state, &upload_id, user.id, None)
        .await
        .unwrap();
    assert_eq!(file.name, "hello.txt");

    // 4. 验证文件可通过 driver 读取
    let policy = policy_repo::find_by_id(state.writer_db(), s3_policy.id)
        .await
        .unwrap();
    let driver = state.driver_registry.get_driver(&policy).unwrap();
    let completed_session =
        aster_drive::db::repository::upload_session_repo::find_by_id(state.writer_db(), &upload_id)
            .await
            .unwrap();
    let temp_key = completed_session.object_temp_key.unwrap();
    let blob =
        aster_drive::db::repository::file_repo::find_blob_by_id(state.writer_db(), file.blob_id)
            .await
            .unwrap();
    assert_ne!(
        blob.storage_path, temp_key,
        "completed presigned uploads must be copied away from the still-valid PUT key"
    );
    let got = driver.get(&blob.storage_path).await.unwrap();
    assert_eq!(got, data);
    assert!(
        !driver.exists(&temp_key).await.unwrap(),
        "presigned temp key should be removed after final copy"
    );

    // 5. 上传相同内容 → S3 presigned 不做 blob 去重（避免回拉 SHA256 抵消直传优势）
    //    每次上传产生独立 blob，各自 ref_count=1
    let init2 = upload::init_upload(&state, user.id, "hello2.txt", data.len() as i64, None, None)
        .await
        .unwrap();
    let url2 = init2.presigned_url.unwrap();
    let id2 = init2.upload_id.unwrap();
    client
        .put(&url2)
        .header("Content-Type", "application/octet-stream")
        .body(data.to_vec())
        .send()
        .await
        .unwrap();
    let file2 = upload::complete_upload(&state, &id2, user.id, None)
        .await
        .unwrap();
    assert_ne!(
        file2.blob_id, file.blob_id,
        "S3 presigned skips dedup — each upload creates its own blob"
    );

    let blob1 =
        aster_drive::db::repository::file_repo::find_blob_by_id(state.writer_db(), file.blob_id)
            .await
            .unwrap();
    let blob2 =
        aster_drive::db::repository::file_repo::find_blob_by_id(state.writer_db(), file2.blob_id)
            .await
            .unwrap();
    assert_eq!(blob1.ref_count, 1);
    assert_eq!(blob2.ref_count, 1);
}

/// S3 presigned URL 在策略强删后仍可能晚到 PUT；延迟任务必须清掉该临时对象。
#[tokio::test]
async fn test_force_delete_policy_cleans_late_s3_presigned_put_e2e() {
    use aster_drive::db::repository::{background_task_repo, policy_repo, upload_session_repo};
    use aster_drive::entities::background_task;
    use aster_drive::services::{files::folder, files::upload, storage_policy::policy, task};
    use aster_drive::types::{BackgroundTaskKind, BackgroundTaskStatus};
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
    use testcontainers::{GenericImage, ImageExt, runners::AsyncRunner};

    let container = GenericImage::new("rustfs/rustfs", RUSTFS_TEST_IMAGE_TAG)
        .with_exposed_port(testcontainers::core::IntoContainerPort::tcp(9000))
        .with_env_var("RUSTFS_ACCESS_KEY", "rustfsadmin")
        .with_env_var("RUSTFS_SECRET_KEY", "rustfsadmin123")
        .start()
        .await
        .expect("failed to start rustfs container");

    let port = container.get_host_port_ipv4(9000).await.unwrap();
    let endpoint = format!("http://127.0.0.1:{port}");
    let bucket = "test-force-delete-late-presigned";
    wait_for_s3_bucket(&endpoint, bucket).await;

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "latepresigneds3",
        "late-presigned-s3@test.com",
        "pass1234",
    )
    .await
    .unwrap();
    let policy = create_s3_policy(
        &state,
        "Late S3 Presigned Cleanup",
        &endpoint,
        bucket,
        r#"{"object_storage_upload_strategy":"presigned"}"#,
        5_242_880,
    )
    .await;
    let folder = folder::create(&state, user.id, "late-s3-presigned", None)
        .await
        .unwrap();
    common::bind_policy_to_folder(&state, folder.id, policy.id).await;

    let data = b"late s3 presigned write after force delete".to_vec();
    let init = upload::init_upload(
        &state,
        user.id,
        "late.bin",
        i64::try_from(data.len()).unwrap(),
        Some(folder.id),
        None,
    )
    .await
    .unwrap();
    assert_eq!(init.mode, aster_drive::types::UploadMode::Presigned);
    let upload_id = init.upload_id.unwrap();
    let presigned_url = init.presigned_url.unwrap();
    let session = upload_session_repo::find_by_id(state.writer_db(), &upload_id)
        .await
        .unwrap();
    let temp_key = session
        .object_temp_key
        .clone()
        .expect("presigned upload session should store temp key");
    let object_key = format!("uploads/{temp_key}");
    let s3_client = s3_test_client(&endpoint);
    assert!(
        !s3_object_exists(&s3_client, bucket, &object_key).await,
        "temp object should not exist before the late PUT"
    );

    policy::delete(&state, policy.id, true)
        .await
        .expect("force deleting policy with pending presigned session should succeed");
    assert!(
        policy_repo::find_by_id(state.writer_db(), policy.id)
            .await
            .is_err()
    );
    assert!(
        upload_session_repo::find_by_id(state.writer_db(), &upload_id)
            .await
            .is_err(),
        "force delete should remove the upload session before the old URL expires"
    );

    let response = reqwest::Client::new()
        .put(&presigned_url)
        .header("Content-Type", "application/octet-stream")
        .body(data)
        .send()
        .await
        .expect("late presigned PUT should send");
    assert!(
        response.status().is_success(),
        "old presigned S3 URL should still accept PUT until it expires: {}",
        response.status()
    );
    assert!(
        s3_object_exists(&s3_client, bucket, &object_key).await,
        "late PUT should create an orphan temp object after policy deletion"
    );

    let cleanup_task = background_task::Entity::find()
        .filter(background_task::Column::Kind.eq(BackgroundTaskKind::StoragePolicyTempCleanup))
        .one(state.writer_db())
        .await
        .unwrap()
        .expect("force delete should schedule delayed cleanup");
    assert_eq!(cleanup_task.status, BackgroundTaskStatus::Pending);
    let mut due_task: background_task::ActiveModel = cleanup_task.clone().into();
    due_task.next_run_at = Set(chrono::Utc::now() - chrono::Duration::seconds(1));
    due_task.update(state.writer_db()).await.unwrap();

    let stats = task::dispatch_due(&state).await.unwrap();
    assert_eq!(stats.claimed, 1);
    assert_eq!(stats.succeeded, 1);
    assert!(
        !s3_object_exists(&s3_client, bucket, &object_key).await,
        "delayed cleanup should delete the late S3 temp object"
    );
    let stored_task = background_task_repo::find_by_id(state.writer_db(), cleanup_task.id)
        .await
        .unwrap();
    assert_eq!(stored_task.status, BackgroundTaskStatus::Succeeded);
}

/// S3 presigned multipart 上传端到端测试：覆盖 presign_parts / progress / complete 排序分支
#[tokio::test]
async fn test_presigned_multipart_upload_s3_e2e() {
    use aster_drive::db::repository::{file_repo, policy_repo};
    use aster_drive::services::files::upload;
    use testcontainers::{GenericImage, ImageExt, runners::AsyncRunner};

    let container = GenericImage::new("rustfs/rustfs", RUSTFS_TEST_IMAGE_TAG)
        .with_exposed_port(testcontainers::core::IntoContainerPort::tcp(9000))
        .with_env_var("RUSTFS_ACCESS_KEY", "rustfsadmin")
        .with_env_var("RUSTFS_SECRET_KEY", "rustfsadmin123")
        .start()
        .await
        .expect("failed to start rustfs container");

    let port = container.get_host_port_ipv4(9000).await.unwrap();
    let endpoint = format!("http://127.0.0.1:{port}");
    let bucket = "test-presigned-multipart";

    wait_for_s3_bucket(&endpoint, bucket).await;

    let state = common::setup().await;

    let user = common::create_test_account(
        &state,
        "s3multipartuser",
        "s3multipart@test.com",
        "pass1234",
    )
    .await
    .unwrap();
    let s3_policy = create_s3_default_policy(
        &state,
        user.id,
        "Test S3 Multipart",
        &endpoint,
        bucket,
        r#"{"object_storage_upload_strategy":"presigned"}"#,
        5_242_880,
    )
    .await;

    let mut data = vec![b'A'; 5_242_880];
    data.extend_from_slice(b"multipart tail");
    let (part1, part2) = data.split_at(5_242_880);

    let init = upload::init_upload(
        &state,
        user.id,
        "multipart.bin",
        data.len() as i64,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        init.mode,
        aster_drive::types::UploadMode::PresignedMultipart
    );
    assert_eq!(init.total_chunks, Some(2));
    assert!(init.presigned_url.is_none());

    let upload_id = init.upload_id.unwrap();
    let urls = upload::presign_parts(&state, &upload_id, user.id, vec![2, 1])
        .await
        .unwrap();
    assert_eq!(urls.len(), 2);

    let client = reqwest::Client::new();
    let resp1 = client
        .put(urls.get(&1).unwrap())
        .header(reqwest::header::CONTENT_LENGTH, part1.len())
        .body(part1.to_vec())
        .send()
        .await
        .unwrap();
    assert!(resp1.status().is_success());
    let etag1 = resp1
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .expect("part 1 etag missing");

    let resp2 = client
        .put(urls.get(&2).unwrap())
        .header(reqwest::header::CONTENT_LENGTH, part2.len())
        .body(part2.to_vec())
        .send()
        .await
        .unwrap();
    assert!(resp2.status().is_success());
    let etag2 = resp2
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .expect("part 2 etag missing");

    let progress = upload::get_progress(&state, &upload_id, user.id)
        .await
        .unwrap();
    assert_eq!(
        progress.status,
        aster_drive::types::UploadSessionStatus::Presigned
    );
    assert_eq!(progress.total_chunks, 2);
    assert_eq!(progress.chunks_on_disk, vec![1, 2]);

    let file = upload::complete_upload(
        &state,
        &upload_id,
        user.id,
        Some(vec![(2, etag2), (1, etag1)]),
    )
    .await
    .unwrap();
    assert_eq!(file.name, "multipart.bin");

    let policy = policy_repo::find_by_id(state.writer_db(), s3_policy.id)
        .await
        .unwrap();
    let driver = state.driver_registry.get_driver(&policy).unwrap();
    let blob = file_repo::find_blob_by_id(state.writer_db(), file.blob_id)
        .await
        .unwrap();
    let stored = driver.get(&blob.storage_path).await.unwrap();
    assert_eq!(stored, data);
}

/// 任意 S3 策略下空文件都应创建独立 blob，而不是复用固定空文件 hash
#[tokio::test]
async fn test_create_empty_file_s3_no_dedup() {
    use aster_drive::db::repository::file_repo;
    use testcontainers::{GenericImage, ImageExt, runners::AsyncRunner};

    let container = GenericImage::new("rustfs/rustfs", RUSTFS_TEST_IMAGE_TAG)
        .with_exposed_port(testcontainers::core::IntoContainerPort::tcp(9000))
        .with_env_var("RUSTFS_ACCESS_KEY", "rustfsadmin")
        .with_env_var("RUSTFS_SECRET_KEY", "rustfsadmin123")
        .start()
        .await
        .expect("failed to start rustfs container");

    let port = container.get_host_port_ipv4(9000).await.unwrap();
    let endpoint = format!("http://127.0.0.1:{port}");
    let bucket = "test-s3-empty-file";

    wait_for_s3_bucket(&endpoint, bucket).await;

    let state = common::setup().await;
    let user = common::create_test_account(&state, "s3empty", "s3-empty@test.com", "pass1234")
        .await
        .unwrap();
    let policy = create_s3_default_policy(
        &state,
        user.id,
        "Test S3 Empty File",
        &endpoint,
        bucket,
        r#"{"object_storage_upload_strategy":"presigned"}"#,
        5_242_880,
    )
    .await;
    let login = local::login(&state, "s3empty", "pass1234", None, None)
        .await
        .unwrap();
    let login = common::expect_authenticated_login(login);
    common::seed_csrf_token(&login.access_token);

    let db = state.writer_db().clone();
    let driver_registry = state.driver_registry.clone();
    let app = create_test_app!(state);

    let req = test::TestRequest::post()
        .uri("/api/v1/files/new")
        .insert_header(("Cookie", common::access_cookie_header(&login.access_token)))
        .insert_header(common::csrf_header_for(&login.access_token))
        .insert_header(("Content-Type", "application/json"))
        .set_json(serde_json::json!({ "name": "empty.txt", "folder_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let file_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/files/new")
        .insert_header(("Cookie", common::access_cookie_header(&login.access_token)))
        .insert_header(common::csrf_header_for(&login.access_token))
        .insert_header(("Content-Type", "application/json"))
        .set_json(serde_json::json!({ "name": "empty.txt", "folder_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body2: Value = test::read_body_json(resp).await;
    let file_id2 = body2["data"]["id"].as_i64().unwrap();
    assert_ne!(body2["data"]["name"].as_str().unwrap(), "empty.txt");

    let file = file_repo::find_by_id(&db, file_id).await.unwrap();
    let file2 = file_repo::find_by_id(&db, file_id2).await.unwrap();
    let blob = file_repo::find_blob_by_id(&db, file.blob_id).await.unwrap();
    let blob2 = file_repo::find_blob_by_id(&db, file2.blob_id)
        .await
        .unwrap();

    assert_eq!(blob.ref_count, 1);
    assert_eq!(blob2.ref_count, 1);
    assert_ne!(blob.id, blob2.id);
    assert!(blob.hash.starts_with("s3-"));
    assert!(blob2.hash.starts_with("s3-"));
    assert_ne!(blob.hash, blob2.hash);
    assert!(blob.storage_path.starts_with("files/"));
    assert!(blob2.storage_path.starts_with("files/"));
    assert_ne!(blob.storage_path, blob2.storage_path);

    let driver = driver_registry.get_driver(&policy).unwrap();
    let stored = driver.get(&blob.storage_path).await.unwrap();
    let stored2 = driver.get(&blob2.storage_path).await.unwrap();
    assert!(stored.is_empty());
    assert!(stored2.is_empty());
}

/// S3 relay_stream 小文件直传：走 /files/upload，服务端直接中继到 S3，不做去重
#[tokio::test]
async fn test_relay_stream_direct_upload_s3_e2e() {
    use aster_drive::db::repository::file_repo;
    use aster_drive::services::files::upload;
    use testcontainers::{GenericImage, ImageExt, runners::AsyncRunner};

    let container = GenericImage::new("rustfs/rustfs", RUSTFS_TEST_IMAGE_TAG)
        .with_exposed_port(testcontainers::core::IntoContainerPort::tcp(9000))
        .with_env_var("RUSTFS_ACCESS_KEY", "rustfsadmin")
        .with_env_var("RUSTFS_SECRET_KEY", "rustfsadmin123")
        .start()
        .await
        .expect("failed to start rustfs container");

    let port = container.get_host_port_ipv4(9000).await.unwrap();
    let endpoint = format!("http://127.0.0.1:{port}");
    let bucket = "test-relay-direct";

    wait_for_s3_bucket(&endpoint, bucket).await;

    let state = common::setup().await;
    let user =
        common::create_test_account(&state, "relaydirect", "relay-direct@test.com", "pass1234")
            .await
            .unwrap();
    let policy = create_s3_default_policy(
        &state,
        user.id,
        "Test S3 Relay Direct",
        &endpoint,
        bucket,
        r#"{"object_storage_upload_strategy":"relay_stream"}"#,
        5_242_880,
    )
    .await;
    let login = local::login(&state, "relaydirect", "pass1234", None, None)
        .await
        .unwrap();
    let login = common::expect_authenticated_login(login);
    common::seed_csrf_token(&login.access_token);

    let data = b"hello relay stream!";
    let init = upload::init_upload(&state, user.id, "relay.txt", data.len() as i64, None, None)
        .await
        .unwrap();
    assert_eq!(init.mode, aster_drive::types::UploadMode::Direct);

    let db = state.writer_db().clone();
    let driver_registry = state.driver_registry.clone();
    let temp_roots = vec![
        state.config.server.temp_dir.clone(),
        state.config.server.upload_temp_dir.clone(),
    ];
    let temp_snapshot_before = snapshot_temp_roots(&temp_roots).unwrap();
    let app = create_test_app!(state);

    let (boundary, payload) = build_multipart_payload("relay.txt", data);
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/files/upload?declared_size={}",
            data.len()
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
    let body: Value = test::read_body_json(resp).await;
    let file_id = body["data"]["id"].as_i64().unwrap();

    let (boundary2, payload2) = build_multipart_payload("relay-copy.txt", data);
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/files/upload?declared_size={}",
            data.len()
        ))
        .insert_header(("Cookie", common::access_cookie_header(&login.access_token)))
        .insert_header(common::csrf_header_for(&login.access_token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary2}"),
        ))
        .set_payload(payload2)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let file_id2 = body["data"]["id"].as_i64().unwrap();
    let temp_snapshot_after = snapshot_temp_roots(&temp_roots).unwrap();

    assert_eq!(
        temp_snapshot_after, temp_snapshot_before,
        "relay_stream direct upload should not create local temp files or upload temp dirs"
    );

    let file = file_repo::find_by_id(&db, file_id).await.unwrap();
    let file2 = file_repo::find_by_id(&db, file_id2).await.unwrap();
    let blob = file_repo::find_blob_by_id(&db, file.blob_id).await.unwrap();
    let blob2 = file_repo::find_blob_by_id(&db, file2.blob_id)
        .await
        .unwrap();

    assert!(blob.hash.starts_with("s3-"));
    assert!(blob2.hash.starts_with("s3-"));
    assert_eq!(blob.ref_count, 1);
    assert_eq!(blob2.ref_count, 1);
    assert_ne!(blob.id, blob2.id);
    assert_ne!(blob.hash, blob2.hash);
    assert_ne!(blob.storage_path, blob2.storage_path);

    let driver = driver_registry.get_driver(&policy).unwrap();
    let stored = driver.get(&blob.storage_path).await.unwrap();
    let stored2 = driver.get(&blob2.storage_path).await.unwrap();
    assert_eq!(stored, data);
    assert_eq!(stored2, data);
}

/// S3 relay_stream 直传：文件大小刚好等于 chunk_size 时仍应走 direct upload。
#[tokio::test]
async fn test_relay_stream_direct_upload_s3_exact_part_size_e2e() {
    use aster_drive::db::repository::file_repo;
    use aster_drive::entities::{upload_session, upload_session_part};
    use aster_drive::services::files::upload;
    use sea_orm::{
        ColumnTrait, EntityTrait, JoinType, PaginatorTrait, QueryFilter, QuerySelect, RelationTrait,
    };
    use testcontainers::{GenericImage, ImageExt, runners::AsyncRunner};

    let container = GenericImage::new("rustfs/rustfs", RUSTFS_TEST_IMAGE_TAG)
        .with_exposed_port(testcontainers::core::IntoContainerPort::tcp(9000))
        .with_env_var("RUSTFS_ACCESS_KEY", "rustfsadmin")
        .with_env_var("RUSTFS_SECRET_KEY", "rustfsadmin123")
        .start()
        .await
        .expect("failed to start rustfs container");

    let port = container.get_host_port_ipv4(9000).await.unwrap();
    let endpoint = format!("http://127.0.0.1:{port}");
    let bucket = "test-relay-direct-exact-part";

    wait_for_s3_bucket(&endpoint, bucket).await;

    let state = common::setup().await;
    let user =
        common::create_test_account(&state, "relayexact", "relay-exact@test.com", "pass1234")
            .await
            .unwrap();
    let policy = create_s3_default_policy(
        &state,
        user.id,
        "Test S3 Relay Direct Exact Part",
        &endpoint,
        bucket,
        r#"{"object_storage_upload_strategy":"relay_stream"}"#,
        5_242_880,
    )
    .await;
    let login = local::login(&state, "relayexact", "pass1234", None, None)
        .await
        .unwrap();
    let login = common::expect_authenticated_login(login);
    common::seed_csrf_token(&login.access_token);

    let db = state.writer_db().clone();
    let sessions_before = upload_session::Entity::find()
        .filter(upload_session::Column::UserId.eq(user.id))
        .count(&db)
        .await
        .unwrap();
    let parts_before = upload_session_part::Entity::find()
        .join(
            JoinType::InnerJoin,
            upload_session_part::Relation::UploadSession.def(),
        )
        .filter(upload_session::Column::UserId.eq(user.id))
        .count(&db)
        .await
        .unwrap();

    let data = vec![b'Z'; 5_242_880];
    let init = upload::init_upload(
        &state,
        user.id,
        "relay-exact.bin",
        data.len() as i64,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(init.mode, aster_drive::types::UploadMode::Direct);
    assert_eq!(
        upload_session::Entity::find()
            .filter(upload_session::Column::UserId.eq(user.id))
            .count(&db)
            .await
            .unwrap(),
        sessions_before,
        "direct init should not create upload sessions at the exact chunk boundary"
    );
    assert_eq!(
        upload_session_part::Entity::find()
            .join(
                JoinType::InnerJoin,
                upload_session_part::Relation::UploadSession.def(),
            )
            .filter(upload_session::Column::UserId.eq(user.id))
            .count(&db)
            .await
            .unwrap(),
        parts_before,
        "direct init should not create multipart part rows at the exact chunk boundary"
    );
    let driver = state.driver_registry.get_driver(&policy).unwrap();
    let app = create_test_app!(state);

    let (boundary, payload) = build_multipart_payload("relay-exact.bin", &data);
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/files/upload?declared_size={}",
            data.len()
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
    let body: Value = test::read_body_json(resp).await;
    let file_id = body["data"]["id"].as_i64().unwrap();

    let file = file_repo::find_by_id(&db, file_id).await.unwrap();
    let blob = file_repo::find_blob_by_id(&db, file.blob_id).await.unwrap();
    let stored = driver.get(&blob.storage_path).await.unwrap();
    assert_eq!(stored, data);
    assert_eq!(
        upload_session::Entity::find()
            .filter(upload_session::Column::UserId.eq(user.id))
            .count(&db)
            .await
            .unwrap(),
        sessions_before,
        "direct /files/upload should not create upload sessions at the exact chunk boundary"
    );
    assert_eq!(
        upload_session_part::Entity::find()
            .join(
                JoinType::InnerJoin,
                upload_session_part::Relation::UploadSession.def(),
            )
            .filter(upload_session::Column::UserId.eq(user.id))
            .count(&db)
            .await
            .unwrap(),
        parts_before,
        "direct /files/upload should not create multipart part rows at the exact chunk boundary"
    );
}

/// S3 relay_stream 大文件分片：服务端直接把 chunk 作为 S3 part，中途不落 data/.uploads
#[tokio::test]
async fn test_relay_stream_chunked_upload_s3_e2e() {
    use aster_drive::db::repository::{file_repo, upload_session_part_repo, upload_session_repo};
    use aster_drive::services::files::upload;
    use testcontainers::{GenericImage, ImageExt, runners::AsyncRunner};

    let container = GenericImage::new("rustfs/rustfs", RUSTFS_TEST_IMAGE_TAG)
        .with_exposed_port(testcontainers::core::IntoContainerPort::tcp(9000))
        .with_env_var("RUSTFS_ACCESS_KEY", "rustfsadmin")
        .with_env_var("RUSTFS_SECRET_KEY", "rustfsadmin123")
        .start()
        .await
        .expect("failed to start rustfs container");

    let port = container.get_host_port_ipv4(9000).await.unwrap();
    let endpoint = format!("http://127.0.0.1:{port}");
    let bucket = "test-relay-chunked";

    wait_for_s3_bucket(&endpoint, bucket).await;

    let state = common::setup().await;
    let user =
        common::create_test_account(&state, "relaychunked", "relay-chunked@test.com", "pass1234")
            .await
            .unwrap();
    let policy = create_s3_default_policy(
        &state,
        user.id,
        "Test S3 Relay Chunked",
        &endpoint,
        bucket,
        r#"{"object_storage_upload_strategy":"relay_stream"}"#,
        1_048_576,
    )
    .await;
    let login = local::login(&state, "relaychunked", "pass1234", None, None)
        .await
        .unwrap();
    let login = common::expect_authenticated_login(login);
    common::seed_csrf_token(&login.access_token);

    let part1 = vec![b'A'; 5_242_880];
    let part2 = b"relay-stream-tail".to_vec();
    let mut data = part1.clone();
    data.extend_from_slice(&part2);

    let init = upload::init_upload(
        &state,
        user.id,
        "relay-multipart.bin",
        data.len() as i64,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(init.mode, aster_drive::types::UploadMode::Chunked);
    assert_eq!(init.chunk_size, Some(5_242_880));
    assert_eq!(init.total_chunks, Some(2));

    let upload_id = init.upload_id.unwrap();
    let app = create_test_app!(state.clone());

    let oversized_init = upload::init_upload(
        &state,
        user.id,
        "relay-oversized-multipart.bin",
        (part1.len() + 1) as i64,
        None,
        None,
    )
    .await
    .unwrap();
    let oversized_upload_id = oversized_init.upload_id.unwrap();
    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/upload/{oversized_upload_id}/0"))
        .insert_header(("Cookie", common::access_cookie_header(&login.access_token)))
        .insert_header(common::csrf_header_for(&login.access_token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(vec![b'Z'; part1.len() + 1])
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::PAYLOAD_TOO_LARGE
    );
    let body: Value = test::read_body_json(resp).await;
    assert_upload_error_contract(&body, "upload.chunk_too_large");
    assert!(
        upload_session_part_repo::list_by_upload(state.writer_db(), &oversized_upload_id)
            .await
            .unwrap()
            .is_empty(),
        "oversized relay chunk must release the claimed part row"
    );
    let oversized_progress = upload::get_progress(&state, &oversized_upload_id, user.id)
        .await
        .unwrap();
    assert!(oversized_progress.chunks_on_disk.is_empty());
    let oversized_relay_temp_dir = aster_forge_utils::paths::upload_temp_dir(
        &state.config.server.upload_temp_dir,
        &oversized_upload_id,
    );
    assert!(
        !std::path::Path::new(&oversized_relay_temp_dir).exists(),
        "oversized relay_stream chunks should not create local upload temp dirs"
    );

    let relay_temp_dir =
        aster_forge_utils::paths::upload_temp_dir(&state.config.server.upload_temp_dir, &upload_id);
    assert!(
        !std::path::Path::new(&relay_temp_dir).exists(),
        "relay_stream should not create local upload temp dir"
    );
    assert!(
        upload_session_part_repo::list_by_upload(state.writer_db(), &upload_id)
            .await
            .unwrap()
            .is_empty()
    );

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/upload/{upload_id}/0"))
        .insert_header(("Cookie", common::access_cookie_header(&login.access_token)))
        .insert_header(common::csrf_header_for(&login.access_token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(part1.clone())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["received_count"], 1);

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/upload/{upload_id}/0"))
        .insert_header(("Cookie", common::access_cookie_header(&login.access_token)))
        .insert_header(common::csrf_header_for(&login.access_token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(part1.clone())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let duplicate: Value = test::read_body_json(resp).await;
    assert_eq!(
        duplicate["data"]["received_count"], 1,
        "duplicate relay part upload should be idempotent"
    );

    let progress = upload::get_progress(&state, &upload_id, user.id)
        .await
        .unwrap();
    assert_eq!(progress.chunks_on_disk, vec![0]);

    let parts = upload_session_part_repo::list_by_upload(state.writer_db(), &upload_id)
        .await
        .unwrap();
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].part_number, 1);
    assert_eq!(parts[0].size, part1.len() as i64);
    assert!(!parts[0].etag.is_empty());
    assert!(
        !std::path::Path::new(&relay_temp_dir).exists(),
        "relay_stream should still avoid local temp dirs after part upload"
    );

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/upload/{upload_id}/1"))
        .insert_header(("Cookie", common::access_cookie_header(&login.access_token)))
        .insert_header(common::csrf_header_for(&login.access_token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(part2.clone())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["received_count"], 2);

    let progress = upload::get_progress(&state, &upload_id, user.id)
        .await
        .unwrap();
    assert_eq!(progress.chunks_on_disk, vec![0, 1]);

    let parts = upload_session_part_repo::list_by_upload(state.writer_db(), &upload_id)
        .await
        .unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].part_number, 1);
    assert_eq!(parts[1].part_number, 2);
    assert_eq!(parts[0].size, part1.len() as i64);
    assert_eq!(parts[1].size, part2.len() as i64);
    assert!(parts.iter().all(|part| !part.etag.is_empty()));

    let session = upload_session_repo::find_by_id(state.writer_db(), &upload_id)
        .await
        .unwrap();
    assert_eq!(session.received_count, 2);
    assert_eq!(
        session.status,
        aster_drive::types::UploadSessionStatus::Uploading
    );

    let file = upload::complete_upload(&state, &upload_id, user.id, None)
        .await
        .unwrap();
    assert_eq!(file.name, "relay-multipart.bin");

    let session = upload_session_repo::find_by_id(state.writer_db(), &upload_id)
        .await
        .unwrap();
    assert_eq!(
        session.status,
        aster_drive::types::UploadSessionStatus::Completed
    );
    assert_eq!(session.file_id, Some(file.id));

    let blob = file_repo::find_blob_by_id(state.writer_db(), file.blob_id)
        .await
        .unwrap();
    assert_eq!(blob.hash, format!("s3-{upload_id}"));
    assert_eq!(blob.storage_path, format!("files/{upload_id}"));
    assert_eq!(blob.ref_count, 1);

    let driver = state.driver_registry.get_driver(&policy).unwrap();
    let stored = driver.get(&blob.storage_path).await.unwrap();
    assert_eq!(stored, data);

    let completed_progress = upload::get_progress(&state, &upload_id, user.id)
        .await
        .unwrap();
    assert_eq!(
        completed_progress.status,
        aster_drive::types::UploadSessionStatus::Completed
    );
    assert_eq!(completed_progress.chunks_on_disk, vec![0, 1]);

    let parts = upload_session_part_repo::list_by_upload(state.writer_db(), &upload_id)
        .await
        .unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].part_number, 1);
    assert_eq!(parts[1].part_number, 2);
    assert!(
        !std::path::Path::new(&relay_temp_dir).exists(),
        "relay_stream multipart should never use local chunk temp dir"
    );
}

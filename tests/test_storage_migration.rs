//! Storage policy blob migration integration tests.

#[macro_use]
mod common;

use actix_web::test;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde_json::Value;
use testcontainers::{GenericImage, ImageExt, runners::AsyncRunner};

use aster_drive::db::repository::{
    background_task_repo, file_repo, policy_repo, storage_migration_checkpoint_repo,
};
use aster_drive::entities::{file, file_blob, file_version, storage_policy};
use aster_drive::runtime::PrimaryAppState;
use aster_drive::services::task_service;
use aster_drive::types::{
    BackgroundTaskStatus, DriverType, FileCategory, StoredStoragePolicyAllowedTypes,
    StoredStoragePolicyOptions,
};

const RUSTFS_TEST_IMAGE_TAG: &str = "1.0.0-alpha.90";

struct RustFsTestContext {
    _container: testcontainers::ContainerAsync<GenericImage>,
    endpoint: String,
    bucket: String,
}

async fn start_rustfs_context(bucket_prefix: &str) -> RustFsTestContext {
    let container = GenericImage::new("rustfs/rustfs", RUSTFS_TEST_IMAGE_TAG)
        .with_exposed_port(testcontainers::core::IntoContainerPort::tcp(9000))
        .with_env_var("RUSTFS_ACCESS_KEY", "rustfsadmin")
        .with_env_var("RUSTFS_SECRET_KEY", "rustfsadmin123")
        .start()
        .await
        .expect("failed to start rustfs container");

    let port = container
        .get_host_port_ipv4(9000)
        .await
        .expect("rustfs port should resolve");
    let endpoint = format!("http://127.0.0.1:{port}");
    let bucket = format!("{bucket_prefix}-{}", uuid::Uuid::new_v4().simple());
    wait_for_s3_bucket(&endpoint, &bucket).await;

    RustFsTestContext {
        _container: container,
        endpoint,
        bucket,
    }
}

async fn create_local_policy(state: &PrimaryAppState, name: &str) -> storage_policy::Model {
    let now = Utc::now();
    let base_path = format!("{}/policy-{name}", state.config.server.temp_dir);
    tokio::fs::create_dir_all(&base_path)
        .await
        .expect("policy test dir should be created");
    policy_repo::create(
        state.writer_db(),
        storage_policy::ActiveModel {
            name: Set(name.to_string()),
            driver_type: Set(DriverType::Local),
            endpoint: Set(String::new()),
            bucket: Set(String::new()),
            access_key: Set(String::new()),
            secret_key: Set(String::new()),
            base_path: Set(base_path),
            max_file_size: Set(0),
            allowed_types: Set(StoredStoragePolicyAllowedTypes::empty()),
            options: Set(StoredStoragePolicyOptions::empty()),
            is_default: Set(false),
            chunk_size: Set(5_242_880),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .expect("local policy should insert")
}

async fn create_s3_policy(
    state: &PrimaryAppState,
    name: &str,
    endpoint: &str,
    bucket: &str,
) -> storage_policy::Model {
    let now = Utc::now();
    let policy = policy_repo::create(
        state.writer_db(),
        storage_policy::ActiveModel {
            name: Set(name.to_string()),
            driver_type: Set(DriverType::S3),
            endpoint: Set(endpoint.to_string()),
            bucket: Set(bucket.to_string()),
            access_key: Set("rustfsadmin".to_string()),
            secret_key: Set("rustfsadmin123".to_string()),
            base_path: Set(format!("migration-{name}")),
            max_file_size: Set(0),
            allowed_types: Set(StoredStoragePolicyAllowedTypes::empty()),
            options: Set(StoredStoragePolicyOptions::from(
                r#"{"s3_upload_strategy":"relay_stream"}"#.to_string(),
            )),
            is_default: Set(false),
            chunk_size: Set(5_242_880),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .expect("s3 policy should insert");
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
                Err(_) => last_err = Some("create_bucket attempt timed out".to_string()),
            }
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

async fn read_s3_object(endpoint: &str, bucket: &str, key: &str) -> Vec<u8> {
    let object = s3_test_client(endpoint)
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .expect("s3 object should exist");
    object
        .body
        .collect()
        .await
        .expect("s3 object body should read")
        .into_bytes()
        .to_vec()
}

async fn s3_object_exists(endpoint: &str, bucket: &str, key: &str) -> bool {
    s3_test_client(endpoint)
        .head_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .is_ok()
}

fn policy_object_key(policy: &storage_policy::Model, storage_path: &str) -> String {
    format!("{}/{}", policy.base_path.trim_matches('/'), storage_path)
}

async fn create_blob_with_object(
    state: &PrimaryAppState,
    policy: &storage_policy::Model,
    bytes: &[u8],
    ref_count: i32,
) -> file_blob::Model {
    let hash = aster_drive::utils::hash::sha256_hex(bytes);
    let storage_path = aster_drive::utils::storage_path_from_blob_key(&hash);
    let full_path = std::path::Path::new(&policy.base_path).join(&storage_path);
    tokio::fs::create_dir_all(full_path.parent().expect("blob path should have parent"))
        .await
        .expect("blob parent should be created");
    tokio::fs::write(&full_path, bytes)
        .await
        .expect("blob object should be written");
    let now = Utc::now();
    file_blob::ActiveModel {
        hash: Set(hash),
        size: Set(i64::try_from(bytes.len()).expect("test bytes len should fit i64")),
        policy_id: Set(policy.id),
        storage_path: Set(storage_path),
        thumbnail_path: Set(Some("old-thumb".to_string())),
        thumbnail_processor: Set(Some("old-processor".to_string())),
        thumbnail_version: Set(Some("old-version".to_string())),
        ref_count: Set(ref_count),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("blob row should insert")
}

async fn create_opaque_blob_with_object(
    state: &PrimaryAppState,
    policy: &storage_policy::Model,
    blob_key: &str,
    bytes: &[u8],
    ref_count: i32,
) -> file_blob::Model {
    let storage_path = aster_drive::utils::storage_path_from_blob_key(blob_key);
    let full_path = std::path::Path::new(&policy.base_path).join(&storage_path);
    tokio::fs::create_dir_all(full_path.parent().expect("blob path should have parent"))
        .await
        .expect("blob parent should be created");
    tokio::fs::write(&full_path, bytes)
        .await
        .expect("blob object should be written");
    let now = Utc::now();
    file_blob::ActiveModel {
        hash: Set(blob_key.to_string()),
        size: Set(i64::try_from(bytes.len()).expect("test bytes len should fit i64")),
        policy_id: Set(policy.id),
        storage_path: Set(storage_path),
        thumbnail_path: Set(None),
        thumbnail_processor: Set(None),
        thumbnail_version: Set(None),
        ref_count: Set(ref_count),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("opaque blob row should insert")
}

async fn create_file_for_blob(state: &PrimaryAppState, blob_id: i64, name: &str) -> file::Model {
    let now = Utc::now();
    file::ActiveModel {
        name: Set(name.to_string()),
        folder_id: Set(None),
        team_id: Set(None),
        blob_id: Set(blob_id),
        size: Set(1),
        owner_user_id: Set(None),
        created_by_user_id: Set(None),
        created_by_username: Set("tester".to_string()),
        mime_type: Set("text/plain".to_string()),
        extension: Set("txt".to_string()),
        compound_extension: Set(None),
        file_category: Set(FileCategory::Document),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        is_locked: Set(false),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("file row should insert")
}

async fn create_version_for_blob(
    state: &PrimaryAppState,
    file_id: i64,
    blob_id: i64,
    version: i32,
) -> file_version::Model {
    file_version::ActiveModel {
        file_id: Set(file_id),
        blob_id: Set(blob_id),
        version: Set(version),
        size: Set(1),
        created_at: Set(Utc::now()),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("file version row should insert")
}

async fn create_migration_task_via_api(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    token: &str,
    source_policy_id: i64,
    target_policy_id: i64,
    delete_source_after_success: bool,
) -> Value {
    let (_, body) = create_migration_task_via_api_with_status(
        app,
        token,
        source_policy_id,
        target_policy_id,
        delete_source_after_success,
    )
    .await;
    body
}

async fn create_migration_task_via_api_with_status(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    token: &str,
    source_policy_id: i64,
    target_policy_id: i64,
    delete_source_after_success: bool,
) -> (actix_web::http::StatusCode, Value) {
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/storage-migrations")
        .insert_header(("Cookie", common::access_cookie_header(token)))
        .insert_header(common::csrf_header_for(token))
        .set_json(serde_json::json!({
            "source_policy_id": source_policy_id,
            "target_policy_id": target_policy_id,
            "delete_source_after_success": delete_source_after_success,
        }))
        .to_request();
    let resp = test::call_service(app, req).await;
    let status = resp.status();
    let body = test::read_body_json(resp).await;
    (status, body)
}

async fn dry_run_migration_via_api(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    token: &str,
    source_policy_id: i64,
    target_policy_id: i64,
) -> (actix_web::http::StatusCode, Value) {
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/storage-migrations/dry-run")
        .insert_header(("Cookie", common::access_cookie_header(token)))
        .insert_header(common::csrf_header_for(token))
        .set_json(serde_json::json!({
            "source_policy_id": source_policy_id,
            "target_policy_id": target_policy_id,
        }))
        .to_request();
    let resp = test::call_service(app, req).await;
    let status = resp.status();
    let body = test::read_body_json(resp).await;
    (status, body)
}

fn assert_conflicting_storage_migration_response(
    status: actix_web::http::StatusCode,
    body: &Value,
) {
    assert_eq!(status, actix_web::http::StatusCode::BAD_REQUEST);
    assert_ne!(body["code"], 0);
    assert!(
        body["msg"]
            .as_str()
            .expect("error message should exist")
            .contains("conflicting active storage policy migration")
    );
}

async fn set_background_task_status(
    state: &PrimaryAppState,
    task_id: i64,
    status: BackgroundTaskStatus,
) {
    let mut task_update: aster_drive::entities::background_task::ActiveModel =
        background_task_repo::find_by_id(state.writer_db(), task_id)
            .await
            .expect("task should exist")
            .into();
    task_update.status = Set(status);
    if status.is_terminal() {
        task_update.finished_at = Set(Some(Utc::now()));
    }
    task_update
        .update(state.writer_db())
        .await
        .expect("task status should update");
}

#[actix_web::test]
async fn test_storage_migration_api_creates_task_and_checkpoint() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-create").await;
    let target = create_local_policy(&state, "target-create").await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;

    assert_eq!(body["code"], 0);
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    assert_eq!(body["data"]["kind"], "storage_policy_migration");
    assert_eq!(
        body["data"]["payload"]["source_policy_id"].as_i64(),
        Some(source.id)
    );
    assert_eq!(
        body["data"]["payload"]["target_policy_id"].as_i64(),
        Some(target.id)
    );

    let checkpoint = storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
        .await
        .expect("checkpoint should exist");
    assert_eq!(checkpoint.source_policy_id, source.id);
    assert_eq!(checkpoint.target_policy_id, target.id);
    assert_eq!(checkpoint.last_processed_blob_id, 0);
}

#[actix_web::test]
async fn test_storage_migration_dry_run_reports_preflight_summary() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-dry-run").await;
    let target = create_local_policy(&state, "target-dry-run").await;
    let source_blob = create_blob_with_object(&state, &source, b"dedup-me", 1).await;
    create_opaque_blob_with_object(&state, &source, "opaque-dry-run-key", b"opaque", 1).await;
    create_blob_with_object(&state, &target, b"dedup-me", 1).await;

    let (status, body) = dry_run_migration_via_api(&app, &token, source.id, target.id).await;

    assert_eq!(status, actix_web::http::StatusCode::OK);
    assert_eq!(body["code"], 0);
    let data = &body["data"];
    assert_eq!(data["source_policy_id"], source.id);
    assert_eq!(data["target_policy_id"], target.id);
    assert_eq!(data["source_blob_count"], 2);
    assert_eq!(
        data["source_total_bytes"].as_i64(),
        Some(source_blob.size + 6)
    );
    assert_eq!(data["content_sha256_blob_count"], 1);
    assert_eq!(data["opaque_blob_count"], 1);
    assert_eq!(data["target_matching_blob_count"], 1);
    assert_eq!(data["estimated_copy_blob_count"], 1);
    assert_eq!(data["target_supports_stream_upload"], true);
    assert_eq!(data["target_connection_ok"], true);
    assert_eq!(data["target_capacity_check"], "sufficient");
    assert_eq!(data["target_capacity"]["status"], "supported");
    assert_eq!(data["target_capacity"]["source"], "local_filesystem");
    assert!(
        data["target_capacity"]["available_bytes"]
            .as_i64()
            .expect("available bytes should be present")
            >= 6
    );
    assert_eq!(data["delete_source_after_success_supported"], false);
    assert_eq!(data["can_start"], true);
    assert_eq!(data["warnings"].as_array().unwrap(), &Vec::<Value>::new());
}

#[actix_web::test]
async fn test_storage_migration_capacity_preflight_uses_estimated_copy_bytes() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-capacity-estimate").await;
    let target = create_local_policy(&state, "target-capacity-estimate").await;

    let shared = create_blob_with_object(&state, &source, b"already-present-large", 1).await;
    let missing = create_blob_with_object(&state, &source, b"x", 1).await;
    create_blob_with_object(&state, &target, b"already-present-large", 1).await;

    let missing_summary = file_repo::summarize_missing_blobs_between_policies(
        state.writer_db(),
        source.id,
        target.id,
    )
    .await
    .expect("missing blob summary should load");
    assert_eq!(missing_summary.count, 1);
    assert_eq!(missing_summary.total_size, missing.size);

    let (status, body) = dry_run_migration_via_api(&app, &token, source.id, target.id).await;

    assert_eq!(status, actix_web::http::StatusCode::OK);
    assert_eq!(body["code"], 0);
    let data = &body["data"];
    assert_eq!(data["source_blob_count"], 2);
    assert_eq!(
        data["source_total_bytes"].as_i64(),
        Some(shared.size + missing.size)
    );
    assert_eq!(data["target_matching_blob_count"], 1);
    assert_eq!(data["estimated_copy_blob_count"], 1);
    assert_eq!(data["target_capacity_check"], "sufficient");
    assert!(
        data["target_capacity"]["available_bytes"]
            .as_i64()
            .expect("available bytes should be present")
            >= missing.size
    );
}

#[actix_web::test]
async fn test_storage_migration_dry_run_does_not_match_opaque_blob_keys() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-opaque-dry-run").await;
    let target = create_local_policy(&state, "target-opaque-dry-run").await;
    let source_blob =
        create_opaque_blob_with_object(&state, &source, "opaque-shared-key", b"same", 1).await;
    create_opaque_blob_with_object(&state, &target, "opaque-shared-key", b"same", 1).await;

    let missing_summary = file_repo::summarize_missing_blobs_between_policies(
        state.writer_db(),
        source.id,
        target.id,
    )
    .await
    .expect("missing blob summary should load");
    assert_eq!(missing_summary.count, 1);
    assert_eq!(missing_summary.total_size, source_blob.size);

    let (status, body) = dry_run_migration_via_api(&app, &token, source.id, target.id).await;

    assert_eq!(status, actix_web::http::StatusCode::OK);
    let data = &body["data"];
    assert_eq!(data["opaque_blob_count"], 1);
    assert_eq!(data["opaque_key_conflict_count"], 1);
    assert_eq!(data["target_matching_blob_count"], 0);
    assert_eq!(data["estimated_copy_blob_count"], 1);
}

#[actix_web::test]
async fn test_storage_policy_capacity_api_reports_local_filesystem_capacity() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let policy = create_local_policy(&state, "capacity-api").await;
    let blob = create_blob_with_object(&state, &policy, b"capacity-api-blob", 1).await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/admin/policies/{}/capacity", policy.id))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;

    assert_eq!(body["code"], 0);
    let data = &body["data"];
    assert_eq!(data["policy_id"], policy.id);
    assert_eq!(data["driver_type"], "local");
    assert_eq!(data["blob_count"], 1);
    assert_eq!(data["blob_total_bytes"], blob.size);
    assert_eq!(data["capacity"]["status"], "supported");
    assert_eq!(data["capacity"]["source"], "local_filesystem");
    assert!(
        data["capacity"]["total_bytes"]
            .as_i64()
            .expect("total bytes should be present")
            > 0
    );
    assert!(
        data["capacity"]["available_bytes"]
            .as_i64()
            .expect("available bytes should be present")
            > 0
    );
}

#[actix_web::test]
async fn test_storage_migration_dry_run_requires_admin() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    admin_create_user!(
        app,
        admin_token,
        "migrationdryuser",
        "migration-dry-user@example.com",
        "password123"
    );
    let (plain_token, _) = login_user!(app, "migrationdryuser", "password123");
    let source = create_local_policy(&state, "source-dry-auth").await;
    let target = create_local_policy(&state, "target-dry-auth").await;

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/storage-migrations/dry-run")
        .insert_header(("Cookie", common::access_cookie_header(&plain_token)))
        .insert_header(common::csrf_header_for(&plain_token))
        .set_json(serde_json::json!({
            "source_policy_id": source.id,
            "target_policy_id": target.id,
        }))
        .to_request();
    let err = test::try_call_service(&app, req).await.unwrap_err();
    assert_eq!(
        err.error_response().status(),
        actix_web::http::StatusCode::FORBIDDEN
    );
}

#[actix_web::test]
async fn test_storage_migration_api_rejects_source_deletion_flag() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-delete-flag").await;
    let target = create_local_policy(&state, "target-delete-flag").await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, true).await;

    assert_ne!(body["code"], 0);
    assert!(
        body["msg"]
            .as_str()
            .expect("error message should exist")
            .contains("delete_source_after_success")
    );
}

#[actix_web::test]
async fn test_storage_migration_resume_reuses_checkpoint_after_failed_task() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-resume").await;
    let target = create_local_policy(&state, "target-resume").await;
    let first = create_blob_with_object(&state, &source, b"first", 1).await;
    let second = create_blob_with_object(&state, &source, b"second", 1).await;
    create_file_for_blob(&state, first.id, "first.txt").await;
    create_file_for_blob(&state, second.id, "second.txt").await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    let second_path = std::path::Path::new(&source.base_path).join(&second.storage_path);
    tokio::fs::write(&second_path, b"bad!!!")
        .await
        .expect("second source object should be tampered before migration starts");

    let stats = task_service::drain(&state)
        .await
        .expect("first migration attempt should drain");
    assert_eq!(stats.failed, 1);
    let checkpoint = storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
        .await
        .expect("checkpoint should exist");
    assert_eq!(checkpoint.last_processed_blob_id, first.id);
    assert_eq!(checkpoint.migrated_blobs, 1);

    tokio::fs::write(&second_path, b"second")
        .await
        .expect("second source object should be restored before resume");
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/admin/storage-migrations/{task_id}/resume"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "pending");

    let stats = task_service::drain(&state)
        .await
        .expect("resumed migration task should drain");
    assert_eq!(stats.succeeded, 1);
    let final_checkpoint =
        storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
            .await
            .expect("checkpoint should exist");
    assert_eq!(final_checkpoint.stage, "complete");
    assert_eq!(final_checkpoint.last_processed_blob_id, second.id);
    assert_eq!(final_checkpoint.migrated_blobs, 2);

    let migrated_second = file_repo::find_blob_by_id(state.writer_db(), second.id)
        .await
        .expect("second blob should exist");
    assert_eq!(migrated_second.policy_id, target.id);
}

#[actix_web::test]
async fn test_storage_migration_moves_blob_to_empty_target_policy() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-move").await;
    let target = create_local_policy(&state, "target-move").await;
    let blob = create_blob_with_object(&state, &source, b"move-me", 1).await;
    create_file_for_blob(&state, blob.id, "move.txt").await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    let stats = task_service::drain(&state)
        .await
        .expect("migration task should drain");
    assert_eq!(stats.succeeded, 1);

    let task = background_task_repo::find_by_id(state.writer_db(), task_id)
        .await
        .expect("task should exist");
    assert_eq!(task.status, BackgroundTaskStatus::Succeeded);
    let migrated = file_repo::find_blob_by_id(state.writer_db(), blob.id)
        .await
        .expect("blob should still exist");
    assert_eq!(migrated.policy_id, target.id);
    assert_eq!(
        migrated.storage_path,
        aster_drive::utils::storage_path_from_blob_key(&blob.hash)
    );
    assert!(migrated.thumbnail_path.is_none());
    assert!(
        std::path::Path::new(&target.base_path)
            .join(&migrated.storage_path)
            .exists()
    );
    let checkpoint = storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
        .await
        .expect("checkpoint should exist");
    assert_eq!(checkpoint.scanned_blobs, 1);
    assert_eq!(checkpoint.migrated_blobs, 1);
    assert_eq!(checkpoint.merged_blobs, 0);
    assert_eq!(checkpoint.migrated_bytes, 7);
}

#[actix_web::test]
async fn test_storage_migration_moves_opaque_local_blob_key_without_content_hash_mismatch() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-opaque").await;
    let target = create_local_policy(&state, "target-opaque").await;
    let blob = create_opaque_blob_with_object(
        &state,
        &source,
        "8a7ab9852bc34e98ac1fd29ddd94365b",
        b"opaque blob bytes",
        1,
    )
    .await;
    create_file_for_blob(&state, blob.id, "opaque.txt").await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let stats = task_service::drain(&state)
        .await
        .expect("opaque migration task should drain");
    assert_eq!(stats.succeeded, 1);

    let migrated = file_repo::find_blob_by_id(state.writer_db(), blob.id)
        .await
        .expect("blob should still exist");
    assert_eq!(migrated.hash, blob.hash);
    assert_eq!(migrated.policy_id, target.id);
    let target_object = tokio::fs::read(
        std::path::Path::new(&target.base_path)
            .join(aster_drive::utils::storage_path_from_blob_key(&blob.hash)),
    )
    .await
    .expect("target object should exist");
    assert_eq!(target_object, b"opaque blob bytes");

    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    let task = background_task_repo::find_by_id(state.writer_db(), task_id)
        .await
        .expect("task should exist");
    assert_eq!(task.status, BackgroundTaskStatus::Succeeded);
    let checkpoint = storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
        .await
        .expect("checkpoint should exist");
    assert_eq!(checkpoint.renamed_opaque_blobs, 0);
}

#[tokio::test]
async fn test_storage_migration_local_to_rustfs_s3_e2e() {
    let rustfs = start_rustfs_context("migration-e2e").await;

    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-rustfs-e2e").await;
    let target = create_s3_policy(
        &state,
        "target-rustfs-e2e",
        &rustfs.endpoint,
        &rustfs.bucket,
    )
    .await;
    let bytes = b"opaque local blob migrated into real rustfs s3";
    let blob = create_opaque_blob_with_object(
        &state,
        &source,
        "8a7ab9852bc34e98ac1fd29ddd94365b",
        bytes,
        1,
    )
    .await;
    create_file_for_blob(&state, blob.id, "rustfs-e2e.txt").await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    assert_eq!(body["code"], 0);
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    let stats = task_service::drain(&state)
        .await
        .expect("local to rustfs migration task should drain");
    assert_eq!(stats.succeeded, 1);

    let migrated = file_repo::find_blob_by_id(state.writer_db(), blob.id)
        .await
        .expect("blob row should remain after policy move");
    assert_eq!(migrated.hash, blob.hash);
    assert_eq!(migrated.policy_id, target.id);
    assert_eq!(
        migrated.storage_path,
        aster_drive::utils::storage_path_from_blob_key(&blob.hash)
    );
    let object_key = policy_object_key(&target, &migrated.storage_path);
    let target_bytes = read_s3_object(&rustfs.endpoint, &rustfs.bucket, &object_key).await;
    assert_eq!(target_bytes, bytes);

    let task = background_task_repo::find_by_id(state.writer_db(), task_id)
        .await
        .expect("task should exist");
    assert_eq!(task.status, BackgroundTaskStatus::Succeeded);
    let checkpoint = storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
        .await
        .expect("checkpoint should exist");
    assert_eq!(checkpoint.stage, "complete");
    assert_eq!(checkpoint.scanned_blobs, 1);
    assert_eq!(checkpoint.migrated_blobs, 1);
    assert_eq!(checkpoint.migrated_bytes, bytes.len() as i64);
}

#[tokio::test]
async fn test_storage_migration_local_to_rustfs_s3_resume_after_partial_failure_e2e() {
    let rustfs = start_rustfs_context("migration-resume-e2e").await;

    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-rustfs-resume").await;
    let target = create_s3_policy(
        &state,
        "target-rustfs-resume",
        &rustfs.endpoint,
        &rustfs.bucket,
    )
    .await;
    let first_bytes = b"resume-first-original-0001";
    let second_bytes = b"resume-second-original-0001";
    let tampered_second_bytes = b"resume-second-tampered-0001";
    assert_eq!(second_bytes.len(), tampered_second_bytes.len());

    let first = create_blob_with_object(&state, &source, first_bytes, 1).await;
    let second = create_blob_with_object(&state, &source, second_bytes, 1).await;
    create_file_for_blob(&state, first.id, "resume-first.txt").await;
    create_file_for_blob(&state, second.id, "resume-second.txt").await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    assert_eq!(body["code"], 0);
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");

    let second_source_path = std::path::Path::new(&source.base_path).join(&second.storage_path);
    tokio::fs::write(&second_source_path, tampered_second_bytes)
        .await
        .expect("second source object should be tampered before first run");
    let stats = task_service::drain(&state)
        .await
        .expect("first rustfs resume attempt should drain");
    assert_eq!(stats.failed, 1);

    let failed_task = background_task_repo::find_by_id(state.writer_db(), task_id)
        .await
        .expect("failed task should exist");
    assert_eq!(failed_task.status, BackgroundTaskStatus::Failed);
    assert!(
        failed_task
            .last_error
            .as_deref()
            .expect("failed task should store last_error")
            .contains("copied blob hash mismatch")
    );
    let checkpoint = storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
        .await
        .expect("checkpoint should exist after failure");
    assert_eq!(checkpoint.stage, "migrate_blobs");
    assert_eq!(checkpoint.last_processed_blob_id, first.id);
    assert_eq!(checkpoint.scanned_blobs, 1);
    assert_eq!(checkpoint.migrated_blobs, 1);
    assert_eq!(checkpoint.migrated_bytes, first_bytes.len() as i64);

    let first_after_failure = file_repo::find_blob_by_id(state.writer_db(), first.id)
        .await
        .expect("first blob should remain after first run");
    assert_eq!(first_after_failure.policy_id, target.id);
    let first_key = policy_object_key(&target, &first_after_failure.storage_path);
    assert_eq!(
        read_s3_object(&rustfs.endpoint, &rustfs.bucket, &first_key).await,
        first_bytes
    );
    let failed_second_key = policy_object_key(
        &target,
        &aster_drive::utils::storage_path_from_blob_key(&second.hash),
    );
    assert!(
        !s3_object_exists(&rustfs.endpoint, &rustfs.bucket, &failed_second_key).await,
        "failed copy should cleanup the partially written second target object"
    );

    tokio::fs::write(&second_source_path, second_bytes)
        .await
        .expect("second source object should be restored before resume");
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/admin/storage-migrations/{task_id}/resume"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "pending");

    let stats = task_service::drain(&state)
        .await
        .expect("resumed rustfs migration should drain");
    assert_eq!(stats.succeeded, 1);

    let second_after_resume = file_repo::find_blob_by_id(state.writer_db(), second.id)
        .await
        .expect("second blob should remain after resume");
    assert_eq!(second_after_resume.policy_id, target.id);
    let second_key = policy_object_key(&target, &second_after_resume.storage_path);
    assert_eq!(
        read_s3_object(&rustfs.endpoint, &rustfs.bucket, &second_key).await,
        second_bytes
    );

    let final_checkpoint =
        storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
            .await
            .expect("checkpoint should exist after resume");
    assert_eq!(final_checkpoint.stage, "complete");
    assert_eq!(final_checkpoint.last_processed_blob_id, second.id);
    assert_eq!(final_checkpoint.scanned_blobs, 2);
    assert_eq!(final_checkpoint.migrated_blobs, 2);
    assert_eq!(
        final_checkpoint.migrated_bytes,
        i64::try_from(first_bytes.len() + second_bytes.len())
            .expect("test byte length should fit i64")
    );
}

#[actix_web::test]
async fn test_storage_migration_crosses_batch_boundary_and_merges_existing_target_blob() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-bulk").await;
    let target = create_local_policy(&state, "target-bulk").await;
    let mut source_blobs = Vec::new();
    let mut source_bytes = Vec::new();

    for index in 0..105 {
        let bytes = format!("bulk migration payload #{index:03}").into_bytes();
        let blob = if index % 10 == 0 {
            create_opaque_blob_with_object(
                &state,
                &source,
                &format!("opaque-bulk-key-{index:03}"),
                &bytes,
                1,
            )
            .await
        } else {
            create_blob_with_object(&state, &source, &bytes, 1).await
        };
        create_file_for_blob(&state, blob.id, &format!("bulk-{index:03}.txt")).await;
        source_blobs.push(blob);
        source_bytes.push(bytes);
    }

    let merge_index = 42;
    let target_duplicate =
        create_blob_with_object(&state, &target, &source_bytes[merge_index], 3).await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    assert_eq!(body["code"], 0);
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    let stats = task_service::drain(&state)
        .await
        .expect("bulk migration task should drain");
    assert_eq!(stats.succeeded, 1);

    for (index, blob) in source_blobs.iter().enumerate() {
        if index == merge_index {
            assert!(
                file_repo::find_blob_by_id(state.writer_db(), blob.id)
                    .await
                    .is_err(),
                "merged source blob row should be deleted"
            );
            continue;
        }

        let migrated = file_repo::find_blob_by_id(state.writer_db(), blob.id)
            .await
            .expect("migrated source blob row should remain");
        assert_eq!(migrated.policy_id, target.id);
        let target_object =
            tokio::fs::read(std::path::Path::new(&target.base_path).join(&migrated.storage_path))
                .await
                .expect("target object should exist for migrated blob");
        assert_eq!(target_object, source_bytes[index]);
    }

    let merged_target = file_repo::find_blob_by_id(state.writer_db(), target_duplicate.id)
        .await
        .expect("target duplicate blob should remain after merge");
    assert_eq!(merged_target.ref_count, 4);
    let merged_file = file::Entity::find()
        .filter(file::Column::Name.eq("bulk-042.txt"))
        .one(state.writer_db())
        .await
        .expect("merged file query should succeed")
        .expect("merged file should exist");
    assert_eq!(merged_file.blob_id, target_duplicate.id);

    let checkpoint = storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
        .await
        .expect("checkpoint should exist");
    assert_eq!(checkpoint.stage, "complete");
    assert_eq!(
        checkpoint.last_processed_blob_id,
        source_blobs.last().expect("source blobs should exist").id
    );
    assert_eq!(checkpoint.scanned_blobs, 105);
    assert_eq!(checkpoint.migrated_blobs, 104);
    assert_eq!(checkpoint.merged_blobs, 1);
    assert_eq!(checkpoint.skipped_blobs, 0);
    assert_eq!(checkpoint.failed_blobs, 0);
    let expected_bytes = source_bytes.iter().try_fold(0_i64, |acc, bytes| {
        i64::try_from(bytes.len()).map(|len| acc + len)
    });
    assert_eq!(
        checkpoint.migrated_bytes,
        expected_bytes.expect("bulk test byte length should fit i64")
    );
}

#[actix_web::test]
async fn test_storage_migration_merges_when_target_blob_already_exists() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-merge").await;
    let target = create_local_policy(&state, "target-merge").await;
    let source_blob = create_blob_with_object(&state, &source, b"same-bytes", 2).await;
    let target_blob = create_blob_with_object(&state, &target, b"same-bytes", 1).await;
    let active_file = create_file_for_blob(&state, source_blob.id, "active.txt").await;
    create_version_for_blob(&state, active_file.id, source_blob.id, 1).await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    let stats = task_service::drain(&state)
        .await
        .expect("migration task should drain");
    assert_eq!(stats.succeeded, 1);

    assert!(
        file_repo::find_blob_by_id(state.writer_db(), source_blob.id)
            .await
            .is_err(),
        "old source blob row should be deleted after merge"
    );
    let merged_target = file_repo::find_blob_by_id(state.writer_db(), target_blob.id)
        .await
        .expect("target blob should exist");
    assert_eq!(merged_target.ref_count, 3);
    let updated_file = file::Entity::find_by_id(active_file.id)
        .one(state.writer_db())
        .await
        .expect("file query should succeed")
        .expect("file should exist");
    assert_eq!(updated_file.blob_id, target_blob.id);
    let updated_version = file_version::Entity::find()
        .filter(file_version::Column::FileId.eq(active_file.id))
        .one(state.writer_db())
        .await
        .expect("version query should succeed")
        .expect("version should exist");
    assert_eq!(updated_version.blob_id, target_blob.id);
    let checkpoint = storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
        .await
        .expect("checkpoint should exist");
    assert_eq!(checkpoint.scanned_blobs, 1);
    assert_eq!(checkpoint.migrated_blobs, 0);
    assert_eq!(checkpoint.merged_blobs, 1);
    assert_eq!(checkpoint.migrated_bytes, 10);
}

#[actix_web::test]
async fn test_storage_migration_fails_when_content_hash_matches_but_size_differs() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-hash-size-mismatch").await;
    let target = create_local_policy(&state, "target-hash-size-mismatch").await;
    let source_blob = create_blob_with_object(&state, &source, b"same-hash-source", 1).await;
    let target_blob = create_blob_with_object(&state, &target, b"x", 1).await;
    let mut target_update: file_blob::ActiveModel = target_blob.into();
    target_update.hash = Set(source_blob.hash.clone());
    target_update
        .update(state.writer_db())
        .await
        .expect("target hash should be forced for boundary test");

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    let stats = task_service::drain(&state)
        .await
        .expect("migration task should drain");
    assert_eq!(stats.failed, 1);

    let task = background_task_repo::find_by_id(state.writer_db(), task_id)
        .await
        .expect("task should exist");
    assert_eq!(task.status, BackgroundTaskStatus::Failed);
    let last_error = task
        .last_error
        .as_deref()
        .expect("failed task should store last_error");
    assert!(
        last_error.contains("target blob record does not match source blob"),
        "{last_error}"
    );
    let source_after = file_repo::find_blob_by_id(state.writer_db(), source_blob.id)
        .await
        .expect("source blob should remain");
    assert_eq!(source_after.policy_id, source.id);
}

#[actix_web::test]
async fn test_storage_migration_does_not_merge_opaque_blob_key_with_same_size() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-opaque-same-size").await;
    let target = create_local_policy(&state, "target-opaque-same-size").await;
    let source_blob =
        create_opaque_blob_with_object(&state, &source, "opaque-shared-same-size", b"source", 1)
            .await;
    let target_blob =
        create_opaque_blob_with_object(&state, &target, "opaque-shared-same-size", b"target", 1)
            .await;
    create_file_for_blob(&state, source_blob.id, "opaque-source.txt").await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    let stats = task_service::drain(&state)
        .await
        .expect("opaque same-size migration task should drain");
    assert_eq!(stats.succeeded, 1);

    let migrated = file_repo::find_blob_by_id(state.writer_db(), source_blob.id)
        .await
        .expect("source blob row should be moved, not merged");
    assert_eq!(migrated.policy_id, target.id);
    assert_ne!(migrated.hash, target_blob.hash);
    assert!(migrated.hash.starts_with("migration-"));
    assert_ne!(migrated.storage_path, target_blob.storage_path);
    let migrated_bytes =
        tokio::fs::read(std::path::Path::new(&target.base_path).join(&migrated.storage_path))
            .await
            .expect("migrated opaque object should exist");
    assert_eq!(migrated_bytes, b"source");
    let target_bytes =
        tokio::fs::read(std::path::Path::new(&target.base_path).join(&target_blob.storage_path))
            .await
            .expect("existing opaque target object should remain");
    assert_eq!(target_bytes, b"target");

    let checkpoint = storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
        .await
        .expect("checkpoint should exist");
    assert_eq!(checkpoint.migrated_blobs, 1);
    assert_eq!(checkpoint.merged_blobs, 0);
    assert_eq!(checkpoint.renamed_opaque_blobs, 1);
}

#[actix_web::test]
async fn test_storage_migration_does_not_merge_opaque_blob_key_with_different_size() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-opaque-different-size").await;
    let target = create_local_policy(&state, "target-opaque-different-size").await;
    let source_blob = create_opaque_blob_with_object(
        &state,
        &source,
        "opaque-shared-different-size",
        b"source-long",
        1,
    )
    .await;
    let target_blob = create_opaque_blob_with_object(
        &state,
        &target,
        "opaque-shared-different-size",
        b"target",
        1,
    )
    .await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    let stats = task_service::drain(&state)
        .await
        .expect("opaque different-size migration task should drain");
    assert_eq!(stats.succeeded, 1);

    let migrated = file_repo::find_blob_by_id(state.writer_db(), source_blob.id)
        .await
        .expect("source blob row should be moved, not merged");
    assert_eq!(migrated.policy_id, target.id);
    assert_ne!(migrated.hash, target_blob.hash);
    assert!(migrated.hash.starts_with("migration-"));
    assert_eq!(migrated.size, source_blob.size);
    let checkpoint = storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
        .await
        .expect("checkpoint should exist");
    assert_eq!(checkpoint.migrated_blobs, 1);
    assert_eq!(checkpoint.merged_blobs, 0);
    assert_eq!(checkpoint.renamed_opaque_blobs, 1);
}

#[actix_web::test]
async fn test_storage_migration_empty_source_succeeds_with_zero_counts() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-empty").await;
    let target = create_local_policy(&state, "target-empty").await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    let stats = task_service::drain(&state)
        .await
        .expect("empty migration task should drain");
    assert_eq!(stats.succeeded, 1);

    let task = background_task_repo::find_by_id(state.writer_db(), task_id)
        .await
        .expect("task should exist");
    assert_eq!(task.status, BackgroundTaskStatus::Succeeded);
    assert_eq!(task.progress_current, 0);
    assert_eq!(task.progress_total, 0);

    let result: aster_drive::services::task_service::StoragePolicyMigrationTaskResult =
        serde_json::from_str(
            task.result_json
                .as_ref()
                .map(AsRef::as_ref)
                .expect("successful migration should store result"),
        )
        .expect("result json should parse");
    assert_eq!(result.scanned_blobs, 0);
    assert_eq!(result.migrated_blobs, 0);
    assert_eq!(result.merged_blobs, 0);
    assert_eq!(result.skipped_blobs, 0);
    assert_eq!(result.failed_blobs, 0);
    assert_eq!(result.migrated_bytes, 0);

    let checkpoint = storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
        .await
        .expect("checkpoint should exist");
    assert_eq!(checkpoint.stage, "complete");
    assert_eq!(checkpoint.last_processed_blob_id, 0);
    assert_eq!(checkpoint.scanned_blobs, 0);
    assert_eq!(checkpoint.migrated_blobs, 0);
    assert_eq!(checkpoint.merged_blobs, 0);
    assert_eq!(checkpoint.skipped_blobs, 0);
    assert_eq!(checkpoint.failed_blobs, 0);
}

#[actix_web::test]
async fn test_storage_migration_cleans_target_object_when_verification_fails() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-cleanup").await;
    let target = create_local_policy(&state, "target-cleanup").await;
    let blob = create_blob_with_object(&state, &source, b"cleanup-me", 1).await;
    create_file_for_blob(&state, blob.id, "cleanup.txt").await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    let source_full_path = std::path::Path::new(&source.base_path).join(&blob.storage_path);
    tokio::fs::write(&source_full_path, b"bad-data!!")
        .await
        .expect("source object should be tampered before migration starts");

    let stats = task_service::drain(&state)
        .await
        .expect("failed migration task should drain");
    assert_eq!(stats.failed, 1);

    let task = background_task_repo::find_by_id(state.writer_db(), task_id)
        .await
        .expect("task should exist");
    assert_eq!(task.status, BackgroundTaskStatus::Failed);
    assert!(
        task.last_error
            .as_deref()
            .expect("failed task should store last_error")
            .contains("copied blob hash mismatch")
    );

    let source_blob = file_repo::find_blob_by_id(state.writer_db(), blob.id)
        .await
        .expect("source blob row should remain");
    assert_eq!(source_blob.policy_id, source.id);
    let target_driver = state
        .driver_registry
        .get_driver(&target)
        .expect("target driver should exist");
    let target_path = aster_drive::utils::storage_path_from_blob_key(&blob.hash);
    assert!(
        !target_driver
            .exists(&target_path)
            .await
            .expect("target existence check should succeed"),
        "failed migration should cleanup the target object"
    );

    let checkpoint = storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
        .await
        .expect("checkpoint should exist");
    assert_eq!(checkpoint.stage, "migrate_blobs");
    assert_eq!(checkpoint.last_processed_blob_id, 0);
    assert_eq!(checkpoint.scanned_blobs, 0);
    assert_eq!(checkpoint.migrated_blobs, 0);
}

#[actix_web::test]
async fn test_storage_migration_fails_when_policy_changes_after_task_creation() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-changed").await;
    let target = create_local_policy(&state, "target-changed").await;
    create_blob_with_object(&state, &source, b"do-not-move", 1).await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");

    let mut target_update: storage_policy::ActiveModel = target.clone().into();
    target_update.base_path = Set(format!("{}-changed", target.base_path));
    target_update.updated_at = Set(Utc::now() + chrono::Duration::seconds(1));
    target_update
        .update(state.writer_db())
        .await
        .expect("target policy should update");

    let stats = task_service::drain(&state)
        .await
        .expect("changed migration task should drain");
    assert_eq!(stats.failed, 1);

    let task = background_task_repo::find_by_id(state.writer_db(), task_id)
        .await
        .expect("task should exist");
    assert_eq!(task.status, BackgroundTaskStatus::Failed);
    assert!(
        task.last_error
            .as_deref()
            .expect("failed task should store last_error")
            .contains("storage policy changed after migration task was created")
    );

    let checkpoint = storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
        .await
        .expect("checkpoint should exist");
    assert_eq!(checkpoint.stage, "prepare_policies");
    assert_eq!(checkpoint.last_processed_blob_id, 0);
    assert_eq!(checkpoint.scanned_blobs, 0);
    assert_eq!(checkpoint.migrated_blobs, 0);
    assert_eq!(checkpoint.merged_blobs, 0);
    assert_eq!(checkpoint.skipped_blobs, 0);
    assert_eq!(checkpoint.failed_blobs, 0);
}

#[actix_web::test]
async fn test_storage_migration_rejects_duplicate_active_pair() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-duplicate").await;
    let target = create_local_policy(&state, "target-duplicate").await;

    let first = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    assert_eq!(first["code"], 0);
    let (status, second) =
        create_migration_task_via_api_with_status(&app, &token, source.id, target.id, false).await;
    assert_conflicting_storage_migration_response(status, &second);
}

#[actix_web::test]
async fn test_storage_migration_active_conflict_matrix() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let source_out = create_local_policy(&state, "source-out").await;
    let target_in = create_local_policy(&state, "target-in").await;
    let other_for_out = create_local_policy(&state, "other-for-out").await;
    let first =
        create_migration_task_via_api(&app, &token, source_out.id, target_in.id, false).await;
    assert_eq!(first["code"], 0);

    let (status, source_with_outgoing) = create_migration_task_via_api_with_status(
        &app,
        &token,
        source_out.id,
        other_for_out.id,
        false,
    )
    .await;
    assert_conflicting_storage_migration_response(status, &source_with_outgoing);

    let (status, source_with_incoming) = create_migration_task_via_api_with_status(
        &app,
        &token,
        target_in.id,
        other_for_out.id,
        false,
    )
    .await;
    assert_conflicting_storage_migration_response(status, &source_with_incoming);

    let (status, target_with_outgoing) = create_migration_task_via_api_with_status(
        &app,
        &token,
        other_for_out.id,
        source_out.id,
        false,
    )
    .await;
    assert_conflicting_storage_migration_response(status, &target_with_outgoing);

    let allowed_second_source = create_local_policy(&state, "allowed-second-source").await;
    let target_with_incoming =
        create_migration_task_via_api(&app, &token, allowed_second_source.id, target_in.id, false)
            .await;
    assert_eq!(target_with_incoming["code"], 0);

    let (status, reverse) =
        create_migration_task_via_api_with_status(&app, &token, target_in.id, source_out.id, false)
            .await;
    assert_conflicting_storage_migration_response(status, &reverse);
}

#[actix_web::test]
async fn test_storage_migration_dry_run_uses_active_conflict_rules() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-dry-conflict").await;
    let target = create_local_policy(&state, "target-dry-conflict").await;
    let other = create_local_policy(&state, "other-dry-conflict").await;

    let first = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    assert_eq!(first["code"], 0);

    let (blocked_status, blocked_body) =
        dry_run_migration_via_api(&app, &token, target.id, other.id).await;
    assert_conflicting_storage_migration_response(blocked_status, &blocked_body);

    let allowed_source = create_local_policy(&state, "allowed-dry-source").await;
    let (allowed_status, allowed_body) =
        dry_run_migration_via_api(&app, &token, allowed_source.id, target.id).await;
    assert_eq!(allowed_status, actix_web::http::StatusCode::OK);
    assert_eq!(allowed_body["code"], 0);
}

#[actix_web::test]
async fn test_storage_migration_terminal_tasks_do_not_block_new_migrations() {
    for status in [
        BackgroundTaskStatus::Succeeded,
        BackgroundTaskStatus::Failed,
        BackgroundTaskStatus::Canceled,
    ] {
        let state = common::setup().await;
        let app = create_test_app!(state.clone());
        let (token, _) = register_and_login!(app);
        let source =
            create_local_policy(&state, &format!("terminal-source-{}", status.as_str())).await;
        let target =
            create_local_policy(&state, &format!("terminal-target-{}", status.as_str())).await;
        let new_target =
            create_local_policy(&state, &format!("terminal-new-target-{}", status.as_str())).await;

        let first = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
        assert_eq!(first["code"], 0);
        let task_id = first["data"]["id"].as_i64().expect("task id should exist");
        set_background_task_status(&state, task_id, status).await;

        let second =
            create_migration_task_via_api(&app, &token, source.id, new_target.id, false).await;
        assert_eq!(second["code"], 0);
    }
}

#[actix_web::test]
async fn test_storage_migration_active_statuses_block_new_migrations() {
    for status in [
        BackgroundTaskStatus::Pending,
        BackgroundTaskStatus::Processing,
        BackgroundTaskStatus::Retry,
    ] {
        let state = common::setup().await;
        let app = create_test_app!(state.clone());
        let (token, _) = register_and_login!(app);
        let source =
            create_local_policy(&state, &format!("active-source-{}", status.as_str())).await;
        let target =
            create_local_policy(&state, &format!("active-target-{}", status.as_str())).await;
        let new_target =
            create_local_policy(&state, &format!("active-new-target-{}", status.as_str())).await;

        let first = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
        assert_eq!(first["code"], 0);
        let task_id = first["data"]["id"].as_i64().expect("task id should exist");
        set_background_task_status(&state, task_id, status).await;

        let (create_status, create_body) = create_migration_task_via_api_with_status(
            &app,
            &token,
            source.id,
            new_target.id,
            false,
        )
        .await;
        assert_conflicting_storage_migration_response(create_status, &create_body);

        let (dry_run_status, dry_run_body) =
            dry_run_migration_via_api(&app, &token, target.id, new_target.id).await;
        assert_conflicting_storage_migration_response(dry_run_status, &dry_run_body);
    }
}

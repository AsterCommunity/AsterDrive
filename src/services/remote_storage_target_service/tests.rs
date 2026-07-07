use super::{
    create, delete,
    driver::{
        build_driver_from_target, list_registered_remote_storage_target_driver_descriptors,
        registered_remote_storage_target_driver_types, validate_driver_from_target,
    },
    list,
    normalization::{normalize_create_input, normalize_update_input},
    paths::{normalize_relative_local_path, resolve_remote_storage_target_local_path},
    resolve_effective_target, resolve_target_by_key, update,
};
use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{master_binding_repo, remote_storage_target_repo};
use crate::entities::{master_binding, remote_storage_target};
use crate::metrics_core::SharedMetricsRecorder;
use crate::runtime::{FollowerRuntimeState, SharedRuntimeState};
use crate::storage::remote_protocol::{
    RemoteCreateLocalStorageTargetRequest, RemoteCreateS3StorageTargetRequest,
    RemoteCreateStorageTargetRequest, RemoteUpdateStorageTargetRequest,
};
use crate::types::DriverType;
use chrono::Utc;
use sea_orm::{DatabaseConnection, Set};
use std::fs;
use std::sync::Arc;

struct TestFollowerState {
    db: DatabaseConnection,
    driver_registry: Arc<crate::storage::DriverRegistry>,
    runtime_config: Arc<crate::config::RuntimeConfig>,
    policy_snapshot: Arc<crate::storage::PolicySnapshot>,
    config: Arc<crate::config::Config>,
    cache: Arc<dyn crate::cache::CacheBackend>,
    metrics: SharedMetricsRecorder,
}

impl SharedRuntimeState for TestFollowerState {
    fn writer_db(&self) -> &DatabaseConnection {
        &self.db
    }

    fn reader_db(&self) -> &DatabaseConnection {
        &self.db
    }

    fn driver_registry(&self) -> &Arc<crate::storage::DriverRegistry> {
        &self.driver_registry
    }

    fn runtime_config(&self) -> &Arc<crate::config::RuntimeConfig> {
        &self.runtime_config
    }

    fn policy_snapshot(&self) -> &Arc<crate::storage::PolicySnapshot> {
        &self.policy_snapshot
    }

    fn config(&self) -> &Arc<crate::config::Config> {
        &self.config
    }

    fn cache(&self) -> &Arc<dyn crate::cache::CacheBackend> {
        &self.cache
    }

    fn metrics(&self) -> &SharedMetricsRecorder {
        &self.metrics
    }
}

impl FollowerRuntimeState for TestFollowerState {}

async fn setup_state() -> TestFollowerState {
    let db = crate::db::connect_with_metrics(
        &crate::config::DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            pool_size: 1,
            retry_count: 0,
        },
        crate::metrics_core::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    migration::Migrator::up(&db, None).await.unwrap();

    let root = std::env::temp_dir().join(format!(
        "aster-remote-storage-target-service-root-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&root).unwrap();
    let config = Arc::new(crate::config::Config {
        server: crate::config::ServerConfig {
            follower: crate::config::ServerFollowerConfig {
                remote_storage_target_local_root: root.to_string_lossy().into_owned(),
            },
            ..Default::default()
        },
        ..Default::default()
    });
    let cache = crate::cache::create_cache(&crate::config::CacheConfig {
        ..Default::default()
    })
    .await;

    TestFollowerState {
        db,
        driver_registry: Arc::new(crate::storage::DriverRegistry::noop()),
        runtime_config: Arc::new(crate::config::RuntimeConfig::new()),
        policy_snapshot: Arc::new(crate::storage::PolicySnapshot::new()),
        config,
        cache,
        metrics: crate::metrics_core::NoopMetrics::arc(),
    }
}

async fn create_binding(state: &TestFollowerState, access_key: &str) -> master_binding::Model {
    let now = Utc::now();
    master_binding_repo::create(
        state.writer_db(),
        master_binding::ActiveModel {
            name: Set(format!("binding-{access_key}")),
            master_url: Set("https://primary.example.com".to_string()),
            access_key: Set(access_key.to_string()),
            secret_key: Set(format!("secret-{access_key}")),
            storage_namespace: Set(format!("ns-{access_key}")),
            is_enabled: Set(true),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .unwrap()
}

fn local_create(name: &str, base_path: &str, is_default: bool) -> RemoteCreateStorageTargetRequest {
    RemoteCreateStorageTargetRequest::Local(RemoteCreateLocalStorageTargetRequest {
        name: name.to_string(),
        base_path: base_path.to_string(),
        is_default,
    })
}

fn s3_create(
    name: &str,
    endpoint: &str,
    bucket: &str,
    base_path: &str,
    is_default: bool,
) -> RemoteCreateStorageTargetRequest {
    RemoteCreateStorageTargetRequest::S3(RemoteCreateS3StorageTargetRequest {
        name: name.to_string(),
        endpoint: endpoint.to_string(),
        bucket: bucket.to_string(),
        access_key: "access".to_string(),
        secret_key: "secret".to_string(),
        base_path: base_path.to_string(),
        is_default,
    })
}

fn model_with_driver(driver_type: DriverType) -> remote_storage_target::Model {
    let now = Utc::now();
    remote_storage_target::Model {
        id: 1,
        master_binding_id: 1,
        target_key: "rst_test".to_string(),
        name: "test".to_string(),
        driver_type,
        endpoint: String::new(),
        bucket: "bucket".to_string(),
        access_key: "access".to_string(),
        secret_key: "secret".to_string(),
        base_path: "profile".to_string(),
        is_default: true,
        desired_revision: 1,
        applied_revision: 1,
        last_error: String::new(),
        created_at: now,
        updated_at: now,
    }
}

fn expect_aster_err<T>(result: crate::errors::Result<T>) -> crate::errors::AsterError {
    match result {
        Ok(_) => panic!("expected AsterError"),
        Err(error) => error,
    }
}

#[test]
fn normalize_relative_local_path_keeps_normal_segments() {
    let normalized = normalize_relative_local_path(" archive/2026 ").unwrap();
    assert_eq!(normalized, "archive/2026");
}

#[test]
fn normalize_relative_local_path_rejects_escape_attempts() {
    let error = normalize_relative_local_path("../secret").unwrap_err();
    assert!(
        error
            .message()
            .contains("server.follower.remote_storage_target_local_root")
    );
}

#[test]
fn normalize_relative_local_path_rejects_backslash_escape_attempts() {
    let error = normalize_relative_local_path("..\\secret").unwrap_err();
    assert!(
        error
            .message()
            .contains("server.follower.remote_storage_target_local_root")
    );
}

#[test]
fn resolve_remote_storage_target_local_path_allows_missing_child_inside_root() {
    let root = std::env::temp_dir().join(format!(
        "aster-remote-storage-target-root-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&root).unwrap();

    let resolved =
        resolve_remote_storage_target_local_path(root.to_str().unwrap(), "profiles/new").unwrap();
    assert_eq!(
        resolved,
        fs::canonicalize(&root)
            .unwrap()
            .join("profiles")
            .join("new")
    );

    let _ = fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[test]
fn resolve_remote_storage_target_local_path_rejects_symlink_escape() {
    let root = std::env::temp_dir().join(format!(
        "aster-remote-storage-target-root-{}",
        uuid::Uuid::new_v4()
    ));
    let outside = std::env::temp_dir().join(format!(
        "aster-remote-storage-target-outside-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();

    let error = resolve_remote_storage_target_local_path(root.to_str().unwrap(), "escape/profile")
        .unwrap_err();
    assert!(
        error
            .message()
            .contains("server.follower.remote_storage_target_local_root")
    );

    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&outside);
}

#[test]
fn normalize_relative_local_path_collapses_current_dir_segments_to_dot() {
    assert_eq!(normalize_relative_local_path("././").unwrap(), ".");
    assert_eq!(
        normalize_relative_local_path("assets/./photos").unwrap(),
        "assets/photos"
    );
}

#[test]
fn normalize_relative_local_path_rejects_blank_values() {
    let error = normalize_relative_local_path(" \t ").unwrap_err();
    assert!(error.message().contains("base_path cannot be blank"));
}

#[test]
fn resolve_remote_storage_target_local_path_rejects_empty_root() {
    let error = resolve_remote_storage_target_local_path("   ", "profile").unwrap_err();
    assert!(
        error
            .message()
            .contains("remote_storage_target_local_root cannot be empty")
    );
}

#[test]
fn normalize_create_input_trims_local_and_s3_fields() {
    let local = normalize_create_input(local_create(" Local ", " ./dropbox/ ", true)).unwrap();
    assert_eq!(local.name, "Local");
    assert_eq!(local.driver_type, DriverType::Local);
    assert_eq!(local.base_path, "dropbox");
    assert_eq!(local.is_default, Some(true));

    let s3 = normalize_create_input(s3_create(
        " S3 ",
        " https://s3.example.com/path ",
        " bucket ",
        " /prefix/ ",
        false,
    ))
    .unwrap();
    assert_eq!(s3.name, "S3");
    assert_eq!(s3.driver_type, DriverType::S3);
    assert_eq!(s3.endpoint, "https://s3.example.com/path");
    assert_eq!(s3.bucket, "bucket");
    assert_eq!(s3.base_path, "prefix");
    assert_eq!(s3.is_default, Some(false));
}

#[test]
fn normalize_create_input_rejects_invalid_values() {
    let error = expect_aster_err(normalize_create_input(local_create(" ", "profile", false)));
    assert!(error.message().contains("name cannot be blank"));

    let error = expect_aster_err(normalize_create_input(s3_create(
        "S3",
        "https://s3.example.com",
        "",
        "",
        false,
    )));
    assert!(error.message().contains("bucket is required"));
}

#[test]
fn normalize_update_input_keeps_existing_driver_fields_and_trims_replacements() {
    let existing = model_with_driver(DriverType::S3);
    let normalized = normalize_update_input(
        existing.clone(),
        RemoteUpdateStorageTargetRequest {
            name: Some(" Updated ".to_string()),
            base_path: Some(" /next/ ".to_string()),
            is_default: Some(true),
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(normalized.name, "Updated");
    assert_eq!(normalized.driver_type, DriverType::S3);
    assert_eq!(normalized.endpoint, existing.endpoint);
    assert_eq!(normalized.bucket, existing.bucket);
    assert_eq!(normalized.access_key, existing.access_key);
    assert_eq!(normalized.secret_key, existing.secret_key);
    assert_eq!(normalized.base_path, "next");
    assert_eq!(normalized.is_default, Some(true));
}

#[test]
fn normalize_update_input_preserves_secret_when_same_driver_omits_credentials() {
    let existing = model_with_driver(DriverType::S3);
    let normalized = normalize_update_input(
        existing.clone(),
        RemoteUpdateStorageTargetRequest {
            name: Some("Renamed".to_string()),
            access_key: None,
            secret_key: None,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(normalized.driver_type, DriverType::S3);
    assert_eq!(normalized.access_key, existing.access_key);
    assert_eq!(normalized.secret_key, existing.secret_key);
}

#[test]
fn normalize_update_input_replaces_secret_when_same_driver_provides_credentials() {
    let normalized = normalize_update_input(
        model_with_driver(DriverType::S3),
        RemoteUpdateStorageTargetRequest {
            access_key: Some(" new-access ".to_string()),
            secret_key: Some(" new-secret ".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(normalized.access_key, "new-access");
    assert_eq!(normalized.secret_key, "new-secret");
}

#[test]
fn normalize_update_input_resets_driver_specific_fields_when_driver_changes() {
    let existing = model_with_driver(DriverType::S3);
    let normalized = normalize_update_input(
        existing,
        RemoteUpdateStorageTargetRequest {
            driver_type: Some(DriverType::Local),
            base_path: Some(" local/profile ".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(normalized.driver_type, DriverType::Local);
    assert_eq!(normalized.endpoint, "");
    assert_eq!(normalized.bucket, "");
    assert_eq!(normalized.access_key, "");
    assert_eq!(normalized.secret_key, "");
    assert_eq!(normalized.base_path, "local/profile");
}

#[test]
fn managed_ingress_driver_registry_contains_supported_builtin_drivers() {
    assert_eq!(
        registered_remote_storage_target_driver_types(),
        vec![DriverType::Local, DriverType::S3]
    );
}

#[test]
fn remote_storage_target_driver_descriptors_cover_builtin_profile_fields() {
    let descriptors = list_registered_remote_storage_target_driver_descriptors()
        .expect("registered remote storage target descriptors should build");
    assert_eq!(descriptors.len(), 2);

    let local = descriptors
        .iter()
        .find(|descriptor| descriptor.driver_type == DriverType::Local)
        .expect("local managed ingress descriptor should be registered");
    assert_eq!(
        local
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["base_path", "is_default"]
    );
    let local_base_path = local
        .fields
        .iter()
        .find(|field| field.name == "base_path")
        .expect("local base_path descriptor should exist");
    assert_eq!(
        local_base_path
            .validation
            .as_ref()
            .map(|validation| validation.relative_local_path),
        Some(true)
    );

    let s3 = descriptors
        .iter()
        .find(|descriptor| descriptor.driver_type == DriverType::S3)
        .expect("s3 managed ingress descriptor should be registered");
    assert_eq!(
        s3.fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "endpoint",
            "bucket",
            "access_key",
            "secret_key",
            "base_path",
            "is_default"
        ]
    );
    assert!(
        s3.fields
            .iter()
            .any(|field| field.name == "secret_key" && field.secret)
    );
    let s3_base_path = s3
        .fields
        .iter()
        .find(|field| field.name == "base_path")
        .expect("s3 base_path descriptor should exist");
    assert_eq!(s3_base_path.validation, None);
}

#[tokio::test]
async fn driver_builder_rejects_remote_remote_storage_targets() {
    let state = setup_state().await;
    let profile = model_with_driver(DriverType::Remote);

    let validate_error = validate_driver_from_target(&state, &profile).unwrap_err();
    assert_eq!(
        validate_error.api_error_code_override(),
        Some(ApiErrorCode::ManagedIngressDriverUnsupported)
    );
    assert!(
        validate_error
            .message()
            .contains("do not support the remote driver")
    );
    let build_error = expect_aster_err(build_driver_from_target(&state, &profile));
    assert_eq!(
        build_error.api_error_code_override(),
        Some(ApiErrorCode::ManagedIngressDriverUnsupported)
    );
    assert!(
        build_error
            .message()
            .contains("do not support the remote driver")
    );
}

#[tokio::test]
async fn driver_builder_rejects_tencent_cos_remote_storage_targets() {
    let state = setup_state().await;
    let profile = model_with_driver(DriverType::TencentCos);

    let validate_error = validate_driver_from_target(&state, &profile).unwrap_err();
    assert_eq!(
        validate_error.api_error_code_override(),
        Some(ApiErrorCode::ManagedIngressDriverUnsupported)
    );
    assert!(
        validate_error
            .message()
            .contains("do not support the tencent_cos driver")
    );
    let build_error = expect_aster_err(build_driver_from_target(&state, &profile));
    assert_eq!(
        build_error.api_error_code_override(),
        Some(ApiErrorCode::ManagedIngressDriverUnsupported)
    );
    assert!(
        build_error
            .message()
            .contains("do not support the tencent_cos driver")
    );
}

#[tokio::test]
async fn create_sets_first_profile_as_default_and_applies_local_driver() {
    let state = setup_state().await;
    let binding = create_binding(&state, "ak-first").await;

    let profile = create(
        &state,
        &binding,
        local_create(" First ", " first/profile ", false),
    )
    .await
    .unwrap();

    assert!(profile.target_key.starts_with("rst_"));
    assert_eq!(profile.name, "First");
    assert_eq!(profile.base_path, "first/profile");
    assert!(profile.is_default);
    assert_eq!(profile.desired_revision, 1);
    assert_eq!(profile.applied_revision, 1);
    assert_eq!(profile.last_error, "");

    let resolved = resolve_effective_target(&state, &binding).await.unwrap();
    assert!(resolved.driver.exists(".").await.is_ok());
}

#[tokio::test]
async fn update_can_promote_second_profile_to_default_and_increments_revision() {
    let state = setup_state().await;
    let binding = create_binding(&state, "ak-update").await;
    let first = create(&state, &binding, local_create("First", "first", false))
        .await
        .unwrap();
    let second = create(&state, &binding, local_create("Second", "second", false))
        .await
        .unwrap();
    assert!(first.is_default);
    assert!(!second.is_default);

    let updated = update(
        &state,
        &binding,
        &second.target_key,
        RemoteUpdateStorageTargetRequest {
            name: Some(" Promoted ".to_string()),
            base_path: Some(" promoted ".to_string()),
            is_default: Some(true),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    assert!(updated.is_default);
    assert_eq!(updated.name, "Promoted");
    assert_eq!(updated.base_path, "promoted");
    assert_eq!(updated.desired_revision, 2);
    assert_eq!(updated.applied_revision, 2);

    let profiles = list(&state, &binding).await.unwrap();
    assert_eq!(profiles.len(), 2);
    assert_eq!(profiles[0].target_key, updated.target_key);
    assert!(profiles[0].is_default);
    assert!(!profiles[1].is_default);
}

#[tokio::test]
async fn update_rejects_unsetting_current_default_directly() {
    let state = setup_state().await;
    let binding = create_binding(&state, "ak-unset").await;
    let profile = create(&state, &binding, local_create("Default", "default", true))
        .await
        .unwrap();

    let error = update(
        &state,
        &binding,
        &profile.target_key,
        RemoteUpdateStorageTargetRequest {
            is_default: Some(false),
            ..Default::default()
        },
    )
    .await
    .unwrap_err();

    assert_eq!(
        error.api_error_code_override(),
        Some(ApiErrorCode::ManagedIngressDefaultUpdateRequiresReplacement)
    );
}

#[tokio::test]
async fn delete_protects_default_when_other_profiles_exist_then_allows_after_replacement() {
    let state = setup_state().await;
    let binding = create_binding(&state, "ak-delete").await;
    let first = create(&state, &binding, local_create("First", "first", true))
        .await
        .unwrap();
    let second = create(&state, &binding, local_create("Second", "second", false))
        .await
        .unwrap();

    let error = delete(&state, &binding, &first.target_key)
        .await
        .unwrap_err();
    assert_eq!(
        error.api_error_code_override(),
        Some(ApiErrorCode::ManagedIngressDefaultDeleteRequiresReplacement)
    );

    update(
        &state,
        &binding,
        &second.target_key,
        RemoteUpdateStorageTargetRequest {
            is_default: Some(true),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    delete(&state, &binding, &first.target_key).await.unwrap();

    let profiles = list(&state, &binding).await.unwrap();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].target_key, second.target_key);
    assert!(profiles[0].is_default);
}

#[tokio::test]
async fn resolve_effective_target_reports_required_default_and_pending_states() {
    let state = setup_state().await;
    let binding = create_binding(&state, "ak-resolve").await;

    let missing_error = expect_aster_err(resolve_effective_target(&state, &binding).await);
    assert_eq!(
        missing_error.api_error_code_override(),
        Some(ApiErrorCode::ManagedIngressRequired)
    );

    let profile = create(&state, &binding, local_create("Default", "default", true))
        .await
        .unwrap();
    let mut stored = remote_storage_target_repo::find_by_binding_and_target_key(
        state.writer_db(),
        binding.id,
        &profile.target_key,
    )
    .await
    .unwrap()
    .unwrap();
    let mut active: remote_storage_target::ActiveModel = stored.clone().into();
    active.last_error = Set("path failed".to_string());
    remote_storage_target_repo::update(state.writer_db(), active)
        .await
        .unwrap();
    let error = expect_aster_err(resolve_effective_target(&state, &binding).await);
    assert_eq!(
        error.api_error_code_override(),
        Some(ApiErrorCode::ManagedIngressDefaultError)
    );

    stored = remote_storage_target_repo::find_by_binding_and_target_key(
        state.writer_db(),
        binding.id,
        &profile.target_key,
    )
    .await
    .unwrap()
    .unwrap();
    let mut active: remote_storage_target::ActiveModel = stored.into();
    active.last_error = Set(String::new());
    active.applied_revision = Set(0);
    active.desired_revision = Set(1);
    remote_storage_target_repo::update(state.writer_db(), active)
        .await
        .unwrap();
    let error = expect_aster_err(resolve_effective_target(&state, &binding).await);
    assert_eq!(
        error.api_error_code_override(),
        Some(ApiErrorCode::ManagedIngressDefaultNotApplied)
    );
}

#[tokio::test]
async fn resolve_target_by_key_reports_missing_error_and_unready_states() {
    let state = setup_state().await;
    let binding = create_binding(&state, "ak-resolve-key").await;

    let missing_error =
        expect_aster_err(resolve_target_by_key(&state, &binding, "rst_missing").await);
    assert_eq!(
        missing_error.api_error_code_override(),
        Some(ApiErrorCode::RemoteStorageTargetNotFound)
    );

    let profile = create(&state, &binding, local_create("Keyed", "keyed", true))
        .await
        .unwrap();
    let stored = remote_storage_target_repo::find_by_binding_and_target_key(
        state.writer_db(),
        binding.id,
        &profile.target_key,
    )
    .await
    .unwrap()
    .unwrap();
    let mut active: remote_storage_target::ActiveModel = stored.into();
    active.last_error = Set("apply failed".to_string());
    remote_storage_target_repo::update(state.writer_db(), active)
        .await
        .unwrap();
    let error =
        expect_aster_err(resolve_target_by_key(&state, &binding, &profile.target_key).await);
    assert_eq!(
        error.api_error_code_override(),
        Some(ApiErrorCode::ManagedIngressDefaultError)
    );

    let stored = remote_storage_target_repo::find_by_binding_and_target_key(
        state.writer_db(),
        binding.id,
        &profile.target_key,
    )
    .await
    .unwrap()
    .unwrap();
    let mut active: remote_storage_target::ActiveModel = stored.into();
    active.last_error = Set(String::new());
    active.applied_revision = Set(0);
    active.desired_revision = Set(1);
    remote_storage_target_repo::update(state.writer_db(), active)
        .await
        .unwrap();
    let error =
        expect_aster_err(resolve_target_by_key(&state, &binding, &profile.target_key).await);
    assert_eq!(
        error.api_error_code_override(),
        Some(ApiErrorCode::ManagedIngressDefaultNotApplied)
    );
}

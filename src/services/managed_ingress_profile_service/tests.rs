use super::{
    create, delete,
    driver::{build_driver_from_profile, validate_driver_from_profile},
    list,
    normalization::{normalize_create_input, normalize_update_input},
    paths::{normalize_relative_local_path, resolve_managed_local_path},
    resolve_effective_target, update,
};
use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{managed_ingress_profile_repo, master_binding_repo};
use crate::entities::{managed_ingress_profile, master_binding};
use crate::metrics_core::SharedMetricsRecorder;
use crate::runtime::{FollowerRuntimeState, SharedRuntimeState};
use crate::storage::remote_protocol::{
    RemoteCreateIngressProfileRequest, RemoteCreateLocalIngressProfileRequest,
    RemoteCreateS3IngressProfileRequest, RemoteUpdateIngressProfileRequest,
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
        "aster-managed-ingress-service-root-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&root).unwrap();
    let config = Arc::new(crate::config::Config {
        server: crate::config::ServerConfig {
            follower: crate::config::ServerFollowerConfig {
                managed_ingress_local_root: root.to_string_lossy().into_owned(),
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

fn local_create(
    name: &str,
    base_path: &str,
    max_file_size: i64,
    is_default: bool,
) -> RemoteCreateIngressProfileRequest {
    RemoteCreateIngressProfileRequest::Local(RemoteCreateLocalIngressProfileRequest {
        name: name.to_string(),
        base_path: base_path.to_string(),
        max_file_size,
        is_default,
    })
}

fn s3_create(
    name: &str,
    endpoint: &str,
    bucket: &str,
    base_path: &str,
    is_default: bool,
) -> RemoteCreateIngressProfileRequest {
    RemoteCreateIngressProfileRequest::S3(RemoteCreateS3IngressProfileRequest {
        name: name.to_string(),
        endpoint: endpoint.to_string(),
        bucket: bucket.to_string(),
        access_key: "access".to_string(),
        secret_key: "secret".to_string(),
        base_path: base_path.to_string(),
        max_file_size: 0,
        is_default,
    })
}

fn model_with_driver(driver_type: DriverType) -> managed_ingress_profile::Model {
    let now = Utc::now();
    managed_ingress_profile::Model {
        id: 1,
        master_binding_id: 1,
        profile_key: "igp_test".to_string(),
        name: "test".to_string(),
        driver_type,
        endpoint: String::new(),
        bucket: "bucket".to_string(),
        access_key: "access".to_string(),
        secret_key: "secret".to_string(),
        base_path: "profile".to_string(),
        max_file_size: 0,
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
            .contains("server.follower.managed_ingress_local_root")
    );
}

#[test]
fn normalize_relative_local_path_rejects_backslash_escape_attempts() {
    let error = normalize_relative_local_path("..\\secret").unwrap_err();
    assert!(
        error
            .message()
            .contains("server.follower.managed_ingress_local_root")
    );
}

#[test]
fn resolve_managed_local_path_allows_missing_child_inside_root() {
    let root = std::env::temp_dir().join(format!(
        "aster-managed-ingress-root-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&root).unwrap();

    let resolved = resolve_managed_local_path(root.to_str().unwrap(), "profiles/new").unwrap();
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
fn resolve_managed_local_path_rejects_symlink_escape() {
    let root = std::env::temp_dir().join(format!(
        "aster-managed-ingress-root-{}",
        uuid::Uuid::new_v4()
    ));
    let outside = std::env::temp_dir().join(format!(
        "aster-managed-ingress-outside-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();

    let error = resolve_managed_local_path(root.to_str().unwrap(), "escape/profile").unwrap_err();
    assert!(
        error
            .message()
            .contains("server.follower.managed_ingress_local_root")
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
fn resolve_managed_local_path_rejects_empty_root() {
    let error = resolve_managed_local_path("   ", "profile").unwrap_err();
    assert!(
        error
            .message()
            .contains("managed_ingress_local_root cannot be empty")
    );
}

#[test]
fn normalize_create_input_trims_local_and_s3_fields() {
    let local = normalize_create_input(local_create(" Local ", " ./dropbox/ ", 42, true)).unwrap();
    assert_eq!(local.name, "Local");
    assert_eq!(local.driver_type, DriverType::Local);
    assert_eq!(local.base_path, "dropbox");
    assert_eq!(local.max_file_size, 42);
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
    let error = expect_aster_err(normalize_create_input(local_create(
        " ", "profile", 0, false,
    )));
    assert!(error.message().contains("name cannot be blank"));

    let error = expect_aster_err(normalize_create_input(local_create(
        "Local", "profile", -1, false,
    )));
    assert!(
        error
            .message()
            .contains("max_file_size must be non-negative")
    );

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
        RemoteUpdateIngressProfileRequest {
            name: Some(" Updated ".to_string()),
            base_path: Some(" /next/ ".to_string()),
            max_file_size: Some(128),
            is_default: Some(true),
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(normalized.name, "Updated");
    assert_eq!(normalized.driver_type, DriverType::S3);
    assert_eq!(normalized.endpoint, existing.endpoint);
    assert_eq!(normalized.bucket, existing.bucket);
    assert_eq!(normalized.base_path, "next");
    assert_eq!(normalized.max_file_size, 128);
    assert_eq!(normalized.is_default, Some(true));
}

#[test]
fn normalize_update_input_resets_driver_specific_fields_when_driver_changes() {
    let existing = model_with_driver(DriverType::S3);
    let normalized = normalize_update_input(
        existing,
        RemoteUpdateIngressProfileRequest {
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

#[tokio::test]
async fn driver_builder_rejects_remote_managed_ingress_profiles() {
    let state = setup_state().await;
    let profile = model_with_driver(DriverType::Remote);

    let validate_error = validate_driver_from_profile(&state, &profile).unwrap_err();
    assert!(
        validate_error
            .message()
            .contains("do not support the remote driver")
    );
    let build_error = expect_aster_err(build_driver_from_profile(&state, &profile));
    assert!(
        build_error
            .message()
            .contains("do not support the remote driver")
    );
}

#[tokio::test]
async fn driver_builder_rejects_tencent_cos_managed_ingress_profiles() {
    let state = setup_state().await;
    let profile = model_with_driver(DriverType::TencentCos);

    let validate_error = validate_driver_from_profile(&state, &profile).unwrap_err();
    assert!(
        validate_error
            .message()
            .contains("do not support the tencent_cos driver")
    );
    let build_error = expect_aster_err(build_driver_from_profile(&state, &profile));
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
        local_create(" First ", " first/profile ", 512, false),
    )
    .await
    .unwrap();

    assert!(profile.profile_key.starts_with("igp_"));
    assert_eq!(profile.name, "First");
    assert_eq!(profile.base_path, "first/profile");
    assert_eq!(profile.max_file_size, 512);
    assert!(profile.is_default);
    assert_eq!(profile.desired_revision, 1);
    assert_eq!(profile.applied_revision, 1);
    assert_eq!(profile.last_error, "");

    let resolved = resolve_effective_target(&state, &binding).await.unwrap();
    assert_eq!(resolved.max_file_size, 512);
    assert!(resolved.driver.exists(".").await.is_ok());
}

#[tokio::test]
async fn update_can_promote_second_profile_to_default_and_increments_revision() {
    let state = setup_state().await;
    let binding = create_binding(&state, "ak-update").await;
    let first = create(&state, &binding, local_create("First", "first", 0, false))
        .await
        .unwrap();
    let second = create(&state, &binding, local_create("Second", "second", 0, false))
        .await
        .unwrap();
    assert!(first.is_default);
    assert!(!second.is_default);

    let updated = update(
        &state,
        &binding,
        &second.profile_key,
        RemoteUpdateIngressProfileRequest {
            name: Some(" Promoted ".to_string()),
            base_path: Some(" promoted ".to_string()),
            max_file_size: Some(2048),
            is_default: Some(true),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    assert!(updated.is_default);
    assert_eq!(updated.name, "Promoted");
    assert_eq!(updated.base_path, "promoted");
    assert_eq!(updated.max_file_size, 2048);
    assert_eq!(updated.desired_revision, 2);
    assert_eq!(updated.applied_revision, 2);

    let profiles = list(&state, &binding).await.unwrap();
    assert_eq!(profiles.len(), 2);
    assert_eq!(profiles[0].profile_key, updated.profile_key);
    assert!(profiles[0].is_default);
    assert!(!profiles[1].is_default);
}

#[tokio::test]
async fn update_rejects_unsetting_current_default_directly() {
    let state = setup_state().await;
    let binding = create_binding(&state, "ak-unset").await;
    let profile = create(
        &state,
        &binding,
        local_create("Default", "default", 0, true),
    )
    .await
    .unwrap();

    let error = update(
        &state,
        &binding,
        &profile.profile_key,
        RemoteUpdateIngressProfileRequest {
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
    let first = create(&state, &binding, local_create("First", "first", 0, true))
        .await
        .unwrap();
    let second = create(&state, &binding, local_create("Second", "second", 0, false))
        .await
        .unwrap();

    let error = delete(&state, &binding, &first.profile_key)
        .await
        .unwrap_err();
    assert_eq!(
        error.api_error_code_override(),
        Some(ApiErrorCode::ManagedIngressDefaultDeleteRequiresReplacement)
    );

    update(
        &state,
        &binding,
        &second.profile_key,
        RemoteUpdateIngressProfileRequest {
            is_default: Some(true),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    delete(&state, &binding, &first.profile_key).await.unwrap();

    let profiles = list(&state, &binding).await.unwrap();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].profile_key, second.profile_key);
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

    let profile = create(
        &state,
        &binding,
        local_create("Default", "default", 0, true),
    )
    .await
    .unwrap();
    let mut stored = managed_ingress_profile_repo::find_by_binding_and_profile_key(
        state.writer_db(),
        binding.id,
        &profile.profile_key,
    )
    .await
    .unwrap()
    .unwrap();
    let mut active: managed_ingress_profile::ActiveModel = stored.clone().into();
    active.last_error = Set("path failed".to_string());
    managed_ingress_profile_repo::update(state.writer_db(), active)
        .await
        .unwrap();
    let error = expect_aster_err(resolve_effective_target(&state, &binding).await);
    assert_eq!(
        error.api_error_code_override(),
        Some(ApiErrorCode::ManagedIngressDefaultError)
    );

    stored = managed_ingress_profile_repo::find_by_binding_and_profile_key(
        state.writer_db(),
        binding.id,
        &profile.profile_key,
    )
    .await
    .unwrap()
    .unwrap();
    let mut active: managed_ingress_profile::ActiveModel = stored.into();
    active.last_error = Set(String::new());
    active.applied_revision = Set(0);
    active.desired_revision = Set(1);
    managed_ingress_profile_repo::update(state.writer_db(), active)
        .await
        .unwrap();
    let error = expect_aster_err(resolve_effective_target(&state, &binding).await);
    assert_eq!(
        error.api_error_code_override(),
        Some(ApiErrorCode::ManagedIngressDefaultNotApplied)
    );
}

use super::{
    create, delete,
    driver::list_registered_remote_storage_target_connector_descriptors,
    list,
    normalization::{normalize_create_input, normalize_update_input},
    paths::{normalize_relative_local_path, resolve_remote_storage_target_local_path},
    resolve_effective_target, resolve_target_by_key, update,
};
use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{master_binding_repo, remote_storage_target_repo};
use crate::runtime::{FollowerRuntimeState, SharedRuntimeState, StorageConnectorRuntimeState};
use crate::storage::remote_protocol::{
    RemoteCreateStorageTargetRequest, RemoteUpdateStorageTargetRequest,
};
use aster_drive_metrics::SharedMetricsRecorder;
use aster_drive_model::entities::{master_binding, remote_storage_target};
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
    cache: Arc<dyn aster_forge_cache::CacheBackend>,
    config_sync: aster_forge_config::ConfigSyncRuntime,
    metrics: SharedMetricsRecorder,
}

impl StorageConnectorRuntimeState for TestFollowerState {
    fn writer_db(&self) -> &DatabaseConnection {
        &self.db
    }

    fn driver_registry(&self) -> &Arc<crate::storage::DriverRegistry> {
        &self.driver_registry
    }

    fn runtime_config(&self) -> &Arc<crate::config::RuntimeConfig> {
        &self.runtime_config
    }

    fn config(&self) -> &Arc<crate::config::Config> {
        &self.config
    }
}

impl SharedRuntimeState for TestFollowerState {
    fn reader_db(&self) -> &DatabaseConnection {
        &self.db
    }

    fn policy_snapshot(&self) -> &Arc<crate::storage::PolicySnapshot> {
        &self.policy_snapshot
    }

    fn cache(&self) -> &Arc<dyn aster_forge_cache::CacheBackend> {
        &self.cache
    }

    fn config_sync(&self) -> &aster_forge_config::ConfigSyncRuntime {
        &self.config_sync
    }

    fn metrics(&self) -> &SharedMetricsRecorder {
        &self.metrics
    }
}

impl FollowerRuntimeState for TestFollowerState {}

async fn setup_state() -> TestFollowerState {
    let db = crate::db::connect_with_metrics(
        &crate::config::DatabaseConfig {
            url: "sqlite::memory:".into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    aster_drive_migration::Migrator::up(&db, None)
        .await
        .unwrap();

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
    let cache = aster_forge_cache::create_cache(&aster_forge_cache::CacheConfig {
        ..Default::default()
    })
    .await;

    TestFollowerState {
        db,
        driver_registry: Arc::new(
            crate::storage::DriverRegistry::noop().expect("built-in storage connector registry"),
        ),
        runtime_config: Arc::new(crate::config::RuntimeConfig::new()),
        policy_snapshot: Arc::new(crate::storage::PolicySnapshot::new()),
        config,
        cache,
        config_sync: aster_forge_config::ConfigSyncRuntime::disabled_for_test("aster_drive"),
        metrics: aster_drive_metrics::NoopMetrics::arc(),
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
    RemoteCreateStorageTargetRequest {
        name: name.to_string(),
        connection: crate::storage::StorageConnectionInput {
            connector_config: aster_drive_storage::ConnectorConfigEnvelope::new(
                aster_drive_storage::ConnectorId::declared("asterdrive.storage.local"),
                1,
                [("base_path".to_string(), serde_json::json!(base_path))]
                    .into_iter()
                    .collect(),
            ),
            credential: crate::storage::StorageConnectorCredentialInput::None,
        },
        is_default,
    }
}

fn s3_create(
    name: &str,
    endpoint: &str,
    bucket: &str,
    base_path: &str,
    is_default: bool,
) -> RemoteCreateStorageTargetRequest {
    RemoteCreateStorageTargetRequest {
        name: name.to_string(),
        connection: crate::storage::StorageConnectionInput {
            connector_config: aster_drive_storage::ConnectorConfigEnvelope::new(
                aster_drive_storage::ConnectorId::declared("asterdrive.storage.s3"),
                1,
                [
                    ("endpoint".to_string(), serde_json::json!(endpoint)),
                    ("bucket".to_string(), serde_json::json!(bucket)),
                    ("base_path".to_string(), serde_json::json!(base_path)),
                ]
                .into_iter()
                .collect(),
            ),
            credential: crate::storage::StorageConnectorCredentialInput::Static(
                serde_json::json!({
                    "s3_access_key_id": "access",
                    "s3_secret_access_key": "secret"
                }),
            ),
        },
        is_default,
    }
}

fn s3_model() -> remote_storage_target::Model {
    let now = Utc::now();
    remote_storage_target::Model {
        id: 1,
        master_binding_id: 1,
        target_key: "rst_test".to_string(),
        name: "test".to_string(),
        connector_id: Some("asterdrive.storage.s3".to_string()),
        connector_config: Some(
            aster_drive_storage::encode_connector_config(
                aster_drive_storage::ConnectorId::declared("asterdrive.storage.s3"),
                1,
                serde_json::json!({
                    "endpoint": "https://s3.example.test",
                    "bucket": "bucket",
                    "base_path": "profile"
                }),
            )
            .unwrap(),
        ),
        driver_type: String::new(),
        endpoint: String::new(),
        bucket: String::new(),
        access_key: String::new(),
        secret_key: String::new(),
        base_path: String::new(),
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

#[tokio::test]
async fn normalize_create_input_uses_connector_validation_and_envelope() {
    let state = setup_state().await;
    let normalized = normalize_create_input(&state, local_create(" Local ", " ./dropbox/ ", true))
        .await
        .unwrap();
    assert_eq!(normalized.name, "Local");
    assert_eq!(normalized.is_default, Some(true));
    let connection = normalized.connection.unwrap();
    assert_eq!(
        connection.connector_config.connector_id.as_str(),
        "asterdrive.storage.local"
    );
    assert_eq!(connection.connector_config.values["base_path"], "dropbox");
}

#[tokio::test]
async fn normalize_create_input_rejects_invalid_connector_values() {
    let state = setup_state().await;
    let error = normalize_create_input(
        &state,
        s3_create("S3", "https://s3.example.com", "", "", false),
    )
    .await
    .unwrap_err();
    assert!(error.message().contains("bucket"));
}

#[tokio::test]
async fn normalize_update_input_keeps_connection_opaque_when_omitted() {
    let state = setup_state().await;
    let existing = s3_model();
    let normalized = normalize_update_input(
        &state,
        &existing,
        RemoteUpdateStorageTargetRequest {
            connection: None,
            name: Some(" Updated ".to_string()),
            is_default: Some(true),
        },
    )
    .await
    .unwrap();
    assert_eq!(normalized.name, "Updated");
    assert!(normalized.connection.is_none());
    assert_eq!(normalized.is_default, Some(true));
}

#[tokio::test]
async fn normalize_update_input_merges_partial_static_credentials_from_saved_connection() {
    let state = setup_state().await;
    let binding = create_binding(&state, "ak-credential-merge").await;
    let created = create(
        &state,
        &binding,
        s3_create(
            "Archive",
            "https://s3.example.test",
            "bucket",
            "prefix",
            true,
        ),
    )
    .await
    .unwrap();
    let existing = remote_storage_target_repo::find_by_binding_and_target_key(
        state.writer_db(),
        binding.id,
        &created.target_key,
    )
    .await
    .unwrap()
    .unwrap();
    let connector_config = created.connector_config;

    let normalized = normalize_update_input(
        &state,
        &existing,
        RemoteUpdateStorageTargetRequest {
            connection: Some(crate::storage::StorageConnectionInput {
                connector_config,
                credential: crate::storage::StorageConnectorCredentialInput::Static(
                    serde_json::json!({"s3_access_key_id": "rotated"}),
                ),
            }),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let crate::storage::StorageConnectorCredentialInput::Static(values) =
        normalized.connection.unwrap().credential
    else {
        panic!("S3 credential should remain static")
    };
    assert_eq!(values["s3_access_key_id"], "rotated");
    assert_eq!(values["s3_secret_access_key"], "secret");
}

#[test]
fn remote_storage_target_registry_contains_supported_builtin_connectors() {
    assert_eq!(
        list_registered_remote_storage_target_connector_descriptors()
            .unwrap()
            .into_iter()
            .map(|descriptor| descriptor.connector_id.to_string())
            .collect::<Vec<_>>(),
        vec![
            "asterdrive.storage.local",
            "asterdrive.storage.s3",
            "asterdrive.storage.alibaba_oss",
            "asterdrive.storage.sftp",
            "asterdrive.storage.azure_blob",
            "asterdrive.storage.huawei_obs",
            "asterdrive.storage.tencent_cos",
            "asterdrive.storage.qiniu",
        ]
    );
}

#[test]
fn remote_storage_target_connector_descriptors_cover_builtin_fields() {
    let descriptors = list_registered_remote_storage_target_connector_descriptors()
        .expect("registered remote storage target descriptors should build");
    assert_eq!(descriptors.len(), 8);

    let local = descriptors
        .iter()
        .find(|descriptor| descriptor.connector_id.as_str() == "asterdrive.storage.local")
        .expect("local remote storage target descriptor should be registered");
    assert!(local.fields.iter().any(|field| field.name == "base_path"));
    let local_base_path = local
        .fields
        .iter()
        .find(|field| field.name == "base_path")
        .expect("local base_path descriptor should exist");
    assert_eq!(local_base_path.name, "base_path");

    let s3 = descriptors
        .iter()
        .find(|descriptor| descriptor.connector_id.as_str() == "asterdrive.storage.s3")
        .expect("s3 remote storage target descriptor should be registered");
    for field in ["endpoint", "bucket", "base_path"] {
        assert!(s3.fields.iter().any(|candidate| candidate.name == field));
    }
    assert!(s3.fields.iter().any(|field| field.secret));
    let s3_base_path = s3
        .fields
        .iter()
        .find(|field| field.name == "base_path")
        .expect("s3 base_path descriptor should exist");
    assert_eq!(s3_base_path.name, "base_path");
}

#[tokio::test]
async fn provider_target_registration_normalizes_through_the_same_contract() {
    let state = setup_state().await;
    let normalized = normalize_create_input(
        &state,
        RemoteCreateStorageTargetRequest {
            name: " SFTP archive ".to_string(),
            connection: crate::storage::StorageConnectionInput {
                connector_config: aster_drive_storage::ConnectorConfigEnvelope::new(
                    aster_drive_storage::ConnectorId::declared("asterdrive.storage.sftp"),
                    1,
                    [
                        ("endpoint".to_string(), serde_json::json!("sftp://HOST:22")),
                        ("base_path".to_string(), serde_json::json!("incoming/")),
                    ]
                    .into_iter()
                    .collect(),
                ),
                credential: crate::storage::StorageConnectorCredentialInput::Static(
                    serde_json::json!({
                        "sftp_username": "user",
                        "sftp_password": "password"
                    }),
                ),
            },
            is_default: false,
        },
    )
    .await
    .expect("provider adapter should use generic target normalization");
    assert_eq!(normalized.name, "SFTP archive");
    let connection = normalized.connection.unwrap();
    assert_eq!(
        connection.connector_config.connector_id.as_str(),
        "asterdrive.storage.sftp"
    );
}

#[tokio::test]
async fn connector_envelope_rejects_unknown_provider_without_s3_fallback() {
    let state = setup_state().await;
    let request = RemoteCreateStorageTargetRequest {
        name: "Future".to_string(),
        connection: crate::storage::StorageConnectionInput {
            connector_config: aster_drive_storage::ConnectorConfigEnvelope::new(
                aster_drive_storage::ConnectorId::declared("com.example.future"),
                1,
                serde_json::from_value(serde_json::json!({"endpoint": "https://HOST"})).unwrap(),
            ),
            credential: crate::storage::StorageConnectorCredentialInput::None,
        },
        is_default: false,
    };
    let error = normalize_create_input(&state, request).await.unwrap_err();
    assert!(error.message().contains("com.example.future"));
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
    assert_eq!(
        profile.connector_config.values["base_path"],
        "first/profile"
    );
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
            connection: Some(local_create("Promoted", " promoted ", true).connection),
            name: Some(" Promoted ".to_string()),
            is_default: Some(true),
        },
    )
    .await
    .unwrap();

    assert!(updated.is_default);
    assert_eq!(updated.name, "Promoted");
    assert_eq!(updated.connector_config.values["base_path"], "promoted");
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
            connection: None,
            is_default: Some(false),
            ..Default::default()
        },
    )
    .await
    .unwrap_err();

    assert_eq!(
        error.api_error_code_override(),
        Some(ApiErrorCode::RemoteStorageTargetDefaultUpdateRequiresReplacement)
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
        Some(ApiErrorCode::RemoteStorageTargetDefaultDeleteRequiresReplacement)
    );

    update(
        &state,
        &binding,
        &second.target_key,
        RemoteUpdateStorageTargetRequest {
            connection: None,
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
        Some(ApiErrorCode::RemoteStorageTargetRequired)
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
        Some(ApiErrorCode::RemoteStorageTargetDefaultError)
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
        Some(ApiErrorCode::RemoteStorageTargetDefaultNotApplied)
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
        Some(ApiErrorCode::RemoteStorageTargetDefaultError)
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
        Some(ApiErrorCode::RemoteStorageTargetDefaultNotApplied)
    );
}

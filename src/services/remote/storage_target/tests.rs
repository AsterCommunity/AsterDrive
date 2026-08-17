use std::{collections::BTreeMap, fs, sync::Arc};

use aster_drive_metrics::SharedMetricsRecorder;
use aster_drive_model::entities::{master_binding, remote_storage_target};
use aster_drive_storage::{ConnectorConfigEnvelope, ConnectorId, StorageConnectorFieldScope};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, Set, TransactionTrait};
use serde_json::{Value, json};

use super::{
    create, credential, delete,
    driver::{
        LOCAL_CONNECTOR_ID, S3_CONNECTOR_ID, SCHEMA_VERSION,
        list_registered_remote_storage_target_connector_descriptors,
        registered_remote_storage_target_connector_ids,
    },
    list,
    migration::migrate_legacy_remote_storage_targets,
    paths::{normalize_relative_local_path, resolve_remote_storage_target_local_path},
    resolve_effective_target, update,
};
use crate::{
    api::api_error_code::ApiErrorCode,
    db::repository::{
        master_binding_repo, remote_storage_target_credential_repo, remote_storage_target_repo,
    },
    runtime::{FollowerRuntimeState, SharedRuntimeState, StorageConnectorRuntimeState},
    storage::remote_protocol::{
        RemoteCreateStorageTargetRequest, RemoteStorageTargetCredentialInput,
        RemoteUpdateStorageTargetRequest,
    },
};

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
    let root = std::env::temp_dir().join(format!("aster-target-{}", uuid::Uuid::new_v4()));
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
    TestFollowerState {
        db,
        driver_registry: Arc::new(crate::storage::DriverRegistry::noop().unwrap()),
        runtime_config: Arc::new(crate::config::RuntimeConfig::new()),
        policy_snapshot: Arc::new(crate::storage::PolicySnapshot::new()),
        config,
        cache: aster_forge_cache::create_cache(&Default::default()).await,
        config_sync: aster_forge_config::ConfigSyncRuntime::disabled_for_test("aster_drive"),
        metrics: aster_drive_metrics::NoopMetrics::arc(),
    }
}
async fn binding(state: &TestFollowerState) -> master_binding::Model {
    let now = Utc::now();
    master_binding_repo::create(
        state.writer_db(),
        master_binding::ActiveModel {
            name: Set("binding".into()),
            master_url: Set("https://primary.example".into()),
            access_key: Set(uuid::Uuid::new_v4().to_string()),
            secret_key: Set("secret".into()),
            storage_namespace: Set(uuid::Uuid::new_v4().to_string()),
            is_enabled: Set(true),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .unwrap()
}
async fn insert_legacy_target(
    state: &TestFollowerState,
    binding_id: i64,
    target_key: &str,
    driver_type: &str,
    access_key: &str,
    secret_key: &str,
) -> remote_storage_target::Model {
    let now = Utc::now();
    remote_storage_target::ActiveModel {
        master_binding_id: Set(binding_id),
        target_key: Set(target_key.into()),
        name: Set(format!("Legacy {target_key}")),
        connector_id: Set(String::new()),
        connector_config: Set(String::new()),
        driver_type: Set(driver_type.into()),
        endpoint: Set("https://s3.example".into()),
        bucket: Set("bucket".into()),
        access_key: Set(access_key.into()),
        secret_key: Set(secret_key.into()),
        base_path: Set("prefix".into()),
        is_default: Set(false),
        desired_revision: Set(1),
        applied_revision: Set(1),
        last_error: Set(String::new()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .unwrap()
}
fn values(entries: &[(&str, Value)]) -> BTreeMap<String, Value> {
    entries
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect()
}
fn config(id: &str, entries: &[(&str, Value)]) -> ConnectorConfigEnvelope {
    ConnectorConfigEnvelope::new(ConnectorId::declared(id), SCHEMA_VERSION, values(entries))
}
fn local(name: &str, path: &str, default: bool) -> RemoteCreateStorageTargetRequest {
    RemoteCreateStorageTargetRequest {
        name: name.into(),
        connector_config: config(LOCAL_CONNECTOR_ID, &[("base_path", json!(path))]),
        credential: None,
        is_default: default,
    }
}
fn s3(name: &str, default: bool) -> RemoteCreateStorageTargetRequest {
    RemoteCreateStorageTargetRequest {
        name: name.into(),
        connector_config: config(
            S3_CONNECTOR_ID,
            &[
                ("endpoint", json!("https://s3.example")),
                ("bucket", json!("bucket")),
                ("base_path", json!("prefix")),
            ],
        ),
        credential: Some(RemoteStorageTargetCredentialInput {
            mode: "static".into(),
            values: values(&[
                ("s3_access_key_id", json!("access")),
                ("s3_secret_access_key", json!("secret")),
            ]),
        }),
        is_default: default,
    }
}

#[test]
fn connector_registry_exposes_scoped_generic_fields() {
    assert_eq!(
        registered_remote_storage_target_connector_ids(),
        vec![LOCAL_CONNECTOR_ID, S3_CONNECTOR_ID]
    );
    let descriptors = list_registered_remote_storage_target_connector_descriptors();
    let s3 = descriptors
        .iter()
        .find(|d| d.connector_id.as_str() == S3_CONNECTOR_ID)
        .unwrap();
    assert!(s3.fields.iter().any(|f| f.name == "endpoint" && f.scope == StorageConnectorFieldScope::ConnectorConfig));
    assert!(s3.fields.iter().any(|f| f.name == "s3_secret_access_key"
        && f.scope == StorageConnectorFieldScope::StaticCredential
        && f.secret));
    assert!(!s3.fields.iter().any(|f| f.name == "is_default"));
}

#[test]
fn local_paths_reject_escape_and_resolve_beneath_root() {
    assert_eq!(
        normalize_relative_local_path(" ./archive/2026 ").unwrap(),
        "archive/2026"
    );
    assert!(normalize_relative_local_path("../secret").is_err());
    let root = std::env::temp_dir().join(format!("aster-path-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let resolved =
        resolve_remote_storage_target_local_path(root.to_str().unwrap(), "next").unwrap();
    assert!(resolved.starts_with(fs::canonicalize(root).unwrap()));
}

#[tokio::test]
async fn local_crud_uses_connector_envelope_and_revision_contract() {
    let state = setup_state().await;
    let binding = binding(&state).await;
    let created = create(&state, &binding, local(" Local ", " ./dropbox/ ", false))
        .await
        .unwrap();
    assert_eq!(created.connector_id, LOCAL_CONNECTOR_ID);
    assert_eq!(
        created.connector_config.values["base_path"],
        json!("dropbox")
    );
    assert!(created.is_default && !created.credential_configured && created.connector_available);
    let updated = update(
        &state,
        &binding,
        &created.target_key,
        RemoteUpdateStorageTargetRequest {
            name: Some("Renamed".into()),
            connector_config: Some(config(LOCAL_CONNECTOR_ID, &[("base_path", json!("next"))])),
            is_default: Some(true),
            credential: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(updated.desired_revision, 2);
    assert_eq!(updated.applied_revision, 2);
    assert_eq!(updated.connector_config.values["base_path"], json!("next"));
    assert!(
        resolve_effective_target(&state, &binding)
            .await
            .unwrap()
            .driver
            .exists(".")
            .await
            .unwrap()
    );
    delete(&state, &binding, &created.target_key).await.unwrap();
    assert!(list(&state, &binding).await.unwrap().is_empty());
}

#[tokio::test]
async fn s3_secret_is_encrypted_not_echoed_and_preserved_on_edit() {
    let state = setup_state().await;
    let binding = binding(&state).await;
    let created = create(&state, &binding, s3("S3", true)).await.unwrap();
    assert!(created.credential_configured);
    let row = remote_storage_target_repo::find_by_binding_and_target_key(
        state.writer_db(),
        binding.id,
        &created.target_key,
    )
    .await
    .unwrap()
    .unwrap();
    assert!(row.access_key.is_empty() && row.secret_key.is_empty());
    let secret = remote_storage_target_credential_repo::find_by_target(state.writer_db(), row.id)
        .await
        .unwrap()
        .unwrap();
    assert!(!secret.ciphertext.contains("access") && !secret.ciphertext.contains("secret"));
    let updated = update(
        &state,
        &binding,
        &created.target_key,
        RemoteUpdateStorageTargetRequest {
            name: Some("S3 renamed".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(updated.credential_configured);
    let plaintext = credential::decrypt(
        &state.config().auth.storage_credential_secret_key,
        row.id,
        S3_CONNECTOR_ID,
        SCHEMA_VERSION,
        &secret.ciphertext,
    )
    .unwrap();
    assert!(plaintext.contains("s3_secret_access_key"));
}

#[tokio::test]
async fn switching_connector_requires_new_credential_and_removes_old_secret_for_local() {
    let state = setup_state().await;
    let binding = binding(&state).await;
    let created = create(&state, &binding, local("Local", "local", true))
        .await
        .unwrap();
    let error = update(
        &state,
        &binding,
        &created.target_key,
        RemoteUpdateStorageTargetRequest {
            connector_config: Some(config(
                S3_CONNECTOR_ID,
                &[
                    ("endpoint", json!("https://s3.example")),
                    ("bucket", json!("bucket")),
                    ("base_path", json!("")),
                ],
            )),
            ..Default::default()
        },
    )
    .await
    .unwrap_err();
    assert!(error.message().contains("requires static credentials"));
    let switched = update(
        &state,
        &binding,
        &created.target_key,
        RemoteUpdateStorageTargetRequest {
            connector_config: Some(s3("x", true).connector_config),
            credential: s3("x", true).credential,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(switched.connector_id, S3_CONNECTOR_ID);
    let back = update(
        &state,
        &binding,
        &created.target_key,
        RemoteUpdateStorageTargetRequest {
            connector_config: Some(config(LOCAL_CONNECTOR_ID, &[("base_path", json!("back"))])),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(!back.credential_configured);
}

#[tokio::test]
async fn default_target_cannot_be_unset_or_deleted_while_replacement_exists() {
    let state = setup_state().await;
    let binding = binding(&state).await;
    let first = create(&state, &binding, local("first", "first", true))
        .await
        .unwrap();
    create(&state, &binding, local("second", "second", false))
        .await
        .unwrap();
    let error = update(
        &state,
        &binding,
        &first.target_key,
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
    let error = delete(&state, &binding, &first.target_key)
        .await
        .unwrap_err();
    assert_eq!(
        error.api_error_code_override(),
        Some(ApiErrorCode::ManagedIngressDefaultDeleteRequiresReplacement)
    );
}

#[test]
fn credential_aad_binds_target_connector_and_schema() {
    let key = "remote-target-test-master-key-32bytes";
    let encrypted = credential::encrypt(key, 7, S3_CONNECTOR_ID, 1, r#"{"x":1}"#).unwrap();
    assert_eq!(
        credential::decrypt(key, 7, S3_CONNECTOR_ID, 1, &encrypted).unwrap(),
        r#"{"x":1}"#
    );
    assert!(credential::decrypt(key, 8, S3_CONNECTOR_ID, 1, &encrypted).is_err());
    assert!(credential::decrypt(key, 7, LOCAL_CONNECTOR_ID, 1, &encrypted).is_err());
    assert!(credential::decrypt(key, 7, S3_CONNECTOR_ID, 2, &encrypted).is_err());
}

#[tokio::test]
async fn legacy_rows_convert_atomically_and_clear_plaintext() {
    let state = setup_state().await;
    let binding = binding(&state).await;
    let legacy = insert_legacy_target(
        &state,
        binding.id,
        "legacy",
        "s3",
        "plain-access",
        "plain-secret",
    )
    .await;
    let txn = state.writer_db().begin().await.unwrap();
    assert_eq!(
        migrate_legacy_remote_storage_targets(
            &txn,
            &state.config().auth.storage_credential_secret_key
        )
        .await
        .unwrap(),
        1
    );
    txn.commit().await.unwrap();
    let row = remote_storage_target_repo::find_by_id(state.writer_db(), legacy.id)
        .await
        .unwrap();
    assert_eq!(row.connector_id, S3_CONNECTOR_ID);
    assert!(row.access_key.is_empty() && row.secret_key.is_empty());
    assert!(
        remote_storage_target_credential_repo::find_by_target(state.writer_db(), row.id)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn legacy_local_row_migrates_without_credential_and_second_run_is_idempotent() {
    let state = setup_state().await;
    let binding = binding(&state).await;
    let legacy = insert_legacy_target(&state, binding.id, "local", "local", "", "").await;
    let txn = state.writer_db().begin().await.unwrap();
    assert_eq!(
        migrate_legacy_remote_storage_targets(
            &txn,
            &state.config().auth.storage_credential_secret_key,
        )
        .await
        .unwrap(),
        1
    );
    txn.commit().await.unwrap();

    let row = remote_storage_target_repo::find_by_id(state.writer_db(), legacy.id)
        .await
        .unwrap();
    assert_eq!(row.connector_id, LOCAL_CONNECTOR_ID);
    assert!(row.driver_type.is_empty() && row.base_path.is_empty());
    assert!(
        remote_storage_target_credential_repo::find_by_target(state.writer_db(), row.id)
            .await
            .unwrap()
            .is_none()
    );

    let txn = state.writer_db().begin().await.unwrap();
    assert_eq!(
        migrate_legacy_remote_storage_targets(
            &txn,
            &state.config().auth.storage_credential_secret_key,
        )
        .await
        .unwrap(),
        0
    );
    txn.commit().await.unwrap();
}

#[tokio::test]
async fn invalid_later_legacy_row_rolls_back_all_conversions() {
    let state = setup_state().await;
    let binding = binding(&state).await;
    let valid = insert_legacy_target(&state, binding.id, "first", "local", "", "").await;
    insert_legacy_target(&state, binding.id, "broken", "s3", "access", "").await;

    let txn = state.writer_db().begin().await.unwrap();
    let error = migrate_legacy_remote_storage_targets(
        &txn,
        &state.config().auth.storage_credential_secret_key,
    )
    .await
    .unwrap_err();
    assert!(error.message().contains("incomplete credentials"));
    txn.rollback().await.unwrap();

    let row = remote_storage_target_repo::find_by_id(state.writer_db(), valid.id)
        .await
        .unwrap();
    assert_eq!(row.driver_type, "local");
    assert!(row.connector_id.is_empty() && row.connector_config.is_empty());
}

#[tokio::test]
async fn unknown_legacy_driver_and_conflicting_destination_credential_abort() {
    let state = setup_state().await;
    let binding = binding(&state).await;
    let unknown = insert_legacy_target(&state, binding.id, "future", "future", "", "").await;
    let txn = state.writer_db().begin().await.unwrap();
    assert!(
        migrate_legacy_remote_storage_targets(
            &txn,
            &state.config().auth.storage_credential_secret_key,
        )
        .await
        .unwrap_err()
        .message()
        .contains("unknown driver")
    );
    txn.rollback().await.unwrap();
    remote_storage_target::Entity::delete_by_id(unknown.id)
        .exec(state.writer_db())
        .await
        .unwrap();

    let legacy =
        insert_legacy_target(&state, binding.id, "conflict", "s3", "access", "secret").await;
    remote_storage_target_credential_repo::upsert(
        state.writer_db(),
        legacy.id,
        S3_CONNECTOR_ID.into(),
        1,
        "conflicting".into(),
    )
    .await
    .unwrap();
    let txn = state.writer_db().begin().await.unwrap();
    assert!(
        migrate_legacy_remote_storage_targets(
            &txn,
            &state.config().auth.storage_credential_secret_key,
        )
        .await
        .unwrap_err()
        .message()
        .contains("conflicting credential")
    );
    txn.rollback().await.unwrap();
}

#[tokio::test]
async fn current_rows_reject_partial_payload_and_wrong_credential_key() {
    let state = setup_state().await;
    let binding = binding(&state).await;
    let mut partial = insert_legacy_target(&state, binding.id, "partial", "", "", "").await;
    let mut active: remote_storage_target::ActiveModel = partial.clone().into();
    active.connector_id = Set(LOCAL_CONNECTOR_ID.into());
    partial = active.update(state.writer_db()).await.unwrap();
    let txn = state.writer_db().begin().await.unwrap();
    assert!(
        migrate_legacy_remote_storage_targets(
            &txn,
            &state.config().auth.storage_credential_secret_key,
        )
        .await
        .unwrap_err()
        .message()
        .contains("partial connector payload")
    );
    txn.rollback().await.unwrap();
    remote_storage_target::Entity::delete_by_id(partial.id)
        .exec(state.writer_db())
        .await
        .unwrap();

    let current = create(&state, &binding, s3("current", false))
        .await
        .unwrap();
    let row = remote_storage_target_repo::find_by_binding_and_target_key(
        state.writer_db(),
        binding.id,
        &current.target_key,
    )
    .await
    .unwrap()
    .unwrap();
    let txn = state.writer_db().begin().await.unwrap();
    assert!(
        migrate_legacy_remote_storage_targets(&txn, "different-remote-target-master-key-32bytes",)
            .await
            .unwrap_err()
            .message()
            .contains("decrypt remote target credential")
    );
    txn.rollback().await.unwrap();
    assert!(
        remote_storage_target_credential_repo::find_by_target(state.writer_db(), row.id)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn current_rows_reject_conflicting_legacy_payload_and_builtin_schema_mismatch() {
    let state = setup_state().await;
    let binding = binding(&state).await;
    let current = create(&state, &binding, local("current", "current", false))
        .await
        .unwrap();
    let row = remote_storage_target_repo::find_by_binding_and_target_key(
        state.writer_db(),
        binding.id,
        &current.target_key,
    )
    .await
    .unwrap()
    .unwrap();

    let mut active: remote_storage_target::ActiveModel = row.clone().into();
    active.driver_type = Set("local".into());
    active.update(state.writer_db()).await.unwrap();
    let txn = state.writer_db().begin().await.unwrap();
    assert!(
        migrate_legacy_remote_storage_targets(
            &txn,
            &state.config().auth.storage_credential_secret_key,
        )
        .await
        .unwrap_err()
        .message()
        .contains("conflicting connector and legacy payloads")
    );
    txn.rollback().await.unwrap();

    let mut active: remote_storage_target::ActiveModel = row.into();
    active.driver_type = Set(String::new());
    active.connector_config = Set(serde_json::to_string(&ConnectorConfigEnvelope::new(
        ConnectorId::declared(LOCAL_CONNECTOR_ID),
        2,
        values(&[("base_path", json!("current"))]),
    ))
    .unwrap());
    active.update(state.writer_db()).await.unwrap();
    let txn = state.writer_db().begin().await.unwrap();
    assert!(
        migrate_legacy_remote_storage_targets(
            &txn,
            &state.config().auth.storage_credential_secret_key,
        )
        .await
        .unwrap_err()
        .message()
        .contains("unsupported connector schema version 2")
    );
    txn.rollback().await.unwrap();
}

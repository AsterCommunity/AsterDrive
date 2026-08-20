//! 存储策略服务子模块：`policies`。

use crate::api::api_error_code::ApiErrorCode;
use crate::api::pagination::{AdminPolicySortBy, load_offset_page};
use crate::db::repository::{
    file_repo, policy_group_repo, policy_repo, system_initialization_repo,
};
use crate::errors::{AsterError, MapAsterErr, Result, validation_error_with_code};
use crate::runtime::{
    RemoteProtocolRuntimeState, SharedRuntimeState, StorageConnectorRuntimeState, TaskRuntimeState,
};
use aster_drive_model::entities::storage_policy;
use aster_drive_storage::{ConnectorConfigEnvelope, StoragePolicyConfigEnvelope};
use aster_forge_api::{OffsetPage, SortOrder};
use aster_forge_db::transaction;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, Set};

use super::models::{
    CreateStoragePolicyInput, StoragePolicy, StoragePolicyActionResult, StoragePolicyCapacityInfo,
    StoragePolicyDiagnostic, UpdateStoragePolicyInput,
};
use super::shared::{
    lock_default_group_assignment, serialize_allowed_types, set_default_policy_and_group,
};
use crate::storage::{
    ExecuteDraftStorageConnectorActionInput, ExecuteSavedStorageConnectorActionInput,
    StorageConnectorConnectionInput, TestDraftStorageConnectorConnectionInput,
};

pub async fn list_paginated(
    state: &impl SharedRuntimeState,
    limit: u64,
    offset: u64,
    sort_by: AdminPolicySortBy,
    sort_order: SortOrder,
) -> Result<OffsetPage<StoragePolicy>> {
    load_offset_page(limit, offset, 100, |limit, offset| async move {
        let (items, total) =
            policy_repo::find_paginated(state.reader_db(), limit, offset, sort_by, sort_order)
                .await?;
        let items = items
            .into_iter()
            .map(StoragePolicy::try_from)
            .collect::<Result<Vec<_>>>()?;
        Ok((items, total))
    })
    .await
}

pub async fn get(state: &impl SharedRuntimeState, id: i64) -> Result<StoragePolicy> {
    policy_repo::find_by_id(state.reader_db(), id)
        .await
        .and_then(StoragePolicy::try_from)
}

pub async fn capacity_info(
    state: &impl SharedRuntimeState,
    id: i64,
) -> Result<StoragePolicyCapacityInfo> {
    let policy = policy_repo::find_by_id(state.reader_db(), id).await?;
    let driver = state.driver_registry().get_driver(&policy)?;
    let blob_summary = file_repo::summarize_blobs_by_policy(state.reader_db(), policy.id).await?;
    let (capacity, diagnostic) =
        capacity_info_or_status(driver.as_ref(), &policy.connector_id).await;
    Ok(StoragePolicyCapacityInfo {
        policy_id: policy.id,
        connector_id: policy.connector_id,
        blob_count: blob_summary.count,
        blob_total_bytes: blob_summary.total_size,
        capacity,
        diagnostic,
    })
}

pub(crate) async fn capacity_info_or_status(
    driver: &dyn aster_drive_storage::StorageDriver,
    connector_id: &str,
) -> (
    aster_drive_storage::StorageCapacityInfo,
    Option<StoragePolicyDiagnostic>,
) {
    match driver.capacity_info().await {
        Ok(capacity) => (capacity, None),
        Err(error) if error.kind() == aster_drive_storage::StorageErrorKind::Unsupported => {
            let error = AsterError::from(error);
            (
                aster_drive_storage::StorageCapacityInfo::unsupported(format!(
                    "{}_driver",
                    connector_id
                )),
                StoragePolicyDiagnostic::from_error(&error),
            )
        }
        Err(error) => {
            let error = AsterError::from(error);
            let kind = error
                .storage_error_kind()
                .map(|kind| kind.as_str())
                .unwrap_or("unknown");
            let api_code = error.api_error_code().as_str();
            tracing::warn!(
                connector_id,
                kind,
                api_code,
                "storage capacity observability failed"
            );
            (
                aster_drive_storage::StorageCapacityInfo::unavailable(format!(
                    "{}_driver",
                    connector_id
                )),
                StoragePolicyDiagnostic::from_error(&error),
            )
        }
    }
}

pub async fn create(
    state: &(impl RemoteProtocolRuntimeState + Sync),
    input: CreateStoragePolicyInput,
) -> Result<StoragePolicy> {
    let CreateStoragePolicyInput {
        name,
        connection,
        max_file_size,
        chunk_size,
        is_default,
        allowed_types,
    } = input;
    let connectors = state.driver_registry().connectors();
    let connection =
        crate::storage::connectors::normalize_connection(connectors, state.writer_db(), connection)
            .await?;
    let StorageConnectorConnectionInput {
        connector_config,
        behavior,
        credential,
    } = connection;
    let behavior = behavior.normalized();
    let connector_id = connector_config.connector_id.as_str().to_string();
    crate::services::ops::deployment::validate_storage_policy_driver(
        connectors,
        state.config(),
        &connector_config.connector_id,
    )?;
    let connector = connectors.require_input_connector(&connector_config.connector_id)?;
    connector.validate_policy_behavior(&behavior)?;
    let descriptor = connector.descriptor();
    let setup_state_at_admission =
        crate::services::storage_policy::connector_catalog::validate_connector_for_current_setup_state(
            state.writer_db(),
            &descriptor,
        )
        .await?;
    let allowed_types = allowed_types.unwrap_or_default();
    let persisted_connector_config = ConnectorConfigEnvelope::new(
        connector_config.connector_id.clone(),
        connector_config.schema_version,
        serde_json::to_value(&connector_config.values).map_err(|error| {
            AsterError::internal_error(format!("serialize connector config: {error}"))
        })?,
    );
    let storage_config =
        aster_drive_storage::encode_storage_policy_config(persisted_connector_config, behavior)
            .map(aster_drive_model::types::StoredStoragePolicyConfig)
            .map_err(|error| {
                AsterError::internal_error(format!("serialize storage policy config: {error}"))
            })?;
    let max_file_size =
        aster_drive_storage::field_contract::normalize_storage_policy_max_file_size(max_file_size)?;
    let chunk_size = chunk_size.unwrap_or(5_242_880);
    let creates_initial_default_policy = is_default
        && setup_state_at_admission
            == crate::services::system_setup::SystemSetupState::NeedsStorage;

    let txn = transaction::begin(state.writer_db()).await?;
    if creates_initial_default_policy {
        system_initialization_repo::acquire_setup_lock(&txn).await?;
        crate::services::system_setup::require_needs_storage(&txn).await?;
    }
    let now = Utc::now();
    let model = storage_policy::ActiveModel {
        name: Set(name),
        max_file_size: Set(max_file_size),
        allowed_types: Set(serialize_allowed_types(&allowed_types)?),
        connector_id: Set(connector_id),
        storage_config: Set(storage_config),
        is_default: Set(false),
        chunk_size: Set(chunk_size),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    let result = policy_repo::create(&txn, model).await?;
    crate::storage::connectors::persist_credential(
        connectors,
        &txn,
        &state.config().auth.storage_credential_secret_key,
        result.id,
        &connector_config,
        credential,
    )
    .await?;
    if is_default {
        set_default_policy_and_group(&txn, result.id).await?;
    }
    transaction::commit(txn).await?;
    state
        .driver_registry()
        .reload_storage_policy_credentials(state.writer_db(), state.config())
        .await?;
    state
        .driver_registry()
        .reload_policy_snapshot(state.policy_snapshot(), state.writer_db())
        .await?;
    crate::services::ops::config::invalidate_public_thumbnail_support_cache();
    crate::services::ops::config::invalidate_public_media_data_support_cache();
    crate::services::ops::config::runtime::publish_storage_topology_reload_after_commit(
        state,
        "create",
        "storage_policy",
        result.id,
    )
    .await;
    policy_repo::find_by_id(state.writer_db(), result.id)
        .await
        .and_then(StoragePolicy::try_from)
}

pub async fn delete(state: &(impl TaskRuntimeState + Sync), id: i64, force: bool) -> Result<()> {
    let policy = policy_repo::find_by_id(state.writer_db(), id).await?;
    tracing::debug!(
        policy_id = id,
        policy_name = %policy.name,
        force,
        "deleting storage policy"
    );

    let blob_count =
        crate::db::repository::file_repo::count_blobs_by_policy(state.writer_db(), id).await?;
    if blob_count > 0 {
        return Err(AsterError::validation_error(format!(
            "cannot delete policy: {blob_count} blob(s) still reference it"
        )));
    }

    let group_ref_count =
        policy_group_repo::count_group_items_by_policy(state.writer_db(), id).await?;
    if group_ref_count > 0 {
        return Err(AsterError::validation_error(format!(
            "cannot delete policy: {group_ref_count} policy group item(s) still reference it"
        )));
    }

    let upload_session_count =
        crate::db::repository::upload_session_repo::count_by_policy(state.writer_db(), id).await?;
    if upload_session_count > 0 {
        if !force {
            return Err(validation_error_with_code(
                ApiErrorCode::PolicyUploadSessionsExist,
                format!(
                    "cannot delete policy: {upload_session_count} upload session(s) still reference it"
                ),
            ));
        }

        let cleanup = crate::services::files::upload::force_cleanup_by_policy(state, id).await?;
        let cleanup_task =
            crate::services::task::storage_policy_cleanup::create_storage_policy_temp_cleanup_task(
                state,
                &policy,
                &cleanup.deferred_temp_keys,
                &cleanup.deferred_multipart_uploads,
            )
            .await?;
        tracing::info!(
            policy_id = id,
            upload_session_count,
            cleaned = cleanup.cleaned,
            deferred_temp_keys = cleanup.deferred_temp_keys.len(),
            deferred_multipart_uploads = cleanup.deferred_multipart_uploads.len(),
            cleanup_task_id = cleanup_task.as_ref().map(|task| task.id),
            "force-cleaned upload sessions before deleting policy"
        );
    }

    let txn = transaction::begin(state.writer_db()).await?;
    lock_default_group_assignment(&txn).await?;
    let policy = policy_repo::find_by_id(&txn, id).await?;

    let blob_count = crate::db::repository::file_repo::count_blobs_by_policy(&txn, id).await?;
    if blob_count > 0 {
        return Err(AsterError::validation_error(format!(
            "cannot delete policy: {blob_count} blob(s) still reference it"
        )));
    }

    let group_ref_count = policy_group_repo::count_group_items_by_policy(&txn, id).await?;
    if group_ref_count > 0 {
        return Err(AsterError::validation_error(format!(
            "cannot delete policy: {group_ref_count} policy group item(s) still reference it"
        )));
    }

    let upload_session_count =
        crate::db::repository::upload_session_repo::count_by_policy(&txn, id).await?;
    if upload_session_count > 0 {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyUploadSessionsExist,
            format!(
                "cannot delete policy: {upload_session_count} upload session(s) still reference it"
            ),
        ));
    }

    let cleared = crate::db::repository::folder_repo::clear_policy_references(&txn, id).await?;
    if cleared > 0 {
        tracing::info!("cleared policy_id on {cleared} folders before deleting policy #{id}");
    }

    policy_repo::delete(&txn, id).await?;
    transaction::commit(txn).await?;

    // 与 update 一致：先 invalidate driver 再 reload snapshot，
    // 避免"策略行已删除但 driver 仍在缓存里"的窗口。
    state.driver_registry().invalidate(id);
    state
        .driver_registry()
        .reload_policy_snapshot(state.policy_snapshot(), state.writer_db())
        .await?;
    crate::services::ops::config::invalidate_public_thumbnail_support_cache();
    crate::services::ops::config::invalidate_public_media_data_support_cache();
    crate::services::ops::config::runtime::publish_storage_topology_reload_after_commit(
        state,
        "delete",
        "storage_policy",
        id,
    )
    .await;
    tracing::info!(
        policy_id = id,
        policy_name = %policy.name,
        force,
        "deleted storage policy"
    );
    Ok(())
}

pub async fn update(
    state: &(impl RemoteProtocolRuntimeState + Sync),
    id: i64,
    input: UpdateStoragePolicyInput,
) -> Result<StoragePolicy> {
    let UpdateStoragePolicyInput {
        name,
        connector_config,
        behavior,
        credential,
        max_file_size,
        chunk_size,
        is_default,
        allowed_types,
    } = input;
    let credential_updated = credential.is_some();
    let existing = policy_repo::find_by_id(state.writer_db(), id).await?;
    let existing_storage_config: StoragePolicyConfigEnvelope =
        serde_json::from_str(existing.storage_config.as_ref()).map_err(|error| {
            AsterError::database_operation(format!(
                "storage policy {} has invalid storage_config: {error}",
                existing.id
            ))
        })?;
    if existing_storage_config.connector.connector_id.as_str() != existing.connector_id {
        return Err(AsterError::database_operation(format!(
            "storage policy {} connector id does not match storage_config",
            existing.id
        )));
    }
    let connectors = state.driver_registry().connectors();
    let connector_config = match connector_config {
        Some(connector_config) => {
            if connector_config.connector_id.as_str() != existing.connector_id {
                return Err(AsterError::validation_error(
                    "storage policy connector_id cannot be changed by patch",
                ));
            }
            crate::storage::connectors::normalize_connector_config(
                connectors,
                state.writer_db(),
                connector_config,
            )
            .await?
        }
        None => ConnectorConfigEnvelope::new(
            existing_storage_config.connector.connector_id.clone(),
            existing_storage_config.connector.schema_version,
            serde_json::from_value(existing_storage_config.connector.values.clone()).map_err(
                |error| {
                    AsterError::database_operation(format!(
                        "storage policy {} connector config must be a JSON object: {error}",
                        existing.id
                    ))
                },
            )?,
        ),
    };
    let behavior = behavior
        .unwrap_or(existing_storage_config.behavior.values)
        .normalized();
    connectors
        .require_input_connector(&connector_config.connector_id)?
        .validate_policy_behavior(&behavior)?;
    let persisted_connector_config = ConnectorConfigEnvelope::new(
        connector_config.connector_id.clone(),
        connector_config.schema_version,
        serde_json::to_value(&connector_config.values).map_err(|error| {
            AsterError::internal_error(format!("serialize connector config: {error}"))
        })?,
    );
    let storage_config =
        aster_drive_storage::encode_storage_policy_config(persisted_connector_config, behavior)
            .map(aster_drive_model::types::StoredStoragePolicyConfig)
            .map_err(|error| {
                AsterError::internal_error(format!("serialize storage policy config: {error}"))
            })?;

    let txn = transaction::begin(state.writer_db()).await?;
    if let Some(false) = is_default
        && existing.is_default
        && policy_repo::find_default(&txn).await?.is_some()
    {
        let all = policy_repo::find_all(&txn).await?;
        let default_count = all.iter().filter(|p| p.is_default).count();
        if default_count <= 1 {
            return Err(AsterError::validation_error(
                "cannot unset the only default storage policy",
            ));
        }
    }

    let existing_is_default = existing.is_default;
    let mut active: storage_policy::ActiveModel = existing.into();
    if let Some(v) = name {
        active.name = Set(v);
    }
    if let Some(v) = max_file_size {
        active.max_file_size =
            Set(aster_drive_storage::field_contract::normalize_storage_policy_max_file_size(v)?);
    }
    if let Some(v) = chunk_size {
        active.chunk_size = Set(v);
    }
    if let Some(v) = is_default {
        active.is_default = Set(v && existing_is_default);
    }
    if let Some(v) = allowed_types {
        active.allowed_types = Set(serialize_allowed_types(&v)?);
    }
    active.storage_config = Set(storage_config);
    active.updated_at = Set(Utc::now());
    let result = active
        .update(&txn)
        .await
        .map_aster_err(AsterError::database_operation)?;

    if let Some(credential) = credential {
        crate::storage::connectors::persist_credential(
            connectors,
            &txn,
            &state.config().auth.storage_credential_secret_key,
            result.id,
            &connector_config,
            credential,
        )
        .await?;
    }

    if is_default == Some(true) {
        set_default_policy_and_group(&txn, result.id).await?;
    }

    transaction::commit(txn).await?;
    if credential_updated {
        state
            .driver_registry()
            .reload_storage_policy_credentials(state.writer_db(), state.config())
            .await?;
    }

    // 失效顺序很关键：必须先 invalidate driver 再 reload snapshot。
    // 如果反过来，中间窗口里读请求可能拿到"新 policy model + 旧 driver cache"，
    // 把写操作发到老的 endpoint/bucket/credential 上——无日志、无报错的静默错路由。
    state.driver_registry().invalidate(id);
    state
        .driver_registry()
        .reload_policy_snapshot(state.policy_snapshot(), state.writer_db())
        .await?;
    crate::services::ops::config::invalidate_public_thumbnail_support_cache();
    crate::services::ops::config::invalidate_public_media_data_support_cache();
    crate::services::ops::config::runtime::publish_storage_topology_reload_after_commit(
        state,
        "update",
        "storage_policy",
        result.id,
    )
    .await;

    policy_repo::find_by_id(state.writer_db(), result.id)
        .await
        .and_then(StoragePolicy::try_from)
}

pub async fn test_default_connection<S: SharedRuntimeState + Sync>(state: &S) -> Result<()> {
    let policy = state
        .policy_snapshot()
        .system_default_policy()
        .ok_or_else(|| {
            AsterError::storage_policy_not_found("system default storage policy not found")
        })?;
    crate::storage::connectors::test_saved_connection(
        state.driver_registry().connectors(),
        state,
        &policy,
    )
    .await
}

pub async fn test_connection<S: StorageConnectorRuntimeState + Sync>(
    state: &S,
    id: i64,
) -> Result<()> {
    let policy = policy_repo::find_by_id(state.writer_db(), id).await?;
    crate::storage::connectors::test_saved_connection(
        state.driver_registry().connectors(),
        state,
        &policy,
    )
    .await
}

pub async fn test_connection_params<S: RemoteProtocolRuntimeState + Sync>(
    state: &S,
    input: TestDraftStorageConnectorConnectionInput,
) -> Result<()> {
    crate::storage::connectors::test_draft_connection(
        state.driver_registry().connectors(),
        state,
        input,
    )
    .await
}

pub async fn execute_saved_action<S: StorageConnectorRuntimeState + Sync>(
    state: &S,
    id: i64,
    input: ExecuteSavedStorageConnectorActionInput,
) -> Result<StoragePolicyActionResult> {
    let policy = policy_repo::find_by_id(state.writer_db(), id).await?;
    crate::storage::connectors::execute_saved_action(
        state.driver_registry().connectors(),
        state,
        &policy,
        input,
    )
    .await
    .map(Into::into)
}

pub async fn execute_draft_action<S: RemoteProtocolRuntimeState + Sync>(
    state: &S,
    input: ExecuteDraftStorageConnectorActionInput,
) -> Result<StoragePolicyActionResult> {
    crate::storage::connectors::execute_draft_action(
        state.driver_registry().connectors(),
        state,
        input,
    )
    .await
    .map(Into::into)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use sea_orm::ActiveValue::Set;
    use tokio::io::AsyncRead;

    use super::*;
    use crate::config::{Config, DatabaseConfig, RuntimeConfig};
    use crate::db;
    use crate::storage::{DriverRegistry, PolicySnapshot};
    use aster_drive_storage::error::storage_driver_error;
    use aster_drive_storage::traits::driver::{BlobMetadata, StorageDriver};
    use aster_drive_storage::traits::extensions::{StorageCapacityInfo, StorageCapacityStatus};
    use aster_forge_cache::CacheConfig;

    async fn setup_state_with_config_sync(
        config_sync: aster_forge_config::ConfigSyncRuntime,
    ) -> crate::runtime::PrimaryAppState {
        let db = db::connect_with_metrics(
            &DatabaseConfig {
                url: "sqlite::memory:".into(),
                pool_size: 1,
                retry_count: 0,
            },
            aster_drive_metrics::NoopMetrics::arc(),
        )
        .await
        .expect("policy service test DB should connect");
        crate::storage::connectors::test_support::migrate_current_storage_test_schema(&db).await;
        let runtime_config = Arc::new(RuntimeConfig::new());
        let cache = aster_forge_cache::create_cache(&CacheConfig {
            backend: "memory".to_string(),
            ..Default::default()
        })
        .await;
        let mut config = Config::default();
        config.auth.storage_credential_secret_key =
            "storage-token-test-master-key-32bytes".to_string();
        let storage_change_bus = crate::services::events::storage_change::StorageChangeBus::new(
            crate::services::events::storage_change::STORAGE_CHANGE_CHANNEL_CAPACITY,
        );
        let share_download_rollback =
            crate::services::share::spawn_detached_share_download_rollback_queue(
                db.clone(),
                crate::config::operations::share_download_rollback_queue_capacity(&runtime_config),
            );

        crate::runtime::PrimaryAppState {
            db_handles: aster_forge_db::DbHandles::single(db),
            driver_registry: Arc::new(
                DriverRegistry::noop().expect("built-in storage connector registry"),
            ),
            runtime_config: runtime_config.clone(),
            policy_snapshot: Arc::new(PolicySnapshot::new()),
            config: Arc::new(config),
            cache,
            config_sync,
            metrics: aster_drive_metrics::NoopMetrics::arc(),
            mail_sender: crate::services::mail::sender::runtime_sender(runtime_config),
            storage_change_bus,
            share_download_rollback,
            background_task_dispatch_wakeup:
                crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
            remote_protocol: crate::runtime::PrimaryAppState::new_remote_protocol(),
        }
    }

    async fn setup_state() -> crate::runtime::PrimaryAppState {
        setup_state_with_config_sync(aster_forge_config::ConfigSyncRuntime::disabled_for_test(
            "aster_drive",
        ))
        .await
    }

    fn local_policy_input(name: &str) -> CreateStoragePolicyInput {
        CreateStoragePolicyInput {
            name: name.to_string(),
            connection: crate::storage::connectors::test_support::local_connection("data/uploads"),
            max_file_size: 0,
            chunk_size: Some(5_242_880),
            is_default: false,
            allowed_types: None,
        }
    }

    struct CapacityErrorDriver(aster_drive_storage::StorageError);

    #[async_trait]
    impl StorageDriver for CapacityErrorDriver {
        async fn put(&self, _path: &str, _data: &[u8]) -> aster_drive_storage::Result<String> {
            Err(self.0.clone())
        }
        async fn get(&self, _path: &str) -> aster_drive_storage::Result<Vec<u8>> {
            Err(self.0.clone())
        }
        async fn get_stream(
            &self,
            _path: &str,
        ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
            Err(self.0.clone())
        }
        async fn delete(&self, _path: &str) -> aster_drive_storage::Result<()> {
            Err(self.0.clone())
        }
        async fn exists(&self, _path: &str) -> aster_drive_storage::Result<bool> {
            Err(self.0.clone())
        }
        async fn metadata(&self, _path: &str) -> aster_drive_storage::Result<BlobMetadata> {
            Err(self.0.clone())
        }
        async fn capacity_info(&self) -> aster_drive_storage::Result<StorageCapacityInfo> {
            Err(self.0.clone())
        }
    }

    #[tokio::test]
    async fn capacity_errors_keep_connector_identity_and_retryability() {
        let unsupported = CapacityErrorDriver(storage_driver_error(
            aster_drive_storage::StorageErrorKind::Unsupported,
            "capacity is not exposed",
        ));
        let (capacity, diagnostic) =
            capacity_info_or_status(&unsupported, "asterdrive.storage.s3").await;
        assert_eq!(capacity.status, StorageCapacityStatus::Unsupported);
        assert_eq!(capacity.source, "asterdrive.storage.s3_driver");
        assert!(!diagnostic.unwrap().retryable);

        let transient = CapacityErrorDriver(storage_driver_error(
            aster_drive_storage::StorageErrorKind::Transient,
            "capacity probe timed out",
        ));
        let (capacity, diagnostic) =
            capacity_info_or_status(&transient, "asterdrive.storage.local").await;
        assert_eq!(capacity.status, StorageCapacityStatus::Unavailable);
        assert!(diagnostic.unwrap().retryable);
    }

    #[tokio::test]
    async fn create_and_update_reject_negative_max_file_size() {
        let state = setup_state().await;
        let mut input = local_policy_input("Local");
        input.max_file_size = -1;
        let error = create(&state, input)
            .await
            .expect_err("negative create max_file_size must fail");
        assert!(error.message().contains("must be non-negative"));

        let policy = create(&state, local_policy_input("Local")).await.unwrap();
        let error = update(
            &state,
            policy.id,
            UpdateStoragePolicyInput {
                max_file_size: Some(-1),
                ..Default::default()
            },
        )
        .await
        .expect_err("negative update max_file_size must fail");
        assert!(error.message().contains("must be non-negative"));
    }

    struct FlakyNotifier {
        attempts: AtomicUsize,
        failures_remaining: AtomicUsize,
    }

    #[async_trait]
    impl aster_forge_config::ConfigChangeNotifier for FlakyNotifier {
        async fn publish_reload(
            &self,
            _message: aster_forge_config::ConfigReloadMessage,
        ) -> aster_forge_config::Result<()> {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            if self
                .failures_remaining
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
                    value.checked_sub(1)
                })
                .is_ok()
            {
                Err(aster_forge_config::ConfigCoreError::notification(
                    "injected notification failure",
                ))
            } else {
                Ok(())
            }
        }

        async fn subscribe(
            &self,
        ) -> aster_forge_config::Result<aster_forge_config::ConfigNotification> {
            Err(aster_forge_config::ConfigCoreError::notification(
                "subscription is not used",
            ))
        }
    }

    #[tokio::test]
    async fn committed_create_survives_notification_failure_and_retries() {
        let notifier = Arc::new(FlakyNotifier {
            attempts: AtomicUsize::new(0),
            failures_remaining: AtomicUsize::new(3),
        });
        let shared_notifier: aster_forge_config::SharedConfigChangeNotifier = notifier.clone();
        let state = setup_state_with_config_sync(
            aster_forge_config::ConfigSyncRuntime::with_notifier_for_test(
                "aster_drive",
                "policy-notification-test",
                shared_notifier,
            ),
        )
        .await;

        let policy = create(&state, local_policy_input("Committed"))
            .await
            .expect("notification failure must not undo committed policy");
        assert_eq!(policy.name, "Committed");
        assert_eq!(notifier.attempts.load(Ordering::SeqCst), 3);
        assert!(
            policy_repo::find_by_id(state.writer_db(), policy.id)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn remote_policy_descriptor_requires_explicit_target_key() {
        let state = setup_state().await;
        let now = Utc::now();
        let remote_node = crate::db::repository::managed_follower_repo::create(
            state.writer_db(),
            aster_drive_model::entities::managed_follower::ActiveModel {
                name: Set("Remote Node".to_string()),
                base_url: Set("http://127.0.0.1:9".to_string()),
                access_key: Set("remote-ak".to_string()),
                secret_key: Set("remote-sk".to_string()),
                is_enabled: Set(true),
                last_capabilities: Set(serde_json::to_string(
                    &crate::storage::remote_protocol::RemoteStorageCapabilities::current(),
                )
                .unwrap()),
                last_error: Set(String::new()),
                last_checked_at: Set(Some(now)),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let error = create(
            &state,
            CreateStoragePolicyInput {
                name: "Remote".to_string(),
                connection: crate::storage::connectors::test_support::remote_connection(
                    "",
                    Some(remote_node.id),
                    None,
                ),
                max_file_size: 0,
                chunk_size: Some(5_242_880),
                is_default: false,
                allowed_types: None,
            },
        )
        .await
        .expect_err("remote target key must be explicit");
        assert_eq!(error.api_error_code_override(), None);
        assert!(
            error
                .message()
                .contains("required provider option field 'remote_storage_target_key' is missing")
        );
    }
}

//! 存储策略服务子模块：`policies`。

use chrono::Utc;
use sea_orm::{ActiveModelTrait, Set};

use crate::api::api_error_code::ApiErrorCode;
use crate::api::pagination::{AdminPolicySortBy, OffsetPage, SortOrder, load_offset_page};
use crate::db::repository::{file_repo, policy_group_repo, policy_repo, upload_session_repo};
use crate::entities::storage_policy;
use crate::errors::{AsterError, MapAsterErr, Result, validation_error_with_code};
use crate::runtime::{RemoteProtocolRuntimeState, SharedRuntimeState, TaskRuntimeState};
use crate::types::{DriverType, parse_storage_policy_options};

use super::models::{
    CreateStoragePolicyInput, ExecuteDraftStoragePolicyActionInput,
    ExecuteSavedStoragePolicyActionInput, PromoteS3CompatiblePolicyDriverInput, StoragePolicy,
    StoragePolicyActionResult, StoragePolicyCapacityInfo, StoragePolicyConnectionInput,
    UpdateStoragePolicyInput,
};
use super::shared::{
    SYSTEM_STORAGE_POLICY_ID, ensure_singleton_group_for_policy, lock_default_group_assignment,
    serialize_allowed_types, serialize_options,
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
        Ok((items.into_iter().map(Into::into).collect(), total))
    })
    .await
}

pub async fn get(state: &impl SharedRuntimeState, id: i64) -> Result<StoragePolicy> {
    policy_repo::find_by_id(state.reader_db(), id)
        .await
        .map(Into::into)
}

pub async fn capacity_info(
    state: &impl SharedRuntimeState,
    id: i64,
) -> Result<StoragePolicyCapacityInfo> {
    let policy = policy_repo::find_by_id(state.reader_db(), id).await?;
    let driver = state.driver_registry().get_driver(&policy)?;
    let blob_summary = file_repo::summarize_blobs_by_policy(state.reader_db(), policy.id).await?;
    let capacity = capacity_info_or_status(driver.as_ref(), policy.driver_type).await;
    Ok(StoragePolicyCapacityInfo {
        policy_id: policy.id,
        driver_type: policy.driver_type,
        blob_count: blob_summary.count,
        blob_total_bytes: blob_summary.total_size,
        capacity,
    })
}

pub(crate) async fn capacity_info_or_status(
    driver: &dyn crate::storage::StorageDriver,
    driver_type: DriverType,
) -> crate::storage::StorageCapacityInfo {
    match driver.capacity_info().await {
        Ok(capacity) => capacity,
        Err(error)
            if error.storage_error_kind()
                == Some(crate::storage::StorageErrorKind::Unsupported) =>
        {
            crate::storage::StorageCapacityInfo::unsupported(format!(
                "{}_driver",
                driver_type.as_str()
            ))
        }
        Err(error) => {
            tracing::warn!(
                driver_type = driver_type.as_str(),
                "storage capacity observability failed: {error}"
            );
            crate::storage::StorageCapacityInfo::unavailable(format!(
                "{}_driver",
                driver_type.as_str()
            ))
        }
    }
}

pub async fn create(
    state: &impl SharedRuntimeState,
    input: CreateStoragePolicyInput,
) -> Result<StoragePolicy> {
    let CreateStoragePolicyInput {
        name,
        connection,
        max_file_size,
        chunk_size,
        is_default,
        allowed_types,
        options,
        application_config,
    } = input;
    let connection =
        crate::storage::connectors::normalize_policy_connection(state.writer_db(), connection)
            .await?;
    let StoragePolicyConnectionInput {
        driver_type,
        endpoint,
        bucket,
        access_key,
        secret_key,
        base_path,
        remote_node_id,
        options: _,
    } = crate::storage::connectors::prepare_connection_for_storage(
        connection,
        &application_config,
    )?;
    let allowed_types = allowed_types.unwrap_or_default();
    let options = options.unwrap_or_default().normalized();
    let serialized_options = serialize_options(&options)?;
    let chunk_size = chunk_size.unwrap_or(5_242_880);
    crate::storage::connectors::validate_policy_options(
        state.writer_db(),
        driver_type,
        remote_node_id,
        &options,
    )
    .await?;

    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    let now = Utc::now();
    let model = storage_policy::ActiveModel {
        name: Set(name),
        driver_type: Set(driver_type),
        endpoint: Set(endpoint),
        bucket: Set(bucket),
        access_key: Set(access_key),
        secret_key: Set(secret_key),
        base_path: Set(base_path),
        remote_node_id: Set(remote_node_id),
        max_file_size: Set(max_file_size),
        allowed_types: Set(serialize_allowed_types(&allowed_types)?),
        options: Set(serialized_options),
        is_default: Set(false),
        chunk_size: Set(chunk_size),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    let result = policy_repo::create(&txn, model).await?;
    crate::storage::connectors::persist_application_config(
        &txn,
        driver_type,
        &state.config().auth.storage_credential_secret_key,
        result.id,
        &options,
        application_config,
    )
    .await?;
    if is_default {
        lock_default_group_assignment(&txn).await?;
        policy_repo::set_only_default(&txn, result.id).await?;
        let default_group_id = ensure_singleton_group_for_policy(&txn, result.id).await?;
        policy_group_repo::set_only_default_group(&txn, default_group_id).await?;
    }
    crate::db::transaction::commit(txn).await?;
    state.policy_snapshot().reload(state.writer_db()).await?;
    crate::services::config_service::invalidate_public_thumbnail_support_cache();
    policy_repo::find_by_id(state.writer_db(), result.id)
        .await
        .map(Into::into)
}

pub async fn delete(state: &(impl TaskRuntimeState + Sync), id: i64, force: bool) -> Result<()> {
    let policy = policy_repo::find_by_id(state.writer_db(), id).await?;
    tracing::debug!(
        policy_id = id,
        policy_name = %policy.name,
        force,
        "deleting storage policy"
    );

    if policy.id == SYSTEM_STORAGE_POLICY_ID {
        return Err(AsterError::validation_error(
            "cannot delete the built-in system storage policy",
        ));
    }

    if policy.is_default {
        let all = policy_repo::find_all(state.writer_db()).await?;
        let default_count = all.iter().filter(|p| p.is_default).count();
        if default_count <= 1 {
            return Err(AsterError::validation_error(
                "cannot delete the only default storage policy",
            ));
        }
    }

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

        let cleanup = crate::services::upload_service::force_cleanup_by_policy(state, id).await?;
        let cleanup_task = crate::services::task_service::storage_policy_cleanup::create_storage_policy_temp_cleanup_task(
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

    let blob_count =
        crate::db::repository::file_repo::count_blobs_by_policy(state.writer_db(), id).await?;
    if blob_count > 0 {
        return Err(AsterError::validation_error(format!(
            "cannot delete policy: {blob_count} blob(s) still reference it"
        )));
    }

    let cleared =
        crate::db::repository::folder_repo::clear_policy_references(state.writer_db(), id).await?;
    if cleared > 0 {
        tracing::info!("cleared policy_id on {cleared} folders before deleting policy #{id}");
    }

    policy_repo::delete(state.writer_db(), id).await?;

    // 与 update 一致：先 invalidate driver 再 reload snapshot，
    // 避免"策略行已删除但 driver 仍在缓存里"的窗口。
    state.driver_registry().invalidate(id);
    state.policy_snapshot().reload(state.writer_db()).await?;
    crate::services::config_service::invalidate_public_thumbnail_support_cache();
    tracing::info!(
        policy_id = id,
        policy_name = %policy.name,
        force,
        "deleted storage policy"
    );
    Ok(())
}

pub async fn update(
    state: &impl SharedRuntimeState,
    id: i64,
    input: UpdateStoragePolicyInput,
) -> Result<StoragePolicy> {
    let UpdateStoragePolicyInput {
        name,
        endpoint,
        bucket,
        access_key,
        secret_key,
        base_path,
        remote_node_id,
        max_file_size,
        chunk_size,
        is_default,
        allowed_types,
        options,
        application_config,
    } = input;
    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    let existing = policy_repo::find_by_id(&txn, id).await?;
    let existing_endpoint = existing.endpoint.clone();
    let existing_bucket = existing.bucket.clone();
    let existing_access_key = existing.access_key.clone();
    let existing_secret_key = existing.secret_key.clone();
    let existing_driver_type = existing.driver_type;
    let existing_remote_node_id = existing.remote_node_id;
    let existing_options = parse_storage_policy_options(existing.options.as_ref());
    let final_endpoint = endpoint.unwrap_or_else(|| existing_endpoint.clone());
    let final_bucket = bucket.unwrap_or_else(|| existing_bucket.clone());
    let final_access_key = access_key
        .clone()
        .unwrap_or_else(|| existing_access_key.clone());
    let final_secret_key = secret_key
        .clone()
        .unwrap_or_else(|| existing_secret_key.clone());
    let final_base_path = base_path
        .clone()
        .unwrap_or_else(|| existing.base_path.clone());
    let normalized_connection = crate::storage::connectors::normalize_policy_connection(
        &txn,
        StoragePolicyConnectionInput {
            driver_type: existing_driver_type,
            endpoint: final_endpoint,
            bucket: final_bucket,
            access_key: final_access_key,
            secret_key: final_secret_key,
            base_path: final_base_path,
            remote_node_id: remote_node_id.or(existing.remote_node_id),
            options: existing_options.clone(),
        },
    )
    .await?;
    let normalized_connection = crate::storage::connectors::prepare_connection_for_storage(
        normalized_connection,
        &application_config,
    )?;
    let normalized_endpoint = normalized_connection.endpoint.clone();
    let normalized_bucket = normalized_connection.bucket.clone();
    let normalized_access_key = normalized_connection.access_key.clone();
    let normalized_secret_key = normalized_connection.secret_key.clone();
    let normalized_remote_node_id = normalized_connection.remote_node_id;
    let options_provided = options.is_some();
    let final_options = options.unwrap_or(existing_options).normalized();
    let serialized_final_options = serialize_options(&final_options)?;
    crate::storage::connectors::validate_policy_options(
        &txn,
        existing_driver_type,
        normalized_remote_node_id,
        &final_options,
    )
    .await?;

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
    if normalized_endpoint != existing_endpoint {
        active.endpoint = Set(normalized_endpoint);
    }
    if normalized_bucket != existing_bucket {
        active.bucket = Set(normalized_bucket);
    }
    if normalized_access_key != existing_access_key {
        active.access_key = Set(normalized_access_key);
    }
    if normalized_secret_key != existing_secret_key {
        active.secret_key = Set(normalized_secret_key);
    }
    if let Some(v) = base_path {
        active.base_path = Set(v);
    }
    if normalized_remote_node_id != existing_remote_node_id {
        active.remote_node_id = Set(normalized_remote_node_id);
    }
    if let Some(v) = max_file_size {
        active.max_file_size = Set(v);
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
    if options_provided {
        active.options = Set(serialized_final_options);
    }
    active.updated_at = Set(Utc::now());
    let result = active
        .update(&txn)
        .await
        .map_aster_err(AsterError::database_operation)?;

    crate::storage::connectors::persist_application_config(
        &txn,
        existing_driver_type,
        &state.config().auth.storage_credential_secret_key,
        result.id,
        &final_options,
        application_config,
    )
    .await?;

    if is_default == Some(true) {
        lock_default_group_assignment(&txn).await?;
        policy_repo::set_only_default(&txn, result.id).await?;
        let default_group_id = ensure_singleton_group_for_policy(&txn, result.id).await?;
        policy_group_repo::set_only_default_group(&txn, default_group_id).await?;
    }

    crate::db::transaction::commit(txn).await?;

    // 失效顺序很关键：必须先 invalidate driver 再 reload snapshot。
    // 如果反过来，中间窗口里读请求可能拿到"新 policy model + 旧 driver cache"，
    // 把写操作发到老的 endpoint/bucket/credential 上——无日志、无报错的静默错路由。
    state.driver_registry().invalidate(id);
    state.policy_snapshot().reload(state.writer_db()).await?;
    crate::services::config_service::invalidate_public_thumbnail_support_cache();

    policy_repo::find_by_id(state.writer_db(), result.id)
        .await
        .map(Into::into)
}

pub async fn promote_s3_compatible_driver(
    state: &impl SharedRuntimeState,
    id: i64,
    input: PromoteS3CompatiblePolicyDriverInput,
) -> Result<StoragePolicy> {
    let existing = policy_repo::find_by_id(state.writer_db(), id).await?;
    crate::storage::connectors::validate_driver_promotion_source(existing.driver_type)?;
    crate::storage::connectors::validate_driver_promotion_target(
        existing.driver_type,
        input.target_driver_type,
    )?;

    let existing_options = parse_storage_policy_options(existing.options.as_ref());
    let normalized_connection = crate::storage::connectors::normalize_policy_connection(
        state.writer_db(),
        StoragePolicyConnectionInput {
            driver_type: input.target_driver_type,
            endpoint: input.endpoint,
            bucket: input.bucket,
            access_key: existing.access_key.clone(),
            secret_key: existing.secret_key.clone(),
            base_path: existing.base_path.clone(),
            remote_node_id: existing.remote_node_id,
            options: existing_options,
        },
    )
    .await?;
    let normalized_endpoint = normalized_connection.endpoint;
    let normalized_bucket = normalized_connection.bucket;
    if normalized_bucket != existing.bucket {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyPromotionBucketChangeDenied,
            "bucket cannot be changed by S3-compatible driver promotion",
        ));
    }

    let active_upload_sessions =
        upload_session_repo::count_active_by_policy(state.writer_db(), id).await?;
    if active_upload_sessions > 0 {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyUploadSessionsExist,
            format!(
                "cannot promote policy: {active_upload_sessions} active upload session(s) still reference it"
            ),
        ));
    }

    let mut candidate_policy = existing.clone();
    candidate_policy.driver_type = input.target_driver_type;
    candidate_policy.endpoint = normalized_endpoint.clone();
    candidate_policy.bucket = normalized_bucket;
    validate_s3_compatible_promotion_candidate(state, &candidate_policy).await?;

    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    let active_upload_sessions = upload_session_repo::count_active_by_policy(&txn, id).await?;
    if active_upload_sessions > 0 {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyUploadSessionsExist,
            format!(
                "cannot promote policy: {active_upload_sessions} active upload session(s) still reference it"
            ),
        ));
    }
    policy_repo::promote_s3_compatible_driver(
        &txn,
        id,
        DriverType::S3,
        input.target_driver_type,
        normalized_endpoint,
    )
    .await?;
    crate::db::transaction::commit(txn).await?;

    // 与普通 update 一致：先 invalidate driver，再 reload snapshot。
    state.driver_registry().invalidate(id);
    state.policy_snapshot().reload(state.writer_db()).await?;
    crate::services::config_service::invalidate_public_thumbnail_support_cache();

    policy_repo::find_by_id(state.writer_db(), id)
        .await
        .map(Into::into)
}

async fn validate_s3_compatible_promotion_candidate(
    state: &impl SharedRuntimeState,
    candidate_policy: &storage_policy::Model,
) -> Result<()> {
    crate::storage::connectors::validate_driver_promotion_candidate(candidate_policy)?;

    verify_s3_compatible_promotion_sample(state, candidate_policy).await
}

async fn verify_s3_compatible_promotion_sample(
    state: &impl SharedRuntimeState,
    candidate_policy: &storage_policy::Model,
) -> Result<()> {
    const PROMOTION_SAMPLE_SIZE: u64 = 10;

    let blobs = file_repo::find_blobs_by_policy_paginated(
        state.writer_db(),
        candidate_policy.id,
        0,
        PROMOTION_SAMPLE_SIZE,
    )
    .await?;
    if blobs.is_empty() {
        return Ok(());
    }

    let driver = state
        .driver_registry()
        .build_uncached_driver(candidate_policy)?;
    for blob in blobs {
        let metadata = driver.metadata(&blob.storage_path).await.map_err(|error| {
            AsterError::storage_driver_error(format!(
                "verify existing object '{}' (blob id {}) before S3-compatible driver promotion: {error}",
                blob.storage_path, blob.id
            ))
        })?;
        let actual_size = crate::utils::numbers::u64_to_i64(metadata.size, "blob metadata size")?;
        if actual_size != blob.size {
            return Err(AsterError::storage_driver_error(format!(
                "object '{}' (blob id {}) size mismatch before S3-compatible driver promotion: expected {}, got {}",
                blob.storage_path, blob.id, blob.size, actual_size
            )));
        }
    }

    Ok(())
}

pub async fn test_default_connection<S: SharedRuntimeState + Sync>(state: &S) -> Result<()> {
    let policy = state
        .policy_snapshot()
        .system_default_policy()
        .ok_or_else(|| {
            AsterError::storage_policy_not_found("system default storage policy not found")
        })?;
    crate::storage::connectors::test_saved_connection(state, &policy).await
}

pub async fn test_connection<S: SharedRuntimeState + Sync>(state: &S, id: i64) -> Result<()> {
    let policy = policy_repo::find_by_id(state.writer_db(), id).await?;
    crate::storage::connectors::test_saved_connection(state, &policy).await
}

pub async fn test_connection_params<S: RemoteProtocolRuntimeState + Sync>(
    state: &S,
    input: StoragePolicyConnectionInput,
) -> Result<()> {
    crate::storage::connectors::test_draft_connection(state, input).await
}

pub async fn execute_saved_action<S: SharedRuntimeState + Sync>(
    state: &S,
    id: i64,
    input: ExecuteSavedStoragePolicyActionInput,
) -> Result<StoragePolicyActionResult> {
    let policy = policy_repo::find_by_id(state.writer_db(), id).await?;
    crate::storage::connectors::execute_saved_action(state, &policy, input.action).await
}

pub async fn execute_draft_action<S: RemoteProtocolRuntimeState + Sync>(
    state: &S,
    input: ExecuteDraftStoragePolicyActionInput,
) -> Result<StoragePolicyActionResult> {
    crate::storage::connectors::execute_draft_action(state, input).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CacheConfig, Config, DatabaseConfig, RuntimeConfig};
    use crate::db;
    use crate::db::repository::{
        storage_connector_application_config_repo, storage_policy_credential_repo,
    };
    use crate::storage::{DriverRegistry, PolicySnapshot};
    use crate::types::{
        MicrosoftGraphCloud, OneDriveAccountMode, StorageCredentialKind, StorageCredentialProvider,
        StoragePolicyOptions, StoredStoragePolicyAllowedTypes,
    };
    use migration::Migrator;
    use sea_orm::ActiveValue::Set;
    use std::sync::Arc;

    async fn setup_state(encryption_key: &str) -> crate::runtime::PrimaryAppState {
        let db = db::connect_with_metrics(
            &DatabaseConfig {
                url: "sqlite::memory:".to_string(),
                pool_size: 1,
                retry_count: 0,
            },
            crate::metrics_core::NoopMetrics::arc(),
        )
        .await
        .expect("policy service test DB should connect");
        Migrator::up(&db, None)
            .await
            .expect("policy service migrations should succeed");
        let runtime_config = Arc::new(RuntimeConfig::new());
        let cache = crate::cache::create_cache(&CacheConfig {
            enabled: false,
            backend: "memory".to_string(),
            ..Default::default()
        })
        .await;
        let mut config = Config::default();
        config.auth.storage_credential_secret_key = encryption_key.to_string();
        let (storage_change_tx, _) = tokio::sync::broadcast::channel(
            crate::services::storage_change_service::STORAGE_CHANGE_CHANNEL_CAPACITY,
        );
        let share_download_rollback =
            crate::services::share_service::spawn_detached_share_download_rollback_queue(
                db.clone(),
                crate::config::operations::share_download_rollback_queue_capacity(&runtime_config),
            );

        crate::runtime::PrimaryAppState {
            db_handles: crate::db::DbHandles::single(db),
            driver_registry: Arc::new(DriverRegistry::noop()),
            runtime_config: runtime_config.clone(),
            policy_snapshot: Arc::new(PolicySnapshot::new()),
            config: Arc::new(config),
            cache,
            metrics: crate::metrics_core::NoopMetrics::arc(),
            mail_sender: crate::services::mail_service::runtime_sender(runtime_config),
            storage_change_tx,
            share_download_rollback,
            background_task_dispatch_wakeup:
                crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
            remote_protocol: crate::runtime::PrimaryAppState::new_remote_protocol(),
        }
    }

    fn onedrive_options() -> StoragePolicyOptions {
        StoragePolicyOptions {
            onedrive_cloud: Some(MicrosoftGraphCloud::Global),
            onedrive_account_mode: Some(OneDriveAccountMode::WorkOrSchool),
            onedrive_tenant: Some("common".to_string()),
            onedrive_root_item_id: Some("root".to_string()),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn create_onedrive_policy_stores_app_config_outside_legacy_key_fields() {
        let encryption_key = "storage-token-test-master-key-32bytes";
        let state = setup_state(encryption_key).await;

        let policy = create(
            &state,
            CreateStoragePolicyInput {
                name: "OneDrive".to_string(),
                connection: StoragePolicyConnectionInput {
                    driver_type: DriverType::OneDrive,
                    endpoint: String::new(),
                    bucket: String::new(),
                    access_key: "legacy-client-id".to_string(),
                    secret_key: "legacy-client-secret".to_string(),
                    base_path: String::new(),
                    remote_node_id: None,
                    options: StoragePolicyOptions::default(),
                },
                max_file_size: 0,
                chunk_size: Some(5_242_880),
                is_default: false,
                allowed_types: None,
                options: Some(onedrive_options()),
                application_config: crate::storage::StorageConnectorApplicationConfigInput {
                    microsoft_graph: Some(crate::storage::MicrosoftGraphApplicationConfigInput {
                        client_id: Some("metadata-client-id".to_string()),
                        client_secret: Some("metadata-client-secret".to_string()),
                        ..Default::default()
                    }),
                },
            },
        )
        .await
        .expect("OneDrive policy should create");

        let stored = policy_repo::find_by_id(state.writer_db(), policy.id)
            .await
            .expect("policy should load");
        assert_eq!(stored.access_key, "");
        assert_eq!(stored.secret_key, "");

        let application_config =
            storage_connector_application_config_repo::find_by_policy_provider(
                state.writer_db(),
                policy.id,
                StorageCredentialProvider::MicrosoftGraph,
            )
            .await
            .expect("application config lookup should succeed")
            .expect("application config should exist");
        assert_eq!(
            application_config.client_id,
            Some("metadata-client-id".to_string())
        );
        assert!(application_config.client_secret_ciphertext.is_some());

        let credential = storage_policy_credential_repo::find_by_policy_provider_kind(
            state.writer_db(),
            policy.id,
            StorageCredentialProvider::MicrosoftGraph,
            StorageCredentialKind::OauthDelegated,
        )
        .await
        .expect("credential lookup should succeed");
        assert!(
            credential.is_none(),
            "saving connector app config must not create an OAuth authorization credential"
        );
    }

    #[tokio::test]
    async fn update_onedrive_policy_clears_legacy_keys_and_writes_app_config_metadata() {
        let encryption_key = "storage-token-test-master-key-32bytes";
        let state = setup_state(encryption_key).await;
        let now = Utc::now();
        let policy = policy_repo::create(
            state.writer_db(),
            storage_policy::ActiveModel {
                name: Set("OneDrive".to_string()),
                driver_type: Set(DriverType::OneDrive),
                endpoint: Set(String::new()),
                bucket: Set(String::new()),
                access_key: Set("old-client-id".to_string()),
                secret_key: Set("old-client-secret".to_string()),
                base_path: Set(String::new()),
                remote_node_id: Set(None),
                max_file_size: Set(0),
                allowed_types: Set(StoredStoragePolicyAllowedTypes::empty()),
                options: Set(
                    crate::types::serialize_storage_policy_options(&onedrive_options())
                        .expect("options should serialize"),
                ),
                is_default: Set(false),
                chunk_size: Set(5_242_880),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .expect("policy should insert");

        update(
            &state,
            policy.id,
            UpdateStoragePolicyInput {
                access_key: Some("ignored-client-id".to_string()),
                secret_key: Some("ignored-client-secret".to_string()),
                application_config: crate::storage::StorageConnectorApplicationConfigInput {
                    microsoft_graph: Some(crate::storage::MicrosoftGraphApplicationConfigInput {
                        client_id: Some("metadata-client-id".to_string()),
                        client_secret: Some("metadata-client-secret".to_string()),
                        ..Default::default()
                    }),
                },
                ..Default::default()
            },
        )
        .await
        .expect("OneDrive policy should update");

        let stored = policy_repo::find_by_id(state.writer_db(), policy.id)
            .await
            .expect("policy should load");
        assert_eq!(stored.access_key, "");
        assert_eq!(stored.secret_key, "");

        let application_config =
            storage_connector_application_config_repo::find_by_policy_provider(
                state.writer_db(),
                policy.id,
                StorageCredentialProvider::MicrosoftGraph,
            )
            .await
            .expect("application config lookup should succeed")
            .expect("application config should exist");
        assert_eq!(
            application_config.client_id,
            Some("metadata-client-id".to_string())
        );
        assert!(application_config.client_secret_ciphertext.is_some());

        let credential = storage_policy_credential_repo::find_by_policy_provider_kind(
            state.writer_db(),
            policy.id,
            StorageCredentialProvider::MicrosoftGraph,
            StorageCredentialKind::OauthDelegated,
        )
        .await
        .expect("credential lookup should succeed");
        assert!(
            credential.is_none(),
            "updating connector app config must not create an OAuth authorization credential"
        );
    }
}

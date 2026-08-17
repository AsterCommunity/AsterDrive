//! 存储策略服务子模块：`policies`。

use aster_forge_db::transaction;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, Set};
use std::collections::BTreeMap;

use crate::api::api_error_code::ApiErrorCode;
use crate::api::pagination::{AdminPolicySortBy, load_offset_page};
use crate::db::repository::{
    file_repo, policy_group_repo, policy_repo, storage_policy_connector_credential_repo,
    system_initialization_repo, upload_session_repo,
};
use crate::errors::{AsterError, MapAsterErr, Result, validation_error_with_code};
use crate::runtime::{
    RemoteProtocolRuntimeState, SharedRuntimeState, StorageConnectorRuntimeState, TaskRuntimeState,
};
use aster_drive_model::entities::storage_policy;
use aster_drive_storage::{ConnectorConfigEnvelope, StoragePolicyConfigEnvelope};
use aster_drive_storage::{
    StorageConnectorCredentialMode, StorageConnectorPromotionDescriptor,
    StorageConnectorPromotionFieldMapping, StorageConnectorPromotionValueMatcher,
};
use aster_forge_api::{OffsetPage, SortOrder};

use super::models::{
    CreateStoragePolicyInput, PromoteStoragePolicyConnectorInput, StoragePolicy,
    StoragePolicyActionResult, StoragePolicyCapacityInfo, StoragePolicyDiagnostic,
    UpdateStoragePolicyInput,
};
use super::shared::{
    SYSTEM_STORAGE_POLICY_ID, serialize_allowed_types, set_default_policy_and_group,
};
use crate::storage::{
    ExecuteDraftStorageConnectorActionInput, ExecuteSavedStorageConnectorActionInput,
    StorageConnectorConnectionInput, StorageConnectorCredentialInput,
    TestDraftStorageConnectorConnectionInput,
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

pub async fn promote_connector(
    state: &(impl RemoteProtocolRuntimeState + Sync),
    id: i64,
    input: PromoteStoragePolicyConnectorInput,
) -> Result<StoragePolicy> {
    const PROMOTION_SAMPLE_SIZE: u64 = 10;

    let existing = policy_repo::find_by_id(state.writer_db(), id).await?;
    let source_storage_config = decode_policy_storage_config(&existing)?;
    let source_connector_id = source_storage_config.connector.connector_id.clone();
    let source_config_values: BTreeMap<String, serde_json::Value> = serde_json::from_value(
        source_storage_config.connector.values.clone(),
    )
    .map_err(|error| {
        AsterError::database_operation(format!(
            "storage policy {id} connector config must be an object: {error}"
        ))
    })?;
    let connectors = state.driver_registry().connectors();
    let target_connector = connectors.require_input_connector(&input.target_connector_id)?;
    let target_descriptor = target_connector.descriptor();
    let promotion = connectors
        .promotion_descriptor(&input.target_connector_id, &input.promotion_id)?
        .ok_or_else(|| {
            validation_error_with_code(
                ApiErrorCode::PolicyPromotionTargetUnsupported,
                format!(
                    "storage connector '{}' does not declare promotion '{}'",
                    input.target_connector_id,
                    input.promotion_id.as_str()
                ),
            )
        })?;
    if promotion.source_connector_id != source_connector_id {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyPromotionSourceUnsupported,
            format!(
                "storage policy #{id} uses connector '{}', but promotion '{}' requires '{}'",
                source_connector_id,
                promotion.promotion_id.as_str(),
                promotion.source_connector_id
            ),
        ));
    }
    if !promotion_requirements_match(&promotion, &source_config_values) {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyPromotionTargetUnsupported,
            format!(
                "storage policy #{id} does not satisfy promotion '{}' requirements",
                promotion.promotion_id.as_str()
            ),
        ));
    }
    crate::services::ops::deployment::validate_storage_policy_driver(
        connectors,
        state.config(),
        &input.target_connector_id,
    )?;
    let active_upload_sessions =
        upload_session_repo::count_active_by_policy(state.writer_db(), id).await?;
    if active_upload_sessions > 0 {
        return Err(active_upload_promotion_error(active_upload_sessions));
    }

    let target_values =
        map_promotion_config_values(&promotion.config_mappings, &source_config_values)?;
    let target_config = ConnectorConfigEnvelope::new(
        input.target_connector_id.clone(),
        target_descriptor.config_schema_version,
        target_values,
    );
    let target_config = crate::storage::connectors::normalize_connector_config(
        connectors,
        state.writer_db(),
        target_config,
    )
    .await?;
    ensure_preserved_promotion_values(
        &promotion.config_mappings,
        &source_config_values,
        &target_config.values,
    )?;

    let source_descriptor = connectors.require_policy(&existing)?.descriptor();
    let source_credential =
        storage_policy_connector_credential_repo::find_by_policy(state.writer_db(), id).await?;
    let (target_credential, target_credential_payload) = match target_descriptor.credential_mode {
        StorageConnectorCredentialMode::StaticSecret => {
            let source_credential = source_credential.as_ref().ok_or_else(|| {
                AsterError::database_operation(format!(
                    "storage policy #{id} has no connector credential to promote"
                ))
            })?;
            let source_schema_version =
                crate::storage::connectors::credential_schema_version(&source_descriptor)?;
            let source_values = crate::storage::connectors::decode_connector_credential(
                &state.config().auth.storage_credential_secret_key,
                source_credential,
                &source_connector_id,
                source_schema_version,
            )?;
            let target_values =
                map_promotion_credential_values(&promotion.credential_mappings, &source_values)?;
            let credential = StorageConnectorCredentialInput::Static(target_values.clone());
            crate::storage::connectors::validate_credential_input(
                connectors,
                &input.target_connector_id,
                &credential,
            )?;
            (credential, Some(target_values))
        }
        StorageConnectorCredentialMode::None => (StorageConnectorCredentialInput::None, None),
        _ => {
            return Err(validation_error_with_code(
                ApiErrorCode::PolicyPromotionTargetUnsupported,
                "connector promotions currently require compatible static credentials or credential-free connectors",
            ));
        }
    };

    let behavior = source_storage_config.behavior.values;
    target_connector.validate_policy_behavior(&behavior)?;
    let persisted_target_config = ConnectorConfigEnvelope::new(
        target_config.connector_id.clone(),
        target_config.schema_version,
        serde_json::to_value(&target_config.values).map_err(|error| {
            AsterError::internal_error(format!("serialize promoted connector config: {error}"))
        })?,
    );
    let encoded_storage_config =
        aster_drive_storage::encode_storage_policy_config(persisted_target_config, behavior)
            .map(aster_drive_model::types::StoredStoragePolicyConfig)
            .map_err(|error| {
                AsterError::internal_error(format!(
                    "serialize promoted storage policy config: {error}"
                ))
            })?;
    let mut candidate = existing.clone();
    candidate.connector_id = input.target_connector_id.as_str().to_string();
    candidate.storage_config = encoded_storage_config.clone();
    let candidate_driver = target_connector
        .build_draft_driver(
            &crate::storage::connectors::remote_connector_context(state),
            &candidate,
            &target_credential,
        )
        .await?;

    let blobs = file_repo::find_stored_blobs_by_policy_paginated(
        state.writer_db(),
        id,
        0,
        PROMOTION_SAMPLE_SIZE,
    )
    .await?;
    verify_promotion_blob_sample(candidate_driver.as_ref(), &blobs).await?;

    let txn = transaction::begin(state.writer_db()).await?;
    let locked = policy_repo::lock_by_id(&txn, id).await?;
    ensure_promotion_policy_unchanged(&existing, &locked)?;
    let active_upload_sessions = upload_session_repo::count_active_by_policy(&txn, id).await?;
    if active_upload_sessions > 0 {
        return Err(active_upload_promotion_error(active_upload_sessions));
    }
    let result = policy_repo::promote_connector(
        &txn,
        locked,
        input.target_connector_id.as_str().to_string(),
        encoded_storage_config,
    )
    .await?;
    if let Some(target_credential_payload) = target_credential_payload {
        let source_credential = source_credential.as_ref().ok_or_else(|| {
            AsterError::database_operation("source connector credential disappeared")
        })?;
        let target_schema_version =
            crate::storage::connectors::credential_schema_version(&target_descriptor)?;
        let target_schema_version_i32 = i32::try_from(target_schema_version).map_err(|_| {
            AsterError::validation_error(
                "connector credential schema version exceeds database range",
            )
        })?;
        let plaintext = serde_json::to_string(&target_credential_payload).map_err(|error| {
            AsterError::validation_error(format!(
                "serialize promoted connector credential payload: {error}"
            ))
        })?;
        let ciphertext =
            crate::services::storage_policy::credential::crypto::encrypt_connector_credential(
                &state.config().auth.storage_credential_secret_key,
                id,
                input.target_connector_id.as_str(),
                target_schema_version,
                &plaintext,
            )?;
        let promoted = storage_policy_connector_credential_repo::promote_if_revision(
            &txn,
            storage_policy_connector_credential_repo::ConnectorCredentialPromotion {
                policy_id: id,
                source_connector_id: &source_credential.connector_id,
                source_schema_version: source_credential.schema_version,
                expected_revision: source_credential.revision,
                target_connector_id: input.target_connector_id.as_str().to_string(),
                target_schema_version: target_schema_version_i32,
                ciphertext,
            },
        )
        .await?;
        if !promoted {
            return Err(AsterError::validation_error(
                "storage connector credential changed while promotion was being validated; retry the operation",
            ));
        }
    }
    transaction::commit(txn).await?;

    state
        .driver_registry()
        .reload_storage_policy_credentials(state.writer_db(), state.config())
        .await?;
    state.driver_registry().invalidate(id);
    state
        .driver_registry()
        .reload_policy_snapshot(state.policy_snapshot(), state.writer_db())
        .await?;
    crate::services::ops::config::invalidate_public_thumbnail_support_cache();
    crate::services::ops::config::invalidate_public_media_data_support_cache();
    crate::services::ops::config::runtime::publish_storage_topology_reload_after_commit(
        state,
        "promote_connector",
        "storage_policy",
        result.id,
    )
    .await;

    policy_repo::find_by_id(state.writer_db(), result.id)
        .await
        .and_then(StoragePolicy::try_from)
}

async fn verify_promotion_blob_sample(
    driver: &dyn aster_drive_storage::StorageDriver,
    blobs: &[aster_drive_model::entities::file_blob::Model],
) -> Result<()> {
    for blob in blobs {
        let storage_path = blob.storage_path.as_deref().ok_or_else(|| {
            AsterError::database_operation(format!("stored blob {} has no storage path", blob.id))
        })?;
        let metadata = driver.metadata(storage_path).await.map_err(|error| {
            AsterError::storage_driver_error(format!(
                "verify existing object '{storage_path}' (blob id {}) before connector promotion: {error}",
                blob.id
            ))
        })?;
        let actual_size =
            aster_forge_utils::numbers::u64_to_i64(metadata.size, "blob metadata size")?;
        if actual_size != blob.size {
            return Err(AsterError::storage_driver_error(format!(
                "object '{storage_path}' (blob id {}) size mismatch before connector promotion: expected {}, got {actual_size}",
                blob.id, blob.size
            )));
        }
    }
    Ok(())
}

fn decode_policy_storage_config(
    policy: &storage_policy::Model,
) -> Result<StoragePolicyConfigEnvelope> {
    let config: StoragePolicyConfigEnvelope = serde_json::from_str(policy.storage_config.as_ref())
        .map_err(|error| {
            AsterError::database_operation(format!(
                "storage policy {} has invalid storage_config: {error}",
                policy.id
            ))
        })?;
    if config.connector.connector_id.as_str() != policy.connector_id {
        return Err(AsterError::database_operation(format!(
            "storage policy {} connector id does not match storage_config",
            policy.id
        )));
    }
    Ok(config)
}

fn promotion_requirements_match(
    promotion: &StorageConnectorPromotionDescriptor,
    values: &BTreeMap<String, serde_json::Value>,
) -> bool {
    promotion.requirements.iter().all(|requirement| {
        let Some(value) = values
            .get(&requirement.source_field)
            .and_then(serde_json::Value::as_str)
        else {
            return false;
        };
        match &requirement.matcher {
            StorageConnectorPromotionValueMatcher::StringEquals {
                value: expected,
                case_sensitive,
            } => compare_promotion_text(value, expected, *case_sensitive, |left, right| {
                left == right
            }),
            StorageConnectorPromotionValueMatcher::StringSuffix {
                suffix,
                case_sensitive,
            } => compare_promotion_text(value, suffix, *case_sensitive, |left, right| {
                left.ends_with(right)
            }),
            StorageConnectorPromotionValueMatcher::UrlHostSuffix { suffix } => {
                url::Url::parse(value)
                    .ok()
                    .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
                    .is_some_and(|host| host.ends_with(&suffix.to_ascii_lowercase()))
            }
        }
    })
}

fn compare_promotion_text(
    value: &str,
    expected: &str,
    case_sensitive: bool,
    compare: impl FnOnce(&str, &str) -> bool,
) -> bool {
    if case_sensitive {
        compare(value, expected)
    } else {
        compare(&value.to_ascii_lowercase(), &expected.to_ascii_lowercase())
    }
}

fn map_promotion_config_values(
    mappings: &[StorageConnectorPromotionFieldMapping],
    source: &BTreeMap<String, serde_json::Value>,
) -> Result<BTreeMap<String, serde_json::Value>> {
    mappings
        .iter()
        .map(|mapping| {
            source
                .get(&mapping.source_field)
                .cloned()
                .map(|value| (mapping.target_field.clone(), value))
                .ok_or_else(|| {
                    AsterError::validation_error(format!(
                        "promotion source config field '{}' is missing",
                        mapping.source_field
                    ))
                })
        })
        .collect()
}

fn map_promotion_credential_values(
    mappings: &[StorageConnectorPromotionFieldMapping],
    source: &serde_json::Value,
) -> Result<serde_json::Value> {
    let source = source.as_object().ok_or_else(|| {
        AsterError::database_operation("stored connector credential payload must be an object")
    })?;
    let target = mappings
        .iter()
        .map(|mapping| {
            source
                .get(&mapping.source_field)
                .cloned()
                .map(|value| (mapping.target_field.clone(), value))
                .ok_or_else(|| {
                    AsterError::database_operation(format!(
                        "stored connector credential is missing promotion field '{}'",
                        mapping.source_field
                    ))
                })
        })
        .collect::<Result<serde_json::Map<String, serde_json::Value>>>()?;
    Ok(serde_json::Value::Object(target))
}

fn ensure_preserved_promotion_values(
    mappings: &[StorageConnectorPromotionFieldMapping],
    source: &BTreeMap<String, serde_json::Value>,
    target: &BTreeMap<String, serde_json::Value>,
) -> Result<()> {
    for mapping in mappings.iter().filter(|mapping| mapping.preserve_value) {
        if source.get(&mapping.source_field) != target.get(&mapping.target_field) {
            return Err(AsterError::validation_error(format!(
                "connector promotion cannot change preserved field '{}'",
                mapping.source_field
            )));
        }
    }
    Ok(())
}

fn active_upload_promotion_error(active_upload_sessions: u64) -> AsterError {
    validation_error_with_code(
        ApiErrorCode::PolicyUploadSessionsExist,
        format!(
            "cannot promote policy: {active_upload_sessions} active upload session(s) still reference it"
        ),
    )
}

fn ensure_promotion_policy_unchanged(
    expected: &storage_policy::Model,
    locked: &storage_policy::Model,
) -> Result<()> {
    if locked.connector_id == expected.connector_id
        && locked.storage_config == expected.storage_config
        && locked.updated_at == expected.updated_at
    {
        return Ok(());
    }
    Err(AsterError::validation_error(
        "storage policy changed while connector promotion was being validated; retry the operation",
    ))
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

    struct PromotionMetadataDriver {
        size: u64,
    }

    #[async_trait]
    impl StorageDriver for PromotionMetadataDriver {
        async fn put(&self, _path: &str, _data: &[u8]) -> aster_drive_storage::Result<String> {
            Err(storage_driver_error(
                aster_drive_storage::StorageErrorKind::Unsupported,
                "unexpected put",
            ))
        }

        async fn get(&self, _path: &str) -> aster_drive_storage::Result<Vec<u8>> {
            Err(storage_driver_error(
                aster_drive_storage::StorageErrorKind::Unsupported,
                "unexpected get",
            ))
        }

        async fn get_stream(
            &self,
            _path: &str,
        ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
            Err(storage_driver_error(
                aster_drive_storage::StorageErrorKind::Unsupported,
                "unexpected get_stream",
            ))
        }

        async fn delete(&self, _path: &str) -> aster_drive_storage::Result<()> {
            Err(storage_driver_error(
                aster_drive_storage::StorageErrorKind::Unsupported,
                "unexpected delete",
            ))
        }

        async fn exists(&self, _path: &str) -> aster_drive_storage::Result<bool> {
            Err(storage_driver_error(
                aster_drive_storage::StorageErrorKind::Unsupported,
                "unexpected exists",
            ))
        }

        async fn metadata(&self, _path: &str) -> aster_drive_storage::Result<BlobMetadata> {
            Ok(BlobMetadata {
                size: self.size,
                content_type: None,
            })
        }
    }

    fn promotion_blob(size: i64) -> aster_drive_model::entities::file_blob::Model {
        let now = Utc::now();
        aster_drive_model::entities::file_blob::Model {
            id: 7,
            hash: "hash".to_string(),
            size,
            policy_id: 3,
            storage_path: Some("objects/sample".to_string()),
            backing: aster_drive_model::types::file_blob::FileBlobBacking::Stored,
            thumbnail_path: None,
            thumbnail_processor: None,
            thumbnail_version: None,
            ref_count: 1,
            created_at: now,
            updated_at: now,
        }
    }

    fn promotion_with_requirements(
        requirements: Vec<aster_drive_storage::StorageConnectorPromotionRequirement>,
    ) -> StorageConnectorPromotionDescriptor {
        StorageConnectorPromotionDescriptor {
            promotion_id: aster_drive_storage::StorageConnectorPromotionId::declared(
                "test_promotion",
            ),
            source_connector_id: aster_drive_storage::ConnectorId::declared("com.example.source"),
            description_key: "description".to_string(),
            confirmation_key: "confirmation".to_string(),
            requirements,
            config_mappings: Vec::new(),
            credential_mappings: Vec::new(),
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
    async fn promotion_sample_verifies_metadata_and_size() {
        let blob = promotion_blob(12);
        verify_promotion_blob_sample(
            &PromotionMetadataDriver { size: 12 },
            std::slice::from_ref(&blob),
        )
        .await
        .expect("matching metadata should pass");

        let error = verify_promotion_blob_sample(
            &PromotionMetadataDriver { size: 11 },
            std::slice::from_ref(&blob),
        )
        .await
        .expect_err("size mismatch must block promotion");
        assert!(error.message().contains("size mismatch"));

        let error = verify_promotion_blob_sample(
            &CapacityErrorDriver(storage_driver_error(
                aster_drive_storage::StorageErrorKind::Transient,
                "metadata unavailable",
            )),
            &[blob],
        )
        .await
        .expect_err("metadata failure must block promotion");
        assert!(error.message().contains("verify existing object"));
        assert!(error.message().contains("blob id 7"));
    }

    #[test]
    fn promotion_matchers_cover_case_suffix_url_and_all_requirements() {
        use aster_drive_storage::{
            StorageConnectorPromotionRequirement, StorageConnectorPromotionValueMatcher,
        };

        let values = BTreeMap::from([
            ("provider".to_string(), serde_json::json!("Tencent-COS")),
            ("bucket".to_string(), serde_json::json!("archive-prod")),
            (
                "endpoint".to_string(),
                serde_json::json!("https://BUCKET.cos.ap-guangzhou.MYQCLOUD.COM/path"),
            ),
        ]);
        let promotion = promotion_with_requirements(vec![
            StorageConnectorPromotionRequirement {
                source_field: "provider".to_string(),
                matcher: StorageConnectorPromotionValueMatcher::StringEquals {
                    value: "tencent-cos".to_string(),
                    case_sensitive: false,
                },
            },
            StorageConnectorPromotionRequirement {
                source_field: "bucket".to_string(),
                matcher: StorageConnectorPromotionValueMatcher::StringSuffix {
                    suffix: "-PROD".to_string(),
                    case_sensitive: false,
                },
            },
            StorageConnectorPromotionRequirement {
                source_field: "endpoint".to_string(),
                matcher: StorageConnectorPromotionValueMatcher::UrlHostSuffix {
                    suffix: ".myqcloud.com".to_string(),
                },
            },
        ]);
        assert!(promotion_requirements_match(&promotion, &values));

        let case_sensitive =
            promotion_with_requirements(vec![StorageConnectorPromotionRequirement {
                source_field: "provider".to_string(),
                matcher: StorageConnectorPromotionValueMatcher::StringEquals {
                    value: "tencent-cos".to_string(),
                    case_sensitive: true,
                },
            }]);
        assert!(!promotion_requirements_match(&case_sensitive, &values));

        for endpoint in [
            "not a url",
            "https://evilmyqcloud.com",
            "https://myqcloud.com",
        ] {
            let invalid_values =
                BTreeMap::from([("endpoint".to_string(), serde_json::json!(endpoint))]);
            let url_requirement =
                promotion_with_requirements(vec![StorageConnectorPromotionRequirement {
                    source_field: "endpoint".to_string(),
                    matcher: StorageConnectorPromotionValueMatcher::UrlHostSuffix {
                        suffix: ".myqcloud.com".to_string(),
                    },
                }]);
            assert!(!promotion_requirements_match(
                &url_requirement,
                &invalid_values
            ));
        }

        let missing = promotion_with_requirements(vec![StorageConnectorPromotionRequirement {
            source_field: "missing".to_string(),
            matcher: StorageConnectorPromotionValueMatcher::StringEquals {
                value: "value".to_string(),
                case_sensitive: false,
            },
        }]);
        assert!(!promotion_requirements_match(&missing, &values));
    }

    #[test]
    fn promotion_mapping_rejects_missing_and_changed_values() {
        let mappings = vec![StorageConnectorPromotionFieldMapping {
            source_field: "bucket".to_string(),
            target_field: "container".to_string(),
            preserve_value: true,
        }];
        let source = BTreeMap::from([("bucket".to_string(), serde_json::json!("archive"))]);
        let mapped = map_promotion_config_values(&mappings, &source).unwrap();
        assert_eq!(mapped["container"], "archive");
        ensure_preserved_promotion_values(&mappings, &source, &mapped).unwrap();

        let changed = BTreeMap::from([("container".to_string(), serde_json::json!("other"))]);
        assert!(
            ensure_preserved_promotion_values(&mappings, &source, &changed)
                .expect_err("changed preserved value must fail")
                .message()
                .contains("cannot change preserved field")
        );
        assert!(
            map_promotion_config_values(&mappings, &BTreeMap::new())
                .expect_err("missing source config must fail")
                .message()
                .contains("source config field 'bucket' is missing")
        );

        let credential_mappings = vec![StorageConnectorPromotionFieldMapping {
            source_field: "access_key".to_string(),
            target_field: "secret_id".to_string(),
            preserve_value: false,
        }];
        assert!(
            map_promotion_credential_values(&credential_mappings, &serde_json::json!("bad"))
                .expect_err("non-object credential must fail")
                .message()
                .contains("must be an object")
        );
        assert!(
            map_promotion_credential_values(&credential_mappings, &serde_json::json!({}))
                .expect_err("missing credential field must fail")
                .message()
                .contains("missing promotion field 'access_key'")
        );
    }

    #[tokio::test]
    async fn promotion_policy_guard_detects_connector_config_and_revision_changes() {
        let state = setup_state().await;
        let created = create(&state, local_policy_input("Promotion guard"))
            .await
            .unwrap();
        let expected = policy_repo::find_by_id(state.writer_db(), created.id)
            .await
            .unwrap();
        ensure_promotion_policy_unchanged(&expected, &expected).unwrap();

        let mut changed_connector = expected.clone();
        changed_connector.connector_id = "com.example.changed".to_string();
        assert!(
            ensure_promotion_policy_unchanged(&expected, &changed_connector)
                .expect_err("connector change must fail")
                .message()
                .contains("changed while connector promotion")
        );

        let mut changed_config = expected.clone();
        changed_config.storage_config =
            aster_drive_model::types::StoredStoragePolicyConfig("{\"changed\":true}".to_string());
        assert!(ensure_promotion_policy_unchanged(&expected, &changed_config).is_err());

        let mut changed_revision = expected.clone();
        changed_revision.updated_at += chrono::Duration::seconds(1);
        assert!(ensure_promotion_policy_unchanged(&expected, &changed_revision).is_err());
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

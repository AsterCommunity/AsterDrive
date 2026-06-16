//! 存储策略服务子模块：`policies`。

use chrono::Utc;
use sea_orm::{ActiveModelTrait, Set};

use crate::api::api_error_code::ApiErrorCode;
use crate::api::pagination::{AdminPolicySortBy, OffsetPage, SortOrder, load_offset_page};
use crate::config::site_url;
use crate::db::repository::{
    file_repo, managed_follower_repo, policy_group_repo, policy_repo, upload_session_repo,
};
use crate::entities::storage_policy;
use crate::errors::{AsterError, MapAsterErr, Result, validation_error_with_code};
use crate::runtime::{RemoteProtocolRuntimeState, SharedRuntimeState, TaskRuntimeState};
use crate::storage::drivers::azure_blob::AzureBlobDriver;
use crate::storage::drivers::tencent_cos::TencentCosDriver;
use crate::types::{
    DriverType, StoragePolicyOptions, StoredStoragePolicyAllowedTypes, parse_storage_policy_options,
};

use super::models::{
    ConfigureTencentCosCorsInput, CreateStoragePolicyInput, ExecuteDraftStoragePolicyActionInput,
    ExecuteSavedStoragePolicyActionInput, PromoteS3CompatiblePolicyDriverInput, StoragePolicy,
    StoragePolicyActionResult, StoragePolicyActionType, StoragePolicyCapacityInfo,
    StoragePolicyConnectionInput, TencentCosCorsConfigResult, UpdateStoragePolicyInput,
};
use super::shared::{
    SYSTEM_STORAGE_POLICY_ID, ensure_singleton_group_for_policy, lock_default_group_assignment,
    normalize_connection_fields, serialize_allowed_types, serialize_options,
    validate_remote_binding,
};

fn driver_type_name(driver_type: DriverType) -> &'static str {
    match driver_type {
        DriverType::Local => "local",
        DriverType::S3 => "s3",
        DriverType::AzureBlob => "azure_blob",
        DriverType::TencentCos => "tencent_cos",
        DriverType::Remote => "remote",
        DriverType::OneDrive => "onedrive",
    }
}

fn storage_policy_credential_label(driver_type: DriverType) -> &'static str {
    match driver_type {
        DriverType::S3 => "S3-compatible",
        DriverType::AzureBlob => "Azure Blob",
        _ => driver_type_name(driver_type),
    }
}

fn ensure_storage_native_thumbnail_supported(
    driver_type: DriverType,
    options: &StoragePolicyOptions,
) -> Result<()> {
    if !options.uses_storage_native_thumbnail() {
        return Ok(());
    }

    if crate::storage::driver_type_supports_native_thumbnail(driver_type) {
        return Ok(());
    }

    Err(validation_error_with_code(
        ApiErrorCode::PolicyNativeThumbnailUnsupported,
        format!(
            "storage policy driver '{}' does not expose storage-native thumbnail processing",
            driver_type_name(driver_type),
        ),
    ))
}

fn is_allowed_s3_compatible_promotion(source: DriverType, target: DriverType) -> bool {
    // Promotion is an in-place metadata switch, not an object copy. Keep this
    // whitelist explicit so future OSS/OBS-style drivers must add their own
    // validation and object-compatibility checks before the UI exposes them.
    matches!((source, target), (DriverType::S3, DriverType::TencentCos))
}

fn validate_connection_secret(value: &str, field: &str, driver: &str) -> Result<()> {
    if value.trim().is_empty() {
        let api_code = match field {
            "access_key" => ApiErrorCode::PolicyStorageAccessKeyRequired,
            "secret_key" => ApiErrorCode::PolicyStorageSecretKeyRequired,
            _ => ApiErrorCode::BadRequest,
        };
        return Err(validation_error_with_code(
            api_code,
            format!("{field} is required for {driver} storage policies"),
        ));
    }
    Ok(())
}

fn validate_connection_credentials(
    driver_type: DriverType,
    access_key: &str,
    secret_key: &str,
) -> Result<()> {
    match driver_type {
        DriverType::S3 | DriverType::TencentCos | DriverType::AzureBlob => {
            let driver = storage_policy_credential_label(driver_type);
            validate_connection_secret(access_key, "access_key", driver)?;
            validate_connection_secret(secret_key, "secret_key", driver)?;
        }
        DriverType::Local | DriverType::Remote | DriverType::OneDrive => {}
    }
    Ok(())
}

fn ensure_onedrive_options_supported(
    driver_type: DriverType,
    options: &StoragePolicyOptions,
) -> Result<()> {
    let has_onedrive_options = options.onedrive_cloud.is_some()
        || options.onedrive_account_mode.is_some()
        || options.onedrive_tenant.is_some()
        || options.onedrive_drive_id.is_some()
        || options.onedrive_root_item_id.is_some()
        || options.onedrive_site_id.is_some()
        || options.onedrive_group_id.is_some();
    if driver_type != DriverType::OneDrive {
        if has_onedrive_options {
            return Err(validation_error_with_code(
                ApiErrorCode::PolicyOneDriveOptionsUnsupported,
                "OneDrive options are only valid for OneDrive storage policies",
            ));
        }
        return Ok(());
    }

    if options.onedrive_account_mode.is_none() {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyOneDriveAccountModeRequired,
            "OneDrive storage policies require onedrive_account_mode",
        ));
    }
    if options.onedrive_cloud == Some(crate::types::MicrosoftGraphCloud::China)
        && options.onedrive_account_mode == Some(crate::types::OneDriveAccountMode::Personal)
    {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyOneDrivePersonalChinaCloudUnsupported,
            "personal OneDrive accounts must use the global Microsoft Graph cloud",
        ));
    }
    if options.onedrive_account_mode == Some(crate::types::OneDriveAccountMode::SharepointSite)
        && options.onedrive_drive_id.is_none()
        && options.onedrive_site_id.is_none()
    {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyOneDriveSharePointSiteRequired,
            "OneDrive sharepoint_site policies require onedrive_site_id when onedrive_drive_id is not set",
        ));
    }
    if options.onedrive_account_mode == Some(crate::types::OneDriveAccountMode::SharepointSite)
        && options.onedrive_group_id.is_some()
    {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyOneDriveOptionsUnsupported,
            "onedrive_group_id is only valid for OneDrive group_drive policies",
        ));
    }
    if options.onedrive_account_mode == Some(crate::types::OneDriveAccountMode::GroupDrive)
        && options.onedrive_drive_id.is_none()
        && options.onedrive_group_id.is_none()
    {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyOneDriveGroupRequired,
            "OneDrive group_drive policies require onedrive_group_id when onedrive_drive_id is not set",
        ));
    }
    if options.onedrive_account_mode == Some(crate::types::OneDriveAccountMode::GroupDrive)
        && options.onedrive_site_id.is_some()
    {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyOneDriveOptionsUnsupported,
            "onedrive_site_id is only valid for OneDrive sharepoint_site policies",
        ));
    }

    Ok(())
}

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
                driver_type_name(driver_type)
            ))
        }
        Err(error) => {
            tracing::warn!(
                driver_type = driver_type_name(driver_type),
                "storage capacity observability failed: {error}"
            );
            crate::storage::StorageCapacityInfo::unavailable(format!(
                "{}_driver",
                driver_type_name(driver_type)
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
    } = input;
    let StoragePolicyConnectionInput {
        driver_type,
        endpoint,
        bucket,
        access_key,
        secret_key,
        base_path,
        remote_node_id,
        options: _,
    } = connection;
    let (endpoint, bucket) = normalize_connection_fields(driver_type, &endpoint, &bucket)?;
    validate_connection_credentials(driver_type, &access_key, &secret_key)?;
    let remote_node_id =
        validate_remote_binding(state.writer_db(), driver_type, remote_node_id).await?;
    let allowed_types = allowed_types.unwrap_or_default();
    let options = options.unwrap_or_default().normalized();
    let serialized_options = serialize_options(&options)?;
    let chunk_size = chunk_size.unwrap_or(5_242_880);
    ensure_storage_native_thumbnail_supported(driver_type, &options)?;
    ensure_onedrive_options_supported(driver_type, &options)?;
    ensure_remote_transport_supports_policy_options(
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

pub async fn delete(state: &impl TaskRuntimeState, id: i64, force: bool) -> Result<()> {
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
    } = input;
    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    let existing = policy_repo::find_by_id(&txn, id).await?;
    let existing_endpoint = existing.endpoint.clone();
    let existing_bucket = existing.bucket.clone();
    let existing_access_key = existing.access_key.clone();
    let existing_secret_key = existing.secret_key.clone();
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
    let (normalized_endpoint, normalized_bucket) =
        normalize_connection_fields(existing.driver_type, &final_endpoint, &final_bucket)?;
    validate_connection_credentials(existing.driver_type, &final_access_key, &final_secret_key)?;
    let normalized_remote_node_id = validate_remote_binding(
        &txn,
        existing.driver_type,
        remote_node_id.or(existing.remote_node_id),
    )
    .await?;
    let options_provided = options.is_some();
    let final_options = options.unwrap_or(existing_options).normalized();
    let serialized_final_options = serialize_options(&final_options)?;
    ensure_storage_native_thumbnail_supported(existing.driver_type, &final_options)?;
    ensure_onedrive_options_supported(existing.driver_type, &final_options)?;
    ensure_remote_transport_supports_policy_options(
        &txn,
        existing.driver_type,
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
    if let Some(v) = access_key {
        active.access_key = Set(v);
    }
    if let Some(v) = secret_key {
        active.secret_key = Set(v);
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
    if existing.driver_type != DriverType::S3 {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyPromotionSourceUnsupported,
            "only generic S3-compatible policies can be promoted",
        ));
    }
    if !is_allowed_s3_compatible_promotion(existing.driver_type, input.target_driver_type) {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyPromotionTargetUnsupported,
            format!(
                "promoting S3-compatible policy to '{}' is not supported",
                driver_type_name(input.target_driver_type),
            ),
        ));
    }

    let (normalized_endpoint, normalized_bucket) =
        normalize_connection_fields(input.target_driver_type, &input.endpoint, &input.bucket)?;
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
    match candidate_policy.driver_type {
        DriverType::TencentCos => TencentCosDriver::validate_policy(candidate_policy)?,
        target => {
            return Err(validation_error_with_code(
                ApiErrorCode::PolicyPromotionTargetUnsupported,
                format!(
                    "promoting S3-compatible policy to '{}' is not supported",
                    driver_type_name(target),
                ),
            ));
        }
    }

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

pub async fn test_default_connection<S: SharedRuntimeState>(state: &S) -> Result<()> {
    let policy = state
        .policy_snapshot()
        .system_default_policy()
        .ok_or_else(|| {
            AsterError::storage_policy_not_found("system default storage policy not found")
        })?;
    let driver = state.driver_registry().get_driver(&policy)?;
    probe_storage_driver(driver.as_ref(), "default storage readiness probe failed").await
}

pub async fn test_connection<S: SharedRuntimeState>(state: &S, id: i64) -> Result<()> {
    let policy = policy_repo::find_by_id(state.writer_db(), id).await?;
    let driver = state.driver_registry().get_driver(&policy)?;
    probe_storage_driver(driver.as_ref(), "write test failed").await
}

pub async fn test_connection_params<S: RemoteProtocolRuntimeState>(
    state: &S,
    input: StoragePolicyConnectionInput,
) -> Result<()> {
    use crate::storage::drivers::local::LocalDriver;
    use crate::storage::drivers::s3::S3Driver;

    let StoragePolicyConnectionInput {
        driver_type,
        endpoint,
        bucket,
        access_key,
        secret_key,
        base_path,
        remote_node_id,
        options,
    } = input;
    let (endpoint, bucket) = normalize_connection_fields(driver_type, &endpoint, &bucket)?;
    validate_connection_credentials(driver_type, &access_key, &secret_key)?;
    let remote_node_id =
        validate_remote_binding(state.writer_db(), driver_type, remote_node_id).await?;
    let options = options.normalized();
    ensure_onedrive_options_supported(driver_type, &options)?;

    let fake_policy = storage_policy::Model {
        id: 0,
        name: String::new(),
        driver_type,
        endpoint,
        bucket,
        access_key,
        secret_key,
        base_path,
        remote_node_id,
        max_file_size: 0,
        allowed_types: StoredStoragePolicyAllowedTypes::empty(),
        options: serialize_options(&options)?,
        is_default: false,
        chunk_size: 0,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let driver: Box<dyn crate::storage::StorageDriver> = match driver_type {
        DriverType::Local => Box::new(LocalDriver::new(&fake_policy)?),
        DriverType::Remote => {
            let remote_node_id = remote_node_id
                .expect("validate_remote_binding must require remote_node_id for remote policies");
            let remote_node =
                managed_follower_repo::find_by_id(state.writer_db(), remote_node_id).await?;
            Box::new(
                state
                    .remote_protocol()
                    .driver_for_policy(&fake_policy, &remote_node)?,
            )
        }
        DriverType::S3 => Box::new(S3Driver::new(&fake_policy)?),
        DriverType::AzureBlob => Box::new(AzureBlobDriver::new(&fake_policy)?),
        DriverType::TencentCos => Box::new(TencentCosDriver::new(&fake_policy)?),
        DriverType::OneDrive => {
            // OneDrive credentials are OAuth records bound to a saved policy id.
            // A draft policy uses id=0 here, so there is no authorization context
            // to reuse for a real Microsoft Graph probe.
            return Err(validation_error_with_code(
                ApiErrorCode::PolicyActionUnsupported,
                "OneDrive draft connection tests require a saved storage policy with completed Microsoft Graph authorization; use the saved policy connection test after authorization",
            ));
        }
    };

    probe_storage_driver(driver.as_ref(), "connection test failed").await
}

pub async fn configure_tencent_cos_cors_for_policy<S: SharedRuntimeState>(
    state: &S,
    id: i64,
) -> Result<TencentCosCorsConfigResult> {
    let policy = policy_repo::find_by_id(state.writer_db(), id).await?;
    if policy.driver_type != DriverType::TencentCos {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyActionUnsupported,
            "storage policy action 'configure_tencent_cos_cors' only supports Tencent COS storage policies",
        ));
    }
    let origins = resolve_cos_cors_allowed_origins(state)?;
    let driver = TencentCosDriver::new(&policy)?;
    driver
        .configure_asterdrive_cors(&origins)
        .await
        .map(Into::into)
}

pub async fn configure_tencent_cos_cors<S: RemoteProtocolRuntimeState>(
    state: &S,
    input: ConfigureTencentCosCorsInput,
) -> Result<TencentCosCorsConfigResult> {
    let ConfigureTencentCosCorsInput { connection } = input;
    let fake_policy = build_connection_test_policy(state, connection).await?;
    if fake_policy.driver_type != DriverType::TencentCos {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyActionUnsupported,
            "storage policy action 'configure_tencent_cos_cors' only supports Tencent COS storage policies",
        ));
    }
    let origins = resolve_cos_cors_allowed_origins(state)?;
    let driver = TencentCosDriver::new(&fake_policy)?;
    driver
        .configure_asterdrive_cors(&origins)
        .await
        .map(Into::into)
}

async fn merge_draft_action_saved_credentials<S: SharedRuntimeState>(
    state: &S,
    policy_id: Option<i64>,
    mut connection: StoragePolicyConnectionInput,
) -> Result<StoragePolicyConnectionInput> {
    if (connection.access_key.trim().is_empty() || connection.secret_key.trim().is_empty())
        && let Some(policy_id) = policy_id
    {
        let saved = policy_repo::find_by_id(state.reader_db(), policy_id).await?;
        if saved.driver_type != connection.driver_type {
            return Err(validation_error_with_code(
                ApiErrorCode::PolicyActionParameterInvalid,
                format!(
                    "draft storage policy action driver '{}' does not match saved policy driver '{}'",
                    driver_type_name(connection.driver_type),
                    driver_type_name(saved.driver_type),
                ),
            ));
        }
        if connection.access_key.trim().is_empty() {
            connection.access_key = saved.access_key;
        }
        if connection.secret_key.trim().is_empty() {
            connection.secret_key = saved.secret_key;
        }
    }
    Ok(connection)
}

pub async fn execute_saved_action<S: SharedRuntimeState>(
    state: &S,
    id: i64,
    input: ExecuteSavedStoragePolicyActionInput,
) -> Result<StoragePolicyActionResult> {
    match input.action {
        StoragePolicyActionType::ConfigureTencentCosCors => {
            let result = configure_tencent_cos_cors_for_policy(state, id).await?;
            Ok(StoragePolicyActionResult {
                action: input.action,
                tencent_cos_cors: Some(result),
            })
        }
    }
}

pub async fn execute_draft_action<S: RemoteProtocolRuntimeState>(
    state: &S,
    input: ExecuteDraftStoragePolicyActionInput,
) -> Result<StoragePolicyActionResult> {
    match input.action {
        StoragePolicyActionType::ConfigureTencentCosCors => {
            let connection =
                merge_draft_action_saved_credentials(state, input.policy_id, input.connection)
                    .await?;
            let result =
                configure_tencent_cos_cors(state, ConfigureTencentCosCorsInput { connection })
                    .await?;
            Ok(StoragePolicyActionResult {
                action: input.action,
                tencent_cos_cors: Some(result),
            })
        }
    }
}

async fn ensure_remote_transport_supports_policy_options<C: sea_orm::ConnectionTrait>(
    db: &C,
    driver_type: DriverType,
    remote_node_id: Option<i64>,
    options: &crate::types::StoragePolicyOptions,
) -> Result<()> {
    if driver_type != DriverType::Remote {
        return Ok(());
    }
    let Some(remote_node_id) = remote_node_id else {
        return Ok(());
    };
    let remote_node = managed_follower_repo::find_by_id(db, remote_node_id).await?;
    if remote_node
        .transport_mode
        .resolves_to_reverse_tunnel(&remote_node.base_url)
        && (options.effective_remote_download_strategy()
            == crate::types::RemoteDownloadStrategy::Presigned
            || options.effective_remote_upload_strategy()
                == crate::types::RemoteUploadStrategy::Presigned)
    {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyRemoteNodeTransferStrategyUnsupported,
            "reverse tunnel remote nodes do not support presigned browser transfer strategies",
        ));
    }
    Ok(())
}

async fn build_connection_test_policy<S: RemoteProtocolRuntimeState>(
    state: &S,
    input: StoragePolicyConnectionInput,
) -> Result<storage_policy::Model> {
    let StoragePolicyConnectionInput {
        driver_type,
        endpoint,
        bucket,
        access_key,
        secret_key,
        base_path,
        remote_node_id,
        options,
    } = input;
    let (endpoint, bucket) = normalize_connection_fields(driver_type, &endpoint, &bucket)?;
    validate_connection_credentials(driver_type, &access_key, &secret_key)?;
    let remote_node_id =
        validate_remote_binding(state.writer_db(), driver_type, remote_node_id).await?;
    let options = options.normalized();
    ensure_onedrive_options_supported(driver_type, &options)?;

    Ok(storage_policy::Model {
        id: 0,
        name: String::new(),
        driver_type,
        endpoint,
        bucket,
        access_key,
        secret_key,
        base_path,
        remote_node_id,
        max_file_size: 0,
        allowed_types: StoredStoragePolicyAllowedTypes::empty(),
        options: serialize_options(&options)?,
        is_default: false,
        chunk_size: 0,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    })
}

fn resolve_cos_cors_allowed_origins(state: &impl SharedRuntimeState) -> Result<Vec<String>> {
    let origins = site_url::public_site_urls(state.runtime_config());
    if origins.is_empty() {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyActionParameterRequired,
            "public_site_url must be configured before configuring COS CORS",
        ));
    }
    Ok(origins)
}

impl From<crate::storage::drivers::tencent_cos::cors::TencentCosCorsApplyResult>
    for TencentCosCorsConfigResult
{
    fn from(value: crate::storage::drivers::tencent_cos::cors::TencentCosCorsApplyResult) -> Self {
        Self {
            rule_id: value.rule_id,
            allowed_origins: value.allowed_origins,
            request_id: value.request_id,
            preserved_rule_count: value.preserved_rule_count,
            replaced_existing_rule: value.replaced_existing_rule,
            response_vary: value.response_vary,
        }
    }
}

async fn probe_storage_driver(
    driver: &dyn crate::storage::StorageDriver,
    write_error_context: &'static str,
) -> Result<()> {
    let test_path = format!("_aster_connection_test-{}", uuid::Uuid::new_v4());
    driver
        .put(&test_path, b"ok")
        .await
        .map_aster_err_ctx(write_error_context, AsterError::storage_driver_error)?;
    driver
        .delete(&test_path)
        .await
        .inspect_err(|error| {
            tracing::warn!(path = %test_path, "failed to clean up connection test file: {error}");
        })
        .map_aster_err_ctx(
            "connection test cleanup failed",
            AsterError::storage_driver_error,
        )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ensure_onedrive_options_supported;
    use crate::api::api_error_code::ApiErrorCode;
    use crate::types::{
        DriverType, MicrosoftGraphCloud, OneDriveAccountMode, StoragePolicyOptions,
    };

    #[test]
    fn onedrive_options_are_rejected_for_non_onedrive_policy() {
        let options = StoragePolicyOptions {
            onedrive_account_mode: Some(OneDriveAccountMode::WorkOrSchool),
            onedrive_drive_id: Some("drive".to_string()),
            onedrive_root_item_id: Some("root".to_string()),
            ..Default::default()
        };

        let error = ensure_onedrive_options_supported(DriverType::S3, &options).unwrap_err();

        assert_eq!(
            error.api_error_code(),
            ApiErrorCode::PolicyOneDriveOptionsUnsupported
        );
        assert!(
            error
                .to_string()
                .contains("OneDrive options are only valid for OneDrive")
        );
    }

    #[test]
    fn onedrive_policy_accepts_automatic_default_drive() {
        let options = StoragePolicyOptions {
            onedrive_account_mode: Some(OneDriveAccountMode::WorkOrSchool),
            ..Default::default()
        };

        ensure_onedrive_options_supported(DriverType::OneDrive, &options)
            .expect("work or school OneDrive resolves the default drive during authorization");
    }

    #[test]
    fn onedrive_policy_requires_account_mode() {
        let options = StoragePolicyOptions {
            onedrive_root_item_id: Some("root".to_string()),
            ..Default::default()
        };

        let error = ensure_onedrive_options_supported(DriverType::OneDrive, &options).unwrap_err();

        assert_eq!(
            error.api_error_code(),
            ApiErrorCode::PolicyOneDriveAccountModeRequired
        );
        assert!(
            error
                .to_string()
                .contains("OneDrive storage policies require onedrive_account_mode")
        );
    }

    #[test]
    fn onedrive_policy_rejects_personal_china_cloud() {
        let options = StoragePolicyOptions {
            onedrive_cloud: Some(MicrosoftGraphCloud::China),
            onedrive_account_mode: Some(OneDriveAccountMode::Personal),
            ..Default::default()
        };

        let error = ensure_onedrive_options_supported(DriverType::OneDrive, &options).unwrap_err();

        assert_eq!(
            error.api_error_code(),
            ApiErrorCode::PolicyOneDrivePersonalChinaCloudUnsupported
        );
        assert!(error.to_string().contains("global Microsoft Graph cloud"));
    }

    #[test]
    fn onedrive_sharepoint_site_requires_site_id_without_drive_id() {
        let options = StoragePolicyOptions {
            onedrive_account_mode: Some(OneDriveAccountMode::SharepointSite),
            ..Default::default()
        };

        let error = ensure_onedrive_options_supported(DriverType::OneDrive, &options).unwrap_err();

        assert_eq!(
            error.api_error_code(),
            ApiErrorCode::PolicyOneDriveSharePointSiteRequired
        );
        assert!(error.to_string().contains("onedrive_site_id"));
    }

    #[test]
    fn onedrive_group_drive_requires_group_id_without_drive_id() {
        let options = StoragePolicyOptions {
            onedrive_account_mode: Some(OneDriveAccountMode::GroupDrive),
            ..Default::default()
        };

        let error = ensure_onedrive_options_supported(DriverType::OneDrive, &options).unwrap_err();

        assert_eq!(
            error.api_error_code(),
            ApiErrorCode::PolicyOneDriveGroupRequired
        );
        assert!(error.to_string().contains("onedrive_group_id"));
    }

    #[test]
    fn onedrive_modes_reject_other_mode_target_ids() {
        let options = StoragePolicyOptions {
            onedrive_account_mode: Some(OneDriveAccountMode::SharepointSite),
            onedrive_site_id: Some("site".to_string()),
            onedrive_group_id: Some("group".to_string()),
            ..Default::default()
        };

        let error = ensure_onedrive_options_supported(DriverType::OneDrive, &options).unwrap_err();

        assert_eq!(
            error.api_error_code(),
            ApiErrorCode::PolicyOneDriveOptionsUnsupported
        );
        assert!(error.to_string().contains("onedrive_group_id"));

        let options = StoragePolicyOptions {
            onedrive_account_mode: Some(OneDriveAccountMode::GroupDrive),
            onedrive_site_id: Some("site".to_string()),
            onedrive_group_id: Some("group".to_string()),
            ..Default::default()
        };

        let error = ensure_onedrive_options_supported(DriverType::OneDrive, &options).unwrap_err();

        assert_eq!(
            error.api_error_code(),
            ApiErrorCode::PolicyOneDriveOptionsUnsupported
        );
        assert!(error.to_string().contains("onedrive_site_id"));
    }
}

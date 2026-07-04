use chrono::Utc;
use sea_orm::Set;

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::remote_storage_target_repo;
use crate::entities::{master_binding, remote_storage_target};
use crate::errors::{AsterError, Result, precondition_failed_with_code};
use crate::runtime::FollowerRuntimeState;
use crate::storage::remote_protocol::{
    RemoteCreateStorageTargetRequest, RemoteStorageTargetInfo, RemoteUpdateStorageTargetRequest,
};

use super::normalization::{new_target_key, normalize_create_input, normalize_update_input};
use super::reconciliation::reconcile_target;

pub async fn list<S: FollowerRuntimeState>(
    state: &S,
    binding: &master_binding::Model,
) -> Result<Vec<RemoteStorageTargetInfo>> {
    Ok(
        remote_storage_target_repo::find_all_by_binding(state.writer_db(), binding.id)
            .await?
            .into_iter()
            .map(Into::into)
            .collect(),
    )
}

pub async fn create<S: FollowerRuntimeState>(
    state: &S,
    binding: &master_binding::Model,
    input: RemoteCreateStorageTargetRequest,
) -> Result<RemoteStorageTargetInfo> {
    let normalized = normalize_create_input(input)?;
    let target_id = crate::db::transaction::with_transaction(state.writer_db(), async |txn| {
        let should_set_default = normalized.is_default == Some(true)
            || remote_storage_target_repo::count_by_binding(txn, binding.id).await? == 0;
        let now = Utc::now();
        let created = remote_storage_target_repo::create(
            txn,
            remote_storage_target::ActiveModel {
                master_binding_id: Set(binding.id),
                target_key: Set(new_target_key()),
                name: Set(normalized.name),
                driver_type: Set(normalized.driver_type),
                endpoint: Set(normalized.endpoint),
                bucket: Set(normalized.bucket),
                access_key: Set(normalized.access_key),
                secret_key: Set(normalized.secret_key),
                base_path: Set(normalized.base_path),
                max_file_size: Set(normalized.max_file_size),
                is_default: Set(false),
                desired_revision: Set(1),
                applied_revision: Set(0),
                last_error: Set(String::new()),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .await?;
        if should_set_default {
            remote_storage_target_repo::set_only_default_for_binding(txn, binding.id, created.id)
                .await?;
        }
        Ok(created.id)
    })
    .await?;
    let target = remote_storage_target_repo::find_by_id(state.writer_db(), target_id).await?;
    Ok(reconcile_target(state, target).await?.into())
}

pub async fn update<S: FollowerRuntimeState>(
    state: &S,
    binding: &master_binding::Model,
    target_key: &str,
    input: RemoteUpdateStorageTargetRequest,
) -> Result<RemoteStorageTargetInfo> {
    let existing = find_target_or_err(state, binding.id, target_key).await?;
    let normalized = normalize_update_input(existing.clone(), input)?;

    if existing.is_default && normalized.is_default == Some(false) {
        return Err(precondition_failed_with_code(
            ApiErrorCode::ManagedIngressDefaultUpdateRequiresReplacement,
            "cannot unset the default remote storage target directly; set another target as default first",
        ));
    }

    let target_id = crate::db::transaction::with_transaction(state.writer_db(), async |txn| {
        let mut active: remote_storage_target::ActiveModel = existing.clone().into();
        active.name = Set(normalized.name);
        active.driver_type = Set(normalized.driver_type);
        active.endpoint = Set(normalized.endpoint);
        active.bucket = Set(normalized.bucket);
        active.access_key = Set(normalized.access_key);
        active.secret_key = Set(normalized.secret_key);
        active.base_path = Set(normalized.base_path);
        active.max_file_size = Set(normalized.max_file_size);
        active.desired_revision =
            Set(existing.desired_revision.checked_add(1).ok_or_else(|| {
                AsterError::internal_error("remote storage target desired_revision overflow")
            })?);
        active.updated_at = Set(Utc::now());
        let updated = remote_storage_target_repo::update(txn, active).await?;
        if normalized.is_default == Some(true) {
            remote_storage_target_repo::set_only_default_for_binding(txn, binding.id, updated.id)
                .await?;
        }
        Ok(updated.id)
    })
    .await?;
    let target = remote_storage_target_repo::find_by_id(state.writer_db(), target_id).await?;
    Ok(reconcile_target(state, target).await?.into())
}

pub async fn delete<S: FollowerRuntimeState>(
    state: &S,
    binding: &master_binding::Model,
    target_key: &str,
) -> Result<RemoteStorageTargetInfo> {
    let existing = find_target_or_err(state, binding.id, target_key).await?;
    tracing::debug!(
        binding_id = binding.id,
        target_key = %existing.target_key,
        is_default = existing.is_default,
        "deleting managed remote storage target"
    );
    let count = remote_storage_target_repo::count_by_binding(state.writer_db(), binding.id).await?;
    if existing.is_default && count > 1 {
        return Err(precondition_failed_with_code(
            ApiErrorCode::ManagedIngressDefaultDeleteRequiresReplacement,
            "cannot delete the default remote storage target while other targets still exist; set another target as default first",
        ));
    }
    remote_storage_target_repo::delete_by_binding_and_target_key(
        state.writer_db(),
        binding.id,
        &existing.target_key,
    )
    .await?;
    tracing::info!(
        binding_id = binding.id,
        target_key = %existing.target_key,
        "deleted managed remote storage target"
    );
    Ok(existing.into())
}

async fn find_target_or_err<S: FollowerRuntimeState>(
    state: &S,
    master_binding_id: i64,
    target_key: &str,
) -> Result<remote_storage_target::Model> {
    remote_storage_target_repo::find_by_binding_and_target_key(
        state.writer_db(),
        master_binding_id,
        target_key,
    )
    .await?
    .ok_or_else(|| AsterError::record_not_found(format!("remote_storage_target '{target_key}'")))
}

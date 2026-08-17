use aster_forge_db::transaction;
use chrono::Utc;
use sea_orm::Set;

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{remote_storage_target_credential_repo, remote_storage_target_repo};
use crate::errors::{AsterError, Result, precondition_failed_with_code};
use crate::runtime::FollowerRuntimeState;
use crate::storage::remote_protocol::{
    RemoteCreateStorageTargetRequest, RemoteStorageTargetInfo, RemoteUpdateStorageTargetRequest,
};
use aster_drive_model::entities::{master_binding, remote_storage_target};

use super::credential;
use super::driver::load_credential;
use super::models::present_target;
use super::normalization::{new_target_key, normalize_create_input, normalize_update_input};
use super::reconciliation::reconcile_target;

pub async fn list<S: FollowerRuntimeState>(
    state: &S,
    binding: &master_binding::Model,
) -> Result<Vec<RemoteStorageTargetInfo>> {
    let targets =
        remote_storage_target_repo::find_all_by_binding(state.writer_db(), binding.id).await?;
    let mut presented = Vec::with_capacity(targets.len());
    for target in targets {
        let configured =
            remote_storage_target_credential_repo::find_by_target(state.writer_db(), target.id)
                .await?
                .is_some();
        presented.push(present_target(target, configured)?);
    }
    Ok(presented)
}

pub async fn create<S: FollowerRuntimeState>(
    state: &S,
    binding: &master_binding::Model,
    input: RemoteCreateStorageTargetRequest,
) -> Result<RemoteStorageTargetInfo> {
    let normalized = normalize_create_input(input)?;
    let encryption_key = state.config().auth.storage_credential_secret_key.clone();
    let target_id = transaction::with_transaction(state.writer_db(), async |txn| {
        let should_set_default = normalized.is_default == Some(true)
            || remote_storage_target_repo::count_by_binding(txn, binding.id).await? == 0;
        let now = Utc::now();
        let connector_id = normalized
            .connector
            .config
            .connector_id
            .as_str()
            .to_string();
        let connector_config = serde_json::to_string(&normalized.connector.config)
            .map_err(|error| AsterError::internal_error(error.to_string()))?;
        let created = remote_storage_target_repo::create(
            txn,
            remote_storage_target::ActiveModel {
                master_binding_id: Set(binding.id),
                target_key: Set(new_target_key()),
                name: Set(normalized.name),
                connector_id: Set(connector_id.clone()),
                connector_config: Set(connector_config),
                driver_type: Set(String::new()),
                endpoint: Set(String::new()),
                bucket: Set(String::new()),
                access_key: Set(String::new()),
                secret_key: Set(String::new()),
                base_path: Set(String::new()),
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
        if let Some(plaintext) = normalized.connector.credential_json {
            let schema_version =
                normalized
                    .connector
                    .credential_schema_version
                    .ok_or_else(|| {
                        AsterError::internal_error(
                            "remote storage target credential is missing its schema version",
                        )
                    })?;
            let ciphertext = credential::encrypt(
                &encryption_key,
                created.id,
                &connector_id,
                schema_version,
                &plaintext,
            )?;
            remote_storage_target_credential_repo::upsert(
                txn,
                created.id,
                connector_id,
                schema_version as i32,
                ciphertext,
            )
            .await?;
        }
        if should_set_default {
            remote_storage_target_repo::set_only_default_for_binding(txn, binding.id, created.id)
                .await?;
        }
        Ok::<_, AsterError>(created.id)
    })
    .await?;
    reconcile_and_present(state, target_id).await
}

pub async fn update<S: FollowerRuntimeState>(
    state: &S,
    binding: &master_binding::Model,
    target_key: &str,
    input: RemoteUpdateStorageTargetRequest,
) -> Result<RemoteStorageTargetInfo> {
    let existing = find_target_or_err(state, binding.id, target_key).await?;
    let saved_credential = load_credential(state, &existing, &existing.connector_id).await?;
    let normalized = normalize_update_input(&existing, input, saved_credential)?;
    if existing.is_default && normalized.is_default == Some(false) {
        return Err(precondition_failed_with_code(
            ApiErrorCode::ManagedIngressDefaultUpdateRequiresReplacement,
            "cannot unset the default remote storage target directly; set another target as default first",
        ));
    }
    let encryption_key = state.config().auth.storage_credential_secret_key.clone();
    let target_id = transaction::with_transaction(state.writer_db(), async |txn| {
        let connector_id = normalized
            .connector
            .config
            .connector_id
            .as_str()
            .to_string();
        let connector_config = serde_json::to_string(&normalized.connector.config)
            .map_err(|error| AsterError::internal_error(error.to_string()))?;
        let mut active: remote_storage_target::ActiveModel = existing.clone().into();
        active.name = Set(normalized.name);
        active.connector_id = Set(connector_id.clone());
        active.connector_config = Set(connector_config);
        active.driver_type = Set(String::new());
        active.endpoint = Set(String::new());
        active.bucket = Set(String::new());
        active.access_key = Set(String::new());
        active.secret_key = Set(String::new());
        active.base_path = Set(String::new());
        active.desired_revision =
            Set(existing.desired_revision.checked_add(1).ok_or_else(|| {
                AsterError::internal_error("remote storage target desired_revision overflow")
            })?);
        active.updated_at = Set(Utc::now());
        let updated = remote_storage_target_repo::update(txn, active).await?;
        match normalized.connector.credential_json {
            Some(plaintext) => {
                let schema_version =
                    normalized
                        .connector
                        .credential_schema_version
                        .ok_or_else(|| {
                            AsterError::internal_error(
                                "remote storage target credential is missing its schema version",
                            )
                        })?;
                let ciphertext = credential::encrypt(
                    &encryption_key,
                    updated.id,
                    &connector_id,
                    schema_version,
                    &plaintext,
                )?;
                remote_storage_target_credential_repo::upsert(
                    txn,
                    updated.id,
                    connector_id,
                    schema_version as i32,
                    ciphertext,
                )
                .await?;
            }
            None => {
                remote_storage_target_credential_repo::delete_by_target(txn, updated.id).await?
            }
        }
        if normalized.is_default == Some(true) {
            remote_storage_target_repo::set_only_default_for_binding(txn, binding.id, updated.id)
                .await?;
        }
        Ok::<_, AsterError>(updated.id)
    })
    .await?;
    reconcile_and_present(state, target_id).await
}

pub async fn delete<S: FollowerRuntimeState>(
    state: &S,
    binding: &master_binding::Model,
    target_key: &str,
) -> Result<RemoteStorageTargetInfo> {
    let existing = find_target_or_err(state, binding.id, target_key).await?;
    let configured =
        remote_storage_target_credential_repo::find_by_target(state.writer_db(), existing.id)
            .await?
            .is_some();
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
    present_target(existing, configured)
}

async fn reconcile_and_present<S: FollowerRuntimeState>(
    state: &S,
    target_id: i64,
) -> Result<RemoteStorageTargetInfo> {
    let target = remote_storage_target_repo::find_by_id(state.writer_db(), target_id).await?;
    let target = reconcile_target(state, target).await?;
    let configured =
        remote_storage_target_credential_repo::find_by_target(state.writer_db(), target_id)
            .await?
            .is_some();
    present_target(target, configured)
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

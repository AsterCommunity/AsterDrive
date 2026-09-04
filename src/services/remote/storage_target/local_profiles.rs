use aster_forge_db::transaction;
use chrono::Utc;
use sea_orm::Set;

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::remote_storage_target_repo;
use crate::errors::{AsterError, Result, precondition_failed_with_code};
use crate::runtime::FollowerRuntimeState;
use crate::storage::remote_protocol::{
    RemoteCreateStorageTargetRequest, RemoteStorageTargetInfo, RemoteUpdateStorageTargetRequest,
};
use aster_drive_model::entities::{master_binding, remote_storage_target};

use super::normalization::{new_target_key, normalize_create_input, normalize_update_input};
use super::reconciliation::reconcile_target;

pub async fn list<S: FollowerRuntimeState>(
    state: &S,
    binding: &master_binding::Model,
) -> Result<Vec<RemoteStorageTargetInfo>> {
    remote_storage_target_repo::find_all_by_binding(state.writer_db(), binding.id)
        .await?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
}

pub async fn create<S: FollowerRuntimeState>(
    state: &S,
    binding: &master_binding::Model,
    input: RemoteCreateStorageTargetRequest,
) -> Result<RemoteStorageTargetInfo> {
    let normalized = normalize_create_input(state, input).await?;
    let connection = normalized.connection.ok_or_else(|| {
        AsterError::internal_error("normalized remote storage target has no connection")
    })?;
    let connector_id = connection.connector_config.connector_id.clone();
    let connector_config = encode_connector_config(&connection.connector_config)?;
    let target_id = transaction::with_transaction(state.writer_db(), async |txn| {
        let should_set_default = normalized.is_default == Some(true)
            || remote_storage_target_repo::count_by_binding(txn, binding.id).await? == 0;
        let now = Utc::now();
        let created = remote_storage_target_repo::create(
            txn,
            remote_storage_target::ActiveModel {
                master_binding_id: Set(binding.id),
                target_key: Set(new_target_key()),
                name: Set(normalized.name),
                connector_id: Set(Some(connector_id.to_string())),
                connector_config: Set(Some(connector_config)),
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
        persist_credential(state, txn, created.id, &connection).await?;
        if should_set_default {
            remote_storage_target_repo::set_only_default_for_binding(txn, binding.id, created.id)
                .await?;
        }
        Ok::<_, AsterError>(created.id)
    })
    .await?;
    let target = remote_storage_target_repo::find_by_id(state.writer_db(), target_id).await?;
    reconcile_target(state, target).await?.try_into()
}

pub async fn update<S: FollowerRuntimeState>(
    state: &S,
    binding: &master_binding::Model,
    target_key: &str,
    input: RemoteUpdateStorageTargetRequest,
) -> Result<RemoteStorageTargetInfo> {
    let existing = find_target_or_err(state, binding.id, target_key).await?;
    let normalized = normalize_update_input(state, &existing, input).await?;

    if existing.is_default && normalized.is_default == Some(false) {
        return Err(precondition_failed_with_code(
            ApiErrorCode::RemoteStorageTargetDefaultUpdateRequiresReplacement,
            "cannot unset the default remote storage target directly; set another target as default first",
        ));
    }

    let target_id = transaction::with_transaction(state.writer_db(), async |txn| {
        let mut active: remote_storage_target::ActiveModel = existing.clone().into();
        active.name = Set(normalized.name);
        if let Some(connection) = normalized.connection.as_ref() {
            active.connector_id = Set(Some(connection.connector_config.connector_id.to_string()));
            active.connector_config =
                Set(Some(encode_connector_config(&connection.connector_config)?));
        }
        active.desired_revision =
            Set(existing.desired_revision.checked_add(1).ok_or_else(|| {
                AsterError::internal_error("remote storage target desired_revision overflow")
            })?);
        active.updated_at = Set(Utc::now());
        let updated = remote_storage_target_repo::update(txn, active).await?;
        if let Some(connection) = normalized.connection.as_ref() {
            if existing.connector_id.as_deref()
                != Some(connection.connector_config.connector_id.as_str())
            {
                crate::db::repository::remote_storage_target_credential_repo::delete_by_target(
                    txn, updated.id,
                )
                .await?;
            }
            persist_credential(state, txn, updated.id, connection).await?;
        }
        if normalized.is_default == Some(true) {
            remote_storage_target_repo::set_only_default_for_binding(txn, binding.id, updated.id)
                .await?;
        }
        Ok::<_, AsterError>(updated.id)
    })
    .await?;
    let target = remote_storage_target_repo::find_by_id(state.writer_db(), target_id).await?;
    reconcile_target(state, target).await?.try_into()
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
            ApiErrorCode::RemoteStorageTargetDefaultDeleteRequiresReplacement,
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
    existing.try_into()
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

fn encode_connector_config(
    config: &aster_drive_storage::ConnectorConfigEnvelope,
) -> Result<String> {
    aster_drive_storage::encode_connector_config(
        config.connector_id.clone(),
        config.schema_version,
        &config.values,
    )
    .map_err(|error| AsterError::internal_error(format!("serialize connector config: {error}")))
}

async fn persist_credential<S: FollowerRuntimeState>(
    state: &S,
    txn: &sea_orm::DatabaseTransaction,
    target_id: i64,
    connection: &crate::storage::StorageConnectionInput,
) -> Result<()> {
    let connector = state
        .driver_registry()
        .connectors()
        .require_remote_target_connector(&connection.connector_config.connector_id)?;
    match &connection.credential {
        crate::storage::StorageConnectorCredentialInput::None => {
            crate::db::repository::remote_storage_target_credential_repo::delete_by_target(
                txn, target_id,
            )
            .await
        }
        crate::storage::StorageConnectorCredentialInput::Static(values) => {
            let schema_version = connector
                .descriptor()
                .credential_schema_version
                .ok_or_else(|| {
                    AsterError::internal_error(format!(
                        "storage connector '{}' has no credential schema version",
                        connection.connector_config.connector_id
                    ))
                })?;
            let plaintext = serde_json::to_string(values).map_err(|error| {
                AsterError::internal_error(format!("serialize connector credential: {error}"))
            })?;
            let ciphertext =
                crate::services::storage_policy::credential::crypto::encrypt_connector_credential(
                    &state.config().auth.storage_credential_secret_key,
                    target_id,
                    connection.connector_config.connector_id.as_str(),
                    schema_version,
                    &plaintext,
                )?;
            let schema_version = i32::try_from(schema_version).map_err(|_| {
                AsterError::validation_error(
                    "connector credential schema version exceeds database range",
                )
            })?;
            crate::db::repository::remote_storage_target_credential_repo::upsert(
                txn,
                target_id,
                connection.connector_config.connector_id.to_string(),
                schema_version,
                ciphertext,
            )
            .await
            .map(|_| ())
        }
        crate::storage::StorageConnectorCredentialInput::AuthorizationApplication(_) => {
            Err(AsterError::validation_error(
                "remote storage targets do not support authorization application credentials",
            ))
        }
    }
}

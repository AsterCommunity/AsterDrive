use std::sync::Arc;

use crate::errors::{AsterError, Result};
use crate::runtime::FollowerRuntimeState;
use crate::storage::connectors::LocalConnector;
use aster_drive_model::entities::remote_storage_target;
use aster_drive_storage::{ConnectorConfigEnvelope, ConnectorId};
use aster_drive_storage::{StorageConnectorDescriptor, StorageDriver};

pub(crate) fn remote_storage_target_descriptor_from_connector(
    connector: &dyn crate::storage::connectors::StorageConnector,
) -> Result<StorageConnectorDescriptor> {
    if !connector.supports_remote_storage_target() {
        return Err(AsterError::validation_error(format!(
            "storage connector '{}' is not a remote target provider",
            connector.descriptor().connector_id
        )));
    }
    Ok(connector.descriptor())
}

#[cfg(test)]
pub(crate) fn list_registered_remote_storage_target_connector_descriptors()
-> Result<Vec<StorageConnectorDescriptor>> {
    crate::storage::connectors::builtin_storage_connector_registry()?
        .remote_target_connectors()
        .into_iter()
        .map(remote_storage_target_descriptor_from_connector)
        .collect()
}
pub(in crate::services::remote::storage_target) async fn validate_driver_from_target<
    S: FollowerRuntimeState,
>(
    state: &S,
    target: &remote_storage_target::Model,
) -> Result<()> {
    build_driver_from_target(state, target).await.map(|_| ())
}

pub(in crate::services::remote::storage_target) async fn build_driver_from_target<
    S: FollowerRuntimeState,
>(
    state: &S,
    target: &remote_storage_target::Model,
) -> Result<Arc<dyn StorageDriver>> {
    let mut connection = load_connection_from_target(state, target).await?;
    let connector_id = connection.connector_config.connector_id.clone();
    let connector = state
        .driver_registry()
        .connectors()
        .require_remote_target_connector(&connector_id)?;
    connection.connector_config =
        connector.validate_connector_config(&connection.connector_config)?;
    connector.validate_credential_input(&connection.credential)?;
    if connector_id.as_str() == LocalConnector::ID {
        let relative = connection
            .connector_config
            .values
            .get("base_path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(".");
        let resolved = super::paths::resolve_remote_storage_target_local_path(
            &state
                .config()
                .server
                .follower
                .remote_storage_target_local_root,
            relative,
        )?;
        connection.connector_config.values.insert(
            "base_path".to_string(),
            serde_json::Value::String(resolved.to_string_lossy().into_owned()),
        );
    }
    let context = crate::storage::connectors::StorageConnectorContext::new(
        state.writer_db(),
        state.config(),
        state.runtime_config(),
        state.driver_registry(),
        None,
    );
    connector
        .build_driver_from_connection(
            &context,
            &connection.connector_config,
            &connection.credential,
        )
        .await
        .map(Arc::from)
}

pub(in crate::services::remote::storage_target) async fn load_connection_from_target<
    S: FollowerRuntimeState,
>(
    state: &S,
    target: &remote_storage_target::Model,
) -> Result<crate::storage::StorageConnectionInput> {
    let connector_id = target
        .connector_id
        .as_deref()
        .map(ConnectorId::declared)
        .ok_or_else(|| {
            AsterError::database_operation(format!(
                "remote storage target #{} has no connector id",
                target.id
            ))
        })?;
    let connector = state
        .driver_registry()
        .connectors()
        .require_remote_target_connector(&connector_id)?;
    let connector_config: ConnectorConfigEnvelope = target
        .connector_config
        .as_deref()
        .ok_or_else(|| {
            AsterError::database_operation(format!(
                "remote storage target #{} has no connector config",
                target.id
            ))
        })
        .and_then(|raw| {
            serde_json::from_str(raw).map_err(|error| {
                AsterError::from(aster_drive_storage::storage_driver_error(
                    aster_drive_storage::StorageErrorKind::Misconfigured,
                    format!(
                        "remote storage target #{} connector config is invalid: {error}",
                        target.id
                    ),
                ))
            })
        })?;
    if connector_config.connector_id != connector_id {
        return Err(AsterError::validation_error(format!(
            "remote storage target #{} connector config id '{}' does not match target connector '{}'",
            target.id, connector_config.connector_id, connector_id
        )));
    }
    let credential = if connector.descriptor().credential_mode
        == aster_drive_storage::StorageConnectorCredentialMode::None
    {
        crate::storage::connectors::StorageConnectorCredentialInput::None
    } else if let Some(saved) =
        crate::db::repository::remote_storage_target_credential_repo::find_by_target(
            state.writer_db(),
            target.id,
        )
        .await?
    {
        if saved.connector_id != connector_id.as_str() {
            return Err(AsterError::database_operation(format!(
                "remote storage target #{} credential connector '{}' does not match target connector '{}'",
                target.id, saved.connector_id, connector_id
            )));
        }
        let saved_schema_version = aster_forge_utils::numbers::i32_to_usize(
            saved.schema_version,
            "remote storage target credential schema version",
        )
        .and_then(|value| {
            u32::try_from(value).map_err(|_| {
                aster_forge_utils::UtilsError::numeric_conversion(format!(
                    "remote storage target credential schema version exceeds u32 range: {value}"
                ))
            })
        })
        .map_err(|error| AsterError::database_operation(error.to_string()))?;
        let expected_schema_version = connector
            .descriptor()
            .credential_schema_version
            .ok_or_else(|| {
                AsterError::database_operation(format!(
                    "remote storage target #{} connector credential schema is missing",
                    target.id
                ))
            })?;
        if saved_schema_version != expected_schema_version {
            return Err(AsterError::database_operation(format!(
                "remote storage target #{} credential schema {} does not match connector schema {}",
                target.id, saved.schema_version, expected_schema_version
            )));
        }
        let plaintext =
            crate::services::storage_policy::credential::crypto::decrypt_connector_credential(
                &state.config().auth.storage_credential_secret_key,
                target.id,
                &saved.connector_id,
                saved_schema_version,
                &saved.ciphertext,
            )?;
        let values: serde_json::Value = serde_json::from_str(&plaintext).map_err(|error| {
            AsterError::database_operation(format!(
                "invalid remote target credential payload: {error}"
            ))
        })?;
        crate::storage::connectors::StorageConnectorCredentialInput::Static(values)
    } else {
        return Err(AsterError::database_operation(format!(
            "remote storage target #{} is missing encrypted credentials",
            target.id
        )));
    };
    Ok(crate::storage::StorageConnectionInput {
        connector_config,
        credential,
    })
}

use chrono::Utc;

use crate::api::api_error_code::ApiErrorCode;
use crate::errors::{AsterError, MapAsterErr, Result, validation_error_with_code};
use aster_drive_model::entities::storage_policy;
use aster_drive_model::types::StoredStoragePolicyAllowedTypes;
use aster_drive_storage::connector_descriptor::{
    StorageConnectorActionDescriptor, StorageConnectorActionEndpoint, StorageConnectorActionId,
    StorageConnectorActionKind, StorageConnectorDescriptor, StorageConnectorFieldDescriptor,
    StorageConnectorFieldScope, StorageConnectorSelectOptionInput, storage_connector_select_field,
};
use aster_drive_storage::{
    ConnectorConfigEnvelope, ConnectorId, StorageConnectorActionSchema,
    StorageConnectorFieldDefaultValue, StorageDriver, StorageErrorKind,
    StoragePolicyBehaviorConfig,
};
use serde::{Serialize, de::DeserializeOwned};

use super::StorageConnectorCredentialInput;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StorageTransferDirection {
    Upload,
    Download,
}

/// Build the common relay/presigned transfer strategy field.
///
/// Direction is explicit because a boolean argument makes call sites such as
/// `transfer_options(true)` impossible to review without opening the helper.
pub(super) fn transfer_strategy_field(
    name: &str,
    direction: StorageTransferDirection,
) -> StorageConnectorFieldDescriptor {
    let mut field = storage_connector_select_field(
        name,
        StorageConnectorFieldScope::ConnectorConfig,
        true,
        transfer_strategy_options(direction),
    );
    field.default_value = Some(StorageConnectorFieldDefaultValue::String(
        "relay_stream".to_string(),
    ));
    field
}

fn transfer_strategy_options(
    direction: StorageTransferDirection,
) -> Vec<StorageConnectorSelectOptionInput<'static>> {
    let (relay_label, relay_description, presigned_label, presigned_description) = match direction {
        StorageTransferDirection::Upload => (
            "upload_strategy_relay_stream",
            "upload_strategy_relay_stream_desc",
            "upload_strategy_presigned",
            "upload_strategy_presigned_desc",
        ),
        StorageTransferDirection::Download => (
            "download_strategy_relay_stream",
            "download_strategy_relay_stream_desc",
            "download_strategy_presigned",
            "download_strategy_presigned_desc",
        ),
    };
    vec![
        StorageConnectorSelectOptionInput {
            value: "relay_stream",
            label_key: relay_label,
            description_key: Some(relay_description),
        },
        StorageConnectorSelectOptionInput {
            value: "presigned",
            label_key: presigned_label,
            description_key: Some(presigned_description),
        },
    ]
}

pub(super) fn runtime_static_credential<T: DeserializeOwned>(
    registry: &crate::storage::DriverRegistry,
    policy: &storage_policy::Model,
    connector_id: &'static str,
) -> Result<T> {
    let credential = registry.get_runtime_credential(policy.id).ok_or_else(|| {
        crate::errors::storage_driver_error(
            StorageErrorKind::Auth,
            format!("storage policy {} is missing static credentials", policy.id),
        )
    })?;
    let values = credential.require::<serde_json::Value>(connector_id)?;
    serde_json::from_value(values.clone()).map_err(|error| {
        crate::errors::storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!(
                "storage policy {} has invalid static credentials: {error}",
                policy.id
            ),
        )
    })
}

pub(super) fn build_connection_test_policy(
    connector_config: ConnectorConfigEnvelope,
    behavior: StoragePolicyBehaviorConfig,
) -> Result<storage_policy::Model> {
    let connector_id = connector_config.connector_id.clone();
    let connector_config = ConnectorConfigEnvelope::new(
        connector_id.clone(),
        connector_config.schema_version,
        serde_json::to_value(connector_config.values).map_err(|error| {
            AsterError::internal_error(format!("serialize draft connector config: {error}"))
        })?,
    );
    let storage_config =
        aster_drive_storage::encode_storage_policy_config(connector_config, behavior)
            .map(aster_drive_model::types::StoredStoragePolicyConfig)
            .map_err(|error| {
                AsterError::internal_error(format!("serialize storage policy config: {error}"))
            })?;
    Ok(storage_policy::Model {
        id: 0,
        name: String::new(),
        connector_id: connector_id.as_str().to_string(),
        storage_config,
        max_file_size: 0,
        allowed_types: StoredStoragePolicyAllowedTypes::empty(),
        is_default: false,
        chunk_size: 0,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    })
}

pub(super) fn encode_normalized_connector_config<T: Serialize>(
    connector_id: ConnectorId,
    schema_version: u32,
    values: T,
) -> Result<ConnectorConfigEnvelope> {
    let values = serde_json::to_value(values)
        .and_then(serde_json::from_value)
        .map_err(|error| {
            AsterError::internal_error(format!(
                "serialize normalized connector config '{connector_id}': {error}"
            ))
        })?;
    Ok(ConnectorConfigEnvelope::new(
        connector_id,
        schema_version,
        values,
    ))
}

/// Decode one persisted policy through the connector's concrete typed schema.
///
/// Connector implementations call this at their boundary and pass plain
/// runtime values to drivers. Drivers never inspect SeaORM entities or generic
/// JSON envelopes themselves.
pub(super) fn decode_typed_policy_config<T: DeserializeOwned>(
    policy: &storage_policy::Model,
    connector_id: &'static str,
    schema_version: u32,
) -> Result<(T, StoragePolicyBehaviorConfig)> {
    decode_typed_policy_config_for_id(policy, &ConnectorId::declared(connector_id), schema_version)
}

pub(super) fn decode_typed_policy_config_for_id<T: DeserializeOwned>(
    policy: &storage_policy::Model,
    connector_id: &ConnectorId,
    schema_version: u32,
) -> Result<(T, StoragePolicyBehaviorConfig)> {
    aster_drive_storage::decode_storage_policy_config(
        policy.storage_config.as_ref(),
        connector_id,
        schema_version,
    )
    .map_err(|error| {
        crate::errors::storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!(
                "storage policy {} has invalid '{}' configuration: {error}",
                policy.id, connector_id
            ),
        )
    })
}

pub(super) fn decode_normalized_connector_config<T: DeserializeOwned>(
    config: &ConnectorConfigEnvelope,
) -> Result<T> {
    serde_json::from_value(serde_json::to_value(&config.values).map_err(|error| {
        AsterError::internal_error(format!("serialize normalized connector config: {error}"))
    })?)
    .map_err(|error| {
        AsterError::validation_error(format!(
            "invalid '{}' connector configuration: {error}",
            config.connector_id
        ))
    })
}

pub(super) fn decode_static_credential<T: DeserializeOwned>(
    credential: &StorageConnectorCredentialInput,
    connector_id: &str,
) -> Result<T> {
    let StorageConnectorCredentialInput::Static(values) = credential else {
        return Err(AsterError::validation_error(format!(
            "storage connector '{connector_id}' requires static credentials"
        )));
    };
    serde_json::from_value(values.clone()).map_err(|error| {
        AsterError::validation_error(format!(
            "invalid static credentials for storage connector '{connector_id}': {error}"
        ))
    })
}

pub(super) fn decode_authorization_application<T: DeserializeOwned>(
    credential: &StorageConnectorCredentialInput,
    connector_id: &str,
) -> Result<T> {
    let StorageConnectorCredentialInput::AuthorizationApplication(values) = credential else {
        return Err(AsterError::validation_error(format!(
            "storage connector '{connector_id}' requires authorization application credentials"
        )));
    };
    serde_json::from_value(values.clone()).map_err(|error| {
        AsterError::validation_error(format!(
            "invalid authorization application credentials for storage connector '{connector_id}': {error}"
        ))
    })
}

pub(super) fn validate_required_credential_field(
    value: &str,
    field: &str,
    connector_id: &str,
) -> Result<()> {
    if value.trim().is_empty() {
        return Err(AsterError::validation_error(format!(
            "credential field '{field}' is required for storage connector '{connector_id}'"
        )));
    }
    Ok(())
}

/// Convert the deprecated `access_key`/`secret_key` policy columns into the
/// current connector-owned static credential struct.
///
/// This helper is exclusive to the AsterDrive 0.5.0 startup migration and will
/// be completely removed together with the legacy columns in AsterDrive 0.6.0.
pub(super) fn import_legacy_static_credential<T: Serialize>(
    connector_id: &str,
    input: super::LegacyStorageConnectorCredentialInput,
    build: impl FnOnce(super::LegacyStoragePolicyStaticCredential) -> T,
) -> Result<Option<serde_json::Value>> {
    if input.application_config.is_some() || input.authorization.is_some() {
        return Err(AsterError::database_operation(format!(
            "connector '{connector_id}' received incompatible legacy authorization credentials",
        )));
    }
    let Some(mut credential) = input.static_credential else {
        return Ok(None);
    };
    credential.access_key = credential.access_key.trim().to_string();
    credential.secret_key = credential.secret_key.trim().to_string();
    if credential.access_key.is_empty() && credential.secret_key.is_empty() {
        return Ok(None);
    }
    if credential.access_key.is_empty() || credential.secret_key.is_empty() {
        return Err(AsterError::database_operation(format!(
            "connector '{connector_id}' has incomplete legacy static credentials",
        )));
    }
    serde_json::to_value(build(credential))
        .map(Some)
        .map_err(|error| {
            AsterError::database_operation(format!(
                "serialize migrated credential for connector '{connector_id}': {error}",
            ))
        })
}

pub(super) fn decode_normalized_connector_action_input<T>(
    descriptor: &StorageConnectorActionDescriptor,
    values: &std::collections::BTreeMap<String, serde_json::Value>,
) -> Result<T>
where
    T: DeserializeOwned + StorageConnectorActionSchema,
{
    let declared_fields = T::action_fields();
    if descriptor.fields != declared_fields {
        return Err(AsterError::internal_error(format!(
            "storage connector action '{}' descriptor fields do not match its typed input schema",
            descriptor.action_id.as_str()
        )));
    }
    serde_json::to_value(values)
        .and_then(serde_json::from_value)
        .map_err(|error| {
            validation_error_with_code(
                ApiErrorCode::PolicyActionParameterInvalid,
                format!(
                    "storage connector action '{}' input is invalid: {error}",
                    descriptor.action_id.as_str()
                ),
            )
        })
}

pub(super) fn unsupported_connector_action_error(
    descriptor: &StorageConnectorDescriptor,
    action_id: &StorageConnectorActionId,
) -> AsterError {
    validation_error_with_code(
        ApiErrorCode::PolicyActionUnsupported,
        format!(
            "storage connector action '{}' is not supported for connector '{}'",
            action_id.as_str(),
            descriptor.connector_id.as_str()
        ),
    )
}

pub(super) fn unsupported_draft_connection_test_error(
    descriptor: StorageConnectorDescriptor,
) -> AsterError {
    if descriptor.actions.iter().any(|action| {
        action.kind == StorageConnectorActionKind::ConnectionTest
            && action
                .endpoints
                .contains(&StorageConnectorActionEndpoint::TestPolicyConnection)
            && action.requires_saved_policy
            && action.requires_authorization
    }) {
        return validation_error_with_code(
            ApiErrorCode::PolicyActionUnsupported,
            format!(
                "storage policy driver '{}' requires a saved storage policy with completed authorization; use the saved policy connection test after authorization",
                descriptor.connector_id.as_str(),
            ),
        );
    }
    validation_error_with_code(
        ApiErrorCode::PolicyActionUnsupported,
        format!(
            "storage policy driver '{}' does not support draft connection tests",
            descriptor.connector_id.as_str(),
        ),
    )
}

pub(super) fn unsupported_saved_connection_test_error(
    descriptor: StorageConnectorDescriptor,
) -> AsterError {
    validation_error_with_code(
        ApiErrorCode::PolicyActionUnsupported,
        format!(
            "storage policy driver '{}' does not support saved-policy connection tests",
            descriptor.connector_id.as_str(),
        ),
    )
}

pub(super) async fn probe_storage_driver(
    driver: &dyn StorageDriver,
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

pub fn unsupported_multipart_error(policy: &storage_policy::Model) -> AsterError {
    crate::errors::storage_driver_error(
        StorageErrorKind::Unsupported,
        format!(
            "storage policy {} (driver: {:?}) does not support multipart upload",
            policy.id, policy.connector_id
        ),
    )
}

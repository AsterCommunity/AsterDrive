use std::sync::Arc;

use crate::errors::{AsterError, Result};
use crate::runtime::FollowerRuntimeState;
use aster_drive_model::entities::remote_storage_target;
use aster_drive_model::types::RemoteStorageTargetDriverKind;
use aster_drive_storage::StorageDriver;
use aster_drive_storage::field_contract::{
    normalize_object_storage_prefix, normalize_required_storage_field,
};
use aster_drive_storage::{ConnectorConfigEnvelope, ConnectorId};

use super::paths::normalize_relative_local_path;

pub(in crate::services::remote::storage_target) struct RemoteStorageTargetDriverFields {
    pub driver_type: RemoteStorageTargetDriverKind,
    pub endpoint: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub base_path: String,
}

pub(in crate::services::remote::storage_target) struct NormalizedRemoteStorageTargetDriverFields {
    pub driver_type: RemoteStorageTargetDriverKind,
    pub endpoint: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub base_path: String,
}

pub type RemoteStorageTargetDriverDescriptor = aster_drive_storage::StorageConnectorDescriptor;

fn normalize_generic_remote_fields(
    connector: BuiltinRemoteStorageTargetDriverConnector,
    fields: RemoteStorageTargetDriverFields,
) -> Result<NormalizedRemoteStorageTargetDriverFields> {
    let base_path = normalize_object_storage_prefix(&fields.base_path);
    let endpoint = fields.endpoint.trim().trim_end_matches('/').to_string();
    let bucket = if matches!(connector, BuiltinRemoteStorageTargetDriverConnector::Sftp) {
        String::new()
    } else {
        normalize_required_storage_field("bucket", &fields.bucket)?
    };
    let access_key = normalize_required_storage_field("access_key", &fields.access_key)?;
    let secret_key = normalize_required_storage_field("secret_key", &fields.secret_key)?;
    if endpoint.is_empty() {
        return Err(AsterError::validation_error("endpoint cannot be empty"));
    }
    Ok(NormalizedRemoteStorageTargetDriverFields {
        driver_type: match connector {
            BuiltinRemoteStorageTargetDriverConnector::Sftp => RemoteStorageTargetDriverKind::Sftp,
            BuiltinRemoteStorageTargetDriverConnector::TencentCos => {
                RemoteStorageTargetDriverKind::TencentCos
            }
            BuiltinRemoteStorageTargetDriverConnector::AlibabaOss => {
                RemoteStorageTargetDriverKind::AlibabaOss
            }
            BuiltinRemoteStorageTargetDriverConnector::Qiniu => {
                RemoteStorageTargetDriverKind::Qiniu
            }
            BuiltinRemoteStorageTargetDriverConnector::AzureBlob => {
                RemoteStorageTargetDriverKind::AzureBlob
            }
            BuiltinRemoteStorageTargetDriverConnector::HuaweiObs => {
                RemoteStorageTargetDriverKind::HuaweiObs
            }
        },
        endpoint,
        bucket,
        access_key,
        secret_key,
        base_path,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuiltinRemoteStorageTargetDriverConnector {
    Sftp,
    TencentCos,
    AlibabaOss,
    Qiniu,
    AzureBlob,
    HuaweiObs,
}

impl BuiltinRemoteStorageTargetDriverConnector {}

fn normalize_local_fields(
    fields: RemoteStorageTargetDriverFields,
) -> Result<NormalizedRemoteStorageTargetDriverFields> {
    Ok(NormalizedRemoteStorageTargetDriverFields {
        driver_type: RemoteStorageTargetDriverKind::Local,
        endpoint: String::new(),
        bucket: String::new(),
        access_key: String::new(),
        secret_key: String::new(),
        base_path: normalize_relative_local_path(&fields.base_path)?,
    })
}

fn normalize_s3_fields(
    fields: RemoteStorageTargetDriverFields,
) -> Result<NormalizedRemoteStorageTargetDriverFields> {
    let normalized = crate::storage::drivers::s3_config::normalize_s3_endpoint_and_bucket(
        &fields.endpoint,
        &fields.bucket,
    )
    .map_err(|error| error.into_aster_error())?;
    Ok(NormalizedRemoteStorageTargetDriverFields {
        driver_type: RemoteStorageTargetDriverKind::S3,
        endpoint: normalized.endpoint,
        bucket: normalized.bucket,
        access_key: aster_drive_storage::field_contract::normalize_required_storage_field(
            "access_key",
            &fields.access_key,
        )?,
        secret_key: aster_drive_storage::field_contract::normalize_required_storage_field(
            "secret_key",
            &fields.secret_key,
        )?,
        base_path: aster_drive_storage::field_contract::normalize_object_storage_prefix(
            &fields.base_path,
        ),
    })
}

pub(crate) fn remote_storage_target_connector_id(
    driver_type: RemoteStorageTargetDriverKind,
) -> Result<ConnectorId> {
    Ok(ConnectorId::declared(match driver_type {
        RemoteStorageTargetDriverKind::Local => "asterdrive.storage.local",
        RemoteStorageTargetDriverKind::S3 => "asterdrive.storage.s3",
        RemoteStorageTargetDriverKind::Sftp => "asterdrive.storage.sftp",
        RemoteStorageTargetDriverKind::TencentCos => "asterdrive.storage.tencent_cos",
        RemoteStorageTargetDriverKind::AlibabaOss => "asterdrive.storage.alibaba_oss",
        RemoteStorageTargetDriverKind::Qiniu => "asterdrive.storage.qiniu",
        RemoteStorageTargetDriverKind::AzureBlob => "asterdrive.storage.azure_blob",
        RemoteStorageTargetDriverKind::HuaweiObs => "asterdrive.storage.huawei_obs",
    }))
}

#[cfg(test)]
pub(crate) fn remote_storage_target_driver_type_for_connector_id(
    connector_id: &ConnectorId,
) -> Option<RemoteStorageTargetDriverKind> {
    match connector_id.as_str() {
        "asterdrive.storage.local" => Some(RemoteStorageTargetDriverKind::Local),
        "asterdrive.storage.s3" => Some(RemoteStorageTargetDriverKind::S3),
        "asterdrive.storage.sftp" => Some(RemoteStorageTargetDriverKind::Sftp),
        "asterdrive.storage.tencent_cos" => Some(RemoteStorageTargetDriverKind::TencentCos),
        "asterdrive.storage.alibaba_oss" => Some(RemoteStorageTargetDriverKind::AlibabaOss),
        "asterdrive.storage.qiniu" => Some(RemoteStorageTargetDriverKind::Qiniu),
        "asterdrive.storage.azure_blob" => Some(RemoteStorageTargetDriverKind::AzureBlob),
        "asterdrive.storage.huawei_obs" => Some(RemoteStorageTargetDriverKind::HuaweiObs),
        _ => None,
    }
}

pub(crate) fn remote_storage_target_descriptor_from_connector(
    connector: &dyn crate::storage::connectors::StorageConnector,
) -> Result<RemoteStorageTargetDriverDescriptor> {
    if !connector.supports_remote_storage_target() {
        return Err(AsterError::validation_error(format!(
            "storage connector '{}' is not a remote target provider",
            connector.descriptor().connector_id
        )));
    }
    Ok(connector.descriptor())
}

#[cfg(test)]
pub(crate) fn registered_remote_storage_target_driver_types() -> Vec<RemoteStorageTargetDriverKind>
{
    crate::storage::connectors::builtin_storage_connector_registry()
        .map(|registry| registry.remote_target_driver_types())
        .unwrap_or_default()
}

#[cfg(test)]
pub(crate) fn list_registered_remote_storage_target_driver_descriptors()
-> Result<Vec<RemoteStorageTargetDriverDescriptor>> {
    crate::storage::connectors::builtin_storage_connector_registry()?
        .remote_target_connectors()
        .into_iter()
        .map(remote_storage_target_descriptor_from_connector)
        .collect()
}

pub fn remote_storage_target_driver_descriptor(
    driver_type: RemoteStorageTargetDriverKind,
) -> Result<RemoteStorageTargetDriverDescriptor> {
    let connector_id = remote_storage_target_connector_id(driver_type)?;
    let registry = crate::storage::connectors::builtin_storage_connector_registry()?;
    remote_storage_target_descriptor_from_connector(
        registry.require_remote_target_connector(&connector_id)?,
    )
}

pub(in crate::services::remote::storage_target) fn normalize_driver_fields(
    fields: RemoteStorageTargetDriverFields,
) -> Result<NormalizedRemoteStorageTargetDriverFields> {
    match fields.driver_type {
        RemoteStorageTargetDriverKind::Local => normalize_local_fields(fields),
        RemoteStorageTargetDriverKind::S3 => normalize_s3_fields(fields),
        RemoteStorageTargetDriverKind::Sftp => {
            normalize_generic_remote_fields(BuiltinRemoteStorageTargetDriverConnector::Sftp, fields)
        }
        RemoteStorageTargetDriverKind::TencentCos => normalize_generic_remote_fields(
            BuiltinRemoteStorageTargetDriverConnector::TencentCos,
            fields,
        ),
        RemoteStorageTargetDriverKind::AlibabaOss => normalize_generic_remote_fields(
            BuiltinRemoteStorageTargetDriverConnector::AlibabaOss,
            fields,
        ),
        RemoteStorageTargetDriverKind::Qiniu => normalize_generic_remote_fields(
            BuiltinRemoteStorageTargetDriverConnector::Qiniu,
            fields,
        ),
        RemoteStorageTargetDriverKind::AzureBlob => normalize_generic_remote_fields(
            BuiltinRemoteStorageTargetDriverConnector::AzureBlob,
            fields,
        ),
        RemoteStorageTargetDriverKind::HuaweiObs => normalize_generic_remote_fields(
            BuiltinRemoteStorageTargetDriverConnector::HuaweiObs,
            fields,
        ),
    }
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
    let connector_id = target
        .connector_id
        .as_deref()
        .map(ConnectorId::declared)
        .ok_or_else(|| {
            AsterError::from(aster_drive_storage::storage_driver_error(
                aster_drive_storage::StorageErrorKind::Misconfigured,
                format!("remote storage target #{} has no connector id", target.id),
            ))
        })?;
    let connector = state
        .driver_registry()
        .connectors()
        .require_remote_target_connector(&connector_id)?;
    let config: ConnectorConfigEnvelope<serde_json::Value> = target
        .connector_config
        .as_deref()
        .ok_or_else(|| {
            AsterError::from(aster_drive_storage::storage_driver_error(
                aster_drive_storage::StorageErrorKind::Misconfigured,
                format!(
                    "remote storage target #{} has no connector config",
                    target.id
                ),
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
    if config.connector_id != connector_id {
        return Err(AsterError::validation_error(format!(
            "remote storage target #{} connector config id '{}' does not match target connector '{}'",
            target.id, config.connector_id, connector_id
        )));
    }
    let mut config = config;
    if connector_id.as_str() == "asterdrive.storage.local" {
        if let Some(values) = config.values.as_object_mut() {
            let relative = values
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
            values.insert(
                "base_path".to_string(),
                serde_json::Value::String(resolved.to_string_lossy().into_owned()),
            );
        }
    }
    let context = crate::storage::connectors::StorageConnectorContext::new(
        state.writer_db(),
        state.config(),
        state.runtime_config(),
        state.driver_registry(),
        None,
    );
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
        if saved.schema_version as u32 != config.schema_version {
            return Err(AsterError::database_operation(format!(
                "remote storage target #{} credential schema {} does not match config schema {}",
                target.id, saved.schema_version, config.schema_version
            )));
        }
        let plaintext =
            crate::services::storage_policy::credential::crypto::decrypt_connector_credential(
                &state.config().auth.storage_credential_secret_key,
                target.id,
                &saved.connector_id,
                saved.schema_version as u32,
                &saved.ciphertext,
            )?;
        let values: serde_json::Value = serde_json::from_str(&plaintext).map_err(|error| {
            AsterError::database_operation(format!(
                "invalid remote target credential payload: {error}"
            ))
        })?;
        crate::storage::connectors::StorageConnectorCredentialInput::Static(
            remap_legacy_credential_values(values, connector)?,
        )
    } else {
        return Err(AsterError::database_operation(format!(
            "remote storage target #{} is missing encrypted credentials",
            target.id
        )));
    };
    connector
        .build_driver_from_connection(&context, &config, &credential)
        .await
        .map(|driver| driver.storage)
}

fn remap_legacy_credential_values(
    values: serde_json::Value,
    connector: &dyn crate::storage::connectors::StorageConnector,
) -> Result<serde_json::Value> {
    let object = values.as_object().ok_or_else(|| {
        AsterError::database_operation("remote target credential payload must be an object")
    })?;
    let fields = connector
        .descriptor()
        .fields
        .into_iter()
        .filter(|field| {
            field.scope == aster_drive_storage::StorageConnectorFieldScope::StaticCredential
        })
        .collect::<Vec<_>>();
    if fields.is_empty() {
        return Ok(serde_json::json!({}));
    }
    let access = object
        .iter()
        .find(|(key, value)| key.contains("access_key") && value.is_string())
        .map(|(_, value)| value.clone())
        .unwrap_or_default();
    let secret = object
        .iter()
        .find(|(key, value)| key.contains("secret_key") && value.is_string())
        .map(|(_, value)| value.clone())
        .unwrap_or_default();
    let mut mapped = serde_json::Map::new();
    mapped.insert(fields[0].name.clone(), access);
    if fields.len() > 1 {
        mapped.insert(fields[1].name.clone(), secret);
    }
    Ok(serde_json::Value::Object(mapped))
}

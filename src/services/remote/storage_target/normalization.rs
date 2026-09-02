use crate::errors::Result;
use crate::storage::remote_protocol::{
    RemoteCreateStorageTargetRequest, RemoteUpdateStorageTargetRequest,
};
use aster_drive_model::entities::remote_storage_target;
use aster_drive_model::types::RemoteStorageTargetDriverKind;
use aster_drive_storage::field_contract::normalize_required_storage_field;

use super::driver::{RemoteStorageTargetDriverFields, normalize_driver_fields};

pub(in crate::services::remote::storage_target) struct NormalizedStorageTargetInput {
    pub name: String,
    pub driver_type: RemoteStorageTargetDriverKind,
    pub endpoint: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub base_path: String,
    pub is_default: Option<bool>,
    pub connector_config: Option<aster_drive_storage::ConnectorConfigEnvelope<serde_json::Value>>,
    pub credential: Option<serde_json::Value>,
}

struct StorageTargetFields {
    name: String,
    driver_type: RemoteStorageTargetDriverKind,
    endpoint: String,
    bucket: String,
    access_key: String,
    secret_key: String,
    base_path: String,
    is_default: Option<bool>,
}

pub(in crate::services::remote::storage_target) fn normalize_create_input(
    input: RemoteCreateStorageTargetRequest,
) -> Result<NormalizedStorageTargetInput> {
    match input {
        RemoteCreateStorageTargetRequest {
            name,
            connector_config,
            credential,
            is_default,
        } => normalize_connector_input(RemoteCreateStorageTargetRequest {
            name,
            connector_config,
            credential,
            is_default,
        }),
    }
}

fn normalize_connector_input(
    input: RemoteCreateStorageTargetRequest,
) -> Result<NormalizedStorageTargetInput> {
    let driver_type = input
        .connector_config
        .connector_id
        .as_str()
        .rsplit_once('.')
        .and_then(|(_, value)| value.parse().ok())
        .ok_or_else(|| {
            crate::errors::AsterError::validation_error("remote target connector id is unsupported")
        })?;
    let values = input.connector_config.values.as_object().ok_or_else(|| {
        crate::errors::AsterError::validation_error("connector config values must be an object")
    })?;
    let credential = input
        .credential
        .clone()
        .unwrap_or_else(|| serde_json::json!({}));
    let credentials = credential.as_object().ok_or_else(|| {
        crate::errors::AsterError::validation_error(
            "remote target credential values must be an object",
        )
    })?;
    let access_key = credentials
        .iter()
        .find(|(key, value)| key.contains("access_key") && value.is_string())
        .and_then(|(_, value)| value.as_str())
        .unwrap_or_default()
        .to_string();
    let secret_key = credentials
        .iter()
        .find(|(key, value)| key.contains("secret_key") && value.is_string())
        .and_then(|(_, value)| value.as_str())
        .unwrap_or_default()
        .to_string();
    let mut normalized = normalize_target_fields(StorageTargetFields {
        name: normalize_required_storage_field("name", &input.name)?,
        driver_type,
        endpoint: values
            .get("endpoint")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        bucket: values
            .get("bucket")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        access_key,
        secret_key,
        base_path: values
            .get("base_path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        is_default: Some(input.is_default),
    })?;
    normalized.connector_config = Some(input.connector_config);
    normalized.credential = input.credential;
    Ok(normalized)
}

pub(in crate::services::remote::storage_target) fn normalize_update_input(
    existing: remote_storage_target::Model,
    input: RemoteUpdateStorageTargetRequest,
) -> Result<NormalizedStorageTargetInput> {
    let legacy_credential = input.credential.clone().or_else(|| {
        let access_key = input
            .access_key
            .clone()
            .unwrap_or_else(|| existing.access_key.clone());
        let secret_key = input
            .secret_key
            .clone()
            .unwrap_or_else(|| existing.secret_key.clone());
        (!access_key.trim().is_empty() || !secret_key.trim().is_empty())
            .then_some(serde_json::json!({"access_key": access_key, "secret_key": secret_key}))
    });
    let legacy_driver = input.driver_type;
    let has_legacy_config = input.driver_type.is_some()
        || input.endpoint.is_some()
        || input.bucket.is_some()
        || input.base_path.is_some();
    let connector_config = input
        .connector_config
        .filter(|_| !has_legacy_config)
        .or_else(|| {
            let id = legacy_driver
                .or(existing.driver_type.into())
                .and_then(|driver| super::driver::remote_storage_target_connector_id(driver).ok());
            if !has_legacy_config {
                existing
                    .connector_config
                    .as_deref()
                    .and_then(|raw| serde_json::from_str(raw).ok())
            } else {
                None
            }
            .or_else(|| {
                id.map(|connector_id| {
                    aster_drive_storage::ConnectorConfigEnvelope::new(
                        connector_id.clone(),
                        1,
                        legacy_connector_values(
                            &connector_id,
                            input.endpoint.clone().unwrap_or(existing.endpoint.clone()),
                            input.bucket.clone().unwrap_or(existing.bucket.clone()),
                            input
                                .base_path
                                .clone()
                                .unwrap_or(existing.base_path.clone()),
                        ),
                    )
                })
            })
        })
        .unwrap_or_else(|| {
            aster_drive_storage::ConnectorConfigEnvelope::new(
                aster_drive_storage::ConnectorId::declared("asterdrive.storage.s3"),
                1,
                serde_json::json!({
                    "endpoint": existing.endpoint,
                    "bucket": existing.bucket,
                    "base_path": existing.base_path,
                }),
            )
        });
    let mut normalized = normalize_connector_input(RemoteCreateStorageTargetRequest {
        name: input.name.unwrap_or(existing.name),
        connector_config,
        credential: legacy_credential,
        is_default: input.is_default.unwrap_or(existing.is_default),
    })?;
    normalized.is_default = input.is_default;
    Ok(normalized)
}

fn legacy_connector_values(
    connector_id: &aster_drive_storage::ConnectorId,
    endpoint: String,
    bucket: String,
    base_path: String,
) -> serde_json::Value {
    if connector_id.as_str() == "asterdrive.storage.local" {
        serde_json::json!({ "base_path": base_path })
    } else {
        serde_json::json!({ "endpoint": endpoint, "bucket": bucket, "base_path": base_path })
    }
}

pub(in crate::services::remote::storage_target) fn new_target_key() -> String {
    format!("rst_{}", aster_forge_utils::id::new_short_token())
}

fn normalize_target_fields(fields: StorageTargetFields) -> Result<NormalizedStorageTargetInput> {
    let StorageTargetFields {
        name,
        driver_type,
        endpoint,
        bucket,
        access_key,
        secret_key,
        base_path,
        is_default,
    } = fields;

    let normalized = normalize_driver_fields(RemoteStorageTargetDriverFields {
        driver_type,
        endpoint,
        bucket,
        access_key,
        secret_key,
        base_path,
    })?;

    Ok(NormalizedStorageTargetInput {
        name,
        driver_type: normalized.driver_type,
        endpoint: normalized.endpoint,
        bucket: normalized.bucket,
        access_key: normalized.access_key,
        secret_key: normalized.secret_key,
        base_path: normalized.base_path,
        is_default,
        connector_config: None,
        credential: None,
    })
}

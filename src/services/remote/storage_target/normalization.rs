use crate::errors::Result;
use crate::storage::remote_protocol::{
    RemoteCreateStorageTargetRequest, RemoteUpdateStorageTargetRequest,
};
use aster_drive_model::entities::remote_storage_target;
use aster_drive_storage::field_contract::normalize_required_storage_field;

use super::driver::{NormalizedConnectorInput, normalize_connector_input};

pub(in crate::services::remote::storage_target) struct NormalizedStorageTargetInput {
    pub name: String,
    pub connector: NormalizedConnectorInput,
    pub is_default: Option<bool>,
}

pub(in crate::services::remote::storage_target) fn normalize_create_input(
    input: RemoteCreateStorageTargetRequest,
) -> Result<NormalizedStorageTargetInput> {
    Ok(NormalizedStorageTargetInput {
        name: normalize_required_storage_field("name", &input.name)?,
        connector: normalize_connector_input(input.connector_config, input.credential, None)?,
        is_default: Some(input.is_default),
    })
}

pub(in crate::services::remote::storage_target) fn normalize_update_input(
    existing: &remote_storage_target::Model,
    input: RemoteUpdateStorageTargetRequest,
    saved_credential: Option<String>,
) -> Result<NormalizedStorageTargetInput> {
    let config = match input.connector_config {
        Some(config) => config,
        None => serde_json::from_str(&existing.connector_config).map_err(|error| {
            crate::errors::AsterError::database_operation(format!(
                "invalid saved remote target connector config: {error}"
            ))
        })?,
    };
    let same_connector = config.connector_id.as_str() == existing.connector_id;
    Ok(NormalizedStorageTargetInput {
        name: match input.name {
            Some(name) => normalize_required_storage_field("name", &name)?,
            None => existing.name.clone(),
        },
        connector: normalize_connector_input(
            config,
            input.credential,
            same_connector.then_some(saved_credential).flatten(),
        )?,
        is_default: input.is_default,
    })
}

pub(in crate::services::remote::storage_target) fn new_target_key() -> String {
    format!("rst_{}", uuid::Uuid::new_v4().simple())
}

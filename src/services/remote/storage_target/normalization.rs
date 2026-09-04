use crate::errors::Result;
use crate::runtime::FollowerRuntimeState;
use crate::storage::StorageConnectionInput;
use crate::storage::remote_protocol::{
    RemoteCreateStorageTargetRequest, RemoteUpdateStorageTargetRequest,
};
use aster_drive_model::entities::remote_storage_target;
use aster_drive_storage::field_contract::normalize_required_storage_field;

#[derive(Debug)]
pub(in crate::services::remote::storage_target) struct NormalizedStorageTargetInput {
    pub name: String,
    pub connection: Option<StorageConnectionInput>,
    pub is_default: Option<bool>,
}

pub(in crate::services::remote::storage_target) async fn normalize_create_input<
    S: FollowerRuntimeState,
>(
    state: &S,
    input: RemoteCreateStorageTargetRequest,
) -> Result<NormalizedStorageTargetInput> {
    Ok(NormalizedStorageTargetInput {
        name: normalize_required_storage_field("name", &input.name)?,
        connection: Some(normalize_connection(state, input.connection).await?),
        is_default: Some(input.is_default),
    })
}

pub(in crate::services::remote::storage_target) async fn normalize_update_input<
    S: FollowerRuntimeState,
>(
    state: &S,
    existing: &remote_storage_target::Model,
    input: RemoteUpdateStorageTargetRequest,
) -> Result<NormalizedStorageTargetInput> {
    let connection = match input.connection {
        Some(mut connection) => {
            if existing.connector_id.as_deref()
                == Some(connection.connector_config.connector_id.as_str())
                && let Ok(saved_connection) =
                    super::driver::load_connection_from_target(state, existing).await
                && let crate::storage::StorageConnectorCredentialInput::Static(saved) =
                    saved_connection.credential
            {
                connection.credential = crate::storage::connectors::merge_saved_static_credential(
                    connection.credential,
                    saved,
                )?;
            }
            Some(normalize_connection(state, connection).await?)
        }
        None => None,
    };
    Ok(NormalizedStorageTargetInput {
        name: input
            .name
            .as_deref()
            .map(|name| normalize_required_storage_field("name", name))
            .transpose()?
            .unwrap_or_else(|| existing.name.clone()),
        connection,
        is_default: input.is_default,
    })
}

async fn normalize_connection<S: FollowerRuntimeState>(
    state: &S,
    connection: StorageConnectionInput,
) -> Result<StorageConnectionInput> {
    state
        .driver_registry()
        .connectors()
        .require_remote_target_connector(&connection.connector_config.connector_id)?;
    let mut connection = crate::storage::connectors::normalize_storage_connection(
        state.driver_registry().connectors(),
        state.writer_db(),
        connection,
    )
    .await?;
    if connection.connector_config.connector_id.as_str()
        == crate::storage::connectors::LocalConnector::ID
    {
        let base_path = connection
            .connector_config
            .values
            .get("base_path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(".");
        connection.connector_config.values.insert(
            "base_path".to_string(),
            serde_json::Value::String(super::paths::normalize_relative_local_path(base_path)?),
        );
    }
    Ok(connection)
}

pub(in crate::services::remote::storage_target) fn new_target_key() -> String {
    format!("rst_{}", aster_forge_utils::id::new_short_token())
}

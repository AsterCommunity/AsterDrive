use std::sync::Arc;

use crate::errors::{AsterError, Result};
use crate::storage::remote_protocol::RemoteStorageTargetInfo;
use aster_drive_model::entities::remote_storage_target;
use aster_drive_storage::{ConnectorConfigEnvelope, StorageDriver};

use super::driver::connector_available;

#[derive(Clone)]
pub struct ResolvedRemoteStorageTarget {
    pub driver: Arc<dyn StorageDriver>,
}

pub(super) fn present_target(
    model: remote_storage_target::Model,
    credential_configured: bool,
) -> Result<RemoteStorageTargetInfo> {
    let connector_config: ConnectorConfigEnvelope = serde_json::from_str(&model.connector_config)
        .map_err(|error| {
        AsterError::database_operation(format!("invalid remote target connector config: {error}"))
    })?;
    Ok(RemoteStorageTargetInfo {
        target_key: model.target_key,
        name: model.name,
        connector_id: model.connector_id.clone(),
        connector_config,
        credential_configured,
        connector_available: connector_available(&model.connector_id),
        is_default: model.is_default,
        desired_revision: model.desired_revision,
        applied_revision: model.applied_revision,
        last_error: model.last_error,
        created_at: model.created_at,
        updated_at: model.updated_at,
    })
}

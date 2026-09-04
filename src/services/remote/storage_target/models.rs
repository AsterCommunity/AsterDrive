use std::sync::Arc;

use crate::storage::remote_protocol::RemoteStorageTargetInfo;
use aster_drive_model::entities::remote_storage_target;
use aster_drive_storage::StorageDriver;

#[derive(Clone)]
pub struct ResolvedRemoteStorageTarget {
    pub driver: Arc<dyn StorageDriver>,
}

impl TryFrom<remote_storage_target::Model> for RemoteStorageTargetInfo {
    type Error = crate::errors::AsterError;

    fn try_from(model: remote_storage_target::Model) -> Result<Self, Self::Error> {
        let connector_id = model.connector_id.ok_or_else(|| {
            crate::errors::AsterError::database_operation(format!(
                "remote storage target #{} has no connector id after startup conversion",
                model.id
            ))
        })?;
        let connector_config = model.connector_config.ok_or_else(|| {
            crate::errors::AsterError::database_operation(format!(
                "remote storage target #{} has no connector config after startup conversion",
                model.id
            ))
        })?;
        let connector_config = serde_json::from_str(&connector_config).map_err(|error| {
            crate::errors::AsterError::database_operation(format!(
                "remote storage target #{} has invalid connector config: {error}",
                model.id
            ))
        })?;
        Ok(Self {
            target_key: model.target_key,
            name: model.name,
            connector_id,
            connector_config,
            is_default: model.is_default,
            desired_revision: model.desired_revision,
            applied_revision: model.applied_revision,
            last_error: model.last_error,
            created_at: model.created_at,
            updated_at: model.updated_at,
        })
    }
}

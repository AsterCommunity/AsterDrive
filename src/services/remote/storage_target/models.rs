use std::sync::Arc;

use crate::storage::remote_protocol::RemoteStorageTargetInfo;
use aster_drive_model::entities::remote_storage_target;
use aster_drive_storage::StorageDriver;

#[derive(Clone)]
pub struct ResolvedRemoteStorageTarget {
    pub driver: Arc<dyn StorageDriver>,
}

impl From<remote_storage_target::Model> for RemoteStorageTargetInfo {
    fn from(model: remote_storage_target::Model) -> Self {
        Self {
            target_key: model.target_key,
            name: model.name,
            connector_id: model.connector_id.or_else(|| {
                super::driver::remote_storage_target_connector_id(model.driver_type)
                    .ok()
                    .map(|id| id.to_string())
            }),
            connector_config: model
                .connector_config
                .as_deref()
                .and_then(|raw| serde_json::from_str(raw).ok()),
            endpoint: model.endpoint,
            bucket: model.bucket,
            base_path: model.base_path,
            is_default: model.is_default,
            desired_revision: model.desired_revision,
            applied_revision: model.applied_revision,
            last_error: model.last_error,
            created_at: model.created_at,
            updated_at: model.updated_at,
        }
    }
}

use std::sync::Arc;

use crate::entities::remote_storage_target;
use crate::storage::StorageDriver;
use crate::storage::remote_protocol::RemoteStorageTargetInfo;

#[derive(Clone)]
pub struct ResolvedRemoteStorageTarget {
    pub driver: Arc<dyn StorageDriver>,
}

impl From<remote_storage_target::Model> for RemoteStorageTargetInfo {
    fn from(model: remote_storage_target::Model) -> Self {
        Self {
            target_key: model.target_key,
            name: model.name,
            driver_type: model.driver_type,
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

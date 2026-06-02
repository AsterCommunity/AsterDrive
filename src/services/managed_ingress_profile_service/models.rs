use std::sync::Arc;

use crate::entities::managed_ingress_profile;
use crate::storage::StorageDriver;
use crate::storage::remote_protocol::RemoteIngressProfileInfo;

#[derive(Clone)]
pub struct ResolvedIngressTarget {
    pub driver: Arc<dyn StorageDriver>,
    pub max_file_size: i64,
}

impl From<managed_ingress_profile::Model> for RemoteIngressProfileInfo {
    fn from(model: managed_ingress_profile::Model) -> Self {
        Self {
            profile_key: model.profile_key,
            name: model.name,
            driver_type: model.driver_type,
            endpoint: model.endpoint,
            bucket: model.bucket,
            base_path: model.base_path,
            max_file_size: model.max_file_size,
            is_default: model.is_default,
            desired_revision: model.desired_revision,
            applied_revision: model.applied_revision,
            last_error: model.last_error,
            created_at: model.created_at,
            updated_at: model.updated_at,
        }
    }
}

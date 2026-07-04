use crate::api::api_error_code::ApiErrorCode;
use crate::errors::{Result, validation_error_with_code};
use crate::storage::remote_protocol::RemoteStorageCapabilities;
use crate::types::DriverType;

use super::driver::{
    RemoteStorageTargetDriverDescriptor, registered_remote_storage_target_driver_types,
    remote_storage_target_driver_descriptor,
};

#[derive(Debug, Clone)]
pub(super) struct RemoteStorageTargetCapabilityResolver {
    remote_node_id: i64,
    capabilities: RemoteStorageCapabilities,
}

impl RemoteStorageTargetCapabilityResolver {
    pub fn from_last_capabilities(remote_node_id: i64, last_capabilities: &str) -> Self {
        Self {
            remote_node_id,
            capabilities: RemoteStorageCapabilities::from_stored_json(last_capabilities),
        }
    }

    pub fn driver_descriptors(&self) -> Vec<RemoteStorageTargetDriverDescriptor> {
        self.supported_registered_driver_types()
            .into_iter()
            .filter_map(|driver_type| remote_storage_target_driver_descriptor(driver_type).ok())
            .collect()
    }

    pub fn ensure_driver_supported(&self, driver_type: DriverType) -> Result<()> {
        if self.supports_driver(driver_type) {
            return Ok(());
        }

        Err(validation_error_with_code(
            ApiErrorCode::ManagedIngressDriverUnsupported,
            format!(
                "remote node #{} does not declare remote storage target support for the {} driver",
                self.remote_node_id,
                driver_type.as_str()
            ),
        ))
    }

    fn supports_driver(&self, driver_type: DriverType) -> bool {
        self.capabilities
            .effective_remote_storage_targets()
            .supports_known_driver(driver_type)
            && remote_storage_target_driver_descriptor(driver_type).is_ok()
    }

    fn supported_registered_driver_types(&self) -> Vec<DriverType> {
        let managed_ingress = self.capabilities.effective_remote_storage_targets();
        if !managed_ingress.enabled {
            return Vec::new();
        }

        registered_remote_storage_target_driver_types()
            .into_iter()
            .filter(|driver_type| managed_ingress.supports_known_driver(*driver_type))
            .filter(|driver_type| remote_storage_target_driver_descriptor(*driver_type).is_ok())
            .collect()
    }
}

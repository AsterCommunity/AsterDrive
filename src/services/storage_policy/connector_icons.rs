//! Built-in connector icon assets and their browser-facing endpoint.
//!
//! Connector descriptors carry a stable URL instead of a frontend build path.
//! The bytes stay in the backend binary so every Primary serves the same
//! connector-owned asset without exposing deployment-specific filesystem paths.

use aster_drive_storage::connector_descriptor::StorageConnectorIconDescriptor;
use aster_drive_storage::{ConnectorId, StorageConnectorDescriptor};

pub const ICON_ENDPOINT_PREFIX: &str = "/api/v1/storage/connectors/";

pub(crate) fn icon_url(connector_id: &ConnectorId, revision: &str) -> String {
    format!(
        "{ICON_ENDPOINT_PREFIX}{}/icon?v={revision}",
        connector_id.as_str(),
    )
}

pub(crate) fn descriptor_with_backend_icon(
    mut descriptor: StorageConnectorDescriptor,
    icon: Option<crate::storage::connectors::StorageConnectorIcon>,
) -> StorageConnectorDescriptor {
    if let Some(icon) = icon {
        descriptor.ui.icon = Some(StorageConnectorIconDescriptor {
            url: icon_url(&descriptor.connector_id, icon.revision),
            content_type: icon.content_type.to_string(),
            revision: icon.revision.to_string(),
        });
    }
    descriptor
}

pub(crate) fn descriptors_with_backend_icons(
    registry: &crate::storage::connectors::StorageConnectorRegistry,
) -> Vec<StorageConnectorDescriptor> {
    registry
        .descriptors()
        .into_iter()
        .map(|descriptor| {
            descriptor_with_backend_icon(
                descriptor.clone(),
                registry.icon(&descriptor.connector_id),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_urls_are_stable() {
        for id in [
            "asterdrive.storage.local",
            "asterdrive.storage.s3",
            "asterdrive.storage.alibaba_oss",
            "asterdrive.storage.sftp",
            "asterdrive.storage.azure_blob",
            "asterdrive.storage.huawei_obs",
            "asterdrive.storage.tencent_cos",
            "asterdrive.storage.remote",
            "asterdrive.storage.onedrive",
            "asterdrive.storage.qiniu",
        ] {
            let connector_id = ConnectorId::declared(id);
            assert!(icon_url(&connector_id, "1").contains("/api/v1/storage/connectors/"));
        }
    }

    #[test]
    fn built_in_descriptors_point_at_backend_assets() {
        let registry = crate::storage::connectors::builtin_storage_connector_registry()
            .expect("built-in connector registry");
        let descriptors = descriptors_with_backend_icons(&registry);
        assert_eq!(descriptors.len(), 10);
        for descriptor in descriptors {
            let icon = registry
                .icon(&descriptor.connector_id)
                .expect("built-in connector icon");
            assert!(!icon.bytes.is_empty());
            let descriptor_icon = descriptor.ui.icon.expect("descriptor icon");
            assert_eq!(
                descriptor_icon.url,
                icon_url(&descriptor.connector_id, icon.revision)
            );
            assert_eq!(descriptor_icon.content_type, icon.content_type);
            assert_eq!(descriptor_icon.revision, icon.revision);
        }
    }
}

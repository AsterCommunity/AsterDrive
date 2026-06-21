use async_trait::async_trait;

use crate::entities::storage_policy;
use crate::errors::Result;
use crate::runtime::RemoteProtocolRuntimeState;
use crate::storage::StorageDriver;
use crate::storage::connector_descriptor::{
    StorageConnectorCapabilities, StorageConnectorCredentialMode, StorageConnectorDescriptor,
    StorageConnectorDescriptorProvider, StorageConnectorFieldKind, StorageConnectorFieldScope,
    StorageConnectorUiDescriptorInput, StorageConnectorUploadWorkflows,
    draft_connection_test_action_descriptor, saved_connection_test_action_descriptor,
    server_relay_simple_upload_capabilities, storage_connector_field,
    storage_connector_ui_descriptor,
};
use crate::storage::drivers::local::LocalDriver;
use crate::types::DriverType;

use super::{StorageConnector, StorageConnectorConnectionInput, StorageConnectorUploadTransport};

pub struct LocalConnector;

impl StorageConnectorDescriptorProvider for LocalConnector {
    fn storage_connector_descriptor() -> StorageConnectorDescriptor {
        StorageConnectorDescriptor {
            driver_type: DriverType::Local,
            enabled: true,
            label: "Local filesystem".to_string(),
            description: "Server-local filesystem storage policy".to_string(),
            ui: storage_connector_ui_descriptor(StorageConnectorUiDescriptorInput {
                label_key: "driver_type_local",
                description_key: "policy_wizard_local_storage_desc",
                icon_src: Some("/static/asterdrive/asterdrive-dark.svg"),
                icon_name: None,
                helper_key: "policy_wizard_local_helper",
                config_step_title_key: "policy_wizard_step_local_title",
                config_step_description_key: "policy_wizard_step_local_desc",
                edit_context_key: "policy_edit_context_local_desc",
                base_path_empty_display: "./data",
                base_path_placeholder: "./data",
            }),
            credential_mode: StorageConnectorCredentialMode::None,
            requires_authorization: false,
            authorization_provider: None,
            capabilities: StorageConnectorCapabilities {
                efficient_range: true,
                capacity: true,
                list: true,
                presigned_download: false,
                storage_native_thumbnail: false,
                storage_native_media_metadata: false,
                remote_node_binding: false,
                s3_transfer_strategy: false,
            },
            upload_workflows: StorageConnectorUploadWorkflows {
                simple_upload: true,
                simple_upload_capabilities: server_relay_simple_upload_capabilities(None),
                stream_upload: true,
                object_multipart_upload: false,
                object_multipart_upload_capabilities: None,
                provider_resumable_upload: false,
                presigned_upload: false,
                frontend_direct_provider_resumable_upload: false,
                provider_resumable_upload_capabilities: None,
            },
            fields: vec![
                storage_connector_field(
                    "base_path",
                    StorageConnectorFieldScope::Connection,
                    StorageConnectorFieldKind::Text,
                    false,
                    false,
                ),
                storage_connector_field(
                    "content_dedup",
                    StorageConnectorFieldScope::PolicyOptions,
                    StorageConnectorFieldKind::Boolean,
                    false,
                    false,
                ),
            ],
            actions: vec![
                draft_connection_test_action_descriptor(),
                saved_connection_test_action_descriptor(false),
            ],
            driver_recommendations: Vec::new(),
            related_issues: vec![328],
        }
    }
}

#[async_trait(?Send)]
impl StorageConnector for LocalConnector {
    fn driver_type() -> DriverType {
        DriverType::Local
    }

    fn normalize_connection_fields(endpoint: &str, bucket: &str) -> Result<(String, String)> {
        Ok((endpoint.trim().to_string(), bucket.trim().to_string()))
    }

    fn validate_connection_credentials(input: &StorageConnectorConnectionInput) -> Result<()> {
        let _ = input;
        Ok(())
    }

    async fn build_draft_driver<S: RemoteProtocolRuntimeState + Sync + ?Sized>(
        state: &S,
        policy: &storage_policy::Model,
    ) -> Result<Box<dyn StorageDriver>> {
        let _ = state;
        Ok(Box::new(LocalDriver::new(policy)?))
    }

    fn upload_transport(policy: &storage_policy::Model) -> StorageConnectorUploadTransport {
        let _ = policy;
        StorageConnectorUploadTransport::Local
    }
}

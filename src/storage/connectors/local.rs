use async_trait::async_trait;

use crate::errors::Result;
use crate::storage::drivers::local::{DEFAULT_LOCAL_STORAGE_PATH, LocalDriver};
use aster_drive_model::entities::storage_policy;
use aster_drive_storage::StorageConnectorConfigSchema;
use aster_drive_storage::StorageDriver;
use aster_drive_storage::connector_descriptor::{
    StorageConnectorCapabilities, StorageConnectorDeploymentScope, StorageConnectorDescriptor,
    StorageConnectorFieldKind, StorageConnectorFieldScope, StorageConnectorObjectNamingMode,
    StorageConnectorUiDescriptorInput, StorageConnectorUploadWorkflows,
    draft_connection_test_action_descriptor, saved_connection_test_action_descriptor,
    server_relay_simple_upload_capabilities, storage_connector_field,
    storage_connector_ui_descriptor,
};

use super::LocalFilesystemPolicyProjection;
use super::{StorageConnector, StorageConnectorCredentialInput, StorageConnectorUploadTransport};

pub struct LocalConnector;

aster_drive_storage::storage_connector_schema! {
    pub struct LocalConnectorConfigV1 {
        config {
            pub base_path: String => storage_connector_field(
                "base_path",
                StorageConnectorFieldScope::ConnectorConfig,
                StorageConnectorFieldKind::Text,
                false,
                false,
            ),
            pub content_dedup: bool => {
                let mut field = storage_connector_field(
                    "content_dedup",
                    StorageConnectorFieldScope::ConnectorConfig,
                    StorageConnectorFieldKind::Boolean,
                    false,
                    false,
                );
                field.default_value = Some(
                    aster_drive_storage::StorageConnectorFieldDefaultValue::Boolean(false),
                );
                field
            },
        }
        credentials none
    }
}

impl LocalConnector {
    pub const ID: &'static str = "asterdrive.storage.local";

    fn decode_config(policy: &storage_policy::Model) -> Result<LocalConnectorConfigV1> {
        super::common::decode_typed_policy_config(policy, Self::ID, 1)
            .map(|(config, _behavior)| config)
    }
}

impl LocalConnector {
    fn descriptor_definition() -> StorageConnectorDescriptor {
        StorageConnectorDescriptor {
            connector_id: aster_drive_storage::ConnectorId::declared(Self::ID),
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
                base_path_empty_display: DEFAULT_LOCAL_STORAGE_PATH,
                base_path_placeholder: DEFAULT_LOCAL_STORAGE_PATH,
            }),
            credential_mode: LocalConnectorConfigV1::credential_mode(),
            deployment_scope: StorageConnectorDeploymentScope::InstanceLocal,
            supports_initial_setup: true,
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
                object_storage_transfer_strategy: false,
                object_naming: StorageConnectorObjectNamingMode::OpaqueUuid,
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
            fields: LocalConnectorConfigV1::descriptor_fields(),
            config_schema_version: 1,
            actions: vec![
                draft_connection_test_action_descriptor(),
                saved_connection_test_action_descriptor(false),
            ],
            driver_recommendations: Vec::new(),
            related_issues: vec![328],
        }
    }
}

#[async_trait]
impl StorageConnector for LocalConnector {
    fn descriptor(&self) -> StorageConnectorDescriptor {
        Self::descriptor_definition()
    }

    async fn build_draft_driver(
        &self,
        context: &super::StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
        credential: &StorageConnectorCredentialInput,
    ) -> Result<Box<dyn StorageDriver>> {
        let _ = (context, credential);
        let config = Self::decode_config(policy)?;
        Ok(Box::new(LocalDriver::new(&config.base_path)?))
    }

    fn build_runtime_driver(
        &self,
        _registry: &crate::storage::DriverRegistry,
        policy: &storage_policy::Model,
    ) -> Result<super::StorageConnectorDriver> {
        let config = Self::decode_config(policy)?;
        Ok(super::StorageConnectorDriver::storage(std::sync::Arc::new(
            LocalDriver::new(&config.base_path)?,
        )))
    }

    fn upload_transport(
        &self,
        policy: &storage_policy::Model,
    ) -> Result<StorageConnectorUploadTransport> {
        let _ = policy;
        Ok(StorageConnectorUploadTransport::Local)
    }

    fn local_filesystem_projection(
        &self,
        policy: &storage_policy::Model,
    ) -> Result<Option<LocalFilesystemPolicyProjection>> {
        let config = Self::decode_config(policy)?;
        Ok(Some(LocalFilesystemPolicyProjection {
            base_path: config.base_path,
            content_dedup: config.content_dedup,
        }))
    }
}

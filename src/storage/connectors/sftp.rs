use async_trait::async_trait;

use crate::entities::storage_policy;
use crate::errors::Result;
use crate::runtime::RemoteProtocolRuntimeState;
use crate::storage::StorageDriver;
use crate::storage::connector_descriptor::{
    StorageConnectorCapabilities, StorageConnectorCredentialMode, StorageConnectorDescriptor,
    StorageConnectorDescriptorProvider, StorageConnectorFieldDisplayInput,
    StorageConnectorFieldKind, StorageConnectorFieldScope, StorageConnectorUiDescriptorInput,
    StorageConnectorUploadWorkflows, draft_connection_test_action_descriptor,
    saved_connection_test_action_descriptor, server_relay_simple_upload_capabilities,
    storage_connector_field, storage_connector_field_with_display, storage_connector_ui_descriptor,
};
use crate::storage::drivers::sftp::SftpDriver;
use crate::types::DriverType;

use super::common::{ensure_onedrive_options_absent, validate_static_secret_credentials};
use super::{StorageConnector, StorageConnectorConnectionInput, StorageConnectorUploadTransport};

pub struct SftpConnector;

impl StorageConnectorDescriptorProvider for SftpConnector {
    fn storage_connector_descriptor() -> StorageConnectorDescriptor {
        StorageConnectorDescriptor {
            driver_type: DriverType::Sftp,
            enabled: true,
            label: "SFTP".to_string(),
            description: "SSH File Transfer Protocol storage policy".to_string(),
            ui: storage_connector_ui_descriptor(StorageConnectorUiDescriptorInput {
                label_key: "driver_type_sftp",
                description_key: "policy_wizard_sftp_storage_desc",
                icon_src: None,
                icon_name: Some("ServerCog"),
                helper_key: "policy_wizard_sftp_helper",
                config_step_title_key: "policy_wizard_step_sftp_title",
                config_step_description_key: "policy_wizard_step_sftp_desc",
                edit_context_key: "policy_edit_context_sftp_desc",
                base_path_empty_display: "core:root",
                base_path_placeholder: "/srv/asterdrive",
            }),
            credential_mode: StorageConnectorCredentialMode::StaticSecret,
            requires_authorization: false,
            authorization_provider: None,
            capabilities: StorageConnectorCapabilities {
                efficient_range: true,
                capacity: false,
                list: false,
                presigned_download: false,
                storage_native_thumbnail: false,
                storage_native_media_metadata: false,
                remote_node_binding: false,
                object_storage_transfer_strategy: false,
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
                storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
                    name: "endpoint",
                    scope: StorageConnectorFieldScope::Connection,
                    kind: StorageConnectorFieldKind::Text,
                    required: true,
                    secret: false,
                    label_key: "endpoint",
                    placeholder: Some("sftp://example.com:22"),
                    help_key: Some("sftp_endpoint_hint"),
                    required_message_key: None,
                    invalid_protocol_message_key: Some("sftp_endpoint_protocol_required_error"),
                    allowed_endpoint_protocols: vec!["sftp:"],
                    allow_endpoint_without_protocol: true,
                    trim_on_blur: true,
                    visible_when_driver_types: Vec::new(),
                }),
                storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
                    name: "access_key",
                    scope: StorageConnectorFieldScope::Connection,
                    kind: StorageConnectorFieldKind::Text,
                    required: true,
                    secret: false,
                    label_key: "sftp_username",
                    placeholder: None,
                    help_key: None,
                    required_message_key: None,
                    invalid_protocol_message_key: None,
                    allowed_endpoint_protocols: Vec::new(),
                    allow_endpoint_without_protocol: false,
                    trim_on_blur: true,
                    visible_when_driver_types: Vec::new(),
                }),
                storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
                    name: "secret_key",
                    scope: StorageConnectorFieldScope::Connection,
                    kind: StorageConnectorFieldKind::Secret,
                    required: true,
                    secret: true,
                    label_key: "sftp_password",
                    placeholder: None,
                    help_key: None,
                    required_message_key: None,
                    invalid_protocol_message_key: None,
                    allowed_endpoint_protocols: Vec::new(),
                    allow_endpoint_without_protocol: false,
                    trim_on_blur: false,
                    visible_when_driver_types: Vec::new(),
                }),
                storage_connector_field(
                    "base_path",
                    StorageConnectorFieldScope::Connection,
                    StorageConnectorFieldKind::Text,
                    false,
                    false,
                ),
                storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
                    name: "sftp_host_key_fingerprint",
                    scope: StorageConnectorFieldScope::PolicyOptions,
                    kind: StorageConnectorFieldKind::Text,
                    required: false,
                    secret: false,
                    label_key: "sftp_host_key_fingerprint",
                    placeholder: Some("SHA256:..."),
                    help_key: Some("sftp_host_key_fingerprint_hint"),
                    required_message_key: None,
                    invalid_protocol_message_key: None,
                    allowed_endpoint_protocols: Vec::new(),
                    allow_endpoint_without_protocol: false,
                    trim_on_blur: true,
                    visible_when_driver_types: Vec::new(),
                }),
            ],
            actions: vec![
                draft_connection_test_action_descriptor(),
                saved_connection_test_action_descriptor(false),
            ],
            driver_recommendations: Vec::new(),
            related_issues: vec![125],
        }
    }
}

#[async_trait(?Send)]
impl StorageConnector for SftpConnector {
    fn driver_type() -> DriverType {
        DriverType::Sftp
    }

    fn normalize_connection_fields(endpoint: &str, bucket: &str) -> Result<(String, String)> {
        let _ = bucket;
        Ok((SftpDriver::normalize_endpoint(endpoint)?, String::new()))
    }

    fn validate_connection_credentials(input: &StorageConnectorConnectionInput) -> Result<()> {
        validate_static_secret_credentials(input, "SFTP")?;
        SftpDriver::validate_connection_parts(
            &input.endpoint,
            &input.access_key,
            &input.secret_key,
            &input.base_path,
        )
    }

    fn supports_saved_draft_credentials() -> bool {
        true
    }

    async fn validate_policy_options<C: sea_orm::ConnectionTrait + Sync>(
        db: &C,
        remote_node_id: Option<i64>,
        options: &crate::types::StoragePolicyOptions,
    ) -> Result<()> {
        let _ = (db, remote_node_id);
        ensure_onedrive_options_absent(options)?;
        if let Some(fingerprint) = options.sftp_host_key_fingerprint.as_deref() {
            SftpDriver::validate_host_key_fingerprint(fingerprint)?;
        }
        Ok(())
    }

    async fn build_draft_driver<S: RemoteProtocolRuntimeState + Sync + ?Sized>(
        state: &S,
        policy: &storage_policy::Model,
    ) -> Result<Box<dyn StorageDriver>> {
        let _ = state;
        Ok(Box::new(SftpDriver::new(policy)?))
    }

    fn upload_transport(policy: &storage_policy::Model) -> StorageConnectorUploadTransport {
        let _ = policy;
        StorageConnectorUploadTransport::Sftp
    }
}

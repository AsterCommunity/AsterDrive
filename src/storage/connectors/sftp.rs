use async_trait::async_trait;

use crate::errors::AsterError;
use crate::errors::Result;
use crate::storage::drivers::sftp::{SftpDriver, SftpDriverConfig, SftpStaticCredentials};
use aster_drive_model::entities::storage_policy;
use aster_drive_storage::StorageConnectorConfigSchema;
use aster_drive_storage::StorageDriver;
use aster_drive_storage::connector_descriptor::{
    StorageConnectorCapabilities, StorageConnectorDeploymentScope, StorageConnectorDescriptor,
    StorageConnectorFieldDisplayInput, StorageConnectorFieldKind, StorageConnectorFieldScope,
    StorageConnectorObjectNamingMode, StorageConnectorUiDescriptorInput,
    StorageConnectorUploadWorkflows, draft_connection_test_action_descriptor,
    saved_connection_test_action_descriptor, server_relay_simple_upload_capabilities,
    storage_connector_field, storage_connector_field_with_display, storage_connector_ui_descriptor,
};

use super::{
    StorageConnector, StorageConnectorCredentialInput, StorageConnectorRuntimeCredential,
    StorageConnectorUploadTransport,
};

pub struct SftpConnector;

aster_drive_storage::storage_connector_schema! {
    pub struct SftpConnectorConfigV1 {
        config {
        pub endpoint: String => storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
            name: "endpoint",
            scope: StorageConnectorFieldScope::ConnectorConfig,
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
        }),
        pub base_path: String => storage_connector_field(
            "base_path", StorageConnectorFieldScope::ConnectorConfig,
            StorageConnectorFieldKind::Text, false, false,
        ),
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub sftp_host_key_fingerprint: Option<String> => {
            let mut field = storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
                name: "sftp_host_key_fingerprint",
                scope: StorageConnectorFieldScope::ConnectorConfig,
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
            });
            field.validation.max_length = Some(512);
            field
        },
        }
        credentials static SftpStaticCredentialsV1 {
            pub sftp_username: String => storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
                name: "sftp_username",
                scope: StorageConnectorFieldScope::StaticCredential,
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
            }),
            pub sftp_password: String => storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
                name: "sftp_password",
                scope: StorageConnectorFieldScope::StaticCredential,
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
            }),
        }
    }
}

impl SftpConnector {
    pub const ID: &'static str = "asterdrive.storage.sftp";

    fn decode_config(policy: &storage_policy::Model) -> Result<SftpConnectorConfigV1> {
        super::common::decode_typed_policy_config(policy, Self::ID, 1).map(|(config, _)| config)
    }

    fn driver_config(config: SftpConnectorConfigV1) -> SftpDriverConfig {
        SftpDriverConfig {
            endpoint: config.endpoint,
            base_path: config.base_path,
            host_key_fingerprint: config.sftp_host_key_fingerprint,
        }
    }

    fn driver_credentials(credentials: SftpStaticCredentialsV1) -> SftpStaticCredentials {
        SftpStaticCredentials {
            username: credentials.sftp_username,
            password: credentials.sftp_password,
        }
    }
}

impl SftpConnector {
    fn descriptor_definition() -> StorageConnectorDescriptor {
        StorageConnectorDescriptor {
            connector_id: aster_drive_storage::ConnectorId::declared(Self::ID),
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
            credential_mode: SftpConnectorConfigV1::credential_mode(),
            deployment_scope: StorageConnectorDeploymentScope::SharedAcrossPrimaryInstances,
            supports_initial_setup: true,
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
            fields: SftpConnectorConfigV1::descriptor_fields(),
            config_schema_version: 1,
            actions: vec![
                draft_connection_test_action_descriptor(),
                saved_connection_test_action_descriptor(false),
            ],
            driver_recommendations: Vec::new(),
            related_issues: vec![125],
        }
    }
}

#[async_trait]
impl StorageConnector for SftpConnector {
    fn descriptor(&self) -> StorageConnectorDescriptor {
        Self::descriptor_definition()
    }

    fn validate_connector_config(
        &self,
        input: &aster_drive_storage::ConnectorConfigEnvelope,
    ) -> Result<aster_drive_storage::ConnectorConfigEnvelope> {
        let normalized =
            aster_drive_storage::connector_descriptor::normalize_storage_connector_config(
                &self.descriptor(),
                input,
            )
            .map_err(|error| AsterError::validation_error(error.to_string()))?;
        let mut config: SftpConnectorConfigV1 =
            super::common::decode_normalized_connector_config(&normalized)?;
        config.endpoint = SftpDriver::normalize_endpoint(&config.endpoint)?;
        if let Some(fingerprint) = config.sftp_host_key_fingerprint.as_deref() {
            SftpDriver::validate_host_key_fingerprint(fingerprint)?;
        }
        super::common::encode_normalized_connector_config(
            normalized.connector_id,
            normalized.schema_version,
            config,
        )
    }

    fn validate_credential_input(&self, input: &StorageConnectorCredentialInput) -> Result<()> {
        let credential: SftpStaticCredentialsV1 =
            super::common::decode_static_credential(input, Self::ID)?;
        super::common::validate_required_credential_field(
            &credential.sftp_username,
            "sftp_username",
            Self::ID,
        )?;
        super::common::validate_required_credential_field(
            &credential.sftp_password,
            "sftp_password",
            Self::ID,
        )
    }

    fn import_legacy_credential(
        &self,
        _encryption_key: &str,
        _policy: &storage_policy::Model,
        input: super::LegacyStorageConnectorCredentialInput,
    ) -> Result<Option<serde_json::Value>> {
        super::common::import_legacy_static_credential(Self::ID, input, |legacy| {
            SftpStaticCredentialsV1 {
                sftp_username: legacy.access_key,
                sftp_password: legacy.secret_key,
            }
        })
    }

    async fn build_draft_driver(
        &self,
        context: &super::StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
        credential: &StorageConnectorCredentialInput,
    ) -> Result<Box<dyn StorageDriver>> {
        let _ = context;
        let config = Self::decode_config(policy)?;
        let credentials = super::common::decode_static_credential(credential, Self::ID)?;
        Ok(Box::new(SftpDriver::new(
            Self::driver_config(config),
            Self::driver_credentials(credentials),
        )?))
    }

    fn build_runtime_driver(
        &self,
        registry: &crate::storage::DriverRegistry,
        policy: &storage_policy::Model,
    ) -> Result<super::StorageConnectorDriver> {
        let config = Self::decode_config(policy)?;
        let Some(StorageConnectorRuntimeCredential::Static(values)) =
            registry.get_runtime_credential(policy.id)
        else {
            return Err(crate::errors::storage_driver_error(
                aster_drive_storage::StorageErrorKind::Auth,
                format!("storage policy {} is missing static credentials", policy.id),
            ));
        };
        let credentials: SftpStaticCredentialsV1 =
            serde_json::from_value(values).map_err(|error| {
                crate::errors::storage_driver_error(
                    aster_drive_storage::StorageErrorKind::Misconfigured,
                    format!(
                        "storage policy {} has invalid static credentials: {error}",
                        policy.id
                    ),
                )
            })?;
        Ok(super::StorageConnectorDriver::storage(std::sync::Arc::new(
            SftpDriver::new(
                Self::driver_config(config),
                Self::driver_credentials(credentials),
            )?,
        )))
    }

    fn upload_transport(
        &self,
        policy: &storage_policy::Model,
    ) -> Result<StorageConnectorUploadTransport> {
        let _ = policy;
        Ok(StorageConnectorUploadTransport::Sftp)
    }
}

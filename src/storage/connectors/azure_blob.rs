use async_trait::async_trait;

use crate::api::api_error_code::ApiErrorCode;
use crate::errors::Result;
use crate::storage::drivers::azure_blob::{
    AzureBlobConfigError, AzureBlobDriver, AzureBlobDriverConfig, AzureBlobStaticCredentials,
};
use aster_drive_model::entities::storage_policy;
use aster_drive_model::types::{ObjectStorageDownloadStrategy, ObjectStorageUploadStrategy};
use aster_drive_storage::StorageDriver;
use aster_drive_storage::connector_descriptor::{
    ObjectStorageConnectorDescriptorInput, StorageConnectorBadgeRgb,
    StorageConnectorDeploymentScope, StorageConnectorDescriptor, StorageConnectorFieldDisplayInput,
    StorageConnectorFieldKind, StorageConnectorFieldScope, StorageConnectorUiDescriptorInput,
    object_storage_connector_descriptor, storage_connector_field,
    storage_connector_field_with_display,
};
use aster_drive_storage::{StorageConnectorConfigSchema, StorageConnectorFieldDefaultValue};

use super::common::{StorageTransferDirection, transfer_strategy_field};
use super::{StorageConnector, StorageConnectorCredentialInput, StorageConnectorUploadTransport};

pub struct AzureBlobConnector;

aster_drive_storage::storage_connector_schema! {
    pub struct AzureBlobConnectorConfigV1 {
        config {
        pub endpoint: String => storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
            name: "endpoint", scope: StorageConnectorFieldScope::ConnectorConfig,
            kind: StorageConnectorFieldKind::Text, required: true, secret: false,
            label_key: "endpoint", placeholder: Some("https://<account>.blob.core.windows.net"),
            help_key: Some("azure_blob_endpoint_hint"), required_message_key: None,
            invalid_protocol_message_key: Some("azure_blob_endpoint_protocol_required_error"),
            allowed_endpoint_protocols: vec!["http:", "https:"],
            allow_endpoint_without_protocol: false, trim_on_blur: false,
        }),
        pub bucket: String => {
            let mut field = storage_connector_field(
                "bucket", StorageConnectorFieldScope::ConnectorConfig,
                StorageConnectorFieldKind::Text, true, false,
            );
            field.required_message_key = Some("policy_wizard_container_required".to_string());
            field
        },
        pub base_path: String => {
            let mut field = storage_connector_field(
                "base_path", StorageConnectorFieldScope::ConnectorConfig,
                StorageConnectorFieldKind::Text, false, false,
            );
            field.default_value = Some(StorageConnectorFieldDefaultValue::String(String::new()));
            field.default_mode = aster_drive_storage::StorageConnectorFieldDefaultMode::MissingOrEmptyText;
            field
        },
        pub object_storage_upload_strategy: ObjectStorageUploadStrategy => transfer_strategy_field(
            "object_storage_upload_strategy", StorageTransferDirection::Upload,
        ),
        pub object_storage_download_strategy: ObjectStorageDownloadStrategy => transfer_strategy_field(
            "object_storage_download_strategy", StorageTransferDirection::Download,
        ),
        }
        credentials static AzureBlobStaticCredentialsV1 {
            pub azure_blob_account_name: String => storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
                name: "azure_blob_account_name", scope: StorageConnectorFieldScope::StaticCredential,
                kind: StorageConnectorFieldKind::Text, required: true, secret: false,
                label_key: "azure_blob_account_name", placeholder: None, help_key: None,
                required_message_key: None, invalid_protocol_message_key: None,
                allowed_endpoint_protocols: Vec::new(), allow_endpoint_without_protocol: false,
                trim_on_blur: true,
            }),
            pub azure_blob_account_key: String => storage_connector_field(
                "azure_blob_account_key", StorageConnectorFieldScope::StaticCredential,
                StorageConnectorFieldKind::Secret, true, true,
            ),
        }
    }
}

impl AzureBlobConnector {
    pub const ID: &'static str = "asterdrive.storage.azure_blob";

    fn decode_config(policy: &storage_policy::Model) -> Result<AzureBlobConnectorConfigV1> {
        super::common::decode_typed_policy_config(policy, Self::ID, 1)
            .map(|(config, _behavior)| config)
    }

    fn driver_config(
        policy: &storage_policy::Model,
        config: AzureBlobConnectorConfigV1,
    ) -> AzureBlobDriverConfig {
        AzureBlobDriverConfig {
            endpoint: config.endpoint,
            container: config.bucket,
            base_path: config.base_path,
            chunk_size: policy.chunk_size,
        }
    }

    fn driver_credentials(credentials: AzureBlobStaticCredentialsV1) -> AzureBlobStaticCredentials {
        AzureBlobStaticCredentials {
            account_name: credentials.azure_blob_account_name,
            account_key: credentials.azure_blob_account_key,
        }
    }
}

impl AzureBlobConnector {
    fn descriptor_definition() -> StorageConnectorDescriptor {
        object_storage_connector_descriptor(ObjectStorageConnectorDescriptorInput {
            connector_id: aster_drive_storage::ConnectorId::declared(Self::ID),
            label: "Azure Blob Storage",
            description: "Azure Blob block blob storage policy",
            ui: StorageConnectorUiDescriptorInput {
                label_key: "driver_type_azure_blob",
                description_key: "policy_wizard_azure_blob_storage_desc",
                icon_src: Some("/static/storage/azure-blob.svg"),
                icon_name: None,
                badge_rgb: StorageConnectorBadgeRgb::new(14, 165, 233),
                helper_key: "policy_wizard_azure_blob_helper",
                config_step_title_key: "policy_wizard_step_connection_title",
                config_step_description_key: "policy_wizard_step_azure_blob_connection_desc",
                edit_context_key: "policy_edit_context_azure_blob_desc",
                base_path_empty_display: "core:root",
                base_path_placeholder: "tenant/prefix",
            },
            deployment_scope: StorageConnectorDeploymentScope::SharedAcrossPrimaryInstances,
            supports_initial_setup: true,
            credential_mode: AzureBlobConnectorConfigV1::credential_mode(),
            fields: AzureBlobConnectorConfigV1::descriptor_fields(),
            presigned_part_etag_required: false,
            storage_native_processing: false,
            config_schema_version: 1,
            related_issues: vec![328, 329],
        })
    }
}

#[async_trait]
impl StorageConnector for AzureBlobConnector {
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
            .map_err(|error| crate::errors::AsterError::validation_error(error.to_string()))?;
        let mut config: AzureBlobConnectorConfigV1 =
            super::common::decode_normalized_connector_config(&normalized)?;
        let connection =
            AzureBlobDriver::try_normalize_endpoint_and_container(&config.endpoint, &config.bucket)
                .map_err(|error| {
                    let api_code = match &error {
                        AzureBlobConfigError::MissingContainer => {
                            ApiErrorCode::PolicyStorageBucketRequired
                        }
                        AzureBlobConfigError::MissingEndpoint
                        | AzureBlobConfigError::InvalidEndpoint(_) => {
                            ApiErrorCode::PolicyStorageEndpointInvalid
                        }
                    };
                    error.into_aster_error().with_api_error_code(api_code)
                })?;
        config.endpoint = connection.endpoint;
        config.bucket = connection.container;
        super::common::encode_normalized_connector_config(
            normalized.connector_id,
            normalized.schema_version,
            config,
        )
    }

    fn validate_credential_input(&self, input: &StorageConnectorCredentialInput) -> Result<()> {
        let credential: AzureBlobStaticCredentialsV1 =
            super::common::decode_static_credential(input, Self::ID)?;
        super::common::validate_required_credential_field(
            &credential.azure_blob_account_name,
            "azure_blob_account_name",
            Self::ID,
        )?;
        super::common::validate_required_credential_field(
            &credential.azure_blob_account_key,
            "azure_blob_account_key",
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
            AzureBlobStaticCredentialsV1 {
                azure_blob_account_name: legacy.access_key,
                azure_blob_account_key: legacy.secret_key,
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
        Ok(Box::new(AzureBlobDriver::new(
            Self::driver_config(policy, config),
            Self::driver_credentials(credentials),
        )?))
    }

    fn build_runtime_driver(
        &self,
        registry: &crate::storage::DriverRegistry,
        policy: &storage_policy::Model,
    ) -> Result<super::StorageConnectorDriver> {
        let config = Self::decode_config(policy)?;
        let credentials: AzureBlobStaticCredentialsV1 =
            super::common::runtime_static_credential(registry, policy, Self::ID)?;
        Ok(super::StorageConnectorDriver::multipart(
            std::sync::Arc::new(AzureBlobDriver::new(
                Self::driver_config(policy, config),
                Self::driver_credentials(credentials),
            )?),
        ))
    }

    fn upload_transport(
        &self,
        policy: &storage_policy::Model,
    ) -> Result<StorageConnectorUploadTransport> {
        let config = Self::decode_config(policy)?;
        Ok(StorageConnectorUploadTransport::ObjectStorage(
            config.object_storage_upload_strategy,
        ))
    }

    fn presigned_download_enabled(&self, policy: &storage_policy::Model) -> Result<bool> {
        let config = Self::decode_config(policy)?;
        Ok(config.object_storage_download_strategy == ObjectStorageDownloadStrategy::Presigned)
    }
}

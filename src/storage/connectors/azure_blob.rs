use async_trait::async_trait;

use crate::api::api_error_code::ApiErrorCode;
use crate::errors::Result;
use crate::storage::drivers::azure_blob::{AzureBlobConfigError, AzureBlobDriver};
use aster_drive_model::entities::storage_policy;
use aster_drive_model::types::{
    ObjectStorageDownloadStrategy, ObjectStorageUploadStrategy, parse_storage_policy_options,
};
use aster_drive_storage::StorageDriver;
use aster_drive_storage::connector_descriptor::{
    ObjectStorageConnectorDescriptorInput, StorageConnectorDeploymentScope,
    StorageConnectorDescriptor, StorageConnectorFieldDisplayInput, StorageConnectorFieldKind,
    StorageConnectorFieldScope, StorageConnectorUiDescriptorInput,
    object_storage_connector_descriptor, storage_connector_field,
    storage_connector_field_with_display, storage_connector_field_with_options,
};
use aster_drive_storage::{StorageConnectorConfigSchema, StorageConnectorFieldDefaultValue};

use super::common::validate_static_secret_credentials;
use super::{StorageConnector, StorageConnectorConnectionInput, StorageConnectorUploadTransport};

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
        pub base_path: String => storage_connector_field(
            "base_path", StorageConnectorFieldScope::ConnectorConfig,
            StorageConnectorFieldKind::Text, false, false,
        ),
        pub object_storage_upload_strategy: ObjectStorageUploadStrategy => object_transfer_field(
            "object_storage_upload_strategy",
        ),
        pub object_storage_download_strategy: ObjectStorageDownloadStrategy => object_transfer_field(
            "object_storage_download_strategy",
        ),
        }
        credentials static {
            access_key => storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
                name: "access_key", scope: StorageConnectorFieldScope::StaticCredential,
                kind: StorageConnectorFieldKind::Text, required: true, secret: false,
                label_key: "azure_blob_account_name", placeholder: None, help_key: None,
                required_message_key: None, invalid_protocol_message_key: None,
                allowed_endpoint_protocols: Vec::new(), allow_endpoint_without_protocol: false,
                trim_on_blur: true,
            }),
            secret_key => storage_connector_field(
                "secret_key", StorageConnectorFieldScope::StaticCredential,
                StorageConnectorFieldKind::Secret, true, true,
            ),
        }
    }
}

fn object_transfer_field(name: &str) -> aster_drive_storage::StorageConnectorFieldDescriptor {
    let mut field = storage_connector_field_with_options(
        name,
        StorageConnectorFieldScope::ConnectorConfig,
        StorageConnectorFieldKind::Select,
        true,
        false,
        vec!["relay_stream", "presigned"],
    );
    field.default_value = Some(StorageConnectorFieldDefaultValue::String(
        "relay_stream".to_string(),
    ));
    field
}

impl AzureBlobConnector {
    pub const ID: &'static str = "asterdrive.storage.azure_blob";
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

    fn encode_config(
        &self,
        input: &StorageConnectorConnectionInput,
    ) -> Result<aster_drive_model::types::StoredConnectorConfig> {
        super::common::encode_typed_connector_config(
            Self::ID,
            1,
            AzureBlobConnectorConfigV1 {
                endpoint: input.endpoint.clone(),
                bucket: input.bucket.clone(),
                base_path: input.base_path.clone(),
                object_storage_upload_strategy: input
                    .options
                    .effective_object_storage_upload_strategy(),
                object_storage_download_strategy: input
                    .options
                    .effective_object_storage_download_strategy(),
            },
        )
    }

    fn normalize_connection_fields(
        &self,
        endpoint: &str,
        bucket: &str,
    ) -> Result<(String, String)> {
        let normalized = AzureBlobDriver::try_normalize_endpoint_and_container(endpoint, bucket)
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
        Ok((normalized.endpoint, normalized.container))
    }

    fn validate_connection_credentials(
        &self,
        input: &StorageConnectorConnectionInput,
    ) -> Result<()> {
        validate_static_secret_credentials(input, "Azure Blob")
    }

    fn supports_saved_draft_credentials(&self) -> bool {
        true
    }

    async fn build_draft_driver(
        &self,
        context: &super::StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
    ) -> Result<Box<dyn StorageDriver>> {
        let _ = context;
        Ok(Box::new(AzureBlobDriver::new(policy)?))
    }

    fn build_runtime_driver(
        &self,
        _registry: &crate::storage::DriverRegistry,
        policy: &storage_policy::Model,
    ) -> Result<super::StorageConnectorDriver> {
        Ok(super::StorageConnectorDriver::multipart(
            std::sync::Arc::new(AzureBlobDriver::new(policy)?),
        ))
    }

    fn upload_transport(&self, policy: &storage_policy::Model) -> StorageConnectorUploadTransport {
        let options = parse_storage_policy_options(policy.options.as_ref());
        StorageConnectorUploadTransport::ObjectStorage(
            options.effective_object_storage_upload_strategy(),
        )
    }

    fn presigned_download_enabled(&self, policy: &storage_policy::Model) -> bool {
        let options = parse_storage_policy_options(policy.options.as_ref());
        options.effective_object_storage_download_strategy()
            == ObjectStorageDownloadStrategy::Presigned
    }
}

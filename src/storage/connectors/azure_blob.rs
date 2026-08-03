use async_trait::async_trait;

use crate::api::api_error_code::ApiErrorCode;
use crate::errors::Result;
use crate::storage::drivers::azure_blob::{AzureBlobConfigError, AzureBlobDriver};
use aster_drive_model::entities::storage_policy;
use aster_drive_model::types::{ObjectStorageDownloadStrategy, parse_storage_policy_options};
use aster_drive_storage::StorageDriver;
use aster_drive_storage::connector_descriptor::{
    ObjectStorageConnectorDescriptorInput, ObjectStorageFieldDescriptorInput,
    StorageConnectorDeploymentScope, StorageConnectorDescriptor, StorageConnectorUiDescriptorInput,
    object_storage_connector_descriptor,
};

use super::common::validate_static_secret_credentials;
use super::{StorageConnector, StorageConnectorConnectionInput, StorageConnectorUploadTransport};

pub struct AzureBlobConnector;

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
            fields: ObjectStorageFieldDescriptorInput {
                endpoint_placeholder: "https://<account>.blob.core.windows.net",
                endpoint_help_key: "azure_blob_endpoint_hint",
                endpoint_protocol_error_key: "azure_blob_endpoint_protocol_required_error",
                bucket_required_message_key: "policy_wizard_container_required",
                access_key_label_key: "azure_blob_account_name",
                secret_key_label_key: "azure_blob_account_key",
                access_key_trim_on_blur: true,
            },
            include_s3_path_style: false,
            include_s3_region: false,
            include_s3_timeouts: false,
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

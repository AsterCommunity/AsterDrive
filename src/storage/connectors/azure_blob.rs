use async_trait::async_trait;

use crate::api::api_error_code::ApiErrorCode;
use crate::entities::storage_policy;
use crate::errors::Result;
use crate::runtime::RemoteProtocolRuntimeState;
use crate::storage::StorageDriver;
use crate::storage::connector_descriptor::{
    StorageConnectorDescriptor, StorageConnectorDescriptorProvider,
    object_storage_connector_descriptor,
};
use crate::storage::drivers::azure_blob::{AzureBlobConfigError, AzureBlobDriver};
use crate::types::{DriverType, parse_storage_policy_options};

use super::common::validate_static_secret_credentials;
use super::{StorageConnector, StorageConnectorConnectionInput, StorageConnectorUploadTransport};

pub struct AzureBlobConnector;

impl StorageConnectorDescriptorProvider for AzureBlobConnector {
    fn storage_connector_descriptor() -> StorageConnectorDescriptor {
        object_storage_connector_descriptor(
            DriverType::AzureBlob,
            "Azure Blob Storage",
            "Azure Blob block blob storage policy",
            false,
            vec![328, 329],
        )
    }
}

#[async_trait(?Send)]
impl StorageConnector for AzureBlobConnector {
    fn driver_type() -> DriverType {
        DriverType::AzureBlob
    }

    fn normalize_connection_fields(endpoint: &str, bucket: &str) -> Result<(String, String)> {
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

    fn validate_connection_credentials(input: &StorageConnectorConnectionInput) -> Result<()> {
        validate_static_secret_credentials(input, "Azure Blob")
    }

    async fn build_draft_driver<S: RemoteProtocolRuntimeState + Sync + ?Sized>(
        state: &S,
        policy: &storage_policy::Model,
    ) -> Result<Box<dyn StorageDriver>> {
        let _ = state;
        Ok(Box::new(AzureBlobDriver::new(policy)?))
    }

    fn upload_transport(policy: &storage_policy::Model) -> StorageConnectorUploadTransport {
        let options = parse_storage_policy_options(policy.options.as_ref());
        StorageConnectorUploadTransport::ObjectStorage(options.effective_s3_upload_strategy())
    }

    fn presigned_download_enabled(policy: &storage_policy::Model) -> bool {
        let options = parse_storage_policy_options(policy.options.as_ref());
        options.effective_s3_download_strategy() == crate::types::S3DownloadStrategy::Presigned
    }
}

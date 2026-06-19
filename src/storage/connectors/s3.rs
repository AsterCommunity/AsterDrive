use async_trait::async_trait;

use crate::entities::storage_policy;
use crate::errors::Result;
use crate::runtime::RemoteProtocolRuntimeState;
use crate::storage::StorageDriver;
use crate::storage::connector_descriptor::{
    StorageConnectorDescriptor, StorageConnectorDescriptorProvider, endpoint_driver_recommendation,
    endpoint_host_rule, object_storage_connector_descriptor,
};
use crate::storage::drivers::s3::S3Driver;
use crate::types::{DriverType, parse_storage_policy_options};

use super::common::{normalize_s3_connection_fields, validate_static_secret_credentials};
use super::{StorageConnector, StorageConnectorConnectionInput, StorageConnectorUploadTransport};

pub struct S3Connector;

impl StorageConnectorDescriptorProvider for S3Connector {
    fn storage_connector_descriptor() -> StorageConnectorDescriptor {
        let mut descriptor = object_storage_connector_descriptor(
            DriverType::S3,
            "S3-compatible object storage",
            "S3-compatible object storage policy",
            false,
            vec![328, 329],
        );
        descriptor
            .driver_recommendations
            .push(endpoint_driver_recommendation(
                DriverType::TencentCos,
                vec![
                    endpoint_host_rule(Some("myqcloud.com"), None),
                    endpoint_host_rule(None, Some(".myqcloud.com")),
                ],
            ));
        descriptor
    }
}

#[async_trait(?Send)]
impl StorageConnector for S3Connector {
    fn driver_type() -> DriverType {
        DriverType::S3
    }

    fn normalize_connection_fields(endpoint: &str, bucket: &str) -> Result<(String, String)> {
        normalize_s3_connection_fields(endpoint, bucket)
    }

    fn validate_connection_credentials(input: &StorageConnectorConnectionInput) -> Result<()> {
        validate_static_secret_credentials(input, "S3-compatible")
    }

    async fn build_draft_driver<S: RemoteProtocolRuntimeState + Sync + ?Sized>(
        state: &S,
        policy: &storage_policy::Model,
    ) -> Result<Box<dyn StorageDriver>> {
        let _ = state;
        Ok(Box::new(S3Driver::new(policy)?))
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

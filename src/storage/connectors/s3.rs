use async_trait::async_trait;

use crate::entities::storage_policy;
use crate::errors::Result;
use crate::runtime::RemoteProtocolRuntimeState;
use crate::storage::StorageDriver;
use crate::storage::connector_descriptor::{
    ObjectStorageConnectorDescriptorInput, ObjectStorageFieldDescriptorInput,
    StorageConnectorDescriptor, StorageConnectorDescriptorProvider,
    StorageConnectorUiDescriptorInput, endpoint_driver_recommendation, endpoint_host_rule,
    object_storage_connector_descriptor,
};
use crate::storage::drivers::s3::S3Driver;
use crate::types::{DriverType, ObjectStorageDownloadStrategy, parse_storage_policy_options};

use super::common::{normalize_s3_connection_fields, validate_static_secret_credentials};
use super::{StorageConnector, StorageConnectorConnectionInput, StorageConnectorUploadTransport};

pub struct S3Connector;

impl StorageConnectorDescriptorProvider for S3Connector {
    fn storage_connector_descriptor() -> StorageConnectorDescriptor {
        let mut descriptor = object_storage_connector_descriptor(
            ObjectStorageConnectorDescriptorInput {
                driver_type: DriverType::S3,
                label: "S3-compatible object storage",
                description: "S3-compatible object storage policy",
                ui: StorageConnectorUiDescriptorInput {
                    label_key: "driver_type_s3",
                    description_key: "policy_wizard_s3_storage_desc",
                    icon_src: Some("/static/storage/amazon-s3.svg"),
                    icon_name: None,
                    helper_key: "policy_wizard_object_storage_helper",
                    config_step_title_key: "policy_wizard_step_connection_title",
                    config_step_description_key: "policy_wizard_step_object_storage_connection_desc",
                    edit_context_key: "policy_edit_context_object_storage_desc",
                    base_path_empty_display: "core:root",
                    base_path_placeholder: "tenant/prefix",
                },
                fields: ObjectStorageFieldDescriptorInput {
                    endpoint_placeholder: "https://s3.amazonaws.com",
                    endpoint_help_key: "s3_endpoint_hint",
                    endpoint_protocol_error_key: "s3_endpoint_protocol_required_error",
                    bucket_required_message_key: "policy_wizard_bucket_required",
                    access_key_label_key: "access_key",
                    secret_key_label_key: "secret_key",
                    access_key_trim_on_blur: false,
                },
                include_s3_path_style: true,
                presigned_part_etag_required: true,
                storage_native_processing: false,
                related_issues: vec![328, 329],
            },
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

    fn supports_saved_draft_credentials() -> bool {
        true
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
        StorageConnectorUploadTransport::ObjectStorage(
            options.effective_object_storage_upload_strategy(),
        )
    }

    fn presigned_download_enabled(policy: &storage_policy::Model) -> bool {
        let options = parse_storage_policy_options(policy.options.as_ref());
        options.effective_object_storage_download_strategy()
            == ObjectStorageDownloadStrategy::Presigned
    }
}

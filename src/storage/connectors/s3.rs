use async_trait::async_trait;

use crate::errors::Result;
use crate::storage::drivers::s3::{S3Driver, S3DriverOptions};
use aster_drive_model::entities::storage_policy;
use aster_drive_model::types::{ObjectStorageDownloadStrategy, parse_storage_policy_options};
use aster_drive_storage::StorageDriver;
use aster_drive_storage::connector_descriptor::{
    ObjectStorageConnectorDescriptorInput, ObjectStorageFieldDescriptorInput,
    StorageConnectorDeploymentScope, StorageConnectorDescriptor, StorageConnectorUiDescriptorInput,
    endpoint_driver_recommendation, endpoint_host_rule, object_storage_connector_descriptor,
};

use super::common::{normalize_s3_connection_fields, validate_static_secret_credentials};
use super::{
    StorageConnector, StorageConnectorConnectionInput, StorageConnectorUploadTransport,
    TencentCosConnector,
};

pub struct S3Connector;

impl S3Connector {
    pub const ID: &'static str = "asterdrive.storage.s3";
}

impl S3Connector {
    fn descriptor_definition() -> StorageConnectorDescriptor {
        let mut descriptor = object_storage_connector_descriptor(
            ObjectStorageConnectorDescriptorInput {
                connector_id: aster_drive_storage::ConnectorId::declared(Self::ID),
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
                deployment_scope: StorageConnectorDeploymentScope::SharedAcrossPrimaryInstances,
                supports_initial_setup: true,
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
                include_s3_region: true,
                include_s3_timeouts: true,
                presigned_part_etag_required: true,
                storage_native_processing: false,
                config_schema_version: 1,
                related_issues: vec![328, 329, 452],
            },
        );
        descriptor
            .driver_recommendations
            .push(endpoint_driver_recommendation(
                aster_drive_storage::ConnectorId::declared(TencentCosConnector::ID),
                vec![
                    endpoint_host_rule(Some("myqcloud.com"), None),
                    endpoint_host_rule(None, Some(".myqcloud.com")),
                ],
            ));
        descriptor
    }
}

#[async_trait]
impl StorageConnector for S3Connector {
    fn descriptor(&self) -> StorageConnectorDescriptor {
        Self::descriptor_definition()
    }

    fn normalize_connection_fields(
        &self,
        endpoint: &str,
        bucket: &str,
    ) -> Result<(String, String)> {
        normalize_s3_connection_fields(endpoint, bucket)
    }

    fn validate_connection_credentials(
        &self,
        input: &StorageConnectorConnectionInput,
    ) -> Result<()> {
        validate_static_secret_credentials(input, "S3-compatible")
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
        Ok(Box::new(S3Driver::new(
            policy,
            S3DriverOptions::default(),
            std::convert::identity,
        )?))
    }

    fn build_runtime_driver(
        &self,
        _registry: &crate::storage::DriverRegistry,
        policy: &storage_policy::Model,
    ) -> Result<super::StorageConnectorDriver> {
        Ok(super::StorageConnectorDriver::multipart(
            std::sync::Arc::new(S3Driver::new(
                policy,
                S3DriverOptions::default(),
                std::convert::identity,
            )?),
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

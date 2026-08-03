use async_trait::async_trait;

use crate::errors::Result;
use crate::storage::drivers::s3::{S3Driver, S3DriverOptions};
use aster_drive_model::entities::storage_policy;
use aster_drive_model::types::{
    ObjectStorageDownloadStrategy, ObjectStorageUploadStrategy, parse_storage_policy_options,
};
use aster_drive_storage::StorageDriver;
use aster_drive_storage::connector_descriptor::{
    ObjectStorageConnectorDescriptorInput, StorageConnectorDeploymentScope,
    StorageConnectorDescriptor, StorageConnectorFieldDisplayInput, StorageConnectorFieldKind,
    StorageConnectorFieldScope, StorageConnectorUiDescriptorInput, endpoint_driver_recommendation,
    endpoint_host_rule, object_storage_connector_descriptor, storage_connector_field,
    storage_connector_field_with_display, storage_connector_field_with_options,
};
use aster_drive_storage::{StorageConnectorConfigSchema, StorageConnectorFieldDefaultValue};

use super::common::{normalize_s3_connection_fields, validate_static_secret_credentials};
use super::{
    StorageConnector, StorageConnectorConnectionInput, StorageConnectorUploadTransport,
    TencentCosConnector,
};

pub struct S3Connector;

aster_drive_storage::storage_connector_schema! {
    pub struct S3ConnectorConfigV1 {
        config {
        pub endpoint: String => storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
            name: "endpoint",
            scope: StorageConnectorFieldScope::ConnectorConfig,
            kind: StorageConnectorFieldKind::Text,
            required: true,
            secret: false,
            label_key: "endpoint",
            placeholder: Some("https://s3.amazonaws.com"),
            help_key: Some("s3_endpoint_hint"),
            required_message_key: None,
            invalid_protocol_message_key: Some("s3_endpoint_protocol_required_error"),
            allowed_endpoint_protocols: vec!["http:", "https:"],
            allow_endpoint_without_protocol: false,
            trim_on_blur: false,
        }),
        pub bucket: String => storage_connector_field(
            "bucket", StorageConnectorFieldScope::ConnectorConfig,
            StorageConnectorFieldKind::Text, true, false,
        ),
        pub base_path: String => storage_connector_field(
            "base_path", StorageConnectorFieldScope::ConnectorConfig,
            StorageConnectorFieldKind::Text, false, false,
        ),
        pub object_storage_upload_strategy: ObjectStorageUploadStrategy => {
            let mut field = storage_connector_field_with_options(
                "object_storage_upload_strategy", StorageConnectorFieldScope::ConnectorConfig,
                StorageConnectorFieldKind::Select, true, false,
                vec!["relay_stream", "presigned"],
            );
            field.default_value = Some(StorageConnectorFieldDefaultValue::String("relay_stream".to_string()));
            field
        },
        pub object_storage_download_strategy: ObjectStorageDownloadStrategy => {
            let mut field = storage_connector_field_with_options(
                "object_storage_download_strategy", StorageConnectorFieldScope::ConnectorConfig,
                StorageConnectorFieldKind::Select, true, false,
                vec!["relay_stream", "presigned"],
            );
            field.default_value = Some(StorageConnectorFieldDefaultValue::String("relay_stream".to_string()));
            field
        },
        pub s3_path_style: bool => {
            let mut field = storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
                name: "s3_path_style", scope: StorageConnectorFieldScope::ConnectorConfig,
                kind: StorageConnectorFieldKind::Boolean, required: false, secret: false,
                label_key: "s3_path_style", placeholder: None, help_key: Some("s3_path_style_desc"),
                required_message_key: None, invalid_protocol_message_key: None,
                allowed_endpoint_protocols: Vec::new(), allow_endpoint_without_protocol: false,
                trim_on_blur: false,
            });
            field.default_value = Some(StorageConnectorFieldDefaultValue::Boolean(true));
            field
        },
        pub s3_region: String => {
            let mut field = storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
                name: "s3_region", scope: StorageConnectorFieldScope::ConnectorConfig,
                kind: StorageConnectorFieldKind::Text, required: false, secret: false,
                label_key: "s3_region", placeholder: Some("auto"), help_key: Some("s3_region_desc"),
                required_message_key: None, invalid_protocol_message_key: None,
                allowed_endpoint_protocols: Vec::new(), allow_endpoint_without_protocol: false,
                trim_on_blur: true,
            });
            field.default_value = Some(StorageConnectorFieldDefaultValue::String("auto".to_string()));
            field.validation.max_length = Some(128);
            field
        },
        pub s3_connect_timeout_secs: u64 => timeout_field("s3_connect_timeout_secs", 5),
        pub s3_read_timeout_secs: u64 => timeout_field("s3_read_timeout_secs", 30),
        pub s3_operation_timeout_secs: u64 => timeout_field("s3_operation_timeout_secs", 3_600),
        }
        credentials static {
            access_key => storage_connector_field(
                "access_key", StorageConnectorFieldScope::StaticCredential,
                StorageConnectorFieldKind::Text, true, false,
            ),
            secret_key => storage_connector_field(
                "secret_key", StorageConnectorFieldScope::StaticCredential,
                StorageConnectorFieldKind::Secret, true, true,
            ),
        }
    }
}

fn timeout_field(
    name: &str,
    default_value: i64,
) -> aster_drive_storage::StorageConnectorFieldDescriptor {
    let mut field = storage_connector_field(
        name,
        StorageConnectorFieldScope::ConnectorConfig,
        StorageConnectorFieldKind::Number,
        false,
        false,
    );
    field.default_value = Some(StorageConnectorFieldDefaultValue::Integer(default_value));
    field.validation.min_integer = Some(1);
    field
}

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
                credential_mode: S3ConnectorConfigV1::credential_mode(),
                fields: S3ConnectorConfigV1::descriptor_fields(),
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

    fn encode_config(
        &self,
        input: &StorageConnectorConnectionInput,
    ) -> Result<aster_drive_model::types::StoredConnectorConfig> {
        super::common::encode_typed_connector_config(
            Self::ID,
            1,
            S3ConnectorConfigV1 {
                endpoint: input.endpoint.clone(),
                bucket: input.bucket.clone(),
                base_path: input.base_path.clone(),
                object_storage_upload_strategy: input
                    .options
                    .effective_object_storage_upload_strategy(),
                object_storage_download_strategy: input
                    .options
                    .effective_object_storage_download_strategy(),
                s3_path_style: input.options.effective_s3_path_style(),
                s3_region: input.options.effective_s3_region().to_string(),
                s3_connect_timeout_secs: input.options.effective_s3_connect_timeout().as_secs(),
                s3_read_timeout_secs: input.options.effective_s3_read_timeout().as_secs(),
                s3_operation_timeout_secs: input.options.effective_s3_operation_timeout().as_secs(),
            },
        )
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

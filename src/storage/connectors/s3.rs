use async_trait::async_trait;
use std::time::Duration;

use crate::errors::{AsterError, Result};
use crate::storage::drivers::s3::{S3Driver, S3DriverConfig, S3DriverOptions, S3StaticCredentials};
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

mod localization;

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
        pub base_path: String => {
            let mut field = storage_connector_field(
                "base_path", StorageConnectorFieldScope::ConnectorConfig,
                StorageConnectorFieldKind::Text, false, false,
            );
            field.default_value = Some(StorageConnectorFieldDefaultValue::String(String::new()));
            field.default_mode = aster_drive_storage::StorageConnectorFieldDefaultMode::MissingOrEmptyText;
            field
        },
        pub object_storage_upload_strategy: ObjectStorageUploadStrategy => {
            transfer_strategy_field(
                "object_storage_upload_strategy",
                StorageTransferDirection::Upload,
            )
        },
        pub object_storage_download_strategy: ObjectStorageDownloadStrategy => {
            transfer_strategy_field(
                "object_storage_download_strategy",
                StorageTransferDirection::Download,
            )
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
        credentials static S3StaticCredentialsV1 {
            pub s3_access_key_id: String => storage_connector_field(
                "s3_access_key_id", StorageConnectorFieldScope::StaticCredential,
                StorageConnectorFieldKind::Text, true, false,
            ),
            pub s3_secret_access_key: String => storage_connector_field(
                "s3_secret_access_key", StorageConnectorFieldScope::StaticCredential,
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

    fn decode_config(policy: &storage_policy::Model) -> Result<S3ConnectorConfigV1> {
        super::common::decode_typed_policy_config(policy, Self::ID, 1)
            .map(|(config, _behavior)| config)
    }

    fn driver_config(config: S3ConnectorConfigV1) -> S3DriverConfig {
        S3DriverConfig {
            endpoint: config.endpoint,
            bucket: config.bucket,
            base_path: config.base_path,
            region: config.s3_region,
            path_style: config.s3_path_style,
            connect_timeout: Duration::from_secs(config.s3_connect_timeout_secs),
            read_timeout: Duration::from_secs(config.s3_read_timeout_secs),
            operation_timeout: Duration::from_secs(config.s3_operation_timeout_secs),
        }
    }

    fn driver_credentials(credentials: S3StaticCredentialsV1) -> S3StaticCredentials {
        S3StaticCredentials {
            access_key: credentials.s3_access_key_id,
            secret_key: credentials.s3_secret_access_key,
        }
    }
}

impl S3Connector {
    fn descriptor_definition() -> StorageConnectorDescriptor {
        object_storage_connector_descriptor(ObjectStorageConnectorDescriptorInput {
            connector_id: aster_drive_storage::ConnectorId::declared(Self::ID),
            label: "S3-compatible object storage",
            description: "S3-compatible object storage policy",
            ui: StorageConnectorUiDescriptorInput {
                label_key: "driver_type_s3",
                description_key: "policy_wizard_s3_storage_desc",
                icon_src: Some("/static/storage/amazon-s3.svg"),
                icon_name: None,
                badge_rgb: StorageConnectorBadgeRgb::new(59, 130, 246),
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
            credential_schema_version: Some(1),
            related_issues: vec![328, 329, 452],
        })
    }
}

#[async_trait]
impl StorageConnector for S3Connector {
    fn descriptor(&self) -> StorageConnectorDescriptor {
        Self::descriptor_definition()
    }

    fn localization(&self) -> Result<aster_drive_storage::StorageConnectorLocalization> {
        let descriptor = Self::descriptor_definition();
        super::localization::builtin_connector_localization(
            Self::ID,
            &descriptor,
            localization::MESSAGES,
        )
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
        let mut config: S3ConnectorConfigV1 =
            super::common::decode_normalized_connector_config(&normalized)?;
        let connection = crate::storage::drivers::s3_config::normalize_s3_endpoint_and_bucket(
            &config.endpoint,
            &config.bucket,
        )
        .map_err(|error| error.into_aster_error())?;
        config.endpoint = connection.endpoint;
        config.bucket = connection.bucket;
        crate::storage::drivers::s3_config::validate_sigv4_region(&config.s3_region).map_err(
            |_| {
                AsterError::validation_error(
                    "s3_region must be 1-128 printable ASCII characters without whitespace or '/'",
                )
            },
        )?;
        super::common::encode_normalized_connector_config(
            normalized.connector_id,
            normalized.schema_version,
            config,
        )
    }

    fn validate_credential_input(&self, input: &StorageConnectorCredentialInput) -> Result<()> {
        let credential: S3StaticCredentialsV1 =
            super::common::decode_static_credential(input, Self::ID)?;
        super::common::validate_required_credential_field(
            &credential.s3_access_key_id,
            "s3_access_key_id",
            Self::ID,
        )?;
        super::common::validate_required_credential_field(
            &credential.s3_secret_access_key,
            "s3_secret_access_key",
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
            S3StaticCredentialsV1 {
                s3_access_key_id: legacy.access_key,
                s3_secret_access_key: legacy.secret_key,
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
        Ok(Box::new(S3Driver::new(
            Self::driver_config(config),
            Self::driver_credentials(credentials),
            S3DriverOptions::default(),
            std::convert::identity,
        )?))
    }

    fn build_runtime_driver(
        &self,
        registry: &crate::storage::DriverRegistry,
        policy: &storage_policy::Model,
    ) -> Result<super::StorageConnectorDriver> {
        let config = Self::decode_config(policy)?;
        let credentials: S3StaticCredentialsV1 =
            super::common::runtime_static_credential(registry, policy, Self::ID)?;
        Ok(super::StorageConnectorDriver::multipart(
            std::sync::Arc::new(S3Driver::new(
                Self::driver_config(config),
                Self::driver_credentials(credentials),
                S3DriverOptions::default(),
                std::convert::identity,
            )?),
        ))
    }

    async fn build_cleanup_driver(
        &self,
        context: &super::StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
        snapshots: super::StoragePolicyCleanupSnapshots<'_>,
    ) -> Result<std::sync::Arc<dyn StorageDriver>> {
        let config = Self::decode_config(policy)?;
        let credentials: S3StaticCredentialsV1 =
            super::common::static_credential_from_cleanup_snapshot(
                context,
                policy,
                snapshots,
                Self::ID,
                1,
            )?;
        Ok(std::sync::Arc::new(S3Driver::new(
            Self::driver_config(config),
            Self::driver_credentials(credentials),
            S3DriverOptions::default(),
            std::convert::identity,
        )?))
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

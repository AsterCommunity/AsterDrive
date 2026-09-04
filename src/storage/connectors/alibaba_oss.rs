use async_trait::async_trait;
use std::time::Duration;

use crate::errors::{AsterError, Result};
use crate::storage::drivers::alibaba_oss::{
    AlibabaOssDriver, AlibabaOssDriverConfig, AlibabaOssStaticCredentials,
};
use aster_drive_model::entities::storage_policy;
use aster_drive_model::types::{ObjectStorageDownloadStrategy, ObjectStorageUploadStrategy};
use aster_drive_storage::StorageDriver;
use aster_drive_storage::connector_descriptor::{
    ObjectStorageConnectorDescriptorInput, StorageConnectorBadgeRgb,
    StorageConnectorDeploymentScope, StorageConnectorDescriptor, StorageConnectorFieldDisplayInput,
    StorageConnectorFieldKind, StorageConnectorFieldScope, StorageConnectorPromotionDescriptor,
    StorageConnectorPromotionRequirement, StorageConnectorPromotionValueMatcher,
    StorageConnectorUiDescriptorInput, object_storage_connector_descriptor,
    storage_connector_field, storage_connector_field_with_display,
};
use aster_drive_storage::{StorageConnectorConfigSchema, StorageConnectorFieldDefaultValue};

use super::common::{StorageTransferDirection, transfer_strategy_field};
use super::{StorageConnector, StorageConnectorCredentialInput, StorageConnectorUploadTransport};

mod localization;

pub struct AlibabaOssConnector;

const PROMOTE_FROM_S3_ID: &str = "promote_from_s3";

fn promote_from_s3_descriptor() -> StorageConnectorPromotionDescriptor {
    super::s3::s3_compatible_promotion_descriptor(super::s3::S3CompatiblePromotionDescriptorInput {
        promotion_id: PROMOTE_FROM_S3_ID,
        description_key: "policy_oss_promote_from_s3_desc",
        confirmation_key: "policy_oss_promote_from_s3_confirm",
        requirements: vec![
            StorageConnectorPromotionRequirement {
                source_field: "endpoint".to_string(),
                matcher: StorageConnectorPromotionValueMatcher::StringPrefix {
                    prefix: "https://".to_string(),
                    case_sensitive: false,
                },
                negate: false,
            },
            StorageConnectorPromotionRequirement {
                source_field: "endpoint".to_string(),
                matcher: StorageConnectorPromotionValueMatcher::UrlHostSuffix {
                    suffix: ".aliyuncs.com".to_string(),
                },
                negate: false,
            },
            StorageConnectorPromotionRequirement {
                source_field: "endpoint".to_string(),
                matcher: StorageConnectorPromotionValueMatcher::UrlHostSuffix {
                    suffix: "-internal.aliyuncs.com".to_string(),
                },
                negate: true,
            },
            StorageConnectorPromotionRequirement {
                source_field: "s3_region".to_string(),
                matcher: StorageConnectorPromotionValueMatcher::StringEquals {
                    value: "auto".to_string(),
                    case_sensitive: false,
                },
                negate: true,
            },
        ],
        target_region_field: Some("oss_region"),
        target_access_key_field: "aliyun_oss_access_key_id",
        target_secret_key_field: "aliyun_oss_access_key_secret",
    })
}

aster_drive_storage::storage_connector_schema! {
    pub struct AlibabaOssConnectorConfigV1 {
        config {
        pub endpoint: String => storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
            name: "endpoint", scope: StorageConnectorFieldScope::ConnectorConfig,
            kind: StorageConnectorFieldKind::Text, required: true, secret: false,
            label_key: "oss_public_endpoint", placeholder: Some("https://oss-cn-hangzhou.aliyuncs.com"),
            help_key: Some("oss_public_endpoint_desc"), required_message_key: None,
            invalid_protocol_message_key: Some("s3_endpoint_protocol_required_error"),
            allowed_endpoint_protocols: vec!["http:", "https:"],
            allow_endpoint_without_protocol: false, trim_on_blur: false,
        }),
        pub oss_server_side_endpoint: String => {
            let mut field = storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
                name: "oss_server_side_endpoint", scope: StorageConnectorFieldScope::ConnectorConfig,
                kind: StorageConnectorFieldKind::Text, required: false, secret: false,
                label_key: "oss_server_side_endpoint", placeholder: Some("https://oss-cn-hangzhou-internal.aliyuncs.com"),
                help_key: Some("oss_server_side_endpoint_desc"), required_message_key: None,
                invalid_protocol_message_key: Some("s3_endpoint_protocol_required_error"),
                allowed_endpoint_protocols: vec!["http:", "https:"],
                allow_endpoint_without_protocol: false, trim_on_blur: false,
            });
            field.default_value = Some(StorageConnectorFieldDefaultValue::String(String::new()));
            field.default_mode = aster_drive_storage::StorageConnectorFieldDefaultMode::MissingOrEmptyText;
            field
        },
        pub oss_region: String => {
            let mut field = storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
                name: "oss_region", scope: StorageConnectorFieldScope::ConnectorConfig,
                kind: StorageConnectorFieldKind::Text, required: true, secret: false,
                label_key: "oss_region", placeholder: Some("cn-hangzhou"),
                help_key: Some("oss_region_desc"), required_message_key: None,
                invalid_protocol_message_key: None, allowed_endpoint_protocols: Vec::new(),
                allow_endpoint_without_protocol: false, trim_on_blur: true,
            });
            field.validation.max_length = Some(128);
            field
        },
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
        pub oss_use_cname: bool => {
            let mut field = storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
                name: "oss_use_cname", scope: StorageConnectorFieldScope::ConnectorConfig,
                kind: StorageConnectorFieldKind::Boolean, required: false, secret: false,
                label_key: "oss_use_cname", placeholder: None, help_key: Some("oss_use_cname_desc"),
                required_message_key: None, invalid_protocol_message_key: None,
                allowed_endpoint_protocols: Vec::new(), allow_endpoint_without_protocol: false,
                trim_on_blur: false,
            });
            field.default_value = Some(StorageConnectorFieldDefaultValue::Boolean(false));
            field
        },
        pub object_storage_upload_strategy: ObjectStorageUploadStrategy => transfer_strategy_field(
            "object_storage_upload_strategy", StorageTransferDirection::Upload,
        ),
        pub object_storage_download_strategy: ObjectStorageDownloadStrategy => transfer_strategy_field(
            "object_storage_download_strategy", StorageTransferDirection::Download,
        ),
        }
        credentials static AlibabaOssStaticCredentialsV1 {
            pub aliyun_oss_access_key_id: String => storage_connector_field(
                "aliyun_oss_access_key_id", StorageConnectorFieldScope::StaticCredential,
                StorageConnectorFieldKind::Text, true, false,
            ),
            pub aliyun_oss_access_key_secret: String => storage_connector_field(
                "aliyun_oss_access_key_secret", StorageConnectorFieldScope::StaticCredential,
                StorageConnectorFieldKind::Secret, true, true,
            ),
        }
    }
}

impl AlibabaOssConnector {
    pub const ID: &'static str = "asterdrive.storage.alibaba_oss";

    fn decode_config(policy: &storage_policy::Model) -> Result<AlibabaOssConnectorConfigV1> {
        super::common::decode_typed_policy_config(policy, Self::ID, 1).map(|(config, _)| config)
    }

    fn driver_config(config: AlibabaOssConnectorConfigV1) -> AlibabaOssDriverConfig {
        AlibabaOssDriverConfig {
            endpoint: config.endpoint,
            server_side_endpoint: config.oss_server_side_endpoint,
            region: config.oss_region,
            bucket: config.bucket,
            base_path: config.base_path,
            use_cname: config.oss_use_cname,
            connect_timeout: Duration::from_secs(5),
            read_timeout: Duration::from_secs(30),
            operation_timeout: Duration::from_secs(3_600),
        }
    }

    fn driver_credentials(
        credentials: AlibabaOssStaticCredentialsV1,
    ) -> AlibabaOssStaticCredentials {
        AlibabaOssStaticCredentials {
            access_key: credentials.aliyun_oss_access_key_id,
            secret_key: credentials.aliyun_oss_access_key_secret,
        }
    }

    fn descriptor_definition() -> StorageConnectorDescriptor {
        let mut descriptor =
            object_storage_connector_descriptor(ObjectStorageConnectorDescriptorInput {
                connector_id: aster_drive_storage::ConnectorId::declared(Self::ID),
                label: "Alibaba Cloud OSS",
                description: "Alibaba Cloud Object Storage Service policy",
                ui: StorageConnectorUiDescriptorInput {
                    label_key: "driver_type_alibaba_oss",
                    description_key: "policy_wizard_alibaba_oss_storage_desc",
                    icon_src: Some("/static/storage/aliyun-oss.svg"),
                    icon_name: None,
                    badge_rgb: StorageConnectorBadgeRgb::new(255, 106, 0),
                    helper_key: "policy_wizard_alibaba_oss_helper",
                    config_step_title_key: "policy_wizard_step_connection_title",
                    config_step_description_key: "policy_wizard_step_alibaba_oss_connection_desc",
                    edit_context_key: "policy_edit_context_object_storage_desc",
                    base_path_empty_display: "core:root",
                    base_path_placeholder: "tenant/prefix",
                },
                deployment_scope: StorageConnectorDeploymentScope::SharedAcrossPrimaryInstances,
                supports_initial_setup: true,
                credential_mode: AlibabaOssConnectorConfigV1::credential_mode(),
                fields: AlibabaOssConnectorConfigV1::descriptor_fields(),
                presigned_part_etag_required: true,
                storage_native_processing: false,
                config_schema_version: 1,
                credential_schema_version: Some(1),
                related_issues: vec![450, 474],
            });
        descriptor.promotions.push(promote_from_s3_descriptor());
        descriptor
    }
}

#[async_trait]
impl StorageConnector for AlibabaOssConnector {
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
        let config: AlibabaOssConnectorConfigV1 =
            super::common::decode_normalized_connector_config(&normalized)?;
        let driver_config = Self::driver_config(config.clone());
        AlibabaOssDriver::validate_connection_config(&driver_config)
            .map_err(|error| AsterError::validation_error(error.message().to_string()))?;
        super::common::encode_normalized_connector_config(
            normalized.connector_id,
            normalized.schema_version,
            config,
        )
    }

    fn validate_credential_input(&self, input: &StorageConnectorCredentialInput) -> Result<()> {
        let credential: AlibabaOssStaticCredentialsV1 =
            super::common::decode_static_credential(input, Self::ID)?;
        super::common::validate_required_credential_field(
            &credential.aliyun_oss_access_key_id,
            "aliyun_oss_access_key_id",
            Self::ID,
        )?;
        super::common::validate_required_credential_field(
            &credential.aliyun_oss_access_key_secret,
            "aliyun_oss_access_key_secret",
            Self::ID,
        )
    }

    async fn build_driver_from_connection(
        &self,
        context: &super::StorageConnectorContext<'_>,
        connector_config: &aster_drive_storage::ConnectorConfigEnvelope,
        credential: &StorageConnectorCredentialInput,
    ) -> Result<Box<dyn StorageDriver>> {
        let _ = context;
        let config = super::common::decode_normalized_connector_config(connector_config)?;
        let credentials = super::common::decode_static_credential(credential, Self::ID)?;
        Ok(Box::new(AlibabaOssDriver::new(
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
        let credentials: AlibabaOssStaticCredentialsV1 =
            super::common::runtime_static_credential(registry, policy, Self::ID)?;
        Ok(super::StorageConnectorDriver::multipart(
            std::sync::Arc::new(AlibabaOssDriver::new(
                Self::driver_config(config),
                Self::driver_credentials(credentials),
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
        let credentials: AlibabaOssStaticCredentialsV1 =
            super::common::static_credential_from_cleanup_snapshot(
                context,
                policy,
                snapshots,
                Self::ID,
                1,
            )?;
        Ok(std::sync::Arc::new(AlibabaOssDriver::new(
            Self::driver_config(config),
            Self::driver_credentials(credentials),
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

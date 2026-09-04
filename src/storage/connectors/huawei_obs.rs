use async_trait::async_trait;
use std::time::Duration;

use crate::errors::{AsterError, Result};
use crate::storage::drivers::huawei_obs::{
    HuaweiObsAddressingMode, HuaweiObsDriver, HuaweiObsDriverConfig, HuaweiObsStaticCredentials,
};
use aster_drive_model::entities::storage_policy;
use aster_drive_model::types::{ObjectStorageDownloadStrategy, ObjectStorageUploadStrategy};
use aster_drive_storage::StorageDriver;
use aster_drive_storage::connector_descriptor::{
    ObjectStorageConnectorDescriptorInput, StorageConnectorBadgeRgb,
    StorageConnectorDeploymentScope, StorageConnectorDescriptor, StorageConnectorFieldDisplayInput,
    StorageConnectorFieldKind, StorageConnectorFieldScope, StorageConnectorPromotionDescriptor,
    StorageConnectorPromotionRequirement, StorageConnectorPromotionValueMatcher,
    StorageConnectorSelectOptionInput, StorageConnectorUiDescriptorInput,
    object_storage_connector_descriptor, storage_connector_field,
    storage_connector_field_with_display, storage_connector_select_field,
};
use aster_drive_storage::{StorageConnectorConfigSchema, StorageConnectorFieldDefaultValue};

use super::common::{StorageTransferDirection, transfer_strategy_field};
use super::{StorageConnector, StorageConnectorCredentialInput, StorageConnectorUploadTransport};

mod localization;

pub struct HuaweiObsConnector;

const PROMOTE_FROM_S3_ID: &str = "promote_from_s3";

fn promote_from_s3_descriptor() -> StorageConnectorPromotionDescriptor {
    super::s3::s3_compatible_promotion_descriptor(super::s3::S3CompatiblePromotionDescriptorInput {
        promotion_id: PROMOTE_FROM_S3_ID,
        description_key: "policy_obs_promote_from_s3_desc",
        confirmation_key: "policy_obs_promote_from_s3_confirm",
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
                matcher: StorageConnectorPromotionValueMatcher::UrlHostContainsField {
                    field: "s3_region".to_string(),
                },
                negate: false,
            },
            StorageConnectorPromotionRequirement {
                source_field: "endpoint".to_string(),
                matcher: StorageConnectorPromotionValueMatcher::UrlHostContainsLabel {
                    label: "obs".to_string(),
                },
                negate: false,
            },
            StorageConnectorPromotionRequirement {
                source_field: "endpoint".to_string(),
                matcher: StorageConnectorPromotionValueMatcher::UrlHostSuffixAny {
                    suffixes: vec![
                        ".myhuaweicloud.com".to_string(),
                        ".myhuaweicloud.eu".to_string(),
                    ],
                },
                negate: false,
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
        target_region_field: Some("obs_region"),
        target_access_key_field: "obs_access_key_id",
        target_secret_key_field: "obs_secret_access_key",
    })
}

aster_drive_storage::storage_connector_schema! {
    pub struct HuaweiObsConnectorConfigV1 {
        config {
        pub endpoint: String => storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
            name: "endpoint",
            scope: StorageConnectorFieldScope::ConnectorConfig,
            kind: StorageConnectorFieldKind::Text,
            required: true,
            secret: false,
            label_key: "endpoint",
            placeholder: Some("https://obs.cn-north-4.myhuaweicloud.com"),
            help_key: Some("huawei_obs_endpoint_hint"),
            required_message_key: None,
            invalid_protocol_message_key: Some("s3_endpoint_protocol_required_error"),
            allowed_endpoint_protocols: vec!["http:", "https:"],
            allow_endpoint_without_protocol: false,
            trim_on_blur: false,
        }),
        pub bucket: String => {
            let mut field = storage_connector_field(
                "bucket", StorageConnectorFieldScope::ConnectorConfig,
                StorageConnectorFieldKind::Text, true, false,
            );
            field.required_message_key = Some("policy_wizard_bucket_required".to_string());
            field
        },
        #[serde(default)]
        pub obs_region: String => {
            let mut field = storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
                name: "obs_region", scope: StorageConnectorFieldScope::ConnectorConfig,
                kind: StorageConnectorFieldKind::Text, required: false, secret: false,
                label_key: "obs_region", placeholder: Some("cn-north-4"),
                help_key: Some("obs_region_desc"), required_message_key: None,
                invalid_protocol_message_key: None, allowed_endpoint_protocols: Vec::new(),
                allow_endpoint_without_protocol: false, trim_on_blur: true,
            });
            field.default_value = Some(StorageConnectorFieldDefaultValue::String(String::new()));
            field.default_mode = aster_drive_storage::StorageConnectorFieldDefaultMode::MissingOnly;
            field.validation.max_length = Some(128);
            field
        },
        pub obs_addressing_mode: HuaweiObsAddressingMode => obs_select_field(
            "obs_addressing_mode",
            vec![
                select_option(
                    "virtual_hosted",
                    "obs_addressing_mode_virtual_hosted",
                    Some("obs_addressing_mode_virtual_hosted_desc"),
                ),
                select_option(
                    "custom_domain",
                    "obs_addressing_mode_custom_domain",
                    Some("obs_addressing_mode_custom_domain_desc"),
                ),
            ],
            "virtual_hosted",
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
        pub object_storage_upload_strategy: ObjectStorageUploadStrategy => transfer_strategy_field(
            "object_storage_upload_strategy", StorageTransferDirection::Upload,
        ),
        pub object_storage_download_strategy: ObjectStorageDownloadStrategy => transfer_strategy_field(
            "object_storage_download_strategy", StorageTransferDirection::Download,
        ),
        }
        credentials static HuaweiObsStaticCredentialsV1 {
            pub obs_access_key_id: String => storage_connector_field(
                "obs_access_key_id", StorageConnectorFieldScope::StaticCredential,
                StorageConnectorFieldKind::Text, true, false,
            ),
            pub obs_secret_access_key: String => storage_connector_field(
                "obs_secret_access_key", StorageConnectorFieldScope::StaticCredential,
                StorageConnectorFieldKind::Secret, true, true,
            ),
        }
    }
}

fn obs_select_field(
    name: &str,
    options: Vec<StorageConnectorSelectOptionInput<'static>>,
    default_value: &str,
) -> aster_drive_storage::StorageConnectorFieldDescriptor {
    let mut field = storage_connector_select_field(
        name,
        StorageConnectorFieldScope::ConnectorConfig,
        true,
        options,
    );
    field.default_value = Some(StorageConnectorFieldDefaultValue::String(
        default_value.to_string(),
    ));
    field
}

const fn select_option(
    value: &'static str,
    label_key: &'static str,
    description_key: Option<&'static str>,
) -> StorageConnectorSelectOptionInput<'static> {
    StorageConnectorSelectOptionInput {
        value,
        label_key,
        description_key,
    }
}

impl HuaweiObsConnector {
    pub const ID: &'static str = "asterdrive.storage.huawei_obs";

    fn decode_config(policy: &storage_policy::Model) -> Result<HuaweiObsConnectorConfigV1> {
        super::common::decode_typed_policy_config(policy, Self::ID, 1).map(|(config, _)| config)
    }

    fn driver_config(config: HuaweiObsConnectorConfigV1) -> HuaweiObsDriverConfig {
        HuaweiObsDriverConfig {
            endpoint: config.endpoint,
            bucket: config.bucket,
            base_path: config.base_path,
            region: config.obs_region,
            addressing_mode: config.obs_addressing_mode,
            connect_timeout: Duration::from_secs(5),
            read_timeout: Duration::from_secs(30),
            operation_timeout: Duration::from_secs(3_600),
        }
    }

    fn driver_credentials(credentials: HuaweiObsStaticCredentialsV1) -> HuaweiObsStaticCredentials {
        HuaweiObsStaticCredentials {
            access_key: credentials.obs_access_key_id,
            secret_key: credentials.obs_secret_access_key,
        }
    }

    fn runtime_driver(
        registry: &crate::storage::DriverRegistry,
        policy: &storage_policy::Model,
    ) -> Result<HuaweiObsDriver> {
        let config = Self::decode_config(policy)?;
        let credentials: HuaweiObsStaticCredentialsV1 =
            super::common::runtime_static_credential(registry, policy, Self::ID)?;
        HuaweiObsDriver::new(
            Self::driver_config(config),
            Self::driver_credentials(credentials),
        )
        .map_err(Into::into)
    }

    fn descriptor_definition() -> StorageConnectorDescriptor {
        let mut descriptor =
            object_storage_connector_descriptor(ObjectStorageConnectorDescriptorInput {
                connector_id: aster_drive_storage::ConnectorId::declared(Self::ID),
                label: "Huawei Cloud OBS",
                description: "Huawei Cloud OBS object storage policy with native OBS signatures",
                ui: StorageConnectorUiDescriptorInput {
                    label_key: "driver_type_huawei_obs",
                    description_key: "policy_wizard_huawei_obs_storage_desc",
                    icon_src: Some("/static/storage/huaweicloud-obs.webp"),
                    icon_name: None,
                    badge_rgb: StorageConnectorBadgeRgb::new(239, 68, 68),
                    helper_key: "policy_wizard_huawei_obs_helper",
                    config_step_title_key: "policy_wizard_step_connection_title",
                    config_step_description_key: "policy_wizard_step_huawei_obs_connection_desc",
                    edit_context_key: "policy_edit_context_object_storage_desc",
                    base_path_empty_display: "core:root",
                    base_path_placeholder: "tenant/prefix",
                },
                deployment_scope: StorageConnectorDeploymentScope::SharedAcrossPrimaryInstances,
                supports_initial_setup: true,
                credential_mode: HuaweiObsConnectorConfigV1::credential_mode(),
                fields: HuaweiObsConnectorConfigV1::descriptor_fields(),
                presigned_part_etag_required: true,
                storage_native_processing: false,
                config_schema_version: 1,
                credential_schema_version: Some(1),
                related_issues: vec![451],
            });
        descriptor.promotions.push(promote_from_s3_descriptor());
        descriptor
    }
}

#[async_trait]
impl StorageConnector for HuaweiObsConnector {
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
        let mut config: HuaweiObsConnectorConfigV1 =
            super::common::decode_normalized_connector_config(&normalized)?;
        let endpoint = HuaweiObsDriver::normalize_endpoint(
            &config.endpoint,
            &config.bucket,
            &config.obs_region,
            config.obs_addressing_mode,
        )
        .map_err(AsterError::from)?;
        config.endpoint = endpoint.endpoint;
        config.bucket = endpoint.bucket;
        config.obs_region = endpoint.region;
        super::common::encode_normalized_connector_config(
            normalized.connector_id,
            normalized.schema_version,
            config,
        )
    }

    fn validate_credential_input(&self, input: &StorageConnectorCredentialInput) -> Result<()> {
        let credential: HuaweiObsStaticCredentialsV1 =
            super::common::decode_static_credential(input, Self::ID)?;
        super::common::validate_required_credential_field(
            &credential.obs_access_key_id,
            "obs_access_key_id",
            Self::ID,
        )?;
        super::common::validate_required_credential_field(
            &credential.obs_secret_access_key,
            "obs_secret_access_key",
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
        Ok(Box::new(HuaweiObsDriver::new(
            Self::driver_config(config),
            Self::driver_credentials(credentials),
        )?))
    }

    fn build_runtime_driver(
        &self,
        registry: &crate::storage::DriverRegistry,
        policy: &storage_policy::Model,
    ) -> Result<super::StorageConnectorDriver> {
        Ok(super::StorageConnectorDriver::multipart(
            std::sync::Arc::new(Self::runtime_driver(registry, policy)?),
        ))
    }

    async fn build_cleanup_driver(
        &self,
        context: &super::StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
        snapshots: super::StoragePolicyCleanupSnapshots<'_>,
    ) -> Result<std::sync::Arc<dyn StorageDriver>> {
        let config = Self::decode_config(policy)?;
        let credentials: HuaweiObsStaticCredentialsV1 =
            super::common::static_credential_from_cleanup_snapshot(
                context,
                policy,
                snapshots,
                Self::ID,
                1,
            )?;
        Ok(std::sync::Arc::new(HuaweiObsDriver::new(
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

use async_trait::async_trait;
use serde::Serialize;
use std::time::Duration;

use crate::api::api_error_code::ApiErrorCode;
use crate::config::site_url;
use crate::errors::{AsterError, Result, validation_error_with_code};
use crate::storage::drivers::alibaba_oss::{
    AlibabaOssDriver, AlibabaOssDriverConfig, AlibabaOssStaticCredentials,
    cdn::{OssDownloadDelivery, default_cdn_auth_validity_seconds},
};
use aster_drive_model::entities::storage_policy;
use aster_drive_model::types::{ObjectStorageDownloadStrategy, ObjectStorageUploadStrategy};
use aster_drive_storage::StorageDriver;
use aster_drive_storage::connector_descriptor::{
    ObjectStorageConnectorDescriptorInput, StorageConnectorActionId,
    StorageConnectorActionOutputValueKind, StorageConnectorBadgeRgb,
    StorageConnectorCustomActionDescriptorInput, StorageConnectorDeploymentScope,
    StorageConnectorDescriptor, StorageConnectorFieldDisplayInput, StorageConnectorFieldKind,
    StorageConnectorFieldScope, StorageConnectorPromotionDescriptor,
    StorageConnectorPromotionRequirement, StorageConnectorPromotionValueMatcher,
    StorageConnectorUiDescriptorInput, custom_action_descriptor,
    object_storage_connector_descriptor, storage_connector_field,
    storage_connector_field_with_display,
};
use aster_drive_storage::{
    StorageConnectorActionSchema, StorageConnectorConfigSchema, StorageConnectorFieldDefaultValue,
};

use super::common::{
    StorageTransferDirection, action_output_field, boolean_action_output_field,
    decode_normalized_connector_action_input, transfer_strategy_field,
    unsupported_connector_action_error,
};
use super::{
    ExecuteDraftStorageConnectorActionInput, ExecuteSavedStorageConnectorActionInput,
    StorageConnector, StorageConnectorActionResult, StorageConnectorCredentialInput,
    StorageConnectorIcon, StorageConnectorUploadTransport,
};

mod localization;

pub struct AlibabaOssConnector;

const PROMOTE_FROM_S3_ID: &str = "promote_from_s3";
const CONFIGURE_CORS_ACTION_ID: &str = "configure_alibaba_oss_cors";

aster_drive_storage::storage_connector_action_schema! {
    struct ConfigureAlibabaOssCorsActionInput {}
}

fn configure_cors_action_descriptor() -> aster_drive_storage::StorageConnectorActionDescriptor {
    custom_action_descriptor(StorageConnectorCustomActionDescriptorInput {
        action_id: StorageConnectorActionId::declared(CONFIGURE_CORS_ACTION_ID),
        label_key: "policy_oss_cors_action",
        description_key: "policy_oss_cors_desc",
        fields: ConfigureAlibabaOssCorsActionInput::action_fields(),
        output_fields: vec![
            action_output_field(
                "bucket",
                "policy_oss_cors_output_bucket",
                StorageConnectorActionOutputValueKind::Text,
            ),
            action_output_field(
                "origins",
                "policy_oss_cors_output_origins",
                StorageConnectorActionOutputValueKind::StringList,
            ),
            action_output_field(
                "preserved_rule_count",
                "policy_oss_cors_output_preserved_rules",
                StorageConnectorActionOutputValueKind::Number,
            ),
            action_output_field(
                "request_id",
                "policy_oss_cors_output_request_id",
                StorageConnectorActionOutputValueKind::Text,
            ),
            boolean_action_output_field(
                "changed",
                "policy_oss_cors_output_changed",
                "policy_oss_cors_output_yes",
                "policy_oss_cors_output_no",
            ),
        ],
        supports_draft: true,
        supports_saved: true,
        requires_authorization: false,
        mutates_remote_state: true,
        requires_confirmation: true,
    })
}

#[derive(Serialize)]
struct AlibabaOssCorsActionOutput {
    bucket: String,
    origins: Vec<String>,
    changed: bool,
    preserved_rule_count: usize,
    request_id: Option<String>,
}

impl From<crate::storage::drivers::alibaba_oss::cors::OssCorsApplyResult>
    for AlibabaOssCorsActionOutput
{
    fn from(value: crate::storage::drivers::alibaba_oss::cors::OssCorsApplyResult) -> Self {
        Self {
            bucket: value.bucket,
            origins: value.origins,
            changed: value.changed,
            preserved_rule_count: value.preserved_rule_count,
            request_id: value.request_id,
        }
    }
}

fn cors_origins(runtime_config: &crate::config::RuntimeConfig) -> Result<Vec<String>> {
    let raw = site_url::public_site_url_config_value(runtime_config).ok_or_else(|| {
        validation_error_with_code(
            ApiErrorCode::PolicyActionParameterRequired,
            "public_site_url must be configured before configuring OSS CORS",
        )
    })?;
    let origins = site_url::parse_public_site_url_value(&raw)?;
    if origins.is_empty() {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyActionParameterRequired,
            "public_site_url must contain at least one origin",
        ));
    }
    Ok(origins)
}

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
        #[serde(default)]
        pub download_delivery: OssDownloadDelivery => {
            let mut field = aster_drive_storage::storage_connector_select_field(
                "download_delivery", StorageConnectorFieldScope::ConnectorConfig, true,
                vec![
                    aster_drive_storage::StorageConnectorSelectOptionInput { value: "oss_origin", label_key: "oss_download_origin", description_key: None },
                    aster_drive_storage::StorageConnectorSelectOptionInput { value: "alibaba_cdn_type_a", label_key: "oss_download_cdn", description_key: None },
                ],
            );
            field.label_key = "oss_download_delivery".into();
            field.help_key = Some("oss_download_delivery_desc".into());
            field.default_value = Some(StorageConnectorFieldDefaultValue::String("oss_origin".into()));
            field
        },
        pub object_storage_upload_strategy: ObjectStorageUploadStrategy => transfer_strategy_field(
            "object_storage_upload_strategy", StorageTransferDirection::Upload,
        ),
        pub object_storage_download_strategy: ObjectStorageDownloadStrategy => transfer_strategy_field(
            "object_storage_download_strategy", StorageTransferDirection::Download,
        ),
        #[serde(default)]
        pub download_base_url: String => {
            let mut field = storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
                name: "download_base_url", scope: StorageConnectorFieldScope::ConnectorConfig,
                kind: StorageConnectorFieldKind::Text, required: false, secret: false,
                label_key: "download_base_url", placeholder: Some("https://cdn.example.com"),
                help_key: Some("oss_download_base_url_desc"), required_message_key: None,
                invalid_protocol_message_key: Some("s3_endpoint_protocol_required_error"),
                allowed_endpoint_protocols: vec!["https:"],
                allow_endpoint_without_protocol: false, trim_on_blur: true,
            });
            field.default_value = Some(StorageConnectorFieldDefaultValue::String(String::new()));
            field.default_mode = aster_drive_storage::StorageConnectorFieldDefaultMode::MissingOrEmptyText;
            field.visible_when = vec![aster_drive_storage::StorageConnectorFieldCondition {
                negate: false,
                scope: None, field: "download_delivery".into(),
                value: StorageConnectorFieldDefaultValue::String("alibaba_cdn_type_a".into()),
            }];
            field.required_when = field.visible_when.clone();
            field
        },
        #[serde(default = "default_cdn_auth_validity_seconds")]
        pub oss_cdn_auth_validity_seconds: u32 => {
            let mut field = storage_connector_field(
                "oss_cdn_auth_validity_seconds", StorageConnectorFieldScope::ConnectorConfig,
                StorageConnectorFieldKind::Number, false, false,
            );
            field.default_value = Some(StorageConnectorFieldDefaultValue::Integer(300));
            field.validation.min_integer = Some(1);
            field.validation.max_integer = Some(31_536_000);
            field.help_key = Some("oss_cdn_auth_validity_seconds_desc".into());
            field.visible_when = vec![aster_drive_storage::StorageConnectorFieldCondition {
                negate: false,
                scope: None, field: "download_delivery".into(),
                value: StorageConnectorFieldDefaultValue::String("alibaba_cdn_type_a".into()),
            }];
            field
        },
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
            pub oss_cdn_auth_key: Option<AlibabaCdnAuthKey> => {
                let mut field = storage_connector_field(
                    "oss_cdn_auth_key", StorageConnectorFieldScope::StaticCredential,
                    StorageConnectorFieldKind::Secret, false, true,
                );
                field.help_key = Some("oss_cdn_auth_key_desc".into());
                field.visible_when = vec![aster_drive_storage::StorageConnectorFieldCondition {
                    negate: false,
                    scope: Some(StorageConnectorFieldScope::ConnectorConfig),
                    field: "download_delivery".into(),
                    value: StorageConnectorFieldDefaultValue::String("alibaba_cdn_type_a".into()),
                }];
                field.required_when = field.visible_when.clone();
                field
            },
        }
    }
}

#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct AlibabaCdnAuthKey(String);

impl std::fmt::Debug for AlibabaCdnAuthKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
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
            download_delivery: config.download_delivery,
            cdn_domain: {
                let trimmed = config.download_base_url.trim();
                match trimmed {
                    "" => None,
                    domain => Some(domain.to_string()),
                }
            },
            cdn_auth_validity_seconds: config.oss_cdn_auth_validity_seconds,
        }
    }

    fn driver_credentials(
        credentials: AlibabaOssStaticCredentialsV1,
    ) -> AlibabaOssStaticCredentials {
        AlibabaOssStaticCredentials {
            access_key: credentials.aliyun_oss_access_key_id,
            secret_key: credentials.aliyun_oss_access_key_secret,
            cdn_auth_key: credentials.oss_cdn_auth_key.map(|key| key.0),
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
                related_issues: vec![450, 474, 685],
            });
        descriptor.actions.push(configure_cors_action_descriptor());
        descriptor.promotions.push(promote_from_s3_descriptor());
        descriptor.capabilities.custom_download_base_url = true;
        descriptor
    }
}

#[async_trait]
impl StorageConnector for AlibabaOssConnector {
    fn icon(&self) -> Option<StorageConnectorIcon> {
        Some(StorageConnectorIcon {
            bytes: include_bytes!("assets/aliyun-oss.svg"),
            content_type: "image/svg+xml",
            revision: "1",
        })
    }

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
        let mut config: AlibabaOssConnectorConfigV1 =
            super::common::decode_normalized_connector_config(&normalized)?;
        config.download_base_url = config.download_base_url.trim().to_string();
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
        )?;
        match credential.oss_cdn_auth_key {
            Some(key) if !key.0.is_empty() => {
                crate::storage::drivers::alibaba_oss::cdn::validate_cdn_auth_key(&key.0)
                    .map_err(|error| AsterError::validation_error(error.message()))?;
            }
            Some(_) | None => {}
        }
        Ok(())
    }

    fn validate_connection_credential(
        &self,
        config: &aster_drive_storage::ConnectorConfigEnvelope,
        credential: &StorageConnectorCredentialInput,
    ) -> Result<()> {
        let config = super::common::decode_normalized_connector_config(config)?;
        let credential = super::common::decode_static_credential(credential, Self::ID)?;
        AlibabaOssDriver::validate_config(
            &Self::driver_config(config),
            &Self::driver_credentials(credential),
        )
        .map_err(|error| AsterError::validation_error(error.message()))
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

    async fn execute_saved_action(
        &self,
        context: &super::StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
        input: ExecuteSavedStorageConnectorActionInput,
    ) -> Result<StorageConnectorActionResult> {
        match input.action_id.as_str() {
            CONFIGURE_CORS_ACTION_ID => {
                let _: ConfigureAlibabaOssCorsActionInput =
                    decode_normalized_connector_action_input(
                        &configure_cors_action_descriptor(),
                        &input.values,
                    )?;
                let origins = cors_origins(context.runtime_config())?;
                let config = Self::decode_config(policy)?;
                let credentials: AlibabaOssStaticCredentialsV1 =
                    super::common::runtime_static_credential(
                        context.driver_registry(),
                        policy,
                        Self::ID,
                    )?;
                let config = Self::driver_config(config);
                let credentials = Self::driver_credentials(credentials);
                let result =
                    AlibabaOssDriver::configure_asterdrive_cors(&config, &credentials, &origins)
                        .await?;
                StorageConnectorActionResult::with_output(
                    input.action_id,
                    AlibabaOssCorsActionOutput::from(result),
                )
            }
            _ => Err(unsupported_connector_action_error(
                &self.descriptor(),
                &input.action_id,
            )),
        }
    }

    async fn execute_draft_action(
        &self,
        context: &super::StorageConnectorContext<'_>,
        input: ExecuteDraftStorageConnectorActionInput,
    ) -> Result<StorageConnectorActionResult> {
        match input.action_id.as_str() {
            CONFIGURE_CORS_ACTION_ID => {
                let _: ConfigureAlibabaOssCorsActionInput =
                    decode_normalized_connector_action_input(
                        &configure_cors_action_descriptor(),
                        &input.values,
                    )?;
                let origins = cors_origins(context.runtime_config())?;
                let connection = input.connection;
                let credential = self
                    .prepare_draft_credential(
                        context,
                        input.policy_id,
                        connection.storage.credential,
                    )
                    .await?;
                self.validate_credential_input(&credential)?;
                let connector_config =
                    self.validate_connector_config(&connection.storage.connector_config)?;
                self.validate_connection_credential(&connector_config, &credential)?;
                let config = super::common::decode_normalized_connector_config(&connector_config)?;
                let credentials = super::common::decode_static_credential(&credential, Self::ID)?;
                let config = Self::driver_config(config);
                let credentials = Self::driver_credentials(credentials);
                let result =
                    AlibabaOssDriver::configure_asterdrive_cors(&config, &credentials, &origins)
                        .await?;
                StorageConnectorActionResult::with_output(
                    input.action_id,
                    AlibabaOssCorsActionOutput::from(result),
                )
            }
            _ => Err(unsupported_connector_action_error(
                &self.descriptor(),
                &input.action_id,
            )),
        }
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

    fn direct_download_enabled(&self, policy: &storage_policy::Model) -> Result<bool> {
        let config = Self::decode_config(policy)?;
        Ok(config.object_storage_download_strategy == ObjectStorageDownloadStrategy::Presigned)
    }
}

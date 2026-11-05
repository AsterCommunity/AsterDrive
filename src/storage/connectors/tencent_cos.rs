use async_trait::async_trait;
use serde::Serialize;

use crate::api::api_error_code::ApiErrorCode;
use crate::config::site_url;
use crate::errors::{AsterError, Result, validation_error_with_code};
use crate::storage::drivers::tencent_cos::{
    CosDownloadDelivery, TencentCosDriver, TencentCosDriverConfig, TencentCosStaticCredentials,
    default_cdn_auth_validity_seconds,
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

pub struct TencentCosConnector;

const CONFIGURE_CORS_ACTION_ID: &str = "configure_tencent_cos_cors";
const PROMOTE_FROM_S3_ID: &str = "promote_from_s3";

aster_drive_storage::storage_connector_action_schema! {
    struct ConfigureTencentCosCorsActionInput {}
}

fn configure_cors_action_descriptor() -> aster_drive_storage::StorageConnectorActionDescriptor {
    custom_action_descriptor(StorageConnectorCustomActionDescriptorInput {
        action_id: StorageConnectorActionId::declared(CONFIGURE_CORS_ACTION_ID),
        label_key: "policy_cos_cors_action",
        description_key: "policy_cos_cors_desc",
        fields: ConfigureTencentCosCorsActionInput::action_fields(),
        output_fields: vec![
            action_output_field(
                "request_id",
                "policy_cos_cors_output_request_id",
                StorageConnectorActionOutputValueKind::Text,
            ),
            action_output_field(
                "rule_id",
                "policy_cos_cors_output_rule_id",
                StorageConnectorActionOutputValueKind::Text,
            ),
            action_output_field(
                "allowed_origins",
                "policy_cos_cors_output_allowed_origins",
                StorageConnectorActionOutputValueKind::StringList,
            ),
            action_output_field(
                "preserved_rule_count",
                "policy_cos_cors_output_preserved_rule_count",
                StorageConnectorActionOutputValueKind::Number,
            ),
            boolean_action_output_field(
                "replaced_existing_rule",
                "policy_cos_cors_output_replaced_existing_rule",
                "policy_cos_cors_output_yes",
                "policy_cos_cors_output_no",
            ),
            boolean_action_output_field(
                "response_vary",
                "policy_cos_cors_output_response_vary",
                "policy_cos_cors_output_yes",
                "policy_cos_cors_output_no",
            ),
        ],
        supports_draft: true,
        supports_saved: true,
        requires_authorization: false,
        mutates_remote_state: true,
        requires_confirmation: true,
    })
}

fn promote_from_s3_descriptor() -> StorageConnectorPromotionDescriptor {
    super::s3::s3_compatible_promotion_descriptor(super::s3::S3CompatiblePromotionDescriptorInput {
        promotion_id: PROMOTE_FROM_S3_ID,
        description_key: "policy_cos_promote_from_s3_desc",
        confirmation_key: "policy_cos_promote_from_s3_confirm",
        requirements: vec![StorageConnectorPromotionRequirement {
            source_field: "endpoint".to_string(),
            matcher: StorageConnectorPromotionValueMatcher::UrlHostSuffix {
                suffix: ".myqcloud.com".to_string(),
            },
            negate: false,
        }],
        target_region_field: None,
        target_access_key_field: "tencent_cos_secret_id",
        target_secret_key_field: "tencent_cos_secret_key",
    })
}

#[derive(Debug, Clone, Serialize)]
struct TencentCosCorsActionOutput {
    rule_id: String,
    allowed_origins: Vec<String>,
    request_id: Option<String>,
    preserved_rule_count: usize,
    replaced_existing_rule: bool,
    response_vary: bool,
}

impl From<crate::storage::drivers::tencent_cos::cors::TencentCosCorsApplyResult>
    for TencentCosCorsActionOutput
{
    fn from(value: crate::storage::drivers::tencent_cos::cors::TencentCosCorsApplyResult) -> Self {
        Self {
            rule_id: value.rule_id,
            allowed_origins: value.allowed_origins,
            request_id: value.request_id,
            preserved_rule_count: value.preserved_rule_count,
            replaced_existing_rule: value.replaced_existing_rule,
            response_vary: value.response_vary,
        }
    }
}

aster_drive_storage::storage_connector_schema! {
    pub struct TencentCosConnectorConfigV1 {
        config {
        pub endpoint: String => storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
            name: "endpoint", scope: StorageConnectorFieldScope::ConnectorConfig,
            kind: StorageConnectorFieldKind::Text, required: true, secret: false,
            label_key: "endpoint", placeholder: Some("https://<bucket-appid>.cos.<region>.myqcloud.com"),
            help_key: Some("cos_endpoint_hint"), required_message_key: None,
            invalid_protocol_message_key: Some("s3_endpoint_protocol_required_error"),
            allowed_endpoint_protocols: vec!["http:", "https:"],
            allow_endpoint_without_protocol: false, trim_on_blur: false,
        }),
        pub bucket: String => {
            let mut field = storage_connector_field(
                "bucket", StorageConnectorFieldScope::ConnectorConfig,
                StorageConnectorFieldKind::Text, true, false,
            );
            field.required_message_key = Some("policy_wizard_bucket_required".to_string());
            field
        },
        pub base_path: String => {
            let mut field = storage_connector_field(
                "base_path", StorageConnectorFieldScope::ConnectorConfig,
                StorageConnectorFieldKind::Text, false, false,
            );
            field.default_value = Some(StorageConnectorFieldDefaultValue::String(String::new()));
            field.default_mode = aster_drive_storage::StorageConnectorFieldDefaultMode::MissingOrEmptyText;
            field
        },
        #[serde(default)]
        pub cos_use_cname: bool => {
            let mut field = storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
                name: "cos_use_cname", scope: StorageConnectorFieldScope::ConnectorConfig,
                kind: StorageConnectorFieldKind::Boolean, required: false, secret: false,
                label_key: "cos_use_cname", placeholder: None, help_key: Some("cos_use_cname_desc"),
                required_message_key: None, invalid_protocol_message_key: None,
                allowed_endpoint_protocols: Vec::new(), allow_endpoint_without_protocol: false,
                trim_on_blur: false,
            });
            field.default_value = Some(StorageConnectorFieldDefaultValue::Boolean(false));
            field
        },
        #[serde(default)]
        pub download_delivery: CosDownloadDelivery => {
            let mut field = aster_drive_storage::storage_connector_select_field(
                "download_delivery", StorageConnectorFieldScope::ConnectorConfig, true,
                vec![
                    aster_drive_storage::StorageConnectorSelectOptionInput { value: "cos_origin", label_key: "cos_download_origin", description_key: None },
                    aster_drive_storage::StorageConnectorSelectOptionInput { value: "custom_origin", label_key: "cos_download_custom_origin", description_key: None },
                    aster_drive_storage::StorageConnectorSelectOptionInput { value: "tencent_cdn_type_d_sha256", label_key: "cos_download_cdn", description_key: None },
                ],
            );
            field.label_key = "cos_download_delivery".into();
            field.help_key = Some("cos_download_delivery_desc".into());
            field.default_value = Some(StorageConnectorFieldDefaultValue::String("cos_origin".into()));
            field
        },
        pub download_base_url: Option<String> => {
            let mut field = storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
            name: "download_base_url", scope: StorageConnectorFieldScope::ConnectorConfig,
            kind: StorageConnectorFieldKind::Text, required: false, secret: false,
            label_key: "download_base_url", placeholder: Some("https://cdn.example.com/assets"),
            help_key: Some("cos_download_base_url_desc"), required_message_key: None,
            invalid_protocol_message_key: Some("s3_endpoint_protocol_required_error"),
            allowed_endpoint_protocols: vec!["https:"], allow_endpoint_without_protocol: false,
            trim_on_blur: true,
            });
            field.visible_when = vec![aster_drive_storage::StorageConnectorFieldCondition {
                negate: true, scope: None,
                field: "download_delivery".into(), value: StorageConnectorFieldDefaultValue::String("cos_origin".into()),
            }];
            field.required_when = field.visible_when.clone();
            field
        },
        #[serde(default = "default_cdn_auth_validity_seconds")]
        pub cdn_auth_validity_seconds: u32 => {
            let mut field = storage_connector_field("cdn_auth_validity_seconds", StorageConnectorFieldScope::ConnectorConfig, StorageConnectorFieldKind::Number, false, false);
            field.default_value = Some(StorageConnectorFieldDefaultValue::Integer(300));
            field.validation.min_integer = Some(1);
            field.validation.max_integer = Some(3600);
            field.help_key = Some("cdn_auth_validity_seconds_desc".into());
            field.visible_when = vec![aster_drive_storage::StorageConnectorFieldCondition {
                negate: false,
                scope: None,
                field: "download_delivery".into(), value: StorageConnectorFieldDefaultValue::String("tencent_cdn_type_d_sha256".into()),
            }];
            field
        },
        pub object_storage_upload_strategy: ObjectStorageUploadStrategy => transfer_strategy_field(
            "object_storage_upload_strategy", StorageTransferDirection::Upload,
        ),
        pub object_storage_download_strategy: ObjectStorageDownloadStrategy => transfer_strategy_field(
            "object_storage_download_strategy", StorageTransferDirection::Download,
        ),
        }
        credentials static TencentCosStaticCredentialsV1 {
            #[serde(default)]
            pub tencent_cdn_auth_key: Option<TencentCdnAuthKey> => {
                let mut field = storage_connector_field("tencent_cdn_auth_key", StorageConnectorFieldScope::StaticCredential, StorageConnectorFieldKind::Secret, false, true);
                field.help_key = Some("tencent_cdn_auth_key_desc".into());
                field.visible_when = vec![aster_drive_storage::StorageConnectorFieldCondition {
                    negate: false,
                    scope: Some(StorageConnectorFieldScope::ConnectorConfig),
                    field: "download_delivery".into(), value: StorageConnectorFieldDefaultValue::String("tencent_cdn_type_d_sha256".into()),
                }];
                field.required_when = field.visible_when.clone();
                field
            },
            pub tencent_cos_secret_id: String => storage_connector_field(
                "tencent_cos_secret_id", StorageConnectorFieldScope::StaticCredential,
                StorageConnectorFieldKind::Text, true, false,
            ),
            pub tencent_cos_secret_key: String => storage_connector_field(
                "tencent_cos_secret_key", StorageConnectorFieldScope::StaticCredential,
                StorageConnectorFieldKind::Secret, true, true,
            ),
        }
    }
}

#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct TencentCdnAuthKey(String);

impl std::fmt::Debug for TencentCdnAuthKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl TencentCosConnector {
    pub const ID: &'static str = "asterdrive.storage.tencent_cos";

    fn decode_config(policy: &storage_policy::Model) -> Result<TencentCosConnectorConfigV1> {
        super::common::decode_typed_policy_config(policy, Self::ID, 1).map(|(config, _)| config)
    }

    fn driver_config(config: TencentCosConnectorConfigV1) -> TencentCosDriverConfig {
        TencentCosDriverConfig {
            endpoint: config.endpoint,
            bucket: config.bucket,
            base_path: config.base_path,
            use_cname: config.cos_use_cname,
            download_base_url: config.download_base_url,
            download_delivery: config.download_delivery,
            cdn_auth_validity_seconds: config.cdn_auth_validity_seconds,
            connect_timeout: std::time::Duration::from_secs(5),
            read_timeout: std::time::Duration::from_secs(30),
            operation_timeout: std::time::Duration::from_secs(3_600),
        }
    }

    fn driver_credentials(
        credentials: TencentCosStaticCredentialsV1,
    ) -> TencentCosStaticCredentials {
        TencentCosStaticCredentials {
            access_key: credentials.tencent_cos_secret_id,
            secret_key: credentials.tencent_cos_secret_key,
            cdn_auth_key: credentials.tencent_cdn_auth_key.map(|key| key.0),
        }
    }

    fn runtime_driver(
        registry: &crate::storage::DriverRegistry,
        policy: &storage_policy::Model,
    ) -> Result<TencentCosDriver> {
        let config = Self::decode_config(policy)?;
        let credentials: TencentCosStaticCredentialsV1 =
            super::common::runtime_static_credential(registry, policy, Self::ID)?;
        Ok(TencentCosDriver::new(
            Self::driver_config(config),
            Self::driver_credentials(credentials),
        )?)
    }
}

impl TencentCosConnector {
    fn descriptor_definition() -> StorageConnectorDescriptor {
        let mut descriptor =
            object_storage_connector_descriptor(ObjectStorageConnectorDescriptorInput {
                connector_id: aster_drive_storage::ConnectorId::declared(Self::ID),
                label: "Tencent COS",
                description: "Tencent Cloud COS object storage policy",
                ui: StorageConnectorUiDescriptorInput {
                    label_key: "driver_type_tencent_cos",
                    description_key: "policy_wizard_tencent_cos_storage_desc",
                    badge_rgb: StorageConnectorBadgeRgb::new(6, 182, 212),
                    helper_key: "policy_wizard_tencent_cos_helper",
                    config_step_title_key: "policy_wizard_step_connection_title",
                    config_step_description_key: "policy_wizard_step_tencent_cos_connection_desc",
                    edit_context_key: "policy_edit_context_object_storage_desc",
                    base_path_empty_display: "core:root",
                    base_path_placeholder: "tenant/prefix",
                },
                deployment_scope: StorageConnectorDeploymentScope::SharedAcrossPrimaryInstances,
                supports_initial_setup: true,
                credential_mode: TencentCosConnectorConfigV1::credential_mode(),
                fields: TencentCosConnectorConfigV1::descriptor_fields(),
                presigned_part_etag_required: true,
                storage_native_processing: true,
                config_schema_version: 1,
                credential_schema_version: Some(1),
                related_issues: vec![328, 329, 474],
            });
        descriptor.actions.push(configure_cors_action_descriptor());
        descriptor.promotions.push(promote_from_s3_descriptor());
        descriptor.capabilities.custom_download_base_url = true;
        descriptor
    }
}

async fn configure_tencent_cos_cors_for_policy(
    runtime_config: &crate::config::RuntimeConfig,
    driver: TencentCosDriver,
) -> Result<TencentCosCorsActionOutput> {
    let origins = resolve_cos_cors_allowed_origins(runtime_config)?;
    driver
        .configure_asterdrive_cors(&origins)
        .await
        .map(Into::into)
}

fn resolve_cos_cors_allowed_origins(
    runtime_config: &crate::config::RuntimeConfig,
) -> Result<Vec<String>> {
    let value = site_url::public_site_url_config_value(runtime_config).ok_or_else(|| {
        validation_error_with_code(
            ApiErrorCode::PolicyActionParameterRequired,
            "public_site_url must be configured before configuring COS CORS",
        )
    })?;
    let origins = site_url::parse_public_site_url_value(&value)?;
    if origins.is_empty() {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyActionParameterRequired,
            "public_site_url must be configured before configuring COS CORS",
        ));
    }
    Ok(origins)
}

#[async_trait]
impl StorageConnector for TencentCosConnector {
    fn icon(&self) -> Option<StorageConnectorIcon> {
        Some(StorageConnectorIcon {
            bytes: include_bytes!("assets/tencent-cloud-cos.webp"),
            content_type: "image/webp",
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
        let mut config: TencentCosConnectorConfigV1 =
            super::common::decode_normalized_connector_config(&normalized)?;
        let connection = crate::storage::drivers::s3_config::normalize_s3_endpoint_and_bucket(
            &config.endpoint,
            &config.bucket,
        )
        .map_err(|error| error.into_aster_error())?;
        config.endpoint = connection.endpoint;
        config.bucket = connection.bucket;
        config.download_base_url = config
            .download_base_url
            .as_deref()
            .map(super::common::normalize_download_base_url)
            .transpose()?;
        TencentCosDriver::validate_connection_config(&Self::driver_config(config.clone()))
            .map_err(|error| AsterError::validation_error(error.message()))?;
        super::common::encode_normalized_connector_config(
            normalized.connector_id,
            normalized.schema_version,
            config,
        )
    }

    fn validate_credential_input(&self, input: &StorageConnectorCredentialInput) -> Result<()> {
        let credential: TencentCosStaticCredentialsV1 =
            super::common::decode_static_credential(input, Self::ID)?;
        super::common::validate_required_credential_field(
            &credential.tencent_cos_secret_id,
            "tencent_cos_secret_id",
            Self::ID,
        )?;
        super::common::validate_required_credential_field(
            &credential.tencent_cos_secret_key,
            "tencent_cos_secret_key",
            Self::ID,
        )?;
        if let Some(key) = credential
            .tencent_cdn_auth_key
            .filter(|key| !key.0.is_empty())
        {
            crate::storage::drivers::tencent_cos::validate_cdn_auth_key(&key.0)
                .map_err(|error| AsterError::validation_error(error.message()))?;
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
        TencentCosDriver::validate_config(
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
        Ok(Box::new(TencentCosDriver::new(
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
        let credentials: TencentCosStaticCredentialsV1 =
            super::common::static_credential_from_cleanup_snapshot(
                context,
                policy,
                snapshots,
                Self::ID,
                1,
            )?;
        Ok(std::sync::Arc::new(TencentCosDriver::new(
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
        let descriptor = self.descriptor();
        match input.action_id.as_str() {
            CONFIGURE_CORS_ACTION_ID => {
                let _: ConfigureTencentCosCorsActionInput =
                    decode_normalized_connector_action_input(
                        &configure_cors_action_descriptor(),
                        &input.values,
                    )?;
                let driver = Self::runtime_driver(context.driver_registry(), policy)?;
                let result =
                    configure_tencent_cos_cors_for_policy(context.runtime_config(), driver).await?;
                StorageConnectorActionResult::with_output(input.action_id, result)
            }
            _ => Err(unsupported_connector_action_error(
                &descriptor,
                &input.action_id,
            )),
        }
    }

    async fn execute_draft_action(
        &self,
        context: &super::StorageConnectorContext<'_>,
        input: ExecuteDraftStorageConnectorActionInput,
    ) -> Result<StorageConnectorActionResult> {
        let descriptor = self.descriptor();
        match input.action_id.as_str() {
            CONFIGURE_CORS_ACTION_ID => {
                let _: ConfigureTencentCosCorsActionInput =
                    decode_normalized_connector_action_input(
                        &configure_cors_action_descriptor(),
                        &input.values,
                    )?;
                let connection = input.connection;
                self.validate_credential_input(&connection.storage.credential)?;
                let connector_config =
                    self.validate_connector_config(&connection.storage.connector_config)?;
                let config = super::common::decode_normalized_connector_config(&connector_config)?;
                let credentials = super::common::decode_static_credential(
                    &connection.storage.credential,
                    Self::ID,
                )?;
                let driver = TencentCosDriver::new(
                    Self::driver_config(config),
                    Self::driver_credentials(credentials),
                )?;
                let result =
                    configure_tencent_cos_cors_for_policy(context.runtime_config(), driver).await?;
                StorageConnectorActionResult::with_output(input.action_id, result)
            }
            _ => Err(unsupported_connector_action_error(
                &descriptor,
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
        match config.object_storage_download_strategy {
            ObjectStorageDownloadStrategy::Presigned => Ok(true),
            ObjectStorageDownloadStrategy::RelayStream => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests;

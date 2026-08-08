use async_trait::async_trait;
use serde::Serialize;

use crate::api::api_error_code::ApiErrorCode;
use crate::config::site_url;
use crate::errors::{AsterError, Result, validation_error_with_code};
use crate::storage::drivers::tencent_cos::{
    TencentCosDriver, TencentCosDriverConfig, TencentCosStaticCredentials,
};
use aster_drive_model::entities::storage_policy;
use aster_drive_model::types::{ObjectStorageDownloadStrategy, ObjectStorageUploadStrategy};
use aster_drive_storage::StorageDriver;
use aster_drive_storage::connector_descriptor::{
    ObjectStorageConnectorDescriptorInput, StorageConnectorActionId, StorageConnectorBadgeRgb,
    StorageConnectorCustomActionDescriptorInput, StorageConnectorDeploymentScope,
    StorageConnectorDescriptor, StorageConnectorFieldDisplayInput, StorageConnectorFieldKind,
    StorageConnectorFieldScope, StorageConnectorTransitionDescriptor,
    StorageConnectorTransitionFieldMapping, StorageConnectorTransitionId,
    StorageConnectorUiDescriptorInput, custom_action_descriptor,
    object_storage_connector_descriptor, storage_connector_field,
    storage_connector_field_with_display,
};
use aster_drive_storage::{
    StorageConnectorActionSchema, StorageConnectorConfigSchema, StorageConnectorFieldDefaultValue,
};

use super::common::{
    StorageTransferDirection, build_connection_test_policy,
    decode_normalized_connector_action_input, transfer_strategy_field,
    unsupported_connector_action_error,
};
use super::s3::{S3Connector, S3ConnectorConfigV1, S3StaticCredentialsV1};
use super::{
    ExecuteDraftStorageConnectorActionInput, ExecuteSavedStorageConnectorActionInput,
    PreparedStorageConnectorTransition, StorageConnector, StorageConnectorActionResult,
    StorageConnectorCredentialInput, StorageConnectorTransitionDraft,
    StorageConnectorTransitionDraftPlan, StorageConnectorTransitionSavedState,
    StorageConnectorUploadTransport,
};

mod localization;

pub struct TencentCosConnector;

const CONFIGURE_CORS_ACTION_ID: &str = "configure_tencent_cos_cors";
const FROM_GENERIC_S3_TRANSITION_ID: &str = "from_generic_s3";

aster_drive_storage::storage_connector_action_schema! {
    struct ConfigureTencentCosCorsActionInput {}
}

fn configure_cors_action_descriptor() -> aster_drive_storage::StorageConnectorActionDescriptor {
    custom_action_descriptor(StorageConnectorCustomActionDescriptorInput {
        action_id: StorageConnectorActionId::declared(CONFIGURE_CORS_ACTION_ID),
        label_key: "policy_cos_cors_action",
        description_key: "policy_cos_cors_desc",
        fields: ConfigureTencentCosCorsActionInput::action_fields(),
        supports_draft: true,
        supports_saved: true,
        requires_authorization: false,
        mutates_remote_state: true,
        requires_confirmation: true,
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
        pub object_storage_upload_strategy: ObjectStorageUploadStrategy => transfer_strategy_field(
            "object_storage_upload_strategy", StorageTransferDirection::Upload,
        ),
        pub object_storage_download_strategy: ObjectStorageDownloadStrategy => transfer_strategy_field(
            "object_storage_download_strategy", StorageTransferDirection::Download,
        ),
        pub storage_native_processing_enabled: bool => {
            let mut field = storage_connector_field(
                "storage_native_processing_enabled", StorageConnectorFieldScope::ConnectorConfig,
                StorageConnectorFieldKind::Boolean, false, false,
            );
            field.default_value = Some(StorageConnectorFieldDefaultValue::Boolean(false));
            field
        },
        pub storage_native_media_metadata_enabled: bool => {
            let mut field = storage_connector_field(
                "storage_native_media_metadata_enabled", StorageConnectorFieldScope::ConnectorConfig,
                StorageConnectorFieldKind::Boolean, false, false,
            );
            field.default_value = Some(StorageConnectorFieldDefaultValue::Boolean(false));
            field
        },
        }
        credentials static TencentCosStaticCredentialsV1 {
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
                    icon_src: Some("/static/storage/tencent-cloud-cos.webp"),
                    icon_name: None,
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
                related_issues: vec![328, 329],
            });
        descriptor.actions.push(configure_cors_action_descriptor());
        descriptor
            .inbound_transitions
            .push(StorageConnectorTransitionDescriptor {
                transition_id: StorageConnectorTransitionId::declared(
                    FROM_GENERIC_S3_TRANSITION_ID,
                ),
                source_connector_id: aster_drive_storage::ConnectorId::declared(S3Connector::ID),
                label_key: "policy_connector_transition_from_s3".to_string(),
                description_key: "policy_connector_transition_from_s3_desc".to_string(),
                supports_draft: true,
                supports_saved: true,
                requires_confirmation: true,
            });
        descriptor
    }
}

impl TencentCosConnector {
    fn transition_from_generic_s3_config(
        source: &aster_drive_storage::ConnectorConfigEnvelope,
    ) -> Result<Option<aster_drive_storage::ConnectorConfigEnvelope>> {
        if source.connector_id.as_str() != S3Connector::ID {
            return Ok(None);
        }
        if source.schema_version != 1 {
            return Err(AsterError::validation_error(format!(
                "S3 connector transition requires config schema version 1, got {}",
                source.schema_version
            )));
        }
        let config: S3ConnectorConfigV1 =
            super::common::decode_normalized_connector_config(source)?;
        let endpoint = url::Url::parse(&config.endpoint)
            .map_err(|error| AsterError::validation_error(error.to_string()))?;
        let Some(host) = endpoint.host_str().map(str::to_ascii_lowercase) else {
            return Ok(None);
        };
        if !host.ends_with(".myqcloud.com") {
            return Ok(None);
        }

        super::common::encode_normalized_connector_config(
            aster_drive_storage::ConnectorId::declared(Self::ID),
            1,
            TencentCosConnectorConfigV1 {
                endpoint: config.endpoint,
                bucket: config.bucket,
                base_path: config.base_path,
                object_storage_upload_strategy: config.object_storage_upload_strategy,
                object_storage_download_strategy: config.object_storage_download_strategy,
                storage_native_processing_enabled: false,
                storage_native_media_metadata_enabled: false,
            },
        )
        .map(Some)
    }

    fn transition_field_mappings() -> Vec<StorageConnectorTransitionFieldMapping> {
        vec![
            StorageConnectorTransitionFieldMapping {
                source_scope: StorageConnectorFieldScope::StaticCredential,
                source_name: "s3_access_key_id".to_string(),
                target_scope: StorageConnectorFieldScope::StaticCredential,
                target_name: "tencent_cos_secret_id".to_string(),
            },
            StorageConnectorTransitionFieldMapping {
                source_scope: StorageConnectorFieldScope::StaticCredential,
                source_name: "s3_secret_access_key".to_string(),
                target_scope: StorageConnectorFieldScope::StaticCredential,
                target_name: "tencent_cos_secret_key".to_string(),
            },
        ]
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
    let origins = site_url::public_site_urls(runtime_config);
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
        let endpoint = url::Url::parse(&config.endpoint)
            .map_err(|error| AsterError::validation_error(error.to_string()))?;
        if !endpoint
            .host_str()
            .is_some_and(|host| host.ends_with(".myqcloud.com"))
        {
            return Err(AsterError::validation_error(
                "COS endpoint must use a Tencent COS myqcloud.com host",
            ));
        }
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
        )
    }

    fn import_legacy_credential(
        &self,
        _encryption_key: &str,
        _policy: &storage_policy::Model,
        input: super::LegacyStorageConnectorCredentialInput,
    ) -> Result<Option<serde_json::Value>> {
        super::common::import_legacy_static_credential(Self::ID, input, |legacy| {
            TencentCosStaticCredentialsV1 {
                tencent_cos_secret_id: legacy.access_key,
                tencent_cos_secret_key: legacy.secret_key,
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
        if input.action_id.as_str() != CONFIGURE_CORS_ACTION_ID {
            return Err(unsupported_connector_action_error(
                &descriptor,
                &input.action_id,
            ));
        }
        let _: ConfigureTencentCosCorsActionInput = decode_normalized_connector_action_input(
            &configure_cors_action_descriptor(),
            &input.values,
        )?;
        let driver = Self::runtime_driver(context.driver_registry(), policy)?;
        let result =
            configure_tencent_cos_cors_for_policy(context.runtime_config(), driver).await?;
        StorageConnectorActionResult::with_output(input.action_id, result)
    }

    async fn execute_draft_action(
        &self,
        context: &super::StorageConnectorContext<'_>,
        input: ExecuteDraftStorageConnectorActionInput,
    ) -> Result<StorageConnectorActionResult> {
        let descriptor = self.descriptor();
        if input.action_id.as_str() != CONFIGURE_CORS_ACTION_ID {
            return Err(unsupported_connector_action_error(
                &descriptor,
                &input.action_id,
            ));
        }
        let _: ConfigureTencentCosCorsActionInput = decode_normalized_connector_action_input(
            &configure_cors_action_descriptor(),
            &input.values,
        )?;
        let connection = input.connection;
        self.validate_credential_input(&connection.credential)?;
        let connector_config = self.validate_connector_config(&connection.connector_config)?;
        let policy = build_connection_test_policy(connector_config, connection.behavior)?;
        let config = Self::decode_config(&policy)?;
        let credentials =
            super::common::decode_static_credential(&connection.credential, Self::ID)?;
        let driver = TencentCosDriver::new(
            Self::driver_config(config),
            Self::driver_credentials(credentials),
        )?;
        let result =
            configure_tencent_cos_cors_for_policy(context.runtime_config(), driver).await?;
        StorageConnectorActionResult::with_output(input.action_id, result)
    }

    fn preview_inbound_transition(
        &self,
        transition_id: &StorageConnectorTransitionId,
        source: StorageConnectorTransitionDraft<'_>,
    ) -> Result<Option<StorageConnectorTransitionDraftPlan>> {
        if transition_id.as_str() != FROM_GENERIC_S3_TRANSITION_ID {
            return Ok(None);
        }
        let Some(connector_config) =
            Self::transition_from_generic_s3_config(source.connector_config)?
        else {
            return Ok(None);
        };
        let connector_config = self.validate_connector_config(&connector_config)?;
        self.validate_policy_behavior(source.behavior)?;
        Ok(Some(StorageConnectorTransitionDraftPlan {
            connector_config,
            behavior: source.behavior.clone(),
            field_mappings: Self::transition_field_mappings(),
        }))
    }

    fn prepare_inbound_transition(
        &self,
        transition_id: &StorageConnectorTransitionId,
        source: StorageConnectorTransitionSavedState<'_>,
    ) -> Result<PreparedStorageConnectorTransition> {
        if transition_id.as_str() != FROM_GENERIC_S3_TRANSITION_ID {
            return Err(AsterError::validation_error(format!(
                "Tencent COS does not support connector transition '{}'",
                transition_id.as_str()
            )));
        }
        if source.policy.connector_id != S3Connector::ID {
            return Err(AsterError::validation_error(
                "Tencent COS transition source must be the generic S3 connector",
            ));
        }
        let connector_config = Self::transition_from_generic_s3_config(source.connector_config)?
            .ok_or_else(|| {
                AsterError::validation_error(
                    "generic S3 policy endpoint is not a Tencent COS endpoint",
                )
            })?;
        let connector_config = self.validate_connector_config(&connector_config)?;
        self.validate_policy_behavior(source.behavior)?;

        let credential = source.credential.ok_or_else(|| {
            AsterError::database_operation("generic S3 policy is missing connector credentials")
        })?;
        if credential.connector_id.as_str() != S3Connector::ID || credential.schema_version != 1 {
            return Err(AsterError::database_operation(
                "generic S3 policy credential does not match connector schema version 1",
            ));
        }
        let source_credential: S3StaticCredentialsV1 =
            serde_json::from_value(credential.values.clone()).map_err(|error| {
                AsterError::database_operation(format!(
                    "generic S3 policy credential payload is invalid: {error}"
                ))
            })?;
        let credential = StorageConnectorCredentialInput::Static(
            serde_json::to_value(TencentCosStaticCredentialsV1 {
                tencent_cos_secret_id: source_credential.s3_access_key_id,
                tencent_cos_secret_key: source_credential.s3_secret_access_key,
            })
            .map_err(|error| {
                AsterError::internal_error(format!(
                    "serialize Tencent COS transition credential: {error}"
                ))
            })?,
        );
        self.validate_credential_input(&credential)?;

        Ok(PreparedStorageConnectorTransition {
            connector_config,
            behavior: source.behavior.clone(),
            credential,
        })
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

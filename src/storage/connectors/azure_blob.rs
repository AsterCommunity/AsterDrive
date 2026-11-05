use async_trait::async_trait;

use crate::api::api_error_code::ApiErrorCode;
use crate::config::site_url;
use crate::errors::{Result, validation_error_with_code};
use crate::storage::drivers::azure_blob::{
    AzureBlobConfigError, AzureBlobDriver, AzureBlobDriverConfig, AzureBlobStaticCredentials,
};
use aster_drive_model::entities::storage_policy;
use aster_drive_model::types::{ObjectStorageDownloadStrategy, ObjectStorageUploadStrategy};
use aster_drive_storage::StorageDriver;
use aster_drive_storage::connector_descriptor::StorageConnectorActionOutputValueKind;
use aster_drive_storage::connector_descriptor::{
    ObjectStorageConnectorDescriptorInput, StorageConnectorBadgeRgb,
    StorageConnectorDeploymentScope, StorageConnectorDescriptor, StorageConnectorFieldDisplayInput,
    StorageConnectorFieldKind, StorageConnectorFieldScope, StorageConnectorUiDescriptorInput,
    object_storage_connector_descriptor, storage_connector_field,
    storage_connector_field_with_display,
};
use aster_drive_storage::{
    StorageConnectorActionDescriptor, StorageConnectorActionId, StorageConnectorActionSchema,
};
use aster_drive_storage::{StorageConnectorConfigSchema, StorageConnectorFieldDefaultValue};

use super::common::{
    StorageTransferDirection, action_output_field, boolean_action_output_field,
    transfer_strategy_field,
};
use super::{
    ExecuteDraftStorageConnectorActionInput, ExecuteSavedStorageConnectorActionInput,
    StorageConnector, StorageConnectorActionResult, StorageConnectorCredentialInput,
    StorageConnectorIcon, StorageConnectorUploadTransport,
};

mod localization;

pub struct AzureBlobConnector;

const CONFIGURE_CORS_ACTION_ID: &str = "configure_azure_blob_cors";

aster_drive_storage::storage_connector_action_schema! {
    struct ConfigureAzureBlobCorsActionInput {}
}

fn configure_cors_action_descriptor() -> StorageConnectorActionDescriptor {
    use aster_drive_storage::connector_descriptor::{
        StorageConnectorCustomActionDescriptorInput, custom_action_descriptor,
    };
    custom_action_descriptor(StorageConnectorCustomActionDescriptorInput {
        action_id: StorageConnectorActionId::declared(CONFIGURE_CORS_ACTION_ID),
        label_key: "azure_cors_action",
        description_key: "azure_cors_description",
        fields: ConfigureAzureBlobCorsActionInput::action_fields(),
        output_fields: vec![
            action_output_field(
                "account",
                "azure_blob_account_name",
                StorageConnectorActionOutputValueKind::Text,
            ),
            action_output_field(
                "service_endpoint",
                "endpoint",
                StorageConnectorActionOutputValueKind::Text,
            ),
            action_output_field(
                "allowed_origins",
                "azure_cors_origins",
                StorageConnectorActionOutputValueKind::StringList,
            ),
            action_output_field(
                "preserved_rule_count",
                "azure_cors_preserved",
                StorageConnectorActionOutputValueKind::Number,
            ),
            action_output_field(
                "request_id",
                "azure_cors_request_id",
                StorageConnectorActionOutputValueKind::Text,
            ),
            boolean_action_output_field(
                "changed",
                "azure_cors_changed",
                "azure_cors_yes",
                "azure_cors_no",
            ),
        ],
        supports_draft: true,
        supports_saved: true,
        requires_authorization: false,
        mutates_remote_state: true,
        requires_confirmation: true,
    })
}

async fn configure_azure_blob_cors_for_policy(
    runtime_config: &crate::config::RuntimeConfig,
    driver: AzureBlobDriver,
    action_id: StorageConnectorActionId,
) -> Result<StorageConnectorActionResult> {
    let origins = resolve_azure_blob_cors_allowed_origins(runtime_config)?;
    StorageConnectorActionResult::with_output(
        action_id,
        driver.configure_asterdrive_cors(&origins).await?,
    )
}

fn resolve_azure_blob_cors_allowed_origins(
    runtime_config: &crate::config::RuntimeConfig,
) -> Result<Vec<String>> {
    let required = || {
        validation_error_with_code(
            ApiErrorCode::PolicyActionParameterRequired,
            "public_site_url must be configured before configuring Azure Blob CORS",
        )
    };
    let value = site_url::public_site_url_config_value(runtime_config).ok_or_else(required)?;
    match site_url::parse_public_site_url_value(&value)? {
        origins if origins.is_empty() => Err(required()),
        origins => Ok(origins),
    }
}

aster_drive_storage::storage_connector_schema! {
    pub struct AzureBlobConnectorConfigV1 {
        config {
        pub endpoint: String => storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
            name: "endpoint", scope: StorageConnectorFieldScope::ConnectorConfig,
            kind: StorageConnectorFieldKind::Text, required: true, secret: false,
            label_key: "endpoint", placeholder: Some("https://<account>.blob.core.windows.net"),
            help_key: Some("azure_blob_endpoint_hint"), required_message_key: None,
            invalid_protocol_message_key: Some("azure_blob_endpoint_protocol_required_error"),
            allowed_endpoint_protocols: vec!["http:", "https:"],
            allow_endpoint_without_protocol: false, trim_on_blur: false,
        }),
        pub bucket: String => {
            let mut field = storage_connector_field(
                "bucket", StorageConnectorFieldScope::ConnectorConfig,
                StorageConnectorFieldKind::Text, true, false,
            );
            field.required_message_key = Some("policy_wizard_container_required".to_string());
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
        }
        credentials static AzureBlobStaticCredentialsV1 {
            pub azure_blob_account_name: String => storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
                name: "azure_blob_account_name", scope: StorageConnectorFieldScope::StaticCredential,
                kind: StorageConnectorFieldKind::Text, required: true, secret: false,
                label_key: "azure_blob_account_name", placeholder: None, help_key: None,
                required_message_key: None, invalid_protocol_message_key: None,
                allowed_endpoint_protocols: Vec::new(), allow_endpoint_without_protocol: false,
                trim_on_blur: true,
            }),
            pub azure_blob_account_key: String => storage_connector_field(
                "azure_blob_account_key", StorageConnectorFieldScope::StaticCredential,
                StorageConnectorFieldKind::Secret, true, true,
            ),
        }
    }
}

impl AzureBlobConnector {
    pub const ID: &'static str = "asterdrive.storage.azure_blob";

    fn decode_config(policy: &storage_policy::Model) -> Result<AzureBlobConnectorConfigV1> {
        super::common::decode_typed_policy_config(policy, Self::ID, 1)
            .map(|(config, _behavior)| config)
    }

    fn driver_config(chunk_size: i64, config: AzureBlobConnectorConfigV1) -> AzureBlobDriverConfig {
        AzureBlobDriverConfig {
            endpoint: config.endpoint,
            container: config.bucket,
            base_path: config.base_path,
            chunk_size,
        }
    }

    fn driver_credentials(credentials: AzureBlobStaticCredentialsV1) -> AzureBlobStaticCredentials {
        AzureBlobStaticCredentials {
            account_name: credentials.azure_blob_account_name,
            account_key: credentials.azure_blob_account_key,
        }
    }
}

impl AzureBlobConnector {
    fn descriptor_definition() -> StorageConnectorDescriptor {
        let mut descriptor =
            object_storage_connector_descriptor(ObjectStorageConnectorDescriptorInput {
                connector_id: aster_drive_storage::ConnectorId::declared(Self::ID),
                label: "Azure Blob Storage",
                description: "Azure Blob block blob storage policy",
                ui: StorageConnectorUiDescriptorInput {
                    label_key: "driver_type_azure_blob",
                    description_key: "policy_wizard_azure_blob_storage_desc",
                    badge_rgb: StorageConnectorBadgeRgb::new(14, 165, 233),
                    helper_key: "policy_wizard_azure_blob_helper",
                    config_step_title_key: "policy_wizard_step_connection_title",
                    config_step_description_key: "policy_wizard_step_azure_blob_connection_desc",
                    edit_context_key: "policy_edit_context_azure_blob_desc",
                    base_path_empty_display: "core:root",
                    base_path_placeholder: "tenant/prefix",
                },
                deployment_scope: StorageConnectorDeploymentScope::SharedAcrossPrimaryInstances,
                supports_initial_setup: true,
                credential_mode: AzureBlobConnectorConfigV1::credential_mode(),
                fields: AzureBlobConnectorConfigV1::descriptor_fields(),
                presigned_part_etag_required: false,
                storage_native_processing: false,
                config_schema_version: 1,
                credential_schema_version: Some(1),
                related_issues: vec![328, 329, 734],
            });
        descriptor.actions.push(configure_cors_action_descriptor());
        descriptor
    }
}

#[async_trait]
impl StorageConnector for AzureBlobConnector {
    fn icon(&self) -> Option<StorageConnectorIcon> {
        Some(StorageConnectorIcon {
            bytes: include_bytes!("assets/azure-blob.svg"),
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
            .map_err(|error| crate::errors::AsterError::validation_error(error.to_string()))?;
        let mut config: AzureBlobConnectorConfigV1 =
            super::common::decode_normalized_connector_config(&normalized)?;
        let connection =
            AzureBlobDriver::try_normalize_endpoint_and_container(&config.endpoint, &config.bucket)
                .map_err(|error| {
                    let api_code = match &error {
                        AzureBlobConfigError::MissingContainer => {
                            ApiErrorCode::PolicyStorageBucketRequired
                        }
                        AzureBlobConfigError::MissingEndpoint
                        | AzureBlobConfigError::InvalidEndpoint => {
                            ApiErrorCode::PolicyStorageEndpointInvalid
                        }
                    };
                    error.into_aster_error().with_api_error_code(api_code)
                })?;
        config.endpoint = connection.endpoint;
        config.bucket = connection.container;
        super::common::encode_normalized_connector_config(
            normalized.connector_id,
            normalized.schema_version,
            config,
        )
    }

    fn validate_credential_input(&self, input: &StorageConnectorCredentialInput) -> Result<()> {
        let credential: AzureBlobStaticCredentialsV1 =
            super::common::decode_static_credential(input, Self::ID)?;
        super::common::validate_required_credential_field(
            &credential.azure_blob_account_name,
            "azure_blob_account_name",
            Self::ID,
        )?;
        super::common::validate_required_credential_field(
            &credential.azure_blob_account_key,
            "azure_blob_account_key",
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
        Ok(Box::new(AzureBlobDriver::new(
            Self::driver_config(0, config),
            Self::driver_credentials(credentials),
        )?))
    }

    fn build_runtime_driver(
        &self,
        registry: &crate::storage::DriverRegistry,
        policy: &storage_policy::Model,
    ) -> Result<super::StorageConnectorDriver> {
        let config = Self::decode_config(policy)?;
        let credentials: AzureBlobStaticCredentialsV1 =
            super::common::runtime_static_credential(registry, policy, Self::ID)?;
        Ok(super::StorageConnectorDriver::multipart(
            std::sync::Arc::new(AzureBlobDriver::new(
                Self::driver_config(policy.chunk_size, config),
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
        let credentials: AzureBlobStaticCredentialsV1 =
            super::common::static_credential_from_cleanup_snapshot(
                context,
                policy,
                snapshots,
                Self::ID,
                1,
            )?;
        Ok(std::sync::Arc::new(AzureBlobDriver::new(
            Self::driver_config(policy.chunk_size, config),
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
            CONFIGURE_CORS_ACTION_ID => {}
            _ => {
                return Err(super::common::unsupported_connector_action_error(
                    &self.descriptor(),
                    &input.action_id,
                ));
            }
        }
        let _: ConfigureAzureBlobCorsActionInput =
            super::common::decode_normalized_connector_action_input(
                &configure_cors_action_descriptor(),
                &input.values,
            )?;
        let config = Self::decode_config(policy)?;
        let credentials =
            super::common::runtime_static_credential(context.driver_registry(), policy, Self::ID)?;
        let driver = AzureBlobDriver::new(
            Self::driver_config(policy.chunk_size, config),
            Self::driver_credentials(credentials),
        )?;
        configure_azure_blob_cors_for_policy(context.runtime_config(), driver, input.action_id)
            .await
    }

    async fn execute_draft_action(
        &self,
        context: &super::StorageConnectorContext<'_>,
        input: ExecuteDraftStorageConnectorActionInput,
    ) -> Result<StorageConnectorActionResult> {
        match input.action_id.as_str() {
            CONFIGURE_CORS_ACTION_ID => {}
            _ => {
                return Err(super::common::unsupported_connector_action_error(
                    &self.descriptor(),
                    &input.action_id,
                ));
            }
        }
        let _: ConfigureAzureBlobCorsActionInput =
            super::common::decode_normalized_connector_action_input(
                &configure_cors_action_descriptor(),
                &input.values,
            )?;
        let connection = input.connection;
        let credential = self
            .prepare_draft_credential(context, input.policy_id, connection.storage.credential)
            .await?;
        self.validate_credential_input(&credential)?;
        let config = self.validate_connector_config(&connection.storage.connector_config)?;
        let config = super::common::decode_normalized_connector_config(&config)?;
        let credentials = super::common::decode_static_credential(&credential, Self::ID)?;
        let driver = AzureBlobDriver::new(
            Self::driver_config(0, config),
            Self::driver_credentials(credentials),
        )?;
        configure_azure_blob_cors_for_policy(context.runtime_config(), driver, input.action_id)
            .await
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

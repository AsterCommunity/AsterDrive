use async_trait::async_trait;

use crate::api::api_error_code::ApiErrorCode;
use crate::config::site_url;
use crate::errors::{Result, validation_error_with_code};
use crate::storage::drivers::tencent_cos::TencentCosDriver;
use aster_drive_model::entities::storage_policy;
use aster_drive_model::types::{
    ObjectStorageDownloadStrategy, ObjectStorageUploadStrategy, parse_storage_policy_options,
};
use aster_drive_storage::StorageDriver;
use aster_drive_storage::connector_descriptor::{
    ObjectStorageConnectorDescriptorInput, StorageConnectorDeploymentScope,
    StorageConnectorDescriptor, StorageConnectorFieldDisplayInput, StorageConnectorFieldKind,
    StorageConnectorFieldScope, StorageConnectorUiDescriptorInput, StoragePolicyExecutableAction,
    object_storage_connector_descriptor, policy_action_descriptor, storage_connector_field,
    storage_connector_field_with_display, storage_connector_field_with_options,
};
use aster_drive_storage::{StorageConnectorConfigSchema, StorageConnectorFieldDefaultValue};

use super::common::{
    build_connection_test_policy, ensure_policy_action_supported, normalize_s3_connection_fields,
    validate_static_secret_credentials,
};
use super::{
    ExecuteDraftStorageConnectorActionInput, StorageConnector, StorageConnectorActionResult,
    StorageConnectorConnectionInput, StorageConnectorUploadTransport, TencentCosCorsConfigResult,
};

pub struct TencentCosConnector;

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
        pub base_path: String => storage_connector_field(
            "base_path", StorageConnectorFieldScope::ConnectorConfig,
            StorageConnectorFieldKind::Text, false, false,
        ),
        pub object_storage_upload_strategy: ObjectStorageUploadStrategy => cos_transfer_field(
            "object_storage_upload_strategy",
        ),
        pub object_storage_download_strategy: ObjectStorageDownloadStrategy => cos_transfer_field(
            "object_storage_download_strategy",
        ),
        pub storage_native_processing_enabled: bool => storage_connector_field(
            "storage_native_processing_enabled", StorageConnectorFieldScope::ConnectorConfig,
            StorageConnectorFieldKind::Boolean, false, false,
        ),
        pub storage_native_media_metadata_enabled: bool => storage_connector_field(
            "storage_native_media_metadata_enabled", StorageConnectorFieldScope::ConnectorConfig,
            StorageConnectorFieldKind::Boolean, false, false,
        ),
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

fn cos_transfer_field(name: &str) -> aster_drive_storage::StorageConnectorFieldDescriptor {
    let mut field = storage_connector_field_with_options(
        name,
        StorageConnectorFieldScope::ConnectorConfig,
        StorageConnectorFieldKind::Select,
        true,
        false,
        vec!["relay_stream", "presigned"],
    );
    field.default_value = Some(StorageConnectorFieldDefaultValue::String(
        "relay_stream".to_string(),
    ));
    field
}

impl TencentCosConnector {
    pub const ID: &'static str = "asterdrive.storage.tencent_cos";
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
                related_issues: vec![328, 329],
            });
        descriptor.actions.push(policy_action_descriptor(
            StoragePolicyExecutableAction::ConfigureTencentCosCors,
        ));
        descriptor
    }
}

impl TencentCosConnector {
    pub(super) fn validate_promotion_candidate(policy: &storage_policy::Model) -> Result<()> {
        Ok(TencentCosDriver::validate_policy(policy)?)
    }
}

async fn configure_tencent_cos_cors_for_policy(
    runtime_config: &crate::config::RuntimeConfig,
    policy: &storage_policy::Model,
) -> Result<TencentCosCorsConfigResult> {
    let origins = resolve_cos_cors_allowed_origins(runtime_config)?;
    let driver = TencentCosDriver::new(policy)?;
    driver
        .configure_asterdrive_cors(&origins)
        .await
        .map(Into::into)
}

async fn merge_draft_action_saved_credentials(
    db: &sea_orm::DatabaseConnection,
    policy_id: Option<i64>,
    connection: StorageConnectorConnectionInput,
) -> Result<StorageConnectorConnectionInput> {
    super::common::merge_saved_static_credentials_for_draft(
        db,
        policy_id,
        connection,
        "draft storage policy action",
    )
    .await
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

    fn encode_config(
        &self,
        input: &StorageConnectorConnectionInput,
    ) -> Result<aster_drive_model::types::StoredConnectorConfig> {
        super::common::encode_typed_connector_config(
            Self::ID,
            1,
            TencentCosConnectorConfigV1 {
                endpoint: input.endpoint.clone(),
                bucket: input.bucket.clone(),
                base_path: input.base_path.clone(),
                object_storage_upload_strategy: input
                    .options
                    .effective_object_storage_upload_strategy(),
                object_storage_download_strategy: input
                    .options
                    .effective_object_storage_download_strategy(),
                storage_native_processing_enabled: input
                    .options
                    .storage_native_processing_enabled(),
                storage_native_media_metadata_enabled: input
                    .options
                    .storage_native_media_metadata_enabled(),
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
        validate_static_secret_credentials(input, "tencent_cos")
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
        Ok(Box::new(TencentCosDriver::new(policy)?))
    }

    fn build_runtime_driver(
        &self,
        _registry: &crate::storage::DriverRegistry,
        policy: &storage_policy::Model,
    ) -> Result<super::StorageConnectorDriver> {
        Ok(super::StorageConnectorDriver::multipart(
            std::sync::Arc::new(TencentCosDriver::new(policy)?),
        ))
    }

    async fn execute_saved_action(
        &self,
        context: &super::StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
        action: StoragePolicyExecutableAction,
    ) -> Result<StorageConnectorActionResult> {
        ensure_policy_action_supported(self.descriptor(), action)?;
        match action {
            StoragePolicyExecutableAction::ConfigureTencentCosCors => {
                let result =
                    configure_tencent_cos_cors_for_policy(context.runtime_config(), policy).await?;
                Ok(StorageConnectorActionResult {
                    action,
                    tencent_cos_cors: Some(result),
                })
            }
        }
    }

    async fn execute_draft_action(
        &self,
        context: &super::StorageConnectorContext<'_>,
        input: ExecuteDraftStorageConnectorActionInput,
    ) -> Result<StorageConnectorActionResult> {
        ensure_policy_action_supported(self.descriptor(), input.action)?;
        match input.action {
            StoragePolicyExecutableAction::ConfigureTencentCosCors => {
                let connection = merge_draft_action_saved_credentials(
                    context.writer_db(),
                    input.policy_id,
                    input.connection,
                )
                .await?;
                let policy =
                    build_connection_test_policy(context.writer_db(), self, connection).await?;
                let result =
                    configure_tencent_cos_cors_for_policy(context.runtime_config(), &policy)
                        .await?;
                Ok(StorageConnectorActionResult {
                    action: input.action,
                    tencent_cos_cors: Some(result),
                })
            }
        }
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

    fn validate_promotion_candidate(&self, policy: &storage_policy::Model) -> Result<()> {
        Self::validate_promotion_candidate(policy)
    }
}

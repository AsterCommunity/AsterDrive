use async_trait::async_trait;

use crate::api::api_error_code::ApiErrorCode;
use crate::config::site_url;
use crate::errors::{Result, validation_error_with_code};
use crate::storage::drivers::tencent_cos::TencentCosDriver;
use aster_drive_model::entities::storage_policy;
use aster_drive_model::types::{ObjectStorageDownloadStrategy, parse_storage_policy_options};
use aster_drive_storage::StorageDriver;
use aster_drive_storage::connector_descriptor::{
    ObjectStorageConnectorDescriptorInput, ObjectStorageFieldDescriptorInput,
    StorageConnectorDeploymentScope, StorageConnectorDescriptor, StorageConnectorUiDescriptorInput,
    StoragePolicyExecutableAction, object_storage_connector_descriptor, policy_action_descriptor,
};

use super::common::{
    build_connection_test_policy, ensure_policy_action_supported, normalize_s3_connection_fields,
    validate_static_secret_credentials,
};
use super::{
    ExecuteDraftStorageConnectorActionInput, StorageConnector, StorageConnectorActionResult,
    StorageConnectorConnectionInput, StorageConnectorUploadTransport, TencentCosCorsConfigResult,
};

pub struct TencentCosConnector;

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
                fields: ObjectStorageFieldDescriptorInput {
                    endpoint_placeholder: "https://<bucket-appid>.cos.<region>.myqcloud.com",
                    endpoint_help_key: "cos_endpoint_hint",
                    endpoint_protocol_error_key: "s3_endpoint_protocol_required_error",
                    bucket_required_message_key: "policy_wizard_bucket_required",
                    access_key_label_key: "access_key",
                    secret_key_label_key: "secret_key",
                    access_key_trim_on_blur: false,
                },
                include_s3_path_style: false,
                include_s3_region: false,
                include_s3_timeouts: false,
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

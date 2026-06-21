use async_trait::async_trait;

use crate::api::api_error_code::ApiErrorCode;
use crate::config::site_url;
use crate::entities::storage_policy;
use crate::errors::{Result, validation_error_with_code};
use crate::runtime::{RemoteProtocolRuntimeState, SharedRuntimeState};
use crate::storage::StorageDriver;
use crate::storage::connector_descriptor::{
    ObjectStorageConnectorDescriptorInput, ObjectStorageFieldDescriptorInput,
    StorageConnectorDescriptor, StorageConnectorDescriptorProvider,
    StorageConnectorUiDescriptorInput, StoragePolicyExecutableAction,
    object_storage_connector_descriptor, policy_action_descriptor,
};
use crate::storage::drivers::tencent_cos::TencentCosDriver;
use crate::types::{DriverType, parse_storage_policy_options};

use super::common::{
    build_connection_test_policy, ensure_policy_action_supported, normalize_s3_connection_fields,
    validate_static_secret_credentials,
};
use super::{
    ExecuteDraftStorageConnectorActionInput, StorageConnector, StorageConnectorActionResult,
    StorageConnectorConnectionInput, StorageConnectorUploadTransport, TencentCosCorsConfigResult,
};

pub struct TencentCosConnector;

impl StorageConnectorDescriptorProvider for TencentCosConnector {
    fn storage_connector_descriptor() -> StorageConnectorDescriptor {
        let mut descriptor =
            object_storage_connector_descriptor(ObjectStorageConnectorDescriptorInput {
                driver_type: DriverType::TencentCos,
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
                    edit_context_key: "policy_edit_context_s3_desc",
                    base_path_empty_display: "core:root",
                    base_path_placeholder: "tenant/prefix",
                },
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
                presigned_part_etag_required: true,
                storage_native_processing: true,
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
        TencentCosDriver::validate_policy(policy)
    }
}

async fn configure_tencent_cos_cors_for_policy<S: SharedRuntimeState + ?Sized>(
    state: &S,
    policy: &storage_policy::Model,
) -> Result<TencentCosCorsConfigResult> {
    let origins = resolve_cos_cors_allowed_origins(state)?;
    let driver = TencentCosDriver::new(policy)?;
    driver
        .configure_asterdrive_cors(&origins)
        .await
        .map(Into::into)
}

async fn merge_draft_action_saved_credentials<S: SharedRuntimeState + ?Sized>(
    state: &S,
    policy_id: Option<i64>,
    connection: StorageConnectorConnectionInput,
) -> Result<StorageConnectorConnectionInput> {
    super::common::merge_saved_static_credentials_for_draft(
        state.writer_db(),
        policy_id,
        connection,
        "draft storage policy action",
    )
    .await
}

fn resolve_cos_cors_allowed_origins(
    state: &(impl SharedRuntimeState + ?Sized),
) -> Result<Vec<String>> {
    let origins = site_url::public_site_urls(state.runtime_config());
    if origins.is_empty() {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyActionParameterRequired,
            "public_site_url must be configured before configuring COS CORS",
        ));
    }
    Ok(origins)
}

#[async_trait(?Send)]
impl StorageConnector for TencentCosConnector {
    fn driver_type() -> DriverType {
        DriverType::TencentCos
    }

    fn normalize_connection_fields(endpoint: &str, bucket: &str) -> Result<(String, String)> {
        normalize_s3_connection_fields(endpoint, bucket)
    }

    fn validate_connection_credentials(input: &StorageConnectorConnectionInput) -> Result<()> {
        validate_static_secret_credentials(input, "tencent_cos")
    }

    fn supports_saved_draft_credentials() -> bool {
        true
    }

    async fn build_draft_driver<S: RemoteProtocolRuntimeState + Sync + ?Sized>(
        state: &S,
        policy: &storage_policy::Model,
    ) -> Result<Box<dyn StorageDriver>> {
        let _ = state;
        Ok(Box::new(TencentCosDriver::new(policy)?))
    }

    async fn execute_saved_action<S: SharedRuntimeState + Sync + ?Sized>(
        state: &S,
        policy: &storage_policy::Model,
        action: StoragePolicyExecutableAction,
    ) -> Result<StorageConnectorActionResult> {
        ensure_policy_action_supported(Self::storage_connector_descriptor(), action)?;
        match action {
            StoragePolicyExecutableAction::ConfigureTencentCosCors => {
                let result = configure_tencent_cos_cors_for_policy(state, policy).await?;
                Ok(StorageConnectorActionResult {
                    action,
                    tencent_cos_cors: Some(result),
                })
            }
        }
    }

    async fn execute_draft_action<S: RemoteProtocolRuntimeState + Sync + ?Sized>(
        state: &S,
        input: ExecuteDraftStorageConnectorActionInput,
    ) -> Result<StorageConnectorActionResult> {
        ensure_policy_action_supported(Self::storage_connector_descriptor(), input.action)?;
        match input.action {
            StoragePolicyExecutableAction::ConfigureTencentCosCors => {
                let connection =
                    merge_draft_action_saved_credentials(state, input.policy_id, input.connection)
                        .await?;
                let policy =
                    build_connection_test_policy::<Self, _>(state.writer_db(), connection).await?;
                let result = configure_tencent_cos_cors_for_policy(state, &policy).await?;
                Ok(StorageConnectorActionResult {
                    action: input.action,
                    tencent_cos_cors: Some(result),
                })
            }
        }
    }

    fn upload_transport(policy: &storage_policy::Model) -> StorageConnectorUploadTransport {
        let options = parse_storage_policy_options(policy.options.as_ref());
        StorageConnectorUploadTransport::ObjectStorage(options.effective_s3_upload_strategy())
    }

    fn presigned_download_enabled(policy: &storage_policy::Model) -> bool {
        let options = parse_storage_policy_options(policy.options.as_ref());
        options.effective_s3_download_strategy() == crate::types::S3DownloadStrategy::Presigned
    }
}

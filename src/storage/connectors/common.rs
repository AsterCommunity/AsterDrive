use chrono::Utc;
use sea_orm::ConnectionTrait;

use crate::api::api_error_code::ApiErrorCode;
use crate::entities::storage_policy;
use crate::errors::{AsterError, MapAsterErr, Result, validation_error_with_code};
use crate::storage::connector_descriptor::{
    StorageConnectorActionKind, StorageConnectorAffordanceAction, StorageConnectorDescriptor,
    StoragePolicyExecutableAction,
};
use crate::storage::drivers::s3_config::{S3ConfigError, normalize_s3_endpoint_and_bucket};
use crate::storage::error::storage_driver_error;
use crate::storage::{StorageDriver, StorageErrorKind};
use crate::types::{
    StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions, serialize_storage_policy_options,
};

use super::{StorageConnector, StorageConnectorConnectionInput};

pub(super) async fn normalize_policy_connection_for<T, C>(
    db: &C,
    input: StorageConnectorConnectionInput,
) -> Result<StorageConnectorConnectionInput>
where
    T: StorageConnector,
    C: ConnectionTrait + Sync,
{
    let (endpoint, bucket) = T::normalize_connection_fields(&input.endpoint, &input.bucket)?;
    let mut normalized = StorageConnectorConnectionInput {
        endpoint,
        bucket,
        options: input.options.normalized(),
        ..input
    };
    if normalized.driver_type != T::driver_type() {
        return Err(AsterError::internal_error(format!(
            "connector {:?} received connection for {:?}",
            T::driver_type(),
            normalized.driver_type
        )));
    }
    T::validate_connection_credentials(&normalized)?;
    normalized.remote_node_id = T::validate_connection_binding(db, &normalized).await?;
    Ok(normalized)
}

pub(super) async fn build_connection_test_policy<T, C>(
    db: &C,
    input: StorageConnectorConnectionInput,
) -> Result<storage_policy::Model>
where
    T: StorageConnector,
    C: ConnectionTrait + Sync,
{
    let input = normalize_policy_connection_for::<T, _>(db, input).await?;
    T::validate_policy_options(db, input.remote_node_id, &input.options).await?;
    Ok(storage_policy::Model {
        id: 0,
        name: String::new(),
        driver_type: input.driver_type,
        endpoint: input.endpoint,
        bucket: input.bucket,
        access_key: input.access_key,
        secret_key: input.secret_key,
        base_path: input.base_path,
        remote_node_id: input.remote_node_id,
        max_file_size: 0,
        allowed_types: StoredStoragePolicyAllowedTypes::empty(),
        options: serialize_connector_options(&input.options)?,
        is_default: false,
        chunk_size: 0,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    })
}

pub(super) fn normalize_s3_connection_fields(
    endpoint: &str,
    bucket: &str,
) -> Result<(String, String)> {
    let normalized =
        normalize_s3_endpoint_and_bucket(endpoint, bucket).map_err(|error| match error {
            S3ConfigError::MissingBucket => error
                .into_aster_error()
                .with_api_error_code(ApiErrorCode::PolicyStorageBucketRequired),
            S3ConfigError::InvalidEndpoint(_) => error
                .into_aster_error()
                .with_api_error_code(ApiErrorCode::PolicyStorageEndpointInvalid),
        })?;
    Ok((normalized.endpoint, normalized.bucket))
}

pub(super) fn reject_unexpected_remote_node(remote_node_id: Option<i64>) -> Result<Option<i64>> {
    if remote_node_id.is_some() {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyRemoteNodeUnexpected,
            "remote_node_id is only valid for remote storage policies",
        ));
    }
    Ok(None)
}

fn validate_connection_secret(value: &str, field: &str, driver: &str) -> Result<()> {
    if value.trim().is_empty() {
        let api_code = match field {
            "access_key" => ApiErrorCode::PolicyStorageAccessKeyRequired,
            "secret_key" => ApiErrorCode::PolicyStorageSecretKeyRequired,
            _ => ApiErrorCode::BadRequest,
        };
        return Err(validation_error_with_code(
            api_code,
            format!("{field} is required for {driver} storage policies"),
        ));
    }
    Ok(())
}

pub(super) fn validate_static_secret_credentials(
    input: &StorageConnectorConnectionInput,
    driver: &str,
) -> Result<()> {
    validate_connection_secret(&input.access_key, "access_key", driver)?;
    validate_connection_secret(&input.secret_key, "secret_key", driver)
}

fn has_onedrive_options(options: &crate::types::StoragePolicyOptions) -> bool {
    options.onedrive_cloud.is_some()
        || options.onedrive_account_mode.is_some()
        || options.onedrive_tenant.is_some()
        || options.onedrive_drive_id.is_some()
        || options.onedrive_root_item_id.is_some()
        || options.onedrive_site_id.is_some()
        || options.onedrive_group_id.is_some()
}

pub(super) fn ensure_onedrive_options_absent(
    options: &crate::types::StoragePolicyOptions,
) -> Result<()> {
    if has_onedrive_options(options) {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyOneDriveOptionsUnsupported,
            "OneDrive options are only valid for OneDrive storage policies",
        ));
    }
    Ok(())
}

pub(super) fn validate_onedrive_options(
    options: &crate::types::StoragePolicyOptions,
) -> Result<()> {
    if options.onedrive_account_mode.is_none() {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyOneDriveAccountModeRequired,
            "OneDrive storage policies require onedrive_account_mode",
        ));
    }
    if options.onedrive_cloud == Some(crate::types::MicrosoftGraphCloud::China)
        && options.onedrive_account_mode == Some(crate::types::OneDriveAccountMode::Personal)
    {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyOneDrivePersonalChinaCloudUnsupported,
            "personal OneDrive accounts must use the global Microsoft Graph cloud",
        ));
    }
    if options.onedrive_account_mode == Some(crate::types::OneDriveAccountMode::SharepointSite)
        && options.onedrive_drive_id.is_none()
        && options.onedrive_site_id.is_none()
    {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyOneDriveSharePointSiteRequired,
            "OneDrive sharepoint_site policies require onedrive_site_id when onedrive_drive_id is not set",
        ));
    }
    if options.onedrive_account_mode == Some(crate::types::OneDriveAccountMode::SharepointSite)
        && options.onedrive_group_id.is_some()
    {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyOneDriveOptionsUnsupported,
            "onedrive_group_id is only valid for OneDrive group_drive policies",
        ));
    }
    if options.onedrive_account_mode == Some(crate::types::OneDriveAccountMode::GroupDrive)
        && options.onedrive_drive_id.is_none()
        && options.onedrive_group_id.is_none()
    {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyOneDriveGroupRequired,
            "OneDrive group_drive policies require onedrive_group_id when onedrive_drive_id is not set",
        ));
    }
    if options.onedrive_account_mode == Some(crate::types::OneDriveAccountMode::GroupDrive)
        && options.onedrive_site_id.is_some()
    {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyOneDriveOptionsUnsupported,
            "onedrive_site_id is only valid for OneDrive sharepoint_site policies",
        ));
    }
    Ok(())
}

pub(super) fn ensure_storage_native_processing_supported(
    descriptor: StorageConnectorDescriptor,
    options: &crate::types::StoragePolicyOptions,
) -> Result<()> {
    if options.uses_storage_native_thumbnail() && !descriptor.capabilities.storage_native_thumbnail
    {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyNativeThumbnailUnsupported,
            format!(
                "storage policy driver '{}' does not expose storage-native thumbnail processing",
                descriptor.driver_type.as_str()
            ),
        ));
    }
    if options.uses_storage_native_media_metadata()
        && !descriptor.capabilities.storage_native_media_metadata
    {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyNativeMediaMetadataUnsupported,
            format!(
                "storage policy driver '{}' does not expose storage-native media metadata processing",
                descriptor.driver_type.as_str()
            ),
        ));
    }
    Ok(())
}

pub(super) fn ensure_policy_action_supported(
    descriptor: StorageConnectorDescriptor,
    action: StoragePolicyExecutableAction,
) -> Result<()> {
    if descriptor.actions.iter().any(|descriptor_action| {
        descriptor_action.kind == StorageConnectorActionKind::PolicyAction
            && descriptor_action.policy_action == Some(action)
    }) {
        return Ok(());
    }
    Err(unsupported_policy_action_error(descriptor, action))
}

pub(super) fn unsupported_policy_action_error(
    descriptor: StorageConnectorDescriptor,
    action: StoragePolicyExecutableAction,
) -> AsterError {
    validation_error_with_code(
        ApiErrorCode::PolicyActionUnsupported,
        format!(
            "storage policy action '{}' is not supported for {} storage policies",
            action.as_str(),
            descriptor.driver_type.as_str()
        ),
    )
}

pub(super) fn unsupported_draft_connection_test_error(
    descriptor: StorageConnectorDescriptor,
) -> AsterError {
    if descriptor.actions.iter().any(|action| {
        action.affordance_action == Some(StorageConnectorAffordanceAction::TestSavedConnection)
            && action.kind == StorageConnectorActionKind::ConnectionTest
            && action.requires_saved_policy
            && action.requires_authorization
    }) {
        return validation_error_with_code(
            ApiErrorCode::PolicyActionUnsupported,
            format!(
                "storage policy driver '{}' requires a saved storage policy with completed authorization; use the saved policy connection test after authorization",
                descriptor.driver_type.as_str(),
            ),
        );
    }
    validation_error_with_code(
        ApiErrorCode::PolicyActionUnsupported,
        format!(
            "storage policy driver '{}' does not support draft connection tests",
            descriptor.driver_type.as_str(),
        ),
    )
}

pub(super) fn unsupported_saved_connection_test_error(
    descriptor: StorageConnectorDescriptor,
) -> AsterError {
    validation_error_with_code(
        ApiErrorCode::PolicyActionUnsupported,
        format!(
            "storage policy driver '{}' does not support saved-policy connection tests",
            descriptor.driver_type.as_str(),
        ),
    )
}

fn serialize_connector_options(
    options: &crate::types::StoragePolicyOptions,
) -> Result<StoredStoragePolicyOptions> {
    serialize_storage_policy_options(options).map_err(|error| {
        AsterError::internal_error(format!("serialize storage policy options: {error}"))
    })
}

pub(super) async fn probe_storage_driver(
    driver: &dyn StorageDriver,
    write_error_context: &'static str,
) -> Result<()> {
    let test_path = format!("_aster_connection_test-{}", uuid::Uuid::new_v4());
    driver
        .put(&test_path, b"ok")
        .await
        .map_aster_err_ctx(write_error_context, AsterError::storage_driver_error)?;
    driver
        .delete(&test_path)
        .await
        .inspect_err(|error| {
            tracing::warn!(path = %test_path, "failed to clean up connection test file: {error}");
        })
        .map_aster_err_ctx(
            "connection test cleanup failed",
            AsterError::storage_driver_error,
        )?;
    Ok(())
}

pub fn unsupported_multipart_error(policy: &storage_policy::Model) -> AsterError {
    storage_driver_error(
        StorageErrorKind::Unsupported,
        format!(
            "storage policy {} (driver: {:?}) does not support multipart upload",
            policy.id, policy.driver_type
        ),
    )
}

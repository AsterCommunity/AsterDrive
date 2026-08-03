//! Storage connector definitions for policy configuration and admin actions.
//!
//! Connectors own configuration-time behavior: descriptors, connection field
//! normalization, credential requirements, draft/saved connection tests, and
//! connector-specific admin actions. Runtime object operations remain in
//! `StorageDriver` implementations.
//!
//! 简单说：`StorageConnector` 管“怎么把 policy 配好并告诉管理端这个 driver 能做什么”，
//! `StorageDriver` 管“policy 已经配好后怎么读写对象”。如果一段逻辑需要数据库、
//! OAuth、表单字段、连接测试或策略动作，它通常属于 connector，而不是 driver。

mod azure_blob;
mod common;
mod contract;
mod local;
mod models;
mod onedrive;
mod remote;
mod s3;
mod sftp;
mod tencent_cos;
mod upload;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use crate::errors::Result;
use crate::runtime::{RemoteProtocolRuntimeState, SharedRuntimeState};
use aster_drive_model::entities::storage_policy;
use aster_drive_model::types::{DriverType, StorageCredentialKind, StorageCredentialProvider};
use aster_drive_storage::StorageDriver;
use aster_drive_storage::connector_descriptor::{
    StorageConnectorActionKind, StorageConnectorAffordanceAction, StorageConnectorDescriptor,
    StorageConnectorObjectNamingMode, StoragePolicyExecutableAction,
};

use azure_blob::AzureBlobConnector;
pub use common::unsupported_multipart_error;
use local::LocalConnector;
pub use models::{
    ExecuteDraftStorageConnectorActionInput, ExecuteSavedStorageConnectorActionInput,
    MicrosoftGraphApplicationConfigInput, StorageConnectorActionResult,
    StorageConnectorApplicationConfigInput, StorageConnectorConnectionInput,
    TencentCosCorsConfigResult, TestDraftStorageConnectorConnectionInput,
};
pub(crate) use models::{
    StorageConnectorCredentialRequirement, StorageConnectorRuntimeCredential,
    StorageCredentialValidationOutcome, StoragePolicyCleanupDriverSnapshot,
    StoragePolicyCleanupOneDriveCredentialSnapshot, StoragePolicyCleanupRemoteNodeSnapshot,
    StoragePolicyCleanupSnapshots,
};
use onedrive::OneDriveConnector;
use remote::RemoteConnector;
use s3::S3Connector;
use sftp::SftpConnector;
use tencent_cos::TencentCosConnector;
pub use upload::StorageConnectorUploadTransport;

pub(crate) use contract::{
    StorageConnector, StorageConnectorContext, StorageConnectorDriver, StorageConnectorRegistry,
    connector_id_for_legacy_driver_type,
};

pub(crate) fn builtin_storage_connector_registry() -> Result<StorageConnectorRegistry> {
    StorageConnectorRegistry::new(vec![
        Arc::new(LocalConnector),
        Arc::new(S3Connector),
        Arc::new(SftpConnector),
        Arc::new(AzureBlobConnector),
        Arc::new(TencentCosConnector),
        Arc::new(RemoteConnector),
        Arc::new(OneDriveConnector),
    ])
}

pub(crate) fn shared_connector_context<'a>(
    state: &'a (impl SharedRuntimeState + Sync),
) -> StorageConnectorContext<'a> {
    StorageConnectorContext::new(
        state.writer_db(),
        state.config(),
        state.runtime_config(),
        state.driver_registry(),
        None,
    )
}

pub(crate) fn remote_connector_context<'a>(
    state: &'a (impl RemoteProtocolRuntimeState + Sync),
) -> StorageConnectorContext<'a> {
    StorageConnectorContext::new(
        state.writer_db(),
        state.config(),
        state.runtime_config(),
        state.driver_registry(),
        Some(state.remote_protocol()),
    )
}

pub(crate) fn list_storage_driver_descriptors(
    registry: &StorageConnectorRegistry,
) -> Vec<StorageConnectorDescriptor> {
    registry.descriptors()
}

pub(crate) fn storage_driver_descriptor(
    registry: &StorageConnectorRegistry,
    driver_type: DriverType,
) -> Result<StorageConnectorDescriptor> {
    Ok(registry.require(driver_type)?.descriptor())
}

pub(crate) fn storage_connector_supports_native_thumbnail(
    registry: &StorageConnectorRegistry,
    driver_type: DriverType,
) -> Result<bool> {
    Ok(storage_driver_descriptor(registry, driver_type)?
        .capabilities
        .storage_native_thumbnail)
}

pub(crate) fn storage_connector_supports_native_media_metadata(
    registry: &StorageConnectorRegistry,
    driver_type: DriverType,
) -> Result<bool> {
    Ok(storage_driver_descriptor(registry, driver_type)?
        .capabilities
        .storage_native_media_metadata)
}

pub(crate) fn ensure_storage_authorization_supported(
    registry: &StorageConnectorRegistry,
    driver_type: DriverType,
    provider: StorageCredentialProvider,
) -> Result<StorageCredentialKind> {
    let descriptor = storage_driver_descriptor(registry, driver_type)?;
    let starts_authorization = descriptor.actions.iter().any(|action| {
        action.affordance_action == Some(StorageConnectorAffordanceAction::StartAuthorization)
            && action.kind == StorageConnectorActionKind::Authorization
    });
    let supported_provider = descriptor
        .authorization_provider
        .as_deref()
        .and_then(|provider| provider.parse().ok());
    if starts_authorization && supported_provider == Some(provider) {
        return Ok(StorageCredentialKind::OauthDelegated);
    }
    Err(crate::errors::AsterError::unsupported_driver(format!(
        "storage credential authorization provider '{}' is not supported for {} storage policies",
        provider.as_str(),
        driver_type.as_str()
    )))
}

/// Gate credential validation through connector-declared actions so credential
/// services never need to know which storage drivers expose validation.
pub(crate) fn ensure_storage_credential_validation_supported(
    registry: &StorageConnectorRegistry,
    driver_type: DriverType,
    provider: StorageCredentialProvider,
) -> Result<StorageCredentialKind> {
    let descriptor = storage_driver_descriptor(registry, driver_type)?;
    let validates_credential = descriptor.actions.iter().any(|action| {
        action.affordance_action == Some(StorageConnectorAffordanceAction::ValidateCredential)
            && action.kind == StorageConnectorActionKind::CredentialValidation
    });
    let supported_provider = descriptor
        .authorization_provider
        .as_deref()
        .and_then(|provider| provider.parse().ok());
    if validates_credential && supported_provider == Some(provider) {
        return Ok(StorageCredentialKind::OauthDelegated);
    }
    Err(crate::errors::AsterError::unsupported_driver(format!(
        "storage credential validation provider '{}' is not supported for {} storage policies",
        provider.as_str(),
        driver_type.as_str()
    )))
}

pub(crate) async fn normalize_policy_connection(
    registry: &StorageConnectorRegistry,
    db: &sea_orm::DatabaseConnection,
    input: StorageConnectorConnectionInput,
) -> Result<StorageConnectorConnectionInput> {
    common::normalize_policy_connection(db, registry.require(input.driver_type)?, input).await
}

pub(crate) fn prepare_connection_for_storage(
    registry: &StorageConnectorRegistry,
    input: StorageConnectorConnectionInput,
    application_config: &StorageConnectorApplicationConfigInput,
) -> Result<StorageConnectorConnectionInput> {
    registry
        .require(input.driver_type)?
        .prepare_connection_for_storage(input, application_config)
}

pub(crate) async fn validate_policy_options(
    registry: &StorageConnectorRegistry,
    db: &sea_orm::DatabaseConnection,
    driver_type: DriverType,
    remote_node_id: Option<i64>,
    options: &aster_drive_model::types::StoragePolicyOptions,
) -> Result<()> {
    registry
        .require(driver_type)?
        .validate_policy_options(db, remote_node_id, options)
        .await
}

pub(crate) async fn persist_application_config(
    registry: &StorageConnectorRegistry,
    db: &sea_orm::DatabaseTransaction,
    driver_type: DriverType,
    encryption_key: &str,
    policy_id: i64,
    options: &aster_drive_model::types::StoragePolicyOptions,
    application_config: StorageConnectorApplicationConfigInput,
) -> Result<()> {
    registry
        .require(driver_type)?
        .persist_application_config(db, encryption_key, policy_id, options, application_config)
        .await
}

pub(crate) async fn test_draft_connection<S: RemoteProtocolRuntimeState + Sync>(
    registry: &StorageConnectorRegistry,
    state: &S,
    input: TestDraftStorageConnectorConnectionInput,
) -> Result<()> {
    let context = remote_connector_context(state);
    registry
        .require(input.connection.driver_type)?
        .test_draft_connection(&context, input)
        .await
}

pub(crate) async fn test_saved_connection<S: SharedRuntimeState + Sync>(
    registry: &StorageConnectorRegistry,
    state: &S,
    policy: &storage_policy::Model,
) -> Result<()> {
    registry
        .require(policy.driver_type)?
        .test_saved_connection(&shared_connector_context(state), policy)
        .await
}

pub(crate) async fn execute_saved_action<S: SharedRuntimeState + Sync>(
    registry: &StorageConnectorRegistry,
    state: &S,
    policy: &storage_policy::Model,
    action: StoragePolicyExecutableAction,
) -> Result<StorageConnectorActionResult> {
    registry
        .require(policy.driver_type)?
        .execute_saved_action(&shared_connector_context(state), policy, action)
        .await
}

pub(crate) async fn execute_draft_action<S: RemoteProtocolRuntimeState + Sync>(
    registry: &StorageConnectorRegistry,
    state: &S,
    input: ExecuteDraftStorageConnectorActionInput,
) -> Result<StorageConnectorActionResult> {
    registry
        .require(input.connection.driver_type)?
        .execute_draft_action(&remote_connector_context(state), input)
        .await
}

pub(crate) fn validate_driver_promotion_source(
    registry: &StorageConnectorRegistry,
    source: DriverType,
) -> Result<()> {
    if !storage_driver_descriptor(registry, source)?
        .driver_recommendations
        .is_empty()
    {
        return Ok(());
    }
    Err(crate::errors::validation_error_with_code(
        crate::api::api_error_code::ApiErrorCode::PolicyPromotionSourceUnsupported,
        "only generic S3-compatible policies can be promoted",
    ))
}

pub(crate) fn validate_driver_promotion_target(
    registry: &StorageConnectorRegistry,
    source: DriverType,
    target: DriverType,
) -> Result<()> {
    if storage_driver_descriptor(registry, source)?
        .driver_recommendations
        .iter()
        .any(|recommendation| {
            recommendation.target_connector_id
                == contract::connector_id_for_legacy_driver_type(target)
        })
    {
        return Ok(());
    }
    Err(crate::errors::validation_error_with_code(
        crate::api::api_error_code::ApiErrorCode::PolicyPromotionTargetUnsupported,
        format!(
            "promoting S3-compatible policy to '{}' is not supported",
            target.as_str()
        ),
    ))
}

pub(crate) fn validate_driver_promotion_candidate(
    registry: &StorageConnectorRegistry,
    policy: &storage_policy::Model,
) -> Result<()> {
    registry
        .require(policy.driver_type)?
        .validate_promotion_candidate(policy)
}

pub(crate) fn resolve_policy_upload_transport(
    registry: &StorageConnectorRegistry,
    policy: &storage_policy::Model,
) -> Result<StorageConnectorUploadTransport> {
    Ok(registry
        .require(policy.driver_type)?
        .upload_transport(policy))
}

pub(crate) fn resolve_policy_object_naming(
    registry: &StorageConnectorRegistry,
    policy: &storage_policy::Model,
) -> Result<StorageConnectorObjectNamingMode> {
    registry.object_naming(policy)
}

pub(crate) fn presigned_download_enabled(
    registry: &StorageConnectorRegistry,
    policy: &storage_policy::Model,
) -> Result<bool> {
    Ok(registry
        .require(policy.driver_type)?
        .presigned_download_enabled(policy))
}

pub(crate) fn presigned_download_requires_filename_match(
    registry: &StorageConnectorRegistry,
    policy: &storage_policy::Model,
) -> Result<bool> {
    Ok(registry
        .require(policy.driver_type)?
        .presigned_download_requires_filename_match(policy))
}

pub(crate) fn runtime_credential_requirement(
    registry: &StorageConnectorRegistry,
    driver_type: DriverType,
) -> Result<Option<StorageConnectorCredentialRequirement>> {
    Ok(registry
        .require(driver_type)?
        .runtime_credential_requirement())
}

pub(crate) async fn load_runtime_credential(
    registry: &StorageConnectorRegistry,
    db: &sea_orm::DatabaseConnection,
    config: &crate::config::Config,
    policy: &storage_policy::Model,
    credential: &aster_drive_model::entities::storage_policy_credential::Model,
) -> Result<Option<StorageConnectorRuntimeCredential>> {
    registry
        .require(policy.driver_type)?
        .load_runtime_credential(db, config, policy, credential)
        .await
}

pub(crate) async fn validate_credential(
    registry: &StorageConnectorRegistry,
    db: &sea_orm::DatabaseConnection,
    config: &crate::config::Config,
    policy: &storage_policy::Model,
    credential: &aster_drive_model::entities::storage_policy_credential::Model,
) -> Result<StorageCredentialValidationOutcome> {
    registry
        .require(policy.driver_type)?
        .validate_credential(db, config, policy, credential)
        .await
}

pub(crate) fn streaming_direct_upload_eligible(
    registry: &StorageConnectorRegistry,
    policy: &storage_policy::Model,
    declared_size: i64,
) -> Result<bool> {
    Ok(resolve_policy_upload_transport(registry, policy)?
        .supports_streaming_direct_upload(policy, declared_size))
}

pub(crate) async fn cleanup_snapshot_for_policy<S: SharedRuntimeState + Sync>(
    registry: &StorageConnectorRegistry,
    state: &S,
    policy: &storage_policy::Model,
) -> Result<Option<StoragePolicyCleanupDriverSnapshot>> {
    registry
        .require(policy.driver_type)?
        .cleanup_snapshot_for_policy(&shared_connector_context(state), policy)
        .await
}

pub(crate) fn can_create_cleanup_task_with_snapshot(
    registry: &StorageConnectorRegistry,
    driver_type: DriverType,
    driver_snapshot: &Option<StoragePolicyCleanupDriverSnapshot>,
) -> bool {
    registry
        .require(driver_type)
        .map(|connector| !connector.cleanup_snapshot_required() || driver_snapshot.is_some())
        .unwrap_or(false)
}

pub(crate) async fn build_cleanup_driver<S: RemoteProtocolRuntimeState + Sync>(
    registry: &StorageConnectorRegistry,
    state: &S,
    policy: &storage_policy::Model,
    snapshots: StoragePolicyCleanupSnapshots<'_>,
) -> Result<Arc<dyn StorageDriver>> {
    registry
        .require(policy.driver_type)?
        .build_cleanup_driver(&remote_connector_context(state), policy, snapshots)
        .await
}

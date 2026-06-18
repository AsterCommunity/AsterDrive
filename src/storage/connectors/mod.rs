//! Storage connector definitions for policy configuration and admin actions.
//!
//! Connectors own configuration-time behavior: descriptors, connection field
//! normalization, credential requirements, draft/saved connection tests, and
//! connector-specific admin actions. Runtime object operations remain in
//! `StorageDriver` implementations.

mod azure_blob;
mod common;
mod local;
mod models;
mod onedrive;
mod remote;
mod s3;
mod tencent_cos;
mod upload;

#[cfg(test)]
mod tests;

use async_trait::async_trait;
use sea_orm::ConnectionTrait;
use std::sync::Arc;

use crate::entities::storage_policy;
use crate::errors::Result;
use crate::runtime::{RemoteProtocolRuntimeState, SharedRuntimeState};
use crate::storage::StorageDriver;
use crate::storage::connector_descriptor::{
    StorageConnectorAction, StorageConnectorActionKind, StorageConnectorDescriptor,
    StorageConnectorDescriptorProvider, StoragePolicyExecutableAction,
};
use crate::storage::drivers::{
    azure_blob::AzureBlobDriver, local::LocalDriver, s3::S3Driver, tencent_cos::TencentCosDriver,
};
use crate::types::{DriverType, StorageCredentialKind, StorageCredentialProvider};

use azure_blob::AzureBlobConnector;
pub use common::unsupported_multipart_error;
use local::LocalConnector;
pub use models::{
    ExecuteDraftStorageConnectorActionInput, ExecuteSavedStorageConnectorActionInput,
    StorageConnectorActionResult, StorageConnectorConnectionInput, TencentCosCorsConfigResult,
};
pub(crate) use models::{
    StoragePolicyCleanupDriverSnapshot, StoragePolicyCleanupOneDriveCredentialSnapshot,
    StoragePolicyCleanupRemoteNodeSnapshot, StoragePolicyCleanupSnapshots,
};
use onedrive::OneDriveConnector;
use remote::RemoteConnector;
use s3::S3Connector;
use tencent_cos::TencentCosConnector;
pub use upload::StorageConnectorUploadTransport;

#[async_trait(?Send)]
trait StorageConnector: StorageConnectorDescriptorProvider + Send + Sync + Sized {
    fn driver_type() -> DriverType;

    fn normalize_connection_fields(endpoint: &str, bucket: &str) -> Result<(String, String)>;

    fn validate_connection_credentials(input: &StorageConnectorConnectionInput) -> Result<()>;

    async fn validate_connection_binding<C: ConnectionTrait + Sync>(
        _db: &C,
        input: &StorageConnectorConnectionInput,
    ) -> Result<Option<i64>> {
        common::reject_unexpected_remote_node(input.remote_node_id)
    }

    async fn validate_policy_options<C: ConnectionTrait + Sync>(
        db: &C,
        remote_node_id: Option<i64>,
        options: &crate::types::StoragePolicyOptions,
    ) -> Result<()> {
        let _ = (db, remote_node_id);
        common::ensure_storage_native_processing_supported(
            Self::storage_connector_descriptor(),
            options,
        )?;
        common::ensure_onedrive_options_absent(options)
    }

    async fn build_draft_driver<S: RemoteProtocolRuntimeState + Sync + ?Sized>(
        state: &S,
        policy: &storage_policy::Model,
    ) -> Result<Box<dyn StorageDriver>>;

    fn upload_transport(policy: &storage_policy::Model) -> StorageConnectorUploadTransport;

    fn presigned_download_enabled(policy: &storage_policy::Model) -> bool {
        let _ = policy;
        false
    }

    async fn test_draft_connection<S: RemoteProtocolRuntimeState + Sync + ?Sized>(
        state: &S,
        input: StorageConnectorConnectionInput,
    ) -> Result<()> {
        if !Self::storage_connector_supports_draft_connection_test() {
            return Err(common::unsupported_draft_connection_test_error(
                Self::storage_connector_descriptor(),
            ));
        }
        let policy =
            common::build_connection_test_policy::<Self, _>(state.writer_db(), input).await?;
        let driver = Self::build_draft_driver(state, &policy).await?;
        common::probe_storage_driver(driver.as_ref(), "connection test failed").await
    }

    async fn test_saved_connection<S: SharedRuntimeState + Sync + ?Sized>(
        state: &S,
        policy: &storage_policy::Model,
    ) -> Result<()> {
        if !Self::storage_connector_supports_saved_connection_test() {
            return Err(common::unsupported_saved_connection_test_error(
                Self::storage_connector_descriptor(),
            ));
        }
        let driver = state.driver_registry().get_driver(policy)?;
        common::probe_storage_driver(driver.as_ref(), "write test failed").await
    }

    async fn execute_saved_action<S: SharedRuntimeState + Sync + ?Sized>(
        state: &S,
        policy: &storage_policy::Model,
        action: StoragePolicyExecutableAction,
    ) -> Result<StorageConnectorActionResult> {
        let _ = (state, policy);
        Err(common::unsupported_policy_action_error(
            Self::storage_connector_descriptor(),
            action,
        ))
    }

    async fn execute_draft_action<S: RemoteProtocolRuntimeState + Sync + ?Sized>(
        state: &S,
        input: ExecuteDraftStorageConnectorActionInput,
    ) -> Result<StorageConnectorActionResult> {
        let _ = state;
        Err(common::unsupported_policy_action_error(
            Self::storage_connector_descriptor(),
            input.action,
        ))
    }
}

/// Static built-in connector registry.
///
/// Keep configuration-time behavior here instead of on `StorageDriver`: drivers
/// are already-built object operators, while connectors know how to validate,
/// authorize, snapshot, and rebuild those drivers for admin/task workflows.
/// Issue #212 can extend this registry with plugin-provided registrations
/// without adding new `match DriverType` dispatch sites.
struct StorageConnectorRegistration {
    driver_type: DriverType,
    connector: BuiltinStorageConnector,
    cleanup_snapshot_required: bool,
    promotion_targets: &'static [DriverType],
}

const S3_PROMOTION_TARGETS: &[DriverType] = &[DriverType::TencentCos];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuiltinStorageConnector {
    Local,
    S3,
    AzureBlob,
    TencentCos,
    Remote,
    OneDrive,
}

impl BuiltinStorageConnector {
    fn descriptor(self) -> StorageConnectorDescriptor {
        match self {
            Self::Local => LocalConnector::storage_connector_descriptor(),
            Self::S3 => S3Connector::storage_connector_descriptor(),
            Self::AzureBlob => AzureBlobConnector::storage_connector_descriptor(),
            Self::TencentCos => TencentCosConnector::storage_connector_descriptor(),
            Self::Remote => RemoteConnector::storage_connector_descriptor(),
            Self::OneDrive => OneDriveConnector::storage_connector_descriptor(),
        }
    }

    async fn normalize_policy_connection<C: ConnectionTrait + Sync>(
        self,
        db: &C,
        input: StorageConnectorConnectionInput,
    ) -> Result<StorageConnectorConnectionInput> {
        match self {
            Self::Local => {
                common::normalize_policy_connection_for::<LocalConnector, _>(db, input).await
            }
            Self::S3 => common::normalize_policy_connection_for::<S3Connector, _>(db, input).await,
            Self::AzureBlob => {
                common::normalize_policy_connection_for::<AzureBlobConnector, _>(db, input).await
            }
            Self::TencentCos => {
                common::normalize_policy_connection_for::<TencentCosConnector, _>(db, input).await
            }
            Self::Remote => {
                common::normalize_policy_connection_for::<RemoteConnector, _>(db, input).await
            }
            Self::OneDrive => {
                common::normalize_policy_connection_for::<OneDriveConnector, _>(db, input).await
            }
        }
    }

    async fn validate_policy_options<C: ConnectionTrait + Sync>(
        self,
        db: &C,
        remote_node_id: Option<i64>,
        options: &crate::types::StoragePolicyOptions,
    ) -> Result<()> {
        match self {
            Self::Local => {
                LocalConnector::validate_policy_options(db, remote_node_id, options).await
            }
            Self::S3 => S3Connector::validate_policy_options(db, remote_node_id, options).await,
            Self::AzureBlob => {
                AzureBlobConnector::validate_policy_options(db, remote_node_id, options).await
            }
            Self::TencentCos => {
                TencentCosConnector::validate_policy_options(db, remote_node_id, options).await
            }
            Self::Remote => {
                RemoteConnector::validate_policy_options(db, remote_node_id, options).await
            }
            Self::OneDrive => {
                OneDriveConnector::validate_policy_options(db, remote_node_id, options).await
            }
        }
    }

    async fn test_draft_connection<S: RemoteProtocolRuntimeState + Sync + ?Sized>(
        self,
        state: &S,
        input: StorageConnectorConnectionInput,
    ) -> Result<()> {
        match self {
            Self::Local => LocalConnector::test_draft_connection(state, input).await,
            Self::S3 => S3Connector::test_draft_connection(state, input).await,
            Self::AzureBlob => AzureBlobConnector::test_draft_connection(state, input).await,
            Self::TencentCos => TencentCosConnector::test_draft_connection(state, input).await,
            Self::Remote => RemoteConnector::test_draft_connection(state, input).await,
            Self::OneDrive => OneDriveConnector::test_draft_connection(state, input).await,
        }
    }

    async fn test_saved_connection<S: SharedRuntimeState + Sync + ?Sized>(
        self,
        state: &S,
        policy: &storage_policy::Model,
    ) -> Result<()> {
        match self {
            Self::Local => LocalConnector::test_saved_connection(state, policy).await,
            Self::S3 => S3Connector::test_saved_connection(state, policy).await,
            Self::AzureBlob => AzureBlobConnector::test_saved_connection(state, policy).await,
            Self::TencentCos => TencentCosConnector::test_saved_connection(state, policy).await,
            Self::Remote => RemoteConnector::test_saved_connection(state, policy).await,
            Self::OneDrive => OneDriveConnector::test_saved_connection(state, policy).await,
        }
    }

    async fn execute_saved_action<S: SharedRuntimeState + Sync + ?Sized>(
        self,
        state: &S,
        policy: &storage_policy::Model,
        action: StoragePolicyExecutableAction,
    ) -> Result<StorageConnectorActionResult> {
        match self {
            Self::Local => LocalConnector::execute_saved_action(state, policy, action).await,
            Self::S3 => S3Connector::execute_saved_action(state, policy, action).await,
            Self::AzureBlob => {
                AzureBlobConnector::execute_saved_action(state, policy, action).await
            }
            Self::TencentCos => {
                TencentCosConnector::execute_saved_action(state, policy, action).await
            }
            Self::Remote => RemoteConnector::execute_saved_action(state, policy, action).await,
            Self::OneDrive => OneDriveConnector::execute_saved_action(state, policy, action).await,
        }
    }

    async fn execute_draft_action<S: RemoteProtocolRuntimeState + Sync + ?Sized>(
        self,
        state: &S,
        input: ExecuteDraftStorageConnectorActionInput,
    ) -> Result<StorageConnectorActionResult> {
        match self {
            Self::Local => LocalConnector::execute_draft_action(state, input).await,
            Self::S3 => S3Connector::execute_draft_action(state, input).await,
            Self::AzureBlob => AzureBlobConnector::execute_draft_action(state, input).await,
            Self::TencentCos => TencentCosConnector::execute_draft_action(state, input).await,
            Self::Remote => RemoteConnector::execute_draft_action(state, input).await,
            Self::OneDrive => OneDriveConnector::execute_draft_action(state, input).await,
        }
    }

    fn upload_transport(self, policy: &storage_policy::Model) -> StorageConnectorUploadTransport {
        match self {
            Self::Local => LocalConnector::upload_transport(policy),
            Self::S3 => S3Connector::upload_transport(policy),
            Self::AzureBlob => AzureBlobConnector::upload_transport(policy),
            Self::TencentCos => TencentCosConnector::upload_transport(policy),
            Self::Remote => RemoteConnector::upload_transport(policy),
            Self::OneDrive => OneDriveConnector::upload_transport(policy),
        }
    }

    fn presigned_download_enabled(self, policy: &storage_policy::Model) -> bool {
        match self {
            Self::Local => LocalConnector::presigned_download_enabled(policy),
            Self::S3 => S3Connector::presigned_download_enabled(policy),
            Self::AzureBlob => AzureBlobConnector::presigned_download_enabled(policy),
            Self::TencentCos => TencentCosConnector::presigned_download_enabled(policy),
            Self::Remote => RemoteConnector::presigned_download_enabled(policy),
            Self::OneDrive => OneDriveConnector::presigned_download_enabled(policy),
        }
    }

    async fn cleanup_snapshot_for_policy<S: SharedRuntimeState + Sync + ?Sized>(
        self,
        state: &S,
        policy: &storage_policy::Model,
    ) -> Result<Option<StoragePolicyCleanupDriverSnapshot>> {
        match self {
            Self::Remote => RemoteConnector::cleanup_snapshot_for_policy(state, policy).await,
            Self::OneDrive => OneDriveConnector::cleanup_snapshot_for_policy(state, policy).await,
            Self::Local | Self::S3 | Self::AzureBlob | Self::TencentCos => Ok(None),
        }
    }

    async fn build_cleanup_driver<S: RemoteProtocolRuntimeState + Sync + ?Sized>(
        self,
        state: &S,
        policy: &storage_policy::Model,
        snapshots: StoragePolicyCleanupSnapshots<'_>,
    ) -> Result<Arc<dyn StorageDriver>> {
        match self {
            Self::Local => Ok(Arc::new(LocalDriver::new(policy)?)),
            Self::S3 => Ok(Arc::new(S3Driver::new(policy)?)),
            Self::AzureBlob => Ok(Arc::new(AzureBlobDriver::new(policy)?)),
            Self::TencentCos => Ok(Arc::new(TencentCosDriver::new(policy)?)),
            Self::Remote => RemoteConnector::build_cleanup_driver(state, policy, snapshots).await,
            Self::OneDrive => {
                OneDriveConnector::build_cleanup_driver(state, policy, snapshots).await
            }
        }
    }

    fn validate_promotion_candidate(self, policy: &storage_policy::Model) -> Result<()> {
        match self {
            Self::TencentCos => TencentCosConnector::validate_promotion_candidate(policy),
            _ => Err(crate::errors::validation_error_with_code(
                crate::api::api_error_code::ApiErrorCode::PolicyPromotionTargetUnsupported,
                format!(
                    "promoting S3-compatible policy to '{}' is not supported",
                    policy.driver_type.as_str()
                ),
            )),
        }
    }
}

static CONNECTOR_REGISTRATIONS: &[StorageConnectorRegistration] = &[
    StorageConnectorRegistration {
        driver_type: DriverType::Local,
        connector: BuiltinStorageConnector::Local,
        cleanup_snapshot_required: false,
        promotion_targets: &[],
    },
    StorageConnectorRegistration {
        driver_type: DriverType::S3,
        connector: BuiltinStorageConnector::S3,
        cleanup_snapshot_required: false,
        promotion_targets: S3_PROMOTION_TARGETS,
    },
    StorageConnectorRegistration {
        driver_type: DriverType::AzureBlob,
        connector: BuiltinStorageConnector::AzureBlob,
        cleanup_snapshot_required: false,
        promotion_targets: &[],
    },
    StorageConnectorRegistration {
        driver_type: DriverType::TencentCos,
        connector: BuiltinStorageConnector::TencentCos,
        cleanup_snapshot_required: false,
        promotion_targets: &[],
    },
    StorageConnectorRegistration {
        driver_type: DriverType::Remote,
        connector: BuiltinStorageConnector::Remote,
        cleanup_snapshot_required: false,
        promotion_targets: &[],
    },
    StorageConnectorRegistration {
        driver_type: DriverType::OneDrive,
        connector: BuiltinStorageConnector::OneDrive,
        cleanup_snapshot_required: true,
        promotion_targets: &[],
    },
];

fn connector_for(driver_type: DriverType) -> Result<&'static StorageConnectorRegistration> {
    CONNECTOR_REGISTRATIONS
        .iter()
        .find(|connector| connector.driver_type == driver_type)
        .ok_or_else(|| {
            crate::errors::AsterError::internal_error(format!(
                "storage connector '{}' is not registered",
                driver_type.as_str()
            ))
        })
}

pub fn list_storage_driver_descriptors() -> Vec<StorageConnectorDescriptor> {
    CONNECTOR_REGISTRATIONS
        .iter()
        .map(|connector| connector.connector.descriptor())
        .collect()
}

pub fn storage_driver_descriptor(driver_type: DriverType) -> StorageConnectorDescriptor {
    connector_for(driver_type)
        .expect("storage connector must be registered for every built-in driver")
        .connector
        .descriptor()
}

pub fn storage_connector_supports_native_thumbnail(driver_type: DriverType) -> bool {
    storage_driver_descriptor(driver_type)
        .capabilities
        .storage_native_thumbnail
}

pub fn storage_connector_supports_native_media_metadata(driver_type: DriverType) -> bool {
    storage_driver_descriptor(driver_type)
        .capabilities
        .storage_native_media_metadata
}

pub fn storage_authorization_provider(
    driver_type: DriverType,
) -> Result<Option<StorageCredentialProvider>> {
    Ok(storage_driver_descriptor(driver_type)
        .authorization_provider
        .as_deref()
        .and_then(StorageCredentialProvider::parse))
}

pub fn ensure_storage_authorization_supported(
    driver_type: DriverType,
    provider: StorageCredentialProvider,
) -> Result<StorageCredentialKind> {
    let descriptor = storage_driver_descriptor(driver_type);
    let starts_authorization = descriptor.actions.iter().any(|action| {
        action.action == StorageConnectorAction::StartAuthorization
            && action.kind == StorageConnectorActionKind::Authorization
    });
    let supported_provider = descriptor
        .authorization_provider
        .as_deref()
        .and_then(StorageCredentialProvider::parse);
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
pub fn ensure_storage_credential_validation_supported(
    driver_type: DriverType,
    provider: StorageCredentialProvider,
) -> Result<StorageCredentialKind> {
    let descriptor = storage_driver_descriptor(driver_type);
    let validates_credential = descriptor.actions.iter().any(|action| {
        action.action == StorageConnectorAction::ValidateCredential
            && action.kind == StorageConnectorActionKind::CredentialValidation
    });
    let supported_provider = descriptor
        .authorization_provider
        .as_deref()
        .and_then(StorageCredentialProvider::parse);
    if validates_credential && supported_provider == Some(provider) {
        return Ok(StorageCredentialKind::OauthDelegated);
    }
    Err(crate::errors::AsterError::unsupported_driver(format!(
        "storage credential validation provider '{}' is not supported for {} storage policies",
        provider.as_str(),
        driver_type.as_str()
    )))
}

pub async fn normalize_policy_connection<C: ConnectionTrait + Sync>(
    db: &C,
    input: StorageConnectorConnectionInput,
) -> Result<StorageConnectorConnectionInput> {
    let connector = connector_for(input.driver_type)?;
    connector
        .connector
        .normalize_policy_connection(db, input)
        .await
}

pub async fn validate_policy_options<C: ConnectionTrait + Sync>(
    db: &C,
    driver_type: DriverType,
    remote_node_id: Option<i64>,
    options: &crate::types::StoragePolicyOptions,
) -> Result<()> {
    connector_for(driver_type)?
        .connector
        .validate_policy_options(db, remote_node_id, options)
        .await
}

pub async fn test_draft_connection<S: RemoteProtocolRuntimeState + Sync>(
    state: &S,
    input: StorageConnectorConnectionInput,
) -> Result<()> {
    let connector = connector_for(input.driver_type)?;
    connector
        .connector
        .test_draft_connection(state, input)
        .await
}

pub async fn test_saved_connection<S: SharedRuntimeState + Sync>(
    state: &S,
    policy: &storage_policy::Model,
) -> Result<()> {
    connector_for(policy.driver_type)?
        .connector
        .test_saved_connection(state, policy)
        .await
}

pub async fn execute_saved_action<S: SharedRuntimeState + Sync>(
    state: &S,
    policy: &storage_policy::Model,
    action: StoragePolicyExecutableAction,
) -> Result<StorageConnectorActionResult> {
    connector_for(policy.driver_type)?
        .connector
        .execute_saved_action(state, policy, action)
        .await
}

pub async fn execute_draft_action<S: RemoteProtocolRuntimeState + Sync>(
    state: &S,
    input: ExecuteDraftStorageConnectorActionInput,
) -> Result<StorageConnectorActionResult> {
    let connector = connector_for(input.connection.driver_type)?;
    connector.connector.execute_draft_action(state, input).await
}

pub fn validate_driver_promotion_source(source: DriverType) -> Result<()> {
    if !connector_for(source)?.promotion_targets.is_empty() {
        return Ok(());
    }
    Err(crate::errors::validation_error_with_code(
        crate::api::api_error_code::ApiErrorCode::PolicyPromotionSourceUnsupported,
        "only generic S3-compatible policies can be promoted",
    ))
}

pub fn validate_driver_promotion_target(source: DriverType, target: DriverType) -> Result<()> {
    if connector_for(source)?.promotion_targets.contains(&target) {
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

pub fn validate_driver_promotion_candidate(policy: &storage_policy::Model) -> Result<()> {
    connector_for(policy.driver_type)?
        .connector
        .validate_promotion_candidate(policy)
}

pub fn resolve_policy_upload_transport(
    policy: &storage_policy::Model,
) -> StorageConnectorUploadTransport {
    connector_for(policy.driver_type)
        .expect("storage connector must be registered for every built-in driver")
        .connector
        .upload_transport(policy)
}

pub fn presigned_download_enabled(policy: &storage_policy::Model) -> bool {
    connector_for(policy.driver_type)
        .expect("storage connector must be registered for every built-in driver")
        .connector
        .presigned_download_enabled(policy)
}

pub fn streaming_direct_upload_eligible(
    policy: &storage_policy::Model,
    declared_size: i64,
) -> bool {
    resolve_policy_upload_transport(policy).supports_streaming_direct_upload(policy, declared_size)
}

pub(crate) async fn cleanup_snapshot_for_policy<S: SharedRuntimeState + Sync>(
    state: &S,
    policy: &storage_policy::Model,
) -> Result<Option<StoragePolicyCleanupDriverSnapshot>> {
    connector_for(policy.driver_type)?
        .connector
        .cleanup_snapshot_for_policy(state, policy)
        .await
}

pub(crate) fn can_create_cleanup_task_with_snapshot(
    driver_type: DriverType,
    driver_snapshot: &Option<StoragePolicyCleanupDriverSnapshot>,
) -> bool {
    connector_for(driver_type)
        .map(|connector| !connector.cleanup_snapshot_required || driver_snapshot.is_some())
        .unwrap_or(false)
}

pub(crate) async fn build_cleanup_driver<S: RemoteProtocolRuntimeState + Sync>(
    state: &S,
    policy: &storage_policy::Model,
    snapshots: StoragePolicyCleanupSnapshots<'_>,
) -> Result<Arc<dyn StorageDriver>> {
    connector_for(policy.driver_type)?
        .connector
        .build_cleanup_driver(state, policy, snapshots)
        .await
}

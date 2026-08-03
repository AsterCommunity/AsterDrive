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
    StorageConnectorActionResult, StorageConnectorConnectionInput, StorageConnectorCredentialInfo,
    StorageConnectorCredentialInput, TencentCosCorsConfigResult,
    TestDraftStorageConnectorConnectionInput,
};
pub(crate) use models::{
    LegacyStorageConnectorCredentialInput, LegacyStoragePolicyStaticCredential,
    LocalFilesystemPolicyProjection, RemotePolicyBindingProjection,
    StorageConnectorCredentialRequirement, StorageConnectorRuntimeCredential,
    StorageCredentialValidationOutcome, StoragePolicyCleanupDriverSnapshot,
    StoragePolicyCleanupOneDriveCredentialSnapshot, StoragePolicyCleanupRemoteNodeSnapshot,
    StoragePolicyCleanupSnapshots,
};
pub(crate) use onedrive::{
    OneDriveApplicationCredentialV1, OneDriveAuthorizationApplicationV1,
    OneDriveAuthorizationCredentialV1, OneDriveAuthorizationMetadataV1, OneDriveConnector,
    OneDriveConnectorConfigV1, OneDriveCredentialV1,
};
use remote::RemoteConnector;
use s3::S3Connector;
use sftp::SftpConnector;
use tencent_cos::TencentCosConnector;
pub use upload::StorageConnectorUploadTransport;

pub(crate) use contract::{
    StorageConnector, StorageConnectorContext, StorageConnectorDriver, StorageConnectorRegistry,
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

pub(crate) async fn normalize_connection(
    registry: &StorageConnectorRegistry,
    db: &sea_orm::DatabaseConnection,
    mut input: StorageConnectorConnectionInput,
) -> Result<StorageConnectorConnectionInput> {
    validate_credential_input(
        registry,
        &input.connector_config.connector_id,
        &input.credential,
    )?;
    input.connector_config =
        normalize_connector_config(registry, db, input.connector_config).await?;
    Ok(input)
}

pub(crate) async fn normalize_connector_config(
    registry: &StorageConnectorRegistry,
    db: &sea_orm::DatabaseConnection,
    connector_config: aster_drive_storage::ConnectorConfigEnvelope,
) -> Result<aster_drive_storage::ConnectorConfigEnvelope> {
    let connector = registry.require_connector(&connector_config.connector_id)?;
    let connector_config = connector.validate_connector_config(&connector_config)?;
    connector
        .validate_config_binding(db, &connector_config)
        .await?;
    Ok(connector_config)
}

pub(crate) fn validate_credential_input(
    registry: &StorageConnectorRegistry,
    connector_id: &aster_drive_storage::ConnectorId,
    credential: &StorageConnectorCredentialInput,
) -> Result<()> {
    registry
        .require_connector(connector_id)?
        .validate_credential_input(credential)
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use aster_drive_model::types::{StoragePolicyOptions, StoredStoragePolicyConfig};

    /// Encode test policy state through the same connector-owned typed schema
    /// used by production configuration flows.
    pub(crate) fn connector_config(
        driver_type: DriverType,
        endpoint: impl Into<String>,
        bucket: impl Into<String>,
        base_path: impl Into<String>,
        remote_node_id: Option<i64>,
        remote_storage_target_key: Option<String>,
        options: StoragePolicyOptions,
    ) -> aster_drive_storage::ConnectorConfigEnvelope<serde_json::Value> {
        let input = StorageConnectorConnectionInput {
            driver_type,
            endpoint: endpoint.into(),
            bucket: bucket.into(),
            access_key: String::new(),
            secret_key: String::new(),
            base_path: base_path.into(),
            remote_node_id,
            remote_storage_target_key,
            options,
        };
        let registry = builtin_storage_connector_registry().expect("built-in connector registry");
        registry
            .require(input.driver_type)
            .expect("test connector")
            .encode_config(&input)
            .expect("typed test connector config")
    }

    /// Encode core behavior fixtures through the versioned production codec.
    pub(crate) fn storage_config(
        connector: aster_drive_storage::ConnectorConfigEnvelope<serde_json::Value>,
        options: &StoragePolicyOptions,
    ) -> StoredStoragePolicyConfig {
        aster_drive_storage::encode_storage_policy_config(
            connector,
            common::behavior_config(options),
        )
        .map(StoredStoragePolicyConfig)
        .expect("typed test storage policy config")
    }

    pub(crate) fn policy_config(
        driver_type: DriverType,
        endpoint: impl Into<String>,
        bucket: impl Into<String>,
        base_path: impl Into<String>,
        remote_node_id: Option<i64>,
        remote_storage_target_key: Option<String>,
        options: &StoragePolicyOptions,
    ) -> StoredStoragePolicyConfig {
        storage_config(
            connector_config(
                driver_type,
                endpoint,
                bucket,
                base_path,
                remote_node_id,
                remote_storage_target_key,
                options.clone(),
            ),
            options,
        )
    }
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

pub(crate) fn storage_policy_supports_native_thumbnail(
    registry: &StorageConnectorRegistry,
    policy: &storage_policy::Model,
) -> Result<bool> {
    Ok(registry
        .require_policy(policy)?
        .descriptor()
        .capabilities
        .storage_native_thumbnail)
}

pub(crate) fn storage_policy_supports_native_media_metadata(
    registry: &StorageConnectorRegistry,
    policy: &storage_policy::Model,
) -> Result<bool> {
    Ok(registry
        .require_policy(policy)?
        .descriptor()
        .capabilities
        .storage_native_media_metadata)
}

pub(crate) fn ensure_storage_authorization_supported(
    registry: &StorageConnectorRegistry,
    policy: &storage_policy::Model,
    provider: StorageCredentialProvider,
) -> Result<StorageCredentialKind> {
    let descriptor = registry.require_policy(policy)?.descriptor();
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
        policy.connector_id
    )))
}

/// Gate credential validation through connector-declared actions so credential
/// services never need to know which storage drivers expose validation.
pub(crate) fn ensure_storage_credential_validation_supported(
    registry: &StorageConnectorRegistry,
    policy: &storage_policy::Model,
    provider: StorageCredentialProvider,
) -> Result<StorageCredentialKind> {
    let descriptor = registry.require_policy(policy)?.descriptor();
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
        policy.connector_id
    )))
}

pub(crate) async fn persist_credential(
    registry: &StorageConnectorRegistry,
    db: &sea_orm::DatabaseTransaction,
    encryption_key: &str,
    policy_id: i64,
    connector_config: &aster_drive_storage::ConnectorConfigEnvelope,
    credential: StorageConnectorCredentialInput,
) -> Result<()> {
    if matches!(
        credential,
        StorageConnectorCredentialInput::AuthorizationApplication(_)
    ) {
        crate::db::repository::storage_policy_connector_credential_repo::delete_by_policy(
            db, policy_id,
        )
        .await?;
    }
    registry
        .require_connector(&connector_config.connector_id)?
        .persist_credential(db, encryption_key, policy_id, connector_config, credential)
        .await
}

pub(crate) async fn persist_static_credential(
    db: &sea_orm::DatabaseTransaction,
    encryption_key: &str,
    policy_id: i64,
    connector_config: &aster_drive_storage::ConnectorConfigEnvelope,
    values: serde_json::Value,
) -> Result<()> {
    persist_connector_credential_payload(
        db,
        encryption_key,
        policy_id,
        &connector_config.connector_id,
        connector_config.schema_version,
        &values,
    )
    .await
}

pub(crate) async fn persist_connector_credential_payload<
    C: sea_orm::ConnectionTrait,
    T: serde::Serialize + ?Sized,
>(
    db: &C,
    encryption_key: &str,
    policy_id: i64,
    connector_id: &aster_drive_storage::ConnectorId,
    schema_version: u32,
    payload: &T,
) -> Result<()> {
    let plaintext = serde_json::to_string(payload).map_err(|error| {
        crate::errors::AsterError::validation_error(format!(
            "serialize connector credential payload: {error}"
        ))
    })?;
    let ciphertext =
        crate::services::storage_policy::credential::crypto::encrypt_connector_credential(
            encryption_key,
            policy_id,
            connector_id.as_str(),
            schema_version,
            &plaintext,
        )?;
    let schema_version = i32::try_from(schema_version).map_err(|_| {
        crate::errors::AsterError::validation_error(
            "connector schema version exceeds database range",
        )
    })?;
    crate::db::repository::storage_policy_connector_credential_repo::upsert(
        db,
        policy_id,
        connector_id.as_str().to_string(),
        schema_version,
        ciphertext,
    )
    .await
    .map(|_| ())
}

pub(crate) async fn persist_connector_credential_value<C: sea_orm::ConnectionTrait>(
    db: &C,
    encryption_key: &str,
    record: &aster_drive_model::entities::storage_policy_connector_credential::Model,
    payload: serde_json::Value,
) -> Result<aster_drive_model::entities::storage_policy_connector_credential::Model> {
    let schema_version = u32::try_from(record.schema_version).map_err(|_| {
        crate::errors::AsterError::database_operation(
            "stored connector credential schema version is negative",
        )
    })?;
    let plaintext = serde_json::to_string(&payload).map_err(|error| {
        crate::errors::AsterError::validation_error(format!(
            "serialize connector credential payload: {error}"
        ))
    })?;
    let ciphertext =
        crate::services::storage_policy::credential::crypto::encrypt_connector_credential(
            encryption_key,
            record.policy_id,
            &record.connector_id,
            schema_version,
            &plaintext,
        )?;
    crate::db::repository::storage_policy_connector_credential_repo::upsert(
        db,
        record.policy_id,
        record.connector_id.clone(),
        record.schema_version,
        ciphertext,
    )
    .await
}

pub(crate) fn decode_typed_connector_credential<T: serde::de::DeserializeOwned>(
    encryption_key: &str,
    record: &aster_drive_model::entities::storage_policy_connector_credential::Model,
    connector_id: &aster_drive_storage::ConnectorId,
    schema_version: u32,
) -> Result<T> {
    let values = decode_connector_credential(encryption_key, record, connector_id, schema_version)?;
    serde_json::from_value(values).map_err(|error| {
        crate::errors::AsterError::database_operation(format!(
            "stored connector credential payload does not match schema: {error}"
        ))
    })
}

pub(crate) fn decode_connector_credential(
    encryption_key: &str,
    record: &aster_drive_model::entities::storage_policy_connector_credential::Model,
    connector_id: &aster_drive_storage::ConnectorId,
    schema_version: u32,
) -> Result<serde_json::Value> {
    let expected_schema_version = i32::try_from(schema_version).map_err(|_| {
        crate::errors::AsterError::database_operation(
            "connector schema version exceeds database range",
        )
    })?;
    if record.connector_id != connector_id.as_str() {
        return Err(crate::errors::AsterError::database_operation(
            "stored credential connector id does not match storage policy",
        ));
    }
    if record.schema_version != expected_schema_version {
        return Err(crate::errors::AsterError::database_operation(
            "stored static credential schema version does not match connector descriptor",
        ));
    }
    let plaintext =
        crate::services::storage_policy::credential::crypto::decrypt_connector_credential(
            encryption_key,
            record.policy_id,
            connector_id.as_str(),
            schema_version,
            &record.ciphertext,
        )?;
    serde_json::from_str(&plaintext).map_err(|error| {
        crate::errors::AsterError::database_operation(format!(
            "stored connector credential payload is invalid JSON: {error}"
        ))
    })
}

pub(crate) async fn test_draft_connection<S: RemoteProtocolRuntimeState + Sync>(
    registry: &StorageConnectorRegistry,
    state: &S,
    input: TestDraftStorageConnectorConnectionInput,
) -> Result<()> {
    let context = remote_connector_context(state);
    let connector_id = input.connection.connector_config.connector_id.clone();
    registry
        .require_connector(&connector_id)?
        .test_draft_connection(&context, input)
        .await
}

pub(crate) async fn test_saved_connection<S: SharedRuntimeState + Sync>(
    registry: &StorageConnectorRegistry,
    state: &S,
    policy: &storage_policy::Model,
) -> Result<()> {
    registry
        .require_policy(policy)?
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
        .require_policy(policy)?
        .execute_saved_action(&shared_connector_context(state), policy, action)
        .await
}

pub(crate) async fn execute_draft_action<S: RemoteProtocolRuntimeState + Sync>(
    registry: &StorageConnectorRegistry,
    state: &S,
    input: ExecuteDraftStorageConnectorActionInput,
) -> Result<StorageConnectorActionResult> {
    let connector_id = input.connection.connector_config.connector_id.clone();
    registry
        .require_connector(&connector_id)?
        .execute_draft_action(&remote_connector_context(state), input)
        .await
}

pub(crate) fn resolve_policy_upload_transport(
    registry: &StorageConnectorRegistry,
    policy: &storage_policy::Model,
) -> Result<StorageConnectorUploadTransport> {
    registry.require_policy(policy)?.upload_transport(policy)
}

pub(crate) fn resolve_policy_behavior(
    registry: &StorageConnectorRegistry,
    policy: &storage_policy::Model,
) -> Result<aster_drive_storage::StoragePolicyBehaviorConfig> {
    registry.require_policy(policy)?.policy_behavior(policy)
}

pub(crate) fn resolve_local_filesystem_projection(
    registry: &StorageConnectorRegistry,
    policy: &storage_policy::Model,
) -> Result<Option<LocalFilesystemPolicyProjection>> {
    registry
        .require_policy(policy)?
        .local_filesystem_projection(policy)
}

pub(crate) fn resolve_remote_policy_binding(
    registry: &StorageConnectorRegistry,
    policy: &storage_policy::Model,
) -> Result<Option<RemotePolicyBindingProjection>> {
    registry
        .require_policy(policy)?
        .remote_binding_projection(policy)
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
    registry
        .require_policy(policy)?
        .presigned_download_enabled(policy)
}

pub(crate) fn presigned_download_requires_filename_match(
    registry: &StorageConnectorRegistry,
    policy: &storage_policy::Model,
) -> Result<bool> {
    registry
        .require_policy(policy)?
        .presigned_download_requires_filename_match(policy)
}

pub(crate) fn runtime_credential_requirement(
    registry: &StorageConnectorRegistry,
    policy: &storage_policy::Model,
) -> Result<Option<StorageConnectorCredentialRequirement>> {
    Ok(registry
        .require_policy(policy)?
        .runtime_credential_requirement())
}

pub(crate) fn credential_info(
    registry: &StorageConnectorRegistry,
    config: &crate::config::Config,
    policy: &storage_policy::Model,
    credential: &aster_drive_model::entities::storage_policy_connector_credential::Model,
) -> Result<Option<StorageConnectorCredentialInfo>> {
    registry
        .require_policy(policy)?
        .credential_info(config, credential)
}

pub(crate) fn credential_validation_failure_payload(
    registry: &StorageConnectorRegistry,
    config: &crate::config::Config,
    policy: &storage_policy::Model,
    credential: &aster_drive_model::entities::storage_policy_connector_credential::Model,
    error_kind: Option<aster_drive_storage::StorageErrorKind>,
    reason: &str,
) -> Result<Option<serde_json::Value>> {
    registry
        .require_policy(policy)?
        .credential_validation_failure_payload(config, credential, error_kind, reason)
}

pub(crate) async fn load_runtime_credential(
    registry: &StorageConnectorRegistry,
    db: &sea_orm::DatabaseConnection,
    config: &crate::config::Config,
    policy: &storage_policy::Model,
    credential: &aster_drive_model::entities::storage_policy_connector_credential::Model,
) -> Result<Option<StorageConnectorRuntimeCredential>> {
    registry
        .require_policy(policy)?
        .load_runtime_credential(db, config, policy, credential)
        .await
}

pub(crate) async fn validate_credential(
    registry: &StorageConnectorRegistry,
    db: &sea_orm::DatabaseConnection,
    config: &crate::config::Config,
    policy: &storage_policy::Model,
    credential: &aster_drive_model::entities::storage_policy_connector_credential::Model,
) -> Result<StorageCredentialValidationOutcome> {
    registry
        .require_policy(policy)?
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
        .require_policy(policy)?
        .cleanup_snapshot_for_policy(&shared_connector_context(state), policy)
        .await
}

pub(crate) fn can_create_cleanup_task_with_snapshot(
    registry: &StorageConnectorRegistry,
    policy: &storage_policy::Model,
    driver_snapshot: &Option<StoragePolicyCleanupDriverSnapshot>,
) -> Result<bool> {
    Ok(!registry.require_policy(policy)?.cleanup_snapshot_required() || driver_snapshot.is_some())
}

pub(crate) async fn build_cleanup_driver<S: RemoteProtocolRuntimeState + Sync>(
    registry: &StorageConnectorRegistry,
    state: &S,
    policy: &storage_policy::Model,
    snapshots: StoragePolicyCleanupSnapshots<'_>,
) -> Result<Arc<dyn StorageDriver>> {
    registry
        .require_policy(policy)?
        .build_cleanup_driver(&remote_connector_context(state), policy, snapshots)
        .await
}

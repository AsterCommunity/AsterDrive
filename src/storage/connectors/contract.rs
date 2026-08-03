use async_trait::async_trait;
use sea_orm::{DatabaseConnection, DatabaseTransaction};
use std::collections::HashMap;
use std::sync::Arc;

use crate::config::{Config, RuntimeConfig};
use crate::errors::{AsterError, Result};
use crate::storage::DriverRegistry;
use crate::storage::remote_protocol::RemoteProtocolRuntime;
use aster_drive_model::entities::{storage_policy, storage_policy_connector_credential};
use aster_drive_storage::ConnectorConfigEnvelope;
use aster_drive_storage::StoragePolicyBehaviorConfig;
use aster_drive_storage::connector_descriptor::{
    StorageConnectorAffordanceAction, StorageConnectorDescriptor, StorageConnectorObjectNamingMode,
    StoragePolicyExecutableAction,
};
use aster_drive_storage::{ConnectorId, MultipartStorageDriver, StorageDriver, StorageErrorKind};

use super::common;
use super::models::{
    ExecuteDraftStorageConnectorActionInput, LegacyStorageConnectorCredentialInput,
    LocalFilesystemPolicyProjection, RemotePolicyBindingProjection, StorageConnectorActionResult,
    StorageConnectorCredentialInfo, StorageConnectorCredentialInput,
    StorageConnectorRuntimeCredential, StorageCredentialValidationOutcome,
    StoragePolicyCleanupDriverSnapshot, StoragePolicyCleanupSnapshots,
    TestDraftStorageConnectorConnectionInput,
};
use super::upload::StorageConnectorUploadTransport;

/// Dependencies available while executing connector configuration workflows.
///
/// This context keeps connector implementations independent from the concrete
/// primary/follower application state and excludes unrelated product services.
pub(crate) struct StorageConnectorContext<'a> {
    writer_db: &'a DatabaseConnection,
    config: &'a Config,
    runtime_config: &'a RuntimeConfig,
    driver_registry: &'a DriverRegistry,
    remote_protocol: Option<&'a RemoteProtocolRuntime>,
}

impl<'a> StorageConnectorContext<'a> {
    pub(crate) fn new(
        writer_db: &'a DatabaseConnection,
        config: &'a Config,
        runtime_config: &'a RuntimeConfig,
        driver_registry: &'a DriverRegistry,
        remote_protocol: Option<&'a RemoteProtocolRuntime>,
    ) -> Self {
        Self {
            writer_db,
            config,
            runtime_config,
            driver_registry,
            remote_protocol,
        }
    }

    pub(crate) fn writer_db(&self) -> &'a DatabaseConnection {
        self.writer_db
    }

    pub(crate) fn config(&self) -> &'a Config {
        self.config
    }

    pub(crate) fn runtime_config(&self) -> &'a RuntimeConfig {
        self.runtime_config
    }

    pub(crate) fn driver_registry(&self) -> &'a DriverRegistry {
        self.driver_registry
    }

    pub(crate) fn remote_protocol(&self) -> Result<&'a RemoteProtocolRuntime> {
        self.remote_protocol.ok_or_else(|| {
            AsterError::internal_error("storage connector requires remote protocol runtime")
        })
    }
}

/// Runtime drivers produced by a connector factory.
///
/// `multipart` is populated only when the connector provides the multipart
/// extension through the same underlying driver instance.
pub(crate) struct StorageConnectorDriver {
    pub(crate) storage: Arc<dyn StorageDriver>,
    pub(crate) multipart: Option<Arc<dyn MultipartStorageDriver>>,
}

impl StorageConnectorDriver {
    pub(crate) fn storage(storage: Arc<dyn StorageDriver>) -> Self {
        Self {
            storage,
            multipart: None,
        }
    }

    pub(crate) fn multipart<T>(driver: Arc<T>) -> Self
    where
        T: StorageDriver + MultipartStorageDriver + 'static,
    {
        let storage: Arc<dyn StorageDriver> = driver.clone();
        let multipart: Arc<dyn MultipartStorageDriver> = driver;
        Self {
            storage,
            multipart: Some(multipart),
        }
    }
}

#[async_trait]
/// Configuration, capability, action, and runtime-factory contract for one
/// storage backend.
///
/// Product services select a connector through [`StorageConnectorRegistry`]
/// and invoke this object-safe contract without matching concrete providers.
pub(crate) trait StorageConnector: Send + Sync {
    fn descriptor(&self) -> StorageConnectorDescriptor;

    fn validate_credential_input(&self, input: &StorageConnectorCredentialInput) -> Result<()> {
        use aster_drive_storage::StorageConnectorCredentialMode;
        let valid = matches!(
            (self.descriptor().credential_mode, input),
            (
                StorageConnectorCredentialMode::None,
                StorageConnectorCredentialInput::None
            ) | (
                StorageConnectorCredentialMode::StaticSecret,
                StorageConnectorCredentialInput::Static(_)
            ) | (
                StorageConnectorCredentialMode::OauthDelegated,
                StorageConnectorCredentialInput::AuthorizationApplication(_)
            )
        );
        if valid {
            return Ok(());
        }
        Err(AsterError::validation_error(format!(
            "credential mode does not match storage connector '{}'",
            self.descriptor().connector_id
        )))
    }

    async fn validate_config_binding(
        &self,
        _db: &DatabaseConnection,
        _config: &ConnectorConfigEnvelope,
    ) -> Result<()> {
        Ok(())
    }

    fn validate_connector_config(
        &self,
        config: &ConnectorConfigEnvelope,
    ) -> Result<ConnectorConfigEnvelope> {
        let normalized =
            aster_drive_storage::connector_descriptor::normalize_storage_connector_config(
                &self.descriptor(),
                config,
            )
            .map_err(|error| AsterError::validation_error(error.to_string()))?;
        Ok(normalized)
    }

    async fn persist_credential(
        &self,
        db: &DatabaseTransaction,
        encryption_key: &str,
        policy_id: i64,
        connector_config: &ConnectorConfigEnvelope,
        credential: StorageConnectorCredentialInput,
    ) -> Result<()> {
        match credential {
            StorageConnectorCredentialInput::None => Ok(()),
            StorageConnectorCredentialInput::Static(values) => {
                super::persist_static_credential(
                    db,
                    encryption_key,
                    policy_id,
                    connector_config,
                    values,
                )
                .await
            }
            StorageConnectorCredentialInput::AuthorizationApplication(_) => {
                Err(AsterError::validation_error(
                    "authorization application credential persistence is connector-owned",
                ))
            }
        }
    }

    async fn build_draft_driver(
        &self,
        _context: &StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
        credential: &StorageConnectorCredentialInput,
    ) -> Result<Box<dyn StorageDriver>>;

    fn build_runtime_driver(
        &self,
        registry: &DriverRegistry,
        policy: &storage_policy::Model,
    ) -> Result<StorageConnectorDriver>;

    fn upload_transport(
        &self,
        policy: &storage_policy::Model,
    ) -> Result<StorageConnectorUploadTransport>;

    /// Decode the core-owned behavior section after validating the complete
    /// connector envelope against this plugin's id and schema version.
    fn policy_behavior(
        &self,
        policy: &storage_policy::Model,
    ) -> Result<StoragePolicyBehaviorConfig> {
        let descriptor = self.descriptor();
        common::decode_typed_policy_config_for_id::<serde_json::Value>(
            policy,
            &descriptor.connector_id,
            descriptor.config_schema_version,
        )
        .map(|(_config, behavior)| behavior)
    }

    fn local_filesystem_projection(
        &self,
        _policy: &storage_policy::Model,
    ) -> Result<Option<LocalFilesystemPolicyProjection>> {
        Ok(None)
    }

    fn remote_binding_projection(
        &self,
        _policy: &storage_policy::Model,
    ) -> Result<Option<RemotePolicyBindingProjection>> {
        Ok(None)
    }

    fn credential_info(
        &self,
        _config: &Config,
        _credential: &storage_policy_connector_credential::Model,
    ) -> Result<Option<StorageConnectorCredentialInfo>> {
        Ok(None)
    }

    fn credential_validation_failure_payload(
        &self,
        _config: &Config,
        _credential: &storage_policy_connector_credential::Model,
        _error_kind: Option<StorageErrorKind>,
        _reason: &str,
    ) -> Result<Option<serde_json::Value>> {
        Ok(None)
    }

    /// Convert rows from the deprecated credential stores into this
    /// connector's current typed payload during the AsterDrive 0.5.0-only
    /// startup migration.
    ///
    /// The default rejects unexpected legacy data so a missing connector hook
    /// stops startup instead of silently discarding credentials. This contract
    /// and the deprecated inputs are scheduled for removal in AsterDrive 0.6.0.
    fn import_legacy_credential(
        &self,
        _encryption_key: &str,
        policy: &storage_policy::Model,
        input: LegacyStorageConnectorCredentialInput,
    ) -> Result<Option<serde_json::Value>> {
        if input.is_empty() {
            return Ok(None);
        }
        Err(AsterError::database_operation(format!(
            "storage policy {} has legacy credentials unsupported by connector '{}'",
            policy.id,
            self.descriptor().connector_id.as_str(),
        )))
    }

    async fn load_runtime_credential(
        &self,
        _db: &DatabaseConnection,
        config: &Config,
        _policy: &storage_policy::Model,
        credential: &storage_policy_connector_credential::Model,
    ) -> Result<Option<StorageConnectorRuntimeCredential>> {
        if self.descriptor().credential_mode
            != aster_drive_storage::StorageConnectorCredentialMode::StaticSecret
        {
            return Ok(None);
        }
        let descriptor = self.descriptor();
        let values = super::decode_connector_credential(
            &config.auth.storage_credential_secret_key,
            credential,
            &descriptor.connector_id,
            descriptor.config_schema_version,
        )?;
        Ok(Some(StorageConnectorRuntimeCredential::Static(values)))
    }

    fn build_authorized_driver(
        &self,
        policy: &storage_policy::Model,
        credential: StorageConnectorRuntimeCredential,
    ) -> Result<Arc<dyn StorageDriver>> {
        let _ = (policy, credential);
        Err(crate::errors::storage_driver_error(
            StorageErrorKind::Unsupported,
            format!(
                "{} storage policies do not use runtime credential driver construction",
                self.descriptor().connector_id.as_str()
            ),
        ))
    }

    async fn validate_credential(
        &self,
        _db: &DatabaseConnection,
        _config: &Config,
        _policy: &storage_policy::Model,
        _credential: &storage_policy_connector_credential::Model,
    ) -> Result<StorageCredentialValidationOutcome> {
        Err(AsterError::unsupported_driver(format!(
            "credential validation is not implemented for {} storage policies",
            self.descriptor().connector_id.as_str()
        )))
    }

    fn presigned_download_enabled(&self, _policy: &storage_policy::Model) -> Result<bool> {
        Ok(false)
    }

    fn presigned_download_requires_filename_match(
        &self,
        _policy: &storage_policy::Model,
    ) -> Result<bool> {
        Ok(false)
    }

    async fn test_draft_connection(
        &self,
        context: &StorageConnectorContext<'_>,
        input: TestDraftStorageConnectorConnectionInput,
    ) -> Result<()> {
        let descriptor = self.descriptor();
        if !descriptor.actions.iter().any(|action| {
            action.affordance_action == Some(StorageConnectorAffordanceAction::TestDraftConnection)
        }) {
            return Err(common::unsupported_draft_connection_test_error(descriptor));
        }
        let connection = input.connection;
        self.validate_credential_input(&connection.credential)?;
        let connector_config = self.validate_connector_config(&connection.connector_config)?;
        self.validate_config_binding(context.writer_db(), &connector_config)
            .await?;
        let policy = common::build_connection_test_policy(connector_config, connection.behavior)?;
        let driver = self
            .build_draft_driver(context, &policy, &connection.credential)
            .await?;
        common::probe_storage_driver(driver.as_ref(), "connection test failed").await
    }

    async fn test_saved_connection(
        &self,
        context: &StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
    ) -> Result<()> {
        let descriptor = self.descriptor();
        if !descriptor.actions.iter().any(|action| {
            action.affordance_action == Some(StorageConnectorAffordanceAction::TestSavedConnection)
        }) {
            return Err(common::unsupported_saved_connection_test_error(descriptor));
        }
        let driver = context.driver_registry().get_driver(policy)?;
        common::probe_storage_driver(driver.as_ref(), "write test failed").await
    }

    async fn execute_saved_action(
        &self,
        _context: &StorageConnectorContext<'_>,
        _policy: &storage_policy::Model,
        action: StoragePolicyExecutableAction,
    ) -> Result<StorageConnectorActionResult> {
        Err(common::unsupported_policy_action_error(
            self.descriptor(),
            action,
        ))
    }

    async fn execute_draft_action(
        &self,
        _context: &StorageConnectorContext<'_>,
        input: ExecuteDraftStorageConnectorActionInput,
    ) -> Result<StorageConnectorActionResult> {
        Err(common::unsupported_policy_action_error(
            self.descriptor(),
            input.action,
        ))
    }

    async fn cleanup_snapshot_for_policy(
        &self,
        _context: &StorageConnectorContext<'_>,
        _policy: &storage_policy::Model,
    ) -> Result<Option<StoragePolicyCleanupDriverSnapshot>> {
        Ok(None)
    }

    fn cleanup_snapshot_required(&self) -> bool {
        false
    }

    async fn build_cleanup_driver(
        &self,
        context: &StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
        _snapshots: StoragePolicyCleanupSnapshots<'_>,
    ) -> Result<Arc<dyn StorageDriver>> {
        Ok(self
            .build_runtime_driver(context.driver_registry(), policy)?
            .storage)
    }
}

/// Ordered connector catalog and runtime-factory lookup table.
///
/// Registration order is preserved for stable descriptor presentation, while
/// runtime dispatch uses plugin-safe [`ConnectorId`] lookup.
pub struct StorageConnectorRegistry {
    ordered: Vec<Arc<dyn StorageConnector>>,
    by_connector_id: HashMap<ConnectorId, Arc<dyn StorageConnector>>,
}

impl StorageConnectorRegistry {
    pub(crate) fn new(connectors: Vec<Arc<dyn StorageConnector>>) -> Result<Self> {
        let mut by_connector_id = HashMap::with_capacity(connectors.len());
        for connector in &connectors {
            let descriptor = connector.descriptor();
            descriptor.connector_id.validate().map_err(|error| {
                AsterError::internal_error(format!(
                    "storage connector declares invalid id '{}': {error}",
                    descriptor.connector_id
                ))
            })?;
            if by_connector_id
                .insert(descriptor.connector_id.clone(), connector.clone())
                .is_some()
            {
                return Err(AsterError::internal_error(format!(
                    "storage connector '{}' is registered more than once",
                    descriptor.connector_id
                )));
            }
        }
        Ok(Self {
            ordered: connectors,
            by_connector_id,
        })
    }

    pub(crate) fn require_connector(
        &self,
        connector_id: &ConnectorId,
    ) -> Result<&dyn StorageConnector> {
        self.by_connector_id
            .get(connector_id)
            .map(AsRef::as_ref)
            .ok_or_else(|| {
                AsterError::internal_error(format!(
                    "storage connector '{}' is not registered",
                    connector_id
                ))
            })
    }

    /// Resolve the runtime factory from the policy's persisted plugin id.
    ///
    /// The policy entity deliberately carries no built-in driver enum. Invalid
    /// persisted ids are treated as a misconfigured storage policy rather than
    /// being translated through a core-owned provider table.
    pub(crate) fn require_policy(
        &self,
        policy: &storage_policy::Model,
    ) -> Result<&dyn StorageConnector> {
        let connector_id = ConnectorId::declared(policy.connector_id.clone());
        connector_id.validate().map_err(|error| {
            crate::errors::storage_driver_error(
                StorageErrorKind::Misconfigured,
                format!(
                    "storage policy {} has invalid connector id '{}': {error}",
                    policy.id, policy.connector_id
                ),
            )
        })?;
        self.require_connector(&connector_id)
    }

    pub(crate) fn descriptors(&self) -> Vec<StorageConnectorDescriptor> {
        self.ordered
            .iter()
            .map(|connector| connector.descriptor())
            .collect()
    }

    pub(crate) fn object_naming(
        &self,
        policy: &storage_policy::Model,
    ) -> Result<StorageConnectorObjectNamingMode> {
        Ok(self
            .require_policy(policy)?
            .descriptor()
            .capabilities
            .object_naming)
    }
}

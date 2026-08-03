use async_trait::async_trait;
use sea_orm::{DatabaseConnection, DatabaseTransaction};
use std::collections::HashMap;
use std::sync::Arc;
use validator::Validate;

use crate::config::{Config, RuntimeConfig};
use crate::errors::{AsterError, Result};
use crate::storage::DriverRegistry;
use crate::storage::remote_protocol::RemoteProtocolRuntime;
use aster_drive_model::entities::{storage_policy, storage_policy_credential};
use aster_drive_model::types::{DriverType, StoragePolicyOptions};
use aster_drive_storage::connector_descriptor::{
    StorageConnectorAffordanceAction, StorageConnectorDescriptor, StorageConnectorObjectNamingMode,
    StoragePolicyExecutableAction,
};
use aster_drive_storage::{MultipartStorageDriver, StorageDriver, StorageErrorKind};

use super::common;
use super::models::{
    ExecuteDraftStorageConnectorActionInput, StorageConnectorActionResult,
    StorageConnectorApplicationConfigInput, StorageConnectorConnectionInput,
    StorageConnectorCredentialRequirement, StorageConnectorRuntimeCredential,
    StorageCredentialValidationOutcome, StoragePolicyCleanupDriverSnapshot,
    StoragePolicyCleanupSnapshots, TestDraftStorageConnectorConnectionInput,
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
    fn driver_type(&self) -> DriverType;

    fn descriptor(&self) -> StorageConnectorDescriptor;

    fn normalize_connection_fields(&self, endpoint: &str, bucket: &str)
    -> Result<(String, String)>;

    fn validate_connection_credentials(
        &self,
        input: &StorageConnectorConnectionInput,
    ) -> Result<()>;

    fn supports_saved_draft_credentials(&self) -> bool {
        false
    }

    fn prepare_connection_for_storage(
        &self,
        input: StorageConnectorConnectionInput,
        application_config: &StorageConnectorApplicationConfigInput,
    ) -> Result<StorageConnectorConnectionInput> {
        if !application_config.is_empty() {
            return Err(AsterError::validation_error(format!(
                "application credential config is not valid for {} storage policies",
                self.driver_type().as_str()
            )));
        }
        Ok(input)
    }

    async fn validate_connection_binding(
        &self,
        _db: &DatabaseConnection,
        input: &StorageConnectorConnectionInput,
    ) -> Result<Option<i64>> {
        common::reject_unexpected_remote_storage_target_key(
            input.remote_storage_target_key.as_deref(),
        )?;
        common::reject_unexpected_remote_node(input.remote_node_id)
    }

    async fn validate_policy_options(
        &self,
        _db: &DatabaseConnection,
        _remote_node_id: Option<i64>,
        options: &StoragePolicyOptions,
    ) -> Result<()> {
        options
            .validate()
            .map_err(|error| AsterError::validation_error(error.to_string()))?;
        common::ensure_s3_region_supported(self.driver_type(), options)?;
        common::ensure_storage_native_processing_supported(self.descriptor(), options)?;
        common::ensure_onedrive_options_absent(options)?;
        common::ensure_sftp_options_absent(options)
    }

    async fn persist_application_config(
        &self,
        _db: &DatabaseTransaction,
        _encryption_key: &str,
        _policy_id: i64,
        _options: &StoragePolicyOptions,
        application_config: StorageConnectorApplicationConfigInput,
    ) -> Result<()> {
        if !application_config.is_empty() {
            return Err(AsterError::validation_error(format!(
                "application credential config is not valid for {} storage policies",
                self.driver_type().as_str()
            )));
        }
        Ok(())
    }

    async fn build_draft_driver(
        &self,
        _context: &StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
    ) -> Result<Box<dyn StorageDriver>>;

    fn build_runtime_driver(
        &self,
        registry: &DriverRegistry,
        policy: &storage_policy::Model,
    ) -> Result<StorageConnectorDriver>;

    fn upload_transport(&self, policy: &storage_policy::Model) -> StorageConnectorUploadTransport;

    fn runtime_credential_requirement(&self) -> Option<StorageConnectorCredentialRequirement> {
        None
    }

    async fn load_runtime_credential(
        &self,
        _db: &DatabaseConnection,
        _config: &Config,
        _policy: &storage_policy::Model,
        _credential: &storage_policy_credential::Model,
    ) -> Result<Option<StorageConnectorRuntimeCredential>> {
        Ok(None)
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
                self.driver_type().as_str()
            ),
        ))
    }

    async fn validate_credential(
        &self,
        _db: &DatabaseConnection,
        _config: &Config,
        _policy: &storage_policy::Model,
        _credential: &storage_policy_credential::Model,
    ) -> Result<StorageCredentialValidationOutcome> {
        Err(AsterError::unsupported_driver(format!(
            "credential validation is not implemented for {} storage policies",
            self.driver_type().as_str()
        )))
    }

    fn presigned_download_enabled(&self, _policy: &storage_policy::Model) -> bool {
        false
    }

    fn presigned_download_requires_filename_match(&self, _policy: &storage_policy::Model) -> bool {
        false
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
        let connection = if self.supports_saved_draft_credentials() {
            common::merge_saved_static_credentials_for_draft(
                context.writer_db(),
                input.policy_id,
                input.connection,
                "draft storage policy connection test",
            )
            .await?
        } else {
            input.connection
        };
        let policy =
            common::build_connection_test_policy(context.writer_db(), self, connection).await?;
        let driver = self.build_draft_driver(context, &policy).await?;
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

    fn validate_promotion_candidate(&self, policy: &storage_policy::Model) -> Result<()> {
        let _ = policy;
        Err(crate::errors::validation_error_with_code(
            crate::api::api_error_code::ApiErrorCode::PolicyPromotionTargetUnsupported,
            format!(
                "promoting S3-compatible policy to '{}' is not supported",
                self.driver_type().as_str()
            ),
        ))
    }
}

/// Ordered connector catalog and runtime-factory lookup table.
///
/// Registration order is preserved for stable descriptor presentation, while
/// runtime dispatch uses `DriverType` lookup and rejects ambiguous contracts.
pub(crate) struct StorageConnectorRegistry {
    ordered: Vec<Arc<dyn StorageConnector>>,
    by_driver_type: HashMap<DriverType, Arc<dyn StorageConnector>>,
}

impl StorageConnectorRegistry {
    pub(crate) fn new(connectors: Vec<Arc<dyn StorageConnector>>) -> Result<Self> {
        let mut by_driver_type = HashMap::with_capacity(connectors.len());
        for connector in &connectors {
            let driver_type = connector.driver_type();
            let descriptor_driver_type = connector.descriptor().driver_type;
            if descriptor_driver_type != driver_type {
                return Err(AsterError::internal_error(format!(
                    "storage connector '{}' descriptor declares driver type '{}'",
                    driver_type.as_str(),
                    descriptor_driver_type.as_str()
                )));
            }
            if by_driver_type
                .insert(driver_type, connector.clone())
                .is_some()
            {
                return Err(AsterError::internal_error(format!(
                    "storage connector '{}' is registered more than once",
                    driver_type.as_str()
                )));
            }
        }
        Ok(Self {
            ordered: connectors,
            by_driver_type,
        })
    }

    pub(crate) fn require(&self, driver_type: DriverType) -> Result<&dyn StorageConnector> {
        self.by_driver_type
            .get(&driver_type)
            .map(AsRef::as_ref)
            .ok_or_else(|| {
                AsterError::internal_error(format!(
                    "storage connector '{}' is not registered",
                    driver_type.as_str()
                ))
            })
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
            .require(policy.driver_type)?
            .descriptor()
            .capabilities
            .object_naming)
    }
}

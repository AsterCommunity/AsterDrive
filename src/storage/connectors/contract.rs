use async_trait::async_trait;
use sea_orm::{DatabaseConnection, DatabaseTransaction};
use std::collections::HashMap;
use std::sync::Arc;

use crate::api::api_error_code::ApiErrorCode;
use crate::config::{Config, RuntimeConfig};
use crate::errors::{AsterError, Result, validation_error_with_code};
use crate::storage::DriverRegistry;
use crate::storage::remote_protocol::RemoteProtocolRuntime;
use aster_drive_model::entities::{storage_policy, storage_policy_connector_credential};
use aster_drive_storage::ConnectorConfigEnvelope;
use aster_drive_storage::StorageConnectorLocalization;
use aster_drive_storage::StoragePolicyBehaviorConfig;
use aster_drive_storage::connector_descriptor::{
    StorageConnectorActionDescriptor, StorageConnectorActionEndpoint, StorageConnectorActionId,
    StorageConnectorActionKind, StorageConnectorDescriptor, StorageConnectorFieldDescriptor,
    StorageConnectorFieldKind, StorageConnectorFieldScope, StorageConnectorObjectNamingMode,
    StorageConnectorPromotionDescriptor, StorageConnectorPromotionId,
};
use aster_drive_storage::{ConnectorId, MultipartStorageDriver, StorageDriver, StorageErrorKind};

use super::common;
use super::models::{
    ExecuteDraftStorageConnectorActionInput, ExecuteSavedStorageConnectorActionInput,
    LegacyStorageConnectorCredentialInput, LocalFilesystemPolicyProjection,
    RemotePolicyBindingProjection, StorageConnectorActionResult,
    StorageConnectorAuthorizationCallback, StorageConnectorAuthorizationError,
    StorageConnectorAuthorizationStart, StorageConnectorCredentialInfo,
    StorageConnectorCredentialInput, StorageConnectorRuntimeCredential,
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
    fn descriptor(&self) -> StorageConnectorDescriptor;

    /// Connector-owned UI messages. The registry validates this resource
    /// against every message id referenced by `descriptor()` before startup.
    fn localization(&self) -> Result<StorageConnectorLocalization>;

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

    /// Validate core-owned policy behavior against connector-declared capabilities.
    ///
    /// Connectors advertise the executable capability; core owns the behavior
    /// payload. Keeping the admission check on this contract makes create,
    /// update, and draft actions consistent for built-ins and external plugins.
    fn validate_policy_behavior(&self, behavior: &StoragePolicyBehaviorConfig) -> Result<()> {
        let descriptor = self.descriptor();
        if behavior.uses_storage_native_thumbnail()
            && !descriptor.capabilities.storage_native_thumbnail
        {
            return Err(validation_error_with_code(
                ApiErrorCode::PolicyNativeThumbnailUnsupported,
                format!(
                    "storage connector '{}' does not expose storage-native thumbnail processing",
                    descriptor.connector_id
                ),
            ));
        }
        if behavior.uses_storage_native_media_metadata()
            && !descriptor.capabilities.storage_native_media_metadata
        {
            return Err(validation_error_with_code(
                ApiErrorCode::PolicyNativeMediaMetadataUnsupported,
                format!(
                    "storage connector '{}' does not expose storage-native media metadata processing",
                    descriptor.connector_id
                ),
            ));
        }
        Ok(())
    }

    /// Resolve edit-form placeholders before a draft connection test.
    ///
    /// Static connectors share an object-level merge implementation. A future
    /// connector with a different persisted credential shape can override this
    /// hook without adding provider branches to the policy service.
    async fn prepare_draft_credential(
        &self,
        context: &StorageConnectorContext<'_>,
        policy_id: Option<i64>,
        input: StorageConnectorCredentialInput,
    ) -> Result<StorageConnectorCredentialInput> {
        use aster_drive_storage::StorageConnectorCredentialMode;

        let descriptor = self.descriptor();
        if descriptor.credential_mode != StorageConnectorCredentialMode::StaticSecret {
            return Ok(input);
        }
        let Some(policy_id) = policy_id else {
            return Ok(input);
        };
        let policy =
            crate::db::repository::policy_repo::find_by_id(context.writer_db(), policy_id).await?;
        if policy.connector_id != descriptor.connector_id.as_str() {
            return Err(AsterError::validation_error(format!(
                "storage policy #{policy_id} uses connector '{}', not '{}'",
                policy.connector_id, descriptor.connector_id
            )));
        }
        let Some(saved) =
            crate::db::repository::storage_policy_connector_credential_repo::find_by_policy(
                context.writer_db(),
                policy_id,
            )
            .await?
        else {
            return Ok(input);
        };
        let saved = super::decode_connector_credential(
            &context.config().auth.storage_credential_secret_key,
            &saved,
            &descriptor.connector_id,
            super::credential_schema_version(&descriptor)?,
        )?;
        common::merge_saved_static_credential(input, saved)
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
                    super::credential_schema_version(&self.descriptor())?,
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
            super::credential_schema_version(&descriptor)?,
        )?;
        Ok(Some(StorageConnectorRuntimeCredential::new(
            descriptor.connector_id,
            values,
        )))
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

    /// Start a connector-owned authorization flow.
    ///
    /// The core service supplies only the saved policy and callback URI. The
    /// connector generates protocol state, builds its opaque flow context, and
    /// returns a typed result that core persists without decoding.
    async fn start_authorization(
        &self,
        _context: &StorageConnectorContext<'_>,
        _policy: &storage_policy::Model,
        _redirect_uri: &str,
    ) -> Result<StorageConnectorAuthorizationStart> {
        Err(AsterError::unsupported_driver(format!(
            "storage connector '{}' does not implement authorization",
            self.descriptor().connector_id.as_str()
        )))
    }

    /// Finish a connector-owned authorization flow after core has consumed the
    /// one-time state row. `code` and the opaque flow context are passed to the
    /// connector; core only persists the returned connector payload.
    async fn finish_authorization(
        &self,
        _context: &StorageConnectorContext<'_>,
        _policy: &storage_policy::Model,
        _flow: &aster_drive_model::entities::storage_policy_authorization_flow::Model,
        _code: &str,
        _now: chrono::DateTime<chrono::Utc>,
    ) -> std::result::Result<
        StorageConnectorAuthorizationCallback,
        StorageConnectorAuthorizationError,
    > {
        Err(StorageConnectorAuthorizationError::new(
            super::StorageAuthorizationFailureReason::UnsupportedProvider,
            AsterError::unsupported_driver(format!(
                "storage connector '{}' does not implement authorization",
                self.descriptor().connector_id.as_str()
            )),
        ))
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
            action.kind == StorageConnectorActionKind::ConnectionTest
                && action
                    .endpoints
                    .contains(&StorageConnectorActionEndpoint::TestPolicyParams)
        }) {
            return Err(common::unsupported_draft_connection_test_error(descriptor));
        }
        let connection = input.connection;
        let credential = self
            .prepare_draft_credential(context, input.policy_id, connection.credential)
            .await?;
        self.validate_credential_input(&credential)?;
        let connector_config = self.validate_connector_config(&connection.connector_config)?;
        self.validate_config_binding(context.writer_db(), &connector_config)
            .await?;
        let behavior = connection.behavior.normalized();
        self.validate_policy_behavior(&behavior)?;
        let policy = common::build_connection_test_policy(connector_config, behavior)?;
        let driver = self
            .build_draft_driver(context, &policy, &credential)
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
            action.kind == StorageConnectorActionKind::ConnectionTest
                && action
                    .endpoints
                    .contains(&StorageConnectorActionEndpoint::TestPolicyConnection)
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
        input: ExecuteSavedStorageConnectorActionInput,
    ) -> Result<StorageConnectorActionResult> {
        Err(common::unsupported_connector_action_error(
            &self.descriptor(),
            &input.action_id,
        ))
    }

    async fn execute_draft_action(
        &self,
        _context: &StorageConnectorContext<'_>,
        input: ExecuteDraftStorageConnectorActionInput,
    ) -> Result<StorageConnectorActionResult> {
        Err(common::unsupported_connector_action_error(
            &self.descriptor(),
            &input.action_id,
        ))
    }

    async fn cleanup_snapshot_for_policy(
        &self,
        context: &StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
    ) -> Result<Option<StoragePolicyCleanupDriverSnapshot>> {
        let descriptor = self.descriptor();
        if descriptor.credential_mode
            != aster_drive_storage::StorageConnectorCredentialMode::StaticSecret
        {
            return Ok(None);
        }
        common::static_credential_cleanup_snapshot(
            context,
            policy,
            descriptor.connector_id.as_str(),
            super::credential_schema_version(&descriptor)?,
        )
        .await
    }

    fn cleanup_snapshot_required(&self) -> bool {
        self.descriptor().credential_mode
            == aster_drive_storage::StorageConnectorCredentialMode::StaticSecret
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
#[derive(Clone)]
pub struct StorageConnectorRegistry {
    ordered: Vec<Arc<dyn StorageConnector>>,
    by_connector_id: HashMap<ConnectorId, Arc<dyn StorageConnector>>,
    localizations: HashMap<ConnectorId, StorageConnectorLocalization>,
}

impl StorageConnectorRegistry {
    pub(crate) fn new(connectors: Vec<Arc<dyn StorageConnector>>) -> Result<Self> {
        let mut by_connector_id = HashMap::with_capacity(connectors.len());
        let mut localizations = HashMap::with_capacity(connectors.len());
        for connector in &connectors {
            let descriptor = connector.descriptor();
            descriptor.validate().map_err(|error| {
                AsterError::internal_error(format!(
                    "storage connector '{}' declares an invalid descriptor: {error}",
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
            let localization = connector.localization()?;
            if localization.connector_id() != &descriptor.connector_id {
                return Err(AsterError::internal_error(format!(
                    "storage connector '{}' returned localization for '{}'",
                    descriptor.connector_id,
                    localization.connector_id()
                )));
            }
            localization
                .validate_message_ids(descriptor.localization_message_ids())
                .map_err(|error| {
                    AsterError::internal_error(format!(
                        "storage connector '{}' declares invalid localization: {error}",
                        descriptor.connector_id
                    ))
                })?;
            localizations.insert(descriptor.connector_id.clone(), localization);
        }
        validate_promotion_contracts(&connectors, &by_connector_id)?;
        Ok(Self {
            ordered: connectors,
            by_connector_id,
            localizations,
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

    /// Resolve a connector id supplied by an API client.
    ///
    /// Registry construction failures are internal errors, while an unknown id
    /// in a request is a validation error owned by the caller.
    pub(crate) fn require_input_connector(
        &self,
        connector_id: &ConnectorId,
    ) -> Result<&dyn StorageConnector> {
        connector_id
            .validate()
            .map_err(|error| AsterError::validation_error(error.to_string()))?;
        self.by_connector_id
            .get(connector_id)
            .map(AsRef::as_ref)
            .ok_or_else(|| {
                AsterError::validation_error(format!(
                    "storage connector '{}' is not available",
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
        self.by_connector_id
            .get(&connector_id)
            .map(AsRef::as_ref)
            .ok_or_else(|| {
                crate::errors::storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    format!(
                        "storage policy {} references unavailable connector '{}'",
                        policy.id, connector_id
                    ),
                )
            })
    }

    pub(crate) fn descriptors(&self) -> Vec<StorageConnectorDescriptor> {
        self.ordered
            .iter()
            .map(|connector| connector.descriptor())
            .collect()
    }

    pub(crate) fn require_localization(
        &self,
        connector_id: &ConnectorId,
    ) -> Result<&StorageConnectorLocalization> {
        self.localizations.get(connector_id).ok_or_else(|| {
            AsterError::internal_error(format!(
                "storage connector '{}' has no registered localization",
                connector_id
            ))
        })
    }

    /// Resolve connector-owned metadata for a provider policy action.
    ///
    /// An unknown connector is a registry/configuration failure. An unknown
    /// action is returned as None for generic metadata consumers such as audit;
    /// execution dispatch performs its own strict endpoint/schema resolution.
    pub(crate) fn action_descriptor(
        &self,
        connector_id: &ConnectorId,
        action_id: &StorageConnectorActionId,
    ) -> Result<Option<StorageConnectorActionDescriptor>> {
        Ok(self
            .require_connector(connector_id)?
            .descriptor()
            .actions
            .into_iter()
            .find(|candidate| {
                candidate.kind == StorageConnectorActionKind::Custom
                    && &candidate.action_id == action_id
            }))
    }

    pub(crate) fn promotion_descriptor(
        &self,
        target_connector_id: &ConnectorId,
        promotion_id: &StorageConnectorPromotionId,
    ) -> Result<Option<StorageConnectorPromotionDescriptor>> {
        Ok(self
            .require_connector(target_connector_id)?
            .descriptor()
            .promotions
            .into_iter()
            .find(|candidate| &candidate.promotion_id == promotion_id))
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

fn validate_promotion_contracts(
    connectors: &[Arc<dyn StorageConnector>],
    by_connector_id: &HashMap<ConnectorId, Arc<dyn StorageConnector>>,
) -> Result<()> {
    for target_connector in connectors {
        let target = target_connector.descriptor();
        for promotion in &target.promotions {
            let source = by_connector_id
                .get(&promotion.source_connector_id)
                .ok_or_else(|| {
                    AsterError::internal_error(format!(
                        "storage connector '{}' promotion '{}' references unavailable source connector '{}'",
                        target.connector_id,
                        promotion.promotion_id.as_str(),
                        promotion.source_connector_id,
                    ))
                })?
                .descriptor();
            match (source.credential_mode, target.credential_mode) {
                (
                    aster_drive_storage::StorageConnectorCredentialMode::StaticSecret,
                    aster_drive_storage::StorageConnectorCredentialMode::StaticSecret,
                ) if !promotion.credential_mappings.is_empty() => {}
                (
                    aster_drive_storage::StorageConnectorCredentialMode::None,
                    aster_drive_storage::StorageConnectorCredentialMode::None,
                ) if promotion.credential_mappings.is_empty() => {}
                _ => {
                    return Err(AsterError::internal_error(format!(
                        "storage connector '{}' promotion '{}' must map compatible static credentials or connect two credential-free connectors",
                        target.connector_id,
                        promotion.promotion_id.as_str(),
                    )));
                }
            }
            validate_promotion_requirements(&target, &source, promotion)?;
            validate_promotion_field_mappings(
                &target,
                &source,
                promotion,
                StorageConnectorFieldScope::ConnectorConfig,
                &promotion.config_mappings,
            )?;
            validate_promotion_field_mappings(
                &target,
                &source,
                promotion,
                StorageConnectorFieldScope::StaticCredential,
                &promotion.credential_mappings,
            )?;
            validate_required_promotion_targets(&target, promotion)?;
        }
    }
    Ok(())
}

fn validate_promotion_requirements(
    target: &StorageConnectorDescriptor,
    source: &StorageConnectorDescriptor,
    promotion: &StorageConnectorPromotionDescriptor,
) -> Result<()> {
    for requirement in &promotion.requirements {
        let field = require_promotion_field(
            target,
            source,
            promotion,
            StorageConnectorFieldScope::ConnectorConfig,
            &requirement.source_field,
            true,
        )?;
        let string_compatible = matches!(field.kind, StorageConnectorFieldKind::Text)
            || field.kind == StorageConnectorFieldKind::Select
                && field.select.as_ref().is_some_and(|select| {
                    select.value_kind
                        == aster_drive_storage::StorageConnectorSelectValueKind::String
                });
        if !string_compatible {
            return Err(AsterError::internal_error(format!(
                "storage connector '{}' promotion '{}' requirement field '{}' must be string-valued",
                target.connector_id,
                promotion.promotion_id.as_str(),
                requirement.source_field,
            )));
        }
    }
    Ok(())
}

fn validate_promotion_field_mappings(
    target: &StorageConnectorDescriptor,
    source: &StorageConnectorDescriptor,
    promotion: &StorageConnectorPromotionDescriptor,
    scope: StorageConnectorFieldScope,
    mappings: &[aster_drive_storage::StorageConnectorPromotionFieldMapping],
) -> Result<()> {
    for mapping in mappings {
        let source_field = require_promotion_field(
            target,
            source,
            promotion,
            scope,
            &mapping.source_field,
            true,
        )?;
        let target_field = require_promotion_field(
            target,
            target,
            promotion,
            scope,
            &mapping.target_field,
            false,
        )?;
        if !promotion_field_kinds_compatible(scope, source_field, target_field) {
            return Err(AsterError::internal_error(format!(
                "storage connector '{}' promotion '{}' maps incompatible {:?} field '{}' to {:?} field '{}'",
                target.connector_id,
                promotion.promotion_id.as_str(),
                source_field.kind,
                mapping.source_field,
                target_field.kind,
                mapping.target_field,
            )));
        }
    }
    Ok(())
}

fn require_promotion_field<'a>(
    target: &StorageConnectorDescriptor,
    descriptor: &'a StorageConnectorDescriptor,
    promotion: &StorageConnectorPromotionDescriptor,
    scope: StorageConnectorFieldScope,
    name: &str,
    source: bool,
) -> Result<&'a StorageConnectorFieldDescriptor> {
    descriptor
        .fields
        .iter()
        .find(|field| field.scope == scope && field.name == name)
        .ok_or_else(|| {
            AsterError::internal_error(format!(
                "storage connector '{}' promotion '{}' references undeclared {} {:?} field '{}' on connector '{}'",
                target.connector_id,
                promotion.promotion_id.as_str(),
                if source { "source" } else { "target" },
                scope,
                name,
                descriptor.connector_id,
            ))
        })
}

fn promotion_field_kinds_compatible(
    scope: StorageConnectorFieldScope,
    source: &StorageConnectorFieldDescriptor,
    target: &StorageConnectorFieldDescriptor,
) -> bool {
    if source.kind == StorageConnectorFieldKind::Select
        && target.kind == StorageConnectorFieldKind::Select
    {
        return source.select.as_ref().map(|select| select.value_kind)
            == target.select.as_ref().map(|select| select.value_kind);
    }
    if scope != StorageConnectorFieldScope::StaticCredential {
        return source.kind == target.kind;
    }
    source.kind == target.kind
        || matches!(
            (source.kind, target.kind),
            (
                StorageConnectorFieldKind::Text,
                StorageConnectorFieldKind::Secret
            ) | (
                StorageConnectorFieldKind::Secret,
                StorageConnectorFieldKind::Text
            )
        )
}

fn validate_required_promotion_targets(
    target: &StorageConnectorDescriptor,
    promotion: &StorageConnectorPromotionDescriptor,
) -> Result<()> {
    for field in target.fields.iter().filter(|field| {
        field.required
            && matches!(
                field.scope,
                StorageConnectorFieldScope::ConnectorConfig
                    | StorageConnectorFieldScope::StaticCredential
            )
    }) {
        let mappings = if field.scope == StorageConnectorFieldScope::ConnectorConfig {
            &promotion.config_mappings
        } else {
            &promotion.credential_mappings
        };
        if mappings
            .iter()
            .any(|mapping| mapping.target_field == field.name)
            || field.default_value.is_some()
        {
            continue;
        }
        return Err(AsterError::internal_error(format!(
            "storage connector '{}' promotion '{}' does not populate required target {:?} field '{}' via a mapping or an unconditional default_value",
            target.connector_id,
            promotion.promotion_id.as_str(),
            field.scope,
            field.name,
        )));
    }
    Ok(())
}

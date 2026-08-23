//! AsterDrive storage contracts, descriptors, and structured errors.
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::unreachable,
        clippy::expect_used,
        clippy::panic,
        clippy::unimplemented,
        clippy::todo
    )
)]

pub mod connector_config;
pub mod connector_descriptor;
pub mod connector_localization;
pub mod error;
pub mod field_contract;
pub mod object_key;
pub mod policy_behavior;
pub mod storage_policy_config;
pub mod traits;

pub use connector_config::{
    CONNECTOR_CONFIG_FORMAT_VERSION, ConnectorConfigCodecError, ConnectorConfigEnvelope,
    ConnectorId, ConnectorIdError, StorageConnectorFieldValue, decode_connector_config,
    encode_connector_config,
};
pub use connector_descriptor::{
    StorageConnectorActionDescriptor, StorageConnectorActionDescriptorError,
    StorageConnectorActionEndpoint, StorageConnectorActionId,
    StorageConnectorActionInvocationError, StorageConnectorActionKind,
    StorageConnectorActionSchema, StorageConnectorBadgeRgb, StorageConnectorCapabilities,
    StorageConnectorConfigSchema, StorageConnectorCredentialManagementDescriptor,
    StorageConnectorCredentialMode, StorageConnectorCustomActionDescriptorInput,
    StorageConnectorDeploymentScope, StorageConnectorDescriptor, StorageConnectorDescriptorError,
    StorageConnectorFieldCondition, StorageConnectorFieldDefaultMode,
    StorageConnectorFieldDefaultRule, StorageConnectorFieldDefaultValue,
    StorageConnectorFieldDescriptor, StorageConnectorFieldDescriptorError,
    StorageConnectorFieldKind, StorageConnectorFieldScope, StorageConnectorFieldValidation,
    StorageConnectorInactiveValueBehavior, StorageConnectorObjectNamingMode,
    StorageConnectorOptionsValidationError, StorageConnectorPromotionDescriptor,
    StorageConnectorPromotionFieldMapping, StorageConnectorPromotionId,
    StorageConnectorPromotionIdError, StorageConnectorPromotionRequirement,
    StorageConnectorPromotionValueMatcher, StorageConnectorSelectDataSource,
    StorageConnectorSelectDescriptor, StorageConnectorSelectOption,
    StorageConnectorSelectOptionInput, StorageConnectorSelectOptionValue,
    StorageConnectorSelectValueKind, StorageConnectorUploadWorkflows, custom_action_descriptor,
    normalize_storage_connector_action_input, normalize_storage_connector_config,
    normalize_storage_connector_custom_action_invocation, storage_connector_dynamic_select_field,
    storage_connector_select_field,
};
pub use connector_localization::{
    StorageConnectorLocalization, StorageConnectorLocalizationBundle,
    StorageConnectorLocalizationCatalog, StorageConnectorLocalizationError,
    StorageConnectorLocalizationManifest, StorageConnectorLocalizationMessage,
    StorageConnectorLocalizationTranslation,
};
pub use error::{
    MapStorageErr, Result, StorageError, StorageErrorContext, StorageErrorKind,
    storage_driver_error, storage_driver_error_with_context,
};
pub use policy_behavior::{
    STORAGE_POLICY_BEHAVIOR_FORMAT_VERSION, STORAGE_POLICY_BEHAVIOR_SCHEMA_VERSION,
    StoragePolicyBehaviorConfig, StoragePolicyBehaviorConfigCodecError,
    StoragePolicyBehaviorConfigEnvelope, decode_storage_policy_behavior_config,
    encode_storage_policy_behavior_config,
};
pub use storage_policy_config::{
    STORAGE_POLICY_CONFIG_FORMAT_VERSION, StoragePolicyConfigCodecError,
    StoragePolicyConfigEnvelope, decode_storage_policy_config, encode_storage_policy_config,
};
pub use traits::driver::{
    BlobMetadata, PresignedDownloadOptions, PresignedUploadRequest, StorageDriver,
    StoragePathVisitor,
};
pub use traits::{
    ListStorageDriver, LocalPathStorageDriver, MultipartStorageDriver, NativeMediaMetadataRequest,
    NativeMediaMetadataResult, NativeMediaMetadataStorageDriver, NativeThumbnailRequest,
    NativeThumbnailStorageDriver, PresignedStorageDriver, ProviderResumableUploadCapabilities,
    ProviderResumableUploadDriver, ProviderResumableUploadFragmentOutcome,
    ProviderResumableUploadSession, ProviderResumableUploadStatus, StorageCapacityInfo,
    StorageCapacityStatus, StorageDriverExtensions, StreamUploadAttempt, StreamUploadCleanup,
    StreamUploadDriver, UploadedMultipartPart,
};

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
pub mod error;
pub mod field_contract;
pub mod object_key;
pub mod policy_behavior;
pub mod traits;

pub use connector_config::{
    CONNECTOR_CONFIG_FORMAT_VERSION, ConnectorConfigCodecError, ConnectorConfigEnvelope,
    ConnectorId, ConnectorIdError, decode_connector_config, encode_connector_config,
};
pub use connector_descriptor::{
    StorageConnectorActionDescriptor, StorageConnectorActionEndpoint, StorageConnectorActionKind,
    StorageConnectorAffordanceAction, StorageConnectorCapabilities, StorageConnectorConfigSchema,
    StorageConnectorCredentialMode, StorageConnectorDeploymentScope, StorageConnectorDescriptor,
    StorageConnectorFieldDefaultValue, StorageConnectorFieldDescriptor, StorageConnectorFieldKind,
    StorageConnectorFieldScope, StorageConnectorFieldValidation, StorageConnectorObjectNamingMode,
    StorageConnectorOptionsValidationError, StorageConnectorUploadWorkflows,
    StoragePolicyExecutableAction, normalize_storage_connector_config,
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
pub use traits::driver::{
    BlobMetadata, PresignedDownloadOptions, StorageDriver, StoragePathVisitor,
};
pub use traits::{
    ListStorageDriver, LocalPathStorageDriver, MultipartStorageDriver, NativeMediaMetadataRequest,
    NativeMediaMetadataResult, NativeMediaMetadataStorageDriver, NativeThumbnailRequest,
    NativeThumbnailStorageDriver, PresignedStorageDriver, ProviderResumableUploadCapabilities,
    ProviderResumableUploadDriver, ProviderResumableUploadFragmentOutcome,
    ProviderResumableUploadSession, ProviderResumableUploadStatus, StorageCapacityInfo,
    StorageCapacityStatus, StorageDriverExtensions, StreamUploadDriver, UploadedMultipartPart,
};

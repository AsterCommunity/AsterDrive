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
pub mod traits;

pub use connector_config::{
    CONNECTOR_CONFIG_FORMAT_VERSION, ConnectorConfigEnvelope, ConnectorId, ConnectorIdError,
};
pub use connector_descriptor::{
    StorageConnectorActionDescriptor, StorageConnectorActionEndpoint, StorageConnectorActionKind,
    StorageConnectorAffordanceAction, StorageConnectorCapabilities, StorageConnectorCredentialMode,
    StorageConnectorDeploymentScope, StorageConnectorDescriptor, StorageConnectorFieldDefaultValue,
    StorageConnectorFieldDescriptor, StorageConnectorFieldKind, StorageConnectorFieldScope,
    StorageConnectorFieldValidation, StorageConnectorObjectNamingMode,
    StorageConnectorOptionsValidationError, StorageConnectorUploadWorkflows,
    StoragePolicyExecutableAction, normalize_storage_connector_config,
};
pub use error::{
    MapStorageErr, Result, StorageError, StorageErrorContext, StorageErrorKind,
    storage_driver_error, storage_driver_error_with_context,
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

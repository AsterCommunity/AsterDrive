//! AsterDrive storage contracts, descriptors, and structured errors.
#![deny(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
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

pub mod connector_descriptor;
pub mod error;
pub mod field_contract;
pub mod object_key;
pub mod traits;

pub use connector_descriptor::{
    StorageConnectorActionDescriptor, StorageConnectorActionEndpoint, StorageConnectorActionKind,
    StorageConnectorAffordanceAction, StorageConnectorCapabilities, StorageConnectorCredentialMode,
    StorageConnectorDeploymentScope, StorageConnectorDescriptor,
    StorageConnectorDescriptorProvider, StorageConnectorFieldDescriptor, StorageConnectorFieldKind,
    StorageConnectorFieldScope, StorageConnectorObjectNamingMode, StorageConnectorUploadWorkflows,
    StoragePolicyExecutableAction,
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

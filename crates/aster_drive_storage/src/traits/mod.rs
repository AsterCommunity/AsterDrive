//! Storage driver trait contracts.

pub mod driver;
pub mod extensions;
pub mod multipart;

pub use driver::{
    BlobMetadata, DirectDownloadCredentials, DirectDownloadOptions, DirectDownloadRequest,
    PresignedUploadRequest, StorageDriver, StoragePathVisitControl, StoragePathVisitor,
};
pub use extensions::{
    DirectDownloadStorageDriver, ExactSizeReader, ListStorageDriver, LocalPathStorageDriver,
    NativeMediaMetadataRequest, NativeMediaMetadataResult, NativeMediaMetadataStorageDriver,
    NativeThumbnailRequest, NativeThumbnailStorageDriver, PresignedUploadStorageDriver,
    ProviderResumableUploadCapabilities, ProviderResumableUploadDriver,
    ProviderResumableUploadFragmentOutcome, ProviderResumableUploadSession,
    ProviderResumableUploadStatus, StorageCapacityInfo, StorageCapacityStatus,
    StorageDriverExtensions, StreamUploadAttempt, StreamUploadCleanup, StreamUploadDriver,
    checked_upload_size, exact_size_error_kind,
};
pub use multipart::{MultipartStorageDriver, UploadedMultipartPart};

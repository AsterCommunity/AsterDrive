//! Storage driver trait contracts.

pub mod driver;
pub mod extensions;
pub mod multipart;

pub use driver::{
    BlobMetadata, DirectDownloadCredentials, DirectDownloadOptions, DirectDownloadRequest,
    PresignedUploadRequest, StorageDriver, StoragePathVisitor,
};
pub use extensions::{
    DirectDownloadStorageDriver, ExactSizeReader, ListStorageDriver, LocalPathStorageDriver,
    NativeMediaMetadataRequest, NativeMediaMetadataResult, NativeMediaMetadataStorageDriver,
    NativeThumbnailRequest, NativeThumbnailStorageDriver, PresignedUploadStorageDriver,
    ProviderResumableUploadCapabilities, ProviderResumableUploadDriver,
    ProviderResumableUploadFragmentOutcome, ProviderResumableUploadSession,
    ProviderResumableUploadStatus, StorageCapacityInfo, StorageCapacityStatus,
    StorageDriverExtensions, StreamUploadAttempt, StreamUploadCleanup, StreamUploadDriver,
    checked_stream_upload_size,
};
pub use multipart::{MultipartStorageDriver, UploadedMultipartPart};

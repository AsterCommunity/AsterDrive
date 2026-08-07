//! Storage driver trait contracts.

pub mod driver;
pub mod extensions;
pub mod multipart;

pub use driver::{
    BlobMetadata, PresignedDownloadOptions, PresignedUploadRequest, StorageDriver,
    StoragePathVisitor,
};
pub use extensions::{
    ListStorageDriver, LocalPathStorageDriver, NativeMediaMetadataRequest,
    NativeMediaMetadataResult, NativeMediaMetadataStorageDriver, NativeThumbnailRequest,
    NativeThumbnailStorageDriver, PresignedStorageDriver, ProviderResumableUploadCapabilities,
    ProviderResumableUploadDriver, ProviderResumableUploadFragmentOutcome,
    ProviderResumableUploadSession, ProviderResumableUploadStatus, StorageCapacityInfo,
    StorageCapacityStatus, StorageDriverExtensions, StreamUploadDriver,
};
pub use multipart::{MultipartStorageDriver, UploadedMultipartPart};

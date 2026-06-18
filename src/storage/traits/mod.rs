//! Storage driver trait contracts.

pub mod driver;
pub mod extensions;
pub mod multipart;

pub use driver::{BlobMetadata, PresignedDownloadOptions, StorageDriver, StoragePathVisitor};
pub use extensions::{
    ListStorageDriver, LocalPathStorageDriver, NativeMediaMetadataRequest,
    NativeMediaMetadataResult, NativeMediaMetadataStorageDriver, NativeThumbnailRequest,
    NativeThumbnailStorageDriver, PresignedStorageDriver, StorageCapacityInfo,
    StorageCapacityStatus, StreamUploadDriver,
};
pub use multipart::{MultipartStorageDriver, UploadedMultipartPart};

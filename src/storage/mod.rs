//! 存储抽象与实现导出。

pub mod drivers;
pub mod error;
mod metrics_driver;
pub mod object_key;
pub mod policy_snapshot;
pub mod registry;
pub mod remote_protocol;
pub mod traits;

pub use error::StorageErrorKind;
pub use policy_snapshot::PolicySnapshot;
pub use registry::DriverRegistry;
pub use traits::driver::{
    BlobMetadata, PresignedDownloadOptions, StorageDriver, StoragePathVisitor,
    driver_type_supports_native_thumbnail,
};
pub use traits::{
    ListStorageDriver, LocalPathStorageDriver, MultipartStorageDriver, NativeMediaMetadataRequest,
    NativeMediaMetadataResult, NativeMediaMetadataStorageDriver, NativeThumbnailRequest,
    NativeThumbnailStorageDriver, PresignedStorageDriver, StorageCapacityInfo,
    StorageCapacityStatus, StreamUploadDriver,
};

// 内部 re-export 供宏和错误处理使用
pub(crate) use crate::errors::MapAsterErr;

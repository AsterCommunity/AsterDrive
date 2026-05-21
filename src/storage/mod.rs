//! 存储抽象与实现导出。

pub mod driver;
pub mod drivers;
pub mod error;
pub mod extensions;
mod metrics_driver;
pub mod multipart;
pub mod object_key;
pub mod policy_snapshot;
pub mod registry;
pub mod remote_protocol;

pub use driver::{
    BlobMetadata, PresignedDownloadOptions, StorageDriver, StoragePathVisitor,
    driver_type_supports_native_thumbnail,
};
pub use error::StorageErrorKind;
pub use extensions::{
    ListStorageDriver, LocalPathStorageDriver, PresignedStorageDriver, StreamUploadDriver,
};
pub use multipart::MultipartStorageDriver;
pub use policy_snapshot::PolicySnapshot;
pub use registry::DriverRegistry;

// 内部 re-export 供宏和错误处理使用
pub(crate) use crate::errors::MapAsterErr;

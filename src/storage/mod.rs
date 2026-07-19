//! 存储抽象与实现导出。
//!
//! 调用侧统一从 `crate::storage::{...}` 导入 storage trait、扩展 trait 和公共 DTO。
//! `crate::storage::traits::*` 深路径只保留给 storage 内部实现层，用来强调 trait
//! 的定义来源或避免驱动实现文件中的命名歧义。

pub mod connector_descriptor;
pub mod connectors;
pub mod drivers;
pub mod error;
pub(crate) mod field_contract;
mod metrics_driver;
pub mod object_key;
pub mod policy_snapshot;
pub mod registry;
pub mod remote_protocol;
pub mod traits;

pub use connector_descriptor::{
    StorageConnectorActionDescriptor, StorageConnectorActionEndpoint, StorageConnectorActionKind,
    StorageConnectorAffordanceAction, StorageConnectorCapabilities, StorageConnectorCredentialMode,
    StorageConnectorDescriptor, StorageConnectorDescriptorProvider,
    StorageConnectorFieldDescriptor, StorageConnectorFieldKind, StorageConnectorFieldScope,
    StorageConnectorObjectNamingMode, StorageConnectorUploadWorkflows,
    StoragePolicyExecutableAction,
};
pub use connectors::{
    ExecuteDraftStorageConnectorActionInput, ExecuteSavedStorageConnectorActionInput,
    MicrosoftGraphApplicationConfigInput, StorageConnectorActionResult,
    StorageConnectorApplicationConfigInput, StorageConnectorConnectionInput,
    TencentCosCorsConfigResult, TestDraftStorageConnectorConnectionInput,
};
pub use error::StorageErrorKind;
pub use policy_snapshot::PolicySnapshot;
pub use registry::DriverRegistry;
pub use traits::driver::{
    BlobMetadata, PresignedDownloadOptions, StorageDriver, StoragePathVisitor,
};
pub use traits::{
    ListStorageDriver, LocalPathStorageDriver, MultipartStorageDriver, NativeMediaMetadataRequest,
    NativeMediaMetadataResult, NativeMediaMetadataStorageDriver, NativeThumbnailRequest,
    NativeThumbnailStorageDriver, PresignedStorageDriver, ProviderResumableUploadCapabilities,
    ProviderResumableUploadDriver, ProviderResumableUploadSession, ProviderResumableUploadStatus,
    StorageCapacityInfo, StorageCapacityStatus, StorageDriverExtensions, StreamUploadDriver,
    UploadedMultipartPart,
};

// 内部 re-export 供宏和错误处理使用
pub(crate) use crate::errors::MapAsterErr;

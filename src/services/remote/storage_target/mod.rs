//! 服务模块：`remote::storage_target`。

mod driver;
mod local_profiles;
mod models;
mod normalization;
mod paths;
mod reconciliation;
mod remote;
mod target;
#[cfg(test)]
mod tests;

pub use driver::{
    RemoteStorageTargetDriverDescriptor, RemoteStorageTargetDriverFieldDescriptor,
    RemoteStorageTargetDriverFieldKind,
};
pub(crate) use driver::{
    registered_remote_storage_target_driver_types, remote_storage_target_driver_descriptor,
};
pub use local_profiles::{create, delete, list, update};
pub use models::ResolvedRemoteStorageTarget;
pub use remote::{
    create_remote, delete_remote, list_remote, list_remote_driver_descriptors, update_remote,
};
pub use target::{resolve_effective_target, resolve_target_by_key};

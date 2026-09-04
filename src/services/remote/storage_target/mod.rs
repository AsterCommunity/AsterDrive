//! 服务模块：`remote::storage_target`。

mod driver;
mod local_profiles;
mod migration;
mod models;
mod normalization;
mod paths;
mod reconciliation;
mod remote;
mod target;
#[cfg(test)]
mod tests;

pub(crate) use driver::remote_storage_target_descriptor_from_connector;
pub use local_profiles::{create, delete, list, update};
pub(crate) use migration::convert_legacy_rows;
pub use models::ResolvedRemoteStorageTarget;
pub use remote::{
    create_remote, delete_remote, list_remote, list_remote_connector_descriptors, update_remote,
};
pub use target::{resolve_effective_target, resolve_target_by_key};

//! 服务模块：`remote::storage_target`。

mod credential;
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

pub use driver::RemoteStorageTargetConnectorDescriptor;
pub(crate) use driver::{
    registered_remote_storage_target_connector_ids, remote_storage_target_connector_descriptor,
};
pub use local_profiles::{create, delete, list, update};
pub(crate) use migration::migrate_legacy_remote_storage_targets;
pub use models::ResolvedRemoteStorageTarget;
pub use remote::{
    create_remote, delete_remote, list_remote, list_remote_connector_descriptors, update_remote,
};
pub use target::{resolve_effective_target, resolve_target_by_key};

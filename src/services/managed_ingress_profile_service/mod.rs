//! 服务模块：`managed_ingress_profile_service`。

mod driver;
mod local_profiles;
mod models;
mod normalization;
mod paths;
mod remote;
#[cfg(test)]
mod tests;

pub(crate) use driver::registered_managed_ingress_driver_types;
pub use driver::{
    ManagedIngressDriverDescriptor, ManagedIngressDriverFieldDescriptor,
    ManagedIngressDriverFieldKind,
};
pub use local_profiles::{create, delete, list, resolve_effective_target, update};
pub use models::ResolvedIngressTarget;
pub use remote::{
    create_remote, delete_remote, list_remote, list_remote_driver_descriptors, update_remote,
};

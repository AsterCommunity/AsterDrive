//! 服务模块：`files::lock`。

mod cleanup;
mod domain;
mod enforcement;
mod lifecycle;
mod listing;
mod models;
mod owner_info;
mod ownership;
mod path;
mod projection;
#[cfg(test)]
mod tests;

pub use cleanup::{cleanup_expired, cleanup_expired_with_audit};
pub use domain::{LockRoot, LockRootSummary, LockTarget, LockWorkspace, ResourceLockState};
pub use enforcement::{
    LockMutationCredentials, SubmittedLockCredentials, enforce_collection_membership_mutation_on,
    enforce_file_mutation, enforce_file_mutation_on, enforce_folder_mutation,
    enforce_folder_mutation_on, lock_workspace_for_mutation_on,
};
pub(crate) use lifecycle::{LockAcquireCommand, acquire_after_namespace_lock_on};
pub use lifecycle::{
    acquire, acquire_on, force_unlock, force_unlock_with_audit, lock, refresh_by_token_on,
    replace_owner_info_and_timeout_by_token_on, unlock, unlock_by_token, unlock_by_token_on,
};
pub use listing::list_paginated;
pub use models::{
    ResourceLock, ResourceLockOwnerInfo, TextLockOwnerInfo, WebdavLockOwnerInfo, WopiLockOwnerInfo,
};
pub(crate) use owner_info::deserialize_resource_lock_owner_info;
pub use path::resolve_entity_path;
pub(crate) use projection::{LockStateMap, load_for_resources, load_for_scope, state_for};

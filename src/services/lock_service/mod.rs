//! 服务模块：`lock_service`。

mod cleanup;
mod lifecycle;
mod listing;
mod models;
mod owner_info;
mod ownership;
mod path;
mod state;
#[cfg(test)]
mod tests;

pub use cleanup::{cleanup_expired, cleanup_expired_with_audit};
pub use lifecycle::{force_unlock, force_unlock_with_audit, lock, unlock, unlock_by_token};
pub use listing::list_paginated;
pub use models::{
    ResourceLock, ResourceLockOwnerInfo, TextLockOwnerInfo, WebdavLockOwnerInfo, WopiLockOwnerInfo,
};
pub(crate) use owner_info::{
    deserialize_resource_lock_owner_info, serialize_resource_lock_owner_info,
};
pub use path::resolve_entity_path;
pub(crate) use state::clear_entity_locked_if_unlocked;
pub use state::set_entity_locked;

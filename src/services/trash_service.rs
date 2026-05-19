//! 回收站服务聚合入口。

mod cleanup;
mod common;
mod listing;
mod models;
mod purge;
mod restore;

pub use cleanup::cleanup_expired;
pub use common::load_retention_days;
pub use listing::{expires_cursor_to_deleted_cursor, list_team_trash, list_trash};
pub use models::{TrashContents, TrashFileCursor, TrashFileItem, TrashFolderItem};
pub(crate) use purge::{publish_purge_all_storage_change, purge_all_in_scope_silent};
pub use purge::{
    purge_all, purge_all_team, purge_file, purge_folder, purge_team_file, purge_team_folder,
};
pub use restore::{restore_file, restore_folder, restore_team_file, restore_team_folder};

const DEFAULT_RETENTION_DAYS: i64 = 7;
const PURGE_ALL_BATCH_SIZE: u64 = 100;

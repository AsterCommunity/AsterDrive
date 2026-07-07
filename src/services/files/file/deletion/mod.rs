//! 文件删除子模块聚合入口。

mod blob_cleanup;
mod purge;
mod soft_delete;

pub(crate) use blob_cleanup::{
    cleanup_unreferenced_blob, cleanup_unreferenced_blob_with_driver,
    ensure_blob_cleanup_if_unreferenced,
};
pub(crate) use purge::{
    BatchPurgeSummary, batch_purge_in_resource_scope, batch_purge_in_resource_scope_silent,
    batch_purge_in_scope,
};
pub use purge::{batch_purge, purge};
pub use soft_delete::delete;
pub(crate) use soft_delete::delete_in_scope;

#[cfg(test)]
mod tests;

//! 存储驱动实现：`local`。

mod driver_impl;
mod listing;
mod paths;
mod promote;
mod stream_upload;
#[cfg(test)]
mod tests;

use std::path::PathBuf;

use crate::entities::storage_policy;
use crate::errors::Result;

pub use paths::{effective_base_path, resolved_base_path, upload_staging_path};
pub use promote::promote_local_file_if_absent;

pub struct LocalDriver {
    pub(super) base_path: PathBuf,
}

impl LocalDriver {
    pub fn new(policy: &storage_policy::Model) -> Result<Self> {
        Ok(Self {
            base_path: resolved_base_path(policy)?,
        })
    }

    pub(super) fn full_path(&self, path: &str) -> Result<PathBuf> {
        paths::resolve_path_within_root(
            &self.base_path,
            &paths::sanitize_relative_path(path)?,
            path,
        )
    }
}

//! 存储驱动实现：`local`。

mod copy;
mod driver_impl;
mod listing;
mod paths;
mod promote;
mod stream_upload;
#[cfg(test)]
mod tests;

use std::path::PathBuf;

use aster_drive_storage::Result;

pub(crate) use copy::copy_file_with_checkpoint;
pub use paths::{effective_base_path, resolved_base_path, upload_staging_path};
pub use promote::{promote_local_file_if_absent, promote_local_file_if_absent_with_check};

pub(crate) const DEFAULT_LOCAL_STORAGE_PATH: &str = "./data/uploads";

pub struct LocalDriver {
    pub(super) base_path: PathBuf,
}

impl LocalDriver {
    /// Builds a local runtime driver from the connector-resolved storage root.
    ///
    /// Database entities deliberately stay outside the driver boundary. The
    /// local connector owns policy/config decoding and passes only the runtime
    /// value needed for filesystem I/O.
    pub fn new(base_path: &str) -> Result<Self> {
        Ok(Self {
            base_path: resolved_base_path(base_path)?,
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

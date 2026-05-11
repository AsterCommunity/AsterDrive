//! RAII cleanup guards for short-lived runtime resources.

use std::path::{Path, PathBuf};

/// RAII 临时文件守卫。用于短生命周期 runtime 临时文件，防止错误返回或 panic unwind
/// 时遗漏清理；进程被 kill 后的残留仍由启动时 runtime temp 清理兜底。
pub struct TempFileGuard {
    path: PathBuf,
    cleanup_label: &'static str,
}

impl TempFileGuard {
    pub fn new(path: PathBuf, cleanup_label: &'static str) -> Self {
        Self {
            path,
            cleanup_label,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(
                path = ?self.path,
                cleanup = self.cleanup_label,
                "failed to cleanup temp file: {error}"
            );
        }
    }
}

/// RAII 临时目录守卫。用于短生命周期 runtime 临时目录；如果进程异常退出，
/// 下次启动的 runtime temp 清理仍会兜底处理残留目录。
pub struct TempDirGuard {
    path: PathBuf,
    cleanup_label: &'static str,
}

impl TempDirGuard {
    pub fn new(path: PathBuf, cleanup_label: &'static str) -> Self {
        Self {
            path,
            cleanup_label,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(
                path = %self.path.display(),
                cleanup = self.cleanup_label,
                "failed to cleanup temp dir: {error}"
            );
        }
    }
}

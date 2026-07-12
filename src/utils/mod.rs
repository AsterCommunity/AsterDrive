//! 工具模块导出。

pub mod hash;
pub(crate) mod http_validators;
pub mod id;
pub mod net;
pub mod numbers;
pub mod paths;
pub mod raii;

use crate::errors::{AsterError, Result};

pub const OUTBOUND_HTTP_USER_AGENT: &str = concat!("AsterDrive/", env!("CARGO_PKG_VERSION"));

/// 校验资源归属权，不匹配则返回 403
pub fn verify_owner(entity_user_id: i64, user_id: i64, entity_name: &str) -> Result<()> {
    if entity_user_id != user_id {
        return Err(AsterError::auth_forbidden(format!(
            "not your {entity_name}"
        )));
    }
    Ok(())
}

/// 校验可为空的 owner 字段；团队空间对象通常没有 personal owner。
pub fn verify_optional_owner(
    entity_user_id: Option<i64>,
    user_id: i64,
    entity_name: &str,
) -> Result<()> {
    verify_owner(
        entity_user_id.ok_or_else(|| {
            AsterError::auth_forbidden(format!("{entity_name} has no personal owner"))
        })?,
        user_id,
        entity_name,
    )
}

/// 清理临时文件/目录，失败时记录 warn 日志而不是静默忽略
pub async fn cleanup_temp_file(path: &str) {
    if let Err(e) = tokio::fs::remove_file(path).await
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!("failed to cleanup temp file {path}: {e}");
    }
}

pub async fn cleanup_temp_dir(path: &str) {
    // macOS Spotlight/Finder 可能在删除过程中往目录里塞 .DS_Store 等文件，
    // 导致 remove_dir_all 的最终 rmdir 返回 ENOTEMPTY，重试即可。
    for _ in 0..3 {
        match tokio::fs::remove_dir_all(path).await {
            Ok(()) => return,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) if e.kind() == std::io::ErrorKind::DirectoryNotEmpty => {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            Err(e) => {
                tracing::warn!("failed to cleanup temp dir {path}: {e}");
                return;
            }
        }
    }
    if let Err(e) = tokio::fs::remove_dir_all(path).await
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!("failed to cleanup temp dir {path}: {e}");
    }
}

/// 启动时只清理短命 runtime 临时目录，不碰任务产物和其他 temp 内容。
pub async fn cleanup_runtime_temp_root(temp_root: &str) {
    cleanup_temp_dir(&paths::runtime_temp_dir(temp_root)).await;
}

#[cfg(test)]
mod tests {
    use super::{cleanup_runtime_temp_root, cleanup_temp_dir, paths};
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_cleanup_runtime_temp_root_only_removes_runtime_namespace() {
        let temp_root =
            std::env::temp_dir().join(format!("aster-drive-utils-{}", uuid::Uuid::new_v4()));
        let temp_root = temp_root.to_string_lossy().into_owned();
        let runtime_dir = PathBuf::from(paths::runtime_temp_dir(&temp_root));
        let task_dir = PathBuf::from(paths::task_temp_dir(&temp_root, 42));

        tokio::fs::create_dir_all(&runtime_dir).await.unwrap();
        tokio::fs::create_dir_all(&task_dir).await.unwrap();
        tokio::fs::write(runtime_dir.join("session.tmp"), b"runtime")
            .await
            .unwrap();
        tokio::fs::write(task_dir.join("artifact.bin"), b"task")
            .await
            .unwrap();

        cleanup_runtime_temp_root(&temp_root).await;

        assert!(!runtime_dir.exists());
        assert!(task_dir.exists());
        assert!(task_dir.join("artifact.bin").exists());

        cleanup_temp_dir(&temp_root).await;
    }
}

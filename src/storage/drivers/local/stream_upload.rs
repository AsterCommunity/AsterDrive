use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWriteExt};

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::storage::traits::extensions::StreamUploadDriver;
use crate::utils::numbers;

use super::LocalDriver;

#[async_trait]
impl StreamUploadDriver for LocalDriver {
    async fn put_reader(
        &self,
        storage_path: &str,
        mut reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> Result<String> {
        let declared_size = numbers::i64_to_u64(size, "local put_reader declared size")?;

        // 创建临时文件
        let temp_path = std::env::temp_dir().join(format!(
            "aster_put_reader_{}_{}",
            std::process::id(),
            rand::random::<u64>()
        ));

        // 流式写入临时文件
        let mut file = tokio::fs::File::create(&temp_path)
            .await
            .map_aster_err(AsterError::storage_driver_error)?;

        let written = tokio::io::copy(&mut reader, &mut file)
            .await
            .map_aster_err_ctx("write temp file", AsterError::storage_driver_error)?;

        // 验证实际写入大小与声明大小一致
        if written != declared_size {
            if let Err(error) = tokio::fs::remove_file(&temp_path).await
                && error.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(
                    path = %temp_path.display(),
                    "failed to cleanup local stream temp file after size mismatch: {error}"
                );
            }
            return Err(AsterError::storage_driver_error(format!(
                "size mismatch: declared {}, actual written {}",
                size, written
            )));
        }

        // 确保数据落盘
        file.flush()
            .await
            .map_aster_err(AsterError::storage_driver_error)?;
        drop(file);

        // 使用 put_file 完成上传
        let temp_path_str = temp_path.to_str().ok_or_else(|| {
            AsterError::storage_driver_error("temp upload path is not valid UTF-8")
        })?;
        let result = self.put_file(storage_path, temp_path_str).await;

        if let Err(error) = tokio::fs::remove_file(&temp_path).await
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(
                path = %temp_path.display(),
                storage_path,
                "failed to cleanup local stream temp file: {error}"
            );
        }

        result
    }

    async fn put_file(&self, storage_path: &str, local_path: &str) -> Result<String> {
        let full = self.full_path(storage_path)?;
        if let Some(parent) = full.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_aster_err(AsterError::storage_driver_error)?;
        }
        // rename 是零拷贝（同一文件系统），跨文件系统 fallback 到 copy + delete
        if tokio::fs::rename(local_path, &full).await.is_err() {
            tokio::fs::copy(local_path, &full)
                .await
                .map_aster_err_ctx("copy file", AsterError::storage_driver_error)?;
            if let Err(error) = tokio::fs::remove_file(local_path).await
                && error.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(
                    local_path,
                    storage_path,
                    "failed to cleanup source file after local copy fallback: {error}"
                );
            }
        }
        Ok(storage_path.to_string())
    }
}

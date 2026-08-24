use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use aster_drive_storage::traits::extensions::{
    StreamUploadAttempt, StreamUploadCleanup, StreamUploadDriver,
};
use aster_drive_storage::{MapStorageErr, StorageErrorKind, storage_driver_error};
use aster_forge_utils::numbers;

use super::LocalDriver;

#[async_trait]
impl StreamUploadDriver for LocalDriver {
    async fn put_reader(
        &self,
        storage_path: &str,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> aster_drive_storage::Result<String> {
        let attempt = StreamUploadAttempt::new(storage_path, size)?;
        let result = async {
            self.stage_attempt(&attempt, reader).await?;
            self.commit_attempt(&attempt).await
        }
        .await;
        if let Err(error) = &result {
            match self.abort_attempt(&attempt).await {
                Ok(StreamUploadCleanup::NotRequired | StreamUploadCleanup::Cleaned) => {}
                Ok(outcome) => tracing::warn!(
                    staging_path = %attempt.staging_path,
                    ?outcome,
                    "local attempt cleanup deferred after upload error: {error}"
                ),
                Err(abort_error) => tracing::warn!(
                    staging_path = %attempt.staging_path,
                    "local attempt cleanup failed after upload error: {abort_error}"
                ),
            }
        }
        result
    }

    async fn stage_attempt(
        &self,
        attempt: &StreamUploadAttempt,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
    ) -> aster_drive_storage::Result<()> {
        let expected_size =
            numbers::i64_to_u64(attempt.expected_size, "local stream upload declared size")
                .map_storage_err(StorageErrorKind::Misconfigured)?;
        let staging_path = self.full_path(&attempt.staging_path)?;
        if let Some(parent) = staging_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_storage_err(StorageErrorKind::Transient)?;
        }

        let mut reader = reader;
        let mut file = tokio::fs::File::create(&staging_path)
            .await
            .map_storage_err(StorageErrorKind::Transient)?;
        let written =
            match tokio::io::copy(&mut (&mut *reader).take(expected_size), &mut file).await {
                Ok(written) => written,
                Err(error) => {
                    cleanup_local_staging_file(&staging_path, "reader error").await;
                    return Err(error).map_storage_err_ctx(
                        StorageErrorKind::Transient,
                        "write local upload attempt",
                    );
                }
            };
        if written != expected_size {
            cleanup_local_staging_file(&staging_path, "size mismatch").await;
            return Err(storage_driver_error(
                StorageErrorKind::Precondition,
                format!(
                    "local stream upload size mismatch: declared {}, actual {written}",
                    attempt.expected_size
                ),
            ));
        }
        let mut extra = [0_u8; 1];
        match reader.read(&mut extra).await {
            Ok(0) => {}
            Ok(_) => {
                cleanup_local_staging_file(&staging_path, "size mismatch").await;
                return Err(storage_driver_error(
                    StorageErrorKind::Precondition,
                    format!(
                        "local stream upload size mismatch: declared {}, actual exceeds declared size",
                        attempt.expected_size
                    ),
                ));
            }
            Err(error) => {
                cleanup_local_staging_file(&staging_path, "length probe error").await;
                return Err(error).map_storage_err_ctx(
                    StorageErrorKind::Transient,
                    "check local upload attempt length",
                );
            }
        }
        if let Err(error) = file.flush().await {
            cleanup_local_staging_file(&staging_path, "flush error").await;
            return Err(error).map_storage_err(StorageErrorKind::Transient);
        }
        if let Err(error) = file.sync_data().await {
            drop(file);
            cleanup_local_staging_file(&staging_path, "sync error").await;
            return Err(error)
                .map_storage_err_ctx(StorageErrorKind::Transient, "sync local upload attempt");
        }
        drop(file);
        Ok(())
    }

    async fn commit_attempt(
        &self,
        attempt: &StreamUploadAttempt,
    ) -> aster_drive_storage::Result<String> {
        let staging_path = self.full_path(&attempt.staging_path)?;
        let destination_path = self.full_path(&attempt.storage_path)?;
        tokio::fs::rename(staging_path, destination_path)
            .await
            .map_storage_err_ctx(StorageErrorKind::Transient, "publish local upload attempt")?;
        Ok(attempt.storage_path.clone())
    }

    async fn abort_attempt(
        &self,
        attempt: &StreamUploadAttempt,
    ) -> aster_drive_storage::Result<StreamUploadCleanup> {
        let staging_path = self.full_path(&attempt.staging_path)?;
        match tokio::fs::remove_file(&staging_path).await {
            Ok(()) => Ok(StreamUploadCleanup::Cleaned),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(StreamUploadCleanup::Cleaned)
            }
            Err(error) => {
                tracing::warn!(
                    staging_path = %staging_path.display(),
                    "local attempt cleanup deferred: {error}"
                );
                Ok(StreamUploadCleanup::Deferred)
            }
        }
    }

    async fn put_file(
        &self,
        storage_path: &str,
        local_path: &str,
    ) -> aster_drive_storage::Result<String> {
        let full = self.full_path(storage_path)?;
        if let Some(parent) = full.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_storage_err(StorageErrorKind::Transient)?;
        }
        // rename 是零拷贝（同一文件系统），跨文件系统 fallback 到 copy + delete
        if tokio::fs::rename(local_path, &full).await.is_err() {
            tokio::fs::copy(local_path, &full)
                .await
                .map_storage_err_ctx(StorageErrorKind::Transient, "copy file")?;
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

async fn cleanup_local_staging_file(path: &std::path::Path, reason: &str) {
    match tokio::fs::remove_file(path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => tracing::warn!(
            staging_path = %path.display(),
            reason,
            "failed to cleanup local staging file: {error}"
        ),
    }
}

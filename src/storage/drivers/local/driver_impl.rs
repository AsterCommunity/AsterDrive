use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncRead, AsyncSeekExt};

use aster_drive_storage::traits::driver::{BlobMetadata, StorageDriver};
use aster_drive_storage::traits::extensions::{
    LocalPathStorageDriver, NativePublishStorageDriver, StorageCapacityInfo, StorageCapacityStatus,
};
use aster_drive_storage::{MapStorageErr, StorageError, StorageErrorKind, storage_driver_error};
use aster_forge_utils::numbers::u64_to_i64;

use super::LocalDriver;

#[async_trait]
impl StorageDriver for LocalDriver {
    async fn put(&self, path: &str, data: &[u8]) -> aster_drive_storage::Result<String> {
        let full = self.full_path(path)?;
        if let Some(parent) = full.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_storage_err(StorageErrorKind::Transient)?;
        }
        tokio::fs::write(&full, data)
            .await
            .map_storage_err(StorageErrorKind::Transient)?;
        Ok(path.to_string())
    }

    async fn get(&self, path: &str) -> aster_drive_storage::Result<Vec<u8>> {
        tokio::fs::read(self.full_path(path)?)
            .await
            .map_err(map_local_object_io_error)
    }

    async fn get_stream(
        &self,
        path: &str,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        let file = tokio::fs::File::open(self.full_path(path)?)
            .await
            .map_err(map_local_object_io_error)?;
        Ok(Box::new(file))
    }

    async fn get_range(
        &self,
        path: &str,
        offset: u64,
        length: Option<u64>,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        use tokio::io::AsyncReadExt;
        let mut file = tokio::fs::File::open(self.full_path(path)?)
            .await
            .map_err(map_local_object_io_error)?;
        if offset > 0 {
            file.seek(std::io::SeekFrom::Start(offset))
                .await
                .map_storage_err_ctx(StorageErrorKind::Transient, "local seek")?;
        }
        Ok(match length {
            Some(len) => Box::new(file.take(len)),
            None => Box::new(file),
        })
    }

    fn supports_efficient_range(&self) -> bool {
        true
    }

    async fn delete(&self, path: &str) -> aster_drive_storage::Result<()> {
        tokio::fs::remove_file(self.full_path(path)?)
            .await
            .map_err(map_local_object_io_error)
    }

    async fn exists(&self, path: &str) -> aster_drive_storage::Result<bool> {
        Ok(self.full_path(path)?.exists())
    }

    async fn metadata(&self, path: &str) -> aster_drive_storage::Result<BlobMetadata> {
        let meta = tokio::fs::metadata(self.full_path(path)?)
            .await
            .map_err(map_local_object_io_error)?;
        Ok(BlobMetadata {
            size: meta.len(),
            content_type: None,
        })
    }

    async fn readiness_check(&self) -> aster_drive_storage::Result<()> {
        let metadata = tokio::fs::metadata(&self.base_path)
            .await
            .map_storage_err_ctx(StorageErrorKind::Transient, "local storage readiness")?;
        if metadata.is_dir() {
            Ok(())
        } else {
            Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                format!(
                    "local storage readiness: base path '{}' is not a directory",
                    self.base_path.display()
                ),
            ))
        }
    }

    async fn copy_object(
        &self,
        src_path: &str,
        dest_path: &str,
    ) -> aster_drive_storage::Result<String> {
        let src_full = self.full_path(src_path)?;
        let dest_full = self.full_path(dest_path)?;
        let parent = dest_full.parent().ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                "local copy destination has no parent",
            )
        })?;
        tokio::fs::create_dir_all(parent)
            .await
            .map_storage_err(StorageErrorKind::Transient)?;
        let temp = parent.join(format!(
            ".aster-copy-{}.tmp",
            aster_forge_utils::id::new_uuid()
        ));
        let result = async {
            tokio::fs::copy(&src_full, &temp)
                .await
                .map_storage_err_ctx(StorageErrorKind::Transient, "copy_object staging")?;
            tokio::fs::rename(&temp, &dest_full)
                .await
                .map_storage_err_ctx(StorageErrorKind::Transient, "copy_object publish")?;
            Ok::<(), aster_drive_storage::StorageError>(())
        }
        .await;
        if let Err(error) = result {
            if let Err(cleanup_error) = tokio::fs::remove_file(&temp).await
                && cleanup_error.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(path = %temp.display(), "failed to clean up local copy staging file: {cleanup_error}");
            }
            return Err(error);
        }
        Ok(dest_path.to_string())
    }

    fn extensions(&self) -> aster_drive_storage::traits::StorageDriverExtensions<'_> {
        aster_drive_storage::traits::StorageDriverExtensions {
            list: Some(self),
            stream_upload: Some(self),
            local_path: Some(self),
            native_publish: Some(self),
            ..Default::default()
        }
    }

    async fn capacity_info(&self) -> aster_drive_storage::Result<StorageCapacityInfo> {
        let base_path = self.base_path.clone();
        tokio::task::spawn_blocking(move || {
            let probe_path = nearest_existing_capacity_path(&base_path)?;
            let total = aster_fs::total_space(&probe_path)
                .map_storage_err_ctx(StorageErrorKind::Transient, "local capacity total_space")?;
            let available = aster_fs::available_space(&probe_path).map_storage_err_ctx(
                StorageErrorKind::Transient,
                "local capacity available_space",
            )?;
            let used = total.saturating_sub(available);
            Ok(StorageCapacityInfo {
                status: StorageCapacityStatus::Supported,
                total_bytes: Some(
                    u64_to_i64(total, "local capacity total_bytes")
                        .map_storage_err(StorageErrorKind::Misconfigured)?,
                ),
                available_bytes: Some(
                    u64_to_i64(available, "local capacity available_bytes")
                        .map_storage_err(StorageErrorKind::Misconfigured)?,
                ),
                used_bytes: Some(
                    u64_to_i64(used, "local capacity used_bytes")
                        .map_storage_err(StorageErrorKind::Misconfigured)?,
                ),
                source: "local_filesystem".to_string(),
                observed_at: chrono::Utc::now(),
            })
        })
        .await
        .map_storage_err_ctx(StorageErrorKind::Transient, "local capacity task")?
    }

    fn capacity_probe_policy(&self) -> aster_drive_storage::StorageCapacityProbePolicy {
        aster_drive_storage::StorageCapacityProbePolicy::local()
    }
}

#[async_trait]
impl NativePublishStorageDriver for LocalDriver {
    async fn publish_staged(
        &self,
        staging_path: &str,
        storage_path: &str,
    ) -> aster_drive_storage::Result<String> {
        let staging = self.full_path(staging_path)?;
        let destination = self.full_path(storage_path)?;
        tokio::fs::rename(staging, destination)
            .await
            .map_storage_err_ctx(StorageErrorKind::Transient, "publish local staged object")?;
        Ok(storage_path.to_string())
    }
}

fn nearest_existing_capacity_path(base_path: &Path) -> aster_drive_storage::Result<PathBuf> {
    let mut candidate = base_path;
    loop {
        match std::fs::metadata(candidate) {
            Ok(_) => return Ok(candidate.to_path_buf()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                candidate = candidate.parent().ok_or_else(|| {
                    storage_driver_error(
                        StorageErrorKind::Misconfigured,
                        format!(
                            "local capacity path has no existing ancestor: {}",
                            base_path.display()
                        ),
                    )
                })?;
            }
            Err(error) => {
                return Err(error).map_storage_err_ctx(
                    StorageErrorKind::Transient,
                    "inspect local capacity path",
                );
            }
        }
    }
}

/// Maps local object I/O without collapsing conclusive absence into a retryable failure.
fn map_local_object_io_error(error: std::io::Error) -> StorageError {
    let kind = match error.kind() {
        std::io::ErrorKind::NotFound => StorageErrorKind::NotFound,
        std::io::ErrorKind::PermissionDenied => StorageErrorKind::Permission,
        _ => StorageErrorKind::Transient,
    };
    StorageError::new(kind, error.to_string())
}

impl LocalPathStorageDriver for LocalDriver {
    fn resolve_local_path(&self, path: &str) -> aster_drive_storage::Result<std::path::PathBuf> {
        self.full_path(path)
    }
}

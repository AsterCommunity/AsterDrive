use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncSeekExt};

use aster_drive_storage::traits::driver::{BlobMetadata, StorageDriver};
use aster_drive_storage::traits::extensions::{
    LocalPathStorageDriver, StorageCapacityInfo, StorageCapacityStatus,
};
use aster_drive_storage::{MapStorageErr, StorageErrorKind, storage_driver_error};
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
            .map_storage_err(StorageErrorKind::Transient)
    }

    async fn get_stream(
        &self,
        path: &str,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        let file = tokio::fs::File::open(self.full_path(path)?)
            .await
            .map_storage_err(StorageErrorKind::Transient)?;
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
            .map_storage_err(StorageErrorKind::Transient)?;
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
            .map_storage_err(StorageErrorKind::Transient)
    }

    async fn exists(&self, path: &str) -> aster_drive_storage::Result<bool> {
        Ok(self.full_path(path)?.exists())
    }

    async fn metadata(&self, path: &str) -> aster_drive_storage::Result<BlobMetadata> {
        let meta = tokio::fs::metadata(self.full_path(path)?)
            .await
            .map_storage_err(StorageErrorKind::Transient)?;
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
        if let Some(parent) = dest_full.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_storage_err(StorageErrorKind::Transient)?;
        }
        tokio::fs::copy(&src_full, &dest_full)
            .await
            .map_storage_err_ctx(StorageErrorKind::Transient, "copy_object")?;
        Ok(dest_path.to_string())
    }

    fn extensions(&self) -> aster_drive_storage::traits::StorageDriverExtensions<'_> {
        aster_drive_storage::traits::StorageDriverExtensions {
            list: Some(self),
            stream_upload: Some(self),
            local_path: Some(self),
            ..Default::default()
        }
    }

    async fn capacity_info(&self) -> aster_drive_storage::Result<StorageCapacityInfo> {
        let base_path = self.base_path.clone();
        tokio::task::spawn_blocking(move || {
            let total = fs2::total_space(&base_path)
                .map_storage_err_ctx(StorageErrorKind::Transient, "local capacity total_space")?;
            let available = fs2::available_space(&base_path).map_storage_err_ctx(
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
}

impl LocalPathStorageDriver for LocalDriver {
    fn resolve_local_path(&self, path: &str) -> aster_drive_storage::Result<std::path::PathBuf> {
        self.full_path(path)
    }
}

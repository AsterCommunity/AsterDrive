use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncSeekExt};

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::storage::driver::{BlobMetadata, StorageDriver};
use crate::storage::extensions::{
    ListStorageDriver, LocalPathStorageDriver, StorageCapacityInfo, StorageCapacityStatus,
    StreamUploadDriver,
};
use crate::utils::numbers::u64_to_i64;

use super::LocalDriver;

#[async_trait]
impl StorageDriver for LocalDriver {
    async fn put(&self, path: &str, data: &[u8]) -> Result<String> {
        let full = self.full_path(path)?;
        if let Some(parent) = full.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_aster_err(AsterError::storage_driver_error)?;
        }
        tokio::fs::write(&full, data)
            .await
            .map_aster_err(AsterError::storage_driver_error)?;
        Ok(path.to_string())
    }

    async fn get(&self, path: &str) -> Result<Vec<u8>> {
        tokio::fs::read(self.full_path(path)?)
            .await
            .map_aster_err(AsterError::storage_driver_error)
    }

    async fn get_stream(&self, path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        let file = tokio::fs::File::open(self.full_path(path)?)
            .await
            .map_aster_err(AsterError::storage_driver_error)?;
        Ok(Box::new(file))
    }

    async fn get_range(
        &self,
        path: &str,
        offset: u64,
        length: Option<u64>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        use tokio::io::AsyncReadExt;
        let mut file = tokio::fs::File::open(self.full_path(path)?)
            .await
            .map_aster_err(AsterError::storage_driver_error)?;
        if offset > 0 {
            file.seek(std::io::SeekFrom::Start(offset))
                .await
                .map_aster_err_ctx("local seek", AsterError::storage_driver_error)?;
        }
        Ok(match length {
            Some(len) => Box::new(file.take(len)),
            None => Box::new(file),
        })
    }

    fn supports_efficient_range(&self) -> bool {
        true
    }

    async fn delete(&self, path: &str) -> Result<()> {
        tokio::fs::remove_file(self.full_path(path)?)
            .await
            .map_aster_err(AsterError::storage_driver_error)
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        Ok(self.full_path(path)?.exists())
    }

    async fn metadata(&self, path: &str) -> Result<BlobMetadata> {
        let meta = tokio::fs::metadata(self.full_path(path)?)
            .await
            .map_aster_err(AsterError::storage_driver_error)?;
        Ok(BlobMetadata {
            size: meta.len(),
            content_type: None,
        })
    }

    async fn readiness_check(&self) -> Result<()> {
        let metadata = tokio::fs::metadata(&self.base_path)
            .await
            .map_aster_err_ctx("local storage readiness", AsterError::storage_driver_error)?;
        if metadata.is_dir() {
            Ok(())
        } else {
            Err(AsterError::storage_driver_error(format!(
                "local storage readiness: base path '{}' is not a directory",
                self.base_path.display()
            )))
        }
    }

    async fn copy_object(&self, src_path: &str, dest_path: &str) -> Result<String> {
        let src_full = self.full_path(src_path)?;
        let dest_full = self.full_path(dest_path)?;
        if let Some(parent) = dest_full.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_aster_err(AsterError::storage_driver_error)?;
        }
        tokio::fs::copy(&src_full, &dest_full)
            .await
            .map_aster_err_ctx("copy_object", AsterError::storage_driver_error)?;
        Ok(dest_path.to_string())
    }

    fn as_list(&self) -> Option<&dyn ListStorageDriver> {
        Some(self)
    }

    fn as_stream_upload(&self) -> Option<&dyn StreamUploadDriver> {
        Some(self)
    }

    fn as_local_path(&self) -> Option<&dyn LocalPathStorageDriver> {
        Some(self)
    }

    async fn capacity_info(&self) -> Result<StorageCapacityInfo> {
        let base_path = self.base_path.clone();
        tokio::task::spawn_blocking(move || {
            let total = fs2::total_space(&base_path).map_aster_err_ctx(
                "local capacity total_space",
                AsterError::storage_driver_error,
            )?;
            let available = fs2::available_space(&base_path).map_aster_err_ctx(
                "local capacity available_space",
                AsterError::storage_driver_error,
            )?;
            let used = total.saturating_sub(available);
            Ok(StorageCapacityInfo {
                status: StorageCapacityStatus::Supported,
                total_bytes: Some(u64_to_i64(total, "local capacity total_bytes")?),
                available_bytes: Some(u64_to_i64(available, "local capacity available_bytes")?),
                used_bytes: Some(u64_to_i64(used, "local capacity used_bytes")?),
                source: "local_filesystem".to_string(),
                observed_at: chrono::Utc::now(),
            })
        })
        .await
        .map_aster_err_ctx("local capacity task", AsterError::storage_driver_error)?
    }
}

impl LocalPathStorageDriver for LocalDriver {
    fn resolve_local_path(&self, path: &str) -> Result<std::path::PathBuf> {
        self.full_path(path)
    }
}

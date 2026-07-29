use std::path::Path;

use async_trait::async_trait;
use tokio::io::AsyncRead;

use aster_drive_storage::error::{StorageErrorKind, storage_driver_error};
use aster_drive_storage::traits::extensions::StreamUploadDriver;

use super::RemoteDriver;

#[async_trait]
impl StreamUploadDriver for RemoteDriver {
    async fn put_reader(
        &self,
        storage_path: &str,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> aster_drive_storage::Result<String> {
        let size = u64::try_from(size).map_err(|_| {
            storage_driver_error(
                StorageErrorKind::Precondition,
                format!("remote stream upload size must be non-negative, got {size}"),
            )
        })?;
        self.client
            .put_reader(&self.object_key(storage_path), reader, size)
            .await?;
        Ok(storage_path.to_string())
    }

    async fn put_file(
        &self,
        storage_path: &str,
        local_path: &str,
    ) -> aster_drive_storage::Result<String> {
        let metadata = tokio::fs::metadata(local_path).await.map_err(|error| {
            storage_driver_error(
                StorageErrorKind::Transient,
                format!("remote put_file metadata: {error}"),
            )
        })?;
        let file = tokio::fs::File::open(Path::new(local_path))
            .await
            .map_err(|error| {
                storage_driver_error(
                    StorageErrorKind::Transient,
                    format!("remote put_file open: {error}"),
                )
            })?;
        self.put_reader(
            storage_path,
            Box::new(file),
            i64::try_from(metadata.len()).map_err(|_| {
                storage_driver_error(
                    StorageErrorKind::Precondition,
                    "remote put_file size exceeds i64 range",
                )
            })?,
        )
        .await
    }
}

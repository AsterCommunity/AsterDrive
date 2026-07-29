use async_trait::async_trait;
use tokio::io::AsyncRead;

use aster_drive_storage::error::{StorageErrorKind, storage_driver_error};
use aster_drive_storage::traits::driver::{BlobMetadata, StorageDriver};
use aster_drive_storage::traits::extensions::StorageCapacityInfo;

use super::RemoteDriver;

#[async_trait]
impl StorageDriver for RemoteDriver {
    async fn put(&self, path: &str, data: &[u8]) -> aster_drive_storage::Result<String> {
        self.client.put_bytes(&self.object_key(path), data).await?;
        Ok(path.to_string())
    }

    async fn get(&self, path: &str) -> aster_drive_storage::Result<Vec<u8>> {
        self.client
            .get_bytes(&self.object_key(path))
            .await
            .map_err(Into::into)
    }

    async fn get_stream(
        &self,
        path: &str,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.client
            .get_stream(&self.object_key(path), None, None)
            .await
            .map_err(Into::into)
    }

    async fn get_range(
        &self,
        path: &str,
        offset: u64,
        length: Option<u64>,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.client
            .get_stream(&self.object_key(path), Some(offset), length)
            .await
            .map_err(Into::into)
    }

    fn supports_efficient_range(&self) -> bool {
        true
    }

    async fn delete(&self, path: &str) -> aster_drive_storage::Result<()> {
        self.client
            .delete(&self.object_key(path))
            .await
            .map_err(Into::into)
    }

    async fn exists(&self, path: &str) -> aster_drive_storage::Result<bool> {
        self.client
            .exists(&self.object_key(path))
            .await
            .map_err(Into::into)
    }

    async fn metadata(&self, path: &str) -> aster_drive_storage::Result<BlobMetadata> {
        self.client
            .metadata(&self.object_key(path))
            .await
            .map_err(Into::into)
    }

    async fn capacity_info(&self) -> aster_drive_storage::Result<StorageCapacityInfo> {
        if !self.supports_capacity {
            return Err(storage_driver_error(
                StorageErrorKind::Unsupported,
                "remote storage node does not support capacity observability",
            ));
        }
        self.client.capacity_info().await.map_err(Into::into)
    }

    fn extensions(&self) -> aster_drive_storage::traits::StorageDriverExtensions<'_> {
        aster_drive_storage::traits::StorageDriverExtensions {
            list: Some(self),
            stream_upload: Some(self),
            presigned: Some(self),
            multipart: Some(self),
            ..Default::default()
        }
    }
}

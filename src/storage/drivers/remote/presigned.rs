use std::time::Duration;

use async_trait::async_trait;

use crate::errors::Result;
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::storage::traits::driver::PresignedDownloadOptions;
use crate::storage::traits::extensions::PresignedStorageDriver;

use super::RemoteDriver;

#[async_trait]
impl PresignedStorageDriver for RemoteDriver {
    async fn presigned_url(
        &self,
        path: &str,
        expires: Duration,
        options: PresignedDownloadOptions,
    ) -> Result<Option<String>> {
        if self.uses_reverse_tunnel {
            return Err(storage_driver_error(
                StorageErrorKind::Unsupported,
                "reverse tunnel remote nodes do not support presigned download URLs",
            ));
        }
        self.client
            .presigned_url(&self.object_key(path), expires, options)
            .map(Some)
    }

    async fn presigned_put_url(&self, path: &str, expires: Duration) -> Result<Option<String>> {
        if self.uses_reverse_tunnel {
            return Err(storage_driver_error(
                StorageErrorKind::Unsupported,
                "reverse tunnel remote nodes do not support presigned upload URLs",
            ));
        }
        self.client
            .presigned_put_url(&self.object_key(path), expires)
            .map(Some)
    }
}

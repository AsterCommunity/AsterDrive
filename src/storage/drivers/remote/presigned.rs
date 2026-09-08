use std::time::Duration;

use async_trait::async_trait;

use aster_drive_storage::error::{StorageErrorKind, storage_driver_error};
use aster_drive_storage::traits::driver::DirectDownloadOptions;
use aster_drive_storage::traits::extensions::{
    DirectDownloadStorageDriver, PresignedUploadStorageDriver,
};
use aster_drive_storage::{DirectDownloadRequest, PresignedUploadRequest};

use super::RemoteDriver;

#[async_trait]
impl DirectDownloadStorageDriver for RemoteDriver {
    async fn resolve_download_url(
        &self,
        path: &str,
        expires: Duration,
        options: DirectDownloadOptions,
    ) -> aster_drive_storage::Result<Option<DirectDownloadRequest>> {
        if self.uses_reverse_tunnel {
            return Err(storage_driver_error(
                StorageErrorKind::Unsupported,
                "reverse tunnel remote nodes do not support presigned download URLs",
            ));
        }
        self.client
            .presigned_url(&self.object_key(path), expires, options)
            .map(|url| Some(DirectDownloadRequest::temporary_url(url, expires)))
            .map_err(Into::into)
    }
}

#[async_trait]
impl PresignedUploadStorageDriver for RemoteDriver {
    async fn presigned_put_request(
        &self,
        path: &str,
        expires: Duration,
    ) -> aster_drive_storage::Result<Option<PresignedUploadRequest>> {
        if self.uses_reverse_tunnel {
            return Err(storage_driver_error(
                StorageErrorKind::Unsupported,
                "reverse tunnel remote nodes do not support presigned upload URLs",
            ));
        }
        self.client
            .presigned_put_url(&self.object_key(path), expires)
            .map(PresignedUploadRequest::without_headers)
            .map(Some)
            .map_err(Into::into)
    }
}

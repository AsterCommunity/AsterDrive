use std::collections::BTreeMap;
use std::time::Duration;

use async_trait::async_trait;
use azure_storage_blob::models::BlobContainerClientListBlobsOptions;
use futures::{StreamExt as _, TryStreamExt as _};

use aster_drive_storage::PresignedUploadRequest;
use aster_drive_storage::traits::driver::{PresignedDownloadOptions, StoragePathVisitor};
use aster_drive_storage::traits::extensions::{ListStorageDriver, PresignedStorageDriver};

use super::AzureBlobDriver;

#[async_trait]
impl PresignedStorageDriver for AzureBlobDriver {
    async fn presigned_url(
        &self,
        path: &str,
        expires: Duration,
        _options: PresignedDownloadOptions,
    ) -> aster_drive_storage::Result<Option<String>> {
        Ok(Some(self.blob_url(path, "r", expires)?.to_string()))
    }

    async fn presigned_put_request(
        &self,
        path: &str,
        expires: Duration,
    ) -> aster_drive_storage::Result<Option<PresignedUploadRequest>> {
        Ok(Some(PresignedUploadRequest::new(
            self.blob_url(path, "cw", expires)?.to_string(),
            BTreeMap::from([("x-ms-blob-type".to_string(), "BlockBlob".to_string())]),
        )))
    }

    fn presigned_single_put_requires_etag(&self) -> bool {
        false
    }
}

#[async_trait]
impl ListStorageDriver for AzureBlobDriver {
    async fn list_paths(&self, prefix: Option<&str>) -> aster_drive_storage::Result<Vec<String>> {
        let mut output = Vec::new();
        self.scan_paths(prefix, &mut VecVisitor(&mut output))
            .await?;
        Ok(output)
    }

    async fn scan_paths(
        &self,
        prefix: Option<&str>,
        visitor: &mut dyn StoragePathVisitor,
    ) -> aster_drive_storage::Result<()> {
        let container = self.container_client("rl")?;

        let full_prefix = prefix.map(|value| self.full_key(value));
        let pager = container
            .list_blobs(Some(BlobContainerClientListBlobsOptions {
                prefix: full_prefix,
                ..Default::default()
            }))
            .map_err(|error| Self::rewrap_azure_error("build Azure Blob list pager", error))?;
        let mut pages = pager.into_stream();
        while let Some(page) = pages.next().await {
            let item =
                page.map_err(|error| Self::map_azure_error("Azure Blob list failed", error))?;
            if let Some(name) = item.name
                && let Some(relative) = self.relative_key(&name)
            {
                visitor.visit_path(relative.to_string())?;
            }
        }
        Ok(())
    }
}

struct VecVisitor<'a>(&'a mut Vec<String>);

impl StoragePathVisitor for VecVisitor<'_> {
    fn visit_path(&mut self, path: String) -> aster_drive_storage::Result<()> {
        self.0.push(path);
        Ok(())
    }
}

use std::collections::BTreeMap;
use std::time::Duration;

use async_trait::async_trait;
use azure_storage_blob::models::BlobContainerClientListBlobsOptions;
use futures::{StreamExt as _, TryStreamExt as _};

use aster_drive_storage::traits::driver::{
    DirectDownloadOptions, StoragePathVisitControl, StoragePathVisitor,
};
use aster_drive_storage::traits::extensions::{
    DirectDownloadStorageDriver, ListStorageDriver, PresignedUploadStorageDriver,
};
use aster_drive_storage::{DirectDownloadRequest, PresignedUploadRequest};

use super::AzureBlobDriver;

#[async_trait]
impl DirectDownloadStorageDriver for AzureBlobDriver {
    async fn resolve_download_url(
        &self,
        path: &str,
        expires: Duration,
        options: DirectDownloadOptions,
    ) -> aster_drive_storage::Result<Option<DirectDownloadRequest>> {
        let url = self.download_url(path, expires, options)?;
        let expires_at = url
            .query_pairs()
            .find(|(name, _)| name == "se")
            .and_then(|(_, value)| chrono::DateTime::parse_from_rfc3339(&value).ok())
            .map(|value| value.with_timezone(&chrono::Utc));
        Ok(Some(DirectDownloadRequest::preauthenticated_url(
            url.to_string(),
            expires_at,
        )))
    }
}

#[async_trait]
impl PresignedUploadStorageDriver for AzureBlobDriver {
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
                && matches!(
                    visitor.visit_path(relative.to_string()).await?,
                    StoragePathVisitControl::Stop
                )
            {
                break;
            }
        }
        Ok(())
    }
}

struct VecVisitor<'a>(&'a mut Vec<String>);

#[async_trait]
impl StoragePathVisitor for VecVisitor<'_> {
    async fn visit_path(
        &mut self,
        path: String,
    ) -> aster_drive_storage::Result<StoragePathVisitControl> {
        self.0.push(path);
        Ok(StoragePathVisitControl::Continue)
    }
}

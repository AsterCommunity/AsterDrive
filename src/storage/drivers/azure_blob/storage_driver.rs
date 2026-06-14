use async_trait::async_trait;
use azure_core::http::RequestContent;
use azure_storage_blob::models::{
    BlobClientDownloadOptions, BlobClientGetPropertiesResultHeaders, HttpRange,
};
use futures::TryStreamExt as _;
use tokio::io::AsyncRead;

use crate::errors::{MapAsterErr, Result};
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::storage::traits::driver::{BlobMetadata, StorageDriver};
use crate::storage::traits::extensions::{
    ListStorageDriver, PresignedStorageDriver, StorageCapacityInfo, StreamUploadDriver,
};
use crate::storage::traits::multipart::MultipartStorageDriver;

use super::{AzureBlobDriver, DEFAULT_OPERATION_SAS_TTL};

#[async_trait]
impl StorageDriver for AzureBlobDriver {
    async fn put(&self, path: &str, data: &[u8]) -> Result<String> {
        let client = self.blob_client(path, "cw")?;
        client
            .upload(RequestContent::from(data.to_vec()), None)
            .await
            .map_err(|error| Self::map_azure_error("Azure Blob put failed", error))?;
        Ok(path.to_string())
    }

    async fn get(&self, path: &str) -> Result<Vec<u8>> {
        let client = self.blob_client(path, "r")?;
        let resp = client
            .download(None)
            .await
            .map_err(|error| Self::map_azure_error("Azure Blob get failed", error))?;
        let bytes = resp
            .body
            .collect()
            .await
            .map_aster_err_ctx("Azure Blob read body failed", |message| {
                storage_driver_error(StorageErrorKind::Transient, message)
            })?;
        Ok(bytes.to_vec())
    }

    async fn get_stream(&self, path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        let client = self.blob_client(path, "r")?;
        let resp = client
            .download(None)
            .await
            .map_err(|error| Self::map_azure_error("Azure Blob get_stream failed", error))?;
        Ok(Box::new(tokio_util::io::StreamReader::new(
            resp.body.map_err(std::io::Error::other),
        )))
    }

    async fn get_range(
        &self,
        path: &str,
        offset: u64,
        length: Option<u64>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        if length == Some(0) {
            return Ok(Box::new(tokio::io::empty()));
        }
        let client = self.blob_client(path, "r")?;
        let range = match length {
            Some(length) => HttpRange::new(offset, length),
            None => HttpRange::from_offset(offset),
        };
        let options = BlobClientDownloadOptions {
            range: Some(range),
            ..Default::default()
        };
        let resp = client
            .download(Some(options))
            .await
            .map_err(|error| Self::map_azure_error("Azure Blob get_range failed", error))?;
        Ok(Box::new(tokio_util::io::StreamReader::new(
            resp.body.map_err(std::io::Error::other),
        )))
    }

    fn supports_efficient_range(&self) -> bool {
        true
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let client = self.blob_client(path, "d")?;
        match client.delete(None).await {
            Ok(_) => {}
            Err(error) if Self::classify_azure_error(&error) == StorageErrorKind::NotFound => {}
            Err(error) => return Err(Self::map_azure_error("Azure Blob delete failed", error)),
        }
        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let client = self.blob_client(path, "r")?;
        client
            .exists()
            .await
            .map_err(|error| Self::map_azure_error("Azure Blob exists check failed", error))
    }

    async fn metadata(&self, path: &str) -> Result<BlobMetadata> {
        let client = self.blob_client(path, "r")?;
        let resp = client
            .get_properties(None)
            .await
            .map_err(|error| Self::map_azure_error("Azure Blob head failed", error))?;
        let size = resp
            .content_length()
            .map_err(|error| {
                Self::map_azure_error("Azure Blob parse content_length failed", error)
            })?
            .unwrap_or(0);
        let content_type = resp.content_type().map_err(|error| {
            Self::map_azure_error("Azure Blob parse content_type failed", error)
        })?;
        Ok(BlobMetadata { size, content_type })
    }

    async fn copy_object(&self, src_path: &str, dest_path: &str) -> Result<String> {
        let source_url = self.blob_url(src_path, "r", DEFAULT_OPERATION_SAS_TTL)?;
        let dest_client = self.block_blob_client(dest_path, "cw")?;

        if let Err(error) = dest_client
            .upload_blob_from_url(source_url.to_string(), None)
            .await
        {
            if !self.endpoint_uses_loopback_host() {
                return Err(Self::map_azure_error(
                    "Azure Blob copy_object failed",
                    error,
                ));
            }

            tracing::debug!(
                error = %Self::format_azure_error(error),
                "Azure Blob server-side copy failed for loopback endpoint; falling back to local copy"
            );
            self.copy_object_via_temp_file(src_path, dest_path).await?;
        }
        Ok(dest_path.to_string())
    }

    async fn capacity_info(&self) -> Result<StorageCapacityInfo> {
        Err(storage_driver_error(
            StorageErrorKind::Unsupported,
            "Azure Blob storage does not expose storage account capacity through the blob data API",
        ))
    }

    fn as_presigned(&self) -> Option<&dyn PresignedStorageDriver> {
        Some(self)
    }

    fn as_list(&self) -> Option<&dyn ListStorageDriver> {
        Some(self)
    }

    fn as_stream_upload(&self) -> Option<&dyn StreamUploadDriver> {
        Some(self)
    }

    fn as_multipart(&self) -> Option<&dyn MultipartStorageDriver> {
        Some(self)
    }
}

impl AzureBlobDriver {
    async fn copy_object_via_temp_file(&self, src_path: &str, dest_path: &str) -> Result<String> {
        use tokio::io::AsyncWriteExt as _;

        let temp_path = crate::utils::raii::TempFileGuard::new(
            std::env::temp_dir().join(format!(
                "aster_azure_copy_{}_{}",
                std::process::id(),
                uuid::Uuid::new_v4()
            )),
            "Azure Blob copy fallback temp file",
        );

        let mut reader = self.get_stream(src_path).await?;
        let mut file = tokio::fs::File::create(temp_path.path())
            .await
            .map_aster_err_ctx("create Azure Blob copy temp file", |message| {
                storage_driver_error(StorageErrorKind::Transient, message)
            })?;
        tokio::io::copy(&mut reader, &mut file)
            .await
            .map_aster_err_ctx("write Azure Blob copy temp file", |message| {
                storage_driver_error(StorageErrorKind::Transient, message)
            })?;
        file.flush()
            .await
            .map_aster_err_ctx("flush Azure Blob copy temp file", |message| {
                storage_driver_error(StorageErrorKind::Transient, message)
            })?;
        drop(file);

        let temp_path_str = temp_path
            .path()
            .to_str()
            .ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "Azure Blob copy temp path is not valid UTF-8",
                )
            })?
            .to_string();
        self.put_file(dest_path, &temp_path_str).await
    }
}

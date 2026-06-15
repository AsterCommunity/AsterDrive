//! Microsoft Graph OneDrive / SharePoint storage driver building blocks.

mod client;
mod error;
mod paths;

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::errors::Result;
use crate::errors::{AsterError, MapAsterErr};
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::storage::traits::driver::{BlobMetadata, StorageDriver};
use crate::storage::traits::extensions::{StorageCapacityInfo, StreamUploadDriver};
use crate::utils::numbers;

pub use client::{
    MicrosoftGraphClient, MicrosoftGraphClientConfig, MicrosoftGraphDriveItem,
    MicrosoftGraphDriveItemParentReference,
};
pub use paths::{
    graph_drive_item_content_path, graph_drive_item_path, normalize_graph_relative_path,
};

#[derive(Clone)]
pub struct OneDriveDriver {
    client: MicrosoftGraphClient,
    drive_id: String,
    root_item_id: String,
    base_path: String,
}

const GRAPH_SIMPLE_UPLOAD_MAX_BYTES: usize = 250 * 1024 * 1024;
const GRAPH_UPLOAD_FRAGMENT_ALIGNMENT: usize = 320 * 1024;
const GRAPH_UPLOAD_FRAGMENT_SIZE: usize = 10 * 1024 * 1024;

impl OneDriveDriver {
    pub fn new(
        client: MicrosoftGraphClient,
        drive_id: impl Into<String>,
        root_item_id: impl Into<String>,
        base_path: impl Into<String>,
    ) -> Self {
        Self {
            client,
            drive_id: drive_id.into(),
            root_item_id: root_item_id.into(),
            base_path: base_path.into(),
        }
    }

    fn graph_path(&self, path: &str) -> crate::errors::Result<String> {
        let relative = paths::join_base_path(&self.base_path, path)?;
        paths::graph_drive_item_path(&self.drive_id, &self.root_item_id, &relative)
    }

    fn graph_content_path(&self, path: &str) -> crate::errors::Result<String> {
        let relative = paths::join_base_path(&self.base_path, path)?;
        paths::graph_drive_item_content_path(&self.drive_id, &self.root_item_id, &relative)
    }

    fn graph_upload_session_path(&self, path: &str) -> crate::errors::Result<String> {
        let relative = paths::join_base_path(&self.base_path, path)?;
        let item_path =
            paths::graph_drive_item_path(&self.drive_id, &self.root_item_id, &relative)?;
        if relative.is_empty() {
            Ok(format!("{item_path}/createUploadSession"))
        } else {
            Ok(format!("{item_path}:/createUploadSession"))
        }
    }

    pub async fn validate_root(&self) -> crate::errors::Result<MicrosoftGraphDriveItem> {
        self.client
            .get_drive_item_by_id(&self.drive_id, &self.root_item_id)
            .await
    }

    async fn put_reader_via_upload_session(
        &self,
        path: &str,
        mut reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> Result<String> {
        let total_size = numbers::i64_to_u64(size, "OneDrive put_reader declared size")?;
        if total_size == 0 {
            self.put(path, &[]).await?;
            return Ok(path.to_string());
        }
        if total_size
            <= numbers::usize_to_u64(
                GRAPH_SIMPLE_UPLOAD_MAX_BYTES,
                "OneDrive simple upload limit",
            )?
        {
            let capacity = numbers::u64_to_usize(total_size, "OneDrive simple upload size")?;
            let mut data = vec![0_u8; capacity];
            reader
                .read_exact(&mut data)
                .await
                .map_aster_err_ctx("read OneDrive simple upload stream", |message| {
                    storage_driver_error(StorageErrorKind::Precondition, message)
                })?;
            reject_extra_upload_bytes(reader).await?;
            self.put(path, &data).await?;
            return Ok(path.to_string());
        }

        let upload_session_path = self.graph_upload_session_path(path)?;
        let upload_url = self
            .client
            .create_upload_session(&upload_session_path)
            .await?;
        let mut uploaded = 0_u64;
        while uploaded < total_size {
            let remaining = total_size - uploaded;
            let read_len = numbers::u64_to_usize(
                remaining.min(numbers::usize_to_u64(
                    GRAPH_UPLOAD_FRAGMENT_SIZE,
                    "OneDrive upload fragment size",
                )?),
                "OneDrive upload next fragment size",
            )?;
            let mut chunk = vec![0_u8; read_len];
            reader
                .read_exact(&mut chunk)
                .await
                .map_aster_err_ctx("read OneDrive upload session fragment", |message| {
                    storage_driver_error(StorageErrorKind::Precondition, message)
                })?;
            if remaining > numbers::usize_to_u64(read_len, "OneDrive upload fragment length")?
                && read_len % GRAPH_UPLOAD_FRAGMENT_ALIGNMENT != 0
            {
                return Err(storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "OneDrive upload session fragment size must be a multiple of 320 KiB",
                ));
            }
            self.client
                .upload_session_fragment(&upload_url, uploaded, total_size, chunk)
                .await?;
            uploaded += numbers::usize_to_u64(read_len, "OneDrive uploaded fragment size")?;
        }
        reject_extra_upload_bytes(reader).await?;
        Ok(path.to_string())
    }
}

#[async_trait]
impl StorageDriver for OneDriveDriver {
    async fn put(&self, path: &str, data: &[u8]) -> Result<String> {
        if data.len() > GRAPH_SIMPLE_UPLOAD_MAX_BYTES {
            return Err(storage_driver_error(
                StorageErrorKind::Unsupported,
                "OneDrive simple upload is limited to 250 MiB; use upload session support for larger objects",
            ));
        }
        self.client
            .put_small_content(&self.graph_content_path(path)?, data)
            .await?;
        Ok(path.to_string())
    }

    async fn get(&self, path: &str) -> Result<Vec<u8>> {
        self.client.get_bytes(&self.graph_content_path(path)?).await
    }

    async fn get_stream(&self, path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.client
            .get_stream(&self.graph_content_path(path)?, None, None)
            .await
    }

    async fn get_range(
        &self,
        path: &str,
        offset: u64,
        length: Option<u64>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.client
            .get_stream(&self.graph_content_path(path)?, Some(offset), length)
            .await
    }

    fn supports_efficient_range(&self) -> bool {
        true
    }

    fn as_stream_upload(&self) -> Option<&dyn StreamUploadDriver> {
        Some(self)
    }

    async fn delete(&self, path: &str) -> Result<()> {
        self.client.delete(&self.graph_path(path)?).await
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        self.client.exists(&self.graph_path(path)?).await
    }

    async fn metadata(&self, path: &str) -> Result<BlobMetadata> {
        self.client.metadata(&self.graph_path(path)?).await
    }

    async fn capacity_info(&self) -> Result<StorageCapacityInfo> {
        self.client.capacity_info(&self.drive_id).await
    }
}

#[async_trait]
impl StreamUploadDriver for OneDriveDriver {
    async fn put_reader(
        &self,
        storage_path: &str,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> Result<String> {
        self.put_reader_via_upload_session(storage_path, reader, size)
            .await
    }

    async fn put_file(&self, storage_path: &str, local_path: &str) -> Result<String> {
        let file = tokio::fs::File::open(local_path).await.map_aster_err_ctx(
            "open OneDrive upload file",
            AsterError::storage_driver_error,
        )?;
        let metadata = file.metadata().await.map_aster_err_ctx(
            "stat OneDrive upload file",
            AsterError::storage_driver_error,
        )?;
        let size = numbers::u64_to_i64(metadata.len(), "OneDrive upload file size")?;
        self.put_reader(storage_path, Box::new(file), size).await
    }
}

async fn reject_extra_upload_bytes(
    mut reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
) -> Result<()> {
    let mut extra = [0_u8; 1];
    let read = reader
        .read(&mut extra)
        .await
        .map_aster_err_ctx("check OneDrive upload stream length", |message| {
            storage_driver_error(StorageErrorKind::Precondition, message)
        })?;
    if read != 0 {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "OneDrive upload stream exceeded declared size",
        ));
    }
    Ok(())
}

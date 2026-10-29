//! Administrator-managed Google Drive blob storage.
//!
//! Drive owns logical blob identities, publication and garbage collection. Google
//! owns object bytes and resumable sessions; aborting a session never authorizes
//! deleting the logical destination. This driver has no product database access.
//!
//! Wire contracts: <https://developers.google.com/workspace/drive/api/guides/manage-uploads>
//! and <https://developers.google.com/workspace/drive/api/reference/rest/v3/files/list>.
use aster_drive_storage::{
    BlobMetadata, ExactSizeReader, ProviderResumableUploadCapabilities,
    ProviderResumableUploadDriver, ProviderResumableUploadFragmentOutcome,
    ProviderResumableUploadSession, ProviderResumableUploadStatus, Result, StorageDriver,
    StorageDriverExtensions, StorageError, StorageErrorKind, StreamUploadAttempt,
    StreamUploadCleanup, StreamUploadDriver, exact_size_error_kind, storage_driver_error,
};
use async_trait::async_trait;
use futures::TryStreamExt;
use reqwest::header::{CONTENT_LENGTH, CONTENT_RANGE, RANGE};
use serde::Deserialize;
use std::{sync::Arc, time::Duration};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio_util::io::StreamReader;

const API: &str = "https://www.googleapis.com/drive/v3";
const UPLOAD: &str = "https://www.googleapis.com/upload/drive/v3/files";
pub(crate) const CHUNK: usize = 8 * 1024 * 1024;
pub(crate) const ALIGNMENT: usize = 256 * 1024;
pub(crate) const MAX_SIMPLE: u64 = 5 * 1024 * 1024;
const APP_KEY: &str = "asterdrive_storage_path";
const MAX_LOOKUP_PAGES: usize = 16;

#[async_trait]
/// Supplies server-side credentials without coupling object I/O to Drive's DB.
/// Implementations must redact Debug and never return tokens to client DTOs.
pub trait GoogleDriveTokenProvider: Send + Sync + std::fmt::Debug {
    async fn access_token(&self) -> Result<String>;
}

#[derive(Clone)]
/// An app-owned blob pool scoped to a folder, a shared drive or appDataFolder.
pub struct GoogleDriveDriver {
    http: reqwest::Client,
    token_provider: Arc<dyn GoogleDriveTokenProvider>,
    root_folder_id: Option<String>,
    shared_drive_id: Option<String>,
    app_data_folder: bool,
    capacity_probe_timeout: Duration,
    api: String,
    upload: String,
}
impl std::fmt::Debug for GoogleDriveDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GoogleDriveDriver")
            .field("root_folder_id", &self.root_folder_id)
            .field("shared_drive_id", &self.shared_drive_id)
            .field("app_data_folder", &self.app_data_folder)
            .finish_non_exhaustive()
    }
}
impl GoogleDriveDriver {
    /// Construct a redirect-free, streaming driver. Client construction and
    /// incompatible pool options return typed errors rather than panicking.
    pub fn new(
        token_provider: Arc<dyn GoogleDriveTokenProvider>,
        root_folder_id: Option<String>,
        shared_drive_id: Option<String>,
        app_data_folder: bool,
        capacity_probe_timeout: Duration,
    ) -> Result<Self> {
        if app_data_folder && (root_folder_id.is_some() || shared_drive_id.is_some()) {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "appDataFolder cannot be combined with a folder or shared drive",
            ));
        }
        Ok(Self {
            http: http_client()?,
            token_provider,
            root_folder_id,
            shared_drive_id,
            app_data_folder,
            capacity_probe_timeout,
            api: API.into(),
            upload: UPLOAD.into(),
        })
    }
    fn root(&self) -> &str {
        if self.app_data_folder {
            "appDataFolder"
        } else {
            self.root_folder_id
                .as_deref()
                .or(self.shared_drive_id.as_deref())
                .unwrap_or("root")
        }
    }
}
#[async_trait]
impl StorageDriver for GoogleDriveDriver {
    async fn capacity_info(&self) -> Result<aster_drive_storage::StorageCapacityInfo> {
        self.observe_capacity().await
    }

    fn capacity_probe_policy(&self) -> aster_drive_storage::StorageCapacityProbePolicy {
        aster_drive_storage::StorageCapacityProbePolicy::network(self.capacity_probe_timeout)
    }
    async fn put(&self, path: &str, data: &[u8]) -> Result<String> {
        self.put_reader(
            path,
            Box::new(std::io::Cursor::new(data.to_vec())),
            i64::try_from(data.len()).map_err(|_| invalid("Google Drive object too large"))?,
        )
        .await
    }
    async fn get(&self, path: &str) -> Result<Vec<u8>> {
        self.download(path, None)
            .await?
            .bytes()
            .await
            .map(|v| v.to_vec())
            .map_err(|e| map_transport("read Google Drive object", e))
    }
    async fn get_stream(&self, path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        Ok(Box::new(StreamReader::new(
            self.download(path, None)
                .await?
                .bytes_stream()
                .map_err(std::io::Error::other),
        )))
    }
    async fn get_range(
        &self,
        path: &str,
        offset: u64,
        length: Option<u64>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        if length == Some(0) {
            return Ok(Box::new(std::io::Cursor::new(Vec::<u8>::new())));
        }
        let range = match length {
            Some(n) => format!(
                "bytes={offset}-{}",
                offset
                    .checked_add(n - 1)
                    .ok_or_else(|| invalid("Google Drive byte range overflow"))?
            ),
            None => format!("bytes={offset}-"),
        };
        Ok(Box::new(StreamReader::new(
            self.download(path, Some(range))
                .await?
                .bytes_stream()
                .map_err(std::io::Error::other),
        )))
    }
    fn supports_efficient_range(&self) -> bool {
        true
    }
    fn max_single_put_size(&self) -> Option<u64> {
        Some(MAX_SIMPLE)
    }
    async fn delete(&self, path: &str) -> Result<()> {
        let Some(info) = self.find(path).await? else {
            return Ok(());
        };
        let response = self
            .auth(
                self.http
                    .delete(self.file_url(&info.id))
                    .query(&[("supportsAllDrives", "true")]),
            )
            .await?;
        if response.status().is_success() || response.status() == reqwest::StatusCode::NOT_FOUND {
            Ok(())
        } else {
            Err(map_http(response).await)
        }
    }
    async fn exists(&self, path: &str) -> Result<bool> {
        Ok(self.find(path).await?.is_some())
    }
    async fn metadata(&self, path: &str) -> Result<BlobMetadata> {
        let info = self.find(path).await?.ok_or_else(|| {
            storage_driver_error(StorageErrorKind::NotFound, "Google Drive object not found")
        })?;
        Ok(BlobMetadata {
            size: info
                .size
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| transient("Google Drive object size is missing or invalid"))?,
            content_type: info.mime_type,
        })
    }
    async fn copy_object(&self, src_path: &str, dest_path: &str) -> Result<String> {
        let size = self.metadata(src_path).await?.size;
        let reader = self.get_stream(src_path).await?;
        self.upload_reader(
            dest_path,
            reader,
            i64::try_from(size).map_err(|_| invalid("Google Drive copy is too large"))?,
            None,
        )
        .await
    }
    fn extensions(&self) -> StorageDriverExtensions<'_> {
        StorageDriverExtensions {
            stream_upload: Some(self),
            provider_resumable: Some(self),
            ..Default::default()
        }
    }
}

mod capacity;
pub(crate) mod client;
mod upload;
use client::{http_client, invalid, map_http, map_transport, transient};

#[cfg(test)]
mod tests;

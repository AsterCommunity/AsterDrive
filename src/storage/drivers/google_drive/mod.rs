//! Google Drive storage driver.
//!
//! The driver treats Google Drive as an object store. AsterDrive owns the
//! logical object path and marks every remote file with private `appProperties`;
//! user-facing Drive files are never discovered or deleted by accident.

mod client;

use async_trait::async_trait;
use bytes::Bytes;
use tokio::io::{AsyncRead, AsyncReadExt};

use aster_drive_storage::traits::driver::{BlobMetadata, StorageDriver};
use aster_drive_storage::traits::extensions::{
    ExactSizeReader, ProviderResumableUploadCapabilities, ProviderResumableUploadDriver,
    ProviderResumableUploadFragmentOutcome, ProviderResumableUploadSession,
    ProviderResumableUploadStatus, StreamUploadDriver, checked_upload_size, exact_size_error_kind,
};
use aster_drive_storage::{MapStorageErr, Result, StorageErrorKind, storage_driver_error};

pub use client::{GoogleDriveAccessTokenProvider, GoogleDriveClient, GoogleDriveClientConfig};

const DEFAULT_FRAGMENT_SIZE: usize = 8 * 1024 * 1024;
const FRAGMENT_ALIGNMENT: usize = 256 * 1024;
const MAX_SIMPLE_UPLOAD_SIZE: u64 = 5 * 1024 * 1024;

#[derive(Clone)]
pub struct GoogleDriveDriver {
    client: GoogleDriveClient,
    base_path: String,
    root_folder_id: String,
    shared_drive_id: Option<String>,
    app_data_folder: bool,
    policy_id: i64,
    chunk_size: i64,
}

impl std::fmt::Debug for GoogleDriveDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GoogleDriveDriver")
            .field("root_folder_id", &self.root_folder_id)
            .field("shared_drive_id", &self.shared_drive_id)
            .field("app_data_folder", &self.app_data_folder)
            .field("policy_id", &self.policy_id)
            .finish()
    }
}

impl GoogleDriveDriver {
    pub fn new(
        client: GoogleDriveClient,
        base_path: impl Into<String>,
        root_folder_id: impl Into<String>,
        shared_drive_id: Option<String>,
        app_data_folder: bool,
        policy_id: i64,
        chunk_size: i64,
    ) -> Result<Self> {
        let root_folder_id = root_folder_id.into();
        if !app_data_folder && root_folder_id.trim().is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Google Drive root folder id cannot be empty",
            ));
        }
        Ok(Self {
            client,
            base_path: base_path.into().trim_matches('/').to_string(),
            root_folder_id,
            shared_drive_id,
            app_data_folder,
            policy_id,
            chunk_size,
        })
    }

    fn object_key(&self, path: &str) -> Result<String> {
        let path = path.trim().trim_start_matches('/');
        if path.is_empty() || path.contains("..") || path.contains('\\') || path.contains('\0') {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Google Drive storage path is invalid",
            ));
        }
        Ok(aster_drive_storage::object_key::join_key_prefix(
            &self.base_path,
            path,
        ))
    }

    #[cfg(test)]
    fn test_fragment_size(&self) -> usize {
        self.fragment_size()
    }

    fn parent_id(&self) -> Option<&str> {
        if self.app_data_folder {
            Some("appDataFolder")
        } else {
            Some(self.root_folder_id.as_str())
        }
    }

    fn fragment_size(&self) -> usize {
        let requested = usize::try_from(self.chunk_size).unwrap_or(DEFAULT_FRAGMENT_SIZE);
        let requested = if requested == 0 {
            DEFAULT_FRAGMENT_SIZE
        } else {
            requested
        };
        let capped = requested.clamp(FRAGMENT_ALIGNMENT, 64 * 1024 * 1024);
        capped - capped % FRAGMENT_ALIGNMENT
    }

    fn app_properties(&self, key: &str) -> std::collections::BTreeMap<String, String> {
        std::collections::BTreeMap::from([
            ("asterdrive_managed".to_string(), "true".to_string()),
            (
                "asterdrive_policy_id".to_string(),
                self.policy_id.to_string(),
            ),
            ("asterdrive_object_key".to_string(), key.to_string()),
        ])
    }

    async fn put_reader_inner(
        &self,
        path: &str,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> Result<String> {
        let size = checked_upload_size(size, "Google Drive upload size")?;
        let key = self.object_key(path)?;
        let mut reader = ExactSizeReader::new(reader, size);
        if size == 0 {
            let mut probe = [0_u8; 1];
            let read = reader.read(&mut probe).await.map_err(|error| {
                storage_driver_error(
                    exact_size_error_kind(&error).unwrap_or(StorageErrorKind::Transient),
                    format!("read Google Drive empty upload: {error}"),
                )
            })?;
            if read != 0 {
                return Err(storage_driver_error(
                    StorageErrorKind::Precondition,
                    "Google Drive upload reader exceeded declared size",
                ));
            }
        }
        let session = self
            .client
            .create_resumable_upload(
                self.parent_id(),
                self.shared_drive_id.as_deref(),
                &key,
                self.app_properties(&key),
                size,
            )
            .await?;
        let fragment_size = self.fragment_size();
        let mut offset = 0_u64;
        while offset < size {
            let remaining = size - offset;
            let read_len = usize::try_from(remaining.min(fragment_size as u64))
                .map_storage_err(StorageErrorKind::Misconfigured)?;
            let mut chunk = vec![0_u8; read_len];
            reader.read_exact(&mut chunk).await.map_err(|error| {
                storage_driver_error(
                    exact_size_error_kind(&error).unwrap_or(StorageErrorKind::Transient),
                    format!("read Google Drive upload fragment: {error}"),
                )
            })?;
            let outcome = self
                .client
                .upload_resumable_fragment(&session, offset, size, Bytes::from(chunk))
                .await?;
            offset = outcome.next_offset.unwrap_or(offset + read_len as u64);
            if outcome.completed {
                return Ok(path.to_string());
            }
        }
        if size == 0 {
            self.client
                .upload_resumable_fragment(&session, 0, 0, Bytes::new())
                .await?;
        }
        Ok(path.to_string())
    }
}

pub fn google_drive_upload_capabilities() -> ProviderResumableUploadCapabilities {
    ProviderResumableUploadCapabilities {
        provider: "google_drive",
        session_label: "Google Drive resumable upload",
        min_fragment_size: FRAGMENT_ALIGNMENT,
        default_fragment_size: DEFAULT_FRAGMENT_SIZE,
        max_fragment_size: 64 * 1024 * 1024,
        fragment_alignment: FRAGMENT_ALIGNMENT,
        max_simple_upload_size: Some(MAX_SIMPLE_UPLOAD_SIZE),
        frontend_direct_upload: false,
        implicit_completion: false,
        abort_supported: true,
        status_query_supported: true,
    }
}

#[async_trait]
impl StorageDriver for GoogleDriveDriver {
    async fn put(&self, path: &str, data: &[u8]) -> Result<String> {
        self.put_reader_inner(
            path,
            Box::new(std::io::Cursor::new(data.to_vec())),
            data.len() as i64,
        )
        .await
    }

    async fn get(&self, path: &str) -> Result<Vec<u8>> {
        let key = self.object_key(path)?;
        let file = self
            .client
            .find_managed_file(
                self.parent_id(),
                self.shared_drive_id.as_deref(),
                &key,
                self.policy_id,
            )
            .await?
            .ok_or_else(|| {
                storage_driver_error(StorageErrorKind::NotFound, "Google Drive object not found")
            })?;
        self.client.download_bytes(&file.id).await
    }

    async fn get_stream(&self, path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        let key = self.object_key(path)?;
        let file = self
            .client
            .find_managed_file(
                self.parent_id(),
                self.shared_drive_id.as_deref(),
                &key,
                self.policy_id,
            )
            .await?
            .ok_or_else(|| {
                storage_driver_error(StorageErrorKind::NotFound, "Google Drive object not found")
            })?;
        self.client.download_stream(&file.id, None, None).await
    }

    async fn get_range(
        &self,
        path: &str,
        offset: u64,
        length: Option<u64>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        let key = self.object_key(path)?;
        let file = self
            .client
            .find_managed_file(
                self.parent_id(),
                self.shared_drive_id.as_deref(),
                &key,
                self.policy_id,
            )
            .await?
            .ok_or_else(|| {
                storage_driver_error(StorageErrorKind::NotFound, "Google Drive object not found")
            })?;
        self.client
            .download_stream(&file.id, Some(offset), length)
            .await
    }

    fn supports_efficient_range(&self) -> bool {
        true
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let key = self.object_key(path)?;
        let Some(file) = self
            .client
            .find_managed_file(
                self.parent_id(),
                self.shared_drive_id.as_deref(),
                &key,
                self.policy_id,
            )
            .await?
        else {
            return Ok(());
        };
        self.client
            .delete_file(&file.id, self.shared_drive_id.as_deref())
            .await
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let key = self.object_key(path)?;
        Ok(self
            .client
            .find_managed_file(
                self.parent_id(),
                self.shared_drive_id.as_deref(),
                &key,
                self.policy_id,
            )
            .await?
            .is_some())
    }

    async fn metadata(&self, path: &str) -> Result<BlobMetadata> {
        let key = self.object_key(path)?;
        let file = self
            .client
            .find_managed_file(
                self.parent_id(),
                self.shared_drive_id.as_deref(),
                &key,
                self.policy_id,
            )
            .await?
            .ok_or_else(|| {
                storage_driver_error(StorageErrorKind::NotFound, "Google Drive object not found")
            })?;
        Ok(BlobMetadata {
            size: u64::try_from(file.size.unwrap_or(0))
                .map_storage_err(StorageErrorKind::Transient)?,
            content_type: file.mime_type,
        })
    }

    async fn readiness_check(&self) -> Result<()> {
        self.client.about_storage().await.map(|_| ())
    }

    fn extensions(&self) -> aster_drive_storage::traits::StorageDriverExtensions<'_> {
        aster_drive_storage::traits::StorageDriverExtensions {
            direct_download: None,
            stream_upload: Some(self),
            provider_resumable: Some(self),
            ..Default::default()
        }
    }
}

#[async_trait]
impl StreamUploadDriver for GoogleDriveDriver {
    async fn put_reader(
        &self,
        storage_path: &str,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> Result<String> {
        self.put_reader_inner(storage_path, reader, size).await
    }

    async fn put_file(&self, storage_path: &str, local_path: &str) -> Result<String> {
        let file = tokio::fs::File::open(local_path).await.map_err(|error| {
            storage_driver_error(
                StorageErrorKind::Transient,
                format!("open Google Drive upload source: {error}"),
            )
        })?;
        let size = file
            .metadata()
            .await
            .map_err(|error| {
                storage_driver_error(
                    StorageErrorKind::Transient,
                    format!("stat Google Drive upload source: {error}"),
                )
            })?
            .len();
        self.put_reader_inner(
            storage_path,
            Box::new(file),
            i64::try_from(size).map_storage_err(StorageErrorKind::Misconfigured)?,
        )
        .await
    }
}

#[async_trait]
impl ProviderResumableUploadDriver for GoogleDriveDriver {
    fn provider_resumable_upload_capabilities(&self) -> ProviderResumableUploadCapabilities {
        google_drive_upload_capabilities()
    }

    async fn create_upload_session(&self, path: &str) -> Result<ProviderResumableUploadSession> {
        let key = self.object_key(path)?;
        let session = self
            .client
            .create_resumable_upload(
                self.parent_id(),
                self.shared_drive_id.as_deref(),
                &key,
                self.app_properties(&key),
                0,
            )
            .await?;
        Ok(ProviderResumableUploadSession {
            upload_url: session.url,
            expires_at: session.expires_at,
            next_expected_ranges: Vec::new(),
        })
    }

    async fn query_upload_session(
        &self,
        upload_url: &str,
    ) -> Result<ProviderResumableUploadStatus> {
        let status = self.client.query_resumable_upload(upload_url).await?;
        Ok(ProviderResumableUploadStatus {
            expires_at: None,
            next_expected_ranges: status.next_expected_ranges,
        })
    }

    async fn abort_upload_session(&self, upload_url: &str) -> Result<()> {
        self.client.abort_resumable_upload(upload_url).await
    }

    async fn upload_session_fragment_reader(
        &self,
        upload_url: &str,
        start: u64,
        total_size: u64,
        mut reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        fragment_size: i64,
    ) -> Result<ProviderResumableUploadFragmentOutcome> {
        let fragment_size =
            usize::try_from(fragment_size).map_storage_err(StorageErrorKind::Misconfigured)?;
        let mut data = vec![0_u8; fragment_size];
        let read = reader.read(&mut data).await.map_err(|error| {
            storage_driver_error(
                StorageErrorKind::Transient,
                format!("read Google Drive upload fragment: {error}"),
            )
        })?;
        data.truncate(read);
        let outcome = self
            .client
            .upload_resumable_fragment_url(upload_url, start, total_size, Bytes::from(data))
            .await?;
        Ok(ProviderResumableUploadFragmentOutcome {
            completed: outcome.completed,
            next_expected_ranges: outcome.next_expected_ranges,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn driver(chunk_size: i64) -> GoogleDriveDriver {
        let client = GoogleDriveClient::new(GoogleDriveClientConfig::new("token")).unwrap();
        GoogleDriveDriver::new(client, "base", "root", None, false, 7, chunk_size).unwrap()
    }

    #[test]
    fn object_keys_include_base_path_and_reject_traversal() {
        let driver = driver(0);
        assert_eq!(driver.object_key("/blob-1").unwrap(), "base/blob-1");
        assert_eq!(
            driver.object_key("nested/blob").unwrap(),
            "base/nested/blob"
        );
        assert!(driver.object_key("../blob").is_err());
    }

    #[test]
    fn fragment_size_is_aligned_and_capped() {
        assert_eq!(driver(1).test_fragment_size(), FRAGMENT_ALIGNMENT);
        assert_eq!(driver(300 * 1024).test_fragment_size(), FRAGMENT_ALIGNMENT);
        assert_eq!(
            driver(64 * 1024 * 1024 + 1).test_fragment_size(),
            64 * 1024 * 1024
        );
    }

    #[test]
    fn app_data_folder_does_not_require_root_id() {
        let client = GoogleDriveClient::new(GoogleDriveClientConfig::new("token")).unwrap();
        assert!(GoogleDriveDriver::new(client, "", "", None, true, 7, 0).is_ok());
    }
}

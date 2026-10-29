//! Administrator-managed Google Drive blob storage. Provider secrets stay on the server.
use aster_drive_storage::{
    BlobMetadata, ExactSizeReader, ProviderResumableUploadCapabilities,
    ProviderResumableUploadDriver, ProviderResumableUploadFragmentOutcome,
    ProviderResumableUploadSession, ProviderResumableUploadStatus, Result, StorageDriver,
    StorageDriverExtensions, StorageError, StorageErrorKind, StreamUploadAttempt,
    StreamUploadCleanup, StreamUploadDriver, storage_driver_error,
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

#[async_trait]
pub trait GoogleDriveTokenProvider: Send + Sync + std::fmt::Debug {
    async fn access_token(&self) -> Result<String>;
}

pub(crate) fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        // Drive uses 308 as a resumable acknowledgement, not a redirect.
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(120))
        .build()
        .expect("Google Drive HTTP client configuration")
}

#[derive(Clone)]
pub struct GoogleDriveDriver {
    http: reqwest::Client,
    token_provider: Arc<dyn GoogleDriveTokenProvider>,
    root_folder_id: Option<String>,
    shared_drive_id: Option<String>,
    app_data_folder: bool,
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
#[derive(Deserialize)]
struct FileInfo {
    id: String,
    // Drive encodes int64 sizes as JSON strings.
    size: Option<String>,
    #[serde(rename = "mimeType")]
    mime_type: Option<String>,
    name: Option<String>,
    #[serde(rename = "driveId")]
    drive_id: Option<String>,
    capabilities: Option<FolderCapabilities>,
}
#[derive(Deserialize)]
struct FolderCapabilities {
    #[serde(rename = "canAddChildren", default)]
    can_add_children: bool,
}
#[derive(Deserialize)]
struct FileList {
    files: Vec<FileInfo>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}
fn invalid(message: &str) -> StorageError {
    storage_driver_error(StorageErrorKind::Precondition, message)
}
fn transient(message: &str) -> StorageError {
    storage_driver_error(StorageErrorKind::Transient, message)
}
fn escape_query(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}
fn validate_path(path: &str) -> Result<()> {
    // Drive limits each property key + value to 124 UTF-8 bytes.
    if path.is_empty() || APP_KEY.len() + path.len() > 124 || path.chars().any(char::is_control) {
        return Err(invalid("invalid Google Drive logical object key"));
    }
    Ok(())
}
impl GoogleDriveDriver {
    pub fn new(
        token_provider: Arc<dyn GoogleDriveTokenProvider>,
        root_folder_id: Option<String>,
        shared_drive_id: Option<String>,
        app_data_folder: bool,
    ) -> Self {
        Self {
            http: http_client(),
            token_provider,
            root_folder_id,
            shared_drive_id,
            app_data_folder,
            api: API.into(),
            upload: UPLOAD.into(),
        }
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
    async fn auth(&self, request: reqwest::RequestBuilder) -> Result<reqwest::Response> {
        request
            .bearer_auth(self.token_provider.access_token().await?)
            .send()
            .await
            .map_err(|e| map_transport("Google Drive request", e))
    }
    async fn response<T: for<'de> Deserialize<'de>>(
        &self,
        response: reqwest::Response,
    ) -> Result<T> {
        if !response.status().is_success() {
            return Err(map_http(response).await);
        }
        response
            .json()
            .await
            .map_err(|_| transient("invalid Google Drive JSON response"))
    }
    fn object_query(&self, path: &str) -> Result<String> {
        validate_path(path)?;
        Ok(format!(
            "appProperties has {{ key='{APP_KEY}' and value='{}' }} and trashed=false and '{}' in parents",
            escape_query(path),
            escape_query(self.root())
        ))
    }
    async fn find(&self, path: &str) -> Result<Option<FileInfo>> {
        let mut request = self.http.get(format!("{}/files", self.api)).query(&[
            ("q", self.object_query(path)?),
            ("fields", "nextPageToken,files(id,size,mimeType)".into()),
            ("pageSize", "2".into()),
            (
                "spaces",
                if self.app_data_folder {
                    "appDataFolder"
                } else {
                    "drive"
                }
                .into(),
            ),
            ("supportsAllDrives", "true".into()),
            ("includeItemsFromAllDrives", "true".into()),
        ]);
        if let Some(drive) = &self.shared_drive_id {
            request = request.query(&[("driveId", drive.as_str()), ("corpora", "drive")]);
        }
        let mut list: FileList = self.response(self.auth(request).await?).await?;
        if list.files.len() > 1 || list.next_page_token.is_some() {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "multiple Google Drive objects have the same AsterDrive key",
            ));
        }
        Ok(list.files.pop())
    }
    fn file_url(&self, id: &str) -> String {
        format!("{}/files/{}", self.api, urlencoding::encode(id))
    }
    pub(crate) async fn validate_root(&self) -> Result<(String, Option<String>)> {
        if self.app_data_folder {
            let _: FileList = self
                .response(
                    self.auth(self.http.get(format!("{}/files", self.api)).query(&[
                        ("spaces", "appDataFolder"),
                        ("pageSize", "1"),
                        ("fields", "files(id)"),
                    ]))
                    .await?,
                )
                .await?;
            return Ok(("appDataFolder".into(), Some("Application data".into())));
        }
        let info: FileInfo = self
            .response(
                self.auth(self.http.get(self.file_url(self.root())).query(&[
                    ("supportsAllDrives", "true"),
                    (
                        "fields",
                        "id,name,mimeType,driveId,capabilities(canAddChildren)",
                    ),
                ]))
                .await?,
            )
            .await?;
        if info.mime_type.as_deref() != Some("application/vnd.google-apps.folder") {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Google Drive root must be a folder",
            ));
        }
        if self
            .shared_drive_id
            .as_ref()
            .is_some_and(|id| info.drive_id.as_ref() != Some(id))
        {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Google Drive folder does not belong to the configured shared drive",
            ));
        }
        if !info.capabilities.is_some_and(|c| c.can_add_children) {
            return Err(storage_driver_error(
                StorageErrorKind::Permission,
                "Google Drive folder is not writable",
            ));
        }
        Ok((info.id, info.name))
    }
    async fn create_session(&self, path: &str, size: Option<u64>) -> Result<String> {
        validate_path(path)?;
        // Reuse the app-owned file on retry; never replace an unmarked file.
        let existing = self.find(path).await?;
        let mut metadata = serde_json::json!({"name": path.rsplit('/').next().unwrap_or("blob"),
            "mimeType": "application/octet-stream", "appProperties": { (APP_KEY): path }});
        let mut request = if let Some(info) = existing {
            self.http
                .patch(format!("{}/{}", self.upload, urlencoding::encode(&info.id)))
        } else {
            metadata["parents"] = serde_json::json!([self.root()]);
            self.http.post(&self.upload)
        };
        request = request
            .query(&[("uploadType", "resumable"), ("supportsAllDrives", "true")])
            .header("X-Upload-Content-Type", "application/octet-stream")
            .json(&metadata);
        if let Some(size) = size {
            request = request.header("X-Upload-Content-Length", size);
        }
        let response = self.auth(request).await?;
        if !response.status().is_success() {
            return Err(map_http(response).await);
        }
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| transient("Google Drive omitted upload session URL"))?;
        self.validate_session_url(location)?;
        Ok(location.into())
    }
    fn validate_session_url(&self, value: &str) -> Result<()> {
        let url = reqwest::Url::parse(value)
            .map_err(|_| invalid("invalid Google Drive upload session URL"))?;
        let upload = reqwest::Url::parse(&self.upload).expect("fixed upload endpoint");
        let valid_path = url.path() == upload.path()
            || url
                .path()
                .strip_prefix(&format!("{}/", upload.path()))
                .is_some_and(|id| {
                    !id.is_empty()
                        && id
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                });
        if url.origin() != upload.origin()
            || !valid_path
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || !url
                .query_pairs()
                .any(|(k, v)| k == "upload_id" && !v.is_empty())
        {
            return Err(invalid("untrusted Google Drive upload session URL"));
        }
        Ok(())
    }
    async fn download(&self, path: &str, range: Option<String>) -> Result<reqwest::Response> {
        let info = self.find(path).await?.ok_or_else(|| {
            storage_driver_error(StorageErrorKind::NotFound, "Google Drive object not found")
        })?;
        let mut request = self
            .http
            .get(self.file_url(&info.id))
            .query(&[("alt", "media"), ("supportsAllDrives", "true")]);
        if let Some(range) = &range {
            request = request.header(RANGE, range);
        }
        let response = self.auth(request).await?;
        if !response.status().is_success() {
            return Err(map_http(response).await);
        }
        if range.is_some() && response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            return Err(transient("Google Drive ignored the requested byte range"));
        }
        Ok(response)
    }
    async fn upload_bytes(
        &self,
        url: &str,
        start: u64,
        total: u64,
        data: Vec<u8>,
    ) -> Result<ProviderResumableUploadFragmentOutcome> {
        self.validate_session_url(url)?;
        let end = start
            .checked_add(data.len() as u64)
            .ok_or_else(|| invalid("upload range overflow"))?;
        if end > total
            || (data.is_empty() && total != 0)
            || (end < total && !data.len().is_multiple_of(ALIGNMENT))
        {
            return Err(invalid(
                "invalid Google Drive upload fragment range or alignment",
            ));
        }
        let range = if total == 0 {
            "bytes */0".into()
        } else {
            format!("bytes {start}-{}/{total}", end - 1)
        };
        let response = self
            .auth(
                self.http
                    .put(url)
                    .header(CONTENT_LENGTH, data.len())
                    .header(CONTENT_RANGE, range)
                    .body(data),
            )
            .await?;
        if response.status().is_success() {
            if end != total {
                return Err(transient(
                    "Google Drive completed before the final fragment",
                ));
            }
            return Ok(ProviderResumableUploadFragmentOutcome {
                completed: true,
                next_expected_ranges: Vec::new(),
            });
        }
        if response.status() == reqwest::StatusCode::PERMANENT_REDIRECT {
            return Ok(ProviderResumableUploadFragmentOutcome {
                completed: false,
                next_expected_ranges: next_ranges(response.headers())?,
            });
        }
        Err(map_http(response).await)
    }
    async fn upload_reader(
        &self,
        path: &str,
        reader: Box<dyn AsyncRead + Unpin + Send>,
        size: i64,
        attempt: Option<&StreamUploadAttempt>,
    ) -> Result<String> {
        let size = u64::try_from(size).map_err(|_| invalid("negative Google Drive upload size"))?;
        let mut reader = ExactSizeReader::new(reader, size);
        let url = self.create_session(path, Some(size)).await?;
        if let Some(attempt) = attempt {
            attempt.set_provider_session(&url);
        }
        let result = async {
            let mut offset = 0;
            loop {
                let count =
                    usize::try_from((size - offset).min(CHUNK as u64)).expect("bounded chunk size");
                let mut data = vec![0; count];
                reader.read_exact(&mut data).await.map_err(|_| {
                    invalid("Google Drive upload reader does not match declared size")
                })?;
                if size == 0 {
                    let mut extra = [0];
                    reader
                        .read(&mut extra)
                        .await
                        .map_err(|_| invalid("Google Drive upload reader exceeds declared size"))?;
                }
                let outcome = self.upload_bytes(&url, offset, size, data).await?;
                offset += count as u64;
                if outcome.completed {
                    return Ok(path.to_owned());
                }
                if offset == size || outcome.next_expected_ranges != [format!("{offset}-")] {
                    return Err(transient(
                        "Google Drive did not acknowledge the complete upload fragment",
                    ));
                }
            }
        }
        .await;
        if result.is_err() {
            if let Err(error) = self.abort_upload_session(&url).await {
                tracing::warn!(kind = ?error.kind(), "Google Drive upload session cleanup failed");
            }
        }
        result
    }
}
#[async_trait]
impl StorageDriver for GoogleDriveDriver {
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
#[async_trait]
impl StreamUploadDriver for GoogleDriveDriver {
    async fn put_reader(
        &self,
        path: &str,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> Result<String> {
        self.upload_reader(path, reader, size, None).await
    }
    async fn stage_attempt(
        &self,
        attempt: &StreamUploadAttempt,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
    ) -> Result<()> {
        self.upload_reader(
            &attempt.storage_path,
            reader,
            attempt.expected_size,
            Some(attempt),
        )
        .await
        .map(|_| ())
    }
    async fn abort_attempt(&self, attempt: &StreamUploadAttempt) -> Result<StreamUploadCleanup> {
        if let Some(url) = attempt.take_provider_session() {
            if let Err(error) = self.abort_upload_session(&url).await {
                attempt.set_provider_session(url);
                return Err(error);
            }
        }
        self.delete(&attempt.storage_path).await?;
        Ok(StreamUploadCleanup::Cleaned)
    }
    async fn put_file(&self, path: &str, local_path: &str) -> Result<String> {
        let file = tokio::fs::File::open(local_path)
            .await
            .map_err(|_| transient("open Google Drive upload file"))?;
        let size = file
            .metadata()
            .await
            .map_err(|_| transient("stat Google Drive upload file"))?
            .len();
        self.put_reader(
            path,
            Box::new(file),
            i64::try_from(size).map_err(|_| invalid("Google Drive upload too large"))?,
        )
        .await
    }
}
#[async_trait]
impl ProviderResumableUploadDriver for GoogleDriveDriver {
    fn provider_resumable_upload_capabilities(&self) -> ProviderResumableUploadCapabilities {
        ProviderResumableUploadCapabilities {
            provider: "google_drive",
            session_label: "Google Drive resumable upload",
            min_fragment_size: ALIGNMENT,
            default_fragment_size: CHUNK,
            max_fragment_size: CHUNK,
            fragment_alignment: ALIGNMENT,
            max_simple_upload_size: Some(MAX_SIMPLE),
            frontend_direct_upload: false,
            implicit_completion: true,
            abort_supported: true,
            status_query_supported: true,
        }
    }
    async fn create_upload_session(&self, path: &str) -> Result<ProviderResumableUploadSession> {
        Ok(ProviderResumableUploadSession {
            upload_url: self.create_session(path, None).await?,
            expires_at: None,
            next_expected_ranges: vec!["0-".into()],
        })
    }
    async fn query_upload_session(&self, url: &str) -> Result<ProviderResumableUploadStatus> {
        self.validate_session_url(url)?;
        let response = self
            .auth(
                self.http
                    .put(url)
                    .header(CONTENT_LENGTH, 0)
                    .header(CONTENT_RANGE, "bytes */*"),
            )
            .await?;
        let next_expected_ranges = if response.status().is_success() {
            Vec::new()
        } else if response.status() == reqwest::StatusCode::PERMANENT_REDIRECT {
            next_ranges(response.headers())?
        } else {
            return Err(map_http(response).await);
        };
        Ok(ProviderResumableUploadStatus {
            expires_at: None,
            next_expected_ranges,
        })
    }
    async fn abort_upload_session(&self, url: &str) -> Result<()> {
        self.validate_session_url(url)?;
        let response = self.auth(self.http.delete(url)).await?;
        // Drive acknowledges resumable cancellation with 499.
        if response.status().is_success() || matches!(response.status().as_u16(), 404 | 410 | 499) {
            Ok(())
        } else {
            Err(map_http(response).await)
        }
    }
    async fn upload_session_fragment_reader(
        &self,
        url: &str,
        start: u64,
        total_size: u64,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        fragment_size: i64,
    ) -> Result<ProviderResumableUploadFragmentOutcome> {
        let size = u64::try_from(fragment_size)
            .map_err(|_| invalid("negative Google Drive fragment size"))?;
        if size > CHUNK as u64 {
            return Err(invalid("Google Drive fragment exceeds configured limit"));
        }
        let mut reader = ExactSizeReader::new(reader, size);
        let mut data = Vec::with_capacity(usize::try_from(size).expect("bounded fragment size"));
        reader
            .read_to_end(&mut data)
            .await
            .map_err(|_| invalid("Google Drive fragment reader does not match declared size"))?;
        self.upload_bytes(url, start, total_size, data).await
    }
}
fn next_ranges(headers: &reqwest::header::HeaderMap) -> Result<Vec<String>> {
    let next = match headers.get(RANGE) {
        None => 0,
        Some(value) => value
            .to_str()
            .ok()
            .and_then(|s| s.strip_prefix("bytes=0-"))
            .and_then(|s| s.parse::<u64>().ok())
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| transient("invalid Google Drive upload acknowledgement range"))?,
    };
    Ok(vec![format!("{next}-")])
}
pub(crate) async fn map_http(response: reqwest::Response) -> StorageError {
    let status = response.status();
    // Provider messages may contain private file names or upload URLs. Only inspect the reason.
    let reason = response
        .json::<serde_json::Value>()
        .await
        .ok()
        .and_then(|v| {
            v["error"]["errors"][0]["reason"]
                .as_str()
                .map(str::to_owned)
        });
    let kind = match status.as_u16() {
        401 => StorageErrorKind::Auth,
        403 if matches!(
            reason.as_deref(),
            Some("rateLimitExceeded" | "userRateLimitExceeded" | "dailyLimitExceeded")
        ) =>
        {
            StorageErrorKind::RateLimited
        }
        403 => StorageErrorKind::Permission,
        404 | 410 => StorageErrorKind::NotFound,
        408 => StorageErrorKind::Transient,
        412 | 416 => StorageErrorKind::Precondition,
        429 => StorageErrorKind::RateLimited,
        500..=599 => StorageErrorKind::Transient,
        _ => StorageErrorKind::Misconfigured,
    };
    storage_driver_error(
        kind,
        format!("Google Drive request failed (HTTP {})", status.as_u16()),
    )
}
fn map_transport(context: &str, error: reqwest::Error) -> StorageError {
    storage_driver_error(
        StorageErrorKind::Transient,
        format!("{context}: {}", error.without_url()),
    )
}
#[cfg(test)]
mod tests;

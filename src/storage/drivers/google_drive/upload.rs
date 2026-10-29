//! Bounded resumable uploads and attempt-owned session cleanup.
use super::client::validate_path;
use super::*;

impl GoogleDriveDriver {
    pub(super) async fn create_session(&self, path: &str, size: Option<u64>) -> Result<String> {
        validate_path(path)?;
        // Reuse the app-owned file on retry; never replace an unmarked file.
        let parent = self
            .resolve_base_folder(true)
            .await?
            .ok_or_else(|| transient("Google Drive base folder resolution failed"))?;
        let existing = self
            .find_unique(
                &self.object_query(path, &parent)?,
                "multiple Google Drive objects have the same AsterDrive key",
            )
            .await?;
        let mut metadata = serde_json::json!({"name": path.rsplit('/').next().unwrap_or("blob"),
            "mimeType": "application/octet-stream", "appProperties": { (APP_KEY): path }});
        let mut request = if let Some(info) = existing {
            self.http
                .patch(format!("{}/{}", self.upload, urlencoding::encode(&info.id)))
        } else {
            metadata["parents"] = serde_json::json!([parent]);
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
    pub(super) fn validate_session_url(&self, value: &str) -> Result<()> {
        let url = reqwest::Url::parse(value)
            .map_err(|_| invalid("invalid Google Drive upload session URL"))?;
        let upload = reqwest::Url::parse(&self.upload).map_err(|_| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                "invalid Google Drive upload endpoint",
            )
        })?;
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
            || url.query_pairs().filter(|(k, _)| k == "upload_id").count() != 1
            || !url
                .query_pairs()
                .any(|(k, v)| k == "upload_id" && !v.trim().is_empty())
            || url
                .query_pairs()
                .any(|(k, v)| k == "uploadType" && v != "resumable")
        {
            return Err(invalid("untrusted Google Drive upload session URL"));
        }
        Ok(())
    }
    pub(super) async fn upload_bytes(
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
        // Non-final fragment sizes must be multiples of 256 KiB according to
        // Google's upload guide. The 8 MiB maximum is our own memory budget.
        // https://developers.google.com/workspace/drive/api/guides/manage-uploads#http---multiple-requests
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
        // Drive's 308 is Resume Incomplete. Never follow its Location or infer
        // acknowledgement from bytes sent; use Range (absent means offset zero).
        if response.status() == reqwest::StatusCode::PERMANENT_REDIRECT {
            let next = next_offset(response.headers())?;
            if next > end || next >= total {
                return Err(transient(
                    "Google Drive acknowledged bytes outside the sent fragment",
                ));
            }
            return Ok(ProviderResumableUploadFragmentOutcome {
                completed: false,
                next_expected_ranges: vec![format!("{next}-")],
            });
        }
        Err(map_http(response).await)
    }
    pub(super) async fn upload_reader(
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
                let count = usize::try_from((size - offset).min(CHUNK as u64))
                    .map_err(|_| invalid("Google Drive chunk size is not representable"))?;
                let mut data = vec![0; count];
                reader
                    .read_exact(&mut data)
                    .await
                    .map_err(map_reader_error)?;
                if size == 0 {
                    let mut extra = [0];
                    reader.read(&mut extra).await.map_err(map_reader_error)?;
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
        let session_closed = if result.is_ok() {
            true
        } else {
            match self.abort_upload_session(&url).await {
                Ok(()) => true,
                Err(error) => {
                    tracing::warn!(kind = ?error.kind(), "Google Drive upload session cleanup failed");
                    false
                }
            }
        };
        if session_closed && let Some(attempt) = attempt {
            // Retain only live/uncertain sessions so outer cancellation cleanup
            // can retry a failed abort, without cancelling a completed upload.
            drop(attempt.take_provider_session());
        }
        result
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
        // storage_path is the eventual blob identity, not attempt-owned staging.
        // The caller owns cleanup of a uniquely allocated unpublished blob. A
        // failed PATCH must leave the pre-existing destination untouched.
        let Some(url) = attempt.take_provider_session() else {
            return Ok(StreamUploadCleanup::NotRequired);
        };
        // Keep the capability in shared attempt state across await, including
        // when this abort future itself is dropped by a timeout or cancellation.
        attempt.set_provider_session(&url);
        self.abort_upload_session(&url).await?;
        drop(attempt.take_provider_session());
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
        google_drive_upload_capabilities()
    }
    fn browser_upload(&self) -> Option<&dyn aster_drive_storage::BrowserUploadDriver> {
        Some(self)
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
        let next_expected_ranges = if matches!(response.status().as_u16(), 200 | 201) {
            Vec::new()
        } else if response.status() == reqwest::StatusCode::PERMANENT_REDIRECT {
            vec![format!("{}-", next_offset(response.headers())?)]
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
        // Accept the Google upload cancellation code only on a session DELETE;
        // normal object requests still classify 499 as an error. Real Drive
        // cancellation is verified separately from the local protocol mock.
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
        // Reject an untrusted session before consuming a potentially unbounded
        // caller reader. These URLs carry provider-side upload capabilities.
        self.validate_session_url(url)?;
        let size = u64::try_from(fragment_size)
            .map_err(|_| invalid("negative Google Drive fragment size"))?;
        if size > CHUNK as u64 {
            return Err(invalid("Google Drive fragment exceeds configured limit"));
        }
        let mut reader = ExactSizeReader::new(reader, size);
        let mut data = Vec::with_capacity(
            usize::try_from(size)
                .map_err(|_| invalid("Google Drive fragment size is not representable"))?,
        );
        reader
            .read_to_end(&mut data)
            .await
            .map_err(map_reader_error)?;
        self.upload_bytes(url, start, total_size, data).await
    }
}
fn next_offset(headers: &reqwest::header::HeaderMap) -> Result<u64> {
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
    Ok(next)
}
fn map_reader_error(error: std::io::Error) -> StorageError {
    storage_driver_error(
        exact_size_error_kind(&error).unwrap_or(StorageErrorKind::Transient),
        "Google Drive upload reader failed or does not match the declared size",
    )
}

pub fn google_drive_upload_capabilities() -> ProviderResumableUploadCapabilities {
    ProviderResumableUploadCapabilities {
        browser_upload_protocol: Some(aster_drive_storage::BrowserUploadProtocol {
            id: "asterdrive.google_drive.resumable".into(),
            version: 1,
        }),
        provider: "google_drive",
        session_label: "Google Drive resumable upload",
        min_fragment_size: ALIGNMENT,
        default_fragment_size: CHUNK,
        max_fragment_size: CHUNK,
        fragment_alignment: ALIGNMENT,
        max_simple_upload_size: Some(MAX_SIMPLE),
        frontend_direct_upload: true,
        implicit_completion: true,
        abort_supported: true,
        status_query_supported: true,
    }
}

//! Native Kodo resumable v2: numbered concurrent parts and explicit completion.
use super::{MultipartSession, QiniuDriver, invalid};
use aster_drive_storage::{
    MultipartStorageDriver, ProviderResumableUploadCapabilities, ProviderResumableUploadDriver,
    ProviderResumableUploadFragmentOutcome, ProviderResumableUploadSession,
    ProviderResumableUploadStatus, Result, StorageDriver, StorageErrorKind, storage_driver_error,
};

pub(crate) fn upload_capabilities() -> ProviderResumableUploadCapabilities {
    ProviderResumableUploadCapabilities {
        browser_upload_protocol: None,
        provider: "qiniu_kodo",
        session_label: "multipart_v2",
        min_fragment_size: 1024 * 1024,
        default_fragment_size: 8 * 1024 * 1024,
        max_fragment_size: 1024 * 1024 * 1024,
        fragment_alignment: 1,
        max_simple_upload_size: Some(1024 * 1024 * 1024),
        frontend_direct_upload: true,
        implicit_completion: false,
        abort_supported: true,
        status_query_supported: true,
    }
}

impl QiniuDriver {
    fn resumable_path(&self, id: &str) -> Result<String> {
        let session: MultipartSession =
            serde_json::from_str(id).map_err(|_| invalid("invalid Kodo resumable session"))?;
        aster_drive_storage::object_key::strip_key_prefix(&self.config.base_path, &session.key)
            .map(str::to_string)
            .ok_or_else(|| invalid("Kodo resumable session prefix mismatch"))
    }
}

#[async_trait::async_trait]
impl ProviderResumableUploadDriver for QiniuDriver {
    fn provider_resumable_upload_capabilities(&self) -> ProviderResumableUploadCapabilities {
        upload_capabilities()
    }
    fn numbered_parts(&self) -> Option<&dyn MultipartStorageDriver> {
        Some(self)
    }

    async fn create_upload_session(&self, path: &str) -> Result<ProviderResumableUploadSession> {
        let id = self.create_multipart_upload(path).await?;
        let session: MultipartSession =
            serde_json::from_str(&id).map_err(|_| invalid("invalid native multipart session"))?;
        Ok(ProviderResumableUploadSession {
            upload_url: id,
            expires_at: chrono::DateTime::from_timestamp(session.expires_at, 0),
            next_expected_ranges: vec!["0-".into()],
        })
    }
    async fn query_upload_session(&self, id: &str) -> Result<ProviderResumableUploadStatus> {
        let path = self.resumable_path(id)?;
        let parts = self.list_uploaded_part_details(&path, id).await?;
        let mut offset = 0u64;
        for (index, part) in parts.iter().enumerate() {
            if part.part_number
                != i32::try_from(index + 1).map_err(|_| invalid("Kodo part number overflow"))?
            {
                break;
            }
            offset = offset
                .checked_add(
                    u64::try_from(part.size).map_err(|_| invalid("Kodo part size overflow"))?,
                )
                .ok_or_else(|| invalid("Kodo offset overflow"))?;
        }
        let session: MultipartSession =
            serde_json::from_str(id).map_err(|_| invalid("invalid native multipart session"))?;
        Ok(ProviderResumableUploadStatus {
            expires_at: chrono::DateTime::from_timestamp(session.expires_at, 0),
            next_expected_ranges: vec![format!("{offset}-")],
        })
    }
    async fn abort_upload_session(&self, id: &str) -> Result<()> {
        self.abort_multipart_upload(&self.resumable_path(id)?, id)
            .await
    }
    async fn complete_upload_session(&self, id: &str, expected_size: u64) -> Result<()> {
        let path = self.resumable_path(id)?;
        let parts = match self.uploaded_parts(&path, id).await {
            Ok(parts) => parts,
            Err(error) if error.kind() == StorageErrorKind::NotFound => {
                if self.metadata(&path).await?.size == expected_size {
                    return Ok(());
                }
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        let size = parts
            .iter()
            .try_fold(0u64, |sum, part| {
                sum.checked_add(u64::try_from(part.size).ok()?)
                    .filter(|_| part.size >= 0)
            })
            .ok_or_else(|| invalid("invalid native part sizes"))?;
        if size != expected_size {
            return Err(storage_driver_error(
                StorageErrorKind::Precondition,
                "Kodo uploaded size does not match declared size",
            ));
        }
        self.complete_multipart_upload(
            &path,
            id,
            parts.into_iter().map(|p| (p.number, p.etag)).collect(),
        )
        .await
    }
    async fn upload_session_fragment_reader(
        &self,
        id: &str,
        start: u64,
        total_size: u64,
        reader: Box<dyn tokio::io::AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> Result<ProviderResumableUploadFragmentOutcome> {
        let path = self.resumable_path(id)?;
        let block = u64::try_from(upload_capabilities().default_fragment_size)
            .map_err(|_| invalid("invalid fragment size"))?;
        let size_u64 = aster_drive_storage::checked_upload_size(size, "Kodo fragment size")?;
        if !start.is_multiple_of(block)
            || size_u64 > block
            || start
                .checked_add(size_u64)
                .is_none_or(|end| end > total_size)
            || (start + size_u64 < total_size && size_u64 != block)
        {
            return Err(invalid(
                "Kodo sequential fragment does not match the negotiated part boundaries",
            ));
        }
        let number =
            i32::try_from(start / block + 1).map_err(|_| invalid("Kodo part number overflow"))?;
        self.upload_multipart_part_reader(&path, id, number, reader, size)
            .await?;
        let status = self.query_upload_session(id).await?;
        Ok(ProviderResumableUploadFragmentOutcome {
            completed: false,
            next_expected_ranges: status.next_expected_ranges,
        })
    }
}

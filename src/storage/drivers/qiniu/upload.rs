use super::{
    QiniuDriver,
    http::{self, ServiceKind},
    invalid,
};
use aster_drive_storage::{
    ExactSizeReader, MultipartStorageDriver, Result, StorageErrorKind, StreamUploadDriver,
    checked_upload_size, storage_driver_error,
};
use serde::Deserialize;
use std::{
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};
use tokio_util::io::ReaderStream;

const FORM_LIMIT: u64 = 1024 * 1024 * 1024;
const STREAM_PART: u64 = 8 * 1024 * 1024;
type UploadReader = Box<dyn AsyncRead + Unpin + Send + Sync>;

struct MultipartCleanup {
    driver: QiniuDriver,
    path: String,
    id: Option<String>,
}
impl Drop for MultipartCleanup {
    fn drop(&mut self) {
        let Some(id) = self.id.take() else {
            return;
        };
        let driver = self.driver.clone();
        let path = self.path.clone();
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                runtime.spawn(async move {
                if let Err(error) = driver.abort_multipart_upload(&path, &id).await {
                    tracing::warn!(kind = error.kind().as_str(), "Kodo canceled stream multipart cleanup failed; provider expiry remains the safety net");
                }
            });
            }
            Err(_) => tracing::warn!(
                "Kodo runtime stopped before multipart cleanup could start; provider expiry remains the safety net"
            ),
        }
    }
}
/// Per-upload ownership bridge. The lock covers a single nonblocking poll, never network I/O.
struct SharedReader(Arc<Mutex<ExactSizeReader<UploadReader>>>);
impl AsyncRead for SharedReader {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.0.lock() {
            Ok(mut reader) => Pin::new(&mut *reader).poll_read(cx, buffer),
            Err(_) => Poll::Ready(Err(std::io::Error::other(
                "Kodo upload reader lock poisoned",
            ))),
        }
    }
}
#[derive(Deserialize)]
struct Uploaded {
    key: String,
    hash: String,
}

impl QiniuDriver {
    pub(super) async fn put_multipart_reader(
        &self,
        path: &str,
        reader: UploadReader,
        size: u64,
    ) -> Result<String> {
        let part_size = size.div_ceil(10_000).max(STREAM_PART);
        if size == 0 || part_size > FORM_LIMIT {
            return Err(invalid(
                "Kodo object exceeds multipart capacity or is empty",
            ));
        }
        let id = self.create_multipart_upload(path).await?;
        let mut cleanup = MultipartCleanup {
            driver: self.clone(),
            path: path.to_string(),
            id: Some(id.clone()),
        };
        let result = async {
            let reader = Arc::new(Mutex::new(ExactSizeReader::new(reader, size)));
            let mut parts = Vec::new();
            let mut remaining = size;
            let mut number = 1;
            while remaining > 0 {
                let current_size = remaining.min(part_size);
                let fragment = SharedReader(reader.clone()).take(current_size);
                let etag = self
                    .upload_multipart_part_reader(
                        path,
                        &id,
                        number,
                        Box::new(fragment),
                        i64::try_from(current_size)
                            .map_err(|_| invalid("Kodo part size overflow"))?,
                    )
                    .await?;
                parts.push((number, etag));
                number += 1;
                remaining -= current_size;
            }
            self.complete_multipart_upload(path, &id, parts).await?;
            Ok(path.to_string())
        }
        .await;
        if result.is_err()
            && let Err(error) = self.abort_multipart_upload(path, &id).await
        {
            tracing::warn!(
                kind = error.kind().as_str(),
                "failed to abort Kodo streaming upload"
            );
        }
        cleanup.id = None;
        result
    }
    pub(super) async fn upload_form(
        &self,
        path: &str,
        body: reqwest::Body,
        size: u64,
    ) -> Result<String> {
        if size > FORM_LIMIT {
            return Err(invalid("Kodo form upload exceeds 1 GiB; use multipart"));
        }
        let key = self.key(path)?;
        let token = self.upload_token(
            &key,
            chrono::Utc::now().timestamp() + 3600,
            Some(size),
            false,
        )?;
        let url = self.endpoint(ServiceKind::Upload).await?;
        let file = reqwest::multipart::Part::stream_with_length(body, size).file_name("object");
        let form = reqwest::multipart::Form::new()
            .text("key", key.clone())
            .text("token", token)
            .part("file", file);
        let result: Uploaded = http::decode(
            self.client
                .post(url)
                .multipart(form)
                .send()
                .await
                .map_err(http::transport_error)?,
        )
        .await?;
        if result.key != key || result.hash.is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Precondition,
                "Kodo upload response does not identify the requested object",
            ));
        }
        Ok(path.to_string())
    }
}

#[async_trait::async_trait]
impl StreamUploadDriver for QiniuDriver {
    async fn put_reader(
        &self,
        path: &str,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> Result<String> {
        let size = checked_upload_size(size, "Kodo upload size")?;
        if size <= FORM_LIMIT {
            let reader = ExactSizeReader::new(reader, size);
            return self
                .upload_form(
                    path,
                    reqwest::Body::wrap_stream(ReaderStream::with_capacity(reader, 64 * 1024)),
                    size,
                )
                .await;
        }
        self.put_multipart_reader(path, reader, size).await
    }
    async fn put_file(&self, path: &str, local_path: &str) -> Result<String> {
        let file = tokio::fs::File::open(local_path).await.map_err(|_| {
            storage_driver_error(
                StorageErrorKind::Transient,
                "cannot open Kodo upload source",
            )
        })?;
        let size = i64::try_from(
            file.metadata()
                .await
                .map_err(|_| invalid("cannot stat Kodo upload source"))?
                .len(),
        )
        .map_err(|_| invalid("Kodo upload size overflow"))?;
        self.put_reader(path, Box::new(file), size).await
    }
}

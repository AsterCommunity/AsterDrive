use std::time::Duration;

use async_trait::async_trait;
use azure_core::http::{Body, RequestContent};
use azure_core::stream::SeekableStream;
use azure_storage_blob::models::{BlockListType, BlockLookupList};
use bytes::Bytes;
use futures::io::AsyncRead as FuturesAsyncRead;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, ReadBuf};

use aster_drive_storage::traits::driver::StorageDriver;
use aster_drive_storage::traits::extensions::StreamUploadDriver;
use aster_drive_storage::traits::multipart::{MultipartStorageDriver, UploadedMultipartPart};
use aster_drive_storage::{MapStorageErr, StorageErrorKind, storage_driver_error};
use aster_forge_utils::numbers;

use super::AzureBlobDriver;

const AZURE_STREAM_BUFFER_SIZE: usize = 64 * 1024;

#[derive(Clone)]
struct AzureSizedReaderStream {
    inner: Arc<Mutex<AzureSizedReaderState>>,
    len: u64,
}

struct AzureSizedReaderState {
    reader: Option<Box<dyn AsyncRead + Unpin + Send + Sync>>,
    remaining: u64,
}

impl AzureSizedReaderStream {
    fn new(reader: Box<dyn AsyncRead + Unpin + Send + Sync>, len: u64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(AzureSizedReaderState {
                reader: Some(reader),
                remaining: len,
            })),
            len,
        }
    }
}

impl std::fmt::Debug for AzureSizedReaderStream {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AzureSizedReaderStream")
            .field("len", &self.len)
            .finish_non_exhaustive()
    }
}

impl FuturesAsyncRead for AzureSizedReaderStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        output: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        if output.is_empty() {
            return Poll::Ready(Ok(0));
        }

        let mut state = self
            .inner
            .lock()
            .map_err(|_| std::io::Error::other("Azure Blob upload stream lock poisoned"))?;
        if state.remaining == 0 {
            return Poll::Ready(Ok(0));
        }

        let output_len = aster_forge_utils::numbers::usize_to_u64(
            output.len(),
            "Azure Blob upload stream output buffer length",
        )
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        let remaining = aster_forge_utils::numbers::u64_to_usize(
            state.remaining.min(output_len),
            "Azure Blob upload stream remaining",
        )
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        let Some(reader) = state.reader.as_mut() else {
            return Poll::Ready(Err(std::io::Error::other(
                "Azure Blob upload stream was already consumed",
            )));
        };
        let mut read_buf = ReadBuf::new(&mut output[..remaining]);
        match Pin::new(reader).poll_read(cx, &mut read_buf) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(())) => {
                let read = read_buf.filled().len();
                if read == 0 {
                    state.reader = None;
                    if state.remaining == 0 {
                        return Poll::Ready(Ok(0));
                    }
                    return Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        format!(
                            "Azure Blob upload stream ended before declared size: {} bytes missing",
                            state.remaining
                        ),
                    )));
                }
                let read_u64 = aster_forge_utils::numbers::usize_to_u64(
                    read,
                    "Azure Blob upload stream read length",
                )
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
                state.remaining -= read_u64;
                Poll::Ready(Ok(read))
            }
            Poll::Ready(Err(error)) => {
                state.reader = None;
                Poll::Ready(Err(error))
            }
        }
    }
}

#[async_trait]
impl SeekableStream for AzureSizedReaderStream {
    async fn reset(&mut self) -> azure_core::Result<()> {
        Err(azure_core::Error::with_message(
            azure_core::error::ErrorKind::Other,
            "Azure Blob streaming upload bodies cannot be reset without buffering",
        ))
    }

    fn len(&self) -> Option<u64> {
        Some(self.len)
    }

    fn buffer_size(&self) -> usize {
        AZURE_STREAM_BUFFER_SIZE
    }
}

#[async_trait]
impl MultipartStorageDriver for AzureBlobDriver {
    async fn create_multipart_upload(&self, path: &str) -> aster_drive_storage::Result<String> {
        Ok(format!("azure-block:{}", self.full_key(path)))
    }

    async fn presigned_upload_part_url(
        &self,
        path: &str,
        _upload_id: &str,
        part_number: i32,
        expires: Duration,
    ) -> aster_drive_storage::Result<String> {
        let mut url = self.block_blob_url(path, "cw", expires)?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("comp", "block");
            query.append_pair("blockid", &Self::block_id_marker(part_number)?);
        }
        Ok(url.to_string())
    }

    async fn complete_multipart_upload(
        &self,
        path: &str,
        _upload_id: &str,
        parts: Vec<(i32, String)>,
    ) -> aster_drive_storage::Result<()> {
        let mut part_numbers: Vec<i32> = parts
            .into_iter()
            .map(|(part_number, _)| part_number)
            .collect();
        part_numbers.sort_unstable();
        let latest = part_numbers
            .into_iter()
            .map(Self::block_id)
            .collect::<aster_drive_storage::Result<Vec<_>>>()?;
        let block_list = BlockLookupList {
            latest: Some(latest),
            ..Default::default()
        };
        let client = self.block_blob_client(path, "cw")?;
        client
            .commit_block_list(
                block_list.try_into().map_err(|error| {
                    storage_driver_error(
                        StorageErrorKind::Misconfigured,
                        format!("build Azure Blob block list: {error}"),
                    )
                })?,
                None,
            )
            .await
            .map_err(|error| {
                Self::map_azure_error("Azure Blob complete multipart upload failed", error)
            })?;
        Ok(())
    }

    async fn upload_multipart_part(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        data: &[u8],
    ) -> aster_drive_storage::Result<String> {
        self.upload_multipart_part_bytes(path, upload_id, part_number, Bytes::copy_from_slice(data))
            .await
    }

    async fn upload_multipart_part_bytes(
        &self,
        path: &str,
        _upload_id: &str,
        part_number: i32,
        data: Bytes,
    ) -> aster_drive_storage::Result<String> {
        let client = self.block_blob_client(path, "cw")?;
        let size = u64::try_from(data.len()).map_err(|error| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                format!("Azure Blob part size conversion failed: {error}"),
            )
        })?;
        client
            .stage_block(
                &Self::block_id(part_number)?,
                size,
                <RequestContent<Bytes, azure_core::http::NoFormat> as From<Bytes>>::from(data),
                None,
            )
            .await
            .map_err(|error| Self::map_azure_error("Azure Blob upload part failed", error))?;
        Ok(Self::block_id_marker(part_number)?)
    }

    async fn upload_multipart_part_reader(
        &self,
        path: &str,
        _upload_id: &str,
        part_number: i32,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> aster_drive_storage::Result<String> {
        let client = self.block_blob_client(path, "cw")?;
        let content_length =
            aster_forge_utils::numbers::i64_to_u64(size, "Azure Blob multipart part size")
                .map_storage_err(StorageErrorKind::Misconfigured)?;
        let body: RequestContent<Bytes, azure_core::http::NoFormat> = Body::SeekableStream(
            Box::new(AzureSizedReaderStream::new(reader, content_length)),
        )
        .into();
        client
            .stage_block(&Self::block_id(part_number)?, content_length, body, None)
            .await
            .map_err(|error| Self::map_azure_error("Azure Blob upload part failed", error))?;
        Ok(Self::block_id_marker(part_number)?)
    }

    async fn abort_multipart_upload(
        &self,
        _path: &str,
        _upload_id: &str,
    ) -> aster_drive_storage::Result<()> {
        // Azure uncommitted blocks are garbage collected by the service. There is
        // no direct abort operation for a block list that has not been committed.
        Ok(())
    }

    async fn list_uploaded_part_details(
        &self,
        path: &str,
        _upload_id: &str,
    ) -> aster_drive_storage::Result<Vec<UploadedMultipartPart>> {
        let client = self.block_blob_client(path, "r")?;
        let list = match client
            .get_block_list(BlockListType::Uncommitted, None)
            .await
        {
            Ok(response) => response
                .into_model()
                .map_err(|error| Self::map_azure_error("Azure Blob decode parts failed", error))?,
            Err(error) if Self::classify_azure_error(&error) == StorageErrorKind::NotFound => {
                return Ok(Vec::new());
            }
            Err(error) => return Err(Self::map_azure_error("Azure Blob list parts failed", error)),
        };
        let parts = list
            .uncommitted_blocks
            .unwrap_or_default()
            .into_iter()
            .filter_map(|block| {
                let name = block.name?;
                let size = block.size?;
                let marker =
                    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &name)
                        .ok()
                        .and_then(|decoded| String::from_utf8(decoded).ok())
                        .or_else(|| String::from_utf8(name).ok())?;
                let number = marker.strip_prefix("aster-part-")?.parse::<i32>().ok()?;
                Some(UploadedMultipartPart {
                    part_number: number,
                    size,
                })
            })
            .collect();
        Ok(parts)
    }
}

#[async_trait]
impl StreamUploadDriver for AzureBlobDriver {
    async fn put_reader(
        &self,
        storage_path: &str,
        mut reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> aster_drive_storage::Result<String> {
        use tokio::io::AsyncReadExt as _;

        let expected_size = numbers::i64_to_u64(size, "Azure Blob put_reader declared size")
            .map_storage_err(StorageErrorKind::Misconfigured)?;
        let chunk_size = self.chunk_size_for_content(expected_size)?;
        let mut remaining = expected_size;
        let mut part_number = 1_i32;
        let mut parts = Vec::new();

        if remaining == 0 {
            self.put(storage_path, &[]).await?;
            return Ok(storage_path.to_string());
        }

        while remaining > 0 {
            let read_limit = numbers::u64_to_usize(
                remaining.min(
                    numbers::usize_to_u64(chunk_size, "Azure Blob put_reader chunk size")
                        .map_storage_err(StorageErrorKind::Misconfigured)?,
                ),
                "Azure Blob put_reader next chunk size",
            )
            .map_storage_err(StorageErrorKind::Misconfigured)?;
            let mut chunk = vec![0_u8; read_limit];
            reader.read_exact(&mut chunk).await.map_storage_err_ctx(
                StorageErrorKind::Precondition,
                "read Azure Blob upload chunk",
            )?;
            let marker = self
                .upload_multipart_part_bytes(storage_path, "", part_number, Bytes::from(chunk))
                .await?;
            parts.push((part_number, marker));
            remaining -= numbers::usize_to_u64(read_limit, "Azure Blob uploaded chunk size")
                .map_storage_err(StorageErrorKind::Misconfigured)?;
            part_number = part_number.checked_add(1).ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "Azure Blob put_reader requires too many blocks",
                )
            })?;
        }

        let mut extra = [0_u8; 1];
        let extra_read = reader.read(&mut extra).await.map_storage_err_ctx(
            StorageErrorKind::Precondition,
            "check Azure Blob upload stream length",
        )?;
        if extra_read != 0 {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Azure Blob upload stream exceeded declared size",
            ));
        }

        self.complete_multipart_upload(storage_path, "", parts)
            .await?;
        Ok(storage_path.to_string())
    }

    async fn put_file(
        &self,
        storage_path: &str,
        local_path: &str,
    ) -> aster_drive_storage::Result<String> {
        let file = tokio::fs::File::open(local_path)
            .await
            .map_storage_err_ctx(StorageErrorKind::Transient, "open Azure Blob upload file")?;
        let metadata = file
            .metadata()
            .await
            .map_storage_err_ctx(StorageErrorKind::Transient, "stat Azure Blob upload file")?;
        let size =
            aster_forge_utils::numbers::u64_to_i64(metadata.len(), "Azure Blob upload file size")
                .map_storage_err(StorageErrorKind::Misconfigured)?;
        self.put_reader(storage_path, Box::new(file), size).await
    }
}

#[cfg(test)]
mod tests {
    use futures::io::AsyncReadExt as _;

    use super::{AZURE_STREAM_BUFFER_SIZE, AzureSizedReaderStream};
    use azure_core::stream::SeekableStream;

    #[tokio::test]
    async fn sized_reader_stream_reads_only_declared_length() {
        let reader = Box::new(std::io::Cursor::new(b"abcdef".to_vec()));
        let mut stream = AzureSizedReaderStream::new(reader, 3);
        let mut output = Vec::new();

        stream
            .read_to_end(&mut output)
            .await
            .expect("stream should read declared length");

        assert_eq!(output, b"abc");
        assert_eq!(stream.len(), Some(3));
        assert_eq!(stream.buffer_size(), AZURE_STREAM_BUFFER_SIZE);
    }

    #[tokio::test]
    async fn sized_reader_stream_rejects_early_eof() {
        let reader = Box::new(std::io::Cursor::new(b"abc".to_vec()));
        let mut stream = AzureSizedReaderStream::new(reader, 5);
        let mut output = Vec::new();

        let error = stream
            .read_to_end(&mut output)
            .await
            .expect_err("short stream should fail");

        assert_eq!(error.kind(), std::io::ErrorKind::UnexpectedEof);
        assert_eq!(output, b"abc");
    }

    #[tokio::test]
    async fn sized_reader_stream_reset_is_not_supported_without_buffering() {
        let reader = Box::new(std::io::Cursor::new(b"abc".to_vec()));
        let mut stream = AzureSizedReaderStream::new(reader, 3);

        assert!(stream.reset().await.is_err());
    }
}

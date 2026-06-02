use std::pin::Pin;
use std::task::{Context, Poll};

use async_trait::async_trait;
use aws_sdk_s3::primitives::ByteStream;
use bytes::Bytes;
use futures::Stream;
use http_body::{Frame, SizeHint};
use tokio::io::AsyncRead;
use tokio_util::io::ReaderStream;

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::storage::traits::extensions::StreamUploadDriver;
use crate::utils::numbers;

use super::S3Driver;

pub(super) const STREAM_UPLOAD_BUFFER_SIZE: usize = 64 * 1024;

pub(super) struct SizedReaderBody<R> {
    stream: ReaderStream<R>,
    remaining: u64,
    finished: bool,
}

impl<R> SizedReaderBody<R>
where
    R: AsyncRead + Unpin,
{
    pub(super) fn new(reader: R, size: u64) -> Self {
        Self {
            stream: ReaderStream::with_capacity(reader, STREAM_UPLOAD_BUFFER_SIZE),
            remaining: size,
            finished: false,
        }
    }
}

impl<R> http_body::Body for SizedReaderBody<R>
where
    R: AsyncRead + Unpin + Send + Sync + 'static,
{
    type Data = Bytes;
    type Error = std::io::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<std::result::Result<Frame<Self::Data>, Self::Error>>> {
        if self.finished {
            return Poll::Ready(None);
        }

        match Pin::new(&mut self.stream).poll_next(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Some(Ok(chunk))) => {
                let chunk_len =
                    match numbers::usize_to_u64(chunk.len(), "s3 upload stream chunk size") {
                        Ok(value) => value,
                        Err(error) => {
                            self.finished = true;
                            return Poll::Ready(Some(Err(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                error.to_string(),
                            ))));
                        }
                    };
                if chunk_len > self.remaining {
                    self.finished = true;
                    return Poll::Ready(Some(Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "upload stream exceeded declared size",
                    ))));
                }

                self.remaining -= chunk_len;
                Poll::Ready(Some(Ok(Frame::data(chunk))))
            }
            Poll::Ready(Some(Err(err))) => {
                self.finished = true;
                Poll::Ready(Some(Err(err)))
            }
            Poll::Ready(None) => {
                self.finished = true;
                if self.remaining == 0 {
                    Poll::Ready(None)
                } else {
                    Poll::Ready(Some(Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        format!(
                            "upload stream ended before declared size: {} bytes missing",
                            self.remaining
                        ),
                    ))))
                }
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        self.finished && self.remaining == 0
    }

    fn size_hint(&self) -> SizeHint {
        let mut hint = SizeHint::new();
        hint.set_exact(self.remaining);
        hint
    }
}
// =============================================================================
// StreamUploadDriver 扩展
// =============================================================================

#[async_trait]
impl StreamUploadDriver for S3Driver {
    async fn put_file(&self, storage_path: &str, local_path: &str) -> Result<String> {
        let key = self.full_key(storage_path);
        let body = ByteStream::from_path(local_path)
            .await
            .map_aster_err_ctx("S3 read file", AsterError::storage_driver_error)?;
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(body)
            .send()
            .await
            .map_err(|err| Self::map_sdk_error("S3 put_file failed", err))?;
        Ok(storage_path.to_string())
    }

    async fn put_reader(
        &self,
        storage_path: &str,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> Result<String> {
        let key = self.full_key(storage_path);
        let content_length = numbers::i64_to_u64(size, "S3 put_reader content_length")?;
        let body = ByteStream::from_body_1_x(SizedReaderBody::new(reader, content_length));

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .content_length(size)
            .body(body)
            .send()
            .await
            .map_err(|err| Self::map_sdk_error("S3 put_reader failed", err))?;

        Ok(storage_path.to_string())
    }
}

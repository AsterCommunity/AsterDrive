use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, ReadBuf};

use crate::errors::{AsterError, Result};
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::storage::extensions::ListStorageDriver;
use crate::storage::multipart::MultipartStorageDriver;

use super::RemoteDriver;

struct HashingReader {
    inner: Box<dyn AsyncRead + Unpin + Send + Sync>,
    hasher: Arc<Mutex<Sha256>>,
}

impl AsyncRead for HashingReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        match Pin::new(&mut self.inner).poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {
                let filled = buf.filled();
                if filled.len() > before {
                    let mut hasher = self
                        .hasher
                        .lock()
                        .unwrap_or_else(|error| error.into_inner());
                    hasher.update(&filled[before..]);
                }
                Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}

#[async_trait]
impl MultipartStorageDriver for RemoteDriver {
    async fn create_multipart_upload(&self, _path: &str) -> Result<String> {
        Ok(crate::utils::id::new_uuid())
    }

    async fn presigned_upload_part_url(
        &self,
        _path: &str,
        upload_id: &str,
        part_number: i32,
        expires: Duration,
    ) -> Result<String> {
        if self.uses_reverse_tunnel {
            return Err(storage_driver_error(
                StorageErrorKind::Unsupported,
                "reverse tunnel remote nodes do not support presigned multipart upload URLs",
            ));
        }
        let part_key = Self::multipart_part_key(upload_id, part_number)?;
        self.client
            .presigned_put_url(&self.object_key(&part_key), expires)
    }

    async fn complete_multipart_upload(
        &self,
        path: &str,
        upload_id: &str,
        mut parts: Vec<(i32, String)>,
    ) -> Result<()> {
        if parts.is_empty() {
            return Err(AsterError::validation_error(
                "multipart completion requires at least one part",
            ));
        }

        parts.sort_by_key(|(part_number, _)| *part_number);
        let mut expected_size = 0i64;
        let mut part_keys = Vec::with_capacity(parts.len());
        for (part_number, _) in parts {
            let part_key = Self::multipart_part_key(upload_id, part_number)?;
            let remote_key = self.object_key(&part_key);
            let metadata = self.client.metadata(&remote_key).await?;
            let part_size = i64::try_from(metadata.size).map_err(|_| {
                storage_driver_error(
                    StorageErrorKind::Precondition,
                    "remote multipart part size exceeds i64 range",
                )
            })?;
            expected_size = expected_size.checked_add(part_size).ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Precondition,
                    "remote multipart expected size overflow",
                )
            })?;
            part_keys.push(remote_key);
        }

        self.client
            .compose_objects(&self.object_key(path), part_keys, expected_size)
            .await?;
        Ok(())
    }

    async fn upload_multipart_part(
        &self,
        _path: &str,
        upload_id: &str,
        part_number: i32,
        data: &[u8],
    ) -> Result<String> {
        let part_key = Self::multipart_part_key(upload_id, part_number)?;
        self.client
            .put_bytes(&self.object_key(&part_key), data)
            .await?;

        let mut hasher = Sha256::new();
        hasher.update(data);
        Ok(format!("\"{}\"", hex::encode(hasher.finalize())))
    }

    async fn upload_multipart_part_reader(
        &self,
        _path: &str,
        upload_id: &str,
        part_number: i32,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> Result<String> {
        let part_key = Self::multipart_part_key(upload_id, part_number)?;
        let size = u64::try_from(size).map_err(|_| {
            storage_driver_error(
                StorageErrorKind::Precondition,
                "remote multipart part size cannot be negative",
            )
        })?;
        let hasher = Arc::new(Mutex::new(Sha256::new()));
        let hashing_reader = HashingReader {
            inner: reader,
            hasher: hasher.clone(),
        };
        self.client
            .put_reader(&self.object_key(&part_key), Box::new(hashing_reader), size)
            .await?;

        let digest = {
            let hasher = hasher.lock().unwrap_or_else(|error| error.into_inner());
            hasher.clone().finalize()
        };
        Ok(format!("\"{}\"", hex::encode(digest)))
    }

    async fn abort_multipart_upload(&self, _path: &str, upload_id: &str) -> Result<()> {
        let prefix = Self::multipart_parts_prefix(upload_id);
        let parts = self.list_paths(Some(&prefix)).await?;
        for part_path in parts {
            self.client.delete(&self.object_key(&part_path)).await?;
        }
        Ok(())
    }

    async fn list_uploaded_parts(&self, _path: &str, upload_id: &str) -> Result<Vec<i32>> {
        let prefix = Self::multipart_parts_prefix(upload_id);
        let mut parts = self
            .list_paths(Some(&prefix))
            .await?
            .into_iter()
            .filter_map(|path| {
                path.rsplit('/')
                    .next()
                    .and_then(|segment| segment.parse::<i32>().ok())
            })
            .collect::<Vec<_>>();
        parts.sort_unstable();
        parts.dedup();
        Ok(parts)
    }
}

use std::collections::BTreeSet;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, ReadBuf};

use aster_drive_storage::error::{StorageErrorKind, storage_driver_error};
use aster_drive_storage::traits::extensions::ListStorageDriver;
use aster_drive_storage::traits::multipart::{
    MultipartStorageCapabilities, MultipartStorageDriver, MultipartUploadMode,
    UploadedMultipartPart,
};

use super::RemoteDriver;

pub(super) struct HashingReader {
    pub(super) inner: Box<dyn AsyncRead + Unpin + Send + Sync>,
    pub(super) hasher: Arc<Mutex<Sha256>>,
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
    fn capabilities(&self) -> MultipartStorageCapabilities {
        MultipartStorageCapabilities {
            min_part_size: 1,
            max_part_size: self.max_multipart_part_size,
            max_parts: self.max_multipart_parts,
            max_object_size: self.max_multipart_object_size,
            upload_mode: MultipartUploadMode::NativeStreaming,
        }
    }

    async fn create_multipart_upload(&self, _path: &str) -> aster_drive_storage::Result<String> {
        Ok(aster_forge_utils::id::new_uuid())
    }

    async fn presigned_upload_part_request(
        &self,
        _path: &str,
        upload_id: &str,
        part_number: i32,
        expires: Duration,
    ) -> aster_drive_storage::Result<aster_drive_storage::PresignedUploadRequest> {
        if self.uses_reverse_tunnel {
            return Err(storage_driver_error(
                StorageErrorKind::Unsupported,
                "reverse tunnel remote nodes do not support presigned multipart upload URLs",
            ));
        }
        let part_key =
            aster_drive_storage::object_path::multipart_part_path(upload_id, part_number)?;
        self.client
            .presigned_put_url(&self.object_key(&part_key), expires)
            .map(aster_drive_storage::PresignedUploadRequest::without_headers)
            .map_err(Into::into)
    }

    async fn complete_multipart_upload(
        &self,
        path: &str,
        upload_id: &str,
        mut parts: Vec<(i32, String)>,
    ) -> aster_drive_storage::Result<()> {
        if parts.is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "multipart completion requires at least one part",
            ));
        }

        parts.sort_by_key(|(part_number, _)| *part_number);
        let mut expected_size = 0i64;
        let mut part_keys = Vec::with_capacity(parts.len());
        for (part_number, _) in parts {
            let part_key =
                aster_drive_storage::object_path::multipart_part_path(upload_id, part_number)?;
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
    ) -> aster_drive_storage::Result<String> {
        let part_key =
            aster_drive_storage::object_path::multipart_part_path(upload_id, part_number)?;
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
    ) -> aster_drive_storage::Result<String> {
        let part_key =
            aster_drive_storage::object_path::multipart_part_path(upload_id, part_number)?;
        let size = u64::try_from(size).map_err(|_| {
            storage_driver_error(
                StorageErrorKind::Precondition,
                "remote multipart part size cannot be negative",
            )
        })?;
        // 占位签（digest 发送时未知）：本地 tee 与 follower 签名回传的 body
        // digest 比对，闭环链路内容完整性。
        let hasher = Arc::new(Mutex::new(Sha256::new()));
        let hashing_reader = HashingReader {
            inner: reader,
            hasher: hasher.clone(),
        };
        let follower_digest = self
            .client
            .put_reader(
                &self.object_key(&part_key),
                Box::new(hashing_reader),
                size,
                None,
            )
            .await?
            .ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Precondition,
                    "remote part upload response missing signed body digest",
                )
            })?;

        let local_digest = {
            let hasher = hasher.lock().unwrap_or_else(|error| error.into_inner());
            hex::encode(hasher.clone().finalize())
        };
        let follower_hex = follower_digest.trim_matches('"').to_owned();
        if follower_hex != local_digest {
            return Err(storage_driver_error(
                StorageErrorKind::Precondition,
                format!(
                    "remote part upload digest mismatch: local {local_digest}, follower {follower_hex}"
                ),
            ));
        }
        Ok(format!("\"{local_digest}\""))
    }

    async fn abort_multipart_upload(
        &self,
        _path: &str,
        upload_id: &str,
    ) -> aster_drive_storage::Result<()> {
        let prefix = aster_drive_storage::object_path::multipart_parts_prefix(upload_id)?;
        let parts = self.list_paths(Some(&prefix)).await?;
        // Validate the entire response before issuing any destructive request.
        // The listing is lower trust than this caller's upload ownership.
        let part_numbers = parts
            .iter()
            .map(|path| canonical_part_number(upload_id, path))
            .collect::<aster_drive_storage::Result<BTreeSet<_>>>()?;
        for part_number in part_numbers {
            let part_path =
                aster_drive_storage::object_path::multipart_part_path(upload_id, part_number)?;
            self.client.delete(&self.object_key(&part_path)).await?;
        }
        Ok(())
    }

    async fn list_uploaded_part_details(
        &self,
        _path: &str,
        upload_id: &str,
    ) -> aster_drive_storage::Result<Vec<UploadedMultipartPart>> {
        let prefix = aster_drive_storage::object_path::multipart_parts_prefix(upload_id)?;
        let mut parts = self
            .list_paths(Some(&prefix))
            .await?
            .into_iter()
            .filter_map(|path| {
                canonical_part_number(upload_id, &path)
                    .ok()
                    .map(|number| (number, path))
            })
            .collect::<Vec<_>>();
        parts.sort_unstable_by_key(|(part_number, _)| *part_number);
        parts.dedup_by_key(|(part_number, _)| *part_number);

        let mut details = Vec::with_capacity(parts.len());
        for (part_number, part_path) in parts {
            let metadata = self.client.metadata(&self.object_key(&part_path)).await?;
            let size = i64::try_from(metadata.size).map_err(|_| {
                storage_driver_error(
                    StorageErrorKind::Precondition,
                    "remote multipart part size exceeds i64 range",
                )
            })?;
            details.push(UploadedMultipartPart { part_number, size });
        }
        Ok(details)
    }
}

fn canonical_part_number(upload_id: &str, path: &str) -> aster_drive_storage::Result<i32> {
    let prefix = aster_drive_storage::object_path::multipart_parts_prefix(upload_id)?;
    let number = path
        .strip_prefix(&prefix)
        .and_then(|suffix| suffix.strip_prefix('/'))
        .and_then(|number| number.parse::<i32>().ok())
        .filter(|number| *number > 0)
        .ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Precondition,
                "remote listing contains a key outside this multipart upload",
            )
        })?;
    if aster_drive_storage::object_path::multipart_part_path(upload_id, number)? != path {
        return Err(storage_driver_error(
            StorageErrorKind::Precondition,
            "remote listing contains a noncanonical multipart part key",
        ));
    }
    Ok(number)
}

use std::path::Path;
use std::sync::{Arc, Mutex};

use aster_drive_storage::error::{StorageErrorKind, storage_driver_error};
use aster_drive_storage::traits::extensions::{StreamUploadDriver, checked_upload_size};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tokio::io::AsyncRead;

use super::RemoteDriver;
use super::multipart::HashingReader;

#[async_trait]
impl StreamUploadDriver for RemoteDriver {
    async fn put_reader(
        &self,
        storage_path: &str,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> aster_drive_storage::Result<String> {
        let size = checked_upload_size(size, "remote stream upload size")?;
        // 占位签（digest 发送时未知）：本地 tee 与 follower 签名回传的 body
        // digest 比对，闭环链路内容完整性；先承诺路径见 put_reader_with_sha256。
        let hasher = Arc::new(Mutex::new(Sha256::new()));
        let hashing_reader = HashingReader {
            inner: reader,
            hasher: hasher.clone(),
        };
        let follower_digest = self
            .client
            .put_reader(
                &self.object_key(storage_path),
                Box::new(hashing_reader),
                size,
                None,
            )
            .await?
            .ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Precondition,
                    "remote stream upload response missing signed body digest",
                )
            })?;
        let local_digest = {
            let hasher = hasher.lock().unwrap_or_else(|error| error.into_inner());
            hex::encode(hasher.clone().finalize())
        };
        if follower_digest.trim_matches('"') != local_digest {
            return Err(storage_driver_error(
                StorageErrorKind::Precondition,
                format!(
                    "remote stream upload digest mismatch: local {local_digest}, follower {}",
                    follower_digest.trim_matches('"')
                ),
            ));
        }
        Ok(storage_path.to_string())
    }

    async fn put_reader_with_sha256(
        &self,
        storage_path: &str,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
        content_sha256: &str,
    ) -> aster_drive_storage::Result<String> {
        let size = checked_upload_size(size, "remote stream upload size")?;
        // 先承诺签名：follower 收完对账 digest，篡改即被拒。
        self.client
            .put_reader(
                &self.object_key(storage_path),
                reader,
                size,
                Some(content_sha256),
            )
            .await?;
        Ok(storage_path.to_string())
    }

    async fn put_file(
        &self,
        storage_path: &str,
        local_path: &str,
    ) -> aster_drive_storage::Result<String> {
        let metadata = tokio::fs::metadata(local_path).await.map_err(|error| {
            storage_driver_error(
                StorageErrorKind::Transient,
                format!("remote put_file metadata: {error}"),
            )
        })?;
        let file = tokio::fs::File::open(Path::new(local_path))
            .await
            .map_err(|error| {
                storage_driver_error(
                    StorageErrorKind::Transient,
                    format!("remote put_file open: {error}"),
                )
            })?;
        self.put_reader(
            storage_path,
            Box::new(file),
            i64::try_from(metadata.len()).map_err(|_| {
                storage_driver_error(
                    StorageErrorKind::Precondition,
                    "remote put_file size exceeds i64 range",
                )
            })?,
        )
        .await
    }
}

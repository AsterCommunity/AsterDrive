use std::path::Path;
use std::time::Duration;

use tokio::io::AsyncWriteExt;

use crate::api::constants::HOUR_SECS;
use crate::api::subcode::ApiSubcode;
use crate::errors::{AsterError, MapAsterErr, Result, thumbnail_generation_error_with_subcode};
use crate::storage::{PresignedDownloadOptions, StorageDriver};

use super::shared::cli_source_temp_path;

fn thumbnail_source_temp_create_failed(message: String) -> AsterError {
    thumbnail_generation_error_with_subcode(ApiSubcode::ThumbnailSourceTempCreateFailed, message)
}

fn thumbnail_source_stream_failed(message: String) -> AsterError {
    thumbnail_generation_error_with_subcode(ApiSubcode::ThumbnailSourceStreamFailed, message)
}

fn thumbnail_source_temp_flush_failed(message: String) -> AsterError {
    thumbnail_generation_error_with_subcode(ApiSubcode::ThumbnailSourceTempFlushFailed, message)
}

fn thumbnail_source_temp_copy_failed(message: String) -> AsterError {
    thumbnail_generation_error_with_subcode(ApiSubcode::ThumbnailSourceTempCopyFailed, message)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PreparedCliSourceKind {
    LocalPath,
    PresignedUrl,
    StreamedTempFile,
}

impl PreparedCliSourceKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::LocalPath => "local_path",
            Self::PresignedUrl => "presigned_url",
            Self::StreamedTempFile => "streamed_temp_file",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedCliSource {
    input_arg: String,
    kind: PreparedCliSourceKind,
}

impl PreparedCliSource {
    pub(crate) fn input_arg(&self) -> &str {
        &self.input_arg
    }

    pub(crate) fn kind(&self) -> PreparedCliSourceKind {
        self.kind
    }
}

pub(crate) async fn prepare_cli_source(
    driver: &dyn StorageDriver,
    storage_path: &str,
    source_file_name: &str,
    source_mime_type: &str,
    temp_dir: &Path,
    allow_presigned_url: bool,
) -> Result<PreparedCliSource> {
    if let Some(local_path_driver) = driver.as_local_path() {
        let path = local_path_driver.resolve_local_path(storage_path)?;
        let input_path = cli_source_temp_path(temp_dir, source_file_name, source_mime_type);
        materialize_local_cli_source(&path, &input_path).await?;
        return Ok(PreparedCliSource {
            input_arg: input_path.to_string_lossy().into_owned(),
            kind: PreparedCliSourceKind::LocalPath,
        });
    }

    if allow_presigned_url && let Some(presigned_driver) = driver.as_presigned() {
        let url = presigned_driver
            .presigned_url(
                storage_path,
                Duration::from_secs(HOUR_SECS),
                PresignedDownloadOptions::default(),
            )
            .await?;
        if let Some(url) = url {
            return Ok(PreparedCliSource {
                input_arg: url,
                kind: PreparedCliSourceKind::PresignedUrl,
            });
        }
    }

    let input_path = cli_source_temp_path(temp_dir, source_file_name, source_mime_type);
    let mut input_file = tokio::fs::File::create(&input_path)
        .await
        .map_aster_err_ctx(
            "create media source temp file",
            thumbnail_source_temp_create_failed,
        )?;
    let mut input_stream = driver.get_stream(storage_path).await?;
    tokio::io::copy(&mut input_stream, &mut input_file)
        .await
        .map_aster_err_ctx(
            "stream media source temp file",
            thumbnail_source_stream_failed,
        )?;
    input_file.flush().await.map_aster_err_ctx(
        "flush media source temp file",
        thumbnail_source_temp_flush_failed,
    )?;
    drop(input_file);

    Ok(PreparedCliSource {
        input_arg: input_path.to_string_lossy().into_owned(),
        kind: PreparedCliSourceKind::StreamedTempFile,
    })
}

async fn materialize_local_cli_source(source_path: &Path, input_path: &Path) -> Result<()> {
    tokio::fs::copy(source_path, input_path)
        .await
        .map(|_| ())
        .map_aster_err_ctx(
            "copy local media source temp file",
            thumbnail_source_temp_copy_failed,
        )
}

#[cfg(test)]
mod tests {
    use super::{PreparedCliSourceKind, prepare_cli_source};
    use crate::errors::Result;
    use crate::storage::{BlobMetadata, PresignedDownloadOptions, StorageDriver};
    use crate::storage::{LocalPathStorageDriver, PresignedStorageDriver};
    use async_trait::async_trait;
    use std::path::PathBuf;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::io::{AsyncRead, ReadBuf};

    struct LocalPathOnlyDriver {
        path: PathBuf,
    }

    #[async_trait]
    impl StorageDriver for LocalPathOnlyDriver {
        async fn put(&self, _path: &str, _data: &[u8]) -> Result<String> {
            unreachable!()
        }

        async fn get(&self, _path: &str) -> Result<Vec<u8>> {
            unreachable!()
        }

        async fn get_stream(&self, _path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
            unreachable!()
        }

        async fn delete(&self, _path: &str) -> Result<()> {
            unreachable!()
        }

        async fn exists(&self, _path: &str) -> Result<bool> {
            unreachable!()
        }

        async fn metadata(&self, _path: &str) -> Result<BlobMetadata> {
            unreachable!()
        }

        fn as_local_path(&self) -> Option<&dyn LocalPathStorageDriver> {
            Some(self)
        }
    }

    impl LocalPathStorageDriver for LocalPathOnlyDriver {
        fn resolve_local_path(&self, _path: &str) -> Result<PathBuf> {
            Ok(self.path.clone())
        }
    }

    struct PresignedOnlyDriver {
        url: String,
    }

    #[async_trait]
    impl StorageDriver for PresignedOnlyDriver {
        async fn put(&self, _path: &str, _data: &[u8]) -> Result<String> {
            unreachable!()
        }

        async fn get(&self, _path: &str) -> Result<Vec<u8>> {
            unreachable!()
        }

        async fn get_stream(&self, _path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
            unreachable!()
        }

        async fn delete(&self, _path: &str) -> Result<()> {
            unreachable!()
        }

        async fn exists(&self, _path: &str) -> Result<bool> {
            unreachable!()
        }

        async fn metadata(&self, _path: &str) -> Result<BlobMetadata> {
            unreachable!()
        }

        fn as_presigned(&self) -> Option<&dyn PresignedStorageDriver> {
            Some(self)
        }
    }

    #[async_trait]
    impl PresignedStorageDriver for PresignedOnlyDriver {
        async fn presigned_url(
            &self,
            _path: &str,
            _expires: std::time::Duration,
            _options: PresignedDownloadOptions,
        ) -> Result<Option<String>> {
            Ok(Some(self.url.clone()))
        }

        async fn presigned_put_url(
            &self,
            _path: &str,
            _expires: std::time::Duration,
        ) -> Result<Option<String>> {
            unreachable!()
        }
    }

    struct StreamingDriver {
        bytes: Vec<u8>,
    }

    #[async_trait]
    impl StorageDriver for StreamingDriver {
        async fn put(&self, _path: &str, _data: &[u8]) -> Result<String> {
            unreachable!()
        }

        async fn get(&self, _path: &str) -> Result<Vec<u8>> {
            unreachable!()
        }

        async fn get_stream(&self, _path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
            Ok(Box::new(BytesReader {
                bytes: self.bytes.clone(),
                offset: 0,
            }))
        }

        async fn delete(&self, _path: &str) -> Result<()> {
            unreachable!()
        }

        async fn exists(&self, _path: &str) -> Result<bool> {
            unreachable!()
        }

        async fn metadata(&self, _path: &str) -> Result<BlobMetadata> {
            unreachable!()
        }
    }

    struct BytesReader {
        bytes: Vec<u8>,
        offset: usize,
    }

    impl AsyncRead for BytesReader {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            let remaining = &self.bytes[self.offset..];
            if remaining.is_empty() {
                return Poll::Ready(Ok(()));
            }
            let amount = remaining.len().min(buf.remaining());
            buf.put_slice(&remaining[..amount]);
            self.offset += amount;
            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn prepare_cli_source_materializes_local_path_with_source_extension() {
        let temp_dir = std::env::temp_dir().join(format!(
            "aster-media-cli-input-local-{}",
            rand::random::<u64>()
        ));
        tokio::fs::create_dir_all(&temp_dir).await.unwrap();
        let source_path = temp_dir.join("example-source.heic");
        tokio::fs::write(&source_path, b"local-source")
            .await
            .unwrap();

        let prepared = prepare_cli_source(
            &LocalPathOnlyDriver { path: source_path },
            "blob/source",
            "avatar.heic",
            "image/heic",
            &temp_dir,
            true,
        )
        .await
        .unwrap();

        assert_eq!(prepared.kind(), PreparedCliSourceKind::LocalPath);
        assert!(prepared.input_arg().ends_with("source.heic"));
        let stored = tokio::fs::read(prepared.input_arg()).await.unwrap();
        assert_eq!(stored, b"local-source");
        crate::utils::cleanup_temp_dir(temp_dir.to_string_lossy().as_ref()).await;
    }

    #[tokio::test]
    async fn prepare_cli_source_materializes_extensionless_local_path() {
        let temp_dir = std::env::temp_dir().join(format!(
            "aster-media-cli-input-local-extensionless-{}",
            rand::random::<u64>()
        ));
        tokio::fs::create_dir_all(&temp_dir).await.unwrap();
        let source_path = temp_dir.join("content-addressed-source");
        tokio::fs::write(&source_path, b"local-source")
            .await
            .unwrap();

        let prepared = prepare_cli_source(
            &LocalPathOnlyDriver { path: source_path },
            "blob/source",
            "dataset.csv",
            "text/csv",
            &temp_dir,
            true,
        )
        .await
        .unwrap();

        assert_eq!(prepared.kind(), PreparedCliSourceKind::LocalPath);
        assert!(prepared.input_arg().ends_with("source.csv"));
        let stored = tokio::fs::read(prepared.input_arg()).await.unwrap();
        assert_eq!(stored, b"local-source");
        crate::utils::cleanup_temp_dir(temp_dir.to_string_lossy().as_ref()).await;
    }

    #[tokio::test]
    async fn prepare_cli_source_uses_presigned_url_when_enabled() {
        let temp_dir = std::env::temp_dir().join(format!(
            "aster-media-cli-input-url-{}",
            rand::random::<u64>()
        ));
        tokio::fs::create_dir_all(&temp_dir).await.unwrap();

        let prepared = prepare_cli_source(
            &PresignedOnlyDriver {
                url: "https://example.com/presigned-source.mp4".to_string(),
            },
            "blob/source",
            "video.mp4",
            "video/mp4",
            &temp_dir,
            true,
        )
        .await
        .unwrap();

        assert_eq!(prepared.kind(), PreparedCliSourceKind::PresignedUrl);
        assert_eq!(
            prepared.input_arg(),
            "https://example.com/presigned-source.mp4"
        );
        crate::utils::cleanup_temp_dir(temp_dir.to_string_lossy().as_ref()).await;
    }

    #[tokio::test]
    async fn prepare_cli_source_streams_into_temp_file_when_needed() {
        let temp_dir = std::env::temp_dir().join(format!(
            "aster-media-cli-input-file-{}",
            rand::random::<u64>()
        ));
        tokio::fs::create_dir_all(&temp_dir).await.unwrap();

        let prepared = prepare_cli_source(
            &StreamingDriver {
                bytes: b"streamed-source".to_vec(),
            },
            "blob/source",
            "video.mp4",
            "video/mp4",
            &temp_dir,
            false,
        )
        .await
        .unwrap();

        assert_eq!(prepared.kind(), PreparedCliSourceKind::StreamedTempFile);
        assert!(prepared.input_arg().ends_with("source.mp4"));
        let stored = tokio::fs::read(prepared.input_arg()).await.unwrap();
        assert_eq!(stored, b"streamed-source");
        crate::utils::cleanup_temp_dir(temp_dir.to_string_lossy().as_ref()).await;
    }
}

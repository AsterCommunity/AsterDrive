//! 服务模块：`thumbnail_service`。

use std::io::Cursor;
use std::path::PathBuf;

use image::ImageFormat;
use image::imageops::FilterType;
use image::{ImageReader, Limits};

use crate::entities::file_blob;
use crate::errors::{
    AsterError, MapAsterErr, Result, thumbnail_generation_error_with_subcode,
    validation_error_with_subcode,
};
use crate::storage::StorageDriver;
use crate::utils::raii::TempFileGuard;
use tokio::io::AsyncWriteExt;

const THUMB_MAX_DIM: u32 = 200;
const THUMB_PREFIX: &str = "_thumb";
pub(crate) const CURRENT_THUMBNAIL_VERSION: &str = "1";
pub(crate) const IMAGES_THUMBNAIL_PROCESSOR_NAMESPACE: &str = "images";
/// 单次解码最大内存分配（防止恶意/超大图 OOM）
const MAX_DECODE_ALLOC: u64 = 128 * 1024 * 1024;

fn thumbnail_format_guess_failed(message: String) -> AsterError {
    thumbnail_generation_error_with_subcode("thumbnail.format_guess_failed", message)
}

fn thumbnail_decode_failed(message: String) -> AsterError {
    thumbnail_generation_error_with_subcode("thumbnail.decode_failed", message)
}

fn thumbnail_encode_failed(message: String) -> AsterError {
    thumbnail_generation_error_with_subcode("thumbnail.encode_failed", message)
}

fn thumbnail_source_open_failed(message: String) -> AsterError {
    thumbnail_generation_error_with_subcode("thumbnail.source_open_failed", message)
}

fn thumbnail_source_stream_failed(message: String) -> AsterError {
    thumbnail_generation_error_with_subcode("thumbnail.source_stream_failed", message)
}

fn thumbnail_task_panicked(message: String) -> AsterError {
    thumbnail_generation_error_with_subcode("thumbnail.task_panicked", message)
}

/// 计算缩略图在存储驱动中的路径
pub(crate) fn thumb_path(blob_hash: &str) -> String {
    thumb_path_for(
        blob_hash,
        IMAGES_THUMBNAIL_PROCESSOR_NAMESPACE,
        CURRENT_THUMBNAIL_VERSION,
    )
}

pub(crate) fn thumb_path_for(
    blob_hash: &str,
    thumbnail_processor: &str,
    thumbnail_version: &str,
) -> String {
    format!(
        "{}/{}/{}/{}/{}/{}.webp",
        THUMB_PREFIX,
        thumbnail_processor,
        thumbnail_version,
        &blob_hash[..2],
        &blob_hash[2..4],
        blob_hash
    )
}

pub(crate) fn thumbnail_etag_value_for(
    blob_hash: &str,
    thumbnail_processor: Option<&str>,
    thumbnail_version: Option<&str>,
) -> String {
    format!(
        "thumb-{}-{}-{blob_hash}",
        thumbnail_processor.unwrap_or(IMAGES_THUMBNAIL_PROCESSOR_NAMESPACE),
        thumbnail_version.unwrap_or(CURRENT_THUMBNAIL_VERSION)
    )
}

pub(crate) fn current_thumbnail_max_dim() -> u32 {
    THUMB_MAX_DIM
}

pub fn is_thumbnail_path(path: &str) -> bool {
    path.trim_start_matches('/')
        .starts_with(&format!("{THUMB_PREFIX}/"))
}

/// 解码图片 → 缩放 → 编码为 WebP（CPU 密集，应在 spawn_blocking 中调用）
fn generate_thumbnail_from_reader<R>(reader: ImageReader<R>) -> Result<Vec<u8>>
where
    R: std::io::BufRead + std::io::Seek,
{
    let mut reader = reader
        .with_guessed_format()
        .map_aster_err_ctx("guess format", thumbnail_format_guess_failed)?;
    let mut limits = Limits::default();
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    reader.limits(limits);

    let img = reader
        .decode()
        .map_aster_err_ctx("decode", thumbnail_decode_failed)?;

    // 已经小于目标尺寸 → 直接编码，跳过 resize
    if img.width() <= THUMB_MAX_DIM && img.height() <= THUMB_MAX_DIM {
        return encode_webp(&img);
    }

    // Triangle（双线性）滤镜：比 Lanczos3 快 2-3 倍，200px 缩略图肉眼无差
    let thumb = img.resize(THUMB_MAX_DIM, THUMB_MAX_DIM, FilterType::Triangle);
    drop(img); // 释放全尺寸像素 buffer，再编码

    encode_webp(&thumb)
}

fn generate_thumbnail_from_local_path(path: PathBuf) -> Result<Vec<u8>> {
    let reader = ImageReader::open(path)
        .map_aster_err_ctx("open thumbnail source", thumbnail_source_open_failed)?;
    generate_thumbnail_from_reader(reader)
}

async fn materialize_thumbnail_source_stream(
    driver: &dyn StorageDriver,
    blob: &file_blob::Model,
    temp_root: &str,
) -> Result<TempFileGuard> {
    let temp_dir = PathBuf::from(crate::utils::paths::runtime_temp_dir(temp_root));
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .map_aster_err_ctx("create thumbnail temp dir", thumbnail_source_stream_failed)?;
    let temp_source = TempFileGuard::new(
        temp_dir.join(format!("thumbnail-source-{}.tmp", uuid::Uuid::new_v4())),
        "thumbnail source temp file",
    );

    let mut stream = driver.get_stream(&blob.storage_path).await?;
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp_source.path())
        .await
        .map_aster_err_ctx(
            "create thumbnail source temp file",
            thumbnail_source_stream_failed,
        )?;

    let copied = tokio::io::copy(&mut stream, &mut file)
        .await
        .map_aster_err_ctx(
            "copy thumbnail source stream",
            thumbnail_source_stream_failed,
        )?;
    file.flush().await.map_aster_err_ctx(
        "flush thumbnail source temp file",
        thumbnail_source_stream_failed,
    )?;
    drop(file);

    let expected_size = crate::utils::numbers::i64_to_u64(blob.size, "thumbnail source blob size")?;
    if copied != expected_size {
        return Err(thumbnail_source_stream_failed(format!(
            "thumbnail source stream size mismatch: expected {expected_size} bytes, got {copied}"
        )));
    }

    Ok(temp_source)
}

fn encode_webp(img: &image::DynamicImage) -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    img.write_to(&mut buf, ImageFormat::WebP)
        .map_aster_err_ctx("encode webp", thumbnail_encode_failed)?;
    Ok(buf.into_inner())
}

pub(crate) async fn render_thumbnail_bytes(
    driver: &dyn StorageDriver,
    blob: &file_blob::Model,
    temp_root: &str,
) -> Result<Vec<u8>> {
    if let Some(local_path_driver) = driver.as_local_path() {
        let path = local_path_driver.resolve_local_path(&blob.storage_path)?;
        return tokio::task::spawn_blocking(move || generate_thumbnail_from_local_path(path))
            .await
            .map_aster_err_ctx("thumbnail task panicked", thumbnail_task_panicked)?;
    }

    let temp_source = materialize_thumbnail_source_stream(driver, blob, temp_root).await?;
    tokio::task::spawn_blocking(move || {
        let result = generate_thumbnail_from_local_path(temp_source.path().to_path_buf());
        drop(temp_source);
        result
    })
    .await
    .map_aster_err_ctx("thumbnail task panicked", thumbnail_task_panicked)?
}

pub(crate) fn ensure_source_size_supported(
    blob: &file_blob::Model,
    max_source_bytes: i64,
) -> Result<()> {
    if blob.size > max_source_bytes {
        return Err(validation_error_with_subcode(
            "thumbnail.source_too_large",
            format!(
                "thumbnail source exceeds {} MiB limit",
                max_source_bytes / 1024 / 1024
            ),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_source_size_supported, render_thumbnail_bytes, thumb_path, thumbnail_etag_value_for,
    };
    use crate::config::operations::DEFAULT_THUMBNAIL_MAX_SOURCE_BYTES;
    use crate::entities::file_blob;
    use crate::errors::Result;
    use crate::storage::{BlobMetadata, LocalPathStorageDriver, StorageDriver};
    use async_trait::async_trait;
    use chrono::Utc;
    use std::io::Cursor;
    use std::path::PathBuf;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::io::{AsyncRead, ReadBuf};

    fn tiny_png() -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        let encoder = image::codecs::png::PngEncoder::new(&mut buf);
        image::ImageEncoder::write_image(
            encoder,
            &[255, 0, 0],
            1,
            1,
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
        buf.into_inner()
    }

    fn blob_with_size(size: i64) -> file_blob::Model {
        file_blob::Model {
            id: 1,
            hash: "abc".repeat(21) + "a",
            size,
            policy_id: 1,
            storage_path: "files/test".to_string(),
            thumbnail_path: None,
            thumbnail_processor: None,
            thumbnail_version: None,
            ref_count: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    struct LocalPathOnlyDriver {
        path: PathBuf,
    }

    #[async_trait]
    impl StorageDriver for LocalPathOnlyDriver {
        async fn put(&self, _path: &str, _data: &[u8]) -> Result<String> {
            unreachable!()
        }

        async fn get(&self, _path: &str) -> Result<Vec<u8>> {
            panic!("local thumbnail rendering should not read the whole object into memory")
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

    struct StreamingOnlyDriver {
        bytes: Vec<u8>,
    }

    #[async_trait]
    impl StorageDriver for StreamingOnlyDriver {
        async fn put(&self, _path: &str, _data: &[u8]) -> Result<String> {
            unreachable!()
        }

        async fn get(&self, _path: &str) -> Result<Vec<u8>> {
            panic!("streaming thumbnail rendering should not read the whole object into memory")
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
    async fn local_path_thumbnail_rendering_does_not_call_get() {
        let source_path = std::env::temp_dir().join(format!(
            "aster-thumbnail-local-path-{}.png",
            uuid::Uuid::new_v4()
        ));
        tokio::fs::write(&source_path, tiny_png()).await.unwrap();

        let driver = LocalPathOnlyDriver {
            path: source_path.clone(),
        };
        let thumbnail = render_thumbnail_bytes(&driver, &blob_with_size(3), "")
            .await
            .unwrap();

        assert!(!thumbnail.is_empty());
        let _ = tokio::fs::remove_file(source_path).await;
    }

    #[tokio::test]
    async fn streaming_thumbnail_rendering_materializes_temp_file_without_calling_get() {
        let temp_root = std::env::temp_dir().join(format!(
            "aster-thumbnail-streaming-{}",
            uuid::Uuid::new_v4()
        ));
        let temp_root_str = temp_root.to_string_lossy().to_string();
        let source = tiny_png();
        let driver = StreamingOnlyDriver {
            bytes: source.clone(),
        };
        let source_size =
            crate::utils::numbers::usize_to_i64(source.len(), "test thumbnail source size")
                .unwrap();

        let thumbnail =
            render_thumbnail_bytes(&driver, &blob_with_size(source_size), &temp_root_str)
                .await
                .unwrap();

        assert!(!thumbnail.is_empty());
        let runtime_temp_dir = PathBuf::from(crate::utils::paths::runtime_temp_dir(&temp_root_str));
        let mut entries = tokio::fs::read_dir(&runtime_temp_dir).await.unwrap();
        assert!(
            entries.next_entry().await.unwrap().is_none(),
            "streaming thumbnail temp file should be cleaned up"
        );
        let _ = tokio::fs::remove_dir_all(temp_root).await;
    }

    #[test]
    fn accepts_thumbnail_source_within_size_limit() {
        let max_source_bytes = crate::utils::numbers::u64_to_i64(
            DEFAULT_THUMBNAIL_MAX_SOURCE_BYTES,
            "thumbnail max source bytes",
        )
        .unwrap();
        assert!(
            ensure_source_size_supported(&blob_with_size(max_source_bytes), max_source_bytes,)
                .is_ok()
        );
    }

    #[test]
    fn rejects_thumbnail_source_above_size_limit() {
        let max_source_bytes = crate::utils::numbers::u64_to_i64(
            DEFAULT_THUMBNAIL_MAX_SOURCE_BYTES,
            "thumbnail max source bytes",
        )
        .unwrap();
        let err =
            ensure_source_size_supported(&blob_with_size(max_source_bytes + 1), max_source_bytes)
                .unwrap_err();
        assert_eq!(err.api_error_subcode(), Some("thumbnail.source_too_large"));
    }

    #[test]
    fn thumbnail_paths_are_versioned() {
        let hash = "abc".repeat(21) + "a";
        assert_eq!(
            thumb_path(&hash),
            format!("_thumb/images/1/ab/ca/{hash}.webp")
        );
    }

    #[test]
    fn thumbnail_etag_uses_thumbnail_version_namespace() {
        let hash = "abc".repeat(21) + "a";
        assert_eq!(
            thumbnail_etag_value_for(&hash, None, None),
            format!("thumb-images-1-{hash}")
        );
    }

    #[test]
    fn thumbnail_etag_can_use_persisted_processor_and_version() {
        let hash = "abc".repeat(21) + "a";
        assert_eq!(
            thumbnail_etag_value_for(&hash, Some("vips-cli"), Some("7")),
            format!("thumb-vips-cli-7-{hash}")
        );
    }
}

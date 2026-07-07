//! 服务模块：`thumbnail`。

use std::io::Cursor;
use std::path::PathBuf;

use image::ImageFormat;
use image::imageops::FilterType;
use image::{ImageReader, Limits};

use crate::api::api_error_code::ApiErrorCode;
use crate::config::operations;
use crate::entities::file_blob;
use crate::errors::{
    AsterError, MapAsterErr, Result, thumbnail_generation_error_with_code,
    validation_error_with_code,
};
use crate::storage::StorageDriver;
use crate::utils::raii::TempFileGuard;
use tokio::io::AsyncWriteExt;

const THUMB_PREFIX: &str = "_thumb";
const PREVIEW_PREFIX: &str = "_preview";
pub(crate) const CURRENT_THUMBNAIL_VERSION: &str = "1";
pub(crate) const CURRENT_IMAGE_PREVIEW_VERSION: &str = "1";
pub(crate) const IMAGES_THUMBNAIL_PROCESSOR_NAMESPACE: &str = "images";
/// 单次解码最大内存分配（防止恶意/超大图 OOM）
const MAX_DECODE_ALLOC: u64 = 128 * 1024 * 1024;

fn thumbnail_format_guess_failed(message: String) -> AsterError {
    thumbnail_generation_error_with_code(ApiErrorCode::ThumbnailFormatGuessFailed, message)
}

fn thumbnail_decode_failed(message: String) -> AsterError {
    thumbnail_generation_error_with_code(ApiErrorCode::ThumbnailDecodeFailed, message)
}

fn thumbnail_encode_failed(message: String) -> AsterError {
    thumbnail_generation_error_with_code(ApiErrorCode::ThumbnailEncodeFailed, message)
}

fn thumbnail_source_open_failed(message: String) -> AsterError {
    thumbnail_generation_error_with_code(ApiErrorCode::ThumbnailSourceOpenFailed, message)
}

fn thumbnail_source_stream_failed(message: String) -> AsterError {
    thumbnail_generation_error_with_code(ApiErrorCode::ThumbnailSourceStreamFailed, message)
}

fn thumbnail_task_panicked(message: String) -> AsterError {
    thumbnail_generation_error_with_code(ApiErrorCode::ThumbnailTaskPanicked, message)
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

pub(crate) fn image_preview_path_for(
    blob_hash: &str,
    image_preview_processor: &str,
    image_preview_version: &str,
) -> String {
    format!(
        "{}/{}/{}/{}/{}/{}.webp",
        PREVIEW_PREFIX,
        image_preview_processor,
        image_preview_version,
        &blob_hash[..2],
        &blob_hash[2..4],
        blob_hash
    )
}

pub(crate) fn image_preview_etag_value_for(
    blob_hash: &str,
    image_preview_processor: &str,
    image_preview_version: &str,
) -> String {
    format!("image-preview-{image_preview_processor}-{image_preview_version}-{blob_hash}")
}

pub(crate) fn thumbnail_version_for_dimension(max_dim: u32) -> String {
    derivative_version_for_dimension(
        CURRENT_THUMBNAIL_VERSION,
        max_dim,
        operations::DEFAULT_THUMBNAIL_MAX_DIMENSION,
    )
}

pub(crate) fn image_preview_version_for_dimension(max_dim: u32) -> String {
    derivative_version_for_dimension(
        CURRENT_IMAGE_PREVIEW_VERSION,
        max_dim,
        operations::DEFAULT_IMAGE_PREVIEW_MAX_DIMENSION,
    )
}

fn derivative_version_for_dimension(base_version: &str, max_dim: u32, default_dim: u32) -> String {
    if max_dim == default_dim {
        base_version.to_string()
    } else {
        format!("{base_version}-d{max_dim}")
    }
}

pub fn is_thumbnail_path(path: &str) -> bool {
    path.trim_start_matches('/')
        .starts_with(&format!("{THUMB_PREFIX}/"))
}

pub fn is_image_preview_path(path: &str) -> bool {
    path.trim_start_matches('/')
        .starts_with(&format!("{PREVIEW_PREFIX}/"))
}

/// 解码图片 → 缩放 → 编码为 WebP（CPU 密集，应在 spawn_blocking 中调用）
fn generate_thumbnail_from_reader<R>(reader: ImageReader<R>, max_dim: u32) -> Result<Vec<u8>>
where
    R: std::io::BufRead + std::io::Seek,
{
    generate_webp_derivative_from_reader(reader, max_dim)
}

pub(crate) fn render_webp_derivative_from_image_bytes<R>(reader: R, max_dim: u32) -> Result<Vec<u8>>
where
    R: std::io::BufRead + std::io::Seek,
{
    generate_webp_derivative_from_reader(ImageReader::new(reader), max_dim)
}

fn generate_webp_derivative_from_reader<R>(reader: ImageReader<R>, max_dim: u32) -> Result<Vec<u8>>
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

    if img.width() <= max_dim && img.height() <= max_dim {
        return encode_webp(&img);
    }

    // Triangle（双线性）滤镜：比 Lanczos3 快 2-3 倍，小尺寸导出肉眼无差
    let preview = img.resize(max_dim, max_dim, FilterType::Triangle);
    drop(img);

    encode_webp(&preview)
}

fn generate_thumbnail_from_local_path(path: PathBuf, max_dim: u32) -> Result<Vec<u8>> {
    let reader = ImageReader::open(path)
        .map_aster_err_ctx("open thumbnail source", thumbnail_source_open_failed)?;
    generate_thumbnail_from_reader(reader, max_dim)
}

fn generate_webp_derivative_from_local_path(path: PathBuf, max_dim: u32) -> Result<Vec<u8>> {
    let reader = ImageReader::open(path)
        .map_aster_err_ctx("open thumbnail source", thumbnail_source_open_failed)?;
    generate_webp_derivative_from_reader(reader, max_dim)
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
    max_dim: u32,
) -> Result<Vec<u8>> {
    if let Some(local_path_driver) = driver.as_local_path() {
        let path = local_path_driver.resolve_local_path(&blob.storage_path)?;
        return tokio::task::spawn_blocking(move || {
            generate_thumbnail_from_local_path(path, max_dim)
        })
        .await
        .map_aster_err_ctx("thumbnail task panicked", thumbnail_task_panicked)?;
    }

    let temp_source = materialize_thumbnail_source_stream(driver, blob, temp_root).await?;
    tokio::task::spawn_blocking(move || {
        let result = generate_thumbnail_from_local_path(temp_source.path().to_path_buf(), max_dim);
        drop(temp_source);
        result
    })
    .await
    .map_aster_err_ctx("thumbnail task panicked", thumbnail_task_panicked)?
}

pub(crate) async fn render_webp_derivative_bytes(
    driver: &dyn StorageDriver,
    blob: &file_blob::Model,
    temp_root: &str,
    max_dim: u32,
) -> Result<Vec<u8>> {
    if let Some(local_path_driver) = driver.as_local_path() {
        let path = local_path_driver.resolve_local_path(&blob.storage_path)?;
        return tokio::task::spawn_blocking(move || {
            generate_webp_derivative_from_local_path(path, max_dim)
        })
        .await
        .map_aster_err_ctx("thumbnail task panicked", thumbnail_task_panicked)?;
    }

    let temp_source = materialize_thumbnail_source_stream(driver, blob, temp_root).await?;
    tokio::task::spawn_blocking(move || {
        let result =
            generate_webp_derivative_from_local_path(temp_source.path().to_path_buf(), max_dim);
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
        return Err(validation_error_with_code(
            ApiErrorCode::ThumbnailSourceTooLarge,
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
        CURRENT_IMAGE_PREVIEW_VERSION, CURRENT_THUMBNAIL_VERSION,
        IMAGES_THUMBNAIL_PROCESSOR_NAMESPACE, ensure_source_size_supported,
        image_preview_etag_value_for, image_preview_path_for, image_preview_version_for_dimension,
        render_thumbnail_bytes, render_webp_derivative_from_image_bytes, thumb_path_for,
        thumbnail_etag_value_for, thumbnail_version_for_dimension,
    };
    use crate::api::api_error_code::ApiErrorCode;
    use crate::config::operations::DEFAULT_THUMBNAIL_MAX_SOURCE_BYTES;
    use crate::entities::file_blob;
    use crate::errors::Result;
    use crate::storage::{BlobMetadata, LocalPathStorageDriver, StorageDriver};
    use async_trait::async_trait;
    use chrono::Utc;
    use image::{GenericImageView, ImageFormat, Rgb, RgbImage};
    use std::io::Cursor;
    use std::path::PathBuf;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::io::{AsyncRead, ReadBuf};

    fn tiny_png() -> Vec<u8> {
        sample_png(1, 1)
    }

    fn sample_png(width: u32, height: u32) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        let image =
            image::DynamicImage::ImageRgb8(RgbImage::from_pixel(width, height, Rgb([255, 0, 0])));
        image.write_to(&mut buf, ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    fn rendered_dimensions(bytes: &[u8]) -> (u32, u32) {
        image::load_from_memory(bytes).unwrap().dimensions()
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
        let thumbnail = render_thumbnail_bytes(
            &driver,
            &blob_with_size(3),
            "",
            crate::config::operations::DEFAULT_THUMBNAIL_MAX_DIMENSION,
        )
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

        let thumbnail = render_thumbnail_bytes(
            &driver,
            &blob_with_size(source_size),
            &temp_root_str,
            crate::config::operations::DEFAULT_THUMBNAIL_MAX_DIMENSION,
        )
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
    fn render_derivative_respects_max_dimension_for_landscape_and_portrait_images() {
        let landscape =
            render_webp_derivative_from_image_bytes(Cursor::new(sample_png(400, 200)), 100)
                .unwrap();
        assert_eq!(rendered_dimensions(&landscape), (100, 50));

        let portrait =
            render_webp_derivative_from_image_bytes(Cursor::new(sample_png(200, 400)), 100)
                .unwrap();
        assert_eq!(rendered_dimensions(&portrait), (50, 100));
    }

    #[test]
    fn render_derivative_preserves_images_already_within_max_dimension() {
        let exact =
            render_webp_derivative_from_image_bytes(Cursor::new(sample_png(100, 50)), 100).unwrap();
        assert_eq!(rendered_dimensions(&exact), (100, 50));

        let smaller =
            render_webp_derivative_from_image_bytes(Cursor::new(sample_png(64, 32)), 100).unwrap();
        assert_eq!(rendered_dimensions(&smaller), (64, 32));
    }

    #[test]
    fn render_derivative_accepts_minimum_dimension_boundary() {
        let rendered =
            render_webp_derivative_from_image_bytes(Cursor::new(sample_png(10, 20)), 1).unwrap();
        assert_eq!(rendered_dimensions(&rendered), (1, 1));
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
        assert_eq!(
            err.api_error_code_override(),
            Some(ApiErrorCode::ThumbnailSourceTooLarge)
        );
    }

    #[test]
    fn thumbnail_paths_are_versioned() {
        let hash = "abc".repeat(21) + "a";
        assert_eq!(
            thumb_path_for(
                &hash,
                IMAGES_THUMBNAIL_PROCESSOR_NAMESPACE,
                CURRENT_THUMBNAIL_VERSION
            ),
            format!("_thumb/images/1/ab/ca/{hash}.webp")
        );
    }

    #[test]
    fn derivative_version_uses_dimension_suffix_only_for_non_default_dimensions() {
        assert_eq!(
            thumbnail_version_for_dimension(
                crate::config::operations::DEFAULT_THUMBNAIL_MAX_DIMENSION,
            ),
            "1"
        );
        assert_eq!(thumbnail_version_for_dimension(320), "1-d320");
        assert_eq!(
            image_preview_version_for_dimension(
                crate::config::operations::DEFAULT_IMAGE_PREVIEW_MAX_DIMENSION,
            ),
            "1"
        );
        assert_eq!(image_preview_version_for_dimension(2048), "1-d2048");
        assert_eq!(
            thumbnail_version_for_dimension(
                crate::config::operations::MAX_DERIVATIVE_MAX_DIMENSION
            ),
            "1-d16384"
        );
        assert_eq!(
            image_preview_version_for_dimension(
                crate::config::operations::MAX_DERIVATIVE_MAX_DIMENSION,
            ),
            "1-d16384"
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

    #[test]
    fn image_preview_paths_are_versioned() {
        let hash = "abc".repeat(21) + "a";
        assert_eq!(
            image_preview_path_for(&hash, "images", CURRENT_IMAGE_PREVIEW_VERSION),
            format!("_preview/images/1/ab/ca/{hash}.webp")
        );
    }

    #[test]
    fn image_preview_etag_uses_preview_version_namespace() {
        let hash = "abc".repeat(21) + "a";
        assert_eq!(
            image_preview_etag_value_for(&hash, "images", CURRENT_IMAGE_PREVIEW_VERSION),
            format!("image-preview-images-1-{hash}")
        );
    }
}

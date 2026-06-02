//! StorageDriver 扩展 trait
//!
//! 将可选能力从核心 StorageDriver 分离，避免每个驱动被迫实现不需要的功能。

use crate::errors::Result;
use crate::storage::traits::driver::{PresignedDownloadOptions, StoragePathVisitor};
use crate::types::{MediaMetadataKind, MediaMetadataPayload};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::AsyncRead;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum StorageCapacityStatus {
    Supported,
    Unsupported,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageCapacityInfo {
    pub status: StorageCapacityStatus,
    pub total_bytes: Option<i64>,
    pub available_bytes: Option<i64>,
    pub used_bytes: Option<i64>,
    pub source: String,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub observed_at: DateTime<Utc>,
}

impl StorageCapacityInfo {
    pub fn unsupported(source: impl Into<String>) -> Self {
        Self {
            status: StorageCapacityStatus::Unsupported,
            total_bytes: None,
            available_bytes: None,
            used_bytes: None,
            source: source.into(),
            observed_at: Utc::now(),
        }
    }

    pub fn unavailable(source: impl Into<String>) -> Self {
        Self {
            status: StorageCapacityStatus::Unavailable,
            total_bytes: None,
            available_bytes: None,
            used_bytes: None,
            source: source.into(),
            observed_at: Utc::now(),
        }
    }
}

/// Presigned URL 支持（S3/R2/OSS/remote follower 等）
#[async_trait]
pub trait PresignedStorageDriver: Send + Sync {
    /// 生成临时下载 URL
    async fn presigned_url(
        &self,
        path: &str,
        expires: Duration,
        options: PresignedDownloadOptions,
    ) -> Result<Option<String>>;

    /// 生成 presigned PUT URL 供客户端直传
    async fn presigned_put_url(&self, path: &str, expires: Duration) -> Result<Option<String>>;
}

/// 路径列举支持（用于后台维护任务）
#[async_trait]
pub trait ListStorageDriver: Send + Sync {
    /// 列出当前策略下的对象路径（相对路径）。
    ///
    /// 该接口会把结果完整收集到内存，适合小范围列举。完整审计、孤儿对象清理
    /// 等大规模扫描路径应使用 `scan_paths`，避免在 S3 等后端一次性拉取全部 key。
    async fn list_paths(&self, prefix: Option<&str>) -> Result<Vec<String>>;

    /// 逐条扫描当前策略下的对象路径，避免一次性拉取整个列表
    ///
    /// 默认实现基于 list_paths，驱动可覆盖优化（如流式 API）
    async fn scan_paths(
        &self,
        prefix: Option<&str>,
        visitor: &mut dyn StoragePathVisitor,
    ) -> Result<()> {
        for path in self.list_paths(prefix).await? {
            visitor.visit_path(path)?;
        }
        Ok(())
    }
}

/// 流式直传支持（避免本地临时文件）
#[async_trait]
pub trait StreamUploadDriver: Send + Sync {
    /// 从 reader 流式写入存储
    ///
    /// 适用于不应先落本地临时文件的上传路径（如 WebDAV 直传、S3 流式上传）。
    /// 驱动可实现优化路径；默认实现写临时文件后调用 put_file。
    async fn put_reader(
        &self,
        storage_path: &str,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> Result<String>;

    /// 从本地文件路径写入存储（分片上传组装后使用）
    ///
    /// 这是 put_reader 默认实现的基础；暴露出来供需要显式控制临时文件生命周期的调用方使用。
    async fn put_file(&self, storage_path: &str, local_path: &str) -> Result<String>;
}

/// 本地路径暴露（仅用于把底层文件路径安全交给受控的外部命令）
pub trait LocalPathStorageDriver: Send + Sync {
    /// 解析某个存储对象在本机文件系统上的真实绝对路径。
    fn resolve_local_path(&self, path: &str) -> Result<PathBuf>;
}

#[derive(Debug, Clone)]
pub struct NativeThumbnailRequest {
    pub storage_path: String,
    pub source_mime_type: String,
    pub max_width: u32,
    pub max_height: u32,
}

/// 存储侧原生缩略图支持（OneDrive / 数据万象 / 对象存储图片处理等）
#[async_trait]
pub trait NativeThumbnailStorageDriver: Send + Sync {
    /// 返回 `None` 表示该驱动当前不支持这个对象或 MIME 的原生缩略图能力。
    async fn get_native_thumbnail(
        &self,
        request: &NativeThumbnailRequest,
    ) -> Result<Option<Vec<u8>>>;
}

#[derive(Debug, Clone)]
pub struct NativeMediaMetadataRequest {
    pub storage_path: String,
    pub source_file_name: String,
    pub source_mime_type: String,
    pub kind: MediaMetadataKind,
}

#[derive(Debug, Clone)]
pub struct NativeMediaMetadataResult {
    pub kind: MediaMetadataKind,
    pub metadata: MediaMetadataPayload,
    pub parser: String,
    pub parser_version: String,
}

/// 存储侧原生媒体信息解析支持（COS CI videoinfo 等）。
#[async_trait]
pub trait NativeMediaMetadataStorageDriver: Send + Sync {
    /// 返回 `None` 表示该驱动当前不支持这个对象、MIME 或 metadata kind。
    async fn get_native_media_metadata(
        &self,
        request: &NativeMediaMetadataRequest,
    ) -> Result<Option<NativeMediaMetadataResult>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativePreviewMode {
    HtmlDocument,
    PdfDocument,
    ImagePage { page: i32 },
}

#[derive(Debug, Clone)]
pub struct NativePreviewRequest {
    pub storage_path: String,
    pub source_file_name: String,
    pub source_mime_type: String,
    pub mode: NativePreviewMode,
    pub expires: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum NativePreviewOpenMode {
    Iframe,
    NewTab,
}

#[derive(Debug, Clone)]
pub struct NativePreviewResult {
    pub url: String,
    pub provider: String,
    pub expires_at: DateTime<Utc>,
    pub cache_key: Option<String>,
    pub version: Option<String>,
    pub open_mode: NativePreviewOpenMode,
}

/// 存储侧外部预览 / 文件解析支持（COS CI、未来的对象存储文档处理等）
#[async_trait]
pub trait NativePreviewStorageDriver: Send + Sync {
    /// 返回 `None` 表示该驱动当前不支持这个对象、MIME 或请求模式。
    async fn create_native_preview(
        &self,
        request: &NativePreviewRequest,
    ) -> Result<Option<NativePreviewResult>>;
}

/// 为所有 StorageDriver 提供 StreamUploadDriver 的默认实现
///
/// 此模块提供基于临时文件的通用实现，供不支持原生流式上传的驱动使用。
pub mod fallback {
    use super::*;
    use crate::errors::AsterError;
    use crate::storage::MapAsterErr;
    use tokio::io::AsyncWriteExt;

    /// 基于临时文件的 put_reader 通用实现
    pub async fn put_reader_with_temp_file<D>(
        driver: &D,
        storage_path: &str,
        mut reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        _size: i64,
    ) -> Result<String>
    where
        D: crate::storage::traits::driver::StorageDriver + ?Sized,
    {
        // 创建临时文件
        let temp_dir = std::env::temp_dir();
        let temp_path = crate::utils::raii::TempFileGuard::new(
            temp_dir.join(format!(
                "aster_put_reader_{}_{}",
                std::process::id(),
                rand::random::<u64>()
            )),
            "put_reader temp file",
        );

        // 流式写入临时文件
        let mut file = tokio::fs::File::create(temp_path.path())
            .await
            .map_aster_err(AsterError::storage_driver_error)?;

        tokio::io::copy(&mut reader, &mut file)
            .await
            .map_aster_err_ctx("write temp file", AsterError::storage_driver_error)?;

        // 确保数据落盘
        file.flush()
            .await
            .map_aster_err(AsterError::storage_driver_error)?;
        drop(file);

        // 使用驱动的 put_file 能力上传（如果驱动实现了 StreamUploadDriver）
        // 否则退化为 put + read file

        if let Some(stream_driver) = driver.as_stream_upload() {
            let temp_path_str = temp_path.path().to_str().ok_or_else(|| {
                AsterError::storage_driver_error("temp upload path is not valid UTF-8")
            })?;
            stream_driver.put_file(storage_path, temp_path_str).await
        } else {
            // 终极 fallback：读文件到内存再 put
            let data = tokio::fs::read(temp_path.path())
                .await
                .map_aster_err(AsterError::storage_driver_error)?;
            driver.put(storage_path, &data).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fallback::put_reader_with_temp_file;
    use crate::errors::Result;
    use crate::storage::traits::driver::{BlobMetadata, StorageDriver};
    use async_trait::async_trait;
    use std::collections::HashSet;
    use std::path::PathBuf;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::io::{AsyncRead, ReadBuf};

    struct NoopDriver;

    #[async_trait]
    impl StorageDriver for NoopDriver {
        async fn put(&self, _path: &str, _data: &[u8]) -> Result<String> {
            unreachable!("put should not be called when temp write fails")
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
    }

    struct FailingReader {
        emitted_chunk: bool,
    }

    impl AsyncRead for FailingReader {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            if !self.emitted_chunk {
                self.emitted_chunk = true;
                buf.put_slice(b"partial");
                Poll::Ready(Ok(()))
            } else {
                Poll::Ready(Err(std::io::Error::other("boom")))
            }
        }
    }

    fn collect_put_reader_temp_files() -> HashSet<PathBuf> {
        let prefix = format!("aster_put_reader_{}_", std::process::id());
        std::fs::read_dir(std::env::temp_dir())
            .expect("temp dir should be readable")
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                let name = path.file_name()?.to_str()?;
                name.starts_with(&prefix).then_some(path)
            })
            .collect()
    }

    #[tokio::test]
    async fn put_reader_with_temp_file_cleans_up_temp_file_on_copy_error() {
        let before = collect_put_reader_temp_files();

        let error = put_reader_with_temp_file(
            &NoopDriver,
            "broken-upload.bin",
            Box::new(FailingReader {
                emitted_chunk: false,
            }),
            7,
        )
        .await
        .expect_err("copy failure should surface as error");

        assert!(error.message().contains("write temp file"));
        assert_eq!(collect_put_reader_temp_files(), before);
    }
}

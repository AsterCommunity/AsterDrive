//! StorageDriver 扩展 trait
//!
//! 将可选能力从核心 StorageDriver 分离，避免每个驱动被迫实现不需要的功能。
//!
//! 判断一项能力放哪儿时，先问一句：它是不是“已配置存储上的运行期对象能力”？
//! 如果是，放在这里并通过 `StorageDriver::as_xxx()` 暴露；如果是管理端字段、
//! OAuth、连接测试、策略动作或前端可见能力声明，应该放到 connector/descriptor。

use crate::errors::Result;
use crate::storage::traits::driver::{PresignedDownloadOptions, StoragePathVisitor};
use crate::types::{MediaMetadataKind, MediaMetadataPayload};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderResumableUploadCapabilities {
    /// Provider 标识，例如 `microsoft_graph`。
    pub provider: &'static str,
    /// 面向日志/诊断的 session 名称，例如 `upload_session`。
    pub session_label: &'static str,
    /// Provider 接受的最小分片大小。
    pub min_fragment_size: usize,
    /// 后端默认使用的分片大小。
    pub default_fragment_size: usize,
    /// Provider 或当前实现允许的最大分片大小。
    pub max_fragment_size: usize,
    /// 分片边界对齐要求。Microsoft Graph 这类 provider 通常有固定对齐规则。
    pub fragment_alignment: usize,
    /// 小文件可绕过 resumable session 的大小上限。
    pub max_simple_upload_size: Option<u64>,
    /// 是否允许浏览器直接拿 provider session 上传。false 表示 session 留在后端内部。
    pub frontend_direct_upload: bool,
}

/// Provider-native resumable upload support.
///
/// 这个 trait 只描述 provider 自己的 resumable/session 上传能力，不给 upload
/// service 暴露“创建 session / 上传 range / complete session”的通用协议。
///
/// 它故意和 S3-compatible multipart 分开：S3 multipart 是 upload service 可直接
/// 编排的对象存储契约；Microsoft Graph 这类 provider 的 upload session 由具体
/// driver 封装，上层通常仍然只通过 `StreamUploadDriver::put_reader()` 写入。
pub trait ProviderResumableUploadDriver: Send + Sync {
    fn provider_resumable_upload_capabilities(&self) -> ProviderResumableUploadCapabilities;
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

/// Presigned URL 支持（S3/R2/OSS/remote follower 等）。
///
/// 这是运行期能力：调用者已经有一个 driver，只是询问它能不能给对象生成临时 URL。
/// 是否在 UI 中显示 presigned 选项，应由 connector descriptor 的 capability 决定。
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

    /// Extra request headers required by a presigned PUT URL.
    ///
    /// S3-compatible providers usually require none. Azure Blob single PUT
    /// requires `x-ms-blob-type: BlockBlob`; the upload init response forwards
    /// these headers to browser clients.
    fn presigned_put_headers(&self) -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    /// Whether browser clients must receive an ETag from a single presigned PUT.
    ///
    /// S3-compatible providers expose ETag by default when CORS is configured
    /// correctly. Azure Blob does not reliably provide a usable ETag to the
    /// browser in this flow, so Azure overrides this to false.
    fn presigned_put_requires_etag(&self) -> bool {
        true
    }
}

/// 路径列举支持（用于后台维护任务）。
///
/// 该能力面向维护/审计任务，不代表用户文件列表 API。用户可见的目录树应走业务
/// 数据库和权限模型，不应直接把底层对象 key 列表暴露出去。
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

/// 流式直传支持（避免本地临时文件）。
///
/// upload service、WebDAV 等上层只依赖这个抽象把 reader 写入对象。具体 driver
/// 可以在内部使用 provider-native session、对象存储 streaming body 或临时文件。
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

/// 本地路径暴露（仅用于把底层文件路径安全交给受控的外部命令）。
///
/// 这个 trait 只适合真正落在本机文件系统上的 driver。远端对象存储不要返回下载后
/// 的临时路径来伪装该能力，否则调用方会误以为可以做零拷贝本地操作。
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

/// 存储侧原生缩略图支持（OneDrive / 数据万象 / 对象存储图片处理等）。
///
/// 返回 `Some` 表示 provider 已经生成可用结果；返回 `None` 表示该对象应回退到
/// AsterDrive 自己的缩略图流水线。
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
///
/// 这表示 provider 能直接解析媒体元数据，不表示所有 MIME / metadata kind 都支持。
/// 不支持当前对象时返回 `None`，让上层回退到本地解析。
#[async_trait]
pub trait NativeMediaMetadataStorageDriver: Send + Sync {
    /// 返回 `None` 表示该驱动当前不支持这个对象、MIME 或 metadata kind。
    async fn get_native_media_metadata(
        &self,
        request: &NativeMediaMetadataRequest,
    ) -> Result<Option<NativeMediaMetadataResult>>;
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

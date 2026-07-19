//! 存储子模块：`driver`。
//!
//! 这一层只描述“一个已经配置好的存储后端如何读写对象”。连接参数如何校验、
//! 管理端表单显示哪些字段、OAuth 怎么授权、连接测试是否需要保存 policy，都不
//! 属于 `StorageDriver`，应放在 `storage::connectors` 和 connector descriptor。

use crate::errors::{AsterError, MapAsterErr, Result};
use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncReadExt};

#[derive(Debug, Clone)]
pub struct BlobMetadata {
    pub size: u64,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PresignedDownloadOptions {
    pub response_cache_control: Option<String>,
    pub response_content_disposition: Option<String>,
    pub response_content_type: Option<String>,
}

pub trait StoragePathVisitor: Send {
    fn visit_path(&mut self, path: String) -> Result<()>;
}

/// 存储驱动核心 trait。
///
/// 设计原则：
/// - 最小接口：仅包含所有存储类型必须实现的基础操作
/// - 默认实现：copy_object 提供基于 get+put 的通用实现，驱动可覆盖优化
/// - 扩展能力：通过 extensions() 返回的可选 trait bundle 暴露，避免强制实现
///
/// 这不是配置层 trait。实现者应该假设 endpoint、bucket、凭据、OAuth token 等
/// 已经由 connector / registry 准备好；这里负责的是对既定存储空间执行对象操作。
#[async_trait]
pub trait StorageDriver: Send + Sync {
    /// 写入文件，返回最终存储路径
    async fn put(&self, path: &str, data: &[u8]) -> Result<String>;

    /// 读取文件全部内容。
    ///
    /// 这个方法只适合缩略图、manifest、探测数据等有明确大小上限的小对象，
    /// 或作为不支持 seek/read 优化的驱动兼容兜底。用户文件传输、复制、预览
    /// 和后台任务处理大对象时应优先使用 `get_stream()` / `get_range()`，避免把
    /// 整个 blob 读入内存。
    async fn get(&self, path: &str) -> Result<Vec<u8>>;

    /// 获取文件流（大文件下载）
    async fn get_stream(&self, path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>>;

    /// 获取文件的指定字节区间（HTTP Range / 视频 seek / 断点续传下载）
    ///
    /// - `offset`: 从文件起始的字节偏移；0 表示从头读
    /// - `length`: `None` 表示读到文件末尾，`Some(n)` 表示最多读 `n` 字节
    ///
    /// 默认实现基于 `get_stream` + 字节丢弃，性能不如原生 Range；
    /// 支持原生 Range 请求的驱动（S3/R2/OSS 等）以及可 seek 的驱动（本地文件）
    /// 应覆盖此方法以避免读完整文件。
    async fn get_range(
        &self,
        path: &str,
        offset: u64,
        length: Option<u64>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        let mut stream = self.get_stream(path).await?;
        if offset > 0 {
            let mut skip = (&mut stream).take(offset);
            tokio::io::copy(&mut skip, &mut tokio::io::sink())
                .await
                .map_aster_err_ctx("skip bytes for range", AsterError::storage_driver_error)?;
        }
        Ok(match length {
            Some(len) => Box::new(stream.take(len)),
            None => stream,
        })
    }

    /// 是否支持高效 Range 读取。
    ///
    /// 默认 `get_range()` 会从完整流里丢弃前缀字节，不能用于大量随机 seek。
    /// 基于本地 seek、HTTP Range 或远端原生 Range 的驱动应覆盖为 `true`。
    fn supports_efficient_range(&self) -> bool {
        false
    }

    /// 删除文件
    async fn delete(&self, path: &str) -> Result<()>;

    /// 文件是否存在
    async fn exists(&self, path: &str) -> Result<bool>;

    /// 获取文件元信息
    async fn metadata(&self, path: &str) -> Result<BlobMetadata>;

    /// 轻量就绪检查。
    ///
    /// 这个方法用于 `/health/ready` 等高频探针路径，只应校验本进程运行时状态
    /// 或本地低成本前置条件。不要在默认实现里进行远端网络 I/O；需要完整写入
    /// 验证的场景应使用管理端的连接测试接口。
    async fn readiness_check(&self) -> Result<()> {
        Ok(())
    }

    /// 同 bucket/存储内复制对象
    ///
    /// 默认实现基于 get + put，性能较慢但通用。
    /// 支持 server-side copy 的驱动（如 S3）应覆盖此方法。
    async fn copy_object(&self, src_path: &str, dest_path: &str) -> Result<String> {
        let data = self.get(src_path).await?;
        self.put(dest_path, &data).await
    }

    // =========================================================================
    // 扩展能力查询（返回 StorageDriverExtensions bundle，不支持的字段为 None）
    //
    // 新能力优先考虑放在独立 extension trait 中，再挂到 extensions() 返回的
    // bundle 上。只有当所有存储后端都必须支持该能力时，才应该把方法直接加到
    // StorageDriver。
    // =========================================================================

    /// 获取该驱动的全部可选运行期能力。
    fn extensions(&self) -> super::extensions::StorageDriverExtensions<'_> {
        super::extensions::StorageDriverExtensions::default()
    }

    /// 获取容量观测信息。
    ///
    /// 不支持容量查询的驱动必须明确返回 `StorageErrorKind::Unsupported`，不要静默
    /// 猜测或 panic。调用方可把该错误转换成用户可见的 `unsupported` 状态。
    async fn capacity_info(&self) -> Result<super::extensions::StorageCapacityInfo> {
        Err(crate::storage::error::storage_driver_error(
            crate::storage::StorageErrorKind::Unsupported,
            "storage driver does not support capacity observability",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tokio::io::AsyncReadExt;

    struct MemoryDriver {
        data: Vec<u8>,
        writes: Mutex<Vec<(String, Vec<u8>)>>,
    }

    impl MemoryDriver {
        fn new(data: &[u8]) -> Self {
            Self {
                data: data.to_vec(),
                writes: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl StorageDriver for MemoryDriver {
        async fn put(&self, path: &str, data: &[u8]) -> Result<String> {
            self.writes
                .lock()
                .expect("writes lock should not be poisoned")
                .push((path.to_string(), data.to_vec()));
            Ok(path.to_string())
        }

        async fn get(&self, _path: &str) -> Result<Vec<u8>> {
            Ok(self.data.clone())
        }

        async fn get_stream(&self, _path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
            Ok(Box::new(std::io::Cursor::new(self.data.clone())))
        }

        async fn delete(&self, _path: &str) -> Result<()> {
            Ok(())
        }

        async fn exists(&self, _path: &str) -> Result<bool> {
            Ok(true)
        }

        async fn metadata(&self, _path: &str) -> Result<BlobMetadata> {
            Ok(BlobMetadata {
                size: self.data.len() as u64,
                content_type: Some("application/octet-stream".to_string()),
            })
        }
    }

    #[tokio::test]
    async fn default_get_range_skips_offset_and_limits_length() {
        let driver = MemoryDriver::new(b"Hello, world!");

        let mut reader = driver.get_range("sample.txt", 7, Some(5)).await.unwrap();
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await.unwrap();

        assert_eq!(bytes, b"world");
    }

    #[tokio::test]
    async fn default_get_range_returns_tail_when_length_is_absent() {
        let driver = MemoryDriver::new(b"Hello, world!");

        let mut reader = driver.get_range("sample.txt", 7, None).await.unwrap();
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await.unwrap();

        assert_eq!(bytes, b"world!");
    }

    #[tokio::test]
    async fn default_copy_object_reads_source_and_writes_destination() {
        let driver = MemoryDriver::new(b"copy body");

        let copied_path = driver
            .copy_object("source.bin", "dest.bin")
            .await
            .expect("copy should succeed");

        assert_eq!(copied_path, "dest.bin");
        assert_eq!(
            *driver
                .writes
                .lock()
                .expect("writes lock should not be poisoned"),
            vec![("dest.bin".to_string(), b"copy body".to_vec())]
        );
    }

    #[test]
    fn default_optional_capabilities_are_absent() {
        let driver = MemoryDriver::new(b"data");

        assert!(driver.extensions().presigned.is_none());
        assert!(driver.extensions().list.is_none());
        assert!(driver.extensions().stream_upload.is_none());
        assert!(driver.extensions().provider_resumable.is_none());
        assert!(driver.extensions().local_path.is_none());
        assert!(driver.extensions().native_thumbnail.is_none());
        assert!(driver.extensions().multipart.is_none());
    }

    #[tokio::test]
    async fn default_capacity_info_returns_unsupported_error() {
        let driver = MemoryDriver::new(b"data");

        let error = driver
            .capacity_info()
            .await
            .expect_err("default capacity observability should be unsupported");

        assert_eq!(
            error.storage_error_kind(),
            Some(crate::storage::StorageErrorKind::Unsupported)
        );
        assert!(
            error
                .message()
                .contains("does not support capacity observability")
        );
    }
}

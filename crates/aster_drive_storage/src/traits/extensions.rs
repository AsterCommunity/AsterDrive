//! StorageDriver 扩展 trait
//!
//! 将可选能力从核心 StorageDriver 分离，避免每个驱动被迫实现不需要的功能。
//!
//! 判断一项能力放哪儿时，先问一句：它是不是“已配置存储上的运行期对象能力”？
//! 如果是，放在这里并通过 `StorageDriver::extensions()` 暴露；如果是管理端字段、
//! OAuth、连接测试、策略动作或前端可见能力声明，应该放到 connector/descriptor。

use crate::error::Result;
use crate::traits::driver::{PresignedDownloadOptions, PresignedUploadRequest, StoragePathVisitor};
use aster_drive_model::types::{MediaMetadataKind, MediaMetadataPayload};
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

/// Optional runtime capabilities exposed by a configured storage driver.
///
/// Decorators forward this bundle as one unit, so adding a capability does not
/// require a matching forwarding method in every decorator.
#[derive(Clone, Copy, Default)]
pub struct StorageDriverExtensions<'a> {
    pub presigned: Option<&'a dyn PresignedStorageDriver>,
    pub list: Option<&'a dyn ListStorageDriver>,
    pub stream_upload: Option<&'a dyn StreamUploadDriver>,
    pub provider_resumable: Option<&'a dyn ProviderResumableUploadDriver>,
    pub local_path: Option<&'a dyn LocalPathStorageDriver>,
    pub native_thumbnail: Option<&'a dyn NativeThumbnailStorageDriver>,
    pub native_media_metadata: Option<&'a dyn NativeMediaMetadataStorageDriver>,
    pub multipart: Option<&'a dyn crate::traits::multipart::MultipartStorageDriver>,
}

/// One product-owned streaming write.
///
/// AsterDrive allocates opaque immutable blob keys before transfer. For
/// provider-atomic drivers that key is already the eventual blob identity;
/// publishing the file/version happens later when the Drive database starts
/// referencing it. Filesystem-like drivers use `staging_path` until commit.
#[derive(Debug, Clone)]
pub struct StreamUploadAttempt {
    pub id: String,
    pub storage_path: String,
    pub staging_path: String,
    pub expected_size: i64,
    provider_session: std::sync::Arc<std::sync::Mutex<Option<String>>>,
}

impl StreamUploadAttempt {
    pub fn new(storage_path: impl Into<String>, expected_size: i64) -> Result<Self> {
        if expected_size < 0 {
            return Err(crate::error::storage_driver_error(
                crate::error::StorageErrorKind::Precondition,
                "stream upload expected size must be non-negative",
            ));
        }
        let storage_path = storage_path.into();
        let id = aster_forge_utils::id::new_uuid();
        let staging_path = storage_path
            .rsplit_once('/')
            .map(|(parent, _)| format!("{parent}/.aster-attempt-{id}.tmp"))
            .unwrap_or_else(|| format!(".aster-attempt-{id}.tmp"));
        Ok(Self {
            id,
            storage_path,
            staging_path,
            expected_size,
            provider_session: std::sync::Arc::new(std::sync::Mutex::new(None)),
        })
    }

    pub fn set_provider_session(&self, session: impl Into<String>) {
        if let Ok(mut value) = self.provider_session.lock() {
            *value = Some(session.into());
        }
    }

    pub fn take_provider_session(&self) -> Option<String> {
        self.provider_session
            .lock()
            .ok()
            .and_then(|mut value| value.take())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamUploadCleanup {
    NotRequired,
    Cleaned,
    Deferred,
    Unknown,
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
    /// Provider 是否在最后一个 range/fragment 接收后隐式完成 upload session。
    pub implicit_completion: bool,
    /// 当前 driver 是否暴露 provider-native session abort 能力给上层。
    pub abort_supported: bool,
    /// 当前 driver 是否暴露 provider-native session status/query 能力给上层。
    pub status_query_supported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderResumableUploadSession {
    pub upload_url: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub next_expected_ranges: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderResumableUploadStatus {
    pub expires_at: Option<DateTime<Utc>>,
    pub next_expected_ranges: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderResumableUploadFragmentOutcome {
    pub completed: bool,
    pub next_expected_ranges: Vec<String>,
}

/// Provider-native resumable upload support.
///
/// 它故意和 S3-compatible multipart 分开：provider resumable 使用顺序 byte range、
/// provider-native progress 和隐式完成语义，不伪装成 numbered multipart parts。
#[async_trait]
pub trait ProviderResumableUploadDriver: Send + Sync {
    fn provider_resumable_upload_capabilities(&self) -> ProviderResumableUploadCapabilities;

    async fn create_upload_session(&self, path: &str) -> Result<ProviderResumableUploadSession>;

    async fn query_upload_session(&self, upload_url: &str)
    -> Result<ProviderResumableUploadStatus>;

    async fn abort_upload_session(&self, upload_url: &str) -> Result<()>;

    async fn upload_session_fragment_reader(
        &self,
        upload_url: &str,
        start: u64,
        total_size: u64,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        fragment_size: i64,
    ) -> Result<ProviderResumableUploadFragmentOutcome>;
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

    /// 生成供客户端直传的完整 presigned PUT 请求。
    ///
    /// URL 和请求头必须由同一个 provider signer 一起产生；调用方不得
    /// 自行补充 provider-specific headers。
    async fn presigned_put_request(
        &self,
        path: &str,
        expires: Duration,
    ) -> Result<Option<PresignedUploadRequest>>;

    /// Whether browser clients must receive an ETag from a single presigned PUT.
    ///
    /// Providers default to requiring ETag. A driver may opt out when the
    /// single-object completion path verifies the final object server-side;
    /// multipart part ETags remain a separate protocol requirement.
    fn presigned_single_put_requires_etag(&self) -> bool {
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
    /// driver 必须保持有界流式读取，并在目标可见前完成大小校验。
    async fn put_reader(
        &self,
        storage_path: &str,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> Result<String>;

    /// Stages one owned attempt.
    ///
    /// The default writes the preallocated opaque final key directly and is
    /// only valid when an incomplete provider request cannot expose a new
    /// object. It deliberately avoids a second full-object provider copy;
    /// unreferenced opaque blobs are recovered by Drive's existing orphan
    /// maintenance after a process-level interruption.
    async fn stage_attempt(
        &self,
        attempt: &StreamUploadAttempt,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
    ) -> Result<()> {
        self.put_reader(&attempt.storage_path, reader, attempt.expected_size)
            .await?;
        Ok(())
    }

    /// Confirms provider completion. Atomic providers already wrote the final
    /// blob identity; filesystem-like drivers override this to publish staging.
    async fn commit_attempt(&self, attempt: &StreamUploadAttempt) -> Result<String> {
        Ok(attempt.storage_path.clone())
    }

    async fn abort_attempt(&self, _attempt: &StreamUploadAttempt) -> Result<StreamUploadCleanup> {
        Ok(StreamUploadCleanup::NotRequired)
    }

    /// 从本地文件路径写入存储（分片上传组装后使用）
    ///
    /// 从本地文件路径写入存储，供需要显式控制临时文件生命周期的调用方使用。
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

#[cfg(test)]
mod tests {
    use super::{StreamUploadAttempt, StreamUploadCleanup, StreamUploadDriver};
    use crate::error::{Result, StorageErrorKind};
    use async_trait::async_trait;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::AsyncRead;
    use tokio::sync::Barrier;

    struct AtomicAttemptDriver {
        writes: std::sync::Mutex<Vec<String>>,
    }

    #[async_trait]
    impl StreamUploadDriver for AtomicAttemptDriver {
        async fn put_reader(
            &self,
            storage_path: &str,
            _reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
            _size: i64,
        ) -> Result<String> {
            self.writes.lock().unwrap().push(storage_path.to_string());
            Ok(storage_path.to_string())
        }

        async fn put_file(&self, _storage_path: &str, _local_path: &str) -> Result<String> {
            unreachable!("atomic attempt test only uses readers")
        }
    }

    struct ParallelAttemptDriver {
        barrier: Barrier,
        active: AtomicUsize,
        max_active: AtomicUsize,
    }

    #[async_trait]
    impl StreamUploadDriver for ParallelAttemptDriver {
        async fn put_reader(
            &self,
            storage_path: &str,
            _reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
            _size: i64,
        ) -> Result<String> {
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active.fetch_max(active, Ordering::SeqCst);
            self.barrier.wait().await;
            self.active.fetch_sub(1, Ordering::SeqCst);
            Ok(storage_path.to_string())
        }

        async fn put_file(&self, _storage_path: &str, _local_path: &str) -> Result<String> {
            unreachable!("parallel attempt test only uses readers")
        }
    }

    #[test]
    fn stream_upload_attempt_uses_unique_staging_namespace() {
        let first = StreamUploadAttempt::new("files/object.bin", 42).unwrap();
        let second = StreamUploadAttempt::new("files/object.bin", 42).unwrap();

        assert_ne!(first.id, second.id);
        assert_ne!(first.staging_path, second.staging_path);
        assert!(first.staging_path.starts_with("files/.aster-attempt-"));
        assert_eq!(first.storage_path, "files/object.bin");
        assert_eq!(first.expected_size, 42);
    }

    #[test]
    fn stream_upload_attempt_rejects_negative_size() {
        let error = StreamUploadAttempt::new("files/object.bin", -1).unwrap_err();

        assert_eq!(error.kind(), StorageErrorKind::Precondition);
    }

    #[tokio::test]
    async fn atomic_attempt_writes_final_key_without_copy_staging() {
        let driver = AtomicAttemptDriver {
            writes: std::sync::Mutex::new(Vec::new()),
        };
        let attempt = StreamUploadAttempt::new("files/opaque-id", 1).unwrap();

        driver
            .stage_attempt(&attempt, Box::new(std::io::Cursor::new(vec![1_u8])))
            .await
            .unwrap();
        assert_eq!(
            driver.commit_attempt(&attempt).await.unwrap(),
            "files/opaque-id"
        );
        assert_eq!(
            driver.abort_attempt(&attempt).await.unwrap(),
            StreamUploadCleanup::NotRequired
        );
        assert_eq!(
            driver.writes.lock().unwrap().as_slice(),
            ["files/opaque-id"]
        );
    }

    #[tokio::test]
    async fn stream_upload_attempts_remain_parallel() {
        let driver = Arc::new(ParallelAttemptDriver {
            barrier: Barrier::new(2),
            active: AtomicUsize::new(0),
            max_active: AtomicUsize::new(0),
        });
        let first = StreamUploadAttempt::new("files/shared.bin", 1).unwrap();
        let second = StreamUploadAttempt::new("files/shared.bin", 1).unwrap();

        let first_driver = Arc::clone(&driver);
        let first_task = tokio::spawn(async move {
            first_driver
                .stage_attempt(&first, Box::new(std::io::Cursor::new(vec![1_u8])))
                .await
        });
        let second_driver = Arc::clone(&driver);
        let second_task = tokio::spawn(async move {
            second_driver
                .stage_attempt(&second, Box::new(std::io::Cursor::new(vec![2_u8])))
                .await
        });

        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            first_task.await.unwrap().unwrap();
            second_task.await.unwrap().unwrap();
        })
        .await
        .expect("independent attempts must not be serialized");
        assert_eq!(driver.max_active.load(Ordering::SeqCst), 2);
    }
}

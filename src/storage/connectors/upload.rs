use crate::entities::storage_policy;
use crate::types::{
    RemoteUploadStrategy, S3UploadStrategy, UploadMode, effective_object_multipart_chunk_size,
};

/// Connector 对 upload service 暴露的上传传输模型。
///
/// 这个 enum 是 upload service 和 storage connector 之间的边界：upload service 通过它
/// 决定 Direct / Chunked / Presigned / Multipart，而不是自己 match `DriverType`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageConnectorUploadTransport {
    /// 本机文件系统写入。
    Local,
    /// S3-compatible / Azure Blob / COS 这类对象存储上传。
    ObjectStorage(S3UploadStrategy),
    /// 通过 remote node 代理上传。
    Remote(RemoteUploadStrategy),
    /// Server-side streaming through `StreamUploadDriver` without exposing a
    /// provider-native browser upload session. OneDrive uses this today: its
    /// driver may create Microsoft Graph upload sessions internally, but the
    /// upload service only sees a generic stream-upload target.
    StreamUpload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageConnectorChunkedCompletion {
    /// Assemble local chunk files into one temp file before storing it.
    AssembleLocalChunks,
    /// Pipe local chunk files directly into `StreamUploadDriver::put_reader`.
    RelayLocalChunksToStreamUpload,
}

impl StorageConnectorUploadTransport {
    /// 返回当前传输模型下实际使用的 chunk size。
    ///
    /// 对象存储 multipart 需要满足 provider 最小 part size，因此会走专门的修正逻辑。
    pub fn effective_chunk_size(self, policy: &storage_policy::Model) -> i64 {
        match self {
            Self::ObjectStorage(_) => effective_object_multipart_chunk_size(policy.chunk_size),
            Self::Local | Self::Remote(_) | Self::StreamUpload => policy.chunk_size,
        }
    }

    /// 根据传输模型和文件大小选择 upload init 返回给客户端的模式。
    ///
    /// 这里只做调度决策；真正创建 multipart upload、presigned URL 或 session 的逻辑在
    /// upload service 后续步骤和具体 driver 中。
    pub fn resolve_init_mode(self, policy: &storage_policy::Model, total_size: i64) -> UploadMode {
        let fits_single_request = self.fits_single_request(policy, total_size);
        match (self, fits_single_request) {
            (Self::ObjectStorage(S3UploadStrategy::Presigned), true)
            | (Self::Remote(RemoteUploadStrategy::Presigned), true) => UploadMode::Presigned,
            (Self::ObjectStorage(S3UploadStrategy::Presigned), false)
            | (Self::Remote(RemoteUploadStrategy::Presigned), false) => {
                UploadMode::PresignedMultipart
            }
            (_, true) => UploadMode::Direct,
            (_, false) => UploadMode::Chunked,
        }
    }

    /// 判断当前请求是否可以走单请求 streaming direct upload。
    ///
    /// 该路径要求客户端声明了正数大小，并且 transport 能把单个 request body 直接转给
    /// 目标 driver/provider。
    pub fn supports_streaming_direct_upload(
        self,
        policy: &storage_policy::Model,
        declared_size: i64,
    ) -> bool {
        if declared_size <= 0 {
            return false;
        }

        match self {
            Self::Local => false,
            Self::ObjectStorage(S3UploadStrategy::RelayStream) => {
                self.fits_single_request(policy, declared_size)
            }
            Self::ObjectStorage(S3UploadStrategy::Presigned) => false,
            Self::Remote(RemoteUploadStrategy::RelayStream)
            | Self::Remote(RemoteUploadStrategy::Presigned) => true,
            Self::StreamUpload => true,
        }
    }

    /// 是否使用 relay multipart progress/session 记录。
    ///
    /// 只有 relay-stream 的对象存储/remote 上传需要这套 tracking；provider stream upload
    /// 走的是普通 chunk 聚合后转发，不把 provider-native session 暴露给 upload service。
    pub fn uses_relay_multipart_tracking(self) -> bool {
        matches!(
            self,
            Self::ObjectStorage(S3UploadStrategy::RelayStream)
                | Self::Remote(RemoteUploadStrategy::RelayStream)
        )
    }

    /// 为 opaque blob hash 生成稳定前缀。
    ///
    /// 这些前缀已经进入持久化数据，改名会影响兼容性。
    pub fn opaque_blob_hash_prefix(self) -> Option<&'static str> {
        match self {
            Self::Local => None,
            // Keep the persisted prefix stable for existing object-storage
            // drivers even though the transport now covers S3-compatible,
            // Azure Blob and COS.
            Self::ObjectStorage(_) => Some("s3"),
            Self::Remote(_) => Some("remote"),
            Self::StreamUpload => Some("provider"),
        }
    }

    /// 决定 chunked upload complete 阶段如何处理已落地的本地 chunk。
    ///
    /// object storage 仍组装成本地文件再进入对象存储路径；remote/provider stream upload
    /// 可以把 chunk 串成 reader 直接喂给 `StreamUploadDriver`，避免再造第二份完整临时文件。
    pub fn chunked_completion(self) -> StorageConnectorChunkedCompletion {
        match self {
            // Remote and provider stream uploads should not assemble a second
            // full temp file. They can relay stored chunks into the connector's
            // stream-upload implementation, which lets the concrete driver own
            // any provider-native resumable/session behavior internally.
            Self::Remote(_) | Self::StreamUpload => {
                StorageConnectorChunkedCompletion::RelayLocalChunksToStreamUpload
            }
            Self::Local | Self::ObjectStorage(_) => {
                StorageConnectorChunkedCompletion::AssembleLocalChunks
            }
        }
    }

    fn fits_single_request(self, policy: &storage_policy::Model, total_size: i64) -> bool {
        let chunk_size = self.effective_chunk_size(policy);
        chunk_size == 0 || total_size <= chunk_size
    }
}

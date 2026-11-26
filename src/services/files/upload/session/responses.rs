//! 上传服务子模块：`responses`。

use serde::Serialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use chrono::{DateTime, Utc};

use aster_drive_model::types::{UploadScheduling, UploadSessionStatus, UploadTransport};
use aster_drive_storage::PresignedUploadRequest;

/// Browser-executor coordination state attached to provider-resumable uploads.
#[derive(Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ProviderResumableUploadResponse {
    /// Next browser instruction round; `None` when the session needs no browser execution.
    pub browser_upload: Option<super::browser::BrowserUploadStep>,
    /// Provider-side upload session expiry; `None` when the provider reports no expiry.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub expires_at: Option<DateTime<Utc>>,
}

/// Response of the upload init (negotiation) call.
#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct InitUploadResponse {
    /// Upload transport the policy selected for this upload.
    pub mode: UploadTransport,
    /// Upload session id for modes that keep server-side state; `None` for stateless
    /// client-direct transports.
    pub upload_id: Option<String>,
    /// Fixed chunk size in bytes the client must split the file into, for chunked modes.
    pub chunk_size: Option<i64>,
    /// Total number of chunks implied by the negotiated chunk size, for chunked modes.
    pub total_chunks: Option<i32>,
    /// 完整的 Presigned PUT 请求（仅 presigned 模式）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presigned_request: Option<PresignedUploadRequest>,
    /// 浏览器直传完成后是否必须从响应读取 ETag。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presigned_require_etag: Option<bool>,
    /// Browser-executor coordination state, present only for provider-resumable modes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_resumable: Option<ProviderResumableUploadResponse>,
    /// Chunk scheduling contract the client must follow; `None` for single-request transports.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upload_scheduling: Option<UploadScheduling>,
}

/// Result of uploading one chunk.
#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ChunkUploadResponse {
    /// Chunks registered as received after this call; a retried chunk does not double-count.
    pub received_count: i32,
    /// Total number of chunks the upload is split into.
    pub total_chunks: i32,
}

/// Progress snapshot of an in-flight upload session.
#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct UploadProgressResponse {
    /// Upload session id.
    pub upload_id: String,
    /// Current lifecycle state of the upload session.
    pub status: UploadSessionStatus,
    /// Chunk count recorded in the session row; may lag the disk state for provider kinds.
    pub received_count: i32,
    /// Zero-based indices of chunks confirmed present, derived per data plane.
    pub chunks_on_disk: Vec<i32>,
    /// Chunk size in bytes the session was initialized with.
    pub chunk_size: i64,
    /// Total number of chunks the upload is split into.
    pub total_chunks: i32,
    /// Target filename recorded at init time.
    pub filename: String,
    /// Browser-executor coordination state, present only for provider-resumable uploads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_resumable: Option<ProviderResumableUploadResponse>,
    /// Chunk scheduling contract the client must follow; `None` for single-request transports.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upload_scheduling: Option<UploadScheduling>,
}

/// One completed multipart part as recorded for recovery.
#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RecoverableUploadPartResponse {
    /// One-based part number within the multipart upload.
    pub part_number: i32,
    /// Provider ETag of the part; empty when the provider reconciles parts at completion.
    pub etag: String,
}

/// Recoverable upload session as listed for the owner, enough to resume on any device.
#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RecoverableUploadSessionResponse {
    /// Upload session id.
    pub upload_id: String,
    /// Upload transport the session was initialized with.
    pub mode: UploadTransport,
    /// Current lifecycle state of the upload session.
    pub status: UploadSessionStatus,
    /// Target filename recorded at init time.
    pub filename: String,
    /// MIME type resolved at init time.
    pub mime_type: String,
    /// Total upload size in bytes as declared at init time.
    pub total_size: i64,
    /// Chunk size in bytes the session was initialized with.
    pub chunk_size: i64,
    /// Total number of chunks the upload is split into.
    pub total_chunks: i32,
    /// Chunk count recorded in the session row; may lag the provider state.
    pub received_count: i32,
    /// Destination folder; `None` places the file in the scope root.
    pub folder_id: Option<i64>,
    /// Zero-based indices of chunks confirmed present, derived per data plane.
    pub chunks_on_disk: Vec<i32>,
    /// Completed multipart parts with their ETags as known locally.
    pub completed_parts: Vec<RecoverableUploadPartResponse>,
    /// Browser-executor coordination state, present only for provider-resumable uploads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_resumable: Option<ProviderResumableUploadResponse>,
    /// Chunk scheduling contract the client must follow; `None` for single-request transports.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upload_scheduling: Option<UploadScheduling>,
    /// Time when the session expires and becomes ineligible for completion or recovery.
    pub expires_at: DateTime<Utc>,
    /// Time of the last state transition, also the assembly lease deadline reference.
    pub updated_at: DateTime<Utc>,
}

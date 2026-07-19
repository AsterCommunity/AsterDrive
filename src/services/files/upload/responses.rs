//! 上传服务子模块：`responses`。

use std::collections::BTreeMap;

use serde::Serialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use chrono::{DateTime, Utc};

use crate::types::{UploadMode, UploadSessionStatus};

#[derive(Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ProviderResumableUploadResponse {
    pub upload_url: String,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub expires_at: Option<DateTime<Utc>>,
    pub next_expected_ranges: Vec<String>,
}

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct InitUploadResponse {
    pub mode: UploadMode,
    pub upload_id: Option<String>,
    pub chunk_size: Option<i64>,
    pub total_chunks: Option<i32>,
    /// Presigned PUT URL（仅 presigned 模式）
    pub presigned_url: Option<String>,
    /// 存储驱动可能要求的 Presigned PUT 请求头。
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub presigned_headers: BTreeMap<String, String>,
    /// 浏览器直传完成后是否必须从响应读取 ETag。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presigned_require_etag: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_resumable: Option<ProviderResumableUploadResponse>,
}

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ChunkUploadResponse {
    pub received_count: i32,
    pub total_chunks: i32,
}

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct UploadProgressResponse {
    pub upload_id: String,
    pub status: UploadSessionStatus,
    pub received_count: i32,
    pub chunks_on_disk: Vec<i32>,
    pub chunk_size: i64,
    pub total_chunks: i32,
    pub filename: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_resumable: Option<ProviderResumableUploadResponse>,
}

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RecoverableUploadPartResponse {
    pub part_number: i32,
    pub etag: String,
}

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RecoverableUploadSessionResponse {
    pub upload_id: String,
    pub mode: UploadMode,
    pub status: UploadSessionStatus,
    pub filename: String,
    pub total_size: i64,
    pub chunk_size: i64,
    pub total_chunks: i32,
    pub received_count: i32,
    pub folder_id: Option<i64>,
    pub chunks_on_disk: Vec<i32>,
    pub completed_parts: Vec<RecoverableUploadPartResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_resumable: Option<ProviderResumableUploadResponse>,
    pub expires_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

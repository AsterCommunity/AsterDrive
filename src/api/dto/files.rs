//! File-related DTOs: mutations, upload, access, and versioning.

use serde::Deserialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::{IntoParams, ToSchema};
use validator::Validate;

// ── Mutations ────────────────────────────────────────────────────────────────

/// Create an empty file.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CreateEmptyRequest {
    #[validate(custom(function = "crate::api::dto::validation::validate_name"))]
    pub name: String,
    pub folder_id: Option<i64>,
}

/// Extract an archive file.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ExtractArchiveRequest {
    pub target_folder_id: Option<i64>,
    pub output_folder_name: Option<String>,
    #[serde(default)]
    pub filename_encoding: crate::types::ArchiveFilenameEncoding,
}

/// Query parameters for archive preview.
#[derive(Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
pub struct ArchivePreviewQuery {
    #[serde(default)]
    pub filename_encoding: crate::types::ArchiveFilenameEncoding,
}

/// Patch (partial update) a file.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PatchFileReq {
    #[validate(custom(function = "crate::api::dto::validation::validate_name"))]
    pub name: Option<String>,
    #[serde(default)]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<i64>)
    )]
    pub folder_id: crate::types::NullablePatch<i64>,
}

/// Lock or unlock a file.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct SetLockReq {
    pub locked: bool,
}

/// Copy a file to a target folder.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CopyFileReq {
    /// Target folder ID (`None` = root directory).
    pub folder_id: Option<i64>,
}

// ── Upload ──────────────────────────────────────────────────────────────────

/// Query parameters for file upload.
#[derive(Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
pub struct FileQuery {
    pub folder_id: Option<i64>,
    pub relative_path: Option<String>,
    pub declared_size: Option<i64>,
}

/// Initialize a chunked upload session.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct InitUploadReq {
    #[validate(custom(function = "crate::api::dto::validation::validate_name"))]
    pub filename: String,
    #[validate(range(min = 0, message = "total_size cannot be negative"))]
    pub total_size: i64,
    pub folder_id: Option<i64>,
    pub relative_path: Option<String>,
    #[validate(custom(function = "crate::api::dto::validation::validate_uuid"))]
    pub frontend_client_id: Option<String>,
}

/// Query parameters for recoverable upload sessions.
#[derive(Deserialize, Validate)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
pub struct UploadSessionsQuery {
    #[validate(custom(function = "crate::api::dto::validation::validate_uuid"))]
    pub frontend_client_id: Option<String>,
}

/// Path parameters for chunk upload.
#[derive(Deserialize)]
pub struct ChunkPath {
    pub upload_id: String,
    pub chunk_number: i32,
}

/// Path parameters for upload session operations (progress / complete / cancel).
#[derive(Deserialize)]
pub struct UploadIdPath {
    pub upload_id: String,
}

/// Complete a chunked upload session.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CompleteUploadReq {
    pub parts: Option<Vec<CompletedPartReq>>,
}

/// A single completed part for multipart completion.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CompletedPartReq {
    pub part_number: i32,
    pub etag: String,
}

/// Request presigned URLs for S3 multipart upload parts.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PresignPartsReq {
    pub part_numbers: Vec<i32>,
}

// ── Access ──────────────────────────────────────────────────────────────────

/// WOPI open file request.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct OpenWopiRequest {
    pub app_key: String,
}

// ── Versions ───────────────────────────────────────────────────────────────

/// Path parameters for file version operations.
#[derive(Deserialize)]
pub struct VersionPath {
    pub id: i64,
    pub version_id: i64,
}

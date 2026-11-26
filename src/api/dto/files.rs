//! File-related DTOs: mutations, upload, access, and versioning.

use aster_forge_api::NullablePatch;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::{IntoParams, ToSchema};
use validator::Validate;

// ── Mutations ────────────────────────────────────────────────────────────────

/// Create an empty file.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CreateEmptyRequest {
    /// File name; validated for characters and length.
    #[validate(custom(function = "crate::api::dto::validation::validate_name"))]
    pub name: String,
    /// Parent folder ID; `None` = root directory.
    pub folder_id: Option<i64>,
    /// Relative path used by clients that create folders in one pass.
    pub relative_path: Option<String>,
}

/// Extract an archive file.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ExtractArchiveRequest {
    /// Folder to extract into; `None` = the archive's parent folder.
    pub target_folder_id: Option<i64>,
    /// Name of the folder that receives the extracted entries; `None` uses
    /// the archive's file name.
    pub output_folder_name: Option<String>,
    /// Decoding strategy for archive entry filenames; defaults to auto
    /// detection.
    #[serde(default)]
    pub filename_encoding: aster_drive_model::types::ArchiveFilenameEncoding,
}

/// Query parameters for archive preview.
#[derive(Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
pub struct ArchivePreviewQuery {
    /// Decoding strategy for archive entry filenames; defaults to auto
    /// detection.
    #[serde(default)]
    pub filename_encoding: aster_drive_model::types::ArchiveFilenameEncoding,
}

/// Query parameters for file content downloads.
#[derive(Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
pub struct DownloadQuery {
    /// Overrides the `Content-Disposition` type (`inline` or `attachment`);
    /// defaults to `attachment`.
    pub disposition: Option<String>,
}

/// Intended use of a file resource; drives disposition and delivery policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum FileResourcePurpose {
    /// In-app preview; served inline with sandboxing for active content.
    Preview,
    /// User-initiated download; served as an attachment.
    Download,
    /// Handoff to an external viewer app or URL-template preview app.
    ExternalViewer,
}

/// How the frontend consumes the resolved resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum FileResourceDeliveryMode {
    /// Fetched by the frontend through a same-origin blob URL.
    BlobUrl,
    /// Fetched as text and injected into the page.
    Text,
    /// Cross-origin URL handed directly to the browser.
    DirectUrl,
    /// Media stream URL for audio/video playback.
    MediaStream,
    /// Session URL rendered inside an iframe.
    IframeSession,
    /// Manifest URL for formats that resolve secondary resources themselves.
    Manifest,
}

/// Which rendition of the blob to resolve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum FileResourceRepresentation {
    /// Server chooses: converts images the browser cannot render when the
    /// request is a blob-URL preview, otherwise serves the original.
    Auto,
    /// Original file content.
    Original,
    /// Converted medium image preview for non-browser-renderable images.
    ImagePreview,
    /// Small thumbnail.
    Thumbnail,
}

/// Whether the browser sends credentials when fetching the resource URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum FileResourceCredentials {
    /// Send same-origin cookies/credentials with the request.
    Include,
    /// Request the URL without credentials (e.g. presigned CDN URLs).
    Omit,
}

/// Whether conditional request headers (`If-None-Match` etc.) may be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum FileResourceConditionalHeaders {
    /// The URL accepts `If-None-Match`/`If-Modified-Since` revalidation.
    Allowed,
    /// The URL must not receive conditional headers (e.g. presigned URLs,
    /// where such headers would break the provider signature).
    Forbidden,
}

/// Whether the resource URL may redirect across origins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum FileResourceRedirectPolicy {
    /// The browser must not follow redirects to another origin.
    SameOriginOnly,
    /// Redirects to other origins (e.g. storage providers) are acceptable.
    MayCrossOrigin,
}

/// Desired purpose, delivery mode, and representation for a file resource.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct FileResourceHandleRequest {
    /// What the content will be used for.
    pub purpose: FileResourcePurpose,
    /// How the frontend will consume the resolved URL.
    pub delivery_mode: FileResourceDeliveryMode,
    /// Which rendition to resolve; defaults to auto selection.
    #[serde(default = "default_file_resource_representation")]
    pub representation: FileResourceRepresentation,
}

fn default_file_resource_representation() -> FileResourceRepresentation {
    FileResourceRepresentation::Auto
}

/// Identity metadata used to cache the resolved resource.
#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct FileResourceIdentity {
    /// Cache key identifying the resource path this handle resolves.
    pub cache_key: String,
    /// Quoted ETag of the resolved content.
    pub etag: Option<String>,
    /// Workspace scope label (`personal` or `team`) the handle was resolved in.
    pub scope: Option<String>,
}

/// Fetch instructions for the resolved resource.
#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct FileResourceRequestInfo {
    /// URL the frontend should fetch the content from.
    pub url: String,
    /// Credential requirements when fetching `url`.
    pub credentials: FileResourceCredentials,
    /// Whether conditional request headers may be sent to `url`.
    pub conditional_headers: FileResourceConditionalHeaders,
    /// Whether redirects from `url` may cross origins.
    pub redirect_policy: FileResourceRedirectPolicy,
}

/// Delivery metadata for the resolved resource.
#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct FileResourceDeliveryInfo {
    /// Delivery mode the handle was resolved for.
    pub mode: FileResourceDeliveryMode,
    /// MIME type of the served content.
    pub mime_type: Option<String>,
}

/// Lifespan metadata for resources backed by expiring URLs.
#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct FileResourceLifecycleInfo {
    /// Time when the resolved URL stops being valid (e.g. presigned expiry).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// Resolved access instructions for a file resource.
#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct FileResourceHandle {
    /// Cache/identity metadata of the resolved resource.
    pub identity: FileResourceIdentity,
    /// Fetch instructions for the frontend.
    pub request: FileResourceRequestInfo,
    /// Delivery metadata for the resolved content.
    pub delivery: FileResourceDeliveryInfo,
    /// Present when the resolved URL expires (e.g. presigned downloads).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<FileResourceLifecycleInfo>,
}

/// Patch (partial update) a file.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PatchFileReq {
    /// New file name; `None` leaves the current name unchanged.
    #[validate(custom(function = "crate::api::dto::validation::validate_name"))]
    pub name: Option<String>,
    /// Move target: absent keeps the current folder, `null` moves the file
    /// to the root directory, a value moves it into that folder.
    #[serde(default)]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<i64>)
    )]
    pub folder_id: NullablePatch<i64>,
}

/// Lock or unlock a file.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct SetLockReq {
    /// `true` locks the file against metadata changes, `false` unlocks it.
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
    /// Folder to upload into; `None` = root directory.
    pub folder_id: Option<i64>,
    /// Relative path used by clients that upload folders in one pass.
    pub relative_path: Option<String>,
    /// Client-declared total upload size in bytes, used for validation.
    pub declared_size: Option<i64>,
}

/// Initialize a chunked upload session.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct InitUploadReq {
    /// Name of the file being uploaded.
    #[validate(custom(function = "crate::api::dto::validation::validate_name"))]
    pub filename: String,
    /// Total file size in bytes; must not be negative.
    #[validate(range(min = 0, message = "total_size cannot be negative"))]
    pub total_size: i64,
    /// Client-provided MIME type; the server falls back to filename-based
    /// detection when omitted.
    #[validate(custom(function = "crate::api::dto::validation::validate_upload_mime_type"))]
    pub mime_type: Option<String>,
    /// Folder to upload into; `None` = root directory.
    pub folder_id: Option<i64>,
    /// Relative path used by clients that upload folders in one pass.
    pub relative_path: Option<String>,
    /// Client-generated UUID identifying this browser session, used to list
    /// recoverable upload sessions.
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
    /// Restrict results to sessions started with this client UUID.
    #[validate(custom(function = "crate::api::dto::validation::validate_uuid"))]
    pub frontend_client_id: Option<String>,
}

/// Path parameters for chunk upload.
#[derive(Deserialize)]
pub struct ChunkPath {
    /// Upload session ID.
    pub upload_id: String,
    /// Zero-based index of the chunk within the session.
    pub chunk_number: i32,
}

/// Path parameters for upload session operations (progress / complete / cancel).
#[derive(Deserialize)]
pub struct UploadIdPath {
    /// Upload session ID.
    pub upload_id: String,
}

/// Complete a chunked upload session.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CompleteUploadReq {
    /// Completed multipart parts with their ETags; required for presigned
    /// multipart sessions, omitted for sessions the server tracked itself.
    pub parts: Option<Vec<CompletedPartReq>>,
}

/// A single completed part for multipart completion.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CompletedPartReq {
    /// Part number the part was uploaded under.
    pub part_number: i32,
    /// ETag the storage provider returned for the uploaded part.
    pub etag: String,
}

/// Request browser descriptors for signed or provider-native numbered parts.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PresignPartsReq {
    /// Part numbers to issue presigned upload URLs for.
    pub part_numbers: Vec<i32>,
}

// ── Access ──────────────────────────────────────────────────────────────────

/// WOPI open file request.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct OpenWopiRequest {
    /// Key identifying the WOPI client app to launch.
    pub app_key: String,
}

// ── Versions ───────────────────────────────────────────────────────────────

/// Path parameters for file version operations.
#[derive(Deserialize)]
pub struct VersionPath {
    /// File ID.
    pub id: i64,
    /// File version ID.
    pub version_id: i64,
}

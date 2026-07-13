//! 后台任务服务子模块：`types`。

use aster_forge_tasks::TaskStepInfo;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::config::operations;
use crate::services::user::account;
use crate::storage::connectors::{
    StoragePolicyCleanupDriverSnapshot, StoragePolicyCleanupOneDriveCredentialSnapshot,
    StoragePolicyCleanupRemoteNodeSnapshot,
};
use crate::types::{ArchiveFilenameEncoding, BackgroundTaskKind, BackgroundTaskStatus, DriverType};

use super::runtime::SystemRuntimeTaskKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TaskPresentationCode {
    BlobMaintenanceIntegrityCheckName,
    BlobMaintenanceOrphanCleanupName,
    BlobMaintenanceRefCountReconcileName,
    RuntimeSystemHealthIssueDetail,
    RuntimeTaskAuditCleanup,
    RuntimeTaskAuthSessionCleanup,
    RuntimeTaskBackgroundTaskDispatch,
    RuntimeTaskBlobReconcile,
    RuntimeTaskCompletedUploadCleanup,
    RuntimeTaskExternalAuthFlowCleanup,
    RuntimeTaskLockCleanup,
    RuntimeTaskMailOutboxDispatch,
    RuntimeTaskMfaFlowCleanup,
    RuntimeTaskRemoteNodeHealthTest,
    RuntimeTaskSystemHealthCheck,
    RuntimeTaskTaskCleanup,
    RuntimeTaskTeamArchiveCleanup,
    RuntimeTaskTrashCleanup,
    RuntimeTaskUploadCleanup,
    RuntimeTaskWopiSessionCleanup,
    StatusTextArchiveExtracted,
    StatusTextArchivePreviewReady,
    StatusTextArchiveReady,
    StatusTextBlobMaintenanceFinished,
    StatusTextImagePreviewAlreadyAvailable,
    StatusTextImagePreviewReady,
    StatusTextMediaMetadataFailed,
    StatusTextMediaMetadataReady,
    StatusTextMediaMetadataUnsupported,
    StatusTextOfflineDownloadImported,
    StatusTextOfflineDownloadDownloaded,
    StatusTextOfflineDownloadVerified,
    StatusTextStorageMigrationCompleted,
    StatusTextSystemHealthy,
    StatusTextTemporaryUploadCleanupFinished,
    StatusTextThumbnailAlreadyAvailable,
    StatusTextThumbnailReady,
    StatusTextTrashPurged,
    StatusTextWaitingPresignedUrlExpiry,
    TaskNameArchiveCompress,
    TaskNameArchiveExtract,
    TaskNameArchivePreviewGenerate,
    TaskNameArchivePreviewGenerateFileId,
    TaskNameImagePreviewGenerate,
    TaskNameImagePreviewGenerateBlobWithProcessor,
    TaskNameMediaMetadataExtractBlob,
    TaskNameMediaMetadataExtractSource,
    TaskNameOfflineDownloadSource,
    TaskNameOfflineDownloadSourceWithEngine,
    TaskNameOfflineDownloadTargetFolder,
    TaskNameOfflineDownloadTargetFolderWithEngine,
    TaskNameOfflineDownloadUrl,
    TaskNameOfflineDownloadUrlWithEngine,
    TaskNameStoragePolicyMigration,
    TaskNameStoragePolicyTempCleanup,
    TaskNameStoragePolicyTempCleanupPolicyId,
    TaskNameThumbnailGenerate,
    TaskNameThumbnailGenerateBlobWithProcessor,
    TaskNameTrashPurgeAll,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TaskPresentationMessage {
    pub code: TaskPresentationCode,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TaskPresentation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<TaskPresentationMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<TaskPresentationMessage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateArchiveTaskParams {
    pub file_ids: Vec<i64>,
    pub folder_ids: Vec<i64>,
    pub archive_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CreateArchiveCompressTaskParams {
    #[serde(default)]
    pub file_ids: Vec<i64>,
    #[serde(default)]
    pub folder_ids: Vec<i64>,
    pub archive_name: Option<String>,
    pub target_folder_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CreateArchiveExtractTaskParams {
    pub target_folder_id: Option<i64>,
    pub output_folder_name: Option<String>,
    pub filename_encoding: ArchiveFilenameEncoding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ArchiveCompressTaskPayload {
    pub file_ids: Vec<i64>,
    pub folder_ids: Vec<i64>,
    pub archive_name: String,
    pub target_folder_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ArchiveExtractTaskPayload {
    pub file_id: i64,
    pub source_file_name: String,
    pub target_folder_id: Option<i64>,
    pub output_folder_name: String,
    #[serde(default)]
    pub filename_encoding: ArchiveFilenameEncoding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ArchivePreviewTaskPayload {
    pub file_id: i64,
    pub source_file_name: String,
    pub source_blob_id: i64,
    pub source_hash: String,
    pub limit_signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ArchiveCompressTaskResult {
    pub target_file_id: i64,
    pub target_file_name: String,
    pub target_folder_id: Option<i64>,
    pub target_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ArchiveExtractTaskResult {
    pub target_folder_id: i64,
    pub target_folder_name: String,
    pub target_path: String,
    pub extracted_file_count: i64,
    pub extracted_folder_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ArchivePreviewTaskResult {
    pub file_id: i64,
    pub source_blob_id: i64,
    pub source_hash: String,
    pub entry_count: i64,
    pub file_count: i64,
    pub directory_count: i64,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeTaskName {
    Known(SystemRuntimeTaskKind),
    Legacy(String),
}

impl RuntimeTaskName {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Known(kind) => kind.as_str(),
            Self::Legacy(value) => value.as_str(),
        }
    }

    pub fn display_name(&self) -> String {
        match self {
            Self::Known(kind) => kind.display_name().to_string(),
            Self::Legacy(value) => value.replace('-', " "),
        }
    }

    pub fn known(&self) -> Option<SystemRuntimeTaskKind> {
        match self {
            Self::Known(kind) => Some(*kind),
            Self::Legacy(_) => None,
        }
    }
}

impl From<SystemRuntimeTaskKind> for RuntimeTaskName {
    fn from(value: SystemRuntimeTaskKind) -> Self {
        Self::Known(value)
    }
}

impl From<String> for RuntimeTaskName {
    fn from(value: String) -> Self {
        SystemRuntimeTaskKind::from_wire_value(&value)
            .map(Self::Known)
            .unwrap_or(Self::Legacy(value))
    }
}

impl From<&str> for RuntimeTaskName {
    fn from(value: &str) -> Self {
        Self::from(value.to_string())
    }
}

impl std::fmt::Display for RuntimeTaskName {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for RuntimeTaskName {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RuntimeTaskName {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self::from)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RuntimeTaskPayload {
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub task_name: RuntimeTaskName,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum RuntimeSystemHealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

impl From<aster_forge_runtime::HealthStatus> for RuntimeSystemHealthStatus {
    fn from(value: aster_forge_runtime::HealthStatus) -> Self {
        match value {
            aster_forge_runtime::HealthStatus::Healthy => Self::Healthy,
            aster_forge_runtime::HealthStatus::Degraded => Self::Degraded,
            aster_forge_runtime::HealthStatus::Unhealthy => Self::Unhealthy,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RuntimeSystemHealthComponent {
    pub name: String,
    pub status: RuntimeSystemHealthStatus,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<aster_forge_runtime::HealthComponentDetail>,
}

impl From<&aster_forge_runtime::HealthComponentReport> for RuntimeSystemHealthComponent {
    fn from(value: &aster_forge_runtime::HealthComponentReport) -> Self {
        Self {
            name: value.name.to_string(),
            status: value.status.into(),
            message: value.message.clone(),
            details: value.details.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RuntimeSystemHealthResult {
    pub status: RuntimeSystemHealthStatus,
    pub components: Vec<RuntimeSystemHealthComponent>,
}

impl From<&aster_forge_runtime::SystemHealthReport> for RuntimeSystemHealthResult {
    fn from(value: &aster_forge_runtime::SystemHealthReport) -> Self {
        Self {
            status: value.status().into(),
            components: value.components.iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ThumbnailGenerateTaskPayload {
    pub blob_id: i64,
    pub blob_hash: String,
    #[serde(default)]
    pub source_file_name: String,
    pub source_mime_type: String,
    pub processor: crate::types::MediaProcessorKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ImagePreviewGenerateTaskPayload {
    pub blob_id: i64,
    pub blob_hash: String,
    #[serde(default)]
    pub source_file_name: String,
    pub source_mime_type: String,
    pub processor: crate::types::MediaProcessorKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TrashPurgeAllTaskPayload {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TrashPurgeAllTaskResult {
    pub purged: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StoragePolicyCleanupPolicySnapshot {
    pub id: i64,
    pub name: String,
    pub driver_type: DriverType,
    pub endpoint: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub base_path: String,
    pub remote_node_id: Option<i64>,
    pub remote_storage_target_key: Option<String>,
    pub max_file_size: i64,
    pub allowed_types: String,
    pub options: String,
    pub is_default: bool,
    pub chunk_size: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyTempCleanupTarget {
    pub temp_key: String,
    pub multipart_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StoragePolicyTempCleanupTaskPayload {
    pub policy: StoragePolicyCleanupPolicySnapshot,
    #[serde(default)]
    pub driver_snapshot: Option<StoragePolicyCleanupDriverSnapshot>,
    /// Deprecated legacy OneDrive cleanup snapshot. New tasks store this under
    /// `driver_snapshot`; keep this field so queued pre-migration tasks decode.
    #[serde(default)]
    pub onedrive_credential: Option<StoragePolicyCleanupOneDriveCredentialSnapshot>,
    /// Deprecated legacy remote cleanup snapshot. New tasks store this under
    /// `driver_snapshot`; keep this field so queued pre-migration tasks decode.
    #[serde(default)]
    pub remote_node: Option<StoragePolicyCleanupRemoteNodeSnapshot>,
    pub temp_keys: Vec<String>,
    pub multipart_uploads: Vec<StoragePolicyTempCleanupTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyTempCleanupTaskPayloadInfo {
    pub policy_id: i64,
    pub policy_name: String,
    pub driver_type: DriverType,
    pub temp_key_count: usize,
    pub multipart_upload_count: usize,
}

impl From<StoragePolicyTempCleanupTaskPayload> for StoragePolicyTempCleanupTaskPayloadInfo {
    fn from(value: StoragePolicyTempCleanupTaskPayload) -> Self {
        Self {
            policy_id: value.policy.id,
            policy_name: value.policy.name,
            driver_type: value.policy.driver_type,
            temp_key_count: value.temp_keys.len(),
            multipart_upload_count: value.multipart_uploads.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_onedrive_cleanup_snapshot_decodes_without_refresh_fields() {
        let value = serde_json::json!({
            "cloud": "global",
            "drive_id": "drive",
            "root_item_id": "root",
            "access_token_ciphertext": "access",
        });

        let snapshot: StoragePolicyCleanupOneDriveCredentialSnapshot =
            serde_json::from_value(value).expect("legacy snapshot should decode");

        assert_eq!(snapshot.tenant_id, None);
        assert_eq!(snapshot.client_id, None);
        assert_eq!(snapshot.client_secret_ciphertext, None);
        assert_eq!(snapshot.refresh_token_ciphertext, None);
        assert_eq!(snapshot.expires_at, None);
    }

    #[test]
    fn onedrive_cleanup_snapshot_serializes_refresh_fields_for_new_tasks() {
        let expires_at = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let snapshot = StoragePolicyCleanupOneDriveCredentialSnapshot {
            cloud: crate::types::MicrosoftGraphCloud::Global,
            tenant_id: Some("tenant".to_string()),
            client_id: Some("client-id".to_string()),
            client_secret_ciphertext: Some("client-secret-ciphertext".to_string()),
            drive_id: "drive".to_string(),
            root_item_id: "root".to_string(),
            access_token_ciphertext: "access".to_string(),
            refresh_token_ciphertext: Some("refresh".to_string()),
            expires_at: Some(expires_at),
        };

        let value = serde_json::to_value(snapshot).expect("snapshot should serialize");

        assert_eq!(value["tenant_id"], "tenant");
        assert_eq!(value["client_id"], "client-id");
        assert_eq!(
            value["client_secret_ciphertext"],
            "client-secret-ciphertext"
        );
        assert_eq!(value["refresh_token_ciphertext"], "refresh");
        assert!(value.get("expires_at").is_some());
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RuntimeTaskResult {
    pub duration_ms: i64,
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_health: Option<RuntimeSystemHealthResult>,
}

impl RuntimeTaskResult {
    pub fn from_timestamps(
        started_at: chrono::DateTime<chrono::Utc>,
        finished_at: chrono::DateTime<chrono::Utc>,
        summary: Option<String>,
        system_health: Option<RuntimeSystemHealthResult>,
    ) -> Self {
        Self {
            duration_ms: (finished_at - started_at).num_milliseconds().max(0),
            summary,
            system_health,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ThumbnailGenerateTaskResult {
    pub blob_id: i64,
    pub thumbnail_path: String,
    pub thumbnail_processor: String,
    pub thumbnail_version: String,
    pub processor: crate::types::MediaProcessorKind,
    pub reused_existing_thumbnail: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ImagePreviewGenerateTaskResult {
    pub blob_id: i64,
    pub image_preview_path: String,
    pub image_preview_processor: String,
    pub image_preview_version: String,
    pub processor: crate::types::MediaProcessorKind,
    pub reused_existing_preview: bool,
}

pub use crate::services::media::metadata::{
    MediaMetadataExtractTaskPayload, MediaMetadataExtractTaskResult,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyTempCleanupTaskResult {
    pub deleted_objects: u64,
    pub missing_objects: u64,
    pub failed_objects: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyMigrationTaskPayload {
    pub source_policy_id: i64,
    pub target_policy_id: i64,
    pub delete_source_after_success: bool,
    pub plan_hash: String,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub source_policy_updated_at: chrono::DateTime<chrono::Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub target_policy_updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyMigrationTaskResult {
    pub source_policy_id: i64,
    pub target_policy_id: i64,
    pub scanned_blobs: i64,
    pub migrated_blobs: i64,
    pub merged_blobs: i64,
    pub skipped_blobs: i64,
    pub failed_blobs: i64,
    pub migrated_bytes: i64,
    #[serde(default)]
    pub renamed_opaque_blobs: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum BlobMaintenanceAction {
    IntegrityCheck,
    RefCountReconcile,
    OrphanCleanup,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct BlobMaintenanceTaskPayload {
    pub action: BlobMaintenanceAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob_ids: Option<Vec<i64>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct BlobMaintenanceTaskResult {
    pub action: BlobMaintenanceAction,
    pub scanned_blobs: i64,
    pub checked_objects: i64,
    pub missing_objects: i64,
    pub size_mismatches: i64,
    pub ref_counts_fixed: i64,
    pub orphan_blobs_deleted: i64,
    pub skipped_blobs: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
/// Parameters for creating an offline download task.
pub struct CreateOfflineDownloadTaskParams {
    /// Source URL validated by `parse_and_validate_source_url`; only `http://`
    /// and `https://` URLs with a host are accepted. Local or executable
    /// schemes such as `file://`, `javascript:`, and `data:` are rejected.
    /// Credentials in the URL userinfo component are rejected so they are not
    /// persisted in task payloads; query parameters are accepted, but display
    /// paths are derived through `redact_url_for_display`.
    pub url: String,
    /// Optional target filename. Empty values are ignored; non-empty values
    /// must pass `normalize_validate_name`, so path separators and unsafe
    /// Windows device names are rejected.
    pub filename: Option<String>,
    /// Optional destination folder. `None` imports into the workspace root.
    pub target_folder_id: Option<i64>,
    /// Optional expected SHA-256 checksum. Values are trimmed, lowercased, and
    /// must be a 64-character hexadecimal string without a `0x` prefix, for
    /// example `0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef`.
    pub expected_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
/// Presentation-safe offline download payload used by task list views.
///
/// `OfflineDownloadTaskPayloadInfo` is derived from `OfflineDownloadTaskPayload`;
/// when `source_display_url` is absent, callers use `redact_url_for_display`
/// with `unwrap_or_else` fallback behavior so legacy payloads do not panic.
pub struct OfflineDownloadTaskPayloadInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_folder_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_sha256: Option<String>,
    pub source_display_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
/// Stored payload for an offline download task.
pub struct OfflineDownloadTaskPayload {
    /// Original source URL. It is validated by `parse_and_validate_source_url`
    /// before task creation and must be an HTTP/HTTPS URL with a host. It is
    /// intentionally persisted for retry execution and administrator
    /// investigations; public task views receive `OfflineDownloadTaskPayloadInfo`
    /// with only `source_display_url`.
    pub url: String,
    /// Optional normalized target filename.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// Optional destination folder ID; absent means workspace root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_folder_id: Option<i64>,
    /// Optional normalized checksum. It is trimmed, lowercased, and validated as
    /// a 64-character hex string; validation errors use
    /// "expected_sha256 must be a 64-character hex string", and comparisons use
    /// this normalized string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_sha256: Option<String>,
    /// Redacted display URL. When `None`, `OfflineDownloadTaskPayloadInfo` and
    /// `OfflineDownloadTaskResult` derive a safe value from `url` with
    /// `redact_url_for_display` and fallback text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_display_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
/// Result payload for offline downloads.
pub struct OfflineDownloadTaskResult {
    pub file_id: i64,
    pub file_name: String,
    pub folder_id: Option<i64>,
    pub file_path: String,
    /// Redacted display URL derived from the payload or source URL. This value
    /// is always safe to render in task detail views.
    pub source_display_url: String,
    /// Final imported content length in bytes.
    pub content_length: i64,
    /// Final SHA-256 digest in lowercase hexadecimal.
    pub sha256: String,
    /// Actual engine used for the final successful transfer. Legacy task
    /// results may not have this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub download_engine: Option<operations::OfflineDownloadEngine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum StoragePolicyMigrationCapacityCheck {
    Sufficient,
    Insufficient,
    Unsupported,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum StoragePolicyMigrationDryRunWarning {
    TargetCapacityUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyMigrationDryRun {
    pub source_policy_id: i64,
    pub target_policy_id: i64,
    pub source_blob_count: i64,
    pub source_total_bytes: i64,
    pub content_sha256_blob_count: i64,
    pub opaque_blob_count: i64,
    pub target_matching_blob_count: i64,
    pub estimated_copy_blob_count: i64,
    pub opaque_key_conflict_count: i64,
    pub target_supports_stream_upload: bool,
    pub target_connection_ok: bool,
    pub target_capacity_check: StoragePolicyMigrationCapacityCheck,
    pub target_capacity: crate::storage::StorageCapacityInfo,
    pub delete_source_after_success_supported: bool,
    pub can_start: bool,
    pub warnings: Vec<StoragePolicyMigrationDryRunWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskPayload {
    ArchiveCompress(ArchiveCompressTaskPayload),
    ArchiveExtract(ArchiveExtractTaskPayload),
    ArchivePreviewGenerate(ArchivePreviewTaskPayload),
    ThumbnailGenerate(ThumbnailGenerateTaskPayload),
    ImagePreviewGenerate(ImagePreviewGenerateTaskPayload),
    MediaMetadataExtract(MediaMetadataExtractTaskPayload),
    TrashPurgeAll(TrashPurgeAllTaskPayload),
    StoragePolicyTempCleanup(StoragePolicyTempCleanupTaskPayloadInfo),
    StoragePolicyMigration(StoragePolicyMigrationTaskPayload),
    BlobMaintenance(BlobMaintenanceTaskPayload),
    OfflineDownload(OfflineDownloadTaskPayloadInfo),
    SystemRuntime(RuntimeTaskPayload),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskResult {
    ArchiveCompress(ArchiveCompressTaskResult),
    ArchiveExtract(ArchiveExtractTaskResult),
    ArchivePreviewGenerate(ArchivePreviewTaskResult),
    ThumbnailGenerate(ThumbnailGenerateTaskResult),
    ImagePreviewGenerate(ImagePreviewGenerateTaskResult),
    MediaMetadataExtract(MediaMetadataExtractTaskResult),
    TrashPurgeAll(TrashPurgeAllTaskResult),
    StoragePolicyTempCleanup(StoragePolicyTempCleanupTaskResult),
    StoragePolicyMigration(StoragePolicyMigrationTaskResult),
    BlobMaintenance(BlobMaintenanceTaskResult),
    OfflineDownload(OfflineDownloadTaskResult),
    SystemRuntime(RuntimeTaskResult),
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TaskInfo {
    pub id: i64,
    pub kind: BackgroundTaskKind,
    pub status: BackgroundTaskStatus,
    pub display_name: String,
    pub creator: Option<account::UserSummary>,
    pub team_id: Option<i64>,
    pub share_id: Option<i64>,
    pub progress_current: i64,
    pub progress_total: i64,
    pub progress_percent: i32,
    pub status_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presentation: Option<TaskPresentation>,
    pub attempt_count: i32,
    pub max_attempts: i32,
    pub last_error: Option<String>,
    pub payload: TaskPayload,
    pub result: Option<TaskResult>,
    pub steps: Vec<TaskStepInfo>,
    pub can_retry: bool,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub lease_expires_at: Option<chrono::DateTime<chrono::Utc>>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: chrono::DateTime<chrono::Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

//! 后台任务服务子模块：`types`。

use aster_forge_tasks::TaskStepInfo;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::config::operations;
use crate::services::user::account;
use crate::storage::connectors::StoragePolicyCleanupDriverSnapshot;
use aster_drive_model::types::{ArchiveFilenameEncoding, BackgroundTaskKind, BackgroundTaskStatus};

use super::runtime::SystemRuntimeTaskKind;

/// Stable wire codes for background-task presentation metadata.
///
/// Clients map each code to localized task titles and status texts; the
/// serialized values are part of the API surface and must stay stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TaskPresentationCode {
    /// Title code for the blob integrity-check maintenance task.
    BlobMaintenanceIntegrityCheckName,
    /// Title code for the blob orphan-cleanup maintenance task.
    BlobMaintenanceOrphanCleanupName,
    /// Title code for the blob reference-count reconciliation maintenance task.
    BlobMaintenanceRefCountReconcileName,
    /// Status detail code carrying the overall status and failing components
    /// of a system health check.
    RuntimeSystemHealthIssueDetail,
    /// Title code for the audit-log and workspace-activity cleanup system task.
    RuntimeTaskAuditCleanup,
    /// Title code for the expired local authentication session cleanup system task.
    RuntimeTaskAuthSessionCleanup,
    /// Title code for the background task dispatch cycle.
    RuntimeTaskBackgroundTaskDispatch,
    /// Title code for the blob reference-count reconciliation system task.
    RuntimeTaskBlobReconcile,
    /// Title code for the expired completed-upload purge system task.
    RuntimeTaskCompletedUploadCleanup,
    /// Title code for the expired external authentication flow cleanup system task.
    RuntimeTaskExternalAuthFlowCleanup,
    /// Title code for the expired file lock cleanup system task.
    RuntimeTaskLockCleanup,
    /// Title code for the mail outbox dispatch cycle.
    RuntimeTaskMailOutboxDispatch,
    /// Title code for the expired multi-factor authentication flow cleanup system task.
    RuntimeTaskMfaFlowCleanup,
    /// Title code for the remote node health probe system task.
    RuntimeTaskRemoteNodeHealthTest,
    /// Title code for the system health check cycle.
    RuntimeTaskSystemHealthCheck,
    /// Title code for the expired background task artifact cleanup system task.
    RuntimeTaskTaskCleanup,
    /// Title code for the archived team cleanup system task.
    RuntimeTaskTeamArchiveCleanup,
    /// Title code for the expired trash retention cleanup system task.
    RuntimeTaskTrashCleanup,
    /// Title code for the expired upload session cleanup system task.
    RuntimeTaskUploadCleanup,
    /// Title code for the expired WOPI session cleanup system task.
    RuntimeTaskWopiSessionCleanup,
    /// Status text code for a completed archive extraction.
    StatusTextArchiveExtracted,
    /// Status text code for a generated archive preview.
    StatusTextArchivePreviewReady,
    /// Status text code for a completed archive file.
    StatusTextArchiveReady,
    /// Status text code for a finished blob maintenance run.
    StatusTextBlobMaintenanceFinished,
    /// Status text code for a completed folder-tree delete.
    StatusTextFolderTreeDeleteFinished,
    /// Status text code shown while a folder-tree mutation is still scanning.
    StatusTextFolderTreeMutationScanning,
    /// Status text code for a completed folder-tree restore.
    StatusTextFolderTreeRestoreFinished,
    /// Status text code when an up-to-date image preview already exists and is reused.
    StatusTextImagePreviewAlreadyAvailable,
    /// Status text code for a newly generated image preview.
    StatusTextImagePreviewReady,
    /// Status text code for failed media metadata extraction.
    StatusTextMediaMetadataFailed,
    /// Status text code for successfully extracted media metadata.
    StatusTextMediaMetadataReady,
    /// Status text code when the media type has no extractable metadata.
    StatusTextMediaMetadataUnsupported,
    /// Status text code for an offline download imported into the workspace.
    StatusTextOfflineDownloadImported,
    /// Status text code for an offline download whose transfer finished.
    StatusTextOfflineDownloadDownloaded,
    /// Status text code for an offline download whose checksum verification passed.
    StatusTextOfflineDownloadVerified,
    /// Status text code for a finished storage policy migration.
    StatusTextStorageMigrationCompleted,
    /// Status text code for a finished forced storage policy purge.
    StatusTextStoragePolicyForcedPurgeCompleted,
    /// Status text code for a healthy system health check result.
    StatusTextSystemHealthy,
    /// Status text code for a finished temporary upload cleanup.
    StatusTextTemporaryUploadCleanupFinished,
    /// Status text code when an up-to-date thumbnail already exists and is reused.
    StatusTextThumbnailAlreadyAvailable,
    /// Status text code for a newly generated thumbnail.
    StatusTextThumbnailReady,
    /// Status text code for a completed trash purge.
    StatusTextTrashPurged,
    /// Status text code while cleanup waits for presigned upload URLs to expire.
    StatusTextWaitingPresignedUrlExpiry,
    /// Title code for archive compression tasks.
    TaskNameArchiveCompress,
    /// Title code for archive extraction tasks.
    TaskNameArchiveExtract,
    /// Title code for archive preview generation tasks.
    TaskNameArchivePreviewGenerate,
    /// Fallback title code used when the archive source file name is blank;
    /// renders the source file and blob IDs.
    TaskNameArchivePreviewGenerateFileId,
    /// Title code for folder-tree delete tasks.
    TaskNameFolderTreeDelete,
    /// Title code for folder-tree restore tasks.
    TaskNameFolderTreeRestore,
    /// Title code for image preview generation tasks.
    TaskNameImagePreviewGenerate,
    /// Title code naming the source blob and image processor for preview generation.
    TaskNameImagePreviewGenerateBlobWithProcessor,
    /// Title code naming the source blob and media kind for metadata extraction.
    TaskNameMediaMetadataExtractBlob,
    /// Title code naming the source file for media metadata extraction.
    TaskNameMediaMetadataExtractSource,
    /// Title code naming the target filename and source URL of an offline download.
    TaskNameOfflineDownloadSource,
    /// Title code naming the target filename, source URL, and download engine.
    TaskNameOfflineDownloadSourceWithEngine,
    /// Title code naming the destination folder and source URL of an offline download.
    TaskNameOfflineDownloadTargetFolder,
    /// Title code naming the destination folder, source URL, and download engine.
    TaskNameOfflineDownloadTargetFolderWithEngine,
    /// Title code showing only the source URL of an offline download.
    TaskNameOfflineDownloadUrl,
    /// Title code showing the source URL and download engine of an offline download.
    TaskNameOfflineDownloadUrlWithEngine,
    /// Title code for storage policy migration tasks.
    TaskNameStoragePolicyMigration,
    /// Title code for forced storage policy purge tasks.
    TaskNameStoragePolicyForcedPurge,
    /// Title code naming the cleaned-up policy for temporary upload cleanup.
    TaskNameStoragePolicyTempCleanup,
    /// Fallback title code showing only the policy ID for temporary upload cleanup.
    TaskNameStoragePolicyTempCleanupPolicyId,
    /// Title code for thumbnail generation tasks.
    TaskNameThumbnailGenerate,
    /// Title code naming the source blob and image processor for thumbnail generation.
    TaskNameThumbnailGenerateBlobWithProcessor,
    /// Title code for empty-trash tasks.
    TaskNameTrashPurgeAll,
}

/// Localizable presentation message attached to a task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TaskPresentationMessage {
    /// Stable presentation code the client resolves to localized copy.
    pub code: TaskPresentationCode,
    /// Interpolation parameters for the localized template, keyed by name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, serde_json::Value>,
}

/// Localizable title and status metadata for a background task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TaskPresentation {
    /// Task title message; absent when the task kind has no dedicated title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<TaskPresentationMessage>,
    /// Current status message; absent when no dedicated status text applies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<TaskPresentationMessage>,
}

/// Selection parameters for creating an in-scope archive download ticket.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateArchiveTaskParams {
    /// IDs of the files to include in the archive.
    pub file_ids: Vec<i64>,
    /// IDs of the folders whose entire subtrees to include in the archive.
    pub folder_ids: Vec<i64>,
    /// Optional archive file name; a default name is chosen when absent.
    pub archive_name: Option<String>,
}

/// Request parameters for creating an archive compression task.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CreateArchiveCompressTaskParams {
    /// IDs of the files to include in the archive.
    #[serde(default)]
    pub file_ids: Vec<i64>,
    /// IDs of the folders whose entire subtrees to include in the archive.
    #[serde(default)]
    pub folder_ids: Vec<i64>,
    /// Optional archive file name; a default name is chosen when absent.
    pub archive_name: Option<String>,
    /// Optional destination folder for the produced archive; `None` places it
    /// beside the selection.
    pub target_folder_id: Option<i64>,
}

/// Request parameters for creating an archive extraction task.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CreateArchiveExtractTaskParams {
    /// Optional destination folder; `None` extracts beside the archive file.
    pub target_folder_id: Option<i64>,
    /// Optional name for the folder that receives the extracted entries.
    pub output_folder_name: Option<String>,
    /// Text encoding assumed for entry names inside the archive.
    pub filename_encoding: ArchiveFilenameEncoding,
}

/// Persisted payload of an archive compression task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ArchiveCompressTaskPayload {
    /// IDs of the files to include in the archive.
    pub file_ids: Vec<i64>,
    /// IDs of the folders whose entire subtrees to include in the archive.
    pub folder_ids: Vec<i64>,
    /// Name of the archive file to produce.
    pub archive_name: String,
    /// Destination folder of the produced archive; `None` places it beside
    /// the selection.
    pub target_folder_id: Option<i64>,
}

/// Persisted payload of an archive extraction task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ArchiveExtractTaskPayload {
    /// ID of the archive file to extract.
    pub file_id: i64,
    /// File name recorded when the task was created, used for display only.
    pub source_file_name: String,
    /// Destination folder; `None` extracts beside the archive file.
    pub target_folder_id: Option<i64>,
    /// Name of the folder that receives the extracted entries.
    pub output_folder_name: String,
    /// Text encoding assumed for entry names inside the archive.
    #[serde(default)]
    pub filename_encoding: ArchiveFilenameEncoding,
}

/// Persisted payload of an archive preview generation task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ArchivePreviewTaskPayload {
    /// ID of the archive file to preview.
    pub file_id: i64,
    /// File name recorded when the task was created, used for display only.
    pub source_file_name: String,
    /// Blob whose content is listed for the preview.
    pub source_blob_id: i64,
    /// Content hash of `source_blob_id`; the preview is invalid once the blob changes.
    pub source_hash: String,
    /// Signature of the listing limits applied when generating the preview, so
    /// cached manifests from different limits cannot be reused.
    pub limit_signature: String,
}

/// Result of an archive compression task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ArchiveCompressTaskResult {
    /// ID of the created archive file.
    pub target_file_id: i64,
    /// Name of the created archive file.
    pub target_file_name: String,
    /// Folder that received the archive; `None` when it was placed beside the
    /// selection.
    pub target_folder_id: Option<i64>,
    /// Display path of the created archive.
    pub target_path: String,
}

/// Result of an archive extraction task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ArchiveExtractTaskResult {
    /// ID of the folder that received the extracted entries.
    pub target_folder_id: i64,
    /// Name of the folder that received the extracted entries.
    pub target_folder_name: String,
    /// Display path of the destination folder.
    pub target_path: String,
    /// Number of files created by the extraction.
    pub extracted_file_count: i64,
    /// Number of folders created by the extraction.
    pub extracted_folder_count: i64,
}

/// Result of an archive preview generation task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ArchivePreviewTaskResult {
    /// ID of the archive file that was listed.
    pub file_id: i64,
    /// Blob whose content was listed.
    pub source_blob_id: i64,
    /// Content hash of `source_blob_id` at listing time.
    pub source_hash: String,
    /// Total number of entries in the archive, including entries beyond the
    /// listing limits.
    pub entry_count: i64,
    /// Number of file entries kept in the preview manifest.
    pub file_count: i64,
    /// Number of directory entries kept in the preview manifest.
    pub directory_count: i64,
    /// Whether the listing was cut off by the preview limits.
    pub truncated: bool,
}

/// Name of a system runtime task recorded in the task history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeTaskName {
    /// A task kind currently defined by the runtime.
    Known(SystemRuntimeTaskKind),
    /// A wire name no longer defined by the runtime, kept so history rows
    /// written by older deployments still display.
    Legacy(String),
}

impl RuntimeTaskName {
    /// Returns the wire name of the task, for known and legacy names alike.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Known(kind) => kind.as_str(),
            Self::Legacy(value) => value.as_str(),
        }
    }

    /// Returns a human-readable name; legacy names have hyphens replaced by
    /// spaces.
    pub fn display_name(&self) -> String {
        match self {
            Self::Known(kind) => kind.display_name().to_string(),
            Self::Legacy(value) => value.replace('-', " "),
        }
    }

    /// Returns the runtime task kind, or `None` for a legacy name.
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

/// Persisted payload of a system runtime task execution record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RuntimeTaskPayload {
    /// Name of the runtime task that produced this record.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub task_name: RuntimeTaskName,
}

/// Overall outcome of a system health check, mirroring the runtime's health
/// status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum RuntimeSystemHealthStatus {
    /// Every checked component is healthy.
    Healthy,
    /// At least one component is degraded but the system still operates.
    Degraded,
    /// At least one component failed its check.
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

/// Per-component health report from a system health check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RuntimeSystemHealthComponent {
    /// Component name, such as `database` or `storage`.
    pub name: String,
    /// Health status of this component.
    pub status: RuntimeSystemHealthStatus,
    /// Human-readable summary of the component's condition.
    pub message: String,
    /// Optional structured details, present only when the component reports them.
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

/// Result of a system health check recorded with the runtime task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RuntimeSystemHealthResult {
    /// Aggregated health status across all checked components.
    pub status: RuntimeSystemHealthStatus,
    /// One report per checked component.
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

/// Persisted payload of a thumbnail generation task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ThumbnailGenerateTaskPayload {
    /// Blob to render the thumbnail from.
    pub blob_id: i64,
    /// Content hash of `blob_id`; the result is stale once the blob changes.
    pub blob_hash: String,
    /// File name recorded when the task was created, used for display only.
    #[serde(default)]
    pub source_file_name: String,
    /// MIME type of the source content.
    pub source_mime_type: String,
    /// Image processor selected for rendering.
    pub processor: aster_drive_model::types::MediaProcessorKind,
}

/// Persisted payload of an image preview generation task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ImagePreviewGenerateTaskPayload {
    /// Blob to render the preview from.
    pub blob_id: i64,
    /// Content hash of `blob_id`; the result is stale once the blob changes.
    pub blob_hash: String,
    /// File name recorded when the task was created, used for display only.
    #[serde(default)]
    pub source_file_name: String,
    /// MIME type of the source content.
    pub source_mime_type: String,
    /// Image processor selected for rendering.
    pub processor: aster_drive_model::types::MediaProcessorKind,
}

/// Persisted payload of an empty-trash task; the trash scope is fixed by the
/// task record, so the payload carries no fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TrashPurgeAllTaskPayload {}

/// Result of an empty-trash task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TrashPurgeAllTaskResult {
    /// Number of trash entries permanently purged.
    pub purged: u32,
}

/// Mutation applied to a folder subtree by a folder-tree task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum FolderTreeMutationOperation {
    /// Move the folder and its whole subtree into the trash.
    Delete,
    /// Restore the folder and its whole subtree from the trash.
    Restore,
}

/// Persisted payload of a folder-tree mutation task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct FolderTreeMutationTaskPayload {
    /// Root folder whose subtree the operation applies to.
    pub folder_id: i64,
    /// Mutation to apply to the subtree.
    pub operation: FolderTreeMutationOperation,
}

/// Result of a folder-tree mutation task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct FolderTreeMutationTaskResult {
    /// Number of files the operation covered in the subtree.
    pub file_count: u64,
    /// Number of folders the operation covered in the subtree.
    pub folder_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StoragePolicyCleanupPolicySnapshot {
    pub id: i64,
    pub name: String,
    pub connector_id: String,
    pub storage_config: String,
    pub max_file_size: i64,
    pub allowed_types: String,
    pub is_default: bool,
    pub chunk_size: i64,
}

/// One in-progress multipart upload to abort during temporary upload cleanup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyTempCleanupTarget {
    /// Object key the upload was started against.
    pub temp_key: String,
    /// Provider-assigned multipart upload ID to abort.
    pub multipart_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StoragePolicyTempCleanupTaskPayload {
    pub policy: StoragePolicyCleanupPolicySnapshot,
    #[serde(default)]
    pub driver_snapshot: Option<StoragePolicyCleanupDriverSnapshot>,
    pub temp_keys: Vec<String>,
    pub multipart_uploads: Vec<StoragePolicyTempCleanupTarget>,
}

/// Presentation-safe payload of a temporary upload cleanup task.
///
/// Unlike the internal execution payload, this variant exposes only counts and
/// policy identity so task views never carry credentials or driver snapshots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyTempCleanupTaskPayloadInfo {
    /// ID of the storage policy whose temporary objects are cleaned up.
    pub policy_id: i64,
    /// Name of the storage policy at task creation time.
    pub policy_name: String,
    /// Connector that owns the affected objects.
    pub connector_id: String,
    /// Number of temporary object keys scheduled for deletion.
    pub temp_key_count: usize,
    /// Number of multipart uploads scheduled to be aborted.
    pub multipart_upload_count: usize,
}

impl From<StoragePolicyTempCleanupTaskPayload> for StoragePolicyTempCleanupTaskPayloadInfo {
    fn from(value: StoragePolicyTempCleanupTaskPayload) -> Self {
        Self {
            policy_id: value.policy.id,
            policy_name: value.policy.name,
            connector_id: value.policy.connector_id,
            temp_key_count: value.temp_keys.len(),
            multipart_upload_count: value.multipart_uploads.len(),
        }
    }
}

/// Result of a system runtime task execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RuntimeTaskResult {
    /// Wall-clock execution duration in milliseconds; clamped to non-negative.
    pub duration_ms: i64,
    /// Optional short outcome summary; `None` for quiet runs worth no text.
    pub summary: Option<String>,
    /// Health report attached by system health check runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_health: Option<RuntimeSystemHealthResult>,
}

impl RuntimeTaskResult {
    /// Builds a result from UTC start and finish timestamps; a negative
    /// duration is clamped to zero.
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

/// Result of a thumbnail generation task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ThumbnailGenerateTaskResult {
    /// Blob the thumbnail was rendered for.
    pub blob_id: i64,
    /// Storage-relative path of the stored thumbnail.
    pub thumbnail_path: String,
    /// Name of the processor that produced the stored thumbnail.
    pub thumbnail_processor: String,
    /// Processor version stamp of the stored thumbnail.
    pub thumbnail_version: String,
    /// Processor used for this run.
    pub processor: aster_drive_model::types::MediaProcessorKind,
    /// Whether an up-to-date stored thumbnail was reused instead of rerendering.
    pub reused_existing_thumbnail: bool,
}

/// Result of an image preview generation task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ImagePreviewGenerateTaskResult {
    /// Blob the preview was rendered for.
    pub blob_id: i64,
    /// Storage-relative path of the stored preview.
    pub image_preview_path: String,
    /// Name of the processor that produced the stored preview.
    pub image_preview_processor: String,
    /// Processor version stamp of the stored preview.
    pub image_preview_version: String,
    /// Processor used for this run.
    pub processor: aster_drive_model::types::MediaProcessorKind,
    /// Whether an up-to-date stored preview was reused instead of rerendering.
    pub reused_existing_preview: bool,
}

pub use crate::services::media::metadata::{
    MediaMetadataExtractTaskPayload, MediaMetadataExtractTaskResult,
};

/// Result of a temporary upload cleanup task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyTempCleanupTaskResult {
    /// Number of objects deleted from the driver.
    pub deleted_objects: u64,
    /// Number of objects that no longer existed and were counted as done.
    pub missing_objects: u64,
    /// Number of objects that could not be deleted and remain for a later run.
    pub failed_objects: u64,
}

/// Product intent applied by a storage-policy migration task.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum StoragePolicyMigrationMode {
    /// Routine migration expects every source object to remain available.
    #[default]
    Normal,
    /// Recovery migration preserves every readable object without deleting the source policy.
    RecoverAvailable,
}

/// Persisted payload of a storage policy migration task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyMigrationTaskPayload {
    /// Policy whose blobs are moved.
    pub source_policy_id: i64,
    /// Policy that receives the blobs.
    pub target_policy_id: i64,
    /// Migration behavior selected when the immutable task plan was created.
    #[serde(default)]
    pub mode: StoragePolicyMigrationMode,
    /// Digest binding the task to the plan captured at creation; execution
    /// rejects a mismatch so resumed runs cannot mix plan revisions.
    pub plan_hash: String,
    /// Source policy revision observed at creation; execution aborts when the
    /// policy has since changed.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub source_policy_updated_at: chrono::DateTime<chrono::Utc>,
    /// Target policy revision observed at creation; execution aborts when the
    /// policy has since changed.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub target_policy_updated_at: chrono::DateTime<chrono::Utc>,
    /// Read-only source evidence required for recovery-mode tasks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_recovery_probe:
        Option<crate::services::storage_policy::recoverability::StoragePolicyRecoveryProbe>,
}

/// Result of a storage policy migration task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyMigrationTaskResult {
    /// Policy the blobs were moved from.
    pub source_policy_id: i64,
    /// Policy the blobs were moved to.
    pub target_policy_id: i64,
    /// Number of blobs examined across the whole run, including resumed runs.
    pub scanned_blobs: i64,
    /// Number of blobs copied from the source to the target driver.
    pub migrated_blobs: i64,
    /// Number of blobs whose references were re-pointed to an existing target
    /// blob with the same content hash.
    pub merged_blobs: i64,
    /// Number of blobs that no longer referenced the source policy and were
    /// left untouched.
    pub skipped_blobs: i64,
    /// Number of blobs whose migration failed; failures count toward the task
    /// retry policy.
    pub failed_blobs: i64,
    /// Logical bytes moved to the target policy.
    pub migrated_bytes: i64,
    /// Number of hash-less (opaque) blobs that collided with an existing
    /// target blob and were therefore given a fresh synthetic content hash.
    #[serde(default)]
    pub renamed_opaque_blobs: i64,
    /// Blob rows that still reference the source policy after task execution.
    #[serde(default)]
    pub remaining_blobs: i64,
}

/// Immutable, administrator-confirmed plan for a destructive policy purge task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyForcedPurgeTaskPayload {
    /// Storage policy whose remaining content references must be removed.
    pub policy_id: i64,
    /// Policy name embedded in the high-risk confirmation phrase.
    pub policy_name: String,
    /// Policy revision observed when the impact preview was confirmed.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub policy_updated_at: chrono::DateTime<chrono::Utc>,
    /// Digest binding this task to the previewed files, revisions, blobs, and blockers.
    pub impact_digest: String,
    /// Administrator-provided reason retained as task evidence and audit context.
    pub reason: String,
}

/// Terminal summary of a confirmed storage-policy forced purge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyForcedPurgeTaskResult {
    /// Source policy removed after all destructive cleanup checks passed.
    pub policy_id: i64,
    /// Number of file histories permanently purged.
    pub purged_files: i64,
    /// Number of source-policy blob metadata rows removed after reference checks.
    pub deleted_blob_records: i64,
    /// Logical quota bytes released through canonical file purge transactions.
    pub freed_logical_bytes: i64,
    /// Confirms that the final guarded policy deletion completed.
    pub policy_deleted: bool,
}

/// Maintenance pass applied to blob records and their stored objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum BlobMaintenanceAction {
    /// Verify stored objects against blob metadata without changing anything.
    IntegrityCheck,
    /// Re-point blob reference counts and fix rows that drifted.
    RefCountReconcile,
    /// Delete stored objects whose blob records are gone.
    OrphanCleanup,
}

/// Persisted payload of a blob maintenance task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct BlobMaintenanceTaskPayload {
    /// Maintenance pass to run.
    pub action: BlobMaintenanceAction,
    /// Optional explicit blob IDs to restrict the pass to; `None` scans all
    /// blobs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob_ids: Option<Vec<i64>>,
}

/// Result of a blob maintenance task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct BlobMaintenanceTaskResult {
    /// Pass that produced this result.
    pub action: BlobMaintenanceAction,
    /// Number of blob rows examined.
    pub scanned_blobs: i64,
    /// Number of stored objects actually probed in the driver.
    pub checked_objects: i64,
    /// Number of objects missing from the driver despite live metadata.
    pub missing_objects: i64,
    /// Number of objects whose stored size disagrees with blob metadata.
    pub size_mismatches: i64,
    /// Number of blob rows whose reference counts were corrected.
    pub ref_counts_fixed: i64,
    /// Number of stored objects removed as orphans.
    pub orphan_blobs_deleted: i64,
    /// Number of blob rows excluded from this pass, such as rows that changed
    /// owner mid-scan.
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
    /// Optional normalized target filename.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// Optional destination folder; absent means the workspace root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_folder_id: Option<i64>,
    /// Optional normalized expected checksum.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_sha256: Option<String>,
    /// Redacted source URL that is safe to render in task views.
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
    /// ID of the imported file.
    pub file_id: i64,
    /// Name of the imported file.
    pub file_name: String,
    /// Folder that received the file; `None` means the workspace root.
    pub folder_id: Option<i64>,
    /// Display path of the imported file.
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

/// Outcome of the target-capacity probe performed before a migration starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum StoragePolicyMigrationCapacityCheck {
    /// The target driver reports enough free space for the estimated copy.
    Sufficient,
    /// The target driver reports less free space than the estimated copy.
    Insufficient,
    /// The target driver cannot report capacity at all.
    Unsupported,
    /// The capacity probe could not run, such as when the connection fails.
    Unavailable,
}

/// Non-blocking warning raised by a migration dry run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum StoragePolicyMigrationDryRunWarning {
    /// The target capacity could not be confirmed, so the estimate is
    /// unverified.
    TargetCapacityUnavailable,
    /// The target multipart capability could not be probed, so the multipart
    /// plan is unverified.
    MultipartCapabilityUnavailable,
}

/// How the target driver transfers multipart data during migration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum StoragePolicyMigrationMultipartUploadMode {
    /// The driver streams parts straight to the provider without buffering.
    NativeStreaming,
    /// The driver buffers each part in memory before uploading.
    Buffered,
}

/// Reason a migration cannot start multipart uploads on the target driver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum StoragePolicyMigrationMultipartBlockReason {
    /// The required part count exceeds the provider's maximum.
    ProviderLimits,
    /// The buffered upload mode cannot fit a part inside the heap budget.
    BufferedHeapBudget,
    /// A single part would exceed the provider's maximum part size.
    ProviderObjectSize,
}

/// Multipart transfer plan the dry run computed for the target driver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyMigrationMultipartPlan {
    /// Size of the blob the plan was computed for, in bytes.
    pub blob_size: i64,
    /// Size of each upload part in bytes.
    pub part_size: i64,
    /// Number of parts the blob is split into.
    pub part_count: i64,
    /// Maximum number of parts the provider allows per upload.
    pub provider_max_parts: i64,
    /// Maximum size of a single part allowed by the provider, if limited.
    pub provider_max_part_size: Option<i64>,
    /// In-memory budget the driver may use to buffer parts, in bytes.
    pub heap_budget: i64,
    /// Transfer mode the target driver uses for multipart uploads.
    pub upload_mode: StoragePolicyMigrationMultipartUploadMode,
    /// Whether migration may start multipart uploads under this plan.
    pub can_start: bool,
    /// Why multipart uploads are blocked; `None` when `can_start` holds.
    pub reason: Option<StoragePolicyMigrationMultipartBlockReason>,
}

/// Preflight estimate computed before a storage policy migration task is
/// created; it is advisory evidence, not a guarantee about execution time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyMigrationDryRun {
    /// Policy whose blobs would be moved.
    pub source_policy_id: i64,
    /// Policy that would receive the blobs.
    pub target_policy_id: i64,
    /// Blob rows currently referenced by the source policy.
    pub source_blob_count: i64,
    /// Total logical size of the source blobs, in bytes.
    pub source_total_bytes: i64,
    /// Source blobs that carry a verifiable content hash.
    pub content_sha256_blob_count: i64,
    /// Source blobs stored without a content hash.
    pub opaque_blob_count: i64,
    /// Source blobs already matched by an identical blob on the target and
    /// therefore deduplicated instead of copied.
    pub target_matching_blob_count: i64,
    /// Blobs the target is missing and estimated to be copied.
    pub estimated_copy_blob_count: i64,
    /// Hash-less blobs whose key collides with an existing opaque hash on the
    /// target and needs a fresh synthetic hash.
    pub opaque_key_conflict_count: i64,
    /// Whether the target driver supports streaming uploads.
    pub target_supports_stream_upload: bool,
    /// Whether the write-and-delete probe against the target driver succeeded.
    pub target_connection_ok: bool,
    /// Verdict of the target capacity probe against the estimated copy size.
    pub target_capacity_check: StoragePolicyMigrationCapacityCheck,
    /// Capacity details reported by the target driver, if any.
    pub target_capacity: aster_drive_storage::StorageCapacityInfo,
    /// Multipart transfer plan; absent when no copy needs multipart upload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multipart_plan: Option<StoragePolicyMigrationMultipartPlan>,
    /// Read-only source evidence used by recover-available mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_recovery_probe:
        Option<crate::services::storage_policy::recoverability::StoragePolicyRecoveryProbe>,
    /// Whether the migration may start based on the blocking checks.
    pub can_start: bool,
    /// Non-blocking warnings about unverified preflight results.
    pub warnings: Vec<StoragePolicyMigrationDryRunWarning>,
}

/// Typed payload of a background task, tagged by `kind` on the wire.
///
/// Sensitive internal payloads (raw URLs, driver snapshots) stay in storage;
/// the variants exposed here are the presentation-safe projections decoded
/// for API responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskPayload {
    /// Compress files and folders into a new archive file.
    ArchiveCompress(ArchiveCompressTaskPayload),
    /// Extract an archive file into a destination folder.
    ArchiveExtract(ArchiveExtractTaskPayload),
    /// Generate the entry listing preview for an archive file.
    ArchivePreviewGenerate(ArchivePreviewTaskPayload),
    /// Render a thumbnail for a blob.
    ThumbnailGenerate(ThumbnailGenerateTaskPayload),
    /// Render a full-size image preview for a blob.
    ImagePreviewGenerate(ImagePreviewGenerateTaskPayload),
    /// Extract media metadata from a file or blob.
    MediaMetadataExtract(MediaMetadataExtractTaskPayload),
    /// Permanently purge every entry in a trash scope.
    TrashPurgeAll(TrashPurgeAllTaskPayload),
    /// Delete or restore a folder subtree.
    FolderTreeMutation(FolderTreeMutationTaskPayload),
    /// Delete temporary objects and abort multipart uploads owned by a policy.
    StoragePolicyTempCleanup(StoragePolicyTempCleanupTaskPayloadInfo),
    /// Move blobs from one storage policy to another.
    StoragePolicyMigration(StoragePolicyMigrationTaskPayload),
    /// Permanently purge a storage policy and everything referencing it.
    StoragePolicyForcedPurge(StoragePolicyForcedPurgeTaskPayload),
    /// Run a blob integrity, reference-count, or orphan maintenance pass.
    BlobMaintenance(BlobMaintenanceTaskPayload),
    /// Download a remote URL and import the result into the workspace.
    OfflineDownload(OfflineDownloadTaskPayloadInfo),
    /// Execution record of a system runtime task.
    SystemRuntime(RuntimeTaskPayload),
}

/// Typed result of a finished background task, tagged by `kind` on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskResult {
    /// Outcome of an archive compression task.
    ArchiveCompress(ArchiveCompressTaskResult),
    /// Outcome of an archive extraction task.
    ArchiveExtract(ArchiveExtractTaskResult),
    /// Outcome of an archive preview generation task.
    ArchivePreviewGenerate(ArchivePreviewTaskResult),
    /// Outcome of a thumbnail generation task.
    ThumbnailGenerate(ThumbnailGenerateTaskResult),
    /// Outcome of an image preview generation task.
    ImagePreviewGenerate(ImagePreviewGenerateTaskResult),
    /// Outcome of a media metadata extraction task.
    MediaMetadataExtract(MediaMetadataExtractTaskResult),
    /// Outcome of an empty-trash task.
    TrashPurgeAll(TrashPurgeAllTaskResult),
    /// Outcome of a folder-tree mutation task.
    FolderTreeMutation(FolderTreeMutationTaskResult),
    /// Outcome of a temporary upload cleanup task.
    StoragePolicyTempCleanup(StoragePolicyTempCleanupTaskResult),
    /// Outcome of a storage policy migration task.
    StoragePolicyMigration(StoragePolicyMigrationTaskResult),
    /// Outcome of a forced storage policy purge task.
    StoragePolicyForcedPurge(StoragePolicyForcedPurgeTaskResult),
    /// Outcome of a blob maintenance task.
    BlobMaintenance(BlobMaintenanceTaskResult),
    /// Outcome of an offline download task.
    OfflineDownload(OfflineDownloadTaskResult),
    /// Outcome of a system runtime task execution.
    SystemRuntime(RuntimeTaskResult),
}

/// API view of one background task row.
///
/// The database stores generic JSON payload, result, and step snapshots; this
/// view decodes them into kind-typed structures so list and detail pages do
/// not need to know the per-kind storage formats.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TaskInfo {
    /// Task record ID.
    pub id: i64,
    /// Kind identifying the task type.
    pub kind: BackgroundTaskKind,
    /// Current lifecycle status of the task.
    pub status: BackgroundTaskStatus,
    /// Human-readable task name shown in task lists.
    pub display_name: String,
    /// User that created the task; `None` for system-generated tasks.
    pub creator: Option<account::UserSummary>,
    /// Team scope of the task, when team-scoped.
    pub team_id: Option<i64>,
    /// Share scope of the task, when created through a public share.
    pub share_id: Option<i64>,
    /// Completed work units, in the unit the task defines.
    pub progress_current: i64,
    /// Total work units, in the unit the task defines; non-positive when unknown.
    pub progress_total: i64,
    /// Derived progress in percent, clamped to 0-100; 0 when the total is unknown
    /// and the task has not succeeded.
    pub progress_percent: i32,
    /// Legacy plain-text status string; newer tasks use `presentation` instead.
    pub status_text: Option<String>,
    /// Localizable title and status metadata derived from the payload and result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presentation: Option<TaskPresentation>,
    /// Attempts made so far, including the current one.
    pub attempt_count: i32,
    /// Attempt budget after which the task is failed permanently.
    pub max_attempts: i32,
    /// Display-safe message of the most recent failure.
    pub last_error: Option<String>,
    /// Kind-tagged task payload.
    pub payload: TaskPayload,
    /// Kind-tagged result; `None` until the task finishes.
    pub result: Option<TaskResult>,
    /// Progress of each declared task step.
    pub steps: Vec<TaskStepInfo>,
    /// Whether the task may be re-enqueued after a failure.
    pub can_retry: bool,
    /// Whether this scoped user task can accept a cancellation request.
    pub can_cancel: bool,
    /// Expiry of the current execution lease; `None` when no worker holds it.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub lease_expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// When the current attempt started; `None` before the first claim.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    /// When the task reached a terminal status; `None` while still pending.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    /// When the finished record becomes eligible for retention cleanup.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: chrono::DateTime<chrono::Utc>,
    /// When the task record was created.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the task record was last updated.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use aster_drive_storage::ConnectorId;

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct TestCleanupSnapshot {
        token: String,
    }

    #[test]
    fn cleanup_snapshot_envelope_round_trips_typed_payload() {
        let connector_id = ConnectorId::declared("plugin.cleanup");
        let snapshot = StoragePolicyCleanupDriverSnapshot::encode(
            connector_id.clone(),
            2,
            &TestCleanupSnapshot {
                token: "ciphertext".to_string(),
            },
        )
        .expect("cleanup snapshot should encode");
        let decoded: TestCleanupSnapshot = snapshot
            .decode(connector_id.as_str(), 2)
            .expect("cleanup snapshot should decode");
        assert_eq!(
            decoded,
            TestCleanupSnapshot {
                token: "ciphertext".to_string()
            }
        );
        assert!(
            snapshot
                .decode::<TestCleanupSnapshot>("plugin.other", 2)
                .is_err()
        );
        assert!(
            snapshot
                .decode::<TestCleanupSnapshot>(connector_id.as_str(), 3)
                .is_err()
        );
    }
}

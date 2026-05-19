//! 后台任务服务子模块：`types`。

use sea_orm::ActiveEnum;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::entities::background_task;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::services::user_service;
use crate::types::{
    BackgroundTaskKind, BackgroundTaskStatus, DriverType, StoredTaskPayload, StoredTaskResult,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TaskStepStatus {
    Pending,
    Active,
    Succeeded,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TaskStepInfo {
    pub key: String,
    pub title: String,
    pub status: TaskStepStatus,
    pub progress_current: i64,
    pub progress_total: i64,
    pub detail: Option<String>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RuntimeTaskPayload {
    pub task_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum RuntimeSystemHealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RuntimeSystemHealthComponent {
    pub name: String,
    pub status: RuntimeSystemHealthStatus,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RuntimeSystemHealthResult {
    pub status: RuntimeSystemHealthStatus,
    pub components: Vec<RuntimeSystemHealthComponent>,
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
    pub max_file_size: i64,
    pub allowed_types: String,
    pub options: String,
    pub is_default: bool,
    pub chunk_size: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StoragePolicyCleanupRemoteNodeSnapshot {
    pub id: i64,
    pub name: String,
    pub base_url: String,
    pub access_key: String,
    pub secret_key: String,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RuntimeTaskResult {
    pub duration_ms: i64,
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_health: Option<RuntimeSystemHealthResult>,
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
pub struct StoragePolicyTempCleanupTaskResult {
    pub deleted_objects: u64,
    pub missing_objects: u64,
    pub failed_objects: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskPayload {
    ArchiveCompress(ArchiveCompressTaskPayload),
    ArchiveExtract(ArchiveExtractTaskPayload),
    ArchivePreviewGenerate(ArchivePreviewTaskPayload),
    ThumbnailGenerate(ThumbnailGenerateTaskPayload),
    TrashPurgeAll(TrashPurgeAllTaskPayload),
    StoragePolicyTempCleanup(StoragePolicyTempCleanupTaskPayloadInfo),
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
    TrashPurgeAll(TrashPurgeAllTaskResult),
    StoragePolicyTempCleanup(StoragePolicyTempCleanupTaskResult),
    SystemRuntime(RuntimeTaskResult),
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TaskInfo {
    pub id: i64,
    pub kind: BackgroundTaskKind,
    pub status: BackgroundTaskStatus,
    pub display_name: String,
    pub creator: Option<user_service::UserSummary>,
    pub team_id: Option<i64>,
    pub share_id: Option<i64>,
    pub progress_current: i64,
    pub progress_total: i64,
    pub progress_percent: i32,
    pub status_text: Option<String>,
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

pub(super) fn parse_task_payload<T>(task: &background_task::Model) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_str(task.payload_json.as_ref()).map_err(|error| {
        AsterError::internal_error(format!(
            "parse payload for task #{} ({}): {error}",
            task.id,
            task.kind.to_value()
        ))
    })
}

pub(super) fn parse_task_payload_info(task: &background_task::Model) -> Result<TaskPayload> {
    match task.kind {
        BackgroundTaskKind::ArchiveCompress => {
            Ok(TaskPayload::ArchiveCompress(parse_task_payload(task)?))
        }
        BackgroundTaskKind::ArchiveExtract => {
            Ok(TaskPayload::ArchiveExtract(parse_task_payload(task)?))
        }
        BackgroundTaskKind::ArchivePreviewGenerate => Ok(TaskPayload::ArchivePreviewGenerate(
            parse_task_payload(task)?,
        )),
        BackgroundTaskKind::ThumbnailGenerate => {
            Ok(TaskPayload::ThumbnailGenerate(parse_task_payload(task)?))
        }
        BackgroundTaskKind::TrashPurgeAll => {
            Ok(TaskPayload::TrashPurgeAll(parse_task_payload(task)?))
        }
        BackgroundTaskKind::StoragePolicyTempCleanup => Ok(TaskPayload::StoragePolicyTempCleanup(
            parse_task_payload::<StoragePolicyTempCleanupTaskPayload>(task)?.into(),
        )),
        BackgroundTaskKind::SystemRuntime => {
            Ok(TaskPayload::SystemRuntime(parse_task_payload(task)?))
        }
    }
}

pub(super) fn parse_task_result_info(task: &background_task::Model) -> Result<Option<TaskResult>> {
    let raw = match task.result_json.as_ref() {
        Some(raw) => raw,
        None => return Ok(None),
    };

    match task.kind {
        BackgroundTaskKind::ArchiveCompress => Ok(Some(TaskResult::ArchiveCompress(
            serde_json::from_str(raw.as_ref()).map_err(|error| {
                AsterError::internal_error(format!(
                    "parse result for task #{} ({}): {error}",
                    task.id,
                    task.kind.to_value()
                ))
            })?,
        ))),
        BackgroundTaskKind::ArchiveExtract => Ok(Some(TaskResult::ArchiveExtract(
            serde_json::from_str(raw.as_ref()).map_err(|error| {
                AsterError::internal_error(format!(
                    "parse result for task #{} ({}): {error}",
                    task.id,
                    task.kind.to_value()
                ))
            })?,
        ))),
        BackgroundTaskKind::ArchivePreviewGenerate => Ok(Some(TaskResult::ArchivePreviewGenerate(
            serde_json::from_str(raw.as_ref()).map_err(|error| {
                AsterError::internal_error(format!(
                    "parse result for task #{} ({}): {error}",
                    task.id,
                    task.kind.to_value()
                ))
            })?,
        ))),
        BackgroundTaskKind::ThumbnailGenerate => Ok(Some(TaskResult::ThumbnailGenerate(
            serde_json::from_str(raw.as_ref()).map_err(|error| {
                AsterError::internal_error(format!(
                    "parse result for task #{} ({}): {error}",
                    task.id,
                    task.kind.to_value()
                ))
            })?,
        ))),
        BackgroundTaskKind::TrashPurgeAll => Ok(Some(TaskResult::TrashPurgeAll(
            serde_json::from_str(raw.as_ref()).map_err(|error| {
                AsterError::internal_error(format!(
                    "parse result for task #{} ({}): {error}",
                    task.id,
                    task.kind.to_value()
                ))
            })?,
        ))),
        BackgroundTaskKind::StoragePolicyTempCleanup => {
            Ok(Some(TaskResult::StoragePolicyTempCleanup(
                serde_json::from_str(raw.as_ref()).map_err(|error| {
                    AsterError::internal_error(format!(
                        "parse result for task #{} ({}): {error}",
                        task.id,
                        task.kind.to_value()
                    ))
                })?,
            )))
        }
        BackgroundTaskKind::SystemRuntime => Ok(Some(TaskResult::SystemRuntime(
            serde_json::from_str(raw.as_ref()).map_err(|error| {
                AsterError::internal_error(format!(
                    "parse result for task #{} ({}): {error}",
                    task.id,
                    task.kind.to_value()
                ))
            })?,
        ))),
    }
}

pub(super) fn serialize_task_payload<T: Serialize>(payload: &T) -> Result<StoredTaskPayload> {
    serde_json::to_string(payload)
        .map(StoredTaskPayload)
        .map_aster_err_ctx("serialize task payload", AsterError::internal_error)
}

pub(super) fn serialize_task_result<T: Serialize>(result: &T) -> Result<StoredTaskResult> {
    serde_json::to_string(result)
        .map(StoredTaskResult)
        .map_aster_err_ctx("serialize task result", AsterError::internal_error)
}

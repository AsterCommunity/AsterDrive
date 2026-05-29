use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Raw JSON payload stored in `background_tasks.payload_json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, DeriveValueType)]
pub struct StoredTaskPayload(pub String);

impl AsRef<str> for StoredTaskPayload {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for StoredTaskPayload {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<StoredTaskPayload> for String {
    fn from(value: StoredTaskPayload) -> Self {
        value.0
    }
}

/// Raw JSON payload stored in `background_tasks.result_json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, DeriveValueType)]
pub struct StoredTaskResult(pub String);

impl AsRef<str> for StoredTaskResult {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for StoredTaskResult {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<StoredTaskResult> for String {
    fn from(value: StoredTaskResult) -> Self {
        value.0
    }
}

/// Raw JSON payload stored in `background_tasks.steps_json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, DeriveValueType)]
pub struct StoredTaskSteps(pub String);

impl AsRef<str> for StoredTaskSteps {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for StoredTaskSteps {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<StoredTaskSteps> for String {
    fn from(value: StoredTaskSteps) -> Self {
        value.0
    }
}

/// Raw JSON payload stored in `resource_locks.owner_info`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, DeriveValueType)]
pub struct StoredLockOwnerInfo(pub String);

impl AsRef<str> for StoredLockOwnerInfo {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for StoredLockOwnerInfo {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<StoredLockOwnerInfo> for String {
    fn from(value: StoredLockOwnerInfo) -> Self {
        value.0
    }
}

/// 后台任务类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
#[serde(rename_all = "snake_case")]
pub enum BackgroundTaskKind {
    #[sea_orm(string_value = "archive_extract")]
    ArchiveExtract,
    #[sea_orm(string_value = "archive_compress")]
    ArchiveCompress,
    #[sea_orm(string_value = "archive_preview_generate")]
    ArchivePreviewGenerate,
    #[sea_orm(string_value = "thumbnail_generate")]
    ThumbnailGenerate,
    #[sea_orm(string_value = "media_metadata_extract")]
    MediaMetadataExtract,
    #[sea_orm(string_value = "trash_purge_all")]
    TrashPurgeAll,
    #[sea_orm(string_value = "storage_policy_temp_cleanup")]
    StoragePolicyTempCleanup,
    #[sea_orm(string_value = "storage_policy_migration")]
    StoragePolicyMigration,
    #[sea_orm(string_value = "blob_maintenance")]
    BlobMaintenance,
    #[sea_orm(string_value = "system_runtime")]
    SystemRuntime,
}

impl BackgroundTaskKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ArchiveExtract => "archive_extract",
            Self::ArchiveCompress => "archive_compress",
            Self::ArchivePreviewGenerate => "archive_preview_generate",
            Self::ThumbnailGenerate => "thumbnail_generate",
            Self::MediaMetadataExtract => "media_metadata_extract",
            Self::TrashPurgeAll => "trash_purge_all",
            Self::StoragePolicyTempCleanup => "storage_policy_temp_cleanup",
            Self::StoragePolicyMigration => "storage_policy_migration",
            Self::BlobMaintenance => "blob_maintenance",
            Self::SystemRuntime => "system_runtime",
        }
    }
}

/// 后台任务状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "snake_case")]
pub enum BackgroundTaskStatus {
    #[sea_orm(string_value = "pending")]
    Pending,
    #[sea_orm(string_value = "processing")]
    Processing,
    #[sea_orm(string_value = "retry")]
    Retry,
    #[sea_orm(string_value = "succeeded")]
    Succeeded,
    #[sea_orm(string_value = "failed")]
    Failed,
    #[sea_orm(string_value = "canceled")]
    Canceled,
}

impl BackgroundTaskStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Retry => "retry",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Canceled)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BackgroundTaskKind, BackgroundTaskStatus, StoredLockOwnerInfo, StoredTaskPayload,
        StoredTaskResult, StoredTaskSteps,
    };

    #[test]
    fn stored_task_payload_wrappers_preserve_raw_json() {
        let payload = StoredTaskPayload::from("{\"kind\":\"archive\"}".to_string());
        assert_eq!(payload.as_ref(), "{\"kind\":\"archive\"}");
        let raw: String = payload.into();
        assert_eq!(raw, "{\"kind\":\"archive\"}");

        let result = StoredTaskResult::from("{\"ok\":true}".to_string());
        assert_eq!(result.as_ref(), "{\"ok\":true}");
        let raw: String = result.into();
        assert_eq!(raw, "{\"ok\":true}");

        let steps = StoredTaskSteps::from("[{\"key\":\"prepare\"}]".to_string());
        assert_eq!(steps.as_ref(), "[{\"key\":\"prepare\"}]");
        let raw: String = steps.into();
        assert_eq!(raw, "[{\"key\":\"prepare\"}]");

        let owner = StoredLockOwnerInfo::from("{\"user\":\"alice\"}".to_string());
        assert_eq!(owner.as_ref(), "{\"user\":\"alice\"}");
        let raw: String = owner.into();
        assert_eq!(raw, "{\"user\":\"alice\"}");
    }

    #[test]
    fn background_task_status_terminal_states_are_explicit() {
        assert!(!BackgroundTaskStatus::Pending.is_terminal());
        assert!(!BackgroundTaskStatus::Processing.is_terminal());
        assert!(!BackgroundTaskStatus::Retry.is_terminal());
        assert!(BackgroundTaskStatus::Succeeded.is_terminal());
        assert!(BackgroundTaskStatus::Failed.is_terminal());
        assert!(BackgroundTaskStatus::Canceled.is_terminal());
    }

    #[test]
    fn background_task_kind_serializes_to_stable_snake_case_names() {
        let cases = [
            (BackgroundTaskKind::ArchiveExtract, "archive_extract"),
            (BackgroundTaskKind::ArchiveCompress, "archive_compress"),
            (
                BackgroundTaskKind::ArchivePreviewGenerate,
                "archive_preview_generate",
            ),
            (BackgroundTaskKind::ThumbnailGenerate, "thumbnail_generate"),
            (
                BackgroundTaskKind::MediaMetadataExtract,
                "media_metadata_extract",
            ),
            (BackgroundTaskKind::TrashPurgeAll, "trash_purge_all"),
            (
                BackgroundTaskKind::StoragePolicyTempCleanup,
                "storage_policy_temp_cleanup",
            ),
            (
                BackgroundTaskKind::StoragePolicyMigration,
                "storage_policy_migration",
            ),
            (BackgroundTaskKind::BlobMaintenance, "blob_maintenance"),
            (BackgroundTaskKind::SystemRuntime, "system_runtime"),
        ];

        for (kind, expected) in cases {
            assert_eq!(serde_json::to_value(kind).unwrap(), expected);
        }
    }
}

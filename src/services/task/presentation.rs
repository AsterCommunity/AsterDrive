//! Stable API presentation metadata for background tasks.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::config::operations;
use aster_drive_model::types::{
    BackgroundTaskStatus, MediaMetadataKind, MediaMetadataStatus, MediaProcessorKind,
};

use super::runtime::SystemRuntimeTaskKind;
use super::types::{
    BlobMaintenanceAction, RuntimeSystemHealthComponent, RuntimeSystemHealthResult,
    RuntimeSystemHealthStatus, RuntimeTaskResult, TaskPayload, TaskPresentation,
    TaskPresentationCode, TaskPresentationMessage, TaskResult,
};

use TaskPresentationCode as Code;

pub(super) fn build_task_presentation(
    payload: &TaskPayload,
    result: Option<&TaskResult>,
    status: BackgroundTaskStatus,
    context: TaskPresentationContext,
) -> Option<TaskPresentation> {
    let title = title_message(payload, result, context);
    let status = status_message(payload, result, status);

    Some(TaskPresentation { title, status })
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct TaskPresentationContext {
    pub(super) selected_offline_download_engine: Option<operations::OfflineDownloadEngine>,
}

fn title_message(
    payload: &TaskPayload,
    result: Option<&TaskResult>,
    context: TaskPresentationContext,
) -> Option<TaskPresentationMessage> {
    match payload {
        TaskPayload::ArchiveCompress(payload) => Some(PresentationMessage::ArchiveCompressTitle {
            name: payload.archive_name.clone(),
        }),
        TaskPayload::ArchiveExtract(payload) => Some(PresentationMessage::ArchiveExtractTitle {
            name: payload.source_file_name.clone(),
        }),
        TaskPayload::ArchivePreviewGenerate(payload) => {
            let source_file_name = payload.source_file_name.trim();
            if source_file_name.is_empty() {
                Some(PresentationMessage::ArchivePreviewGenerateFileIdTitle {
                    file_id: payload.file_id,
                    blob_id: payload.source_blob_id,
                })
            } else {
                Some(PresentationMessage::ArchivePreviewGenerateTitle {
                    name: source_file_name.to_string(),
                })
            }
        }
        TaskPayload::ThumbnailGenerate(payload) => {
            let source_file_name = payload.source_file_name.trim();
            if source_file_name.is_empty() {
                Some(PresentationMessage::ThumbnailGenerateBlobTitle {
                    blob_id: payload.blob_id,
                    processor: payload.processor,
                })
            } else {
                Some(PresentationMessage::ThumbnailGenerateSourceTitle {
                    source: source_file_name.to_string(),
                    blob_id: payload.blob_id,
                    processor: payload.processor,
                })
            }
        }
        TaskPayload::ImagePreviewGenerate(payload) => {
            let source_file_name = payload.source_file_name.trim();
            if source_file_name.is_empty() {
                Some(PresentationMessage::ImagePreviewGenerateBlobTitle {
                    blob_id: payload.blob_id,
                    processor: payload.processor,
                })
            } else {
                Some(PresentationMessage::ImagePreviewGenerateSourceTitle {
                    source: source_file_name.to_string(),
                    blob_id: payload.blob_id,
                    processor: payload.processor,
                })
            }
        }
        TaskPayload::MediaMetadataExtract(payload) => {
            let source_file_name = payload.source_file_name.trim();
            if source_file_name.is_empty() {
                Some(PresentationMessage::MediaMetadataExtractBlobTitle {
                    blob_id: payload.blob_id,
                    kind: payload.media_kind,
                })
            } else {
                Some(PresentationMessage::MediaMetadataExtractSourceTitle {
                    source: source_file_name.to_string(),
                    blob_id: payload.blob_id,
                    kind: payload.media_kind,
                })
            }
        }
        TaskPayload::TrashPurgeAll(_) => Some(PresentationMessage::TrashPurgeAllTitle),
        TaskPayload::StoragePolicyTempCleanup(payload) => {
            let policy_name = payload.policy_name.trim();
            if policy_name.is_empty() {
                Some(PresentationMessage::StoragePolicyTempCleanupPolicyIdTitle {
                    policy_id: payload.policy_id,
                })
            } else {
                Some(PresentationMessage::StoragePolicyTempCleanupTitle {
                    policy: policy_name.to_string(),
                    policy_id: payload.policy_id,
                })
            }
        }
        TaskPayload::StoragePolicyMigration(payload) => {
            Some(PresentationMessage::StoragePolicyMigrationTitle {
                source_policy_id: payload.source_policy_id,
                target_policy_id: payload.target_policy_id,
            })
        }
        TaskPayload::BlobMaintenance(payload) => Some(PresentationMessage::BlobMaintenanceTitle {
            action: payload.action,
            selected_blob_count: payload
                .blob_ids
                .as_ref()
                .and_then(|blob_ids| (!blob_ids.is_empty()).then_some(blob_ids.len())),
        }),
        TaskPayload::OfflineDownload(payload) => {
            let engine = match result {
                Some(TaskResult::OfflineDownload(result)) => result.download_engine,
                _ => None,
            }
            .or(context.selected_offline_download_engine);
            if let Some(filename) = payload.filename.as_deref().map(str::trim)
                && !filename.is_empty()
            {
                Some(PresentationMessage::OfflineDownloadSourceTitle {
                    filename: filename.to_string(),
                    source: payload.source_display_url.clone(),
                    engine,
                })
            } else if let Some(target_folder_id) = payload.target_folder_id {
                Some(PresentationMessage::OfflineDownloadTargetFolderTitle {
                    source: payload.source_display_url.clone(),
                    target_folder_id,
                    engine,
                })
            } else {
                Some(PresentationMessage::OfflineDownloadUrlTitle {
                    source: payload.source_display_url.clone(),
                    engine,
                })
            }
        }
        TaskPayload::SystemRuntime(payload) => payload
            .task_name
            .known()
            .map(|kind| PresentationMessage::RuntimeTaskTitle { kind }),
    }
    .map(Into::into)
}

fn status_message(
    payload: &TaskPayload,
    result: Option<&TaskResult>,
    status: BackgroundTaskStatus,
) -> Option<TaskPresentationMessage> {
    match (payload, result) {
        (TaskPayload::SystemRuntime(_), Some(TaskResult::SystemRuntime(result))) => {
            system_runtime_status_message(result)
        }
        (_, Some(result)) => status_message_from_result(result),
        _ => status_message_without_result(payload, status),
    }
}

fn status_message_from_result(result: &TaskResult) -> Option<TaskPresentationMessage> {
    match result {
        TaskResult::ArchiveCompress(result) => Some(PresentationMessage::ArchiveReadyStatus {
            name: result.target_file_name.clone(),
            target_file_id: result.target_file_id,
            target_path: result.target_path.clone(),
        }),
        TaskResult::ArchiveExtract(result) => Some(PresentationMessage::ArchiveExtractedStatus {
            name: result.target_folder_name.clone(),
            target_folder_id: result.target_folder_id,
            target_path: result.target_path.clone(),
            file_count: result.extracted_file_count,
            folder_count: result.extracted_folder_count,
        }),
        TaskResult::ArchivePreviewGenerate(_) => {
            Some(PresentationMessage::ArchivePreviewReadyStatus)
        }
        TaskResult::ThumbnailGenerate(result) => Some(if result.reused_existing_thumbnail {
            PresentationMessage::ThumbnailAlreadyAvailableStatus
        } else {
            PresentationMessage::ThumbnailReadyStatus
        }),
        TaskResult::ImagePreviewGenerate(result) => Some(if result.reused_existing_preview {
            PresentationMessage::ImagePreviewAlreadyAvailableStatus
        } else {
            PresentationMessage::ImagePreviewReadyStatus
        }),
        TaskResult::MediaMetadataExtract(result) => {
            Some(PresentationMessage::MediaMetadataStatus {
                status: result.status,
            })
        }
        TaskResult::TrashPurgeAll(result) => Some(PresentationMessage::TrashPurgedStatus {
            purged: result.purged,
        }),
        TaskResult::StoragePolicyTempCleanup(result) => {
            Some(PresentationMessage::TemporaryUploadCleanupFinishedStatus {
                deleted_objects: result.deleted_objects,
                missing_objects: result.missing_objects,
                failed_objects: result.failed_objects,
            })
        }
        TaskResult::StoragePolicyMigration(result) => {
            Some(PresentationMessage::StorageMigrationCompletedStatus {
                source_policy_id: result.source_policy_id,
                target_policy_id: result.target_policy_id,
                scanned_blobs: result.scanned_blobs,
                migrated_blobs: result.migrated_blobs,
                merged_blobs: result.merged_blobs,
                skipped_blobs: result.skipped_blobs,
                failed_blobs: result.failed_blobs,
                migrated_bytes: result.migrated_bytes,
                renamed_opaque_blobs: result.renamed_opaque_blobs,
            })
        }
        TaskResult::BlobMaintenance(_) => Some(PresentationMessage::BlobMaintenanceFinishedStatus),
        TaskResult::OfflineDownload(result) => {
            Some(PresentationMessage::OfflineDownloadImportedStatus {
                name: result.file_name.clone(),
                target_path: result.file_path.clone(),
                content_length: result.content_length,
            })
        }
        TaskResult::SystemRuntime(_) => None,
    }
    .map(Into::into)
}

fn status_message_without_result(
    payload: &TaskPayload,
    status: BackgroundTaskStatus,
) -> Option<TaskPresentationMessage> {
    match payload {
        TaskPayload::StoragePolicyTempCleanup(_)
            if matches!(
                status,
                BackgroundTaskStatus::Pending | BackgroundTaskStatus::Retry
            ) =>
        {
            Some(PresentationMessage::WaitingPresignedUrlExpiryStatus.into())
        }
        _ => None,
    }
}

fn runtime_system_health_status_message(
    health: &RuntimeSystemHealthResult,
) -> Option<TaskPresentationMessage> {
    if health.status == RuntimeSystemHealthStatus::Healthy {
        return Some(PresentationMessage::SystemHealthyStatus.into());
    }

    let components = health
        .components
        .iter()
        .filter(|component| component.status != RuntimeSystemHealthStatus::Healthy)
        .cloned()
        .collect::<Vec<_>>();

    Some(
        PresentationMessage::RuntimeSystemHealthIssueStatus {
            status: health.status,
            components,
        }
        .into(),
    )
}

fn system_runtime_status_message(result: &RuntimeTaskResult) -> Option<TaskPresentationMessage> {
    result
        .system_health
        .as_ref()
        .and_then(runtime_system_health_status_message)
}

fn media_metadata_status_code(status: MediaMetadataStatus) -> Code {
    match status {
        MediaMetadataStatus::Ready => Code::StatusTextMediaMetadataReady,
        MediaMetadataStatus::Failed => Code::StatusTextMediaMetadataFailed,
        MediaMetadataStatus::Unsupported => Code::StatusTextMediaMetadataUnsupported,
    }
}

#[derive(Debug, Default)]
struct Params(BTreeMap<String, Value>);

impl Params {
    fn new() -> Self {
        Self::default()
    }

    fn with(mut self, key: &str, value: Value) -> Self {
        self.0.insert(key.to_string(), value);
        self
    }

    fn finish(self) -> BTreeMap<String, Value> {
        self.0
    }
}

enum PresentationMessage {
    ArchiveCompressTitle {
        name: String,
    },
    ArchiveExtractTitle {
        name: String,
    },
    ArchivePreviewGenerateTitle {
        name: String,
    },
    ArchivePreviewGenerateFileIdTitle {
        file_id: i64,
        blob_id: i64,
    },
    ThumbnailGenerateSourceTitle {
        source: String,
        blob_id: i64,
        processor: MediaProcessorKind,
    },
    ThumbnailGenerateBlobTitle {
        blob_id: i64,
        processor: MediaProcessorKind,
    },
    ImagePreviewGenerateSourceTitle {
        source: String,
        blob_id: i64,
        processor: MediaProcessorKind,
    },
    ImagePreviewGenerateBlobTitle {
        blob_id: i64,
        processor: MediaProcessorKind,
    },
    MediaMetadataExtractSourceTitle {
        source: String,
        blob_id: i64,
        kind: MediaMetadataKind,
    },
    MediaMetadataExtractBlobTitle {
        blob_id: i64,
        kind: MediaMetadataKind,
    },
    TrashPurgeAllTitle,
    StoragePolicyTempCleanupTitle {
        policy: String,
        policy_id: i64,
    },
    StoragePolicyTempCleanupPolicyIdTitle {
        policy_id: i64,
    },
    StoragePolicyMigrationTitle {
        source_policy_id: i64,
        target_policy_id: i64,
    },
    BlobMaintenanceTitle {
        action: BlobMaintenanceAction,
        selected_blob_count: Option<usize>,
    },
    OfflineDownloadSourceTitle {
        filename: String,
        source: String,
        engine: Option<operations::OfflineDownloadEngine>,
    },
    OfflineDownloadTargetFolderTitle {
        source: String,
        target_folder_id: i64,
        engine: Option<operations::OfflineDownloadEngine>,
    },
    OfflineDownloadUrlTitle {
        source: String,
        engine: Option<operations::OfflineDownloadEngine>,
    },
    RuntimeTaskTitle {
        kind: SystemRuntimeTaskKind,
    },
    ArchiveReadyStatus {
        name: String,
        target_file_id: i64,
        target_path: String,
    },
    ArchiveExtractedStatus {
        name: String,
        target_folder_id: i64,
        target_path: String,
        file_count: i64,
        folder_count: i64,
    },
    ArchivePreviewReadyStatus,
    ThumbnailAlreadyAvailableStatus,
    ThumbnailReadyStatus,
    ImagePreviewAlreadyAvailableStatus,
    ImagePreviewReadyStatus,
    MediaMetadataStatus {
        status: MediaMetadataStatus,
    },
    TrashPurgedStatus {
        purged: u32,
    },
    TemporaryUploadCleanupFinishedStatus {
        deleted_objects: u64,
        missing_objects: u64,
        failed_objects: u64,
    },
    StorageMigrationCompletedStatus {
        source_policy_id: i64,
        target_policy_id: i64,
        scanned_blobs: i64,
        migrated_blobs: i64,
        merged_blobs: i64,
        skipped_blobs: i64,
        failed_blobs: i64,
        migrated_bytes: i64,
        renamed_opaque_blobs: i64,
    },
    BlobMaintenanceFinishedStatus,
    OfflineDownloadImportedStatus {
        name: String,
        target_path: String,
        content_length: i64,
    },
    WaitingPresignedUrlExpiryStatus,
    SystemHealthyStatus,
    RuntimeSystemHealthIssueStatus {
        status: RuntimeSystemHealthStatus,
        components: Vec<RuntimeSystemHealthComponent>,
    },
}

impl From<PresentationMessage> for TaskPresentationMessage {
    fn from(message: PresentationMessage) -> Self {
        let (code, params) = match message {
            PresentationMessage::ArchiveCompressTitle { name } => (
                Code::TaskNameArchiveCompress,
                Params::new().with("name", json!(name)).finish(),
            ),
            PresentationMessage::ArchiveExtractTitle { name } => (
                Code::TaskNameArchiveExtract,
                Params::new().with("name", json!(name)).finish(),
            ),
            PresentationMessage::ArchivePreviewGenerateTitle { name } => (
                Code::TaskNameArchivePreviewGenerate,
                Params::new().with("name", json!(name)).finish(),
            ),
            PresentationMessage::ArchivePreviewGenerateFileIdTitle { file_id, blob_id } => (
                Code::TaskNameArchivePreviewGenerateFileId,
                Params::new()
                    .with("fileId", json!(file_id))
                    .with("blobId", json!(blob_id))
                    .finish(),
            ),
            PresentationMessage::ThumbnailGenerateSourceTitle {
                source,
                blob_id,
                processor,
            } => (
                Code::TaskNameThumbnailGenerate,
                Params::new()
                    .with("source", json!(source))
                    .with("blobId", json!(blob_id))
                    .with("processor", json!(processor.as_str()))
                    .finish(),
            ),
            PresentationMessage::ThumbnailGenerateBlobTitle { blob_id, processor } => (
                Code::TaskNameThumbnailGenerateBlobWithProcessor,
                Params::new()
                    .with("blobId", json!(blob_id))
                    .with("processor", json!(processor.as_str()))
                    .finish(),
            ),
            PresentationMessage::ImagePreviewGenerateSourceTitle {
                source,
                blob_id,
                processor,
            } => (
                Code::TaskNameImagePreviewGenerate,
                Params::new()
                    .with("source", json!(source))
                    .with("blobId", json!(blob_id))
                    .with("processor", json!(processor.as_str()))
                    .finish(),
            ),
            PresentationMessage::ImagePreviewGenerateBlobTitle { blob_id, processor } => (
                Code::TaskNameImagePreviewGenerateBlobWithProcessor,
                Params::new()
                    .with("blobId", json!(blob_id))
                    .with("processor", json!(processor.as_str()))
                    .finish(),
            ),
            PresentationMessage::MediaMetadataExtractSourceTitle {
                source,
                blob_id,
                kind,
            } => (
                Code::TaskNameMediaMetadataExtractSource,
                Params::new()
                    .with("source", json!(source))
                    .with("blobId", json!(blob_id))
                    .with("kind", json!(kind.as_str()))
                    .finish(),
            ),
            PresentationMessage::MediaMetadataExtractBlobTitle { blob_id, kind } => (
                Code::TaskNameMediaMetadataExtractBlob,
                Params::new()
                    .with("blobId", json!(blob_id))
                    .with("kind", json!(kind.as_str()))
                    .finish(),
            ),
            PresentationMessage::TrashPurgeAllTitle => {
                (Code::TaskNameTrashPurgeAll, BTreeMap::new())
            }
            PresentationMessage::StoragePolicyTempCleanupTitle { policy, policy_id } => (
                Code::TaskNameStoragePolicyTempCleanup,
                Params::new()
                    .with("policy", json!(policy))
                    .with("policyId", json!(policy_id))
                    .finish(),
            ),
            PresentationMessage::StoragePolicyTempCleanupPolicyIdTitle { policy_id } => (
                Code::TaskNameStoragePolicyTempCleanupPolicyId,
                Params::new().with("policyId", json!(policy_id)).finish(),
            ),
            PresentationMessage::StoragePolicyMigrationTitle {
                source_policy_id,
                target_policy_id,
            } => (
                Code::TaskNameStoragePolicyMigration,
                Params::new()
                    .with("sourcePolicyId", json!(source_policy_id))
                    .with("targetPolicyId", json!(target_policy_id))
                    .finish(),
            ),
            PresentationMessage::BlobMaintenanceTitle {
                action,
                selected_blob_count,
            } => (
                match action {
                    BlobMaintenanceAction::IntegrityCheck => {
                        Code::BlobMaintenanceIntegrityCheckName
                    }
                    BlobMaintenanceAction::RefCountReconcile => {
                        Code::BlobMaintenanceRefCountReconcileName
                    }
                    BlobMaintenanceAction::OrphanCleanup => Code::BlobMaintenanceOrphanCleanupName,
                },
                match selected_blob_count {
                    Some(count) => Params::new().with("count", json!(count)).finish(),
                    None => BTreeMap::new(),
                },
            ),
            PresentationMessage::RuntimeTaskTitle { kind } => {
                (kind.presentation_code(), BTreeMap::new())
            }
            PresentationMessage::OfflineDownloadSourceTitle {
                filename,
                source,
                engine,
            } => match engine {
                Some(engine) => (
                    Code::TaskNameOfflineDownloadSourceWithEngine,
                    Params::new()
                        .with("filename", json!(filename))
                        .with("source", json!(source))
                        .with("engine", json!(engine.as_str()))
                        .finish(),
                ),
                None => (
                    Code::TaskNameOfflineDownloadSource,
                    Params::new()
                        .with("filename", json!(filename))
                        .with("source", json!(source))
                        .finish(),
                ),
            },
            PresentationMessage::OfflineDownloadTargetFolderTitle {
                source,
                target_folder_id,
                engine,
            } => match engine {
                Some(engine) => (
                    Code::TaskNameOfflineDownloadTargetFolderWithEngine,
                    Params::new()
                        .with("source", json!(source))
                        .with("targetFolderId", json!(target_folder_id))
                        .with("engine", json!(engine.as_str()))
                        .finish(),
                ),
                None => (
                    Code::TaskNameOfflineDownloadTargetFolder,
                    Params::new()
                        .with("source", json!(source))
                        .with("targetFolderId", json!(target_folder_id))
                        .finish(),
                ),
            },
            PresentationMessage::OfflineDownloadUrlTitle { source, engine } => match engine {
                Some(engine) => (
                    Code::TaskNameOfflineDownloadUrlWithEngine,
                    Params::new()
                        .with("source", json!(source))
                        .with("engine", json!(engine.as_str()))
                        .finish(),
                ),
                None => (
                    Code::TaskNameOfflineDownloadUrl,
                    Params::new().with("source", json!(source)).finish(),
                ),
            },
            PresentationMessage::ArchiveReadyStatus {
                name,
                target_file_id,
                target_path,
            } => (
                Code::StatusTextArchiveReady,
                Params::new()
                    .with("name", json!(name))
                    .with("targetFileId", json!(target_file_id))
                    .with("targetPath", json!(target_path))
                    .finish(),
            ),
            PresentationMessage::ArchiveExtractedStatus {
                name,
                target_folder_id,
                target_path,
                file_count,
                folder_count,
            } => (
                Code::StatusTextArchiveExtracted,
                Params::new()
                    .with("name", json!(name))
                    .with("targetFolderId", json!(target_folder_id))
                    .with("targetPath", json!(target_path))
                    .with("fileCount", json!(file_count))
                    .with("folderCount", json!(folder_count))
                    .finish(),
            ),
            PresentationMessage::ArchivePreviewReadyStatus => {
                (Code::StatusTextArchivePreviewReady, BTreeMap::new())
            }
            PresentationMessage::ThumbnailAlreadyAvailableStatus => {
                (Code::StatusTextThumbnailAlreadyAvailable, BTreeMap::new())
            }
            PresentationMessage::ThumbnailReadyStatus => {
                (Code::StatusTextThumbnailReady, BTreeMap::new())
            }
            PresentationMessage::ImagePreviewAlreadyAvailableStatus => (
                Code::StatusTextImagePreviewAlreadyAvailable,
                BTreeMap::new(),
            ),
            PresentationMessage::ImagePreviewReadyStatus => {
                (Code::StatusTextImagePreviewReady, BTreeMap::new())
            }
            PresentationMessage::MediaMetadataStatus { status } => {
                (media_metadata_status_code(status), BTreeMap::new())
            }
            PresentationMessage::TrashPurgedStatus { purged } => (
                Code::StatusTextTrashPurged,
                Params::new().with("purged", json!(purged)).finish(),
            ),
            PresentationMessage::TemporaryUploadCleanupFinishedStatus {
                deleted_objects,
                missing_objects,
                failed_objects,
            } => (
                Code::StatusTextTemporaryUploadCleanupFinished,
                Params::new()
                    .with("deletedObjects", json!(deleted_objects))
                    .with("missingObjects", json!(missing_objects))
                    .with("failedObjects", json!(failed_objects))
                    .finish(),
            ),
            PresentationMessage::StorageMigrationCompletedStatus {
                source_policy_id,
                target_policy_id,
                scanned_blobs,
                migrated_blobs,
                merged_blobs,
                skipped_blobs,
                failed_blobs,
                migrated_bytes,
                renamed_opaque_blobs,
            } => (
                Code::StatusTextStorageMigrationCompleted,
                Params::new()
                    .with("sourcePolicyId", json!(source_policy_id))
                    .with("targetPolicyId", json!(target_policy_id))
                    .with("scannedBlobs", json!(scanned_blobs))
                    .with("migratedBlobs", json!(migrated_blobs))
                    .with("mergedBlobs", json!(merged_blobs))
                    .with("skippedBlobs", json!(skipped_blobs))
                    .with("failedBlobs", json!(failed_blobs))
                    .with("migratedBytes", json!(migrated_bytes))
                    .with("renamedOpaqueBlobs", json!(renamed_opaque_blobs))
                    .finish(),
            ),
            PresentationMessage::BlobMaintenanceFinishedStatus => {
                (Code::StatusTextBlobMaintenanceFinished, BTreeMap::new())
            }
            PresentationMessage::OfflineDownloadImportedStatus {
                name,
                target_path,
                content_length,
            } => (
                Code::StatusTextOfflineDownloadImported,
                Params::new()
                    .with("name", json!(name))
                    .with("targetPath", json!(target_path))
                    .with("contentLength", json!(content_length))
                    .finish(),
            ),
            PresentationMessage::WaitingPresignedUrlExpiryStatus => {
                (Code::StatusTextWaitingPresignedUrlExpiry, BTreeMap::new())
            }
            PresentationMessage::SystemHealthyStatus => {
                (Code::StatusTextSystemHealthy, BTreeMap::new())
            }
            PresentationMessage::RuntimeSystemHealthIssueStatus { status, components } => (
                Code::RuntimeSystemHealthIssueDetail,
                Params::new()
                    .with("status", json!(status))
                    .with("components", json!(components))
                    .finish(),
            ),
        };
        TaskPresentationMessage { code, params }
    }
}

#[cfg(test)]
mod tests {
    use super::Params;
    use serde_json::json;

    #[test]
    fn params_builder_owns_values_and_keeps_btree_order() {
        let params = Params::new()
            .with("z", json!({"nested": [1, 2, 3]}))
            .with("a", json!(42))
            .finish();

        assert_eq!(
            params.keys().map(String::as_str).collect::<Vec<_>>(),
            ["a", "z"]
        );
        assert_eq!(params["a"], json!(42));
        assert_eq!(params["z"], json!({"nested": [1, 2, 3]}));
    }
}

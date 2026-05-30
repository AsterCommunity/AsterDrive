//! Stable API presentation metadata for background tasks.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::types::{
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
) -> Option<TaskPresentation> {
    let title = title_message(payload);
    let status = status_message(payload, result, status);

    Some(TaskPresentation { title, status })
}

fn title_message(payload: &TaskPayload) -> Option<TaskPresentationMessage> {
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

fn params<const N: usize>(entries: [(&str, Value); N]) -> BTreeMap<String, Value> {
    entries
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect()
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
                params([("name", json!(name))]),
            ),
            PresentationMessage::ArchiveExtractTitle { name } => (
                Code::TaskNameArchiveExtract,
                params([("name", json!(name))]),
            ),
            PresentationMessage::ArchivePreviewGenerateTitle { name } => (
                Code::TaskNameArchivePreviewGenerate,
                params([("name", json!(name))]),
            ),
            PresentationMessage::ArchivePreviewGenerateFileIdTitle { file_id, blob_id } => (
                Code::TaskNameArchivePreviewGenerateFileId,
                params([("fileId", json!(file_id)), ("blobId", json!(blob_id))]),
            ),
            PresentationMessage::ThumbnailGenerateSourceTitle {
                source,
                blob_id,
                processor,
            } => (
                Code::TaskNameThumbnailGenerate,
                params([
                    ("source", json!(source)),
                    ("blobId", json!(blob_id)),
                    ("processor", json!(processor.as_str())),
                ]),
            ),
            PresentationMessage::ThumbnailGenerateBlobTitle { blob_id, processor } => (
                Code::TaskNameThumbnailGenerateBlobWithProcessor,
                params([
                    ("blobId", json!(blob_id)),
                    ("processor", json!(processor.as_str())),
                ]),
            ),
            PresentationMessage::MediaMetadataExtractSourceTitle {
                source,
                blob_id,
                kind,
            } => (
                Code::TaskNameMediaMetadataExtractSource,
                params([
                    ("source", json!(source)),
                    ("blobId", json!(blob_id)),
                    ("kind", json!(kind.as_str())),
                ]),
            ),
            PresentationMessage::MediaMetadataExtractBlobTitle { blob_id, kind } => (
                Code::TaskNameMediaMetadataExtractBlob,
                params([("blobId", json!(blob_id)), ("kind", json!(kind.as_str()))]),
            ),
            PresentationMessage::TrashPurgeAllTitle => {
                (Code::TaskNameTrashPurgeAll, BTreeMap::new())
            }
            PresentationMessage::StoragePolicyTempCleanupTitle { policy, policy_id } => (
                Code::TaskNameStoragePolicyTempCleanup,
                params([("policy", json!(policy)), ("policyId", json!(policy_id))]),
            ),
            PresentationMessage::StoragePolicyTempCleanupPolicyIdTitle { policy_id } => (
                Code::TaskNameStoragePolicyTempCleanupPolicyId,
                params([("policyId", json!(policy_id))]),
            ),
            PresentationMessage::StoragePolicyMigrationTitle {
                source_policy_id,
                target_policy_id,
            } => (
                Code::TaskNameStoragePolicyMigration,
                params([
                    ("sourcePolicyId", json!(source_policy_id)),
                    ("targetPolicyId", json!(target_policy_id)),
                ]),
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
                    Some(count) => params([("count", json!(count))]),
                    None => BTreeMap::new(),
                },
            ),
            PresentationMessage::RuntimeTaskTitle { kind } => {
                (kind.presentation_code(), BTreeMap::new())
            }
            PresentationMessage::ArchiveReadyStatus {
                name,
                target_file_id,
                target_path,
            } => (
                Code::StatusTextArchiveReady,
                params([
                    ("name", json!(name)),
                    ("targetFileId", json!(target_file_id)),
                    ("targetPath", json!(target_path)),
                ]),
            ),
            PresentationMessage::ArchiveExtractedStatus {
                name,
                target_folder_id,
                target_path,
                file_count,
                folder_count,
            } => (
                Code::StatusTextArchiveExtracted,
                params([
                    ("name", json!(name)),
                    ("targetFolderId", json!(target_folder_id)),
                    ("targetPath", json!(target_path)),
                    ("fileCount", json!(file_count)),
                    ("folderCount", json!(folder_count)),
                ]),
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
            PresentationMessage::MediaMetadataStatus { status } => {
                (media_metadata_status_code(status), BTreeMap::new())
            }
            PresentationMessage::TrashPurgedStatus { purged } => (
                Code::StatusTextTrashPurged,
                params([("purged", json!(purged))]),
            ),
            PresentationMessage::TemporaryUploadCleanupFinishedStatus {
                deleted_objects,
                missing_objects,
                failed_objects,
            } => (
                Code::StatusTextTemporaryUploadCleanupFinished,
                params([
                    ("deletedObjects", json!(deleted_objects)),
                    ("missingObjects", json!(missing_objects)),
                    ("failedObjects", json!(failed_objects)),
                ]),
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
                params([
                    ("sourcePolicyId", json!(source_policy_id)),
                    ("targetPolicyId", json!(target_policy_id)),
                    ("scannedBlobs", json!(scanned_blobs)),
                    ("migratedBlobs", json!(migrated_blobs)),
                    ("mergedBlobs", json!(merged_blobs)),
                    ("skippedBlobs", json!(skipped_blobs)),
                    ("failedBlobs", json!(failed_blobs)),
                    ("migratedBytes", json!(migrated_bytes)),
                    ("renamedOpaqueBlobs", json!(renamed_opaque_blobs)),
                ]),
            ),
            PresentationMessage::BlobMaintenanceFinishedStatus => {
                (Code::StatusTextBlobMaintenanceFinished, BTreeMap::new())
            }
            PresentationMessage::WaitingPresignedUrlExpiryStatus => {
                (Code::StatusTextWaitingPresignedUrlExpiry, BTreeMap::new())
            }
            PresentationMessage::SystemHealthyStatus => {
                (Code::StatusTextSystemHealthy, BTreeMap::new())
            }
            PresentationMessage::RuntimeSystemHealthIssueStatus { status, components } => (
                Code::RuntimeSystemHealthIssueDetail,
                params([("status", json!(status)), ("components", json!(components))]),
            ),
        };
        TaskPresentationMessage { code, params }
    }
}

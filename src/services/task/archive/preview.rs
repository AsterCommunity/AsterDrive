//! 归档预览任务子模块。

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::file_repo;
use crate::entities::{background_task, file, file_blob};
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState, TaskRuntimeState};
use crate::services::{
    files::archive::preview,
    task::{
        TaskExecutionContext, cleanup_task_temp_dir_for_task_kind, create_typed_task_record,
        mark_task_progress, mark_task_succeeded, prepare_task_temp_dir,
        spec::{self, ArchivePreviewGenerateTask, decode_payload_as},
        steps::{
            TASK_STEP_DOWNLOAD_SOURCE, TASK_STEP_PERSIST_MANIFEST, TASK_STEP_SCAN_ARCHIVE,
            TASK_STEP_WAITING, parse_task_steps_json, set_task_step_active,
            set_task_step_succeeded,
        },
        types::{ArchivePreviewTaskPayload, ArchivePreviewTaskResult},
    },
    workspace::storage::WorkspaceStorageScope,
};
use crate::types::{BackgroundTaskKind, BackgroundTaskStatus};

pub(crate) async fn ensure_archive_preview_task(
    state: &impl TaskRuntimeState,
    source_file: &file::Model,
    blob: &file_blob::Model,
    limit_signature: &str,
) -> Result<()> {
    let display_name =
        archive_preview_task_display_name(source_file.id, blob.id, &blob.hash, limit_signature);
    if let Some(existing) =
        crate::db::repository::background_task_repo::find_latest_by_kind_and_display_name(
            state.writer_db(),
            BackgroundTaskKind::ArchivePreviewGenerate,
            &display_name,
        )
        .await?
    {
        match existing.status {
            BackgroundTaskStatus::Pending
            | BackgroundTaskStatus::Processing
            | BackgroundTaskStatus::Retry => {
                state.wake_background_task_dispatcher();
                return Ok(());
            }
            BackgroundTaskStatus::Failed => {
                return Err(preview::map_failed_task_error(
                    existing.last_error.as_deref(),
                ));
            }
            BackgroundTaskStatus::Succeeded | BackgroundTaskStatus::Canceled => {}
        }
    }

    let payload = ArchivePreviewTaskPayload {
        file_id: source_file.id,
        source_file_name: source_file.name.clone(),
        source_blob_id: blob.id,
        source_hash: blob.hash.clone(),
        limit_signature: limit_signature.to_string(),
    };
    let scope = archive_preview_task_scope(source_file)?;
    create_typed_task_record::<ArchivePreviewGenerateTask>(state, scope, &display_name, &payload)
        .await?;
    Ok(())
}

fn archive_preview_task_scope(source_file: &file::Model) -> Result<WorkspaceStorageScope> {
    let actor_user_id = source_file
        .created_by_user_id
        .or(source_file.owner_user_id)
        .ok_or_else(|| {
            AsterError::internal_error(format!(
                "archive preview source file #{} has no actor user",
                source_file.id
            ))
        })?;
    Ok(match source_file.team_id {
        Some(team_id) => WorkspaceStorageScope::Team {
            team_id,
            actor_user_id,
        },
        None => WorkspaceStorageScope::Personal {
            user_id: source_file.owner_user_id.unwrap_or(actor_user_id),
        },
    })
}

fn archive_preview_task_display_name(
    file_id: i64,
    blob_id: i64,
    source_hash: &str,
    limit_signature: &str,
) -> String {
    let source_hash_short = source_hash.chars().take(12).collect::<String>();
    let limit_hash = crate::utils::hash::sha256_hex(limit_signature.as_bytes());
    let limit_hash_short = limit_hash.chars().take(12).collect::<String>();
    format!(
        "Generate archive preview for file #{file_id} blob #{blob_id} {source_hash_short} {limit_hash_short}"
    )
}

pub(super) async fn process_archive_preview_task(
    state: &PrimaryAppState,
    task: &background_task::Model,
    context: TaskExecutionContext,
) -> Result<()> {
    let lease_guard = context.lease_guard().clone();
    let result = async {
        let payload = decode_payload_as::<ArchivePreviewGenerateTask>(task)?;
        let mut steps =
            parse_task_steps_json(task.steps_json.as_ref().map(|raw| raw.as_ref()), task.kind)?;
        set_task_step_succeeded(
            &mut steps,
            TASK_STEP_WAITING,
            Some("Worker claimed task"),
            None,
        )?;
        let source_file = file_repo::find_by_id(state.writer_db(), payload.file_id).await?;
        ensure_source_file_matches_payload(&source_file, &payload)?;
        let archive_format = preview::ensure_archive_preview_source_supported(&source_file)?;
        let limits = preview::ArchivePreviewLimits::from_runtime_config(
            state.runtime_config(),
            crate::types::ArchiveFilenameEncoding::Auto,
            archive_format,
        )?;
        if limits.task_signature != payload.limit_signature {
            return Err(preview::archive_preview_validation_error(
                ApiErrorCode::ArchivePreviewRejected,
                "archive preview limits changed before generation completed",
            ));
        }
        if source_file.size > limits.max_source_bytes {
            return Err(preview::archive_preview_validation_error(
                ApiErrorCode::ArchivePreviewSourceTooLarge,
                format!(
                    "source archive size {} exceeds archive preview limit {}",
                    source_file.size, limits.max_source_bytes
                ),
            ));
        }
        let blob = file_repo::find_blob_by_id(state.writer_db(), source_file.blob_id).await?;
        ensure_source_blob_matches_payload(&blob, &payload)?;

        let policy = state.policy_snapshot().get_policy_or_err(blob.policy_id)?;
        let driver = state.driver_registry().get_driver(&policy)?;
        let use_range_scan = driver.supports_efficient_range();
        let source_archive_path = if use_range_scan {
            context.ensure_active()?;
            set_task_step_succeeded(
                &mut steps,
                TASK_STEP_DOWNLOAD_SOURCE,
                Some("Source archive range reader ready"),
                Some((1, 4)),
            )?;
            None
        } else {
            context.ensure_active()?;
            set_task_step_active(
                &mut steps,
                TASK_STEP_DOWNLOAD_SOURCE,
                Some("Downloading source archive"),
                None,
            )?;
            mark_task_progress(
                state,
                &lease_guard,
                0,
                4,
                Some("Downloading source archive"),
                &steps,
            )
            .await?;
            let task_temp_dir = prepare_task_temp_dir(state, lease_guard.lease()).await?;
            let source_archive_path =
                std::path::Path::new(&task_temp_dir).join(archive_format.temp_file_name());
            preview::download_blob_to_temp(
                state,
                &context,
                &source_file,
                &blob,
                &source_archive_path,
            )
            .await?;
            set_task_step_succeeded(
                &mut steps,
                TASK_STEP_DOWNLOAD_SOURCE,
                Some("Source archive downloaded"),
                Some((1, 4)),
            )?;
            Some(source_archive_path)
        };
        context.ensure_active()?;
        set_task_step_active(
            &mut steps,
            TASK_STEP_SCAN_ARCHIVE,
            Some("Scanning archive manifest"),
            Some((1, 4)),
        )?;
        mark_task_progress(
            state,
            &lease_guard,
            1,
            4,
            Some("Scanning archive manifest"),
            &steps,
        )
        .await?;
        let manifest = match source_archive_path {
            Some(source_archive_path) => {
                preview::scan_manifest_from_temp(&source_file, &blob, &source_archive_path, &limits)
                    .await?
            }
            None => {
                preview::scan_manifest_from_storage_range(&source_file, &blob, driver, &limits)
                    .await?
            }
        };
        context.ensure_active()?;
        set_task_step_succeeded(
            &mut steps,
            TASK_STEP_SCAN_ARCHIVE,
            Some("Archive manifest scanned"),
            Some((3, 4)),
        )?;
        set_task_step_active(
            &mut steps,
            TASK_STEP_PERSIST_MANIFEST,
            Some("Saving archive manifest"),
            Some((3, 4)),
        )?;
        mark_task_progress(
            state,
            &lease_guard,
            3,
            4,
            Some("Saving archive manifest"),
            &steps,
        )
        .await?;

        preview::store_cached_manifest(state, &source_file, &blob, &limits, &manifest).await?;
        cleanup_task_temp_dir_for_task_kind(state, task.kind, task.id).await?;
        set_task_step_succeeded(
            &mut steps,
            TASK_STEP_PERSIST_MANIFEST,
            Some("Archive manifest ready"),
            Some((4, 4)),
        )?;

        let result = ArchivePreviewTaskResult {
            file_id: source_file.id,
            source_blob_id: blob.id,
            source_hash: blob.hash.clone(),
            entry_count: manifest.entry_count,
            file_count: manifest.file_count,
            directory_count: manifest.directory_count,
            truncated: manifest.entries_truncated(),
        };
        let result_json = spec::serialize_result::<ArchivePreviewGenerateTask>(&result)?;
        mark_task_succeeded(
            state,
            &lease_guard,
            Some(&result_json),
            4,
            4,
            Some("Archive preview ready"),
            &steps,
        )
        .await
    }
    .await;

    if result.is_err()
        && let Err(cleanup_error) =
            cleanup_task_temp_dir_for_task_kind(state, task.kind, task.id).await
    {
        tracing::warn!(
            task_id = task.id,
            "failed to cleanup archive preview temp dir after error: {cleanup_error}"
        );
    }
    result
}

fn ensure_source_file_matches_payload(
    source_file: &file::Model,
    payload: &ArchivePreviewTaskPayload,
) -> Result<()> {
    if source_file.deleted_at.is_some() {
        return Err(AsterError::file_not_found(format!(
            "archive preview source file #{} is deleted",
            source_file.id
        )));
    }
    if source_file.blob_id != payload.source_blob_id {
        return Err(preview::archive_preview_validation_error(
            ApiErrorCode::ArchivePreviewRejected,
            "archive preview source changed before generation completed",
        ));
    }
    Ok(())
}

fn ensure_source_blob_matches_payload(
    blob: &file_blob::Model,
    payload: &ArchivePreviewTaskPayload,
) -> Result<()> {
    if blob.id != payload.source_blob_id || blob.hash != payload.source_hash {
        return Err(preview::archive_preview_validation_error(
            ApiErrorCode::ArchivePreviewRejected,
            "archive preview source blob changed before generation completed",
        ));
    }
    Ok(())
}

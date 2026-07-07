//! 后台任务服务子模块：`media_metadata`。

use crate::db::repository::background_task_repo;
use crate::entities::{background_task, file, file_blob};
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState, TaskRuntimeState};
use crate::services::media::metadata;
use crate::storage::StorageErrorKind;
use crate::types::{
    BackgroundTaskKind, BackgroundTaskStatus, MediaMetadataKind, MediaMetadataStatus,
};

use super::retry::{TaskRetryClass, TaskRetryPolicy};
use super::spec::{self, MediaMetadataExtractTask, decode_payload_as};
use super::steps::{
    TASK_STEP_EXTRACT_METADATA, TASK_STEP_INSPECT_SOURCE, TASK_STEP_PERSIST_METADATA,
    TASK_STEP_WAITING, parse_task_steps_json, set_task_step_active, set_task_step_succeeded,
};
use super::types::{MediaMetadataExtractTaskPayload, MediaMetadataExtractTaskResult};
use super::{
    TaskExecutionContext, TypedTaskCreate, insert_typed_task_record, mark_task_progress,
    mark_task_succeeded,
};

pub(super) struct MediaMetadataRetryPolicy;

impl TaskRetryPolicy for MediaMetadataRetryPolicy {
    fn retry_class(error: &AsterError) -> TaskRetryClass {
        match error {
            AsterError::DatabaseConnection(_) | AsterError::DatabaseOperation(_) => {
                TaskRetryClass::Auto
            }
            AsterError::StorageDriverError(_) => match error.storage_error_kind() {
                Some(StorageErrorKind::Transient | StorageErrorKind::RateLimited) => {
                    TaskRetryClass::Auto
                }
                _ => TaskRetryClass::Never,
            },
            _ => TaskRetryClass::Never,
        }
    }
}

pub(crate) async fn ensure_media_metadata_task(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    source_file: &file::Model,
    kind: MediaMetadataKind,
) -> Result<()> {
    let display_name = metadata::task_display_name(blob.id, kind);
    if let Some(existing) = background_task_repo::find_latest_by_kind_and_display_name(
        state.writer_db(),
        BackgroundTaskKind::MediaMetadataExtract,
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
                return Ok(());
            }
            BackgroundTaskStatus::Succeeded | BackgroundTaskStatus::Canceled => {}
        }
    }

    let payload = MediaMetadataExtractTaskPayload {
        blob_id: blob.id,
        blob_hash: blob.hash.clone(),
        source_file_name: source_file.name.clone(),
        source_mime_type: source_file.mime_type.clone(),
        media_kind: kind,
    };
    insert_typed_task_record(
        state,
        state.writer_db(),
        TypedTaskCreate::<MediaMetadataExtractTask>::new(display_name, payload).progress(0, 4),
    )
    .await?;

    state.wake_background_task_dispatcher();
    Ok(())
}

pub(super) async fn process_media_metadata_extract_task(
    state: &PrimaryAppState,
    task: &background_task::Model,
    context: TaskExecutionContext,
) -> Result<()> {
    let lease_guard = context.lease_guard().clone();
    let payload = decode_payload_as::<MediaMetadataExtractTask>(task)?;
    let mut steps =
        parse_task_steps_json(task.steps_json.as_ref().map(|raw| raw.as_ref()), task.kind)?;
    set_task_step_succeeded(
        &mut steps,
        TASK_STEP_WAITING,
        Some("Worker claimed task"),
        Some((1, 4)),
    )?;
    set_task_step_active(
        &mut steps,
        TASK_STEP_INSPECT_SOURCE,
        Some("Loading source blob"),
        Some((1, 4)),
    )?;
    mark_task_progress(
        state,
        &lease_guard,
        1,
        4,
        Some("Loading source blob"),
        &steps,
    )
    .await?;

    let blob =
        crate::db::repository::file_repo::find_blob_by_id(state.writer_db(), payload.blob_id)
            .await?;
    if blob.hash != payload.blob_hash {
        return Err(AsterError::validation_error("source blob hash changed"));
    }
    context.ensure_active()?;

    set_task_step_succeeded(
        &mut steps,
        TASK_STEP_INSPECT_SOURCE,
        Some("Source blob is ready"),
        Some((2, 4)),
    )?;
    set_task_step_active(
        &mut steps,
        TASK_STEP_EXTRACT_METADATA,
        Some("Extracting media metadata"),
        Some((2, 4)),
    )?;
    mark_task_progress(
        state,
        &lease_guard,
        2,
        4,
        Some("Extracting media metadata"),
        &steps,
    )
    .await?;

    let extracted = match metadata::extract_for_blob(
        state,
        &blob,
        &payload.source_file_name,
        &payload.source_mime_type,
        payload.media_kind,
    )
    .await
    {
        Ok(extracted) => extracted,
        Err(error) => metadata::ExtractedMediaMetadata {
            kind: payload.media_kind,
            status: MediaMetadataStatus::Failed,
            metadata: None,
            error_message: Some(metadata::cache_error_message(&error)),
            parser: parser_name_for_kind(payload.media_kind).to_string(),
            parser_version: "1".to_string(),
        },
    };
    context.ensure_active()?;

    set_task_step_succeeded(
        &mut steps,
        TASK_STEP_EXTRACT_METADATA,
        Some(metadata::result_status_text(extracted.status)),
        Some((3, 4)),
    )?;
    set_task_step_active(
        &mut steps,
        TASK_STEP_PERSIST_METADATA,
        Some("Persisting media metadata"),
        Some((3, 4)),
    )?;
    mark_task_progress(
        state,
        &lease_guard,
        3,
        4,
        Some("Persisting media metadata"),
        &steps,
    )
    .await?;

    let record = metadata::persist_extracted(state, &blob, extracted).await?;
    context.ensure_active()?;
    set_task_step_succeeded(
        &mut steps,
        TASK_STEP_PERSIST_METADATA,
        Some("Media metadata cached"),
        Some((4, 4)),
    )?;

    let result_json =
        spec::serialize_result::<MediaMetadataExtractTask>(&MediaMetadataExtractTaskResult {
            blob_id: blob.id,
            media_kind: record.kind,
            status: record.status,
            parser: record.parser.clone(),
        })?;
    mark_task_succeeded(
        state,
        &lease_guard,
        Some(&result_json),
        4,
        4,
        Some(metadata::result_status_text(record.status)),
        &steps,
    )
    .await
}

fn parser_name_for_kind(kind: MediaMetadataKind) -> &'static str {
    match kind {
        MediaMetadataKind::Image => "image",
        MediaMetadataKind::Audio => "lofty",
        MediaMetadataKind::Video => "ffprobe",
    }
}

//! 后台任务服务子模块：`thumbnail`。

use crate::db::repository::background_task_repo;
use crate::entities::{background_task, file_blob};
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::services::media_processing_service;
use crate::storage::StorageErrorKind;
use crate::types::{BackgroundTaskKind, BackgroundTaskStatus};
use crate::utils::numbers::usize_to_i64;

use super::retry::{TaskRetryClass, TaskRetryPolicy};
use super::spec::ImagePreviewGenerateTask;
use super::spec::{self, BackgroundTaskSpec, ThumbnailGenerateTask, decode_payload_as};
use super::steps::{
    TASK_STEP_INSPECT_SOURCE, TASK_STEP_PERSIST_THUMBNAIL, TASK_STEP_RENDER_THUMBNAIL,
    TASK_STEP_WAITING, parse_task_steps_json, set_task_step_active, set_task_step_succeeded,
};
use super::types::{
    ImagePreviewGenerateTaskPayload, ImagePreviewGenerateTaskResult, ThumbnailGenerateTaskPayload,
    ThumbnailGenerateTaskResult,
};
use super::{
    TaskExecutionContext, TypedTaskCreate, insert_typed_task_record, mark_task_progress,
    mark_task_succeeded,
};

pub(super) struct ThumbnailRetryPolicy;

fn thumbnail_step_count() -> Result<i64> {
    usize_to_i64(
        ThumbnailGenerateTask::step_specs().len(),
        "thumbnail task step count",
    )
}

impl TaskRetryPolicy for ThumbnailRetryPolicy {
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

pub(crate) async fn ensure_thumbnail_task(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    source_file_name: &str,
    source_mime_type: &str,
) -> Result<()> {
    let processor = media_processing_service::resolve_thumbnail_processor_for_blob(
        state,
        blob,
        source_file_name,
        source_mime_type,
    )?
    .kind();
    tracing::debug!(
        blob_id = blob.id,
        source_file_name,
        processor = processor.as_str(),
        source_mime_type,
        "ensuring thumbnail background task"
    );
    let display_name = thumbnail_task_display_name(blob.id, processor);
    if let Some(existing) = background_task_repo::find_latest_by_kind_and_display_name(
        state.writer_db(),
        BackgroundTaskKind::ThumbnailGenerate,
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
                return Err(AsterError::record_not_found(
                    "thumbnail is unavailable for this file",
                ));
            }
            BackgroundTaskStatus::Succeeded | BackgroundTaskStatus::Canceled => {}
        }
    }

    let payload = ThumbnailGenerateTaskPayload {
        blob_id: blob.id,
        blob_hash: blob.hash.clone(),
        source_file_name: source_file_name.to_string(),
        source_mime_type: source_mime_type.to_string(),
        processor,
    };
    insert_typed_task_record(
        state,
        state.writer_db(),
        TypedTaskCreate::<ThumbnailGenerateTask>::new(display_name, payload)
            .progress(0, thumbnail_step_count()?),
    )
    .await?;

    state.wake_background_task_dispatcher();

    Ok(())
}

pub(crate) async fn ensure_image_preview_task(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    source_file_name: &str,
    source_mime_type: &str,
) -> Result<()> {
    let processor = media_processing_service::resolve_thumbnail_processor_for_blob(
        state,
        blob,
        source_file_name,
        source_mime_type,
    )?
    .kind();
    tracing::debug!(
        blob_id = blob.id,
        source_file_name,
        processor = processor.as_str(),
        source_mime_type,
        "ensuring image preview background task"
    );
    let display_name = image_preview_task_display_name(blob.id, processor);
    if let Some(existing) = background_task_repo::find_latest_by_kind_and_display_name(
        state.writer_db(),
        BackgroundTaskKind::ImagePreviewGenerate,
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
                return Err(AsterError::record_not_found(
                    "image preview is unavailable for this file",
                ));
            }
            BackgroundTaskStatus::Succeeded | BackgroundTaskStatus::Canceled => {}
        }
    }

    let payload = ImagePreviewGenerateTaskPayload {
        blob_id: blob.id,
        blob_hash: blob.hash.clone(),
        source_file_name: source_file_name.to_string(),
        source_mime_type: source_mime_type.to_string(),
        processor,
    };
    insert_typed_task_record(
        state,
        state.writer_db(),
        TypedTaskCreate::<ImagePreviewGenerateTask>::new(display_name, payload)
            .progress(0, thumbnail_step_count()?),
    )
    .await?;

    state.wake_background_task_dispatcher();

    Ok(())
}

pub(super) async fn process_thumbnail_generate_task(
    state: &PrimaryAppState,
    task: &background_task::Model,
    context: TaskExecutionContext,
) -> Result<()> {
    let lease_guard = context.lease_guard().clone();
    let payload = decode_payload_as::<ThumbnailGenerateTask>(task)?;
    tracing::debug!(
        task_id = task.id,
        blob_id = payload.blob_id,
        source_file_name = payload.source_file_name,
        processor = payload.processor.as_str(),
        source_mime_type = payload.source_mime_type,
        "processing thumbnail background task"
    );
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
    context.ensure_active()?;
    set_task_step_succeeded(
        &mut steps,
        TASK_STEP_INSPECT_SOURCE,
        Some("Source blob is ready"),
        Some((2, 4)),
    )?;
    set_task_step_active(
        &mut steps,
        TASK_STEP_RENDER_THUMBNAIL,
        Some("Generating thumbnail"),
        Some((2, 4)),
    )?;
    mark_task_progress(
        state,
        &lease_guard,
        2,
        4,
        Some("Generating thumbnail"),
        &steps,
    )
    .await?;

    let stored = media_processing_service::generate_and_store_thumbnail_with_processor(
        state,
        &blob,
        &payload.source_file_name,
        &payload.source_mime_type,
        payload.processor,
    )
    .await?;
    tracing::debug!(
        task_id = task.id,
        blob_id = blob.id,
        processor = payload.processor.as_str(),
        reused_existing_thumbnail = stored.reused_existing_thumbnail,
        thumbnail_processor = stored.thumbnail_processor,
        thumbnail_version = stored.thumbnail_version,
        thumbnail_path = stored.thumbnail_path,
        "thumbnail background task completed render phase"
    );
    context.ensure_active()?;

    if stored.reused_existing_thumbnail {
        set_task_step_succeeded(
            &mut steps,
            TASK_STEP_RENDER_THUMBNAIL,
            Some("Existing thumbnail reused"),
            Some((3, 4)),
        )?;
        set_task_step_succeeded(
            &mut steps,
            TASK_STEP_PERSIST_THUMBNAIL,
            Some("Existing thumbnail reused"),
            Some((4, 4)),
        )?;
    } else {
        set_task_step_succeeded(
            &mut steps,
            TASK_STEP_RENDER_THUMBNAIL,
            Some("Thumbnail rendered"),
            Some((3, 4)),
        )?;
        set_task_step_succeeded(
            &mut steps,
            TASK_STEP_PERSIST_THUMBNAIL,
            Some("Thumbnail stored"),
            Some((4, 4)),
        )?;
    }

    let result_json =
        spec::serialize_result::<ThumbnailGenerateTask>(&ThumbnailGenerateTaskResult {
            blob_id: blob.id,
            thumbnail_path: stored.thumbnail_path.clone(),
            thumbnail_processor: stored.thumbnail_processor.clone(),
            thumbnail_version: stored.thumbnail_version.clone(),
            processor: payload.processor,
            reused_existing_thumbnail: stored.reused_existing_thumbnail,
        })?;
    let status_text = if stored.reused_existing_thumbnail {
        "Thumbnail already available"
    } else {
        "Thumbnail ready"
    };
    let step_count = thumbnail_step_count()?;
    mark_task_succeeded(
        state,
        &lease_guard,
        Some(&result_json),
        step_count,
        step_count,
        Some(status_text),
        &steps,
    )
    .await
}

pub(super) async fn process_image_preview_generate_task(
    state: &PrimaryAppState,
    task: &background_task::Model,
    context: TaskExecutionContext,
) -> Result<()> {
    let lease_guard = context.lease_guard().clone();
    let payload = decode_payload_as::<ImagePreviewGenerateTask>(task)?;
    tracing::debug!(
        task_id = task.id,
        blob_id = payload.blob_id,
        source_file_name = payload.source_file_name,
        processor = payload.processor.as_str(),
        source_mime_type = payload.source_mime_type,
        "processing image preview background task"
    );
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
    context.ensure_active()?;
    set_task_step_succeeded(
        &mut steps,
        TASK_STEP_INSPECT_SOURCE,
        Some("Source blob is ready"),
        Some((2, 4)),
    )?;
    set_task_step_active(
        &mut steps,
        TASK_STEP_RENDER_THUMBNAIL,
        Some("Generating image preview"),
        Some((2, 4)),
    )?;
    mark_task_progress(
        state,
        &lease_guard,
        2,
        4,
        Some("Generating image preview"),
        &steps,
    )
    .await?;

    let stored = media_processing_service::generate_and_store_image_preview_with_processor(
        state,
        &blob,
        &payload.source_file_name,
        &payload.source_mime_type,
        payload.processor,
    )
    .await?;
    tracing::debug!(
        task_id = task.id,
        blob_id = blob.id,
        processor = payload.processor.as_str(),
        reused_existing_preview = stored.reused_existing_preview,
        image_preview_processor = stored.image_preview_processor,
        image_preview_version = stored.image_preview_version,
        image_preview_path = stored.image_preview_path,
        "image preview background task completed render phase"
    );
    context.ensure_active()?;

    if stored.reused_existing_preview {
        set_task_step_succeeded(
            &mut steps,
            TASK_STEP_RENDER_THUMBNAIL,
            Some("Existing image preview reused"),
            Some((3, 4)),
        )?;
        set_task_step_succeeded(
            &mut steps,
            TASK_STEP_PERSIST_THUMBNAIL,
            Some("Existing image preview reused"),
            Some((4, 4)),
        )?;
    } else {
        set_task_step_succeeded(
            &mut steps,
            TASK_STEP_RENDER_THUMBNAIL,
            Some("Image preview rendered"),
            Some((3, 4)),
        )?;
        set_task_step_succeeded(
            &mut steps,
            TASK_STEP_PERSIST_THUMBNAIL,
            Some("Image preview stored"),
            Some((4, 4)),
        )?;
    }

    let result_json =
        spec::serialize_result::<ImagePreviewGenerateTask>(&ImagePreviewGenerateTaskResult {
            blob_id: blob.id,
            image_preview_path: stored.image_preview_path.clone(),
            image_preview_processor: stored.image_preview_processor.clone(),
            image_preview_version: stored.image_preview_version.clone(),
            processor: payload.processor,
            reused_existing_preview: stored.reused_existing_preview,
        })?;
    let status_text = if stored.reused_existing_preview {
        "Image preview already available"
    } else {
        "Image preview ready"
    };
    let step_count = thumbnail_step_count()?;
    mark_task_succeeded(
        state,
        &lease_guard,
        Some(&result_json),
        step_count,
        step_count,
        Some(status_text),
        &steps,
    )
    .await
}

fn thumbnail_task_display_name(
    blob_id: i64,
    processor: crate::types::MediaProcessorKind,
) -> String {
    format!(
        "Generate thumbnail for blob #{blob_id} via {}",
        thumbnail_processor_display_name(processor)
    )
}

fn image_preview_task_display_name(
    blob_id: i64,
    processor: crate::types::MediaProcessorKind,
) -> String {
    format!(
        "Generate image preview for blob #{blob_id} via {}",
        thumbnail_processor_display_name(processor)
    )
}

fn thumbnail_processor_display_name(processor: crate::types::MediaProcessorKind) -> &'static str {
    match processor {
        crate::types::MediaProcessorKind::Images => "AsterDrive built-in",
        crate::types::MediaProcessorKind::Lofty => "AsterDrive built-in audio",
        _ => processor.as_str(),
    }
}

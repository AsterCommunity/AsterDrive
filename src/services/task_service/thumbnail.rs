//! 后台任务服务子模块：`thumbnail`。

use chrono::Utc;
use sea_orm::Set;

use crate::db::repository::background_task_repo;
use crate::entities::{background_task, file_blob};
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::services::media_processing_service;
use crate::storage::StorageErrorKind;
use crate::types::{BackgroundTaskKind, BackgroundTaskStatus};

use super::retry::{TaskRetryClass, TaskRetryPolicy};
use super::steps::{
    TASK_STEP_INSPECT_SOURCE, TASK_STEP_PERSIST_THUMBNAIL, TASK_STEP_RENDER_THUMBNAIL,
    TASK_STEP_WAITING, initial_task_steps, parse_task_steps_json, serialize_task_steps,
    set_task_step_active, set_task_step_succeeded,
};
use super::types::{
    ThumbnailGenerateTaskPayload, ThumbnailGenerateTaskResult, parse_task_payload,
    serialize_task_payload, serialize_task_result,
};
use super::{
    TaskLeaseGuard, configured_task_max_attempts, mark_task_progress, mark_task_succeeded,
    task_expiration_from, truncate_display_name,
};

pub(super) struct ThumbnailRetryPolicy;

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
        &state.db,
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

    let now = Utc::now();
    let payload = ThumbnailGenerateTaskPayload {
        blob_id: blob.id,
        blob_hash: blob.hash.clone(),
        source_file_name: source_file_name.to_string(),
        source_mime_type: source_mime_type.to_string(),
        processor,
    };
    let payload_json = serialize_task_payload(&payload)?;
    let steps_json =
        serialize_task_steps(&initial_task_steps(BackgroundTaskKind::ThumbnailGenerate))?;
    background_task_repo::create(
        &state.db,
        background_task::ActiveModel {
            kind: Set(BackgroundTaskKind::ThumbnailGenerate),
            status: Set(BackgroundTaskStatus::Pending),
            creator_user_id: Set(None),
            team_id: Set(None),
            share_id: Set(None),
            display_name: Set(truncate_display_name(&display_name)),
            payload_json: Set(payload_json),
            result_json: Set(None),
            steps_json: Set(Some(steps_json)),
            progress_current: Set(0),
            progress_total: Set(4),
            status_text: Set(None),
            attempt_count: Set(0),
            max_attempts: Set(configured_task_max_attempts(
                state,
                BackgroundTaskKind::ThumbnailGenerate,
            )),
            next_run_at: Set(now),
            processing_token: Set(0),
            processing_started_at: Set(None),
            last_heartbeat_at: Set(None),
            lease_expires_at: Set(None),
            started_at: Set(None),
            finished_at: Set(None),
            last_error: Set(None),
            expires_at: Set(task_expiration_from(state, now)),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await?;

    state.wake_background_task_dispatcher();

    Ok(())
}

pub(super) async fn process_thumbnail_generate_task(
    state: &PrimaryAppState,
    task: &background_task::Model,
    lease_guard: TaskLeaseGuard,
) -> Result<()> {
    let payload: ThumbnailGenerateTaskPayload = parse_task_payload(task)?;
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
        crate::db::repository::file_repo::find_blob_by_id(&state.db, payload.blob_id).await?;
    lease_guard.ensure_active()?;
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
    lease_guard.ensure_active()?;

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

    let result_json = serialize_task_result(&ThumbnailGenerateTaskResult {
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
    mark_task_succeeded(
        state,
        &lease_guard,
        Some(&result_json),
        4,
        4,
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

fn thumbnail_processor_display_name(processor: crate::types::MediaProcessorKind) -> &'static str {
    match processor {
        crate::types::MediaProcessorKind::Images => "AsterDrive built-in",
        crate::types::MediaProcessorKind::Lofty => "AsterDrive built-in audio",
        _ => processor.as_str(),
    }
}

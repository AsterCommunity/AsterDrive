//! 后台任务服务子模块：`media_metadata`。

use chrono::Utc;
use sea_orm::Set;

use crate::db::repository::background_task_repo;
use crate::entities::{background_task, file, file_blob};
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::services::media_metadata_service;
use crate::storage::StorageErrorKind;
use crate::types::{
    BackgroundTaskKind, BackgroundTaskStatus, MediaMetadataKind, MediaMetadataStatus,
};

use super::retry::{TaskRetryClass, TaskRetryPolicy};
use super::steps::{
    TASK_STEP_EXTRACT_METADATA, TASK_STEP_INSPECT_SOURCE, TASK_STEP_PERSIST_METADATA,
    TASK_STEP_WAITING, initial_task_steps, parse_task_steps_json, serialize_task_steps,
    set_task_step_active, set_task_step_succeeded,
};
use super::types::{
    MediaMetadataExtractTaskPayload, MediaMetadataExtractTaskResult, parse_task_payload,
    serialize_task_payload, serialize_task_result,
};
use super::{
    TaskLeaseGuard, configured_task_max_attempts, mark_task_progress, mark_task_succeeded,
    task_expiration_from, truncate_display_name,
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
    let display_name = media_metadata_service::task_display_name(blob.id, kind);
    if let Some(existing) = background_task_repo::find_latest_by_kind_and_display_name(
        &state.db,
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

    let now = Utc::now();
    let payload = MediaMetadataExtractTaskPayload {
        blob_id: blob.id,
        blob_hash: blob.hash.clone(),
        source_file_name: source_file.name.clone(),
        source_mime_type: source_file.mime_type.clone(),
        kind,
    };
    let payload_json = serialize_task_payload(&payload)?;
    let steps_json = serialize_task_steps(&initial_task_steps(
        BackgroundTaskKind::MediaMetadataExtract,
    ))?;
    background_task_repo::create(
        &state.db,
        background_task::ActiveModel {
            kind: Set(BackgroundTaskKind::MediaMetadataExtract),
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
                BackgroundTaskKind::MediaMetadataExtract,
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

pub(super) async fn process_media_metadata_extract_task(
    state: &PrimaryAppState,
    task: &background_task::Model,
    lease_guard: TaskLeaseGuard,
) -> Result<()> {
    let payload: MediaMetadataExtractTaskPayload = parse_task_payload(task)?;
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
    if blob.hash != payload.blob_hash {
        return Err(AsterError::validation_error("source blob hash changed"));
    }
    lease_guard.ensure_active()?;

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

    let extracted = match media_metadata_service::extract_for_blob(
        state,
        &blob,
        &payload.source_file_name,
        &payload.source_mime_type,
        payload.kind,
    )
    .await
    {
        Ok(extracted) => extracted,
        Err(error) => media_metadata_service::ExtractedMediaMetadata {
            kind: payload.kind,
            status: MediaMetadataStatus::Failed,
            metadata: None,
            error_message: Some(media_metadata_service::cache_error_message(&error)),
            parser: parser_name_for_kind(payload.kind).to_string(),
            parser_version: "1".to_string(),
        },
    };
    lease_guard.ensure_active()?;

    set_task_step_succeeded(
        &mut steps,
        TASK_STEP_EXTRACT_METADATA,
        Some(media_metadata_service::result_status_text(extracted.status)),
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

    let record = media_metadata_service::persist_extracted(state, &blob, extracted).await?;
    lease_guard.ensure_active()?;
    set_task_step_succeeded(
        &mut steps,
        TASK_STEP_PERSIST_METADATA,
        Some("Media metadata cached"),
        Some((4, 4)),
    )?;

    let result_json = serialize_task_result(&MediaMetadataExtractTaskResult {
        blob_id: blob.id,
        kind: record.kind,
        status: record.status,
        parser: record.parser.clone(),
    })?;
    mark_task_succeeded(
        state,
        &lease_guard,
        Some(&result_json),
        4,
        4,
        Some(media_metadata_service::result_status_text(record.status)),
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

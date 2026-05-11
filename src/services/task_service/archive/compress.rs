//! 归档任务子模块：`compress`。

use std::path::Path;

use crate::entities::background_task;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::PrimaryAppState;
use crate::services::{
    batch_service,
    workspace_storage_service::{self, WorkspaceStorageScope},
};
use crate::types::BackgroundTaskKind;

use super::super::steps::{
    TASK_STEP_BUILD_ARCHIVE, TASK_STEP_PREPARE_SOURCES, TASK_STEP_STORE_RESULT, TASK_STEP_WAITING,
    parse_task_steps_json, set_task_step_active, set_task_step_succeeded,
};
use super::super::types::{
    ArchiveCompressTaskPayload, ArchiveCompressTaskResult, CreateArchiveCompressTaskParams,
    CreateArchiveTaskParams, TaskStepInfo, parse_task_payload, serialize_task_result,
};
use super::super::{
    TaskLeaseGuard, cleanup_task_temp_dir_for_task, create_task_record, mark_task_progress,
    mark_task_succeeded, prepare_task_temp_dir, task_scope,
};
use super::common::{ArchiveSinkContext, build_file_display_path, write_archive_to_sink};
use super::selection::{
    ArchiveBuildLimits, collect_archive_entries_from_selection_in_scope,
    ensure_archive_selection_active, resolve_archive_compress_target_folder_id,
    resolve_archive_download_in_scope,
};

pub(crate) async fn create_archive_compress_task_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    params: CreateArchiveCompressTaskParams,
) -> Result<super::super::TaskInfo> {
    let resolved = resolve_archive_download_in_scope(
        state,
        scope,
        &CreateArchiveTaskParams {
            file_ids: params.file_ids,
            folder_ids: params.folder_ids,
            archive_name: params.archive_name,
        },
    )
    .await?;
    let target_folder_id = resolve_archive_compress_target_folder_id(
        state,
        scope,
        &resolved.selection,
        params.target_folder_id,
    )
    .await?;
    let payload = ArchiveCompressTaskPayload {
        file_ids: resolved.selection.file_ids.clone(),
        folder_ids: resolved.selection.folder_ids.clone(),
        archive_name: resolved.archive_name.clone(),
        target_folder_id,
    };
    let display_name = format!("Compress {}", payload.archive_name);
    let task = create_task_record(
        state,
        scope,
        BackgroundTaskKind::ArchiveCompress,
        &display_name,
        &payload,
    )
    .await?;
    super::super::get_task_in_scope(state, scope, task.id).await
}

pub(super) async fn process_archive_compress_task(
    state: &PrimaryAppState,
    task: &background_task::Model,
    lease_guard: TaskLeaseGuard,
) -> Result<()> {
    let scope = task_scope(task)?;
    let payload: ArchiveCompressTaskPayload = parse_task_payload(task)?;
    let mut steps =
        parse_task_steps_json(task.steps_json.as_ref().map(|raw| raw.as_ref()), task.kind)?;
    set_task_step_succeeded(
        &mut steps,
        TASK_STEP_WAITING,
        Some("Worker claimed task"),
        None,
    )?;
    set_task_step_active(
        &mut steps,
        TASK_STEP_PREPARE_SOURCES,
        Some("Validating archive selection"),
        None,
    )?;
    mark_task_progress(
        state,
        &lease_guard,
        0,
        0,
        Some("Validating archive selection"),
        &steps,
    )
    .await?;
    if let Some(target_folder_id) = payload.target_folder_id {
        workspace_storage_service::verify_folder_access(state, scope, target_folder_id).await?;
    }

    let selection = batch_service::load_normalized_selection_in_scope(
        state,
        scope,
        &payload.file_ids,
        &payload.folder_ids,
    )
    .await?;
    ensure_archive_selection_active(scope, &selection)?;
    let limits = ArchiveBuildLimits::from_runtime_config(&state.runtime_config);
    let collected =
        collect_archive_entries_from_selection_in_scope(state, scope, &selection, limits).await?;
    let total_bytes = collected.total_source_bytes();
    let estimated_output_bytes = collected.estimated_output_bytes();
    if estimated_output_bytes > 0 {
        workspace_storage_service::check_quota(&state.db, scope, estimated_output_bytes).await?;
    }
    let entries = collected.into_entries();
    let progress_total = total_bytes.max(0);
    set_task_step_succeeded(
        &mut steps,
        TASK_STEP_PREPARE_SOURCES,
        Some("Archive sources are ready"),
        None,
    )?;
    set_task_step_active(
        &mut steps,
        TASK_STEP_BUILD_ARCHIVE,
        Some("Packing archive"),
        Some((0, progress_total)),
    )?;
    mark_task_progress(
        state,
        &lease_guard,
        0,
        progress_total,
        Some("Preparing archive"),
        &steps,
    )
    .await?;

    let task_temp_dir = prepare_task_temp_dir(state, lease_guard.lease()).await?;
    let archive_temp_path = Path::new(&task_temp_dir).join(&payload.archive_name);
    let archive_temp_path_string = archive_temp_path.to_string_lossy().to_string();
    let archive_temp_path_for_worker = archive_temp_path_string.clone();
    let handle = tokio::runtime::Handle::current();
    let db = state.db.clone();
    let driver_registry = state.driver_registry.clone();
    let policy_snapshot = state.policy_snapshot.clone();
    let lease_guard_for_worker = lease_guard.clone();
    let steps_for_worker = steps.clone();

    let (archive_size, mut steps) =
        tokio::task::spawn_blocking(move || -> Result<(i64, Vec<TaskStepInfo>)> {
            let file = std::fs::File::create(&archive_temp_path_for_worker)
                .map_aster_err_ctx("create archive temp file", AsterError::storage_driver_error)?;
            let writer = std::io::BufWriter::new(file);
            let mut steps = steps_for_worker;
            let (writer, _) = write_archive_to_sink(
                ArchiveSinkContext {
                    handle: &handle,
                    db: &db,
                    driver_registry: driver_registry.as_ref(),
                    policy_snapshot: policy_snapshot.as_ref(),
                    lease_guard: Some(&lease_guard_for_worker),
                },
                entries,
                progress_total,
                limits,
                writer,
                |current, entry_path| {
                    let status_text = format!("Packing {entry_path}");
                    set_task_step_active(
                        &mut steps,
                        TASK_STEP_BUILD_ARCHIVE,
                        Some(&status_text),
                        Some((current, progress_total)),
                    )?;
                    handle.block_on(async {
                        super::super::update_task_progress_db(
                            &db,
                            &lease_guard_for_worker,
                            current,
                            progress_total,
                            Some(&status_text),
                            &steps,
                        )
                        .await
                    })
                },
            )?;
            set_task_step_succeeded(
                &mut steps,
                TASK_STEP_BUILD_ARCHIVE,
                Some("Archive file created"),
                Some((progress_total, progress_total)),
            )?;
            writer
                .into_inner()
                .map_aster_err(AsterError::storage_driver_error)?;
            let metadata = std::fs::metadata(&archive_temp_path_for_worker).map_aster_err_ctx(
                "read archive temp file metadata",
                AsterError::storage_driver_error,
            )?;
            Ok((
                i64::try_from(metadata.len()).map_aster_err_with(|| {
                    AsterError::internal_error("archive temp file exceeds i64 range")
                })?,
                steps,
            ))
        })
        .await
        .map_err(|error| {
            AsterError::internal_error(format!("archive compress worker failed: {error}"))
        })??;

    set_task_step_active(
        &mut steps,
        TASK_STEP_STORE_RESULT,
        Some("Saving archive to workspace"),
        None,
    )?;
    mark_task_progress(
        state,
        &lease_guard,
        progress_total,
        progress_total,
        Some("Saving archive"),
        &steps,
    )
    .await?;
    let stored = workspace_storage_service::store_from_temp(
        state,
        workspace_storage_service::StoreFromTempParams::new(
            scope,
            payload.target_folder_id,
            &payload.archive_name,
            &archive_temp_path_string,
            archive_size,
        ),
    )
    .await?;
    cleanup_task_temp_dir_for_task(state, task.id).await?;
    set_task_step_succeeded(
        &mut steps,
        TASK_STEP_STORE_RESULT,
        Some(&format!("Saved archive as {}", stored.name)),
        None,
    )?;

    let result = ArchiveCompressTaskResult {
        target_file_id: stored.id,
        target_file_name: stored.name.clone(),
        target_folder_id: stored.folder_id,
        target_path: build_file_display_path(&state.db, stored.folder_id, &stored.name).await?,
    };
    let result_json = serialize_task_result(&result)?;
    mark_task_succeeded(
        state,
        &lease_guard,
        Some(&result_json),
        progress_total,
        progress_total,
        Some(&format!("Archive ready: {}", stored.name)),
        &steps,
    )
    .await
}

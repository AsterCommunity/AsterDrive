//! 归档解包任务子模块入口。

mod import;
mod staging;

use aster_forge_tasks::TaskExecutionContext;
use std::path::Path;

use aster_forge_tasks::{set_task_step_active, set_task_step_succeeded};
use chrono::Utc;

use super::common::{build_folder_display_path, create_unique_folder_in_scope};
use crate::config::operations;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::PrimaryAppState;
use crate::services::files::archive::core::format::{
    ArchiveFormat, detect_supported_archive_format,
};
use crate::services::{
    events::storage_change,
    task::{
        cleanup_task_temp_dir_for_lease, create_typed_task_record, get_task_in_scope,
        is_task_lease_lost, is_task_lease_renewal_timed_out, mark_task_progress,
        mark_task_succeeded,
        spec::{self, ArchiveExtractTask, decode_payload_as},
        steps::{
            TASK_STEP_DOWNLOAD_SOURCE, TASK_STEP_IMPORT_RESULT, TASK_STEP_WAITING,
            parse_task_steps_json,
        },
        task_scope,
        types::{
            ArchiveExtractTaskPayload, ArchiveExtractTaskResult, CreateArchiveExtractTaskParams,
            TaskInfo,
        },
    },
    workspace::storage::{self, WorkspaceStorageScope},
};
use aster_drive_model::entities::{background_task, file};
use import::materialize_archive_extract_stage;
use staging::{
    ArchiveExtractLimits, ArchiveExtractStageOptions, StageArchiveForExtractParams,
    download_file_to_temp, stage_zip_archive_for_extract,
};

pub(crate) async fn create_archive_extract_task_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_id: i64,
    params: CreateArchiveExtractTaskParams,
) -> Result<TaskInfo> {
    storage::require_scope_access_with_db(state, state.writer_db(), scope).await?;
    let source_file = storage::verify_file_access(state, scope, file_id).await?;
    storage::ensure_active_file_scope(&source_file, scope)?;
    ensure_extract_source_supported(&source_file)?;

    if let Some(target_folder_id) = params.target_folder_id {
        storage::verify_folder_access(state, scope, target_folder_id).await?;
    }

    let payload = ArchiveExtractTaskPayload {
        file_id: source_file.id,
        source_file_name: source_file.name.clone(),
        target_folder_id: params.target_folder_id.or(source_file.folder_id),
        output_folder_name: resolve_extract_output_folder_name(
            params.output_folder_name.as_ref(),
            &source_file.name,
        )?,
        filename_encoding: params.filename_encoding,
    };
    let display_name = format!("Extract {}", source_file.name);
    let task =
        create_typed_task_record::<ArchiveExtractTask>(state, scope, &display_name, &payload)
            .await?;
    get_task_in_scope(state, scope, task.id).await
}

pub(super) async fn process_archive_extract_task(
    state: &PrimaryAppState,
    task: &background_task::Model,
    context: TaskExecutionContext,
) -> Result<()> {
    let lease_guard = context.lease_guard().clone();
    let result = async {
        let scope = task_scope(task)?;
        let payload = decode_payload_as::<ArchiveExtractTask>(task)?;
        let mut steps = parse_task_steps_json(task.steps_json.as_ref().map(|raw| raw.as_ref()))?;
        set_task_step_succeeded(
            &mut steps,
            TASK_STEP_WAITING,
            Some("Worker claimed task"),
            None,
        )?;
        let source_file = storage::verify_file_access(state, scope, payload.file_id).await?;
        storage::ensure_active_file_scope(&source_file, scope)?;
        let archive_format = ensure_extract_source_supported(&source_file)?;
        if let Some(target_folder_id) = payload.target_folder_id {
            storage::verify_folder_access(state, scope, target_folder_id).await?;
        }
        let max_staging_bytes =
            operations::archive_extract_max_staging_bytes(state.runtime_config());
        let extract_limits = ArchiveExtractLimits::from_runtime_config(state.runtime_config());

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
            0,
            Some("Downloading source archive"),
            &steps,
        )
        .await?;
        ensure_source_archive_allowed(source_file.size, max_staging_bytes, extract_limits)?;
        let task_temp_dir = aster_forge_tasks::prepare_task_temp_dir_in_root(
            &state.config().server.temp_dir,
            lease_guard.lease(),
        )
        .await?;
        let task_temp_path = Path::new(&task_temp_dir);
        let source_archive_path = task_temp_path.join(archive_format.temp_file_name());
        let stage_root = task_temp_path.join("extract");
        tokio::fs::create_dir_all(&stage_root)
            .await
            .map_aster_err_ctx(
                "create archive extract staging dir",
                AsterError::storage_driver_error,
            )?;
        download_file_to_temp(state, &context, &source_file, &source_archive_path).await?;
        set_task_step_succeeded(
            &mut steps,
            TASK_STEP_DOWNLOAD_SOURCE,
            Some("Source archive downloaded"),
            None,
        )?;
        let steps_for_worker = steps.clone();

        let db = state.writer_db().clone();
        let handle = tokio::runtime::Handle::current();
        let context_for_worker = context.clone();
        let source_archive_path_for_worker = source_archive_path;
        let stage_root_for_worker = stage_root.clone();
        let stage_options = ArchiveExtractStageOptions {
            scope,
            source_archive_size: source_file.size,
            max_staging_bytes,
            limits: extract_limits,
            filename_encoding: payload.filename_encoding,
        };
        let (staged, mut steps) = tokio::task::spawn_blocking(move || {
            let mut steps = steps_for_worker;
            let stage_params = StageArchiveForExtractParams {
                handle: &handle,
                db: &db,
                context: &context_for_worker,
                archive_path: &source_archive_path_for_worker,
                stage_root: &stage_root_for_worker,
                options: stage_options,
            };
            let staged = match archive_format {
                ArchiveFormat::Zip => stage_zip_archive_for_extract(stage_params, &mut steps)?,
            };
            Ok::<_, AsterError>((staged, steps))
        })
        .await
        .map_err(|error| {
            AsterError::internal_error(format!("archive extract worker failed: {error}"))
        })??;

        context.ensure_active()?;
        let created_root = create_unique_folder_in_scope(
            state,
            scope,
            payload.target_folder_id,
            &payload.output_folder_name,
            &crate::db::repository::task_resource_publication_repo::TaskResourcePublication::new(
                context.lease_guard().lease(),
                "extract:root",
            ),
        )
        .await?;

        context.ensure_active()?;
        set_task_step_active(
            &mut steps,
            TASK_STEP_IMPORT_RESULT,
            Some("Importing extracted files"),
            Some((0, staged.total_bytes)),
        )?;
        mark_task_progress(
            state,
            &lease_guard,
            staged.total_bytes,
            staged.total_progress,
            Some("Importing extracted files"),
            &steps,
        )
        .await?;
        // Each published directory/file commits with its receipt. Keep committed output
        // on cancellation or failure so a retry can resume without duplicate mutations.
        let import_summary = materialize_archive_extract_stage(
            state,
            &context,
            scope,
            &stage_root,
            staged.total_bytes,
            &created_root,
            &mut steps,
        )
        .await?;
        let event = storage_change::StorageChangeEvent::new(
            storage_change::StorageChangeKind::FolderCreated,
            scope,
            import_summary.file_ids,
            import_summary.folder_ids,
            import_summary.affected_parent_ids,
        );
        storage_change::publish(
            state,
            if import_summary.reused_publication {
                event.with_quota_refresh()
            } else {
                event.with_storage_delta(import_summary.storage_delta)
            },
        );
        cleanup_task_temp_dir_for_lease(&[&state.config().server.temp_dir], lease_guard.lease())
            .await?;
        set_task_step_succeeded(
            &mut steps,
            TASK_STEP_IMPORT_RESULT,
            Some(&format!("Imported into {}", created_root.name)),
            Some((staged.total_bytes, staged.total_bytes)),
        )?;

        let result = ArchiveExtractTaskResult {
            target_folder_id: created_root.id,
            target_folder_name: created_root.name.clone(),
            target_path: build_folder_display_path(state.writer_db(), created_root.id).await?,
            extracted_file_count: staged.file_count,
            extracted_folder_count: staged.directory_count,
        };
        let result_json = spec::serialize_result::<ArchiveExtractTask>(&result)?;
        let progress_total = staged.total_progress;
        mark_task_succeeded(
            state,
            &lease_guard,
            Some(&result_json),
            progress_total,
            progress_total,
            Some(&format!("Extracted to {}", created_root.name)),
            &steps,
        )
        .await
    }
    .await;

    match result {
        Ok(()) => Ok(()),
        Err(error) => {
            // Partial publications are durable. Ask only this workspace to reread
            // metadata/quota when an interrupted import cannot emit its final batch.
            let published_root =
                crate::db::repository::task_resource_publication_repo::TaskResourcePublication::new(
                    lease_guard.lease(),
                    "extract:root",
                );
            match published_root.find_folder(state.writer_db()).await {
                Ok(Some(_)) => {
                    if let Ok(scope) = task_scope(task) {
                        let event = storage_change::StorageChangeEvent::new(
                            storage_change::StorageChangeKind::SyncRequired,
                            scope,
                            vec![],
                            vec![],
                            vec![],
                        )
                        .with_quota_refresh();
                        storage_change::publish(state, event);
                    }
                }
                Ok(None) => {}
                Err(lookup_error) => {
                    tracing::warn!(task_id=task.id,%lookup_error,"cannot inspect interrupted archive publication")
                }
            }
            if !is_task_lease_lost(&error)
                && !is_task_lease_renewal_timed_out(&error)
                && let Err(cleanup_error) = cleanup_task_temp_dir_for_lease(
                    &[&state.config().server.temp_dir],
                    lease_guard.lease(),
                )
                .await
            {
                tracing::warn!(
                    task_id = task.id,
                    "failed to cleanup archive extract temp dir after error: {cleanup_error}"
                );
            }
            Err(error)
        }
    }
}

fn ensure_extract_source_supported(source_file: &file::Model) -> Result<ArchiveFormat> {
    detect_supported_archive_format(source_file).ok_or_else(|| {
        AsterError::validation_error("online extract currently supports .zip files only")
    })
}

fn resolve_extract_output_folder_name(
    output_folder_name: Option<&String>,
    source_file_name: &str,
) -> Result<String> {
    let candidate = match output_folder_name.map(|value| value.trim()) {
        Some(value) if !value.is_empty() => value.to_string(),
        _ => default_extract_output_folder_name(source_file_name),
    };
    aster_forge_validation::filename::validate_name(&candidate)?;
    Ok(candidate)
}

fn default_extract_output_folder_name(source_file_name: &str) -> String {
    if let Some(stripped) = ArchiveFormat::Zip.strip_extension(source_file_name)
        && !stripped.is_empty()
    {
        return stripped.to_string();
    }
    format!("extracted-{}", Utc::now().format("%Y%m%d-%H%M%S"))
}

fn ensure_source_archive_allowed(
    source_archive_size: i64,
    max_staging_bytes: i64,
    limits: ArchiveExtractLimits,
) -> Result<()> {
    if source_archive_size > limits.max_source_bytes {
        return Err(AsterError::validation_error(format!(
            "source archive size {} exceeds server limit {}",
            source_archive_size, limits.max_source_bytes
        )));
    }
    if source_archive_size > max_staging_bytes {
        return Err(AsterError::validation_error(format!(
            "source archive requires {} staging bytes before extraction, exceeds server limit {}",
            source_archive_size, max_staging_bytes
        )));
    }
    Ok(())
}

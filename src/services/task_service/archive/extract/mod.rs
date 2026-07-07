//! 归档解包任务子模块入口。

mod import;
mod staging;

use std::path::Path;

use chrono::Utc;

use super::common::{build_folder_display_path, create_unique_folder_in_scope};
use crate::config::operations;
use crate::db::repository::background_task_repo;
use crate::entities::{background_task, file};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::archive_service::format::{ArchiveFormat, detect_supported_archive_format};
use crate::services::{
    storage_change_service,
    task_service::{
        TaskExecutionContext, TaskLease, cleanup_task_temp_dir_for_task_kind,
        create_typed_task_record, get_task_in_scope, is_task_lease_lost,
        is_task_lease_renewal_timed_out, mark_task_progress, mark_task_succeeded,
        prepare_task_temp_dir,
        spec::{self, ArchiveExtractTask, decode_payload_as},
        steps::{
            TASK_STEP_DOWNLOAD_SOURCE, TASK_STEP_IMPORT_RESULT, TASK_STEP_WAITING,
            parse_task_steps_json, set_task_step_active, set_task_step_succeeded,
        },
        task_scope,
        types::{
            ArchiveExtractTaskPayload, ArchiveExtractTaskResult, CreateArchiveExtractTaskParams,
            TaskInfo,
        },
    },
    workspace::storage::{self, WorkspaceStorageScope},
};
use crate::types::BackgroundTaskStatus;
use import::materialize_archive_extract_stage;
use staging::{
    ArchiveExtractLimits, ArchiveExtractPolicyResolver, ArchiveExtractStageOptions,
    StageArchiveForExtractParams, download_file_to_temp, stage_zip_archive_for_extract,
};

pub(crate) async fn create_archive_extract_task_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_id: i64,
    params: CreateArchiveExtractTaskParams,
) -> Result<TaskInfo> {
    storage::require_scope_access_with_db(state, state.writer_db(), scope)
        .await?;
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
        let mut steps =
            parse_task_steps_json(task.steps_json.as_ref().map(|raw| raw.as_ref()), task.kind)?;
        set_task_step_succeeded(
            &mut steps,
            TASK_STEP_WAITING,
            Some("Worker claimed task"),
            None,
        )?;
        let source_file =
            storage::verify_file_access(state, scope, payload.file_id).await?;
        storage::ensure_active_file_scope(&source_file, scope)?;
        let archive_format = ensure_extract_source_supported(&source_file)?;
        if let Some(target_folder_id) = payload.target_folder_id {
            storage::verify_folder_access(state, scope, target_folder_id).await?;
        }
        let max_staging_bytes =
            operations::archive_extract_max_staging_bytes(state.runtime_config());
        let extract_limits = ArchiveExtractLimits::from_runtime_config(state.runtime_config());
        let policy_resolver = resolve_archive_extract_policy_resolver(state, scope).await?;

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
        let task_temp_dir = prepare_task_temp_dir(state, lease_guard.lease()).await?;
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
        let policy_snapshot = state.policy_snapshot().clone();
        let handle = tokio::runtime::Handle::current();
        let context_for_worker = context.clone();
        let source_archive_path_for_worker = source_archive_path;
        let stage_root_for_worker = stage_root.clone();
        let stage_options = ArchiveExtractStageOptions {
            scope,
            policy_resolver,
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
                policy_snapshot: policy_snapshot.as_ref(),
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
        let import_summary = match materialize_archive_extract_stage(
            state,
            &context,
            scope,
            &stage_root,
            staged.total_bytes,
            &created_root,
            &mut steps,
        )
        .await
        {
            Ok(summary) => summary,
            Err(error) => {
                cleanup_created_extract_root_after_import_error(
                    state,
                    scope,
                    created_root.id,
                    &context,
                    &error,
                )
                .await;
                return Err(error);
            }
        };
        storage_change_service::publish(
            state,
            storage_change_service::StorageChangeEvent::new(
                storage_change_service::StorageChangeKind::FolderCreated,
                scope,
                import_summary.file_ids,
                import_summary.folder_ids,
                import_summary.affected_parent_ids,
            )
            .with_storage_delta(import_summary.storage_delta),
        );
        cleanup_task_temp_dir_for_task_kind(state, task.kind, task.id).await?;
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
            if !is_task_lease_lost(&error)
                && !is_task_lease_renewal_timed_out(&error)
                && let Err(cleanup_error) =
                    cleanup_task_temp_dir_for_task_kind(state, task.kind, task.id).await
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

async fn cleanup_created_extract_root_after_import_error(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    root_folder_id: i64,
    context: &TaskExecutionContext,
    error: &AsterError,
) {
    if !is_task_lease_lost(error) && !is_task_lease_renewal_timed_out(error) {
        cleanup_created_extract_root(state, scope, root_folder_id).await;
        return;
    }

    let lease = context.lease_guard().lease();
    match background_task_repo::find_by_id(state.writer_db(), lease.task_id).await {
        Ok(current_task)
            if should_cleanup_created_extract_root_for_lease_error(&current_task, lease) =>
        {
            cleanup_created_extract_root(state, scope, root_folder_id).await;
        }
        Ok(current_task) => {
            tracing::info!(
                task_id = lease.task_id,
                processing_token = lease.processing_token,
                current_processing_token = current_task.processing_token,
                current_status = ?current_task.status,
                root_folder_id,
                "skipping archive extract root cleanup because the task lease moved"
            );
        }
        Err(query_error) => {
            tracing::warn!(
                task_id = lease.task_id,
                processing_token = lease.processing_token,
                root_folder_id,
                "failed to check archive extract task lease before cleanup: {query_error}"
            );
        }
    }
}

fn should_cleanup_created_extract_root_for_lease_error(
    current_task: &background_task::Model,
    lease: TaskLease,
) -> bool {
    current_task.status == BackgroundTaskStatus::Processing
        && current_task.processing_token == lease.processing_token
}

async fn cleanup_created_extract_root(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    root_folder_id: i64,
) {
    match crate::services::files::folder::collect_folder_tree_in_scope(
        state.writer_db(),
        scope,
        root_folder_id,
        true,
    )
    .await
    {
        Ok((files, folder_ids)) => {
            if let Err(error) =
                crate::services::files::file::batch_purge_in_scope(state, scope, files).await
            {
                tracing::warn!(
                    root_folder_id,
                    "failed to purge partially imported archive files: {error}"
                );
            }
            if let Err(error) = crate::db::repository::property_repo::delete_all_for_entities(
                state.writer_db(),
                crate::types::EntityType::Folder,
                &folder_ids,
            )
            .await
            {
                tracing::warn!(
                    root_folder_id,
                    "failed to delete partially imported archive folder properties: {error}"
                );
            }
            if let Err(error) = crate::db::repository::share_repo::delete_by_folder_ids(
                state.writer_db(),
                &folder_ids,
            )
            .await
            {
                tracing::warn!(
                    root_folder_id,
                    "failed to delete partially imported archive shares: {error}"
                );
            }
            crate::services::files::folder::invalidate_folder_path_cache_for_ids(
                state,
                &folder_ids,
            )
            .await;
            if let Err(error) =
                crate::db::repository::folder_repo::delete_many(state.writer_db(), &folder_ids)
                    .await
            {
                tracing::warn!(
                    root_folder_id,
                    "failed to delete partially imported archive folders: {error}"
                );
            }
        }
        Err(error) => {
            tracing::warn!(
                root_folder_id,
                "failed to collect partially imported archive root for cleanup: {error}"
            );
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
    crate::utils::validate_name(&candidate)?;
    Ok(candidate)
}

fn default_extract_output_folder_name(source_file_name: &str) -> String {
    for archive_format in [ArchiveFormat::Zip] {
        if let Some(stripped) = archive_format.strip_extension(source_file_name)
            && !stripped.is_empty()
        {
            return stripped.to_string();
        }
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

async fn resolve_archive_extract_policy_resolver(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
) -> Result<ArchiveExtractPolicyResolver> {
    match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            Ok(ArchiveExtractPolicyResolver::Personal { user_id })
        }
        WorkspaceStorageScope::Team {
            team_id,
            actor_user_id,
        } => {
            let policy_group_id = storage::require_team_policy_group_id(
                state,
                team_id,
                actor_user_id,
            )
            .await?;
            Ok(ArchiveExtractPolicyResolver::Team { policy_group_id })
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::should_cleanup_created_extract_root_for_lease_error;
    use crate::entities::background_task;
    use crate::services::task_service::TaskLease;
    use crate::types::{BackgroundTaskKind, BackgroundTaskStatus, StoredTaskPayload};

    fn task_model(status: BackgroundTaskStatus, processing_token: i64) -> background_task::Model {
        let now = Utc::now();
        background_task::Model {
            id: 42,
            kind: BackgroundTaskKind::ArchiveExtract,
            status,
            creator_user_id: Some(7),
            team_id: None,
            share_id: None,
            display_name: "Extract archive.zip".to_string(),
            payload_json: StoredTaskPayload("{}".to_string()),
            result_json: None,
            runtime_json: None,
            steps_json: None,
            progress_current: 0,
            progress_total: 0,
            status_text: None,
            attempt_count: 0,
            max_attempts: 1,
            next_run_at: now,
            processing_token,
            processing_started_at: Some(now),
            last_heartbeat_at: Some(now),
            lease_expires_at: Some(now),
            started_at: Some(now),
            finished_at: None,
            last_error: None,
            failure_can_retry: None,
            expires_at: now,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn lease_error_cleanup_only_when_task_still_matches_current_token() {
        let lease = TaskLease::new(42, 3);

        assert!(should_cleanup_created_extract_root_for_lease_error(
            &task_model(BackgroundTaskStatus::Processing, 3),
            lease,
        ));
        assert!(!should_cleanup_created_extract_root_for_lease_error(
            &task_model(BackgroundTaskStatus::Processing, 4),
            lease,
        ));
        assert!(!should_cleanup_created_extract_root_for_lease_error(
            &task_model(BackgroundTaskStatus::Succeeded, 3),
            lease,
        ));
    }
}

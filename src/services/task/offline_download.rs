//! Offline download background task.

use std::path::Path;
use std::time::Duration as StdDuration;

use crate::config::operations;
use crate::entities::{background_task, file};
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::{
    files::folder,
    task::{
        TaskExecutionContext, cleanup_task_temp_dir_for_task_in_root, create_typed_task_record,
        get_task_in_scope, is_task_lease_lost, is_task_lease_renewal_timed_out, mark_task_progress,
        mark_task_succeeded, prepare_task_temp_dir_in_root,
        spec::{self, OfflineDownloadTask, decode_payload_as},
        steps::{
            TASK_STEP_DOWNLOAD_SOURCE, TASK_STEP_STORE_RESULT, TASK_STEP_VALIDATE_SOURCE,
            TASK_STEP_VERIFY_SOURCE, TASK_STEP_WAITING, parse_task_steps_json,
            set_task_step_active, set_task_step_succeeded,
        },
        task_scope,
        types::{
            CreateOfflineDownloadTaskParams, OfflineDownloadTaskPayload, OfflineDownloadTaskResult,
            TaskInfo,
        },
    },
    workspace::storage::{self, WorkspaceStorageScope},
};

const OFFLINE_DOWNLOAD_TEMP_FILE_NAME: &str = "source";
const PROGRESS_UPDATE_INTERVAL: StdDuration = StdDuration::from_millis(800);
const THROTTLED_DOWNLOAD_TIMEOUT_SLACK_SECS: u64 = 30;

mod aria2;
mod builtin;
mod engine;
mod naming;
mod runtime;
mod source;
mod transfer;

pub(crate) use aria2::{ProbeAria2RpcInput, probe_aria2_rpc};
use engine::{
    OfflineDownloadEngineSelection, OfflineDownloadStartRequest, download_with_enabled_engines,
};
use naming::{offline_download_task_base_display_name, resolve_offline_download_filename};
use source::{
    effective_offline_download_request_timeout, normalize_offline_download_request,
    parse_and_validate_source_url,
};

pub(super) use engine::{OfflineDownloadComplete, OfflineDownloadEngine};
pub(super) use naming::response_filename;
pub(super) use runtime::selected_engine_from_runtime_json;
pub(super) use source::{redact_url_for_display, resolve_source_host};
pub(super) use transfer::{
    OfflineDownloadRateLimiter, declared_content_length, ensure_download_size_allowed,
    transient_storage_error, verify_expected_sha256,
};

pub(crate) async fn create_offline_download_task_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    params: CreateOfflineDownloadTaskParams,
) -> Result<TaskInfo> {
    storage::require_scope_access_with_db(state, state.writer_db(), scope).await?;
    let request = normalize_offline_download_request(params)?;
    if operations::offline_download_enabled_engines(state.runtime_config()).is_empty() {
        return Err(AsterError::validation_error(
            "offline download is disabled because no download engine is enabled",
        ));
    }
    if let Some(target_folder_id) = request.target_folder_id {
        storage::verify_folder_access(state, scope, target_folder_id).await?;
    }

    let payload = OfflineDownloadTaskPayload {
        // Keep the original URL in the stored task payload for retries and
        // administrator investigations. Task API responses expose only the
        // redacted source_display_url wrapper, not this raw URL.
        url: request.url.as_str().to_string(),
        filename: request.filename,
        target_folder_id: request.target_folder_id,
        expected_sha256: request.expected_sha256,
        source_display_url: Some(redact_url_for_display(&request.url)),
    };
    let display_name = offline_download_task_base_display_name(&payload);
    let task =
        create_typed_task_record::<OfflineDownloadTask>(state, scope, &display_name, &payload)
            .await?;
    get_task_in_scope(state, scope, task.id).await
}

pub(super) async fn process_offline_download_task(
    state: &PrimaryAppState,
    task: &background_task::Model,
    context: TaskExecutionContext,
) -> Result<()> {
    let lease_guard = context.lease_guard().clone();
    let result = async {
        let scope = task_scope(task)?;
        let payload = decode_payload_as::<OfflineDownloadTask>(task)?;
        let base_display_name = offline_download_task_base_display_name(&payload);
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
            TASK_STEP_VALIDATE_SOURCE,
            Some("Validating source URL"),
            None,
        )?;
        mark_task_progress(
            state,
            &lease_guard,
            0,
            0,
            Some("Validating source URL"),
            &steps,
        )
        .await?;

        if let Some(target_folder_id) = payload.target_folder_id {
            storage::verify_folder_access(state, scope, target_folder_id).await?;
        }
        let source_url = parse_and_validate_source_url(&payload.url)?;
        let source_display_url = payload
            .source_display_url
            .clone()
            .unwrap_or_else(|| redact_url_for_display(&source_url));
        let temp_root = offline_download_temp_root(state);
        let task_temp_dir = prepare_task_temp_dir_in_root(&temp_root, lease_guard.lease()).await?;
        let temp_path = Path::new(&task_temp_dir).join(OFFLINE_DOWNLOAD_TEMP_FILE_NAME);
        let max_bytes = operations::offline_download_max_file_size_bytes(state.runtime_config());
        let max_bytes_per_sec =
            operations::offline_download_max_bytes_per_sec(state.runtime_config());
        let timeout = StdDuration::from_secs(
            operations::offline_download_request_timeout_secs(state.runtime_config()).max(1),
        );
        let timeout =
            effective_offline_download_request_timeout(timeout, max_bytes, max_bytes_per_sec)?;
        let engine_kinds = operations::offline_download_enabled_engines(state.runtime_config());
        if engine_kinds.is_empty() {
            return Err(AsterError::validation_error(
                "offline download is disabled because no download engine is enabled",
            ));
        }

        set_task_step_succeeded(
            &mut steps,
            TASK_STEP_VALIDATE_SOURCE,
            Some("Source URL accepted"),
            None,
        )?;
        set_task_step_active(
            &mut steps,
            TASK_STEP_DOWNLOAD_SOURCE,
            Some("Downloading source file"),
            None,
        )?;
        mark_task_progress(
            state,
            &lease_guard,
            0,
            0,
            Some("Downloading source file"),
            &steps,
        )
        .await?;

        let download_request = OfflineDownloadStartRequest {
            url: source_url,
            temp_path: temp_path.clone(),
            expected_sha256: payload.expected_sha256.clone(),
            max_bytes_per_sec,
            runtime_json: task
                .runtime_json
                .as_ref()
                .map(|value| value.as_ref().to_string()),
        };
        let downloaded = download_with_enabled_engines(
            state,
            &context,
            &mut steps,
            OfflineDownloadEngineSelection {
                engine_kinds: &engine_kinds,
                max_bytes,
                timeout,
                base_display_name: &base_display_name,
            },
            download_request,
        )
        .await?;

        set_task_step_succeeded(
            &mut steps,
            TASK_STEP_DOWNLOAD_SOURCE,
            Some("Source file downloaded"),
            Some((downloaded.bytes_written, downloaded.progress_total())),
        )?;
        set_task_step_active(
            &mut steps,
            TASK_STEP_VERIFY_SOURCE,
            Some("Verifying downloaded file"),
            Some((downloaded.bytes_written, downloaded.progress_total())),
        )?;
        mark_task_progress(
            state,
            &lease_guard,
            downloaded.bytes_written,
            downloaded.progress_total(),
            Some("Verifying downloaded file"),
            &steps,
        )
        .await?;
        set_task_step_succeeded(
            &mut steps,
            TASK_STEP_VERIFY_SOURCE,
            Some("Downloaded file verified"),
            Some((downloaded.bytes_written, downloaded.progress_total())),
        )?;

        context.ensure_active()?;
        let filename = resolve_offline_download_filename(
            payload.filename.as_deref(),
            downloaded.response_filename.as_deref(),
            &downloaded.final_url,
        )?;
        set_task_step_active(
            &mut steps,
            TASK_STEP_STORE_RESULT,
            Some("Importing file to workspace"),
            Some((downloaded.bytes_written, downloaded.progress_total())),
        )?;
        mark_task_progress(
            state,
            &lease_guard,
            downloaded.bytes_written,
            downloaded.progress_total(),
            Some("Importing file to workspace"),
            &steps,
        )
        .await?;

        let stored = storage::store_from_temp_internal(
            state,
            storage::StoreFromTempParams::new(
                scope,
                payload.target_folder_id,
                &filename,
                &temp_path.to_string_lossy(),
                downloaded.bytes_written,
            ),
            storage::StoreFromTempHints {
                precomputed_hash: Some(&downloaded.sha256),
                operation_context: context.storage_operation_context(),
                ..Default::default()
            },
            storage::NewFileMode::ResolveUnique,
            true,
        )
        .await?;
        cleanup_task_temp_dir_for_task_in_root(&temp_root, task.id).await?;
        set_task_step_succeeded(
            &mut steps,
            TASK_STEP_STORE_RESULT,
            Some(&format!("Imported as {}", stored.name)),
            Some((downloaded.bytes_written, downloaded.progress_total())),
        )?;
        let result_json =
            spec::serialize_result::<OfflineDownloadTask>(&OfflineDownloadTaskResult {
                file_id: stored.id,
                file_name: stored.name.clone(),
                folder_id: stored.folder_id,
                file_path: build_download_result_path(state, scope, &stored).await?,
                source_display_url,
                content_length: downloaded.bytes_written,
                sha256: downloaded.sha256.clone(),
                download_engine: downloaded.engine,
            })?;
        mark_task_succeeded(
            state,
            &lease_guard,
            Some(&result_json),
            downloaded.bytes_written,
            downloaded.progress_total(),
            Some("Offline download imported"),
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
                && let Err(cleanup_error) = cleanup_task_temp_dir_for_task_in_root(
                    &offline_download_temp_root(state),
                    task.id,
                )
                .await
            {
                tracing::warn!(
                    task_id = task.id,
                    "failed to cleanup offline download temp dir after error: {cleanup_error}"
                );
            }
            Err(error)
        }
    }
}

pub(super) fn offline_download_temp_root(state: &PrimaryAppState) -> String {
    operations::offline_download_temp_dir(state.runtime_config())
        .unwrap_or_else(|| state.config().server.temp_dir.clone())
}

pub(super) struct OfflineDownloadRetryPolicy;

impl super::retry::TaskRetryPolicy for OfflineDownloadRetryPolicy {
    fn retry_class(error: &AsterError) -> super::retry::TaskRetryClass {
        match error {
            AsterError::ValidationError(_) | AsterError::FileTooLarge(_) => {
                super::retry::TaskRetryClass::Never
            }
            _ => super::retry::default_retry_class(error),
        }
    }
}

async fn build_download_result_path(
    state: &PrimaryAppState,
    _scope: WorkspaceStorageScope,
    file: &file::Model,
) -> Result<String> {
    let Some(folder_id) = file.folder_id else {
        return Ok(format!("/{}", file.name));
    };
    let paths = folder::build_folder_paths(state.writer_db(), &[folder_id]).await?;
    let folder_path = paths.get(&folder_id).cloned().unwrap_or_default();
    if folder_path.is_empty() || folder_path == "/" {
        Ok(format!("/{}", file.name))
    } else {
        Ok(format!(
            "{}/{}",
            folder_path.trim_end_matches('/'),
            file.name
        ))
    }
}

#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::time::Duration as StdDuration;

use async_trait::async_trait;
use url::Url;

use crate::config::operations;
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;

use super::super::steps::{TASK_STEP_DOWNLOAD_SOURCE, set_task_step_active};
use super::super::{
    TaskLeaseGuard, mark_task_progress, set_task_display_name, set_task_runtime_json,
};
use super::naming::offline_download_task_display_name_with_engine;
use super::runtime::{
    decode_offline_download_runtime_state, persist_offline_download_runtime_state,
};
use super::{aria2, builtin};

#[derive(Debug, Clone)]
pub(in crate::services::task_service) struct OfflineDownloadStartRequest {
    pub(super) url: Url,
    pub(super) temp_path: PathBuf,
    pub(super) expected_sha256: Option<String>,
    pub(super) max_bytes_per_sec: Option<u64>,
    pub(super) runtime_json: Option<String>,
}

#[derive(Debug)]
pub(in crate::services::task_service) struct OfflineDownloadComplete {
    pub(super) final_url: Url,
    pub(super) response_filename: Option<String>,
    pub(super) bytes_written: i64,
    pub(super) sha256: String,
    pub(super) engine: Option<operations::OfflineDownloadEngine>,
    declared_content_length: Option<i64>,
}

impl OfflineDownloadComplete {
    pub(super) fn new(
        final_url: Url,
        response_filename: Option<String>,
        bytes_written: i64,
        sha256: String,
        declared_content_length: Option<i64>,
    ) -> Self {
        Self {
            final_url,
            response_filename,
            bytes_written,
            sha256,
            engine: None,
            declared_content_length,
        }
    }

    fn with_engine(mut self, engine: operations::OfflineDownloadEngine) -> Self {
        self.engine = Some(engine);
        self
    }

    pub(super) fn progress_total(&self) -> i64 {
        self.declared_content_length
            .unwrap_or(self.bytes_written)
            .max(self.bytes_written)
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct OfflineDownloadEngineSelection<'a> {
    pub(super) engine_kinds: &'a [operations::OfflineDownloadEngine],
    pub(super) max_bytes: i64,
    pub(super) timeout: StdDuration,
    pub(super) base_display_name: &'a str,
}

#[async_trait]
pub(in crate::services::task_service) trait OfflineDownloadEngine {
    async fn download(
        &mut self,
        state: &PrimaryAppState,
        lease_guard: &TaskLeaseGuard,
        request: OfflineDownloadStartRequest,
        steps: &mut [super::super::TaskStepInfo],
    ) -> Result<OfflineDownloadComplete>;
}

pub(super) async fn download_with_enabled_engines(
    state: &PrimaryAppState,
    lease_guard: &TaskLeaseGuard,
    steps: &mut [super::super::TaskStepInfo],
    selection: OfflineDownloadEngineSelection<'_>,
    request: OfflineDownloadStartRequest,
) -> Result<OfflineDownloadComplete> {
    let mut last_error = None;
    let mut request = request;
    for (index, engine_kind) in selection.engine_kinds.iter().copied().enumerate() {
        lease_guard.ensure_active()?;
        set_task_display_name(
            state,
            lease_guard,
            &offline_download_task_display_name_with_engine(
                selection.base_display_name,
                engine_kind,
            ),
        )
        .await?;
        let mut runtime_state =
            decode_offline_download_runtime_state(request.runtime_json.as_deref());
        runtime_state.engine = Some(engine_kind);
        if !matches!(engine_kind, operations::OfflineDownloadEngine::Aria2) {
            runtime_state.aria2 = None;
        }
        request.runtime_json =
            Some(persist_offline_download_runtime_state(state, lease_guard, &runtime_state).await?);
        let mut engine = offline_download_engine_for_kind(
            state,
            engine_kind,
            selection.max_bytes,
            selection.timeout,
        )?;
        match engine
            .download(state, lease_guard, request.clone(), steps)
            .await
        {
            Ok(downloaded) => return Ok(downloaded.with_engine(engine_kind)),
            Err(error) => {
                if !should_try_next_offline_download_engine(&error)
                    || index + 1 >= selection.engine_kinds.len()
                {
                    return Err(error);
                }
                tracing::warn!(
                    task_id = lease_guard.lease().task_id,
                    engine = engine_kind.as_str(),
                    "offline download engine failed; trying next enabled engine: {error}"
                );
                if matches!(engine_kind, operations::OfflineDownloadEngine::Aria2)
                    && let Err(clear_error) = set_task_runtime_json(state, lease_guard, None).await
                {
                    tracing::warn!(
                        task_id = lease_guard.lease().task_id,
                        "failed to clear aria2 runtime state before trying next engine: {clear_error}"
                    );
                }
                let next_engine = selection.engine_kinds[index + 1].as_str();
                let status_text = format!("{} failed; trying {next_engine}", engine_kind.as_str());
                set_task_step_active(steps, TASK_STEP_DOWNLOAD_SOURCE, Some(&status_text), None)?;
                mark_task_progress(state, lease_guard, 0, 0, Some(&status_text), steps).await?;
                last_error = Some(error);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| {
        AsterError::validation_error("offline download is disabled because no engine is enabled")
    }))
}

fn offline_download_engine_for_kind(
    state: &PrimaryAppState,
    kind: operations::OfflineDownloadEngine,
    max_bytes: i64,
    timeout: StdDuration,
) -> Result<Box<dyn OfflineDownloadEngine + Send>> {
    match kind {
        operations::OfflineDownloadEngine::Builtin => Ok(Box::new(
            builtin::BuiltinHttpOfflineDownloadEngine::new(max_bytes, timeout),
        )),
        operations::OfflineDownloadEngine::Aria2 => Ok(Box::new(
            aria2::Aria2OfflineDownloadEngine::from_runtime_config(state, max_bytes, timeout)?,
        )),
    }
}

fn should_try_next_offline_download_engine(error: &AsterError) -> bool {
    matches!(error, AsterError::StorageDriverError(_))
}

use serde::{Deserialize, Serialize};

use crate::config::operations;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::PrimaryAppState;

use super::super::{TaskLeaseGuard, set_task_runtime_json};

#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct OfflineDownloadRuntimeState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) engine: Option<operations::OfflineDownloadEngine>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) aria2: Option<Aria2TaskRuntime>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Aria2TaskRuntime {
    pub(super) gid: String,
    pub(super) processing_token: i64,
}

pub(super) fn decode_offline_download_runtime_state(
    raw: Option<&str>,
) -> OfflineDownloadRuntimeState {
    let Some(raw) = raw else {
        return OfflineDownloadRuntimeState::default();
    };
    if raw.trim().is_empty() {
        return OfflineDownloadRuntimeState::default();
    }
    serde_json::from_str(raw).unwrap_or_else(|error| {
        tracing::warn!("invalid offline download runtime_json; ignoring it: {error}");
        OfflineDownloadRuntimeState::default()
    })
}

pub(super) fn serialize_offline_download_runtime_state(
    runtime_state: &OfflineDownloadRuntimeState,
) -> Result<String> {
    serde_json::to_string(runtime_state).map_aster_err_ctx(
        "serialize offline download runtime state",
        AsterError::internal_error,
    )
}

pub(super) async fn persist_offline_download_runtime_state(
    state: &PrimaryAppState,
    lease_guard: &TaskLeaseGuard,
    runtime_state: &OfflineDownloadRuntimeState,
) -> Result<String> {
    let runtime_json = serialize_offline_download_runtime_state(runtime_state)?;
    set_task_runtime_json(state, lease_guard, Some(&runtime_json)).await?;
    Ok(runtime_json)
}

pub(in crate::services::task_service) fn selected_engine_from_runtime_json(
    raw: Option<&str>,
) -> Option<operations::OfflineDownloadEngine> {
    decode_offline_download_runtime_state(raw).engine
}

use std::path::Path;
use std::time::{Duration as StdDuration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize, de::DeserializeOwned, ser::SerializeSeq};
use sha2::Digest;
use tokio::io::AsyncReadExt;
use url::Url;

use crate::config::operations;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::PrimaryAppState;
use crate::services::task_service::types::TaskStepInfo;
use crate::storage::error::{StorageErrorKind, storage_driver_error_with_subcode};
use crate::utils::numbers::u64_to_i64;

use super::super::steps::{TASK_STEP_DOWNLOAD_SOURCE, set_task_step_active};
use super::super::{TaskExecutionContext, mark_task_progress};
use super::runtime::{
    Aria2TaskRuntime, decode_offline_download_runtime_state, persist_offline_download_runtime_state,
};
use super::{
    OFFLINE_DOWNLOAD_TEMP_FILE_NAME, OfflineDownloadComplete, OfflineDownloadEngine,
    OfflineDownloadStartRequest, ensure_download_size_allowed, resolve_source_host,
    transient_storage_error, verify_expected_sha256,
};

const ARIA2_STATUS_POLL_INTERVAL: StdDuration = StdDuration::from_secs(2);
const ARIA2_STATUS_MAX_CONSECUTIVE_RPC_ERRORS: u8 = 3;

pub(super) struct Aria2OfflineDownloadEngine {
    pub(super) max_bytes: i64,
    pub(super) download_timeout: StdDuration,
    pub(super) client: Aria2RpcClient,
    pub(super) split: u64,
    pub(super) max_connection_per_server: u64,
    pub(super) lowest_speed_limit_bytes_per_sec: Option<u64>,
}

impl Aria2OfflineDownloadEngine {
    pub(super) fn from_runtime_config(
        state: &PrimaryAppState,
        max_bytes: i64,
        download_timeout: StdDuration,
    ) -> Result<Self> {
        let rpc_url = operations::offline_download_aria2_rpc_url(&state.runtime_config)
            .ok_or_else(|| {
                AsterError::validation_error(
                    "offline_download_aria2_rpc_url is required when offline_download_engine is aria2",
                )
            })?;
        let rpc_timeout = StdDuration::from_secs(
            operations::offline_download_aria2_request_timeout_secs(&state.runtime_config).max(1),
        );
        Ok(Self {
            max_bytes,
            download_timeout,
            client: Aria2RpcClient::new(
                &rpc_url,
                operations::offline_download_aria2_rpc_secret(&state.runtime_config),
                rpc_timeout,
            )?,
            split: operations::offline_download_aria2_split(&state.runtime_config),
            max_connection_per_server: operations::offline_download_aria2_max_connection_per_server(
                &state.runtime_config,
            ),
            lowest_speed_limit_bytes_per_sec:
                operations::offline_download_aria2_lowest_speed_limit_bytes_per_sec(
                    &state.runtime_config,
                ),
        })
    }

    pub(super) fn options(&self, request: &OfflineDownloadStartRequest) -> Aria2AddUriOptions {
        let dir = request
            .temp_path
            .parent()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default();
        let out = request
            .temp_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| OFFLINE_DOWNLOAD_TEMP_FILE_NAME.to_string());

        Aria2AddUriOptions {
            dir,
            out,
            allow_overwrite: "true".to_string(),
            auto_file_renaming: "false".to_string(),
            follow_torrent: "false".to_string(),
            follow_metalink: "false".to_string(),
            max_redirect: "0".to_string(),
            user_agent: crate::utils::OUTBOUND_HTTP_USER_AGENT.to_string(),
            split: self.split.to_string(),
            max_connection_per_server: self.max_connection_per_server.to_string(),
            lowest_speed_limit: self
                .lowest_speed_limit_bytes_per_sec
                .map(|limit| limit.to_string()),
            max_download_limit: request.max_bytes_per_sec.map(|limit| limit.to_string()),
        }
    }
}

#[async_trait]
impl OfflineDownloadEngine for Aria2OfflineDownloadEngine {
    async fn download(
        &mut self,
        state: &PrimaryAppState,
        context: &TaskExecutionContext,
        request: OfflineDownloadStartRequest,
        steps: &mut [TaskStepInfo],
    ) -> Result<OfflineDownloadComplete> {
        let lease_guard = context.lease_guard();
        resolve_source_host(&request.url).await?;
        let mut runtime_state =
            decode_offline_download_runtime_state(request.runtime_json.as_deref());
        if let Some(previous) = runtime_state.aria2.as_ref()
            && previous.processing_token != lease_guard.lease().processing_token
            && let Err(error) = self.client.force_remove(&previous.gid).await
        {
            tracing::warn!(
                task_id = lease_guard.lease().task_id,
                gid = previous.gid,
                "failed to cleanup stale aria2 offline download before retry: {error}"
            );
        }

        prepare_aria2_output_dir(&request.temp_path).await?;
        let gid = self
            .client
            .add_uri(request.url.as_str(), self.options(&request))
            .await?;
        runtime_state.engine = Some(operations::OfflineDownloadEngine::Aria2);
        runtime_state.aria2 = Some(Aria2TaskRuntime {
            gid: gid.clone(),
            processing_token: lease_guard.lease().processing_token,
        });
        persist_offline_download_runtime_state(state, context, &runtime_state).await?;

        let result = self
            .poll_until_complete(state, context, &request, steps, &gid)
            .await;
        if result.is_err() {
            self.cleanup_download_after_error(lease_guard.lease().task_id, &gid)
                .await;
        }
        result
    }
}

impl Aria2OfflineDownloadEngine {
    async fn cleanup_download_after_error(&self, task_id: i64, gid: &str) {
        if let Err(cleanup_error) = self.client.force_remove(gid).await {
            tracing::warn!(
                task_id,
                gid,
                "failed to force-remove aria2 offline download after error: {cleanup_error}"
            );
        }
        if let Err(cleanup_error) = self.client.remove_download_result(gid).await {
            tracing::warn!(
                task_id,
                gid,
                "failed to remove aria2 offline download result after error: {cleanup_error}"
            );
        }
    }
}

pub(super) async fn prepare_aria2_output_dir(temp_path: &Path) -> Result<()> {
    let Some(dir) = temp_path.parent() else {
        return Ok(());
    };
    tokio::fs::create_dir_all(dir).await.map_aster_err_ctx(
        "create aria2 offline download output dir",
        AsterError::storage_driver_error,
    )?;
    allow_external_aria2_writer_chain(dir).await
}

#[cfg(unix)]
async fn allow_external_aria2_writer_chain(token_dir: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    // aria2 may run as a different OS user from AsterDrive. The shared task
    // leaf directories must therefore be writable by that external process.
    // Prefer a shared owner/group or POSIX ACLs in deployments that can manage
    // them; these broad modes are the compatibility fallback.
    let Some(task_dir) = token_dir.parent() else {
        return Ok(());
    };
    let Some(tasks_dir) = task_dir.parent() else {
        return Ok(());
    };

    tokio::fs::set_permissions(tasks_dir, std::fs::Permissions::from_mode(0o711))
        .await
        .map_aster_err_ctx(
            "set aria2 offline download tasks dir permissions",
            AsterError::storage_driver_error,
        )?;

    for dir in [task_dir, token_dir] {
        tokio::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o777))
            .await
            .map_aster_err_ctx(
                "set aria2 offline download output dir permissions",
                AsterError::storage_driver_error,
            )?;
    }

    Ok(())
}

#[cfg(not(unix))]
async fn allow_external_aria2_writer_chain(_dir: &Path) -> Result<()> {
    Ok(())
}

impl Aria2OfflineDownloadEngine {
    async fn poll_until_complete(
        &self,
        state: &PrimaryAppState,
        context: &TaskExecutionContext,
        request: &OfflineDownloadStartRequest,
        steps: &mut [TaskStepInfo],
        gid: &str,
    ) -> Result<OfflineDownloadComplete> {
        let lease_guard = context.lease_guard();
        let started_at = Instant::now();
        let mut last_progress = Instant::now()
            .checked_sub(ARIA2_STATUS_POLL_INTERVAL)
            .unwrap_or_else(Instant::now);
        let mut declared_content_length = None;
        let mut consecutive_rpc_errors = 0_u8;

        loop {
            context.ensure_active()?;
            if started_at.elapsed() > self.download_timeout {
                return Err(AsterError::storage_driver_error(
                    "transient: aria2 offline download timed out",
                ));
            }

            let status = tokio::select! {
                biased;
                shutdown = context.shutdown_requested() => {
                    shutdown?;
                    unreachable!("shutdown_requested only resolves when shutdown is requested");
                }
                status = self.client.tell_status(gid) => status,
            };
            let status = match status {
                Ok(status) => {
                    consecutive_rpc_errors = 0;
                    status
                }
                Err(error) => {
                    consecutive_rpc_errors = consecutive_rpc_errors.saturating_add(1);
                    if consecutive_rpc_errors >= ARIA2_STATUS_MAX_CONSECUTIVE_RPC_ERRORS {
                        return Err(error);
                    }
                    tracing::warn!(
                        task_id = lease_guard.lease().task_id,
                        gid,
                        consecutive_rpc_errors,
                        max_consecutive_rpc_errors = ARIA2_STATUS_MAX_CONSECUTIVE_RPC_ERRORS,
                        "transient aria2 tellStatus poll failed: {error}"
                    );
                    context
                        .sleep_or_shutdown(ARIA2_STATUS_POLL_INTERVAL)
                        .await?;
                    continue;
                }
            };
            let completed = parse_aria2_length(&status.completed_length, "aria2 completedLength")?;
            ensure_download_size_allowed(completed, self.max_bytes)?;
            let total = parse_aria2_length(&status.total_length, "aria2 totalLength")?;
            if total > 0 {
                ensure_download_size_allowed(total, self.max_bytes)?;
                declared_content_length = Some(total);
            }

            match &status.status {
                Aria2DownloadStatus::Complete => {
                    break;
                }
                Aria2DownloadStatus::Active
                | Aria2DownloadStatus::Waiting
                | Aria2DownloadStatus::Paused => {
                    if last_progress.elapsed() >= ARIA2_STATUS_POLL_INTERVAL {
                        let progress_total = total.max(completed);
                        let status_text = format!("Downloaded {completed} bytes");
                        set_task_step_active(
                            steps,
                            TASK_STEP_DOWNLOAD_SOURCE,
                            Some(&status_text),
                            Some((completed, progress_total)),
                        )?;
                        mark_task_progress(
                            state,
                            lease_guard,
                            completed,
                            progress_total,
                            Some(&status_text),
                            steps,
                        )
                        .await?;
                        last_progress = Instant::now();
                    }
                }
                Aria2DownloadStatus::Error => {
                    let summary = status.error_summary();
                    let prefix = match classify_aria2_download_failure(&status) {
                        Aria2DownloadFailureClass::TransientOrUnknown => "transient: ",
                        Aria2DownloadFailureClass::PermanentClientError => "",
                    };
                    return Err(AsterError::storage_driver_error(format!(
                        "{prefix}aria2 offline download failed: {summary}"
                    )));
                }
                Aria2DownloadStatus::Removed => {
                    return Err(AsterError::storage_driver_error(
                        "transient: aria2 offline download was removed",
                    ));
                }
                Aria2DownloadStatus::Unknown(other) => {
                    return Err(AsterError::storage_driver_error(format!(
                        "transient: aria2 offline download entered unsupported status {other}"
                    )));
                }
            }

            context
                .sleep_or_shutdown(ARIA2_STATUS_POLL_INTERVAL)
                .await?;
        }

        let bytes_written = downloaded_file_size(&request.temp_path).await?;
        ensure_download_size_allowed(bytes_written, self.max_bytes)?;
        if bytes_written <= 0 {
            return Err(AsterError::validation_error(
                "offline download source returned an empty file",
            ));
        }
        if let Some(length) = declared_content_length
            && bytes_written != length
        {
            return Err(AsterError::storage_driver_error(format!(
                "transient: offline download size mismatch: declared {length}, received {bytes_written}"
            )));
        }
        let sha256 = sha256_file(&request.temp_path).await?;
        verify_expected_sha256(request.expected_sha256.as_deref(), &sha256)?;
        if let Err(error) = self.client.remove_download_result(gid).await {
            tracing::warn!(
                gid,
                "failed to remove completed aria2 download result: {error}"
            );
        }

        Ok(OfflineDownloadComplete::new(
            request.url.clone(),
            None,
            bytes_written,
            sha256,
            declared_content_length,
        ))
    }
}

pub(super) struct Aria2RpcClient {
    endpoint: Url,
    secret: Option<String>,
    client: reqwest::Client,
}

pub(crate) struct ProbeAria2RpcInput {
    pub rpc_url: String,
    pub rpc_secret: Option<String>,
    pub request_timeout: StdDuration,
}

pub(crate) async fn probe_aria2_rpc(input: ProbeAria2RpcInput) -> Result<String> {
    let client = Aria2RpcClient::new(&input.rpc_url, input.rpc_secret, input.request_timeout)?;
    let version = client.get_version().await?;
    let feature_count = version.enabled_features.len();
    Ok(format!(
        "aria2 RPC ready: version {}, {feature_count} enabled features",
        version.version
    ))
}

impl Aria2RpcClient {
    pub(super) fn new(
        endpoint: &str,
        secret: Option<String>,
        timeout: StdDuration,
    ) -> Result<Self> {
        let endpoint = Url::parse(endpoint)
            .map_aster_err_ctx("parse aria2 RPC URL", AsterError::validation_error)?;
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .user_agent(crate::utils::OUTBOUND_HTTP_USER_AGENT)
            .build()
            .map_aster_err_ctx("build aria2 RPC HTTP client", AsterError::internal_error)?;
        Ok(Self {
            endpoint,
            secret,
            client,
        })
    }

    async fn add_uri(&self, source_url: &str, options: Aria2AddUriOptions) -> Result<String> {
        let uris = [source_url];
        self.call(
            Aria2RpcMethod::AddUri,
            self.params(
                Aria2RpcParam::Uris(&uris),
                Some(Aria2RpcParam::AddUriOptions(&options)),
            ),
        )
        .await
    }

    async fn tell_status(&self, gid: &str) -> Result<Aria2TellStatus> {
        let keys = [
            Aria2TellStatusKey::Gid,
            Aria2TellStatusKey::Status,
            Aria2TellStatusKey::TotalLength,
            Aria2TellStatusKey::CompletedLength,
            Aria2TellStatusKey::DownloadSpeed,
            Aria2TellStatusKey::ErrorCode,
            Aria2TellStatusKey::ErrorMessage,
        ];
        self.call(
            Aria2RpcMethod::TellStatus,
            self.params(
                Aria2RpcParam::String(gid),
                Some(Aria2RpcParam::TellStatusKeys(&keys)),
            ),
        )
        .await
    }

    async fn get_version(&self) -> Result<Aria2Version> {
        self.call(Aria2RpcMethod::GetVersion, self.params_empty())
            .await
    }

    async fn force_remove(&self, gid: &str) -> Result<()> {
        self.call_ignore_missing(Aria2RpcMethod::ForceRemove, gid)
            .await
    }

    async fn remove_download_result(&self, gid: &str) -> Result<()> {
        self.call_ignore_missing(Aria2RpcMethod::RemoveDownloadResult, gid)
            .await
    }

    async fn call_ignore_missing(&self, method: Aria2RpcMethod, gid: &str) -> Result<()> {
        match self
            .call_raw::<String>(method, self.params(Aria2RpcParam::String(gid), None))
            .await
        {
            Ok(_) => Ok(()),
            Err(Aria2RpcCallError::Rpc {
                method,
                code,
                message,
                http_status,
            }) if aria2_rpc_error_is_missing_download(method, code, &message, http_status) => {
                Ok(())
            }
            Err(error) => Err(error.into_aster_error()),
        }
    }

    pub(super) fn params<'a>(
        &'a self,
        first: Aria2RpcParam<'a>,
        second: Option<Aria2RpcParam<'a>>,
    ) -> Aria2RpcParams<'a> {
        Aria2RpcParams {
            secret: self.secret.as_deref(),
            first: Some(first),
            second,
        }
    }

    pub(super) fn params_empty(&self) -> Aria2RpcParams<'_> {
        Aria2RpcParams {
            secret: self.secret.as_deref(),
            first: None,
            second: None,
        }
    }

    async fn call<T: DeserializeOwned>(
        &self,
        method: Aria2RpcMethod,
        params: Aria2RpcParams<'_>,
    ) -> Result<T> {
        self.call_raw(method, params)
            .await
            .map_err(Aria2RpcCallError::into_aster_error)
    }

    async fn call_raw<T: DeserializeOwned>(
        &self,
        method: Aria2RpcMethod,
        params: Aria2RpcParams<'_>,
    ) -> std::result::Result<T, Aria2RpcCallError> {
        let request = Aria2JsonRpcRequest {
            jsonrpc: "2.0",
            id: format!("asterdrive-{}", crate::utils::id::new_uuid()),
            method: method.as_str(),
            params,
        };
        let response = self
            .client
            .post(self.endpoint.clone())
            .json(&request)
            .send()
            .await
            .map_err(|error| {
                Aria2RpcCallError::Transport(transient_storage_error(format!(
                    "send aria2 RPC request: {error}"
                )))
            })?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.map_err(|error| {
                Aria2RpcCallError::Transport(transient_storage_error(format!(
                    "read aria2 RPC HTTP error response: {error}"
                )))
            })?;
            if let Some(error) = parse_aria2_rpc_error_response(&body) {
                return Err(Aria2RpcCallError::Rpc {
                    method,
                    code: error.code,
                    message: error.message,
                    http_status: Some(status),
                });
            }
            return Err(Aria2RpcCallError::Transport(transient_storage_error(
                format!("aria2 RPC returned HTTP {status}"),
            )));
        }
        let body = response
            .json::<Aria2JsonRpcResponse<T>>()
            .await
            .map_err(|error| {
                Aria2RpcCallError::Transport(transient_storage_error(format!(
                    "decode aria2 RPC response: {error}"
                )))
            })?;
        if let Some(error) = body.error {
            return Err(Aria2RpcCallError::Rpc {
                method,
                code: error.code,
                message: error.message,
                http_status: None,
            });
        }
        body.result.ok_or_else(|| {
            Aria2RpcCallError::Transport(AsterError::storage_driver_error(format!(
                "transient: aria2 RPC {} returned no result",
                method.as_str()
            )))
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Aria2RpcMethod {
    AddUri,
    TellStatus,
    GetVersion,
    ForceRemove,
    RemoveDownloadResult,
}

impl Aria2RpcMethod {
    fn as_str(self) -> &'static str {
        match self {
            Self::AddUri => "aria2.addUri",
            Self::TellStatus => "aria2.tellStatus",
            Self::GetVersion => "aria2.getVersion",
            Self::ForceRemove => "aria2.forceRemove",
            Self::RemoveDownloadResult => "aria2.removeDownloadResult",
        }
    }
}

#[derive(Debug)]
pub(super) struct Aria2RpcParams<'a> {
    secret: Option<&'a str>,
    first: Option<Aria2RpcParam<'a>>,
    second: Option<Aria2RpcParam<'a>>,
}

impl Serialize for Aria2RpcParams<'_> {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let len = usize::from(self.secret.is_some())
            + usize::from(self.first.is_some())
            + usize::from(self.second.is_some());
        let mut seq = serializer.serialize_seq(Some(len))?;
        if let Some(secret) = self.secret {
            seq.serialize_element(&Aria2RpcSecret(secret))?;
        }
        if let Some(first) = self.first {
            seq.serialize_element(&first)?;
        }
        if let Some(second) = self.second {
            seq.serialize_element(&second)?;
        }
        seq.end()
    }
}

struct Aria2RpcSecret<'a>(&'a str);

impl Serialize for Aria2RpcSecret<'_> {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(&format_args!("token:{}", self.0))
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum Aria2RpcParam<'a> {
    String(&'a str),
    Uris(&'a [&'a str]),
    AddUriOptions(&'a Aria2AddUriOptions),
    TellStatusKeys(&'a [Aria2TellStatusKey]),
}

impl Serialize for Aria2RpcParam<'_> {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::String(value) => serializer.serialize_str(value),
            Self::Uris(uris) => uris.serialize(serializer),
            Self::AddUriOptions(options) => options.serialize(serializer),
            Self::TellStatusKeys(keys) => keys.serialize(serializer),
        }
    }
}

#[derive(Debug, Serialize)]
pub(super) struct Aria2AddUriOptions {
    pub(super) dir: String,
    pub(super) out: String,
    #[serde(rename = "allow-overwrite")]
    pub(super) allow_overwrite: String,
    #[serde(rename = "auto-file-renaming")]
    pub(super) auto_file_renaming: String,
    #[serde(rename = "follow-torrent")]
    pub(super) follow_torrent: String,
    #[serde(rename = "follow-metalink")]
    pub(super) follow_metalink: String,
    #[serde(rename = "max-redirect")]
    pub(super) max_redirect: String,
    #[serde(rename = "user-agent")]
    pub(super) user_agent: String,
    pub(super) split: String,
    #[serde(rename = "max-connection-per-server")]
    pub(super) max_connection_per_server: String,
    #[serde(rename = "lowest-speed-limit", skip_serializing_if = "Option::is_none")]
    pub(super) lowest_speed_limit: Option<String>,
    #[serde(rename = "max-download-limit", skip_serializing_if = "Option::is_none")]
    pub(super) max_download_limit: Option<String>,
}

#[derive(Debug, Serialize, Clone, Copy)]
pub(super) enum Aria2TellStatusKey {
    #[serde(rename = "gid")]
    Gid,
    #[serde(rename = "status")]
    Status,
    #[serde(rename = "totalLength")]
    TotalLength,
    #[serde(rename = "completedLength")]
    CompletedLength,
    #[serde(rename = "downloadSpeed")]
    DownloadSpeed,
    #[serde(rename = "errorCode")]
    ErrorCode,
    #[serde(rename = "errorMessage")]
    ErrorMessage,
}

pub(super) enum Aria2RpcCallError {
    Rpc {
        method: Aria2RpcMethod,
        code: i64,
        message: String,
        http_status: Option<reqwest::StatusCode>,
    },
    Transport(AsterError),
}

impl Aria2RpcCallError {
    pub(super) fn into_aster_error(self) -> AsterError {
        match self {
            Self::Rpc {
                method,
                code,
                message,
                http_status,
            } => {
                if aria2_rpc_error_is_unauthorized(method, code, &message, http_status) {
                    return storage_driver_error_with_subcode(
                        StorageErrorKind::Auth,
                        crate::api::subcode::ApiSubcode::OfflineDownloadAria2RpcAuthFailed,
                        "aria2 RPC authentication failed: check offline_download_aria2_rpc_secret",
                    );
                }
                let status_context = http_status
                    .map(|status| format!(" after HTTP {status}"))
                    .unwrap_or_default();
                let method = method.as_str();
                AsterError::storage_driver_error(format!(
                    "transient: aria2 RPC {method} failed{status_context} with code {code}: {message}"
                ))
            }
            Self::Transport(error) => error,
        }
    }
}

#[derive(Serialize)]
pub(super) struct Aria2JsonRpcRequest<'a> {
    jsonrpc: &'static str,
    id: String,
    method: &'static str,
    params: Aria2RpcParams<'a>,
}

#[derive(Deserialize)]
struct Aria2JsonRpcResponse<T> {
    result: Option<T>,
    error: Option<Aria2JsonRpcError>,
}

#[derive(Deserialize)]
pub(super) struct Aria2JsonRpcError {
    pub(super) code: i64,
    pub(super) message: String,
}

#[derive(Deserialize)]
struct Aria2JsonRpcErrorResponse {
    error: Option<Aria2JsonRpcError>,
}

pub(super) fn parse_aria2_rpc_error_response(raw: &str) -> Option<Aria2JsonRpcError> {
    serde_json::from_str::<Aria2JsonRpcErrorResponse>(raw)
        .ok()
        .and_then(|response| response.error)
}

pub(super) fn aria2_rpc_error_is_unauthorized(
    method: Aria2RpcMethod,
    code: i64,
    message: &str,
    http_status: Option<reqwest::StatusCode>,
) -> bool {
    let message = message.to_ascii_lowercase();
    // aria2 1.36.x JSON-RPC auth failures have code 1 and messages such as
    // "Unauthorized" or "session not started"; ambiguous type errors are not auth.
    matches!(method, Aria2RpcMethod::GetVersion)
        && code == 1
        && (http_status.is_none() || http_status == Some(reqwest::StatusCode::BAD_REQUEST))
        && (message.contains("unauthorized")
            || message.contains("session not started")
            || message.contains("authentication"))
}

pub(super) fn aria2_rpc_error_is_missing_download(
    method: Aria2RpcMethod,
    code: i64,
    message: &str,
    http_status: Option<reqwest::StatusCode>,
) -> bool {
    let message = message.to_ascii_lowercase();
    // aria2 1.36.x missing-download errors mention GID/not-found. "wrong type"
    // is intentionally not treated as missing because it can indicate bad params.
    matches!(
        method,
        Aria2RpcMethod::ForceRemove | Aria2RpcMethod::RemoveDownloadResult
    ) && code == 1
        && (http_status.is_none() || http_status == Some(reqwest::StatusCode::BAD_REQUEST))
        && message.contains("gid")
        && (message.contains("not found")
            || message.contains("notfound")
            || message.contains("does not exist"))
}

#[derive(Debug, Deserialize)]
struct Aria2Version {
    version: String,
    #[serde(rename = "enabledFeatures", default)]
    enabled_features: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct Aria2TellStatus {
    pub(super) status: Aria2DownloadStatus,
    #[serde(rename = "totalLength", default)]
    pub(super) total_length: String,
    #[serde(rename = "completedLength", default)]
    pub(super) completed_length: String,
    #[serde(rename = "errorCode", default)]
    pub(super) error_code: Option<String>,
    #[serde(rename = "errorMessage", default)]
    pub(super) error_message: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Aria2DownloadStatus {
    Active,
    Waiting,
    Paused,
    Error,
    Complete,
    Removed,
    Unknown(String),
}

impl<'de> Deserialize<'de> for Aria2DownloadStatus {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "active" => Self::Active,
            "waiting" => Self::Waiting,
            "paused" => Self::Paused,
            "error" => Self::Error,
            "complete" => Self::Complete,
            "removed" => Self::Removed,
            _ => Self::Unknown(value),
        })
    }
}

impl Aria2TellStatus {
    fn error_summary(&self) -> String {
        match (self.error_code.as_deref(), self.error_message.as_deref()) {
            (Some(code), Some(message)) if !message.is_empty() => {
                format!("{code}: {message}")
            }
            (Some(code), _) => code.to_string(),
            (_, Some(message)) if !message.is_empty() => message.to_string(),
            _ => "unknown error".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Aria2DownloadFailureClass {
    PermanentClientError,
    TransientOrUnknown,
}

pub(super) fn classify_aria2_download_failure(
    status: &Aria2TellStatus,
) -> Aria2DownloadFailureClass {
    // aria2 tellStatus exposes download failures as errorCode/errorMessage rather
    // than a typed HTTP status. Keep that compatibility parsing isolated here so
    // the polling path consumes a typed classification and treats ambiguous
    // messages as retryable.
    if let Some(message) = status.error_message.as_deref()
        && aria2_error_message_indicates_permanent_client_error(message)
    {
        return Aria2DownloadFailureClass::PermanentClientError;
    }

    Aria2DownloadFailureClass::TransientOrUnknown
}

fn aria2_error_message_indicates_permanent_client_error(message: &str) -> bool {
    if let Some(status_code) = extract_http_status_code(message)
        && (400..500).contains(&status_code)
    {
        return true;
    }

    let message = message.to_ascii_lowercase();
    [
        "resource not found",
        "not found",
        "forbidden",
        "unauthorized",
        "permission denied",
        "bad request",
    ]
    .iter()
    .any(|phrase| message.contains(phrase))
}

fn extract_http_status_code(message: &str) -> Option<u16> {
    let lower = message.to_ascii_lowercase();
    for marker in ["status=", "status:", "status code", "http status"] {
        if let Some(index) = lower.find(marker) {
            let rest = &lower[index + marker.len()..];
            let digits: String = rest
                .chars()
                .skip_while(|ch| !ch.is_ascii_digit())
                .take_while(|ch| ch.is_ascii_digit())
                .collect();
            if digits.len() == 3
                && let Ok(status) = digits.parse::<u16>()
                && (100..600).contains(&status)
            {
                return Some(status);
            }
        }
    }
    None
}

pub(super) fn parse_aria2_length(value: &str, field: &str) -> Result<i64> {
    if value.trim().is_empty() {
        tracing::warn!(field, "aria2 RPC length field was empty; treating as zero");
        return Ok(0);
    }
    let parsed = value
        .parse::<u64>()
        .map_aster_err_ctx(&format!("parse {field}"), AsterError::storage_driver_error)?;
    u64_to_i64(parsed, field)
}

async fn downloaded_file_size(path: &Path) -> Result<i64> {
    let metadata = tokio::fs::metadata(path).await.map_aster_err_ctx(
        "stat offline download temp file",
        AsterError::storage_driver_error,
    )?;
    u64_to_i64(metadata.len(), "offline download temp file size")
}

async fn sha256_file(path: &Path) -> Result<String> {
    let mut file = tokio::fs::File::open(path).await.map_aster_err_ctx(
        "open offline download temp file",
        AsterError::storage_driver_error,
    )?;
    let mut hasher = crate::utils::hash::new_sha256();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).await.map_aster_err_ctx(
            "read offline download temp file",
            AsterError::storage_driver_error,
        )?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(crate::utils::hash::sha256_digest_to_hex(&hasher.finalize()))
}

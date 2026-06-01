use std::time::{Duration as StdDuration, Instant};

use async_trait::async_trait;
use futures::StreamExt;
use sha2::Digest;
use tokio::io::AsyncWriteExt;

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::PrimaryAppState;
use crate::utils::numbers::usize_to_i64;

use super::super::steps::{TASK_STEP_DOWNLOAD_SOURCE, set_task_step_active};
use super::super::{TaskLeaseGuard, mark_task_progress};
use super::{
    OfflineDownloadComplete, OfflineDownloadEngine, OfflineDownloadRateLimiter,
    OfflineDownloadStartRequest, PROGRESS_UPDATE_INTERVAL, declared_content_length,
    ensure_download_size_allowed, resolve_source_host, response_filename, transient_storage_error,
    verify_expected_sha256,
};

pub(super) struct BuiltinHttpOfflineDownloadEngine {
    max_bytes: i64,
    request_timeout: StdDuration,
}

impl BuiltinHttpOfflineDownloadEngine {
    pub(super) fn new(max_bytes: i64, request_timeout: StdDuration) -> Self {
        Self {
            max_bytes,
            request_timeout,
        }
    }
}

#[async_trait]
impl OfflineDownloadEngine for BuiltinHttpOfflineDownloadEngine {
    async fn download(
        &mut self,
        state: &PrimaryAppState,
        lease_guard: &TaskLeaseGuard,
        request: OfflineDownloadStartRequest,
        steps: &mut [super::super::TaskStepInfo],
    ) -> Result<OfflineDownloadComplete> {
        let resolved = resolve_source_host(&request.url).await?;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(self.request_timeout)
            .user_agent(crate::utils::OUTBOUND_HTTP_USER_AGENT)
            .resolve_to_addrs(&resolved.domain, &resolved.socket_addrs)
            .build()
            .map_aster_err_ctx(
                "build offline download HTTP client",
                AsterError::internal_error,
            )?;
        let response = client
            .get(request.url.clone())
            .send()
            .await
            .map_aster_err_ctx("request offline download source", transient_storage_error)?;
        let status = response.status();
        if status.is_redirection() {
            return Err(AsterError::validation_error(
                "offline download redirects are not supported",
            ));
        }
        if !status.is_success() {
            return Err(AsterError::storage_driver_error(format!(
                "transient: offline download source returned HTTP {status}"
            )));
        }

        let declared_content_length = declared_content_length(response.headers())?;
        if let Some(length) = declared_content_length {
            ensure_download_size_allowed(length, self.max_bytes)?;
        }
        let response_filename = response_filename(response.headers());
        let final_url = response.url().clone();
        let mut stream = response.bytes_stream();
        let mut file = tokio::fs::File::create(&request.temp_path)
            .await
            .map_aster_err_ctx(
                "create offline download temp file",
                AsterError::storage_driver_error,
            )?;
        let mut hasher = crate::utils::hash::new_sha256();
        let mut written = 0_i64;
        let progress_total = declared_content_length.unwrap_or(0).max(0);
        let mut last_progress = Instant::now()
            .checked_sub(PROGRESS_UPDATE_INTERVAL)
            .unwrap_or_else(Instant::now);
        let rate_limiter = OfflineDownloadRateLimiter::new(request.max_bytes_per_sec);

        while let Some(chunk) = stream.next().await {
            lease_guard.ensure_active()?;
            let chunk =
                chunk.map_aster_err_ctx("read offline download body", transient_storage_error)?;
            let chunk_len = usize_to_i64(chunk.len(), "offline download chunk size")?;
            written = written.checked_add(chunk_len).ok_or_else(|| {
                AsterError::file_too_large("offline download size exceeds supported range")
            })?;
            ensure_download_size_allowed(written, self.max_bytes)?;
            file.write_all(&chunk).await.map_aster_err_ctx(
                "write offline download temp file",
                AsterError::storage_driver_error,
            )?;
            hasher.update(&chunk);
            OfflineDownloadRateLimiter::throttle(rate_limiter.as_ref(), chunk.len(), lease_guard)
                .await?;

            if last_progress.elapsed() >= PROGRESS_UPDATE_INTERVAL {
                let status_text = format!("Downloaded {written} bytes");
                set_task_step_active(
                    steps,
                    TASK_STEP_DOWNLOAD_SOURCE,
                    Some(&status_text),
                    Some((written, progress_total.max(written))),
                )?;
                mark_task_progress(
                    state,
                    lease_guard,
                    written,
                    progress_total.max(written),
                    Some(&status_text),
                    steps,
                )
                .await?;
                last_progress = Instant::now();
            }
        }

        file.flush().await.map_aster_err_ctx(
            "flush offline download temp file",
            AsterError::storage_driver_error,
        )?;
        file.sync_all().await.map_aster_err_ctx(
            "sync offline download temp file",
            AsterError::storage_driver_error,
        )?;
        if let Some(length) = declared_content_length
            && written != length
        {
            return Err(AsterError::storage_driver_error(format!(
                "transient: offline download size mismatch: declared {length}, received {written}"
            )));
        }
        if written <= 0 {
            return Err(AsterError::validation_error(
                "offline download source returned an empty file",
            ));
        }
        let sha256 = crate::utils::hash::sha256_digest_to_hex(&hasher.finalize());
        verify_expected_sha256(request.expected_sha256.as_deref(), &sha256)?;

        Ok(OfflineDownloadComplete::new(
            final_url,
            response_filename,
            written,
            sha256,
            declared_content_length,
        ))
    }
}

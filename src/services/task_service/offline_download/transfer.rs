use std::num::NonZeroU32;

use governor::{Quota, RateLimiter};
use reqwest::header::CONTENT_LENGTH;

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::services::task_service::TaskLeaseGuard;
use crate::utils::numbers::{u64_to_i64, usize_to_u32};

pub(in crate::services::task_service) struct OfflineDownloadRateLimiter {
    limiter: governor::DefaultDirectRateLimiter,
    pub(super) max_batch_bytes: u32,
}

impl OfflineDownloadRateLimiter {
    pub(in crate::services::task_service) fn new(max_bytes_per_sec: Option<u64>) -> Option<Self> {
        let max_bytes_per_sec = max_bytes_per_sec?;
        let max_batch_bytes = u32::try_from(max_bytes_per_sec).unwrap_or(u32::MAX);
        let max_batch_bytes = NonZeroU32::new(max_batch_bytes)?;
        Some(Self {
            limiter: RateLimiter::direct(Quota::per_second(max_batch_bytes)),
            max_batch_bytes: max_batch_bytes.get(),
        })
    }

    pub(in crate::services::task_service) async fn throttle(
        limiter: Option<&Self>,
        chunk_len: usize,
        lease_guard: &TaskLeaseGuard,
    ) -> Result<()> {
        let Some(limiter) = limiter else {
            return Ok(());
        };
        let mut remaining = usize_to_u32(chunk_len, "offline download throttle chunk size")?;
        while remaining > 0 {
            lease_guard.ensure_active()?;
            let batch = remaining.min(limiter.max_batch_bytes);
            let batch = NonZeroU32::new(batch).ok_or_else(|| {
                AsterError::internal_error("offline download throttle batch cannot be zero")
            })?;
            tokio::select! {
                biased;
                shutdown = lease_guard.shutdown_requested() => shutdown?,
                result = limiter.limiter.until_n_ready(batch) => {
                    result.map_aster_err_ctx(
                        "reserve offline download throttle capacity",
                        AsterError::internal_error,
                    )?;
                }
            }
            remaining -= batch.get();
        }
        Ok(())
    }
}

pub(in crate::services::task_service) fn declared_content_length(
    headers: &reqwest::header::HeaderMap,
) -> Result<Option<i64>> {
    let Some(value) = headers.get(CONTENT_LENGTH) else {
        return Ok(None);
    };
    let Ok(raw) = value.to_str() else {
        return Ok(None);
    };
    let Ok(parsed) = raw.parse::<u64>() else {
        return Ok(None);
    };
    Ok(u64_to_i64(parsed, "offline download content length").ok())
}

pub(in crate::services::task_service) fn verify_expected_sha256(
    expected: Option<&str>,
    actual: &str,
) -> Result<()> {
    let Some(expected) = expected else {
        return Ok(());
    };
    if expected != actual {
        return Err(AsterError::validation_error(format!(
            "offline download sha256 mismatch: expected {expected}, got {actual}"
        )));
    }
    Ok(())
}

pub(in crate::services::task_service) fn ensure_download_size_allowed(
    size: i64,
    max_bytes: i64,
) -> Result<()> {
    if size > max_bytes {
        return Err(AsterError::file_too_large(format!(
            "offline download source size {size} exceeds server limit {max_bytes}"
        )));
    }
    Ok(())
}

pub(in crate::services::task_service) fn transient_storage_error(
    message: impl Into<String>,
) -> AsterError {
    AsterError::storage_driver_error(format!("transient: {}", message.into()))
}

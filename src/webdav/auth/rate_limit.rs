//! WebDAV Basic Auth 的 IP 限流与账号失败退避。

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use actix_web::HttpRequest;
use aster_forge_cache::CacheExt;
use governor::{DefaultKeyedRateLimiter, Quota, RateLimiter};

use crate::config::RateLimitTier;
use crate::runtime::SharedRuntimeState;

use super::cache::username_cache_component;

const FAILURE_WINDOW_SECS: u64 = 15 * 60;
const FAILURE_THRESHOLD: u32 = 3;
const FAILURE_SLOT_COUNT: u32 = 8;
const MAX_BACKOFF_SECS: u64 = 60;
const CLEANUP_INTERVAL: u64 = 1_024;

type IpRateLimiter = DefaultKeyedRateLimiter<IpAddr>;

#[derive(Clone)]
pub(crate) struct WebdavAuthProtection {
    enabled: bool,
    ip_limiter: Arc<IpRateLimiter>,
    trusted_proxies: Arc<Vec<String>>,
    checks: Arc<AtomicU64>,
}

impl WebdavAuthProtection {
    #[expect(
        clippy::expect_used,
        reason = "NonZeroU64 seconds_per_request always creates a non-zero duration"
    )]
    pub(crate) fn new(enabled: bool, tier: &RateLimitTier, trusted_proxies: &[String]) -> Self {
        let quota = Quota::with_period(Duration::from_secs(tier.seconds_per_request.get()))
            .expect("non-zero WebDAV auth period should build")
            .allow_burst(tier.burst_size);
        Self {
            enabled,
            ip_limiter: Arc::new(RateLimiter::keyed(quota)),
            trusted_proxies: Arc::new(trusted_proxies.to_vec()),
            checks: Arc::new(AtomicU64::new(0)),
        }
    }

    pub(super) fn check_ip(&self, request: &HttpRequest) -> Result<(), u64> {
        if !self.enabled {
            return Ok(());
        }
        let peer = request
            .peer_addr()
            .map(|address| address.ip())
            .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
        let client_ip = aster_forge_actix_middleware::client_ip::real_ip_from_headers(
            request.headers(),
            peer,
            &self.trusted_proxies,
        );

        self.cleanup_if_needed();
        self.ip_limiter.check_key(&client_ip).map_err(|rejection| {
            aster_forge_actix_middleware::rate_limit::retry_after_seconds(&rejection).max(1)
        })
    }

    fn cleanup_if_needed(&self) {
        if self
            .checks
            .fetch_add(1, Ordering::Relaxed)
            .is_multiple_of(CLEANUP_INTERVAL)
        {
            self.ip_limiter.retain_recent();
        }
    }

    pub(super) const fn enabled(&self) -> bool {
        self.enabled
    }
}

pub(super) async fn check_username_backoff(
    state: &impl SharedRuntimeState,
    enabled: bool,
    username: &str,
) -> Result<(), u64> {
    if !enabled {
        return Ok(());
    }
    let key = backoff_key(username);
    let Some(blocked_until) = state.cache().get::<u64>(&key).await else {
        return Ok(());
    };
    let now = unix_timestamp();
    if blocked_until > now {
        return Err(blocked_until - now);
    }
    state.cache().delete(&key).await;
    Ok(())
}

pub(super) async fn record_username_failure(
    state: &impl SharedRuntimeState,
    enabled: bool,
    username: &str,
) -> Option<u64> {
    if !enabled {
        return None;
    }
    let mut failure_count = FAILURE_SLOT_COUNT;
    for slot in 1..=FAILURE_SLOT_COUNT {
        if state
            .cache()
            .set_bytes_if_absent(
                &failure_slot_key(username, slot),
                vec![1],
                Some(FAILURE_WINDOW_SECS),
            )
            .await
        {
            failure_count = slot;
            break;
        }
    }

    if failure_count < FAILURE_THRESHOLD {
        return None;
    }

    let exponent = failure_count.saturating_sub(FAILURE_THRESHOLD);
    let backoff_secs = 2_u64
        .checked_shl(exponent)
        .unwrap_or(MAX_BACKOFF_SECS)
        .min(MAX_BACKOFF_SECS);
    let blocked_until = unix_timestamp().saturating_add(backoff_secs);
    state
        .cache()
        .set(&backoff_key(username), &blocked_until, Some(backoff_secs))
        .await;
    Some(backoff_secs)
}

pub(super) async fn clear_username_failures(
    state: &impl SharedRuntimeState,
    enabled: bool,
    username: &str,
) {
    if !enabled {
        return;
    }
    let mut keys = Vec::with_capacity(FAILURE_SLOT_COUNT as usize + 1);
    keys.push(backoff_key(username));
    keys.extend((1..=FAILURE_SLOT_COUNT).map(|slot| failure_slot_key(username, slot)));
    state.cache().delete_many(&keys).await;
}

fn normalized_username_component(username: &str) -> String {
    username_cache_component(&username.trim().to_ascii_lowercase())
}

fn failure_prefix(username: &str) -> String {
    format!(
        "webdav_auth_failure:{}:",
        normalized_username_component(username)
    )
}

fn failure_slot_key(username: &str, slot: u32) -> String {
    format!("{}slot:{slot}", failure_prefix(username))
}

fn backoff_key(username: &str) -> String {
    format!("{}backoff", failure_prefix(username))
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::test_support::CacheOnlyState;

    #[tokio::test]
    async fn username_failure_keys_are_hashed_normalized_and_bounded() {
        let key = failure_slot_key(" Alice ", 1);
        assert!(!key.contains("Alice"));
        assert_eq!(key, failure_slot_key("alice", 1));

        let state = CacheOnlyState::new().await;
        for _ in 0..FAILURE_SLOT_COUNT + 2 {
            record_username_failure(&state, true, "Alice").await;
        }
        for slot in 1..=FAILURE_SLOT_COUNT {
            assert!(
                state
                    .cache()
                    .get_bytes(&failure_slot_key("alice", slot))
                    .await
                    .is_some()
            );
        }
        assert!(
            state
                .cache()
                .get_bytes(&failure_slot_key("alice", FAILURE_SLOT_COUNT + 1))
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn username_backoff_starts_after_three_failures_and_clears_on_success() {
        let state = CacheOnlyState::new().await;

        assert_eq!(record_username_failure(&state, true, "alice").await, None);
        assert_eq!(record_username_failure(&state, true, "alice").await, None);
        assert_eq!(
            record_username_failure(&state, true, "alice").await,
            Some(2)
        );
        assert!(check_username_backoff(&state, true, "alice").await.is_err());

        clear_username_failures(&state, true, "alice").await;
        assert_eq!(check_username_backoff(&state, true, "alice").await, Ok(()));
        assert_eq!(record_username_failure(&state, true, "alice").await, None);
    }
}

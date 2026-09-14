//! Demand-driven capacity observation for upload admission.
//!
//! Capacity is an advisory snapshot, not a reservation. The coordinator keeps provider I/O out
//! of the common upload-init path by serving fresh observations, using stale-but-sufficient data
//! while one request refreshes it, and confirming stale rejection decisions before they reach a
//! client. Probe tasks are request-independent, bounded by timeout, coalesced per policy, and
//! limited globally.

use std::sync::Arc;
use std::time::{Duration, Instant};

use aster_drive_metrics::SharedMetricsRecorder;
use aster_drive_model::entities::storage_policy;
use aster_drive_storage::{
    StorageCapacityAssessment, StorageCapacityInfo, StorageCapacityStatus, StorageDriver,
    StorageErrorKind,
};
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use parking_lot::RwLock;
use tokio::sync::{Mutex, Notify, Semaphore};

use crate::errors::{AsterError, Result};

const SUPPORTED_FRESH_FOR: Duration = Duration::from_secs(2);
const INCONCLUSIVE_FRESH_FOR: Duration = Duration::from_millis(250);
const UNSUPPORTED_FRESH_FOR: Duration = Duration::from_secs(30);
const SUFFICIENT_STALE_FOR: Duration = Duration::from_secs(30);
const CAPACITY_PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const CAPACITY_PROBE_WAIT_TIMEOUT: Duration = Duration::from_millis(2_250);
const MAX_CONCURRENT_CAPACITY_PROBES: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
struct CapacityPolicyIdentity {
    connector_id: String,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<&storage_policy::Model> for CapacityPolicyIdentity {
    fn from(policy: &storage_policy::Model) -> Self {
        Self {
            connector_id: policy.connector_id.clone(),
            updated_at: policy.updated_at,
        }
    }
}

#[derive(Debug, Clone)]
struct CachedCapacityObservation {
    result: Result<StorageCapacityInfo>,
    cached_at: Instant,
}

impl CachedCapacityObservation {
    fn age(&self) -> Duration {
        self.cached_at.elapsed()
    }

    fn fresh_for(&self) -> Duration {
        match &self.result {
            Ok(capacity) => match capacity.status {
                StorageCapacityStatus::Supported if capacity.available_bytes.is_some() => {
                    SUPPORTED_FRESH_FOR
                }
                StorageCapacityStatus::Unsupported => UNSUPPORTED_FRESH_FOR,
                StorageCapacityStatus::Supported | StorageCapacityStatus::Unavailable => {
                    INCONCLUSIVE_FRESH_FOR
                }
            },
            Err(_) => INCONCLUSIVE_FRESH_FOR,
        }
    }

    fn assessment(&self, required_bytes: i64) -> Result<StorageCapacityAssessment> {
        self.result
            .clone()
            .map(|capacity| capacity.assess(required_bytes))
    }

    fn stale_sufficient_assessment(
        &self,
        required_bytes: i64,
    ) -> Option<StorageCapacityAssessment> {
        if self.age() > SUFFICIENT_STALE_FOR {
            return None;
        }
        match self.assessment(required_bytes) {
            Ok(assessment @ StorageCapacityAssessment::Sufficient { .. }) => Some(assessment),
            _ => None,
        }
    }

    fn usable_observation(&self) -> bool {
        matches!(
            &self.result,
            Ok(StorageCapacityInfo {
                status: StorageCapacityStatus::Supported,
                available_bytes: Some(_),
                ..
            } | StorageCapacityInfo {
                status: StorageCapacityStatus::Unsupported,
                ..
            })
        )
    }
}

#[derive(Debug, Clone, Default)]
struct CapacityProbeCache {
    latest: Option<CachedCapacityObservation>,
    last_usable: Option<CachedCapacityObservation>,
}

struct CapacityProbeState {
    identity: CapacityPolicyIdentity,
    cache: RwLock<CapacityProbeCache>,
    refresh_lock: Arc<Mutex<()>>,
    refresh_version: std::sync::atomic::AtomicU64,
    refresh_notify: Notify,
}

impl CapacityProbeState {
    fn new(identity: CapacityPolicyIdentity) -> Self {
        Self {
            identity,
            cache: RwLock::new(CapacityProbeCache::default()),
            refresh_lock: Arc::new(Mutex::new(())),
            refresh_version: std::sync::atomic::AtomicU64::new(0),
            refresh_notify: Notify::new(),
        }
    }

    fn cache(&self) -> CapacityProbeCache {
        self.cache.read().clone()
    }

    fn publish(&self, result: Result<StorageCapacityInfo>) {
        let observation = CachedCapacityObservation {
            result,
            cached_at: Instant::now(),
        };
        let mut cache = self.cache.write();
        if observation.usable_observation() {
            cache.last_usable = Some(observation.clone());
        }
        cache.latest = Some(observation);
        drop(cache);
        self.refresh_version
            .fetch_add(1, std::sync::atomic::Ordering::Release);
        self.refresh_notify.notify_waiters();
    }

    fn version(&self) -> u64 {
        self.refresh_version
            .load(std::sync::atomic::Ordering::Acquire)
    }
}

#[derive(Clone, Copy)]
struct CapacityProbeLimits {
    probe_timeout: Duration,
    wait_timeout: Duration,
}

impl Default for CapacityProbeLimits {
    fn default() -> Self {
        Self {
            probe_timeout: CAPACITY_PROBE_TIMEOUT,
            wait_timeout: CAPACITY_PROBE_WAIT_TIMEOUT,
        }
    }
}

pub(crate) struct CapacityProbeCoordinator {
    states: DashMap<i64, Arc<CapacityProbeState>>,
    probe_limit: Arc<Semaphore>,
    limits: CapacityProbeLimits,
}

impl CapacityProbeCoordinator {
    pub(crate) fn new() -> Self {
        Self::with_limits(
            MAX_CONCURRENT_CAPACITY_PROBES,
            CapacityProbeLimits::default(),
        )
    }

    fn with_limits(max_concurrent_probes: usize, limits: CapacityProbeLimits) -> Self {
        Self {
            states: DashMap::new(),
            probe_limit: Arc::new(Semaphore::new(max_concurrent_probes.max(1))),
            limits,
        }
    }

    pub(crate) fn invalidate(&self, policy_id: i64) {
        self.states.remove(&policy_id);
    }

    pub(crate) fn invalidate_all(&self) {
        self.states.clear();
    }

    pub(crate) async fn assess(
        &self,
        policy: &storage_policy::Model,
        driver: Arc<dyn StorageDriver>,
        required_bytes: i64,
        metrics: SharedMetricsRecorder,
    ) -> Result<StorageCapacityAssessment> {
        if required_bytes < 0 {
            return Err(AsterError::validation_error(
                "capacity assessment required_bytes must be non-negative",
            ));
        }

        let state = self.state_for(policy);
        let cache = state.cache();
        if let Some(latest) = cache.latest.as_ref()
            && latest.age() <= latest.fresh_for()
        {
            if !latest.usable_observation()
                && let Some(stale) = cache
                    .last_usable
                    .as_ref()
                    .and_then(|usable| usable.stale_sufficient_assessment(required_bytes))
            {
                metrics.record_storage_capacity_probe_cache("stale_after_error");
                return Ok(stale);
            }
            metrics.record_storage_capacity_probe_cache("fresh");
            return latest.assessment(required_bytes);
        }
        if let Some(assessment) = cache
            .last_usable
            .as_ref()
            .and_then(|usable| usable.stale_sufficient_assessment(required_bytes))
        {
            metrics.record_storage_capacity_probe_cache("stale_sufficient");
            self.trigger_refresh(
                policy.id,
                policy.connector_id.clone(),
                state,
                driver,
                metrics,
            );
            return Ok(assessment);
        }
        if cache.latest.is_some() {
            metrics.record_storage_capacity_probe_cache("confirm_refresh");
        } else {
            metrics.record_storage_capacity_probe_cache("cold");
        }

        self.refresh_and_wait(
            policy.id,
            policy.connector_id.clone(),
            state,
            driver,
            required_bytes,
            metrics,
        )
        .await
    }

    fn state_for(&self, policy: &storage_policy::Model) -> Arc<CapacityProbeState> {
        let identity = CapacityPolicyIdentity::from(policy);
        match self.states.entry(policy.id) {
            Entry::Occupied(mut entry) => {
                if entry.get().identity == identity {
                    entry.get().clone()
                } else {
                    let state = Arc::new(CapacityProbeState::new(identity));
                    entry.insert(state.clone());
                    state
                }
            }
            Entry::Vacant(entry) => {
                let state = Arc::new(CapacityProbeState::new(identity));
                entry.insert(state.clone());
                state
            }
        }
    }

    async fn refresh_and_wait(
        &self,
        policy_id: i64,
        connector_id: String,
        state: Arc<CapacityProbeState>,
        driver: Arc<dyn StorageDriver>,
        required_bytes: i64,
        metrics: SharedMetricsRecorder,
    ) -> Result<StorageCapacityAssessment> {
        let baseline_version = state.version();
        let notified = state.refresh_notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        self.trigger_refresh(policy_id, connector_id, state.clone(), driver, metrics);

        if state.version() == baseline_version
            && tokio::time::timeout(self.limits.wait_timeout, &mut notified)
                .await
                .is_err()
        {
            return Err(capacity_probe_timeout_error(
                "wait for coalesced storage capacity probe",
            ));
        }
        state
            .cache()
            .latest
            .ok_or_else(|| {
                AsterError::internal_error(
                    "capacity probe completed without publishing an observation",
                )
            })?
            .assessment(required_bytes)
    }

    fn trigger_refresh(
        &self,
        policy_id: i64,
        connector_id: String,
        state: Arc<CapacityProbeState>,
        driver: Arc<dyn StorageDriver>,
        metrics: SharedMetricsRecorder,
    ) {
        let Ok(refresh_guard) = state.refresh_lock.clone().try_lock_owned() else {
            return;
        };
        let probe_limit = self.probe_limit.clone();
        let probe_timeout = self.limits.probe_timeout;
        tokio::spawn(async move {
            let _refresh_guard = refresh_guard;
            let started_at = Instant::now();
            let probe = async {
                let _permit = probe_limit.acquire_owned().await.map_err(|_| {
                    AsterError::internal_error("storage capacity probe limiter is closed")
                })?;
                match driver.capacity_info().await {
                    Ok(capacity) => Ok(capacity),
                    Err(error) if error.kind() == StorageErrorKind::Unsupported => Ok(
                        StorageCapacityInfo::unsupported(format!("{connector_id}_driver")),
                    ),
                    Err(error) => Err(AsterError::from(error)),
                }
            };
            let (result, outcome) = match tokio::time::timeout(probe_timeout, probe).await {
                Ok(Ok(capacity)) => {
                    let outcome = capacity_status_metric_label(capacity.status);
                    (Ok(capacity), outcome)
                }
                Ok(Err(error)) => {
                    tracing::warn!(
                        policy_id,
                        error_kind = error
                            .storage_error_kind()
                            .map(StorageErrorKind::as_str)
                            .unwrap_or("unknown"),
                        "storage capacity probe failed"
                    );
                    (Err(error), "failure")
                }
                Err(_) => {
                    tracing::warn!(policy_id, "storage capacity probe timed out");
                    (
                        Err(capacity_probe_timeout_error("probe storage capacity")),
                        "timeout",
                    )
                }
            };
            metrics.record_storage_capacity_probe(outcome, started_at.elapsed().as_secs_f64());
            state.publish(result);
        });
    }
}

fn capacity_status_metric_label(status: StorageCapacityStatus) -> &'static str {
    match status {
        StorageCapacityStatus::Supported => "supported",
        StorageCapacityStatus::Unsupported => "unsupported",
        StorageCapacityStatus::Unavailable => "unavailable",
    }
}

fn capacity_probe_timeout_error(context: &str) -> AsterError {
    AsterError::from(aster_drive_storage::storage_driver_error(
        StorageErrorKind::Transient,
        format!("{context} timed out"),
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use tokio::io::AsyncRead;

    use super::*;
    use aster_drive_storage::{BlobMetadata, StorageError, storage_driver_error};

    #[derive(Default)]
    struct ProbeConcurrency {
        active: AtomicUsize,
        max_active: AtomicUsize,
    }

    struct ActiveProbeGuard<'a> {
        concurrency: &'a ProbeConcurrency,
    }

    impl Drop for ActiveProbeGuard<'_> {
        fn drop(&mut self) {
            self.concurrency.active.fetch_sub(1, Ordering::SeqCst);
        }
    }

    struct ProbeDriver {
        calls: AtomicUsize,
        responses: std::sync::Mutex<VecDeque<aster_drive_storage::Result<StorageCapacityInfo>>>,
        fallback: aster_drive_storage::Result<StorageCapacityInfo>,
        delay: Duration,
        concurrency: Arc<ProbeConcurrency>,
    }

    impl ProbeDriver {
        fn new(
            responses: Vec<aster_drive_storage::Result<StorageCapacityInfo>>,
            fallback: aster_drive_storage::Result<StorageCapacityInfo>,
            delay: Duration,
        ) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                responses: std::sync::Mutex::new(responses.into()),
                fallback,
                delay,
                concurrency: Arc::new(ProbeConcurrency::default()),
            }
        }

        fn with_concurrency(mut self, concurrency: Arc<ProbeConcurrency>) -> Self {
            self.concurrency = concurrency;
            self
        }
    }

    #[async_trait]
    impl StorageDriver for ProbeDriver {
        async fn put(&self, path: &str, _data: &[u8]) -> aster_drive_storage::Result<String> {
            Ok(path.to_string())
        }

        async fn get(&self, _path: &str) -> aster_drive_storage::Result<Vec<u8>> {
            Ok(Vec::new())
        }

        async fn get_stream(
            &self,
            _path: &str,
        ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
            Ok(Box::new(tokio::io::empty()))
        }

        async fn delete(&self, _path: &str) -> aster_drive_storage::Result<()> {
            Ok(())
        }

        async fn exists(&self, _path: &str) -> aster_drive_storage::Result<bool> {
            Ok(false)
        }

        async fn metadata(&self, _path: &str) -> aster_drive_storage::Result<BlobMetadata> {
            Ok(BlobMetadata {
                size: 0,
                content_type: None,
            })
        }

        async fn capacity_info(&self) -> aster_drive_storage::Result<StorageCapacityInfo> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let active = self.concurrency.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.concurrency
                .max_active
                .fetch_max(active, Ordering::SeqCst);
            let _guard = ActiveProbeGuard {
                concurrency: &self.concurrency,
            };
            tokio::time::sleep(self.delay).await;
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| self.fallback.clone())
        }
    }

    fn capacity(
        status: StorageCapacityStatus,
        available_bytes: Option<i64>,
    ) -> StorageCapacityInfo {
        StorageCapacityInfo {
            status,
            total_bytes: available_bytes,
            available_bytes,
            used_bytes: None,
            source: "test".to_string(),
            observed_at: chrono::Utc::now(),
        }
    }

    fn policy(id: i64) -> storage_policy::Model {
        let now = chrono::Utc::now();
        storage_policy::Model {
            id,
            name: format!("Policy {id}"),
            connector_id: "asterdrive.storage.local".to_string(),
            storage_config: aster_drive_model::types::StoredStoragePolicyConfig::from(
                r#"{"format_version":1,"connector":{"format_version":1,"connector_id":"asterdrive.storage.local","schema_version":1,"values":{"base_path":"/tmp","content_dedup":false}},"behavior":{"format_version":1,"schema_version":1,"values":{}}}"#.to_string(),
            ),
            max_file_size: 0,
            allowed_types: aster_drive_model::types::StoredStoragePolicyAllowedTypes::from(
                "[]".to_string(),
            ),
            is_default: false,
            chunk_size: 1024,
            created_at: now,
            updated_at: now,
        }
    }

    fn coordinator(max_concurrent: usize) -> CapacityProbeCoordinator {
        CapacityProbeCoordinator::with_limits(
            max_concurrent,
            CapacityProbeLimits {
                probe_timeout: Duration::from_millis(100),
                wait_timeout: Duration::from_millis(150),
            },
        )
    }

    fn sufficient(bytes: i64) -> aster_drive_storage::Result<StorageCapacityInfo> {
        Ok(capacity(StorageCapacityStatus::Supported, Some(bytes)))
    }

    fn transient_error() -> aster_drive_storage::Result<StorageCapacityInfo> {
        Err(storage_driver_error(
            StorageErrorKind::Transient,
            "injected capacity failure",
        ))
    }

    fn force_cached_age(coordinator: &CapacityProbeCoordinator, policy_id: i64, age: Duration) {
        let state = coordinator.states.get(&policy_id).unwrap().clone();
        let mut cache = state.cache.write();
        if let Some(latest) = cache.latest.as_mut() {
            latest.cached_at = Instant::now() - age;
        }
        if let Some(usable) = cache.last_usable.as_mut() {
            usable.cached_at = Instant::now() - age;
        }
    }

    async fn wait_for_calls(driver: &ProbeDriver, expected: usize) {
        tokio::time::timeout(Duration::from_secs(1), async {
            while driver.calls.load(Ordering::SeqCst) < expected {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("capacity probe should start");
    }

    #[tokio::test]
    async fn cold_probe_is_singleflight_and_survives_first_waiter_cancellation() {
        let coordinator = Arc::new(coordinator(8));
        let policy = policy(1);
        let driver = Arc::new(ProbeDriver::new(
            vec![],
            sufficient(100),
            Duration::from_millis(40),
        ));
        let first_coordinator = coordinator.clone();
        let first_driver = driver.clone();
        let first_policy = policy.clone();
        let first = tokio::spawn(async move {
            first_coordinator
                .assess(
                    &first_policy,
                    first_driver,
                    10,
                    aster_drive_metrics::NoopMetrics::arc(),
                )
                .await
        });
        wait_for_calls(&driver, 1).await;
        first.abort();

        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..16 {
            let coordinator = coordinator.clone();
            let driver = driver.clone();
            let policy = policy.clone();
            tasks.spawn(async move {
                coordinator
                    .assess(&policy, driver, 10, aster_drive_metrics::NoopMetrics::arc())
                    .await
            });
        }
        while let Some(result) = tasks.join_next().await {
            assert!(matches!(
                result.unwrap().unwrap(),
                StorageCapacityAssessment::Sufficient { .. }
            ));
        }
        assert_eq!(driver.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn fresh_observation_is_reused_and_explicit_invalidation_reprobes() {
        let coordinator = coordinator(8);
        let policy = policy(2);
        let driver = Arc::new(ProbeDriver::new(vec![], sufficient(100), Duration::ZERO));

        for _ in 0..2 {
            assert!(matches!(
                coordinator
                    .assess(
                        &policy,
                        driver.clone(),
                        10,
                        aster_drive_metrics::NoopMetrics::arc(),
                    )
                    .await
                    .unwrap(),
                StorageCapacityAssessment::Sufficient { .. }
            ));
        }
        assert_eq!(driver.calls.load(Ordering::SeqCst), 1);

        coordinator.invalidate(policy.id);
        coordinator
            .assess(
                &policy,
                driver.clone(),
                10,
                aster_drive_metrics::NoopMetrics::arc(),
            )
            .await
            .unwrap();
        assert_eq!(driver.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn stale_sufficient_is_served_while_single_refresh_updates_snapshot() {
        let coordinator = coordinator(8);
        let policy = policy(3);
        let driver = Arc::new(ProbeDriver::new(
            vec![sufficient(100), sufficient(5)],
            sufficient(5),
            Duration::from_millis(80),
        ));
        coordinator
            .assess(
                &policy,
                driver.clone(),
                10,
                aster_drive_metrics::NoopMetrics::arc(),
            )
            .await
            .unwrap();
        force_cached_age(&coordinator, policy.id, Duration::from_secs(3));

        let stale = tokio::time::timeout(
            Duration::from_millis(50),
            coordinator.assess(
                &policy,
                driver.clone(),
                10,
                aster_drive_metrics::NoopMetrics::arc(),
            ),
        )
        .await
        .expect("stale sufficient observation should not wait for provider I/O")
        .unwrap();
        assert!(matches!(
            stale,
            StorageCapacityAssessment::Sufficient { .. }
        ));
        wait_for_calls(&driver, 2).await;
        tokio::time::sleep(Duration::from_millis(90)).await;
        assert!(matches!(
            coordinator
                .assess(
                    &policy,
                    driver.clone(),
                    10,
                    aster_drive_metrics::NoopMetrics::arc(),
                )
                .await
                .unwrap(),
            StorageCapacityAssessment::Insufficient { .. }
        ));
        assert_eq!(driver.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn failed_stale_refresh_keeps_sufficient_snapshot_without_masking_large_request() {
        let coordinator = coordinator(8);
        let policy = policy(21);
        let driver = Arc::new(ProbeDriver::new(
            vec![sufficient(100), transient_error()],
            transient_error(),
            Duration::from_millis(20),
        ));
        coordinator
            .assess(
                &policy,
                driver.clone(),
                10,
                aster_drive_metrics::NoopMetrics::arc(),
            )
            .await
            .unwrap();
        force_cached_age(&coordinator, policy.id, Duration::from_secs(3));

        assert!(matches!(
            coordinator
                .assess(
                    &policy,
                    driver.clone(),
                    10,
                    aster_drive_metrics::NoopMetrics::arc(),
                )
                .await
                .unwrap(),
            StorageCapacityAssessment::Sufficient { .. }
        ));
        wait_for_calls(&driver, 2).await;
        tokio::time::sleep(Duration::from_millis(30)).await;

        assert!(matches!(
            coordinator
                .assess(
                    &policy,
                    driver.clone(),
                    10,
                    aster_drive_metrics::NoopMetrics::arc(),
                )
                .await
                .unwrap(),
            StorageCapacityAssessment::Sufficient { .. }
        ));
        assert!(
            coordinator
                .assess(
                    &policy,
                    driver.clone(),
                    101,
                    aster_drive_metrics::NoopMetrics::arc(),
                )
                .await
                .is_err(),
            "a recent refresh error must replace a stale insufficient conclusion"
        );
        assert_eq!(driver.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn stale_insufficient_is_confirmed_before_rejection() {
        let coordinator = coordinator(8);
        let policy = policy(4);
        let driver = Arc::new(ProbeDriver::new(
            vec![sufficient(5), sufficient(100)],
            sufficient(100),
            Duration::ZERO,
        ));
        let first = coordinator
            .assess(
                &policy,
                driver.clone(),
                10,
                aster_drive_metrics::NoopMetrics::arc(),
            )
            .await
            .unwrap();
        assert!(matches!(
            first,
            StorageCapacityAssessment::Insufficient { .. }
        ));
        force_cached_age(&coordinator, policy.id, Duration::from_secs(3));

        let confirmed = coordinator
            .assess(
                &policy,
                driver.clone(),
                10,
                aster_drive_metrics::NoopMetrics::arc(),
            )
            .await
            .unwrap();
        assert!(matches!(
            confirmed,
            StorageCapacityAssessment::Sufficient { .. }
        ));
        assert_eq!(driver.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn transient_failure_is_negative_cached_then_retried() {
        let coordinator = coordinator(8);
        let policy = policy(5);
        let driver = Arc::new(ProbeDriver::new(
            vec![transient_error(), sufficient(100)],
            sufficient(100),
            Duration::ZERO,
        ));
        assert!(
            coordinator
                .assess(
                    &policy,
                    driver.clone(),
                    10,
                    aster_drive_metrics::NoopMetrics::arc(),
                )
                .await
                .is_err()
        );
        assert!(
            coordinator
                .assess(
                    &policy,
                    driver.clone(),
                    10,
                    aster_drive_metrics::NoopMetrics::arc(),
                )
                .await
                .is_err()
        );
        assert_eq!(driver.calls.load(Ordering::SeqCst), 1);

        force_cached_age(&coordinator, policy.id, Duration::from_secs(1));
        assert!(matches!(
            coordinator
                .assess(
                    &policy,
                    driver.clone(),
                    10,
                    aster_drive_metrics::NoopMetrics::arc(),
                )
                .await
                .unwrap(),
            StorageCapacityAssessment::Sufficient { .. }
        ));
        assert_eq!(driver.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn runtime_unsupported_is_cached_as_capability_result() {
        let coordinator = coordinator(8);
        let policy = policy(6);
        let driver = Arc::new(ProbeDriver::new(
            vec![],
            Err(StorageError::new(
                StorageErrorKind::Unsupported,
                "capacity unsupported",
            )),
            Duration::ZERO,
        ));
        for _ in 0..2 {
            assert_eq!(
                coordinator
                    .assess(
                        &policy,
                        driver.clone(),
                        10,
                        aster_drive_metrics::NoopMetrics::arc(),
                    )
                    .await
                    .unwrap(),
                StorageCapacityAssessment::Unsupported
            );
        }
        assert_eq!(driver.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn probe_timeout_is_bounded_and_retryable() {
        let coordinator = CapacityProbeCoordinator::with_limits(
            1,
            CapacityProbeLimits {
                probe_timeout: Duration::from_millis(20),
                wait_timeout: Duration::from_millis(50),
            },
        );
        let policy = policy(7);
        let driver = Arc::new(ProbeDriver::new(
            vec![],
            sufficient(100),
            Duration::from_millis(100),
        ));

        let started_at = Instant::now();
        let error = coordinator
            .assess(
                &policy,
                driver.clone(),
                10,
                aster_drive_metrics::NoopMetrics::arc(),
            )
            .await
            .unwrap_err();
        assert!(started_at.elapsed() < Duration::from_millis(80));
        assert_eq!(
            error.storage_error_kind(),
            Some(StorageErrorKind::Transient)
        );
        assert_eq!(driver.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn global_probe_limit_bounds_different_policies() {
        let coordinator = Arc::new(coordinator(2));
        let concurrency = Arc::new(ProbeConcurrency::default());
        let mut tasks = tokio::task::JoinSet::new();
        for id in 10..16 {
            let coordinator = coordinator.clone();
            let policy = policy(id);
            let driver = Arc::new(
                ProbeDriver::new(vec![], sufficient(100), Duration::from_millis(20))
                    .with_concurrency(concurrency.clone()),
            );
            tasks.spawn(async move {
                coordinator
                    .assess(&policy, driver, 10, aster_drive_metrics::NoopMetrics::arc())
                    .await
            });
        }
        while let Some(result) = tasks.join_next().await {
            assert!(matches!(
                result.unwrap().unwrap(),
                StorageCapacityAssessment::Sufficient { .. }
            ));
        }
        assert_eq!(concurrency.max_active.load(Ordering::SeqCst), 2);
        assert_eq!(concurrency.active.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn policy_revision_change_does_not_reuse_old_observation() {
        let coordinator = coordinator(8);
        let mut policy = policy(20);
        let driver = Arc::new(ProbeDriver::new(
            vec![sufficient(100), sufficient(5)],
            sufficient(5),
            Duration::ZERO,
        ));
        coordinator
            .assess(
                &policy,
                driver.clone(),
                10,
                aster_drive_metrics::NoopMetrics::arc(),
            )
            .await
            .unwrap();
        policy.updated_at += chrono::Duration::seconds(1);

        assert!(matches!(
            coordinator
                .assess(
                    &policy,
                    driver.clone(),
                    10,
                    aster_drive_metrics::NoopMetrics::arc(),
                )
                .await
                .unwrap(),
            StorageCapacityAssessment::Insufficient { .. }
        ));
        assert_eq!(driver.calls.load(Ordering::SeqCst), 2);
    }
}

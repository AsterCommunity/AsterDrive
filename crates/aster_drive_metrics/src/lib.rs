//! AsterDrive product metrics contracts and AsterForge adapters.
#![deny(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::unreachable,
        clippy::expect_used,
        clippy::panic,
        clippy::unimplemented,
        clippy::todo
    )
)]

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

#[cfg(feature = "metrics")]
use aster_forge_metrics::MetricsRecorder as ForgeMetricsRecorder;
use aster_forge_metrics::{DbQueryMetric, SharedMetricsRecorder as SharedForgeMetricsRecorder};
use aster_forge_runtime::{HealthCheckScope, SystemHealthReport};
use tokio_util::sync::CancellationToken;

/// Records AsterDrive product and shared infrastructure metrics.
///
/// Every method defaults to a no-op so tests and builds without the `metrics`
/// feature can execute the same product paths without conditional branches.
#[expect(
    unused_variables,
    reason = "default no-op metric methods intentionally ignore their inputs"
)]
pub trait MetricsRecorder: Send + Sync {
    /// Returns whether this recorder performs real metric collection.
    ///
    /// Callers may use this to skip collection work that has a measurable cost,
    /// such as database callbacks or HTTP route-label resolution.
    fn enabled(&self) -> bool {
        false
    }

    /// Returns the Forge recorder used by shared middleware and database hooks.
    fn forge_recorder(&self) -> SharedForgeMetricsRecorder {
        aster_forge_metrics::NoopMetrics::arc()
    }

    /// Records one completed HTTP request.
    fn record_http_request(&self, method: &str, route: &str, status: u16, duration_seconds: f64) {}

    /// Records one database query measurement.
    fn record_db_query(&self, metric: &DbQueryMetric) {}

    /// Records an authentication event.
    fn record_auth_event(&self, action: &'static str, status: &'static str, reason: &'static str) {}

    /// Records a file upload attempt.
    fn record_file_upload(&self, mode: &'static str, status: &'static str) {}

    /// Records a file download attempt.
    fn record_file_download(&self, source: &'static str, outcome: &'static str, has_range: bool) {}

    /// Records the creation of an upload session.
    fn record_upload_session(&self, mode: &'static str) {}

    /// Records an upload-session lifecycle transition.
    fn record_upload_session_event(
        &self,
        mode: &'static str,
        event: &'static str,
        status: &'static str,
    ) {
    }

    /// Records a background-task status transition.
    fn record_background_task_transition(&self, kind: &'static str, status: &'static str) {}

    /// Records one runtime configuration reload decision.
    fn record_config_reload(
        &self,
        source: &'static str,
        decision: &'static str,
        status: &'static str,
        changed_keys: u64,
        duration_seconds: f64,
    ) {
    }

    /// Records one runtime configuration mutation.
    fn record_config_mutation(
        &self,
        source: &'static str,
        operation: &'static str,
        status: &'static str,
        changed_keys: u64,
    ) {
    }

    /// Sets the number of background tasks waiting to run.
    fn set_background_tasks_pending(&self, pending: u64) {}

    /// Records one storage-driver operation and its duration.
    fn record_storage_driver_operation(
        &self,
        driver: &'static str,
        operation: &'static str,
        status: &'static str,
        kind: &'static str,
        duration_seconds: f64,
    ) {
    }

    /// Records events emitted by the share-download rollback queue.
    fn record_share_download_rollback_event(&self, event: &'static str, count: u64) {}

    /// Sets the number of pending share-download rollback operations.
    fn set_share_download_rollback_pending(&self, pending: u64) {}

    /// Builds the optional Forge system-metrics updater task.
    ///
    /// The returned future owns its cancellation token and is intended to be
    /// registered with the runtime component lifecycle.
    fn system_metrics_updater_task(
        &self,
        shutdown_token: CancellationToken,
    ) -> Option<Pin<Box<dyn Future<Output = ()> + Send + 'static>>> {
        None
    }
}

/// Shared trait object used by AsterDrive runtime and business components.
pub type SharedMetricsRecorder = Arc<dyn MetricsRecorder>;

/// No-op recorder used by tests and builds without metric collection.
pub struct NoopMetrics;

impl MetricsRecorder for NoopMetrics {}

impl NoopMetrics {
    /// Creates a no-op metrics recorder.
    pub fn new() -> Self {
        Self
    }

    /// Creates a shared no-op metrics recorder.
    pub fn arc() -> SharedMetricsRecorder {
        Arc::new(Self::new())
    }
}

impl aster_forge_metrics::DbMetricsRecorder for NoopMetrics {
    fn enabled(&self) -> bool {
        false
    }

    fn record_db_query(&self, _metric: &DbQueryMetric) {}
}

impl aster_forge_metrics::MetricsRecorder for NoopMetrics {}

impl Default for NoopMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "metrics")]
mod product {
    use std::sync::OnceLock;

    use aster_forge_metrics::prometheus::{ProductMetricError, ProductMetricResult};

    aster_forge_metrics::product_metrics! {
        pub struct DriveProductMetrics {
            file_uploads: counter(
                "file",
                "uploads_total",
                "Total Drive file upload attempts.",
                &["mode", "status"],
            ),
            file_downloads: counter(
                "file",
                "downloads_total",
                "Total Drive file download attempts.",
                &["source", "outcome", "range"],
            ),
            upload_sessions: counter(
                "upload",
                "sessions_total",
                "Total Drive upload sessions created.",
                &["mode"],
            ),
            upload_session_events: counter(
                "upload",
                "session_events_total",
                "Total Drive upload session lifecycle events.",
                &["mode", "event", "status"],
            ),
            storage_driver_operations: counter(
                "storage_driver",
                "operations_total",
                "Total Drive storage driver operations.",
                &["driver", "operation", "status", "kind"],
            ),
            storage_driver_operation_duration: histogram_with_buckets(
                "storage_driver",
                "operation_duration_seconds",
                "Drive storage driver operation duration in seconds.",
                &["driver", "operation", "status", "kind"],
                &[0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 5.0, 15.0, 60.0],
            ),
            share_download_rollback_events: counter(
                "share_download_rollback",
                "events_total",
                "Total shared download rollback queue events.",
                &["event"],
            ),
            share_download_rollback_pending: gauge(
                "share_download_rollback",
                "pending",
                "Pending shared download rollback operations.",
                &[],
            ),
        }
    }

    static PRODUCT_METRICS: OnceLock<ProductMetricResult<DriveProductMetrics>> = OnceLock::new();
    static PRODUCT_METRICS_WARNED: OnceLock<()> = OnceLock::new();

    pub fn get() -> Option<&'static DriveProductMetrics> {
        let result = PRODUCT_METRICS.get_or_init(DriveProductMetrics::register);
        match result {
            Ok(metrics) => Some(metrics),
            Err(error) => {
                warn_once(error);
                None
            }
        }
    }

    fn warn_once(error: &ProductMetricError) {
        PRODUCT_METRICS_WARNED.get_or_init(|| {
            tracing::warn!(
                error = %error,
                "failed to register Drive product metrics; product metrics will be skipped"
            );
        });
    }
}

/// Creates the runtime metrics recorder for the active crate features.
///
/// With the `metrics` feature enabled, this initializes the configured Forge
/// recorder and attaches Drive product metrics. Otherwise it returns
/// [`NoopMetrics`].
pub fn create_metrics_recorder() -> SharedMetricsRecorder {
    #[cfg(feature = "metrics")]
    {
        let forge = aster_forge_metrics::init_configured_or_noop();
        if forge.enabled() {
            return Arc::new(DriveMetricsRecorder::new(forge));
        }
    }

    NoopMetrics::arc()
}

/// Records one Forge runtime health report when the metrics backend is enabled.
pub fn record_health_report(scope: HealthCheckScope, report: &SystemHealthReport) {
    #[cfg(feature = "metrics")]
    report.record_metrics(
        scope.as_str(),
        &aster_forge_metrics::prometheus::PrometheusMetricsRecorder,
    );

    #[cfg(not(feature = "metrics"))]
    let _ = (scope, report);
}

#[cfg(feature = "metrics")]
struct DriveMetricsRecorder {
    forge: SharedForgeMetricsRecorder,
    product: Option<&'static product::DriveProductMetrics>,
}

#[cfg(feature = "metrics")]
impl DriveMetricsRecorder {
    fn new(forge: SharedForgeMetricsRecorder) -> Self {
        let product = product::get();
        Self { forge, product }
    }
}

#[cfg(feature = "metrics")]
impl aster_forge_metrics::DbMetricsRecorder for DriveMetricsRecorder {
    fn enabled(&self) -> bool {
        self.forge.enabled()
    }

    fn record_db_query(&self, metric: &DbQueryMetric) {
        self.forge.record_db_query(metric);
    }
}

#[cfg(feature = "metrics")]
impl ForgeMetricsRecorder for DriveMetricsRecorder {
    fn record_http_request(&self, method: &str, route: &str, status: u16, duration_seconds: f64) {
        self.forge
            .record_http_request(method, route, status, duration_seconds);
    }

    fn record_auth_event(&self, action: &'static str, status: &'static str, reason: &'static str) {
        self.forge.record_auth_event(action, status, reason);
    }

    fn record_application_event(
        &self,
        category: &'static str,
        event: &'static str,
        status: &'static str,
    ) {
        self.forge.record_application_event(category, event, status);
    }

    fn record_config_reload(
        &self,
        source: &'static str,
        decision: &'static str,
        status: &'static str,
        changed_keys: u64,
        duration_seconds: f64,
    ) {
        self.forge
            .record_config_reload(source, decision, status, changed_keys, duration_seconds);
    }

    fn record_config_mutation(
        &self,
        source: &'static str,
        operation: &'static str,
        status: &'static str,
        changed_keys: u64,
    ) {
        self.forge
            .record_config_mutation(source, operation, status, changed_keys);
    }

    fn record_background_task_transition(&self, kind: &'static str, status: &'static str) {
        self.forge.record_background_task_transition(kind, status);
    }

    fn set_background_tasks_pending(&self, pending: u64) {
        self.forge.set_background_tasks_pending(pending);
    }

    fn record_external_operation(
        &self,
        system: &'static str,
        operation: &'static str,
        status: &'static str,
        duration_seconds: f64,
    ) {
        self.forge
            .record_external_operation(system, operation, status, duration_seconds);
    }

    fn system_metrics_updater_task(
        &self,
        shutdown_token: CancellationToken,
    ) -> Option<Pin<Box<dyn Future<Output = ()> + Send + 'static>>> {
        self.forge.system_metrics_updater_task(shutdown_token)
    }
}

#[cfg(feature = "metrics")]
impl MetricsRecorder for DriveMetricsRecorder {
    fn enabled(&self) -> bool {
        self.forge.enabled()
    }

    fn forge_recorder(&self) -> SharedForgeMetricsRecorder {
        self.forge.clone()
    }

    fn record_http_request(&self, method: &str, route: &str, status: u16, duration_seconds: f64) {
        self.forge
            .record_http_request(method, route, status, duration_seconds);
    }

    fn record_db_query(&self, metric: &DbQueryMetric) {
        self.forge.record_db_query(metric);
    }

    fn record_auth_event(&self, action: &'static str, status: &'static str, reason: &'static str) {
        self.forge.record_auth_event(action, status, reason);
    }

    fn record_config_reload(
        &self,
        source: &'static str,
        decision: &'static str,
        status: &'static str,
        changed_keys: u64,
        duration_seconds: f64,
    ) {
        self.forge
            .record_config_reload(source, decision, status, changed_keys, duration_seconds);
    }

    fn record_config_mutation(
        &self,
        source: &'static str,
        operation: &'static str,
        status: &'static str,
        changed_keys: u64,
    ) {
        self.forge
            .record_config_mutation(source, operation, status, changed_keys);
    }

    fn record_file_upload(&self, mode: &'static str, status: &'static str) {
        if let Some(product) = self.product {
            product.file_uploads.inc(&[mode, status], 1);
        }
    }

    fn record_file_download(&self, source: &'static str, outcome: &'static str, has_range: bool) {
        if let Some(product) = self.product {
            let range = if has_range { "range" } else { "full" };
            product.file_downloads.inc(&[source, outcome, range], 1);
        }
    }

    fn record_upload_session(&self, mode: &'static str) {
        if let Some(product) = self.product {
            product.upload_sessions.inc(&[mode], 1);
        }
    }

    fn record_upload_session_event(
        &self,
        mode: &'static str,
        event: &'static str,
        status: &'static str,
    ) {
        if let Some(product) = self.product {
            product.upload_session_events.inc(&[mode, event, status], 1);
        }
    }

    fn record_background_task_transition(&self, kind: &'static str, status: &'static str) {
        self.forge.record_background_task_transition(kind, status);
    }

    fn set_background_tasks_pending(&self, pending: u64) {
        self.forge.set_background_tasks_pending(pending);
    }

    fn record_storage_driver_operation(
        &self,
        driver: &'static str,
        operation: &'static str,
        status: &'static str,
        kind: &'static str,
        duration_seconds: f64,
    ) {
        if let Some(product) = self.product {
            let labels = [driver, operation, status, kind];
            product.storage_driver_operations.inc(&labels, 1);
            product
                .storage_driver_operation_duration
                .observe(&labels, duration_seconds);
        }
    }

    fn record_share_download_rollback_event(&self, event: &'static str, count: u64) {
        if let Some(product) = self.product {
            product.share_download_rollback_events.inc(&[event], count);
        }
    }

    fn set_share_download_rollback_pending(&self, pending: u64) {
        if let Some(product) = self.product {
            product
                .share_download_rollback_pending
                .set(&[], pending as f64);
        }
    }

    fn system_metrics_updater_task(
        &self,
        shutdown_token: CancellationToken,
    ) -> Option<Pin<Box<dyn Future<Output = ()> + Send + 'static>>> {
        self.forge.system_metrics_updater_task(shutdown_token)
    }
}

#[cfg(test)]
mod tests {
    use tokio_util::sync::CancellationToken;

    use super::{MetricsRecorder, NoopMetrics};

    #[test]
    fn noop_recorder_is_disabled_for_drive_and_forge_consumers() {
        let recorder = NoopMetrics::arc();

        assert!(!recorder.enabled());
        assert!(!recorder.forge_recorder().enabled());
    }

    #[test]
    fn noop_recorder_does_not_create_a_system_metrics_task() {
        let recorder = NoopMetrics;

        assert!(
            recorder
                .system_metrics_updater_task(CancellationToken::new())
                .is_none()
        );
    }
}

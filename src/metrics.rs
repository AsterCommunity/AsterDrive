//! Metrics facade backed by AsterForge.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

#[cfg(feature = "metrics")]
use aster_forge_metrics::MetricsRecorder as ForgeMetricsRecorder;
use aster_forge_metrics::{DbQueryMetric, SharedMetricsRecorder as SharedForgeMetricsRecorder};
use aster_forge_runtime::{HealthCheckScope, SystemHealthReport};
use tokio_util::sync::CancellationToken;

/// Drive 应用指标记录接口。
///
/// 所有方法默认 no-op，方便测试和非 metrics 构建复用同一条业务路径。
#[allow(unused_variables)]
pub trait MetricsRecorder: Send + Sync {
    /// 当前 recorder 是否会真实记录指标。
    ///
    /// 用于跳过会额外产生成本的采集逻辑，例如 DB callback 和 HTTP route label。
    fn enabled(&self) -> bool {
        false
    }

    /// 返回 Forge 公共指标 recorder，供 Actix middleware、DB callback 等基础设施复用。
    fn forge_recorder(&self) -> SharedForgeMetricsRecorder {
        aster_forge_metrics::NoopMetrics::arc()
    }

    fn record_http_request(&self, method: &str, route: &str, status: u16, duration_seconds: f64) {}

    fn record_db_query(&self, metric: &DbQueryMetric) {}

    fn record_auth_event(&self, action: &'static str, status: &'static str, reason: &'static str) {}

    fn record_file_upload(&self, mode: &'static str, status: &'static str) {}

    fn record_file_download(&self, source: &'static str, outcome: &'static str, has_range: bool) {}

    fn record_upload_session(&self, mode: &'static str) {}

    fn record_upload_session_event(
        &self,
        mode: &'static str,
        event: &'static str,
        status: &'static str,
    ) {
    }

    fn record_background_task_transition(&self, kind: &'static str, status: &'static str) {}

    fn record_config_reload(
        &self,
        source: &'static str,
        decision: &'static str,
        status: &'static str,
        changed_keys: u64,
        duration_seconds: f64,
    ) {
    }

    fn record_config_mutation(
        &self,
        source: &'static str,
        operation: &'static str,
        status: &'static str,
        changed_keys: u64,
    ) {
    }

    fn set_background_tasks_pending(&self, pending: u64) {}

    fn record_storage_driver_operation(
        &self,
        driver: &'static str,
        operation: &'static str,
        status: &'static str,
        kind: &'static str,
        duration_seconds: f64,
    ) {
    }

    fn record_share_download_rollback_event(&self, event: &'static str, count: u64) {}

    fn set_share_download_rollback_pending(&self, pending: u64) {}

    fn system_metrics_updater_task(
        &self,
        shutdown_token: CancellationToken,
    ) -> Option<Pin<Box<dyn Future<Output = ()> + Send + 'static>>> {
        None
    }
}

pub type SharedMetricsRecorder = Arc<dyn MetricsRecorder>;

/// 非 metrics 构建和测试使用的空实现。
pub struct NoopMetrics;

impl MetricsRecorder for NoopMetrics {}

impl NoopMetrics {
    pub fn new() -> Self {
        Self
    }

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

/// Creates the runtime metrics recorder for this build.
pub fn create_metrics_recorder() -> SharedMetricsRecorder {
    #[cfg(feature = "metrics")]
    {
        let forge = aster_forge_metrics::init_configured_or_noop();
        if forge.enabled() {
            return Arc::new(DriveMetricsRecorder::new(forge));
        }
    }

    crate::metrics::NoopMetrics::arc()
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

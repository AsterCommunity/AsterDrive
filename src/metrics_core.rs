//! 指标记录核心接口。
//!
//! 这个模块始终编译，不依赖 Prometheus。业务代码只依赖 `MetricsRecorder`，
//! `metrics` feature 关闭时注入 `NoopMetrics`，真实 Prometheus 实现由
//! `crate::metrics` 在 feature 边界内提供。

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

/// 应用指标记录接口。
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

    fn record_http_request(&self, method: &str, route: &str, status: u16, duration_seconds: f64) {}

    fn record_db_query(&self, info: &sea_orm::metric::Info<'_>) {}

    fn record_auth_event(&self, action: &'static str, status: &'static str, reason: &'static str) {}

    fn record_file_upload(&self, mode: &'static str, status: &'static str) {}

    fn record_file_download(&self, source: &'static str, outcome: &str, has_range: bool) {}

    fn record_upload_session(&self, mode: &'static str) {}

    fn record_upload_session_event(
        &self,
        mode: &'static str,
        event: &'static str,
        status: &'static str,
    ) {
    }

    fn record_background_task_transition(&self, kind: &'static str, status: &'static str) {}

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

impl Default for NoopMetrics {
    fn default() -> Self {
        Self::new()
    }
}

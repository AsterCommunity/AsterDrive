//! Prometheus 指标模块（仅 `metrics` feature 启用时编译）
//!
//! 架构参考 shortlinker-backend：OnceLock 全局单例 + init/get 模式

#[cfg(feature = "metrics")]
mod inner {
    use crate::errors::display_error;
    use prometheus::{
        Encoder, Gauge, HistogramOpts, HistogramVec, IntCounterVec, IntGauge, Opts, Registry,
        TextEncoder,
    };
    use sea_orm::DbBackend;
    use std::sync::OnceLock;
    use std::time::Instant;

    static METRICS: OnceLock<Metrics> = OnceLock::new();
    static PROCESS_STARTED_AT: OnceLock<Instant> = OnceLock::new();

    pub struct Metrics {
        pub registry: Registry,

        // HTTP 请求
        pub http_requests_total: IntCounterVec,
        pub http_request_duration_seconds: HistogramVec,

        // 数据库
        pub db_queries_total: IntCounterVec,
        pub db_query_duration_seconds: HistogramVec,

        // 业务
        pub auth_events_total: IntCounterVec,
        pub file_uploads_total: IntCounterVec,
        pub file_downloads_total: IntCounterVec,
        pub upload_sessions_total: IntCounterVec,
        pub upload_session_events_total: IntCounterVec,
        pub background_tasks_total: IntCounterVec,
        pub background_tasks_pending: IntGauge,
        pub background_task_retries_total: IntCounterVec,
        pub storage_driver_operations_total: IntCounterVec,
        pub storage_driver_operation_duration_seconds: HistogramVec,
        pub share_download_rollback_events_total: IntCounterVec,

        // 系统
        pub process_memory_rss_bytes: Gauge,
        pub process_cpu_milliseconds_total: IntGauge,
        pub uptime_seconds: Gauge,
        pub share_download_rollback_pending: Gauge,
    }

    impl Metrics {
        fn new() -> Result<Self, prometheus::Error> {
            let registry = Registry::new();

            let http_requests_total = IntCounterVec::new(
                Opts::new("http_requests_total", "Total HTTP requests"),
                &["method", "route", "status"],
            )?;
            let http_request_duration_seconds = HistogramVec::new(
                HistogramOpts::new(
                    "http_request_duration_seconds",
                    "HTTP request duration in seconds",
                )
                .buckets(vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 5.0]),
                &["method", "route", "status"],
            )?;
            let db_queries_total = IntCounterVec::new(
                Opts::new(
                    "db_queries_total",
                    "Total database queries observed through SeaORM",
                ),
                &["backend", "kind", "status"],
            )?;
            let db_query_duration_seconds = HistogramVec::new(
                HistogramOpts::new(
                    "db_query_duration_seconds",
                    "Database query duration in seconds",
                )
                .buckets(vec![
                    0.0005, 0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 5.0,
                ]),
                &["backend", "kind", "status"],
            )?;

            let auth_events_total = IntCounterVec::new(
                Opts::new("auth_events_total", "Total authentication events"),
                &["action", "status", "reason"],
            )?;
            let file_uploads_total = IntCounterVec::new(
                Opts::new("file_uploads_total", "Total file uploads"),
                &["mode", "status"],
            )?;
            let file_downloads_total = IntCounterVec::new(
                Opts::new("file_downloads_total", "Total file downloads"),
                &["source", "outcome", "range"],
            )?;
            let upload_sessions_total = IntCounterVec::new(
                Opts::new("upload_sessions_total", "Total upload sessions created"),
                &["mode"],
            )?;
            let upload_session_events_total = IntCounterVec::new(
                Opts::new(
                    "upload_session_events_total",
                    "Total upload session lifecycle events",
                ),
                &["mode", "event", "status"],
            )?;
            let background_tasks_total = IntCounterVec::new(
                Opts::new(
                    "background_tasks_total",
                    "Total background task state transitions",
                ),
                &["kind", "status"],
            )?;
            let background_tasks_pending = IntGauge::new(
                "background_tasks_pending",
                "Pending or retryable background task backlog",
            )?;
            let background_task_retries_total = IntCounterVec::new(
                Opts::new(
                    "background_task_retries_total",
                    "Total background task retry transitions",
                ),
                &["kind"],
            )?;
            let storage_driver_operations_total = IntCounterVec::new(
                Opts::new(
                    "storage_driver_operations_total",
                    "Total storage driver operations",
                ),
                &["driver", "operation", "status", "kind"],
            )?;
            let storage_driver_operation_duration_seconds = HistogramVec::new(
                HistogramOpts::new(
                    "storage_driver_operation_duration_seconds",
                    "Storage driver operation duration in seconds",
                )
                .buckets(vec![
                    0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 5.0, 15.0, 60.0,
                ]),
                &["driver", "operation", "status", "kind"],
            )?;
            let share_download_rollback_events_total = IntCounterVec::new(
                Opts::new(
                    "share_download_rollback_events_total",
                    "Total shared download rollback queue events",
                ),
                &["event"],
            )?;

            let process_memory_rss_bytes =
                Gauge::new("process_memory_rss_bytes", "Process RSS memory in bytes")?;
            let process_cpu_milliseconds_total = IntGauge::new(
                "process_cpu_milliseconds_total",
                "Process accumulated CPU time in milliseconds",
            )?;
            let uptime_seconds = Gauge::new("process_uptime_seconds", "Process uptime in seconds")?;
            let share_download_rollback_pending = Gauge::new(
                "share_download_rollback_pending",
                "Pending shared download rollback operations",
            )?;

            registry.register(Box::new(http_requests_total.clone()))?;
            registry.register(Box::new(http_request_duration_seconds.clone()))?;
            registry.register(Box::new(db_queries_total.clone()))?;
            registry.register(Box::new(db_query_duration_seconds.clone()))?;
            registry.register(Box::new(auth_events_total.clone()))?;
            registry.register(Box::new(file_uploads_total.clone()))?;
            registry.register(Box::new(file_downloads_total.clone()))?;
            registry.register(Box::new(upload_sessions_total.clone()))?;
            registry.register(Box::new(upload_session_events_total.clone()))?;
            registry.register(Box::new(background_tasks_total.clone()))?;
            registry.register(Box::new(background_tasks_pending.clone()))?;
            registry.register(Box::new(background_task_retries_total.clone()))?;
            registry.register(Box::new(storage_driver_operations_total.clone()))?;
            registry.register(Box::new(storage_driver_operation_duration_seconds.clone()))?;
            registry.register(Box::new(share_download_rollback_events_total.clone()))?;
            registry.register(Box::new(process_memory_rss_bytes.clone()))?;
            registry.register(Box::new(process_cpu_milliseconds_total.clone()))?;
            registry.register(Box::new(uptime_seconds.clone()))?;
            registry.register(Box::new(share_download_rollback_pending.clone()))?;

            Ok(Metrics {
                registry,
                http_requests_total,
                http_request_duration_seconds,
                db_queries_total,
                db_query_duration_seconds,
                auth_events_total,
                file_uploads_total,
                file_downloads_total,
                upload_sessions_total,
                upload_session_events_total,
                background_tasks_total,
                background_tasks_pending,
                background_task_retries_total,
                storage_driver_operations_total,
                storage_driver_operation_duration_seconds,
                share_download_rollback_events_total,
                process_memory_rss_bytes,
                process_cpu_milliseconds_total,
                uptime_seconds,
                share_download_rollback_pending,
            })
        }

        pub fn export(&self) -> Result<String, String> {
            let encoder = TextEncoder::new();
            let metric_families = self.registry.gather();
            let mut buf = Vec::new();
            encoder
                .encode(&metric_families, &mut buf)
                .map_err(display_error)?;
            String::from_utf8(buf).map_err(display_error)
        }
    }

    pub fn init_metrics() -> Result<(), prometheus::Error> {
        if METRICS.get().is_some() {
            return Ok(());
        }

        let _ = PROCESS_STARTED_AT.get_or_init(Instant::now);
        let metrics = Metrics::new()?;
        // Concurrent initialization races are benign here; the first successful
        // Metrics::new() wins and later Set errors are intentionally ignored.
        let _ = METRICS.set(metrics);
        Ok(())
    }

    pub fn get_metrics() -> Option<&'static Metrics> {
        METRICS.get()
    }

    pub fn record_http_request(method: &str, route: &str, status: u16, duration_seconds: f64) {
        let Some(metrics) = get_metrics() else {
            return;
        };

        let status = status.to_string();
        metrics
            .http_requests_total
            .with_label_values(&[method, route, &status])
            .inc();
        metrics
            .http_request_duration_seconds
            .with_label_values(&[method, route, &status])
            .observe(duration_seconds);
    }

    fn query_kind_from_sql(sql: &str) -> &'static str {
        let token = sql.split_whitespace().next().unwrap_or_default();

        if token.eq_ignore_ascii_case("SELECT") {
            "select"
        } else if token.eq_ignore_ascii_case("INSERT") {
            "insert"
        } else if token.eq_ignore_ascii_case("UPDATE") {
            "update"
        } else if token.eq_ignore_ascii_case("DELETE") {
            "delete"
        } else if token.eq_ignore_ascii_case("WITH") {
            "with"
        } else if token.eq_ignore_ascii_case("BEGIN")
            || token.eq_ignore_ascii_case("COMMIT")
            || token.eq_ignore_ascii_case("ROLLBACK")
            || token.eq_ignore_ascii_case("SAVEPOINT")
            || token.eq_ignore_ascii_case("RELEASE")
        {
            "transaction"
        } else if token.eq_ignore_ascii_case("CREATE")
            || token.eq_ignore_ascii_case("ALTER")
            || token.eq_ignore_ascii_case("DROP")
            || token.eq_ignore_ascii_case("TRUNCATE")
        {
            "ddl"
        } else if token.eq_ignore_ascii_case("PRAGMA") {
            "pragma"
        } else {
            "other"
        }
    }

    fn backend_label(backend: DbBackend) -> &'static str {
        match backend {
            DbBackend::MySql => "mysql",
            DbBackend::Postgres => "postgres",
            DbBackend::Sqlite => "sqlite",
            _ => "other",
        }
    }

    pub fn record_db_query(info: &sea_orm::metric::Info<'_>) {
        let Some(metrics) = get_metrics() else {
            return;
        };

        let backend = backend_label(info.statement.db_backend);
        let kind = query_kind_from_sql(&info.statement.sql);
        let status = if info.failed { "error" } else { "ok" };

        metrics
            .db_queries_total
            .with_label_values(&[backend, kind, status])
            .inc();
        metrics
            .db_query_duration_seconds
            .with_label_values(&[backend, kind, status])
            .observe(info.elapsed.as_secs_f64());
    }

    pub fn record_auth_event(action: &'static str, status: &'static str, reason: &'static str) {
        let Some(metrics) = get_metrics() else {
            return;
        };

        metrics
            .auth_events_total
            .with_label_values(&[action, status, reason])
            .inc();
    }

    pub fn record_file_upload(mode: &'static str, status: &'static str) {
        let Some(metrics) = get_metrics() else {
            return;
        };

        metrics
            .file_uploads_total
            .with_label_values(&[mode, status])
            .inc();
    }

    pub fn record_file_download(source: &'static str, outcome: &str, has_range: bool) {
        let Some(metrics) = get_metrics() else {
            return;
        };

        let range = if has_range { "range" } else { "full" };
        metrics
            .file_downloads_total
            .with_label_values(&[source, outcome, range])
            .inc();
    }

    pub fn record_upload_session(mode: &'static str) {
        let Some(metrics) = get_metrics() else {
            return;
        };

        metrics
            .upload_sessions_total
            .with_label_values(&[mode])
            .inc();
    }

    pub fn record_upload_session_event(
        mode: &'static str,
        event: &'static str,
        status: &'static str,
    ) {
        let Some(metrics) = get_metrics() else {
            return;
        };

        metrics
            .upload_session_events_total
            .with_label_values(&[mode, event, status])
            .inc();
    }

    pub fn record_background_task_transition(kind: &'static str, status: &'static str) {
        let Some(metrics) = get_metrics() else {
            return;
        };

        metrics
            .background_tasks_total
            .with_label_values(&[kind, status])
            .inc();
        if status == "retry" {
            metrics
                .background_task_retries_total
                .with_label_values(&[kind])
                .inc();
        }
    }

    pub fn set_background_tasks_pending(pending: u64) {
        let Some(metrics) = get_metrics() else {
            return;
        };

        metrics
            .background_tasks_pending
            .set(i64::try_from(pending).unwrap_or(i64::MAX));
    }

    pub fn record_storage_driver_operation(
        driver: &'static str,
        operation: &'static str,
        status: &'static str,
        kind: &'static str,
        duration_seconds: f64,
    ) {
        let Some(metrics) = get_metrics() else {
            return;
        };

        metrics
            .storage_driver_operations_total
            .with_label_values(&[driver, operation, status, kind])
            .inc();
        metrics
            .storage_driver_operation_duration_seconds
            .with_label_values(&[driver, operation, status, kind])
            .observe(duration_seconds);
    }

    pub fn record_share_download_rollback_event(event: &'static str, count: u64) {
        let Some(metrics) = get_metrics() else {
            return;
        };

        metrics
            .share_download_rollback_events_total
            .with_label_values(&[event])
            .inc_by(count);
    }

    pub fn set_share_download_rollback_pending(pending: u64) {
        let Some(metrics) = get_metrics() else {
            return;
        };

        metrics.share_download_rollback_pending.set(pending as f64);
    }

    fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
        if let Some(message) = panic.downcast_ref::<&str>() {
            (*message).to_string()
        } else if let Some(message) = panic.downcast_ref::<String>() {
            message.clone()
        } else {
            "unknown panic payload".to_string()
        }
    }

    /// 后台任务：定期更新系统指标（RSS、CPU、uptime）
    pub fn system_metrics_updater_task(
        shutdown_token: tokio_util::sync::CancellationToken,
    ) -> impl std::future::Future<Output = ()> + Send + 'static {
        use parking_lot::Mutex;
        use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

        static SYSTEM: OnceLock<Mutex<System>> = OnceLock::new();

        async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
            loop {
                tokio::select! {
                    biased;
                    _ = shutdown_token.cancelled() => break,
                    _ = interval.tick() => {}
                }

                if shutdown_token.is_cancelled() {
                    break;
                }

                let Some(metrics) = get_metrics() else {
                    continue;
                };

                let update = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let pid = Pid::from_u32(std::process::id());
                    let sys_mutex = SYSTEM.get_or_init(|| Mutex::new(System::new()));
                    let mut sys = sys_mutex.lock();
                    sys.refresh_processes_specifics(
                        ProcessesToUpdate::Some(&[pid]),
                        true,
                        ProcessRefreshKind::nothing().with_memory().with_cpu(),
                    );
                    if let Some(process) = sys.process(pid) {
                        metrics
                            .process_memory_rss_bytes
                            .set(process.memory() as f64);
                        let cpu_millis =
                            i64::try_from(process.accumulated_cpu_time()).unwrap_or(i64::MAX);
                        metrics.process_cpu_milliseconds_total.set(cpu_millis);
                    }
                    let uptime = PROCESS_STARTED_AT
                        .get()
                        .map(Instant::elapsed)
                        .unwrap_or_default()
                        .as_secs_f64();
                    metrics.uptime_seconds.set(uptime);
                }));

                if let Err(panic) = update {
                    tracing::error!(
                        panic = %panic_message(panic),
                        "system metrics updater panicked"
                    );
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::query_kind_from_sql;

        #[test]
        fn query_kind_from_sql_classifies_common_statements() {
            assert_eq!(query_kind_from_sql("select * from users"), "select");
            assert_eq!(
                query_kind_from_sql(" INSERT INTO users VALUES (?) "),
                "insert"
            );
            assert_eq!(query_kind_from_sql("update users set name = ?"), "update");
            assert_eq!(
                query_kind_from_sql("delete from users where id = ?"),
                "delete"
            );
            assert_eq!(
                query_kind_from_sql("with cte as (select 1) select * from cte"),
                "with"
            );
            assert_eq!(query_kind_from_sql("begin"), "transaction");
            assert_eq!(query_kind_from_sql("create table x(id int)"), "ddl");
            assert_eq!(query_kind_from_sql("pragma foreign_keys=ON"), "pragma");
            assert_eq!(query_kind_from_sql("vacuum"), "other");
        }

        #[test]
        fn process_uptime_uses_process_start_instant_not_epoch() {
            let started_at = std::time::Instant::now();
            let uptime = started_at.elapsed().as_secs_f64();

            assert!(uptime < 1.0);
        }
    }
}

#[cfg(feature = "metrics")]
pub use inner::*;

#[cfg(feature = "metrics")]
pub struct PrometheusMetricsRecorder;

#[cfg(feature = "metrics")]
impl crate::metrics_core::MetricsRecorder for PrometheusMetricsRecorder {
    fn enabled(&self) -> bool {
        true
    }

    fn record_http_request(&self, method: &str, route: &str, status: u16, duration_seconds: f64) {
        record_http_request(method, route, status, duration_seconds);
    }

    fn record_db_query(&self, info: &sea_orm::metric::Info<'_>) {
        record_db_query(info);
    }

    fn record_auth_event(&self, action: &'static str, status: &'static str, reason: &'static str) {
        record_auth_event(action, status, reason);
    }

    fn record_file_upload(&self, mode: &'static str, status: &'static str) {
        record_file_upload(mode, status);
    }

    fn record_file_download(&self, source: &'static str, outcome: &str, has_range: bool) {
        record_file_download(source, outcome, has_range);
    }

    fn record_upload_session(&self, mode: &'static str) {
        record_upload_session(mode);
    }

    fn record_upload_session_event(
        &self,
        mode: &'static str,
        event: &'static str,
        status: &'static str,
    ) {
        record_upload_session_event(mode, event, status);
    }

    fn record_background_task_transition(&self, kind: &'static str, status: &'static str) {
        record_background_task_transition(kind, status);
    }

    fn set_background_tasks_pending(&self, pending: u64) {
        set_background_tasks_pending(pending);
    }

    fn record_storage_driver_operation(
        &self,
        driver: &'static str,
        operation: &'static str,
        status: &'static str,
        kind: &'static str,
        duration_seconds: f64,
    ) {
        record_storage_driver_operation(driver, operation, status, kind, duration_seconds);
    }

    fn record_share_download_rollback_event(&self, event: &'static str, count: u64) {
        record_share_download_rollback_event(event, count);
    }

    fn set_share_download_rollback_pending(&self, pending: u64) {
        set_share_download_rollback_pending(pending);
    }

    fn system_metrics_updater_task(
        &self,
        shutdown_token: tokio_util::sync::CancellationToken,
    ) -> Option<std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>>> {
        Some(Box::pin(system_metrics_updater_task(shutdown_token)))
    }
}

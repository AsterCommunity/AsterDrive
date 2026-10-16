//! AsterDrive product metrics contracts and AsterForge adapters.
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

mod lifecycle;
pub use lifecycle::*;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use aster_forge_metrics::SharedMetricsRecorder as SharedForgeMetricsRecorder;
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

    fn admission_rejection(&self, reason: AdmissionReason) {}
    fn transfer_bytes(&self, labels: TransferLabels, bytes: u64) {}
    fn transfer_active(&self, labels: TransferLabels, delta: i64) {}
    fn transfer_finished(&self, labels: TransferLabels, outcome: TransferOutcome, seconds: f64) {}
    fn upload_phase(
        &self,
        mode: &'static str,
        phase: UploadPhase,
        outcome: TransferOutcome,
        seconds: f64,
    ) {
    }
    fn upload_backlog(
        &self,
        mode: &'static str,
        state: &'static str,
        count: u64,
        oldest_seconds: f64,
    ) {
    }
    fn blob_cleanup_backlog(&self, count: u64, oldest_seconds: f64) {}
    fn blob_cleanup(&self, outcome: CleanupOutcome, retired_bytes: u64) {}
    fn snapshot_status(&self, source: &'static str, valid: bool, last_success: f64) {}
    fn capacity_snapshot(
        &self,
        scope: &'static str,
        available: Option<u64>,
        total: Option<u64>,
        used: Option<u64>,
    ) {
    }
    fn staging_snapshot(&self, kind: &'static str, bytes: u64) {}
    fn remote_nodes(&self, status: &'static str, count: u64) {}
    fn remote_connections(&self, delta: i64) {}
    fn remote_reconnect(&self, reason: RemoteReason) {}
    fn remote_request(
        &self,
        transport: RemoteTransport,
        operation: RemoteOperation,
        outcome: TransferOutcome,
        reason: RemoteReason,
        seconds: f64,
    ) {
    }

    /// Returns the Forge recorder used by shared middleware and database hooks.
    fn forge_recorder(&self) -> SharedForgeMetricsRecorder {
        aster_forge_metrics::NoopMetrics::arc()
    }

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

    /// Records a storage placement routing decision using bounded labels.
    fn record_storage_routing(&self, selection: &'static str, outcome: &'static str) {}

    /// Records a placement decision with bounded topology identifiers.
    fn record_storage_routing_detail(
        &self,
        profile_id: &str,
        rule_id: &str,
        policy_id: &str,
        selection: &str,
        outcome: &str,
    ) {
    }

    /// Records the bounded outcome of one upload target capacity assessment.
    fn record_upload_capacity_admission(&self, outcome: &'static str) {}

    /// Records the bounded outcome of one physical upload-staging reservation.
    fn record_upload_staging_capacity_admission(&self, outcome: &'static str) {}

    /// Records how upload admission obtained a capacity observation.
    fn record_storage_capacity_probe_cache(&self, outcome: &'static str) {}

    /// Records one provider capacity probe and its bounded outcome.
    fn record_storage_capacity_probe(&self, outcome: &'static str, duration_seconds: f64) {}

    /// Records the selected upload data plane after initialization succeeds or fails.
    fn record_upload_data_plane(&self, data_plane: &'static str, status: &'static str) {}

    /// Records a background-task status transition.
    fn record_background_task_transition(&self, kind: &'static str, status: &'static str) {}

    /// Records an avatar upload lifecycle outcome.
    fn record_avatar_upload(&self, status: &'static str) {}

    /// Records the compressed source bytes accepted by the avatar staging boundary.
    fn record_avatar_source_bytes(&self, bytes: u64) {}

    /// Records a validated avatar source dimension.
    fn record_avatar_dimension(&self, axis: &'static str, pixels: u32) {}

    /// Exposes a configured avatar resource boundary.
    fn set_avatar_budget_bytes(&self, budget: &'static str, bytes: u64) {}

    /// Records synchronous avatar render latency.
    fn record_avatar_render_duration(&self, processor: &'static str, duration_seconds: f64) {}

    /// Records filesystem publication latency for rendered avatar variants.
    fn record_avatar_publish_duration(&self, status: &'static str, duration_seconds: f64) {}

    /// Records time spent waiting for an avatar render concurrency permit.
    fn record_avatar_render_wait_duration(&self, duration_seconds: f64) {}

    /// Adjusts the number of avatar renders waiting for a concurrency permit.
    fn adjust_avatar_render_waiting(&self, delta: i64) {}

    /// Adjusts the number of avatar renders currently holding a concurrency permit.
    fn adjust_avatar_render_active(&self, delta: i64) {}

    /// Records an avatar rejection at a stable boundary.
    fn record_avatar_rejection(&self, reason: &'static str) {}

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

    /// Records a streaming upload attempt lifecycle event.
    fn record_stream_upload_attempt(&self, event: &'static str, status: &'static str) {}

    /// Records bytes associated with a streaming upload attempt.
    fn record_stream_upload_bytes(&self, kind: &'static str, bytes: u64) {}

    /// Adjusts the number of streaming attempts in the data plane.
    fn adjust_stream_upload_active(&self, delta: i64) {}

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

impl Default for NoopMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "metrics")]
mod product {

    aster_forge_metrics::product_metrics! {
        pub struct DriveProductMetrics {
            admission_rejections: counter("upload", "admission_rejections_total", "Upload requests rejected by a stable resource boundary.", &["reason"]).with_unit("1"),
            transfer_bytes: counter("file", "transfer_bytes_total", "Observed bytes consumed at AD data-plane boundaries.", &["direction", "source", "mode"]).with_unit("By"),
            transfer_active: gauge("file", "transfers_in_flight", "Active AD data-plane streams.", &["direction", "source", "mode"]).with_unit("1"),
            transfer_duration: histogram_with_buckets("file", "transfer_duration_seconds", "AD stream lifecycle duration through EOF error or drop.", &["direction", "source", "mode", "outcome"], &[0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 15.0, 60.0, 300.0, 900.0, 3600.0]).with_unit("s"),
            upload_phase: histogram_with_buckets("upload", "phase_duration_seconds", "Upload phase duration, excluding completed retries.", &["mode", "phase", "status"], &[0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 15.0, 60.0, 300.0]).with_unit("s"),
            upload_backlog: gauge("upload", "sessions_active", "Persisted upload session snapshot by mode and state.", &["mode", "state"]).with_unit("1"),
            upload_oldest: gauge("upload", "session_oldest_age_seconds", "Oldest persisted session age since creation.", &["mode", "state"]).with_unit("s"),
            cleanup_pending: gauge("blob", "cleanup_pending", "Unreferenced or claimed blobs pending physical cleanup.", &[]).with_unit("1"),
            cleanup_oldest: gauge("blob", "cleanup_oldest_age_seconds", "Oldest stored blob cleanup interval age; retained across retries.", &[]).with_unit("s"),
            cleanup_objects: counter("blob", "cleanup_objects_total", "Claimed blob cleanup outcomes.", &["outcome"]).with_unit("1"),
            cleanup_retired: counter("blob", "cleanup_retired_backing_bytes_total", "Blob backing bytes retired after deletion acknowledgment and metadata removal.", &[]).with_unit("By"),
            snapshot_valid: gauge("observability", "snapshot_valid", "Whether the last sampling attempt succeeded.", &["source"]).with_unit("1"),
            snapshot_updated: gauge("observability", "snapshot_last_success_timestamp_seconds", "Unix timestamp of the last successful sample.", &["source"]).with_unit("s"),
            capacity_known: gauge("storage", "capacity_known", "Whether this capacity dimension is supported and current.", &["scope", "kind"]).with_unit("1"),
            capacity: gauge("storage", "capacity_bytes", "Capacity snapshot; targets report independent minima and must not be summed.", &["scope", "kind"]).with_unit("By"),
            staging: gauge("upload", "staging_bytes", "Persisted staging and reservation snapshot bytes.", &["kind"]).with_unit("By"),
            remote_nodes: gauge("remote", "nodes", "Persisted node status snapshot.", &["status"]).with_unit("1"),
            remote_connections: gauge("remote", "tunnel_connections", "Locally established reverse tunnel connections.", &[]).with_unit("1"),
            remote_reconnects: counter("remote", "tunnel_reconnects_total", "Reverse tunnel connection retry decisions.", &["reason"]).with_unit("1"),
            remote_requests: counter("remote", "requests_total", "Remote transport requests observed by this node; separate from driver operations.", &["transport", "operation", "status", "reason"]).with_unit("1"),
            remote_duration: histogram_with_buckets("remote", "request_duration_seconds", "Remote transport request duration.", &["transport", "operation", "status"], &[0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 15.0, 60.0, 300.0, 3600.0]).with_unit("s"),
            file_uploads: counter(
                "file",
                "uploads_total",
                "Total Drive file upload attempts.",
                &["mode", "status"],
            ).with_unit("1"),
            file_downloads: counter(
                "file",
                "downloads_total",
                "Total Drive file download attempts.",
                &["source", "outcome", "range"],
            ).with_unit("1"),
            upload_sessions: counter(
                "upload",
                "sessions_total",
                "Total Drive upload sessions created.",
                &["mode"],
            ).with_unit("1"),
            upload_session_events: counter(
                "upload",
                "session_events_total",
                "Total Drive upload session lifecycle events.",
                &["mode", "event", "status"],
            ).with_unit("1"),
            storage_routing_decisions: counter(
                "storage",
                "routing_decisions_total",
                "Total storage placement routing decisions.",
                &["selection", "outcome"],
            ).with_unit("1"),
            storage_routing_details: counter(
                "storage",
                "routing_details_total",
                "Storage placement decisions by bounded topology identifiers.",
                &["profile", "rule", "policy", "selection", "outcome"],
            ).with_unit("1"),
            upload_capacity_admissions: counter(
                "upload",
                "capacity_admissions_total",
                "Upload target capacity assessment outcomes.",
                &["outcome"],
            ).with_unit("1"),
            upload_staging_capacity_admissions: counter(
                "upload",
                "staging_capacity_admissions_total",
                "Physical upload-staging reservation outcomes.",
                &["outcome"],
            ).with_unit("1"),
            storage_capacity_probe_cache: counter(
                "storage",
                "capacity_probe_cache_total",
                "Capacity observation cache decisions.",
                &["outcome"],
            ).with_unit("1"),
            storage_capacity_probe_duration: histogram_with_buckets(
                "storage",
                "capacity_probe_duration_seconds",
                "Storage capacity probe duration by bounded outcome.",
                &["outcome"],
                &[0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0],
            ).with_unit("s"),
            upload_data_planes: counter(
                "upload",
                "data_planes_total",
                "Upload initialization outcomes by selected data plane.",
                &["data_plane", "status"],
            ).with_unit("1"),
            storage_driver_operations: counter(
                "storage_driver",
                "operations_total",
                "Total Drive storage driver operations.",
                &["driver", "operation", "status", "kind"],
            ).with_unit("1"),
    storage_driver_operation_duration: histogram_with_buckets(
                "storage_driver",
                "operation_duration_seconds",
                "Drive storage driver operation duration in seconds.",
                &["driver", "operation", "status", "kind"],
                &[0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 5.0, 15.0, 60.0],
            ).with_unit("s"),
            stream_upload_attempts: counter(
                "stream_upload",
                "attempts_total",
                "Total streaming upload attempt lifecycle events.",
                &["event", "status"],
            ).with_unit("1"),
            stream_upload_bytes: histogram_with_buckets(
                "stream_upload",
                "bytes",
                "Streaming upload attempt byte counts.",
                &["kind"],
                &[1024.0, 64.0 * 1024.0, 1024.0 * 1024.0, 5.0 * 1024.0 * 1024.0, 50.0 * 1024.0 * 1024.0, 1024.0 * 1024.0 * 1024.0, 10.0 * 1024.0 * 1024.0 * 1024.0],
            ).with_unit("By"),
            stream_upload_active: gauge(
                "stream_upload",
                "active",
                "Streaming upload attempts currently in the data plane.",
                &[],
            ).with_unit("1"),
            avatar_uploads: counter(
                "avatar",
                "uploads_total",
                "Total synchronous avatar upload outcomes.",
                &["status"],
            ).with_unit("1"),
            avatar_source_bytes: histogram_with_buckets(
                "avatar",
                "source_bytes",
                "Compressed avatar source bytes accepted by staging.",
                &[],
                &[1024.0, 64.0 * 1024.0, 1024.0 * 1024.0, 4.0 * 1024.0 * 1024.0, 10.0 * 1024.0 * 1024.0, 16.0 * 1024.0 * 1024.0],
            ).with_unit("By"),
            avatar_dimensions: histogram_with_buckets(
                "avatar",
                "dimension_pixels",
                "Validated avatar source dimensions in pixels.",
                &["axis"],
                &[128.0, 256.0, 512.0, 1024.0],
            ).with_unit("{pixel}"),
            avatar_budget_bytes: gauge(
                "avatar",
                "budget_bytes",
                "Configured avatar source and decoder allocation boundaries in bytes.",
                &["budget"],
            ).with_unit("By"),
            avatar_render_duration: histogram_with_buckets(
                "avatar",
                "render_duration_seconds",
                "Synchronous avatar render duration in seconds.",
                &["processor"],
                &[0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 15.0],
            ).with_unit("s"),
            avatar_publish_duration: histogram_with_buckets(
                "avatar",
                "publish_duration_seconds",
                "Filesystem publication duration for rendered avatar variants.",
                &["status"],
                &[0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 15.0],
            ).with_unit("s"),
            avatar_render_wait_duration: histogram_with_buckets(
                "avatar",
                "render_wait_duration_seconds",
                "Time spent waiting for an avatar render concurrency permit.",
                &[],
                &[0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 15.0, 30.0, 60.0],
            ).with_unit("s"),
            avatar_render_waiting: gauge(
                "avatar",
                "render_waiting",
                "Avatar renders waiting for a concurrency permit.",
                &[],
            ).with_unit("1"),
            avatar_render_active: gauge(
                "avatar",
                "render_active",
                "Avatar renders currently holding a concurrency permit.",
                &[],
            ).with_unit("1"),
            avatar_rejections: counter(
                "avatar",
                "rejections_total",
                "Total avatar upload rejections by boundary.",
                &["reason"],
            ).with_unit("1"),
            share_download_rollback_events: counter(
                "share_download_rollback",
                "events_total",
                "Total shared download rollback queue events.",
                &["event"],
            ).with_unit("1"),
            share_download_rollback_pending: gauge(
                "share_download_rollback",
                "pending",
                "Pending shared download rollback operations.",
                &[],
            ).with_unit("1"),
        }
    }
}

/// Builds the product recorder against an explicit, instance-owned provider.
pub fn create_metrics_recorder(
    provider: aster_forge_metrics::MetricsProvider,
) -> aster_forge_metrics::MetricResult<SharedMetricsRecorder> {
    #[cfg(feature = "metrics")]
    if provider.enabled() {
        let forge = Arc::new(aster_forge_metrics::InfrastructureMetricsRecorder::new(
            provider.clone(),
        )?);
        let product = product::DriveProductMetrics::register(&provider)?;
        return Ok(Arc::new(DriveMetricsRecorder { forge, product }));
    }
    let _ = provider;
    Ok(NoopMetrics::arc())
}

/// Records one Forge runtime health report when the metrics backend is enabled.
pub fn record_health_report(
    metrics: &SharedMetricsRecorder,
    scope: HealthCheckScope,
    report: &SystemHealthReport,
) {
    #[cfg(feature = "metrics")]
    metrics
        .forge_recorder()
        .record_health(scope.as_str(), report);
    #[cfg(not(feature = "metrics"))]
    let _ = (metrics, scope, report);
}

#[cfg(feature = "metrics")]
struct DriveMetricsRecorder {
    forge: SharedForgeMetricsRecorder,
    product: product::DriveProductMetrics,
}

#[cfg(feature = "metrics")]
impl MetricsRecorder for DriveMetricsRecorder {
    fn admission_rejection(&self, reason: AdmissionReason) {
        {
            let product = &self.product;
            product.admission_rejections.inc(&[reason.label()], 1);
        }
    }

    fn transfer_bytes(&self, labels: TransferLabels, bytes: u64) {
        {
            let product = &self.product;
            product.transfer_bytes.inc(&labels.values(), bytes);
        }
    }
    fn transfer_active(&self, labels: TransferLabels, delta: i64) {
        {
            let product = &self.product;
            product.transfer_active.add(&labels.values(), delta as f64);
        }
    }
    fn transfer_finished(&self, labels: TransferLabels, outcome: TransferOutcome, seconds: f64) {
        {
            let product = &self.product;
            let [direction, source, mode] = labels.values();
            product
                .transfer_duration
                .observe(&[direction, source, mode, outcome.label()], seconds);
        }
    }
    fn upload_phase(
        &self,
        mode: &'static str,
        phase: UploadPhase,
        outcome: TransferOutcome,
        seconds: f64,
    ) {
        {
            let product = &self.product;
            product
                .upload_phase
                .observe(&[mode, phase.label(), outcome.label()], seconds);
        }
    }
    fn upload_backlog(
        &self,
        mode: &'static str,
        state: &'static str,
        count: u64,
        oldest_seconds: f64,
    ) {
        {
            let product = &self.product;
            product.upload_backlog.set(&[mode, state], count as f64);
            product.upload_oldest.set(&[mode, state], oldest_seconds);
        }
    }
    fn blob_cleanup_backlog(&self, count: u64, oldest_seconds: f64) {
        {
            let product = &self.product;
            product.cleanup_pending.set(&[], count as f64);
            product.cleanup_oldest.set(&[], oldest_seconds);
        }
    }
    fn blob_cleanup(&self, outcome: CleanupOutcome, retired_bytes: u64) {
        {
            let product = &self.product;
            product.cleanup_objects.inc(&[outcome.label()], 1);
            product.cleanup_retired.inc(&[], retired_bytes);
        }
    }
    fn snapshot_status(&self, source: &'static str, valid: bool, last_success: f64) {
        {
            let product = &self.product;
            product
                .snapshot_valid
                .set(&[source], if valid { 1.0 } else { 0.0 });
            if valid {
                product.snapshot_updated.set(&[source], last_success);
            }
        }
    }
    fn capacity_snapshot(
        &self,
        scope: &'static str,
        available: Option<u64>,
        total: Option<u64>,
        used: Option<u64>,
    ) {
        {
            let product = &self.product;
            for (kind, value) in [("available", available), ("total", total), ("used", used)] {
                product
                    .capacity_known
                    .set(&[scope, kind], if value.is_some() { 1.0 } else { 0.0 });
                if let Some(value) = value {
                    product.capacity.set(&[scope, kind], value as f64);
                }
            }
        }
    }
    fn staging_snapshot(&self, kind: &'static str, bytes: u64) {
        {
            let product = &self.product;
            product.staging.set(&[kind], bytes as f64);
        }
    }
    fn remote_nodes(&self, status: &'static str, count: u64) {
        {
            let product = &self.product;
            product.remote_nodes.set(&[status], count as f64);
        }
    }
    fn remote_connections(&self, delta: i64) {
        {
            let product = &self.product;
            product.remote_connections.add(&[], delta as f64);
        }
    }
    fn remote_reconnect(&self, reason: RemoteReason) {
        {
            let product = &self.product;
            product.remote_reconnects.inc(&[reason.label()], 1);
        }
    }
    fn remote_request(
        &self,
        transport: RemoteTransport,
        operation: RemoteOperation,
        outcome: TransferOutcome,
        reason: RemoteReason,
        seconds: f64,
    ) {
        {
            let product = &self.product;
            product.remote_requests.inc(
                &[
                    transport.label(),
                    operation.label(),
                    outcome.label(),
                    reason.label(),
                ],
                1,
            );
            product.remote_duration.observe(
                &[transport.label(), operation.label(), outcome.label()],
                seconds,
            );
        }
    }

    fn enabled(&self) -> bool {
        self.forge.enabled()
    }

    fn forge_recorder(&self) -> SharedForgeMetricsRecorder {
        self.forge.clone()
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
        {
            let product = &self.product;
            product.file_uploads.inc(&[mode, status], 1);
        }
    }

    fn record_file_download(&self, source: &'static str, outcome: &'static str, has_range: bool) {
        {
            let product = &self.product;
            let range = if has_range { "range" } else { "full" };
            product.file_downloads.inc(&[source, outcome, range], 1);
        }
    }

    fn record_upload_session(&self, mode: &'static str) {
        {
            let product = &self.product;
            product.upload_sessions.inc(&[mode], 1);
        }
    }

    fn record_upload_session_event(
        &self,
        mode: &'static str,
        event: &'static str,
        status: &'static str,
    ) {
        {
            let product = &self.product;
            product.upload_session_events.inc(&[mode, event, status], 1);
        }
    }

    fn record_storage_routing(&self, selection: &'static str, outcome: &'static str) {
        {
            let product = &self.product;
            product
                .storage_routing_decisions
                .inc(&[selection, outcome], 1);
        }
    }

    fn record_storage_routing_detail(
        &self,
        profile_id: &str,
        rule_id: &str,
        policy_id: &str,
        selection: &str,
        outcome: &str,
    ) {
        {
            let product = &self.product;
            product
                .storage_routing_details
                .inc(&[profile_id, rule_id, policy_id, selection, outcome], 1);
        }
    }

    fn record_upload_capacity_admission(&self, outcome: &'static str) {
        {
            let product = &self.product;
            product.upload_capacity_admissions.inc(&[outcome], 1);
        }
    }

    fn record_upload_staging_capacity_admission(&self, outcome: &'static str) {
        {
            let product = &self.product;
            product
                .upload_staging_capacity_admissions
                .inc(&[outcome], 1);
        }
    }

    fn record_storage_capacity_probe_cache(&self, outcome: &'static str) {
        {
            let product = &self.product;
            product.storage_capacity_probe_cache.inc(&[outcome], 1);
        }
    }

    fn record_storage_capacity_probe(&self, outcome: &'static str, duration_seconds: f64) {
        {
            let product = &self.product;
            product
                .storage_capacity_probe_duration
                .observe(&[outcome], duration_seconds);
        }
    }

    fn record_upload_data_plane(&self, data_plane: &'static str, status: &'static str) {
        {
            let product = &self.product;
            product.upload_data_planes.inc(&[data_plane, status], 1);
        }
    }

    fn record_background_task_transition(&self, kind: &'static str, status: &'static str) {
        self.forge.record_background_task_transition(kind, status);
    }

    fn record_avatar_upload(&self, status: &'static str) {
        {
            let product = &self.product;
            product.avatar_uploads.inc(&[status], 1);
        }
    }

    fn record_avatar_source_bytes(&self, bytes: u64) {
        {
            let product = &self.product;
            product.avatar_source_bytes.observe(&[], bytes as f64);
        }
    }

    fn record_avatar_dimension(&self, axis: &'static str, pixels: u32) {
        {
            let product = &self.product;
            product
                .avatar_dimensions
                .observe(&[axis], f64::from(pixels));
        }
    }

    fn set_avatar_budget_bytes(&self, budget: &'static str, bytes: u64) {
        {
            let product = &self.product;
            product.avatar_budget_bytes.set(&[budget], bytes as f64);
        }
    }

    fn record_avatar_render_duration(&self, processor: &'static str, duration_seconds: f64) {
        {
            let product = &self.product;
            product
                .avatar_render_duration
                .observe(&[processor], duration_seconds);
        }
    }

    fn record_avatar_publish_duration(&self, status: &'static str, duration_seconds: f64) {
        {
            let product = &self.product;
            product
                .avatar_publish_duration
                .observe(&[status], duration_seconds);
        }
    }

    fn record_avatar_render_wait_duration(&self, duration_seconds: f64) {
        {
            let product = &self.product;
            product
                .avatar_render_wait_duration
                .observe(&[], duration_seconds);
        }
    }

    fn adjust_avatar_render_waiting(&self, delta: i64) {
        {
            let product = &self.product;
            product.avatar_render_waiting.add(&[], delta as f64);
        }
    }

    fn adjust_avatar_render_active(&self, delta: i64) {
        {
            let product = &self.product;
            product.avatar_render_active.add(&[], delta as f64);
        }
    }

    fn record_avatar_rejection(&self, reason: &'static str) {
        {
            let product = &self.product;
            product.avatar_rejections.inc(&[reason], 1);
        }
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
        {
            let product = &self.product;
            let labels = [driver, operation, status, kind];
            product.storage_driver_operations.inc(&labels, 1);
            product
                .storage_driver_operation_duration
                .observe(&labels, duration_seconds);
        }
    }

    fn record_stream_upload_attempt(&self, event: &'static str, status: &'static str) {
        {
            let product = &self.product;
            product.stream_upload_attempts.inc(&[event, status], 1);
        }
    }

    fn record_stream_upload_bytes(&self, kind: &'static str, bytes: u64) {
        {
            let product = &self.product;
            product.stream_upload_bytes.observe(&[kind], bytes as f64);
        }
    }

    fn adjust_stream_upload_active(&self, delta: i64) {
        {
            let product = &self.product;
            product.stream_upload_active.add(&[], delta as f64);
        }
    }

    fn record_share_download_rollback_event(&self, event: &'static str, count: u64) {
        {
            let product = &self.product;
            product.share_download_rollback_events.inc(&[event], count);
        }
    }

    fn set_share_download_rollback_pending(&self, pending: u64) {
        {
            let product = &self.product;
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

    #[cfg(feature = "metrics")]
    #[test]
    fn migration_preserves_the_entire_pre_instance_product_schema() {
        let baseline: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/pre_instance_schema.json"))
                .unwrap();
        let provider = aster_forge_metrics::MetricsProvider::builder()
            .build()
            .unwrap();
        super::product::DriveProductMetrics::register(&provider).unwrap();
        let descriptors = provider.descriptors();
        for expected in baseline["metrics"].as_array().unwrap() {
            let actual = descriptors
                .iter()
                .find(|descriptor| {
                    descriptor.subsystem == expected["subsystem"].as_str().unwrap()
                        && descriptor.name == expected["name"].as_str().unwrap()
                })
                .unwrap();
            assert_eq!(actual.help, expected["help"].as_str().unwrap());
            assert_eq!(
                format!("{:?}", actual.kind).to_ascii_lowercase(),
                expected["kind"].as_str().unwrap()
            );
            assert_eq!(
                actual.labels,
                expected["labels"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|label| label.as_str().unwrap())
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                actual.buckets,
                expected["buckets"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|bucket| bucket.as_f64().unwrap())
                    .collect::<Vec<_>>()
            );
        }
    }

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

    #[cfg(feature = "prometheus")]
    #[tokio::test]
    async fn avatar_metrics_register_record_and_export_expected_families() {
        let provider = aster_forge_metrics::MetricsProvider::builder()
            .with_prometheus()
            .unwrap()
            .build()
            .unwrap();
        let recorder = super::create_metrics_recorder(provider.clone()).unwrap();
        assert!(recorder.enabled());

        recorder.record_avatar_upload("applied");
        recorder.record_avatar_source_bytes(4096);
        recorder.record_avatar_dimension("width", 1024);
        recorder.record_avatar_dimension("height", 768);
        recorder.set_avatar_budget_bytes("decode_alloc", 32 * 1024 * 1024);
        recorder.set_avatar_budget_bytes("source_configured", 10 * 1024 * 1024);
        recorder.set_avatar_budget_bytes("source_hard_ceiling", 16 * 1024 * 1024);
        recorder.record_avatar_render_duration("images", 0.25);
        recorder.record_avatar_publish_duration("success", 0.01);
        recorder.record_avatar_publish_duration("conflict", 0.02);
        recorder.record_avatar_publish_duration("failed", 0.03);
        recorder.record_avatar_render_wait_duration(0.125);
        recorder.adjust_avatar_render_waiting(1);
        recorder.adjust_avatar_render_waiting(-1);
        recorder.adjust_avatar_render_active(1);
        recorder.adjust_avatar_render_active(-1);
        recorder.record_avatar_rejection("source");
        recorder.record_avatar_rejection("dimensions");
        recorder.record_avatar_rejection("decode_or_render");
        recorder.record_avatar_rejection("processor_unavailable");

        let body = provider
            .export_prometheus(std::time::Duration::from_secs(1))
            .await
            .expect("Drive avatar metrics should export");
        assert!(body.contains("avatar_uploads_total"));
        assert!(body.contains("status=\"applied\""));
        assert!(body.contains("avatar_source_bytes_bucket"));
        assert!(body.contains("avatar_source_bytes_count"));
        assert!(body.contains("avatar_dimension_pixels_bucket"));
        assert!(body.contains("axis=\"width\""));
        assert!(body.contains("axis=\"height\""));
        assert!(body.contains("avatar_budget_bytes"));
        assert!(body.contains("budget=\"decode_alloc\""));
        assert!(body.contains("budget=\"source_configured\""));
        assert!(body.contains("budget=\"source_hard_ceiling\""));
        assert!(body.contains("avatar_render_duration_seconds_bucket"));
        assert!(body.contains("avatar_render_duration_seconds_count"));
        assert!(body.contains("avatar_publish_duration_seconds_bucket"));
        assert!(body.contains("avatar_publish_duration_seconds_count"));
        assert!(body.contains("status=\"success\""));
        assert!(body.contains("status=\"conflict\""));
        assert!(body.contains("status=\"failed\""));
        assert!(body.contains("avatar_render_wait_duration_seconds_bucket"));
        assert!(body.contains("avatar_render_wait_duration_seconds_count"));
        assert!(body.contains("avatar_render_waiting 0"));
        assert!(body.contains("avatar_render_active 0"));
        assert!(body.contains("avatar_rejections_total"));
        assert!(body.contains("reason=\"source\""));
        assert!(body.contains("reason=\"dimensions\""));
        assert!(body.contains("reason=\"decode_or_render\""));
        assert!(body.contains("reason=\"processor_unavailable\""));
    }

    #[cfg(feature = "prometheus")]
    #[tokio::test]
    async fn stream_upload_metrics_register_record_and_export_expected_families() {
        let provider = aster_forge_metrics::MetricsProvider::builder()
            .with_prometheus()
            .unwrap()
            .build()
            .unwrap();
        let recorder = super::create_metrics_recorder(provider.clone()).unwrap();

        recorder.record_stream_upload_attempt("attempt", "started");
        recorder.record_stream_upload_attempt("commit", "success");
        recorder.record_stream_upload_attempt("abort", "cleaned");
        recorder.record_stream_upload_attempt("abort", "not_required");
        recorder.record_stream_upload_attempt("abort", "deferred");
        recorder.record_stream_upload_attempt("abort", "failed");
        recorder.record_stream_upload_bytes("expected", 10 * 1024 * 1024 * 1024);
        recorder.adjust_stream_upload_active(1);
        recorder.adjust_stream_upload_active(-1);

        let body = provider
            .export_prometheus(std::time::Duration::from_secs(1))
            .await
            .expect("Drive stream upload metrics should export");
        assert!(body.contains("stream_upload_attempts_total"));
        assert!(body.contains("event=\"attempt\""));
        assert!(body.contains("event=\"commit\""));
        assert!(body.contains("event=\"abort\""));
        assert!(body.contains("status=\"cleaned\""));
        assert!(body.contains("status=\"not_required\""));
        assert!(body.contains("status=\"deferred\""));
        assert!(body.contains("stream_upload_bytes_bucket"));
        assert!(body.contains("kind=\"expected\""));
        assert!(body.contains("le=\"10737418240\""));
        assert!(body.contains("stream_upload_active 0"));
    }

    #[cfg(feature = "prometheus")]
    #[tokio::test]
    async fn upload_admission_metrics_use_bounded_status_labels() {
        let provider = aster_forge_metrics::MetricsProvider::builder()
            .with_prometheus()
            .unwrap()
            .build()
            .unwrap();
        let recorder = super::create_metrics_recorder(provider.clone()).unwrap();

        for outcome in ["sufficient", "insufficient", "unsupported", "unavailable"] {
            recorder.record_upload_capacity_admission(outcome);
        }
        for outcome in ["sufficient", "insufficient", "unavailable", "recovered"] {
            recorder.record_upload_staging_capacity_admission(outcome);
        }
        for outcome in [
            "fresh",
            "stale_sufficient",
            "stale_after_error",
            "confirm_refresh",
            "cold",
            "descriptor_unsupported",
        ] {
            recorder.record_storage_capacity_probe_cache(outcome);
        }
        for outcome in [
            "supported",
            "unsupported",
            "unavailable",
            "failure",
            "timeout",
        ] {
            recorder.record_storage_capacity_probe(outcome, 0.01);
        }
        recorder.record_upload_data_plane("streaming_direct", "success");
        recorder.record_upload_data_plane("staged", "failure");

        let body = provider
            .export_prometheus(std::time::Duration::from_secs(1))
            .await
            .expect("Drive upload admission metrics should export");
        assert!(body.contains("upload_capacity_admissions_total"));
        assert!(body.contains("outcome=\"sufficient\""));
        assert!(body.contains("outcome=\"insufficient\""));
        assert!(body.contains("outcome=\"unsupported\""));
        assert!(body.contains("outcome=\"unavailable\""));
        assert!(body.contains("upload_staging_capacity_admissions_total"));
        assert!(body.contains("outcome=\"recovered\""));
        assert!(body.contains("storage_capacity_probe_cache_total"));
        assert!(body.contains("outcome=\"stale_sufficient\""));
        assert!(body.contains("outcome=\"stale_after_error\""));
        assert!(body.contains("outcome=\"descriptor_unsupported\""));
        assert!(body.contains("storage_capacity_probe_duration_seconds_bucket"));
        assert!(body.contains("outcome=\"timeout\""));
        assert!(body.contains("upload_data_planes_total"));
        assert!(body.contains("data_plane=\"streaming_direct\""));
        assert!(body.contains("data_plane=\"staged\""));
    }
}

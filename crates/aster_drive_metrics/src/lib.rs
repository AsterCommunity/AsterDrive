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
            admission_rejections: counter("aster.drive.upload", "admission.rejections", "Upload requests rejected by a stable resource boundary.", &["reason"]).with_unit("{event}"),
            transfer_bytes: counter("aster.drive.file", "transfer", "Observed bytes consumed at AD data-plane boundaries.", &["direction", "source", "mode"]).with_unit("By"),
            transfer_active: gauge("aster.drive.file", "transfers.in.flight", "Active AD data-plane streams.", &["direction", "source", "mode"]).with_unit("{count}"),
            transfer_duration: histogram_with_buckets("aster.drive.file", "transfer.duration", "AD stream lifecycle duration through EOF error or drop.", &["direction", "source", "mode", "outcome"], &[0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 15.0, 60.0, 300.0, 900.0, 3600.0]).with_unit("s"),
            upload_phase: histogram_with_buckets("aster.drive.upload", "phase.duration", "Upload phase duration, excluding completed retries.", &["mode", "phase", "status"], &[0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 15.0, 60.0, 300.0]).with_unit("s"),
            upload_backlog: gauge("aster.drive.upload", "sessions.active", "Persisted upload session snapshot by mode and state.", &["mode", "state"]).with_unit("{count}"),
            upload_oldest: gauge("aster.drive.upload", "session.oldest.age", "Oldest persisted session age since creation.", &["mode", "state"]).with_unit("s"),
            cleanup_pending: gauge("aster.drive.blob", "cleanup.pending", "Unreferenced or claimed blobs pending physical cleanup.", &[]).with_unit("{count}"),
            cleanup_oldest: gauge("aster.drive.blob", "cleanup.oldest.age", "Oldest stored blob cleanup interval age; retained across retries.", &[]).with_unit("s"),
            cleanup_objects: counter("aster.drive.blob", "cleanup.objects", "Claimed blob cleanup outcomes.", &["outcome"]).with_unit("{event}"),
            cleanup_retired: counter("aster.drive.blob", "cleanup.retired.backing", "Blob backing bytes retired after deletion acknowledgment and metadata removal.", &[]).with_unit("By"),
            snapshot_valid: gauge("aster.drive.observability", "snapshot.valid", "Whether the last sampling attempt succeeded.", &["source"]).with_unit("{count}"),
            snapshot_updated: gauge("aster.drive.observability", "snapshot.last.success.timestamp", "Unix timestamp of the last successful sample.", &["source"]).with_unit("s"),
            capacity_known: gauge("aster.drive.storage", "capacity.known", "Whether this capacity dimension is supported and current.", &["scope", "kind"]).with_unit("{count}"),
            capacity: gauge("aster.drive.storage", "capacity", "Capacity snapshot; targets report independent minima and must not be summed.", &["scope", "kind"]).with_unit("By"),
            staging: gauge("aster.drive.upload", "staging", "Persisted staging and reservation snapshot bytes.", &["kind"]).with_unit("By"),
            remote_nodes: gauge("aster.drive.remote", "nodes", "Persisted node status snapshot.", &["status"]).with_unit("{count}"),
            remote_connections: gauge("aster.drive.remote", "tunnel.connections", "Locally established reverse tunnel connections.", &[]).with_unit("{count}"),
            remote_reconnects: counter("aster.drive.remote", "tunnel.reconnects", "Reverse tunnel connection retry decisions.", &["reason"]).with_unit("{event}"),
            remote_requests: counter("aster.drive.remote", "requests", "Remote transport requests observed by this node; separate from driver operations.", &["transport", "operation", "status", "reason"]).with_unit("{event}"),
            remote_duration: histogram_with_buckets("aster.drive.remote", "request.duration", "Remote transport request duration.", &["transport", "operation", "status"], &[0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 15.0, 60.0, 300.0, 3600.0]).with_unit("s"),
            file_uploads: counter("aster.drive.file", "uploads",
                "Total Drive file upload attempts.",
                &["mode", "status"],
            ).with_unit("{event}"),
            file_downloads: counter("aster.drive.file", "downloads",
                "Total Drive file download attempts.",
                &["source", "outcome", "range"],
            ).with_unit("{event}"),
            upload_sessions: counter("aster.drive.upload", "sessions",
                "Total Drive upload sessions created.",
                &["mode"],
            ).with_unit("{event}"),
            upload_session_events: counter("aster.drive.upload", "session.events",
                "Total Drive upload session lifecycle events.",
                &["mode", "event", "status"],
            ).with_unit("{event}"),
            storage_routing_decisions: counter("aster.drive.storage", "routing.decisions",
                "Total storage placement routing decisions.",
                &["selection", "outcome"],
            ).with_unit("{event}"),
            storage_routing_details: counter("aster.drive.storage", "routing.details",
                "Storage placement decisions by bounded topology identifiers.",
                &["profile", "rule", "policy", "selection", "outcome"],
            ).with_unit("{event}"),
            upload_capacity_admissions: counter("aster.drive.upload", "capacity.admissions",
                "Upload target capacity assessment outcomes.",
                &["outcome"],
            ).with_unit("{event}"),
            upload_staging_capacity_admissions: counter("aster.drive.upload", "staging.capacity.admissions",
                "Physical upload-staging reservation outcomes.",
                &["outcome"],
            ).with_unit("{event}"),
            storage_capacity_probe_cache: counter("aster.drive.storage", "capacity.probe.cache",
                "Capacity observation cache decisions.",
                &["outcome"],
            ).with_unit("{event}"),
            storage_capacity_probe_duration: histogram_with_buckets("aster.drive.storage", "capacity.probe.duration",
                "Storage capacity probe duration by bounded outcome.",
                &["outcome"],
                &[0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0],
            ).with_unit("s"),
            upload_data_planes: counter("aster.drive.upload", "data.planes",
                "Upload initialization outcomes by selected data plane.",
                &["data_plane", "status"],
            ).with_unit("{event}"),
            storage_driver_operations: counter("aster.drive.storage.driver", "operations",
                "Total Drive storage driver operations.",
                &["driver", "operation", "status", "kind"],
            ).with_unit("{event}"),
    storage_driver_operation_duration: histogram_with_buckets("aster.drive.storage.driver", "operation.duration",
                "Drive storage driver operation duration in seconds.",
                &["driver", "operation", "status", "kind"],
                &[0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 5.0, 15.0, 60.0],
            ).with_unit("s"),
            stream_upload_attempts: counter("aster.drive.stream.upload", "attempts",
                "Total streaming upload attempt lifecycle events.",
                &["event", "status"],
            ).with_unit("{event}"),
            stream_upload_bytes: histogram_with_buckets("aster.drive.stream.upload", "bytes",
                "Streaming upload attempt byte counts.",
                &["kind"],
                &[1024.0, 64.0 * 1024.0, 1024.0 * 1024.0, 5.0 * 1024.0 * 1024.0, 50.0 * 1024.0 * 1024.0, 1024.0 * 1024.0 * 1024.0, 10.0 * 1024.0 * 1024.0 * 1024.0],
            ).with_unit("By"),
            stream_upload_active: gauge("aster.drive.stream.upload", "active",
                "Streaming upload attempts currently in the data plane.",
                &[],
            ).with_unit("{count}"),
            avatar_uploads: counter("aster.drive.avatar", "uploads",
                "Total synchronous avatar upload outcomes.",
                &["status"],
            ).with_unit("{event}"),
            avatar_source_bytes: histogram_with_buckets("aster.drive.avatar", "source",
                "Compressed avatar source bytes accepted by staging.",
                &[],
                &[1024.0, 64.0 * 1024.0, 1024.0 * 1024.0, 4.0 * 1024.0 * 1024.0, 10.0 * 1024.0 * 1024.0, 16.0 * 1024.0 * 1024.0],
            ).with_unit("By"),
            avatar_dimensions: histogram_with_buckets("aster.drive.avatar", "dimension",
                "Validated avatar source dimensions in pixels.",
                &["axis"],
                &[128.0, 256.0, 512.0, 1024.0],
            ).with_unit("{pixel}"),
            avatar_budget_bytes: gauge("aster.drive.avatar", "budget",
                "Configured avatar source and decoder allocation boundaries in bytes.",
                &["budget"],
            ).with_unit("By"),
            avatar_render_duration: histogram_with_buckets("aster.drive.avatar", "render.duration",
                "Synchronous avatar render duration in seconds.",
                &["processor"],
                &[0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 15.0],
            ).with_unit("s"),
            avatar_publish_duration: histogram_with_buckets("aster.drive.avatar", "publish.duration",
                "Filesystem publication duration for rendered avatar variants.",
                &["status"],
                &[0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 15.0],
            ).with_unit("s"),
            avatar_render_wait_duration: histogram_with_buckets("aster.drive.avatar", "render.wait.duration",
                "Time spent waiting for an avatar render concurrency permit.",
                &[],
                &[0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 15.0, 30.0, 60.0],
            ).with_unit("s"),
            avatar_render_waiting: gauge("aster.drive.avatar", "render.waiting",
                "Avatar renders waiting for a concurrency permit.",
                &[],
            ).with_unit("{count}"),
            avatar_render_active: gauge("aster.drive.avatar", "render.active",
                "Avatar renders currently holding a concurrency permit.",
                &[],
            ).with_unit("{count}"),
            avatar_rejections: counter("aster.drive.avatar", "rejections",
                "Total avatar upload rejections by boundary.",
                &["reason"],
            ).with_unit("{event}"),
            share_download_rollback_events: counter("aster.drive.share.download.rollback", "events",
                "Total shared download rollback queue events.",
                &["event"],
            ).with_unit("{event}"),
            share_download_rollback_pending: gauge("aster.drive.share.download.rollback", "pending",
                "Pending shared download rollback operations.",
                &[],
            ).with_unit("{count}"),
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
    fn product_catalog_is_namespaced_and_has_unique_wire_names() {
        let provider = aster_forge_metrics::MetricsProvider::builder()
            .with_prometheus()
            .unwrap()
            .build()
            .unwrap();
        super::product::DriveProductMetrics::register(&provider).unwrap();
        let descriptors = provider.descriptors();
        assert_eq!(descriptors.len(), 49);
        let mut names = std::collections::BTreeSet::new();
        for descriptor in descriptors {
            assert!(descriptor.subsystem.starts_with("aster.drive."));
            assert!(names.insert(descriptor.prometheus_name()));
            assert!(!descriptor.name.ends_with("_total"));
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
        assert!(body.contains("aster_drive_avatar_uploads_total"));
        assert!(body.contains("status=\"applied\""));
        assert!(body.contains("aster_drive_avatar_source_bytes_bucket"));
        assert!(body.contains("aster_drive_avatar_source_bytes_count"));
        assert!(body.contains("aster_drive_avatar_dimension_bucket"));
        assert!(body.contains("axis=\"width\""));
        assert!(body.contains("axis=\"height\""));
        assert!(body.contains("aster_drive_avatar_budget_bytes"));
        assert!(body.contains("budget=\"decode_alloc\""));
        assert!(body.contains("budget=\"source_configured\""));
        assert!(body.contains("budget=\"source_hard_ceiling\""));
        assert!(body.contains("aster_drive_avatar_render_duration_seconds_bucket"));
        assert!(body.contains("aster_drive_avatar_render_duration_seconds_count"));
        assert!(body.contains("aster_drive_avatar_publish_duration_seconds_bucket"));
        assert!(body.contains("aster_drive_avatar_publish_duration_seconds_count"));
        assert!(body.contains("status=\"success\""));
        assert!(body.contains("status=\"conflict\""));
        assert!(body.contains("status=\"failed\""));
        assert!(body.contains("aster_drive_avatar_render_wait_duration_seconds_bucket"));
        assert!(body.contains("aster_drive_avatar_render_wait_duration_seconds_count"));
        assert!(body.contains("aster_drive_avatar_render_waiting 0"));
        assert!(body.contains("aster_drive_avatar_render_active 0"));
        assert!(body.contains("aster_drive_avatar_rejections_total"));
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
        assert!(body.contains("aster_drive_stream_upload_attempts_total"));
        assert!(body.contains("event=\"attempt\""));
        assert!(body.contains("event=\"commit\""));
        assert!(body.contains("event=\"abort\""));
        assert!(body.contains("status=\"cleaned\""));
        assert!(body.contains("status=\"not_required\""));
        assert!(body.contains("status=\"deferred\""));
        assert!(body.contains("aster_drive_stream_upload_bytes_bytes_bucket"));
        assert!(body.contains("kind=\"expected\""));
        assert!(body.contains("le=\"10737418240\""));
        assert!(body.contains("aster_drive_stream_upload_active 0"));
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
        assert!(body.contains("aster_drive_upload_capacity_admissions_total"));
        assert!(body.contains("outcome=\"sufficient\""));
        assert!(body.contains("outcome=\"insufficient\""));
        assert!(body.contains("outcome=\"unsupported\""));
        assert!(body.contains("outcome=\"unavailable\""));
        assert!(body.contains("aster_drive_upload_staging_capacity_admissions_total"));
        assert!(body.contains("outcome=\"recovered\""));
        assert!(body.contains("aster_drive_storage_capacity_probe_cache_total"));
        assert!(body.contains("outcome=\"stale_sufficient\""));
        assert!(body.contains("outcome=\"stale_after_error\""));
        assert!(body.contains("outcome=\"descriptor_unsupported\""));
        assert!(body.contains("aster_drive_storage_capacity_probe_duration_seconds_bucket"));
        assert!(body.contains("outcome=\"timeout\""));
        assert!(body.contains("aster_drive_upload_data_planes_total"));
        assert!(body.contains("data_plane=\"streaming_direct\""));
        assert!(body.contains("data_plane=\"staged\""));
    }
    #[cfg(feature = "prometheus")]
    #[tokio::test]
    async fn data_plane_records_counts_terminal_outcomes_and_remote_reasons_exactly_once() {
        use super::*;
        let provider = aster_forge_metrics::MetricsProvider::builder()
            .with_prometheus()
            .unwrap()
            .build()
            .unwrap();
        let metrics = create_metrics_recorder(provider.clone()).unwrap();
        let labels = TransferLabels {
            direction: TransferDirection::Ingress,
            source: TransferSource::File,
            mode: TransferMode::Chunked,
        };
        let mut transfer = TransferGuard::new(metrics.clone(), labels);
        transfer.bytes(3);
        transfer.bytes(7);
        transfer.finish(TransferOutcome::Failure);
        transfer.finish(TransferOutcome::Success);
        drop(transfer);
        metrics.admission_rejection(AdmissionReason::Quota);
        metrics.upload_phase(
            "chunked",
            UploadPhase::Ingest,
            TransferOutcome::Failure,
            0.25,
        );
        metrics.blob_cleanup(CleanupOutcome::Deleted, 42);
        metrics.remote_nodes("online", 2);
        metrics.remote_connections(1);
        metrics.remote_connections(-1);
        metrics.remote_reconnect(RemoteReason::Disconnected);
        metrics.remote_request(
            RemoteTransport::Reverse,
            RemoteOperation::Write,
            TransferOutcome::Failure,
            RemoteReason::Timeout,
            0.5,
        );
        let body = provider
            .export_prometheus(std::time::Duration::from_secs(1))
            .await
            .unwrap();
        for expected in [
            "aster_drive_file_transfer_bytes_total{direction=\"ingress\",mode=\"chunked\",source=\"file\"} 10",
            "aster_drive_file_transfers_in_flight{direction=\"ingress\",mode=\"chunked\",source=\"file\"} 0",
            "aster_drive_file_transfer_duration_seconds_count{direction=\"ingress\",mode=\"chunked\",outcome=\"failure\",source=\"file\"} 1",
            "aster_drive_upload_admission_rejections_total{reason=\"quota\"} 1",
            "aster_drive_upload_phase_duration_seconds_sum{mode=\"chunked\",phase=\"ingest\",status=\"failure\"} 0.25",
            "aster_drive_blob_cleanup_retired_backing_bytes_total 42",
            "aster_drive_remote_nodes{status=\"online\"} 2",
            "aster_drive_remote_tunnel_connections 0",
            "aster_drive_remote_tunnel_reconnects_total{reason=\"disconnected\"} 1",
            "aster_drive_remote_requests_total{operation=\"write\",reason=\"timeout\",status=\"failure\",transport=\"reverse\"} 1",
            "aster_drive_remote_request_duration_seconds_sum{operation=\"write\",status=\"failure\",transport=\"reverse\"} 0.5",
        ] {
            assert!(
                body.lines().any(|line| line == expected),
                "missing {expected} in {body}"
            );
        }
        assert!(
            !body.contains("outcome=\"success\""),
            "terminal guard must not double count"
        );
    }

    #[cfg(feature = "prometheus")]
    #[tokio::test]
    async fn invalid_snapshots_and_unknown_capacity_preserve_last_observation_with_validity_flags()
    {
        let provider = aster_forge_metrics::MetricsProvider::builder()
            .with_prometheus()
            .unwrap()
            .build()
            .unwrap();
        let metrics = super::create_metrics_recorder(provider.clone()).unwrap();
        metrics.upload_backlog("stream", "expired", 3, 120.0);
        metrics.blob_cleanup_backlog(2, 240.0);
        metrics.staging_snapshot("reserved", 42);
        metrics.capacity_snapshot("staging", Some(80), Some(100), Some(20));
        metrics.snapshot_status("database", true, 1234.0);
        metrics.snapshot_status("database", false, 9999.0);
        metrics.capacity_snapshot("staging", None, None, None);
        let body = provider
            .export_prometheus(std::time::Duration::from_secs(1))
            .await
            .unwrap();
        for expected in [
            "aster_drive_upload_sessions_active{mode=\"stream\",state=\"expired\"} 3",
            "aster_drive_upload_session_oldest_age_seconds{mode=\"stream\",state=\"expired\"} 120",
            "aster_drive_blob_cleanup_pending 2",
            "aster_drive_blob_cleanup_oldest_age_seconds 240",
            "aster_drive_observability_snapshot_valid{source=\"database\"} 0",
            "aster_drive_observability_snapshot_last_success_timestamp_seconds{source=\"database\"} 1234",
            "aster_drive_storage_capacity_bytes{kind=\"available\",scope=\"staging\"} 80",
            "aster_drive_storage_capacity_bytes{kind=\"total\",scope=\"staging\"} 100",
            "aster_drive_storage_capacity_bytes{kind=\"used\",scope=\"staging\"} 20",
            "aster_drive_storage_capacity_known{kind=\"available\",scope=\"staging\"} 0",
            "aster_drive_storage_capacity_known{kind=\"total\",scope=\"staging\"} 0",
            "aster_drive_storage_capacity_known{kind=\"used\",scope=\"staging\"} 0",
        ] {
            assert!(
                body.lines().any(|line| line == expected),
                "missing {expected} in {body}"
            );
        }
    }
    #[cfg(feature = "prometheus")]
    #[tokio::test]
    async fn product_and_shared_recorders_use_the_same_provider_without_cross_instance_writes() {
        let provider = aster_forge_metrics::MetricsProvider::builder()
            .with_prometheus()
            .unwrap()
            .build()
            .unwrap();
        let isolated = aster_forge_metrics::MetricsProvider::builder()
            .with_prometheus()
            .unwrap()
            .build()
            .unwrap();
        let metrics = super::create_metrics_recorder(provider.clone()).unwrap();
        let _isolated_metrics = super::create_metrics_recorder(isolated.clone()).unwrap();
        metrics.record_auth_event("login", "failure", "credentials");
        metrics.record_config_reload("database", "applied", "success", 2, 0.125);
        metrics.record_config_mutation("admin", "update", "success", 3);
        metrics.record_background_task_transition("cleanup", "retry");
        metrics.set_background_tasks_pending(4);
        metrics.record_file_upload("stream", "success");
        metrics.record_file_download("share", "success", true);
        metrics.record_upload_session("chunked");
        metrics.record_upload_session_event("chunked", "complete", "success");
        metrics.record_storage_routing("default", "selected");
        metrics.record_storage_routing_detail("profile", "rule", "policy", "default", "selected");
        metrics.record_storage_driver_operation("local", "read", "success", "stream", 0.5);
        metrics.record_share_download_rollback_event("retried", 2);
        metrics.set_share_download_rollback_pending(1);
        let body = metrics
            .forge_recorder()
            .provider()
            .unwrap()
            .export_prometheus(std::time::Duration::from_secs(1))
            .await
            .unwrap();
        for expected in [
            "aster_auth_events_total{action=\"login\",reason=\"credentials\",status=\"failure\"} 1",
            "aster_config_reloads_total{decision=\"applied\",source=\"database\",status=\"success\"} 1",
            "aster_config_mutations_total{operation=\"update\",source=\"admin\",status=\"success\"} 1",
            "aster_task_transitions_total{kind=\"cleanup\",status=\"retry\"} 1",
            "aster_task_retries_total{kind=\"cleanup\"} 1",
            "aster_task_pending 4",
            "aster_drive_file_uploads_total{mode=\"stream\",status=\"success\"} 1",
            "aster_drive_file_downloads_total{outcome=\"success\",range=\"range\",source=\"share\"} 1",
            "aster_drive_storage_driver_operations_total{driver=\"local\",kind=\"stream\",operation=\"read\",status=\"success\"} 1",
            "aster_drive_share_download_rollback_events_total{event=\"retried\"} 2",
            "aster_drive_share_download_rollback_pending 1",
        ] {
            assert!(
                body.lines().any(|line| line == expected),
                "missing {expected} in {body}"
            );
        }
        assert_eq!(
            body,
            provider
                .export_prometheus(std::time::Duration::from_secs(1))
                .await
                .unwrap()
        );
        let other = isolated
            .export_prometheus(std::time::Duration::from_secs(1))
            .await
            .unwrap();
        assert!(!other.contains("action=\"login\""));
        assert!(!other.contains("source=\"share\""));
    }
}

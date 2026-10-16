//! Bounded product labels and cancellation-safe lifecycle ownership.

use crate::SharedMetricsRecorder;
use std::time::Instant;

macro_rules! labels {
    ($name:ident { $($variant:ident => $label:literal),* $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name { $($variant),* }
        impl $name { pub const fn label(self) -> &'static str { match self { $(Self::$variant => $label),* } } }
    };
}
labels!(AdmissionReason { Quota => "quota", TargetCapacity => "target_capacity", StagingCapacity => "staging_capacity", Concurrency => "concurrency", CapacityUnavailable => "capacity_unavailable" });
labels!(TransferDirection { Ingress => "ingress", Egress => "egress" });
labels!(TransferSource { File => "file", Share => "share", Webdav => "webdav", Wopi => "wopi", Remote => "remote" });
labels!(TransferMode { Stream => "stream", Chunked => "chunked", Multipart => "multipart", Range => "range", Resumable => "resumable", Presigned => "presigned" });
labels!(TransferOutcome { Success => "success", Failure => "failure", Cancelled => "cancelled" });
labels!(UploadPhase { Assemble => "assemble", Finalize => "finalize", Complete => "complete", Ingest => "ingest" });
labels!(CleanupOutcome { Deleted => "deleted", Missing => "missing", Virtual => "virtual", Failure => "failure", Cancelled => "cancelled" });
labels!(RemoteTransport { Direct => "direct", Reverse => "reverse" });
labels!(RemoteOperation { Read => "read", Write => "write", Delete => "delete", Metadata => "metadata", Capacity => "capacity", Control => "control" });
labels!(RemoteReason { Ok => "ok", Timeout => "timeout", Disconnected => "disconnected", Rejected => "rejected", Error => "error", Cancelled => "cancelled" });

#[derive(Clone, Copy, Debug)]
pub struct TransferLabels {
    pub direction: TransferDirection,
    pub source: TransferSource,
    pub mode: TransferMode,
}
impl TransferLabels {
    pub const fn values(self) -> [&'static str; 3] {
        [
            self.direction.label(),
            self.source.label(),
            self.mode.label(),
        ]
    }
}

/// Own exactly one gauge increment and terminal duration observation. Actual bytes
/// are recorded as consumed, including bytes preceding an error or cancellation.
pub struct TransferGuard {
    metrics: SharedMetricsRecorder,
    labels: TransferLabels,
    started: Option<Instant>,
    pending_bytes: u64,
}
impl TransferGuard {
    pub fn new(metrics: SharedMetricsRecorder, labels: TransferLabels) -> Self {
        let started = metrics.enabled().then(Instant::now);
        if started.is_some() {
            metrics.transfer_active(labels, 1);
        }
        Self {
            metrics,
            labels,
            started,
            pending_bytes: 0,
        }
    }
    pub fn bytes(&mut self, bytes: u64) {
        if self.started.is_none() {
            return;
        }
        self.pending_bytes = self.pending_bytes.saturating_add(bytes);
        if self.pending_bytes >= 1024 * 1024 {
            self.flush_bytes();
        }
    }
    fn flush_bytes(&mut self) {
        if self.pending_bytes > 0 {
            self.metrics
                .transfer_bytes(self.labels, std::mem::take(&mut self.pending_bytes));
        }
    }
    pub fn finish(&mut self, outcome: TransferOutcome) {
        if let Some(started) = self.started.take() {
            self.flush_bytes();
            self.metrics.transfer_active(self.labels, -1);
            self.metrics
                .transfer_finished(self.labels, outcome, started.elapsed().as_secs_f64());
        }
    }
}
impl Drop for TransferGuard {
    fn drop(&mut self) {
        self.finish(TransferOutcome::Cancelled);
    }
}

pub struct UploadPhaseGuard {
    metrics: SharedMetricsRecorder,
    mode: &'static str,
    phase: UploadPhase,
    started: Option<Instant>,
}
impl UploadPhaseGuard {
    pub fn new(metrics: SharedMetricsRecorder, mode: &'static str, phase: UploadPhase) -> Self {
        let started = metrics.enabled().then(Instant::now);
        Self {
            metrics,
            mode,
            phase,
            started,
        }
    }
    pub fn finish(&mut self, outcome: TransferOutcome) {
        if let Some(started) = self.started.take() {
            self.metrics.upload_phase(
                self.mode,
                self.phase,
                outcome,
                started.elapsed().as_secs_f64(),
            );
        }
    }
}
impl Drop for UploadPhaseGuard {
    fn drop(&mut self) {
        self.finish(TransferOutcome::Cancelled);
    }
}

pub struct RemoteRequestGuard {
    metrics: SharedMetricsRecorder,
    transport: RemoteTransport,
    operation: RemoteOperation,
    started: Option<Instant>,
}
impl RemoteRequestGuard {
    pub fn new(
        metrics: SharedMetricsRecorder,
        transport: RemoteTransport,
        operation: RemoteOperation,
    ) -> Self {
        let started = metrics.enabled().then(Instant::now);
        Self {
            metrics,
            transport,
            operation,
            started,
        }
    }
    pub fn finish(&mut self, outcome: TransferOutcome, reason: RemoteReason) {
        if let Some(started) = self.started.take() {
            self.metrics.remote_request(
                self.transport,
                self.operation,
                outcome,
                reason,
                started.elapsed().as_secs_f64(),
            );
        }
    }
}
impl Drop for RemoteRequestGuard {
    fn drop(&mut self) {
        self.finish(TransferOutcome::Cancelled, RemoteReason::Cancelled);
    }
}

pub struct RemoteConnectionGuard {
    metrics: SharedMetricsRecorder,
}
impl RemoteConnectionGuard {
    pub fn new(metrics: SharedMetricsRecorder) -> Self {
        metrics.remote_connections(1);
        Self { metrics }
    }
}
impl Drop for RemoteConnectionGuard {
    fn drop(&mut self) {
        self.metrics.remote_connections(-1);
    }
}

pub struct BlobCleanupGuard {
    metrics: SharedMetricsRecorder,
    finished: bool,
}
impl BlobCleanupGuard {
    pub fn new(metrics: SharedMetricsRecorder) -> Self {
        Self {
            metrics,
            finished: false,
        }
    }
    pub fn finish(&mut self, outcome: CleanupOutcome, retired_bytes: u64) {
        if !self.finished {
            self.finished = true;
            self.metrics.blob_cleanup(outcome, retired_bytes);
        }
    }
}
impl Drop for BlobCleanupGuard {
    fn drop(&mut self) {
        self.finish(CleanupOutcome::Cancelled, 0);
    }
}

/// Observe one actual phase, including error and future-drop paths.
pub async fn observe_upload_phase<T, E>(
    metrics: SharedMetricsRecorder,
    mode: &'static str,
    phase: UploadPhase,
    operation: impl std::future::Future<Output = Result<T, E>>,
) -> Result<T, E> {
    let mut guard = UploadPhaseGuard::new(metrics, mode, phase);
    let result = operation.await;
    guard.finish(if result.is_ok() {
        TransferOutcome::Success
    } else {
        TransferOutcome::Failure
    });
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Capture {
        bytes: Mutex<u64>,
        active: Mutex<i64>,
        transfers: Mutex<Vec<TransferOutcome>>,
        phases: Mutex<Vec<TransferOutcome>>,
        cleanup: Mutex<Vec<(CleanupOutcome, u64)>>,
    }
    impl crate::MetricsRecorder for Capture {
        fn enabled(&self) -> bool {
            true
        }
        fn transfer_bytes(&self, _: TransferLabels, bytes: u64) {
            *self.bytes.lock().unwrap() += bytes;
        }
        fn transfer_active(&self, _: TransferLabels, delta: i64) {
            *self.active.lock().unwrap() += delta;
        }
        fn transfer_finished(&self, _: TransferLabels, outcome: TransferOutcome, _: f64) {
            self.transfers.lock().unwrap().push(outcome);
        }
        fn upload_phase(&self, _: &'static str, _: UploadPhase, outcome: TransferOutcome, _: f64) {
            self.phases.lock().unwrap().push(outcome);
        }
        fn blob_cleanup(&self, outcome: CleanupOutcome, bytes: u64) {
            self.cleanup.lock().unwrap().push((outcome, bytes));
        }
    }

    #[test]
    fn transfer_batches_bytes_and_releases_exactly_once_on_every_terminal_path() {
        for outcome in [
            TransferOutcome::Success,
            TransferOutcome::Failure,
            TransferOutcome::Cancelled,
        ] {
            let metrics = Arc::new(Capture::default());
            let labels = TransferLabels {
                direction: TransferDirection::Ingress,
                source: TransferSource::File,
                mode: TransferMode::Chunked,
            };
            let mut guard = TransferGuard::new(metrics.clone(), labels);
            guard.bytes(1024 * 1024 - 1);
            assert_eq!(*metrics.bytes.lock().unwrap(), 0);
            guard.bytes(2);
            assert_eq!(*metrics.bytes.lock().unwrap(), 1024 * 1024 + 1);
            guard.bytes(7);
            if outcome != TransferOutcome::Cancelled {
                guard.finish(outcome);
                guard.finish(outcome);
            }
            drop(guard);
            assert_eq!(*metrics.bytes.lock().unwrap(), 1024 * 1024 + 8);
            assert_eq!(*metrics.active.lock().unwrap(), 0);
            assert_eq!(*metrics.transfers.lock().unwrap(), vec![outcome]);
        }
    }

    #[tokio::test]
    async fn phase_errors_and_cancelled_futures_are_distinct_and_cleanup_never_double_counts() {
        let metrics = Arc::new(Capture::default());
        assert!(
            observe_upload_phase(metrics.clone(), "stream", UploadPhase::Finalize, async {
                Err::<(), _>(())
            })
            .await
            .is_err()
        );
        let mut future = Box::pin(observe_upload_phase(
            metrics.clone(),
            "stream",
            UploadPhase::Finalize,
            std::future::pending::<Result<(), ()>>(),
        ));
        assert!(
            std::future::poll_fn(|context| {
                let result = std::future::Future::poll(future.as_mut(), context);
                std::task::Poll::Ready(result.is_pending())
            })
            .await
        );
        drop(future);
        assert_eq!(
            *metrics.phases.lock().unwrap(),
            vec![TransferOutcome::Failure, TransferOutcome::Cancelled]
        );
        let mut cleanup = BlobCleanupGuard::new(metrics.clone());
        cleanup.finish(CleanupOutcome::Deleted, 10);
        cleanup.finish(CleanupOutcome::Deleted, 10);
        drop(cleanup);
        drop(BlobCleanupGuard::new(metrics.clone()));
        assert_eq!(
            *metrics.cleanup.lock().unwrap(),
            vec![
                (CleanupOutcome::Deleted, 10),
                (CleanupOutcome::Cancelled, 0)
            ]
        );
    }
}

use aster_drive_metrics::MetricsRecorder;
use aster_drive_storage::{
    StorageDriver, StreamUploadAttempt, StreamUploadCleanup, StreamUploadDriver,
};

pub(crate) struct StreamUploadMetricsGuard<'a> {
    recorder: &'a dyn MetricsRecorder,
}

impl<'a> StreamUploadMetricsGuard<'a> {
    pub(crate) fn new(recorder: &'a dyn MetricsRecorder, expected_size: i64) -> Self {
        recorder.record_stream_upload_attempt("attempt", "started");
        recorder.record_stream_upload_bytes(
            "expected",
            u64::try_from(expected_size).unwrap_or_default(),
        );
        recorder.adjust_stream_upload_active(1);
        Self { recorder }
    }
}

impl Drop for StreamUploadMetricsGuard<'_> {
    fn drop(&mut self) {
        self.recorder.adjust_stream_upload_active(-1);
    }
}

pub(crate) async fn cleanup_stream_upload_attempt(
    driver: &dyn StreamUploadDriver,
    attempt: &StreamUploadAttempt,
    metrics: &dyn MetricsRecorder,
    binding_id: i64,
    object_key: &str,
) {
    match driver.abort_attempt(attempt).await {
        Ok(StreamUploadCleanup::NotRequired) => {
            metrics.record_stream_upload_attempt("abort", "not_required");
        }
        Ok(StreamUploadCleanup::Cleaned) => {
            metrics.record_stream_upload_attempt("abort", "cleaned");
        }
        Ok(outcome) => {
            tracing::warn!(
                binding_id,
                object_key,
                storage_path = %attempt.storage_path,
                ?outcome,
                "follower upload attempt cleanup deferred"
            );
            metrics.record_stream_upload_attempt("abort", "deferred");
        }
        Err(error) => {
            tracing::warn!(
                binding_id,
                object_key,
                storage_path = %attempt.storage_path,
                "failed to cleanup follower upload attempt: {error}"
            );
            metrics.record_stream_upload_attempt("abort", "failed");
        }
    }
}

pub(crate) async fn abort_direct_stream_attempt(
    stream_driver: &dyn StreamUploadDriver,
    attempt: &StreamUploadAttempt,
    metrics: &dyn MetricsRecorder,
    driver: &dyn StorageDriver,
    prepared_upload: &crate::services::workspace::storage::PreparedNonDedupBlobUpload,
    reason: &str,
) {
    match stream_driver.abort_attempt(attempt).await {
        Ok(StreamUploadCleanup::NotRequired) => {
            metrics.record_stream_upload_attempt("abort", "not_required");
        }
        Ok(StreamUploadCleanup::Cleaned) => {
            metrics.record_stream_upload_attempt("abort", "cleaned");
        }
        Ok(outcome) => {
            tracing::warn!(
                staging_path = %attempt.staging_path,
                ?outcome,
                "direct stream attempt cleanup deferred"
            );
            metrics.record_stream_upload_attempt("abort", "deferred");
        }
        Err(error) => {
            tracing::warn!(
                staging_path = %attempt.staging_path,
                "direct stream attempt cleanup failed: {error}"
            );
            metrics.record_stream_upload_attempt("abort", "failed");
        }
    }
    crate::services::workspace::storage::cleanup_preuploaded_blob_upload(
        driver,
        prepared_upload,
        reason,
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::{
        StreamUploadMetricsGuard, abort_direct_stream_attempt, cleanup_stream_upload_attempt,
    };
    use crate::services::workspace::storage::PreparedNonDedupBlobUpload;
    use aster_drive_metrics::MetricsRecorder;
    use aster_drive_storage::{
        BlobMetadata, StorageDriver, StorageError, StorageErrorKind, StreamUploadAttempt,
        StreamUploadCleanup, StreamUploadDriver,
    };
    use async_trait::async_trait;
    use std::sync::Mutex;
    use tokio::io::AsyncRead;

    #[derive(Clone, Copy)]
    enum AbortBehavior {
        NotRequired,
        Cleaned,
        Deferred,
        Unknown,
        Failed,
    }

    impl AbortBehavior {
        fn expected_status(self) -> &'static str {
            match self {
                Self::NotRequired => "not_required",
                Self::Cleaned => "cleaned",
                Self::Deferred | Self::Unknown => "deferred",
                Self::Failed => "failed",
            }
        }
    }

    struct CleanupDriver {
        behavior: AbortBehavior,
        deleted_paths: Mutex<Vec<String>>,
    }

    impl CleanupDriver {
        fn new(behavior: AbortBehavior) -> Self {
            Self {
                behavior,
                deleted_paths: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl StorageDriver for CleanupDriver {
        async fn put(&self, _path: &str, _data: &[u8]) -> aster_drive_storage::Result<String> {
            unreachable!("cleanup tests do not upload objects")
        }

        async fn get(&self, _path: &str) -> aster_drive_storage::Result<Vec<u8>> {
            unreachable!("cleanup tests do not read objects")
        }

        async fn get_stream(
            &self,
            _path: &str,
        ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
            unreachable!("cleanup tests do not stream objects")
        }

        async fn delete(&self, path: &str) -> aster_drive_storage::Result<()> {
            self.deleted_paths.lock().unwrap().push(path.to_string());
            Ok(())
        }

        async fn exists(&self, _path: &str) -> aster_drive_storage::Result<bool> {
            unreachable!("cleanup tests do not inspect object existence")
        }

        async fn metadata(&self, _path: &str) -> aster_drive_storage::Result<BlobMetadata> {
            unreachable!("cleanup tests do not inspect object metadata")
        }
    }

    #[async_trait]
    impl StreamUploadDriver for CleanupDriver {
        async fn put_reader(
            &self,
            _storage_path: &str,
            _reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
            _size: i64,
        ) -> aster_drive_storage::Result<String> {
            unreachable!("cleanup tests do not upload readers")
        }

        async fn abort_attempt(
            &self,
            _attempt: &StreamUploadAttempt,
        ) -> aster_drive_storage::Result<StreamUploadCleanup> {
            match self.behavior {
                AbortBehavior::NotRequired => Ok(StreamUploadCleanup::NotRequired),
                AbortBehavior::Cleaned => Ok(StreamUploadCleanup::Cleaned),
                AbortBehavior::Deferred => Ok(StreamUploadCleanup::Deferred),
                AbortBehavior::Unknown => Ok(StreamUploadCleanup::Unknown),
                AbortBehavior::Failed => Err(StorageError::new(
                    StorageErrorKind::Transient,
                    "abort failed",
                )),
            }
        }

        async fn put_file(
            &self,
            _storage_path: &str,
            _local_path: &str,
        ) -> aster_drive_storage::Result<String> {
            unreachable!("cleanup tests do not upload files")
        }
    }

    #[derive(Default)]
    struct RecordingMetrics {
        events: Mutex<Vec<String>>,
        bytes: Mutex<Vec<(String, u64)>>,
        active: Mutex<i64>,
    }

    impl MetricsRecorder for RecordingMetrics {
        fn record_stream_upload_attempt(&self, event: &'static str, status: &'static str) {
            self.events
                .lock()
                .unwrap()
                .push(format!("{event}:{status}"));
        }

        fn record_stream_upload_bytes(&self, kind: &'static str, bytes: u64) {
            self.bytes.lock().unwrap().push((kind.to_string(), bytes));
        }

        fn adjust_stream_upload_active(&self, delta: i64) {
            *self.active.lock().unwrap() += delta;
        }
    }

    fn opaque_preupload() -> PreparedNonDedupBlobUpload {
        PreparedNonDedupBlobUpload::Opaque {
            upload_id: "upload-id".to_string(),
            hash_prefix: "test",
            storage_path: "files/preuploaded-object".to_string(),
            size: 4,
            policy_id: 1,
        }
    }

    #[test]
    fn metrics_guard_records_expected_bytes_and_balances_active_attempts() {
        let metrics = RecordingMetrics::default();
        {
            let _guard = StreamUploadMetricsGuard::new(&metrics, 4096);
            assert_eq!(*metrics.active.lock().unwrap(), 1);
        }

        assert_eq!(*metrics.active.lock().unwrap(), 0);
        assert_eq!(
            metrics.events.lock().unwrap().as_slice(),
            ["attempt:started"]
        );
        assert_eq!(
            metrics.bytes.lock().unwrap().as_slice(),
            [("expected".to_string(), 4096)]
        );
    }

    #[tokio::test]
    async fn follower_cleanup_records_every_abort_outcome() {
        let attempt = StreamUploadAttempt::new("files/object", 4).unwrap();

        for behavior in [
            AbortBehavior::NotRequired,
            AbortBehavior::Cleaned,
            AbortBehavior::Deferred,
            AbortBehavior::Unknown,
            AbortBehavior::Failed,
        ] {
            let driver = CleanupDriver::new(behavior);
            let metrics = RecordingMetrics::default();

            cleanup_stream_upload_attempt(&driver, &attempt, &metrics, 7, "object").await;

            assert_eq!(
                metrics.events.lock().unwrap().as_slice(),
                [format!("abort:{}", behavior.expected_status())]
            );
            assert!(driver.deleted_paths.lock().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn direct_cleanup_records_every_outcome_and_deletes_preupload() {
        let attempt = StreamUploadAttempt::new("files/object", 4).unwrap();
        let prepared = opaque_preupload();

        for behavior in [
            AbortBehavior::NotRequired,
            AbortBehavior::Cleaned,
            AbortBehavior::Deferred,
            AbortBehavior::Unknown,
            AbortBehavior::Failed,
        ] {
            let driver = CleanupDriver::new(behavior);
            let metrics = RecordingMetrics::default();

            abort_direct_stream_attempt(
                &driver,
                &attempt,
                &metrics,
                &driver,
                &prepared,
                "test cleanup",
            )
            .await;

            assert_eq!(
                metrics.events.lock().unwrap().as_slice(),
                [format!("abort:{}", behavior.expected_status())]
            );
            assert_eq!(
                driver.deleted_paths.lock().unwrap().as_slice(),
                ["files/preuploaded-object"]
            );
        }
    }
}

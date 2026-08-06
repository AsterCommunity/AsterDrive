//! StorageDriver metrics decorator.

use aster_drive_metrics::SharedMetricsRecorder;
use aster_drive_storage::traits::extensions;
use aster_drive_storage::{BlobMetadata, MultipartStorageDriver, Result, StorageDriver};
use async_trait::async_trait;
use bytes::Bytes;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, ReadBuf};

pub(crate) struct MetricsStorageDriver {
    inner: Arc<dyn StorageDriver>,
    multipart: Option<Arc<dyn MultipartStorageDriver>>,
    driver: &'static str,
    metrics: SharedMetricsRecorder,
}

pub(crate) struct MetricsMultipartStorageDriver {
    inner: Arc<dyn MultipartStorageDriver>,
    driver: &'static str,
    metrics: SharedMetricsRecorder,
}

struct TimingReader {
    inner: Box<dyn AsyncRead + Unpin + Send>,
    metrics: SharedMetricsRecorder,
    driver: &'static str,
    operation: &'static str,
    started_at: Instant,
    recorded: bool,
    finished: bool,
}

impl MetricsStorageDriver {
    pub(crate) fn new(
        inner: Arc<dyn StorageDriver>,
        driver: &'static str,
        metrics: SharedMetricsRecorder,
        multipart: Option<Arc<dyn MultipartStorageDriver>>,
    ) -> Self {
        Self {
            inner,
            multipart,
            driver,
            metrics,
        }
    }

    fn record<T>(&self, operation: &'static str, result: &Result<T>, started_at: Instant) {
        record_result(&self.metrics, self.driver, operation, result, started_at);
    }
}

impl MetricsMultipartStorageDriver {
    pub(crate) fn new(
        inner: Arc<dyn MultipartStorageDriver>,
        driver: &'static str,
        metrics: SharedMetricsRecorder,
    ) -> Self {
        Self {
            inner,
            driver,
            metrics,
        }
    }

    fn record<T>(&self, operation: &'static str, result: &Result<T>, started_at: Instant) {
        record_result(&self.metrics, self.driver, operation, result, started_at);
    }
}

fn record_result<T>(
    metrics: &SharedMetricsRecorder,
    driver: &'static str,
    operation: &'static str,
    result: &Result<T>,
    started_at: Instant,
) {
    let (status, kind) = match result {
        Ok(_) => ("success", "ok"),
        Err(error) => ("failure", error.kind().as_str()),
    };
    metrics.record_storage_driver_operation(
        driver,
        operation,
        status,
        kind,
        started_at.elapsed().as_secs_f64(),
    );
}

fn record_failure_kind(
    metrics: &SharedMetricsRecorder,
    driver: &'static str,
    operation: &'static str,
    kind: &'static str,
    started_at: Instant,
) {
    metrics.record_storage_driver_operation(
        driver,
        operation,
        "failure",
        kind,
        started_at.elapsed().as_secs_f64(),
    );
}

impl TimingReader {
    fn new(
        inner: Box<dyn AsyncRead + Unpin + Send>,
        metrics: SharedMetricsRecorder,
        driver: &'static str,
        operation: &'static str,
        started_at: Instant,
    ) -> Self {
        Self {
            inner,
            metrics,
            driver,
            operation,
            started_at,
            recorded: false,
            finished: false,
        }
    }

    fn record_success_once(&mut self) {
        if self.recorded {
            return;
        }
        self.recorded = true;
        self.finished = true;
        self.metrics.record_storage_driver_operation(
            self.driver,
            self.operation,
            "success",
            "ok",
            self.started_at.elapsed().as_secs_f64(),
        );
    }

    fn record_failure_once(&mut self) {
        if self.recorded {
            return;
        }
        self.recorded = true;
        self.finished = true;
        record_failure_kind(
            &self.metrics,
            self.driver,
            self.operation,
            "non_storage",
            self.started_at,
        );
    }

    fn record_aborted_once(&mut self) {
        if self.recorded {
            return;
        }
        self.recorded = true;
        record_failure_kind(
            &self.metrics,
            self.driver,
            self.operation,
            "aborted",
            self.started_at,
        );
    }
}

impl AsyncRead for TimingReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let requested = buf.remaining();
        match Pin::new(&mut self.inner).poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {
                if requested > 0 && buf.filled().len() == before {
                    self.record_success_once();
                }
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(error)) => {
                self.record_failure_once();
                Poll::Ready(Err(error))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Drop for TimingReader {
    fn drop(&mut self) {
        if !self.finished {
            self.record_aborted_once();
        }
    }
}

#[async_trait]
impl StorageDriver for MetricsStorageDriver {
    async fn put(&self, path: &str, data: &[u8]) -> aster_drive_storage::Result<String> {
        let started_at = Instant::now();
        let result = self.inner.put(path, data).await;
        self.record("put", &result, started_at);
        result
    }

    async fn get(&self, path: &str) -> aster_drive_storage::Result<Vec<u8>> {
        let started_at = Instant::now();
        let result = self.inner.get(path).await;
        self.record("get", &result, started_at);
        result
    }

    async fn get_stream(
        &self,
        path: &str,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        let started_at = Instant::now();
        let result = self.inner.get_stream(path).await;
        match result {
            Ok(reader) => Ok(Box::new(TimingReader::new(
                reader,
                self.metrics.clone(),
                self.driver,
                "get_stream",
                started_at,
            ))),
            Err(error) => {
                let result = Err(error);
                self.record("get_stream", &result, started_at);
                result
            }
        }
    }

    async fn get_range(
        &self,
        path: &str,
        offset: u64,
        length: Option<u64>,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        let started_at = Instant::now();
        let result = self.inner.get_range(path, offset, length).await;
        match result {
            Ok(reader) => Ok(Box::new(TimingReader::new(
                reader,
                self.metrics.clone(),
                self.driver,
                "get_range",
                started_at,
            ))),
            Err(error) => {
                let result = Err(error);
                self.record("get_range", &result, started_at);
                result
            }
        }
    }

    fn supports_efficient_range(&self) -> bool {
        self.inner.supports_efficient_range()
    }

    async fn delete(&self, path: &str) -> aster_drive_storage::Result<()> {
        let started_at = Instant::now();
        let result = self.inner.delete(path).await;
        self.record("delete", &result, started_at);
        result
    }

    async fn exists(&self, path: &str) -> aster_drive_storage::Result<bool> {
        let started_at = Instant::now();
        let result = self.inner.exists(path).await;
        self.record("exists", &result, started_at);
        result
    }

    async fn metadata(&self, path: &str) -> aster_drive_storage::Result<BlobMetadata> {
        let started_at = Instant::now();
        let result = self.inner.metadata(path).await;
        self.record("metadata", &result, started_at);
        result
    }

    async fn readiness_check(&self) -> aster_drive_storage::Result<()> {
        let started_at = Instant::now();
        let result = self.inner.readiness_check().await;
        self.record("readiness_check", &result, started_at);
        result
    }

    async fn copy_object(
        &self,
        src_path: &str,
        dest_path: &str,
    ) -> aster_drive_storage::Result<String> {
        let started_at = Instant::now();
        let result = self.inner.copy_object(src_path, dest_path).await;
        self.record("copy_object", &result, started_at);
        result
    }

    async fn capacity_info(&self) -> aster_drive_storage::Result<extensions::StorageCapacityInfo> {
        let started_at = Instant::now();
        let result = self.inner.capacity_info().await;
        self.record("capacity_info", &result, started_at);
        result
    }

    fn extensions(&self) -> extensions::StorageDriverExtensions<'_> {
        let mut extensions = self.inner.extensions();
        // Multipart is the only capability with an operation-level metrics
        // wrapper; all other capabilities can be forwarded as-is.
        extensions.multipart = self.multipart.as_deref();
        extensions
    }
}

#[async_trait]
impl MultipartStorageDriver for MetricsMultipartStorageDriver {
    async fn create_multipart_upload(&self, path: &str) -> aster_drive_storage::Result<String> {
        let started_at = Instant::now();
        let result = self.inner.create_multipart_upload(path).await;
        self.record("create_multipart_upload", &result, started_at);
        result
    }

    async fn presigned_upload_part_url(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        expires: Duration,
    ) -> aster_drive_storage::Result<String> {
        let started_at = Instant::now();
        let result = self
            .inner
            .presigned_upload_part_url(path, upload_id, part_number, expires)
            .await;
        self.record("presigned_upload_part_url", &result, started_at);
        result
    }

    async fn complete_multipart_upload(
        &self,
        path: &str,
        upload_id: &str,
        parts: Vec<(i32, String)>,
    ) -> aster_drive_storage::Result<()> {
        let started_at = Instant::now();
        let result = self
            .inner
            .complete_multipart_upload(path, upload_id, parts)
            .await;
        self.record("complete_multipart_upload", &result, started_at);
        result
    }

    async fn upload_multipart_part(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        data: &[u8],
    ) -> aster_drive_storage::Result<String> {
        let started_at = Instant::now();
        let result = self
            .inner
            .upload_multipart_part(path, upload_id, part_number, data)
            .await;
        self.record("upload_multipart_part", &result, started_at);
        result
    }

    async fn upload_multipart_part_bytes(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        data: Bytes,
    ) -> aster_drive_storage::Result<String> {
        let started_at = Instant::now();
        let result = self
            .inner
            .upload_multipart_part_bytes(path, upload_id, part_number, data)
            .await;
        self.record("upload_multipart_part_bytes", &result, started_at);
        result
    }

    async fn upload_multipart_part_reader(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> aster_drive_storage::Result<String> {
        let started_at = Instant::now();
        let result = self
            .inner
            .upload_multipart_part_reader(path, upload_id, part_number, reader, size)
            .await;
        self.record("upload_multipart_part_reader", &result, started_at);
        result
    }

    async fn abort_multipart_upload(
        &self,
        path: &str,
        upload_id: &str,
    ) -> aster_drive_storage::Result<()> {
        let started_at = Instant::now();
        let result = self.inner.abort_multipart_upload(path, upload_id).await;
        self.record("abort_multipart_upload", &result, started_at);
        result
    }

    async fn list_uploaded_part_details(
        &self,
        path: &str,
        upload_id: &str,
    ) -> aster_drive_storage::Result<
        Vec<aster_drive_storage::traits::multipart::UploadedMultipartPart>,
    > {
        let started_at = Instant::now();
        let result = self.inner.list_uploaded_part_details(path, upload_id).await;
        self.record("list_uploaded_part_details", &result, started_at);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aster_drive_metrics::MetricsRecorder;
    use parking_lot::Mutex;
    use std::io;
    use tokio::io::{AsyncReadExt, ReadBuf};

    #[derive(Default)]
    struct CapturingMetrics {
        storage_operations: Mutex<Vec<(&'static str, &'static str, &'static str)>>,
    }

    impl MetricsRecorder for CapturingMetrics {
        fn enabled(&self) -> bool {
            true
        }

        fn record_storage_driver_operation(
            &self,
            _driver: &'static str,
            operation: &'static str,
            status: &'static str,
            kind: &'static str,
            _duration_seconds: f64,
        ) {
            self.storage_operations
                .lock()
                .push((operation, status, kind));
        }
    }

    struct MemoryDriver;

    #[async_trait]
    impl StorageDriver for MemoryDriver {
        async fn put(&self, _path: &str, _data: &[u8]) -> aster_drive_storage::Result<String> {
            panic!("not used")
        }

        async fn get(&self, _path: &str) -> aster_drive_storage::Result<Vec<u8>> {
            panic!("not used")
        }

        async fn get_stream(
            &self,
            _path: &str,
        ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
            Ok(Box::new(io::Cursor::new(Vec::from("hello"))))
        }

        async fn get_range(
            &self,
            _path: &str,
            _offset: u64,
            _length: Option<u64>,
        ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
            Ok(Box::new(ErrorReader))
        }

        async fn delete(&self, _path: &str) -> aster_drive_storage::Result<()> {
            panic!("not used")
        }

        async fn exists(&self, _path: &str) -> aster_drive_storage::Result<bool> {
            panic!("not used")
        }

        async fn metadata(&self, _path: &str) -> aster_drive_storage::Result<BlobMetadata> {
            panic!("not used")
        }

        async fn readiness_check(&self) -> aster_drive_storage::Result<()> {
            Err(aster_drive_storage::StorageError::new(
                aster_drive_storage::StorageErrorKind::Unknown,
                "unclassified storage error",
            ))
        }
    }

    struct ProviderResumableDriver;

    #[async_trait]
    impl StorageDriver for ProviderResumableDriver {
        async fn put(&self, _path: &str, _data: &[u8]) -> aster_drive_storage::Result<String> {
            panic!("not used")
        }

        async fn get(&self, _path: &str) -> aster_drive_storage::Result<Vec<u8>> {
            panic!("not used")
        }

        async fn get_stream(
            &self,
            _path: &str,
        ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
            panic!("not used")
        }

        async fn delete(&self, _path: &str) -> aster_drive_storage::Result<()> {
            panic!("not used")
        }

        async fn exists(&self, _path: &str) -> aster_drive_storage::Result<bool> {
            panic!("not used")
        }

        async fn metadata(&self, _path: &str) -> aster_drive_storage::Result<BlobMetadata> {
            panic!("not used")
        }

        fn extensions(&self) -> extensions::StorageDriverExtensions<'_> {
            extensions::StorageDriverExtensions {
                provider_resumable: Some(self),
                ..Default::default()
            }
        }
    }

    #[async_trait]
    impl extensions::ProviderResumableUploadDriver for ProviderResumableDriver {
        fn provider_resumable_upload_capabilities(
            &self,
        ) -> extensions::ProviderResumableUploadCapabilities {
            extensions::ProviderResumableUploadCapabilities {
                provider: "test_provider",
                session_label: "test upload session",
                min_fragment_size: 1,
                default_fragment_size: 1,
                max_fragment_size: 1,
                fragment_alignment: 1,
                max_simple_upload_size: None,
                frontend_direct_upload: true,
                implicit_completion: true,
                abort_supported: true,
                status_query_supported: true,
            }
        }

        async fn create_upload_session(
            &self,
            _path: &str,
        ) -> Result<extensions::ProviderResumableUploadSession> {
            panic!("not used")
        }

        async fn query_upload_session(
            &self,
            _upload_url: &str,
        ) -> Result<extensions::ProviderResumableUploadStatus> {
            panic!("not used")
        }

        async fn abort_upload_session(&self, _upload_url: &str) -> Result<()> {
            panic!("not used")
        }

        async fn upload_session_fragment_reader(
            &self,
            _upload_url: &str,
            _start: u64,
            _total_size: u64,
            _reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
            _fragment_size: i64,
        ) -> Result<extensions::ProviderResumableUploadFragmentOutcome> {
            panic!("not used")
        }
    }

    struct ErrorReader;

    impl AsyncRead for ErrorReader {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            Poll::Ready(Err(io::Error::other("read failed")))
        }
    }

    #[tokio::test]
    async fn get_stream_records_on_reader_completion() {
        let metrics = Arc::new(CapturingMetrics::default());
        let driver = MetricsStorageDriver::new(
            Arc::new(MemoryDriver),
            "asterdrive.storage.local",
            metrics.clone(),
            None,
        );

        let mut reader = driver
            .get_stream("object.bin")
            .await
            .expect("stream should open");

        assert!(metrics.storage_operations.lock().is_empty());
        let mut data = Vec::new();
        reader
            .read_to_end(&mut data)
            .await
            .expect("stream should read");

        assert_eq!(data, b"hello");
        assert_eq!(
            metrics.storage_operations.lock().as_slice(),
            &[("get_stream", "success", "ok")]
        );
    }

    #[tokio::test]
    async fn get_range_records_reader_errors_as_non_storage() {
        let metrics = Arc::new(CapturingMetrics::default());
        let driver = MetricsStorageDriver::new(
            Arc::new(MemoryDriver),
            "asterdrive.storage.local",
            metrics.clone(),
            None,
        );

        let mut reader = driver
            .get_range("object.bin", 0, None)
            .await
            .expect("range should open");

        let mut data = Vec::new();
        reader
            .read_to_end(&mut data)
            .await
            .expect_err("stream read should fail");

        assert_eq!(
            metrics.storage_operations.lock().as_slice(),
            &[("get_range", "failure", "non_storage")]
        );
    }

    #[tokio::test]
    async fn dropping_reader_before_eof_records_aborted() {
        let metrics = Arc::new(CapturingMetrics::default());
        let driver = MetricsStorageDriver::new(
            Arc::new(MemoryDriver),
            "asterdrive.storage.local",
            metrics.clone(),
            None,
        );

        let reader = driver
            .get_stream("object.bin")
            .await
            .expect("stream should open");

        drop(reader);

        assert_eq!(
            metrics.storage_operations.lock().as_slice(),
            &[("get_stream", "failure", "aborted")]
        );
    }

    #[tokio::test]
    async fn unknown_storage_errors_are_tagged_explicitly() {
        let metrics = Arc::new(CapturingMetrics::default());
        let driver = MetricsStorageDriver::new(
            Arc::new(MemoryDriver),
            "asterdrive.storage.local",
            metrics.clone(),
            None,
        );

        driver
            .readiness_check()
            .await
            .expect_err("readiness check should fail");

        assert_eq!(
            metrics.storage_operations.lock().as_slice(),
            &[("readiness_check", "failure", "unknown")]
        );
    }

    #[test]
    fn metrics_wrapper_preserves_provider_resumable_extension() {
        let driver = MetricsStorageDriver::new(
            Arc::new(ProviderResumableDriver),
            "asterdrive.storage.onedrive",
            Arc::new(CapturingMetrics::default()),
            None,
        );

        let provider = driver
            .extensions()
            .provider_resumable
            .expect("metrics wrapper should preserve provider resumable support");
        let capabilities = provider.provider_resumable_upload_capabilities();

        assert_eq!(capabilities.provider, "test_provider");
        assert!(capabilities.frontend_direct_upload);
        assert!(capabilities.abort_supported);
        assert!(capabilities.status_query_supported);

        let extensions = driver.extensions();
        assert!(extensions.provider_resumable.is_some());
    }
}

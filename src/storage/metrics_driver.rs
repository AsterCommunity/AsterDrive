//! StorageDriver metrics decorator.

use super::traits::driver::{BlobMetadata, StorageDriver};
use super::traits::extensions;
use super::traits::multipart::MultipartStorageDriver;
use crate::errors::Result;
use crate::metrics_core::SharedMetricsRecorder;
use crate::types::DriverType;
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
        driver_type: DriverType,
        metrics: SharedMetricsRecorder,
        multipart: Option<Arc<dyn MultipartStorageDriver>>,
    ) -> Self {
        Self {
            inner,
            multipart,
            driver: driver_type.as_str(),
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
        driver_type: DriverType,
        metrics: SharedMetricsRecorder,
    ) -> Self {
        Self {
            inner,
            driver: driver_type.as_str(),
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
        Err(error) => match error.storage_error_kind() {
            Some(kind) => ("failure", kind.as_str()),
            None => ("failure", "non_storage"),
        },
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
    async fn put(&self, path: &str, data: &[u8]) -> Result<String> {
        let started_at = Instant::now();
        let result = self.inner.put(path, data).await;
        self.record("put", &result, started_at);
        result
    }

    async fn get(&self, path: &str) -> Result<Vec<u8>> {
        let started_at = Instant::now();
        let result = self.inner.get(path).await;
        self.record("get", &result, started_at);
        result
    }

    async fn get_stream(&self, path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
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
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
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

    async fn delete(&self, path: &str) -> Result<()> {
        let started_at = Instant::now();
        let result = self.inner.delete(path).await;
        self.record("delete", &result, started_at);
        result
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let started_at = Instant::now();
        let result = self.inner.exists(path).await;
        self.record("exists", &result, started_at);
        result
    }

    async fn metadata(&self, path: &str) -> Result<BlobMetadata> {
        let started_at = Instant::now();
        let result = self.inner.metadata(path).await;
        self.record("metadata", &result, started_at);
        result
    }

    async fn readiness_check(&self) -> Result<()> {
        let started_at = Instant::now();
        let result = self.inner.readiness_check().await;
        self.record("readiness_check", &result, started_at);
        result
    }

    async fn copy_object(&self, src_path: &str, dest_path: &str) -> Result<String> {
        let started_at = Instant::now();
        let result = self.inner.copy_object(src_path, dest_path).await;
        self.record("copy_object", &result, started_at);
        result
    }

    async fn capacity_info(&self) -> Result<extensions::StorageCapacityInfo> {
        let started_at = Instant::now();
        let result = self.inner.capacity_info().await;
        self.record("capacity_info", &result, started_at);
        result
    }

    fn as_presigned(&self) -> Option<&dyn extensions::PresignedStorageDriver> {
        self.inner.as_presigned()
    }

    fn as_list(&self) -> Option<&dyn extensions::ListStorageDriver> {
        self.inner.as_list()
    }

    fn as_stream_upload(&self) -> Option<&dyn extensions::StreamUploadDriver> {
        self.inner.as_stream_upload()
    }

    fn as_local_path(&self) -> Option<&dyn extensions::LocalPathStorageDriver> {
        self.inner.as_local_path()
    }

    fn as_native_thumbnail(&self) -> Option<&dyn extensions::NativeThumbnailStorageDriver> {
        self.inner.as_native_thumbnail()
    }

    fn as_native_media_metadata(
        &self,
    ) -> Option<&dyn extensions::NativeMediaMetadataStorageDriver> {
        self.inner.as_native_media_metadata()
    }

    fn as_native_preview(&self) -> Option<&dyn extensions::NativePreviewStorageDriver> {
        self.inner.as_native_preview()
    }

    fn as_multipart(&self) -> Option<&dyn MultipartStorageDriver> {
        self.multipart.as_deref()
    }
}

#[async_trait]
impl MultipartStorageDriver for MetricsMultipartStorageDriver {
    async fn create_multipart_upload(&self, path: &str) -> Result<String> {
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
    ) -> Result<String> {
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
    ) -> Result<()> {
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
    ) -> Result<String> {
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
    ) -> Result<String> {
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
    ) -> Result<String> {
        let started_at = Instant::now();
        let result = self
            .inner
            .upload_multipart_part_reader(path, upload_id, part_number, reader, size)
            .await;
        self.record("upload_multipart_part_reader", &result, started_at);
        result
    }

    async fn abort_multipart_upload(&self, path: &str, upload_id: &str) -> Result<()> {
        let started_at = Instant::now();
        let result = self.inner.abort_multipart_upload(path, upload_id).await;
        self.record("abort_multipart_upload", &result, started_at);
        result
    }

    async fn list_uploaded_parts(&self, path: &str, upload_id: &str) -> Result<Vec<i32>> {
        let started_at = Instant::now();
        let result = self.inner.list_uploaded_parts(path, upload_id).await;
        self.record("list_uploaded_parts", &result, started_at);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::AsterError;
    use crate::metrics_core::MetricsRecorder;
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
        async fn put(&self, _path: &str, _data: &[u8]) -> Result<String> {
            panic!("not used")
        }

        async fn get(&self, _path: &str) -> Result<Vec<u8>> {
            panic!("not used")
        }

        async fn get_stream(&self, _path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
            Ok(Box::new(io::Cursor::new(Vec::from("hello"))))
        }

        async fn get_range(
            &self,
            _path: &str,
            _offset: u64,
            _length: Option<u64>,
        ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
            Ok(Box::new(ErrorReader))
        }

        async fn delete(&self, _path: &str) -> Result<()> {
            panic!("not used")
        }

        async fn exists(&self, _path: &str) -> Result<bool> {
            panic!("not used")
        }

        async fn metadata(&self, _path: &str) -> Result<BlobMetadata> {
            panic!("not used")
        }

        async fn readiness_check(&self) -> Result<()> {
            Err(AsterError::validation_error("not a storage error"))
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
            DriverType::Local,
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
            DriverType::Local,
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
            DriverType::Local,
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
    async fn non_storage_errors_are_tagged_explicitly() {
        let metrics = Arc::new(CapturingMetrics::default());
        let driver = MetricsStorageDriver::new(
            Arc::new(MemoryDriver),
            DriverType::Local,
            metrics.clone(),
            None,
        );

        driver
            .readiness_check()
            .await
            .expect_err("readiness check should fail");

        assert_eq!(
            metrics.storage_operations.lock().as_slice(),
            &[("readiness_check", "failure", "non_storage")]
        );
    }
}

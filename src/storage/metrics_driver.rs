//! StorageDriver metrics decorator.

use super::driver::{BlobMetadata, StorageDriver};
use crate::errors::Result;
use crate::metrics_core::SharedMetricsRecorder;
use crate::types::DriverType;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::AsyncRead;

pub(crate) struct MetricsStorageDriver {
    inner: Arc<dyn StorageDriver>,
    driver: &'static str,
    metrics: SharedMetricsRecorder,
}

impl MetricsStorageDriver {
    pub(crate) fn new(
        inner: Arc<dyn StorageDriver>,
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
        let (status, kind) = match result {
            Ok(_) => ("success", "ok"),
            Err(error) => (
                "failure",
                error.storage_error_kind().unwrap_or_default().as_str(),
            ),
        };
        self.metrics.record_storage_driver_operation(
            self.driver,
            operation,
            status,
            kind,
            started_at.elapsed().as_secs_f64(),
        );
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
        self.record("get_stream", &result, started_at);
        result
    }

    async fn get_range(
        &self,
        path: &str,
        offset: u64,
        length: Option<u64>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        let started_at = Instant::now();
        let result = self.inner.get_range(path, offset, length).await;
        self.record("get_range", &result, started_at);
        result
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

    fn as_presigned(&self) -> Option<&dyn super::extensions::PresignedStorageDriver> {
        self.inner.as_presigned()
    }

    fn as_list(&self) -> Option<&dyn super::extensions::ListStorageDriver> {
        self.inner.as_list()
    }

    fn as_stream_upload(&self) -> Option<&dyn super::extensions::StreamUploadDriver> {
        self.inner.as_stream_upload()
    }

    fn as_local_path(&self) -> Option<&dyn super::extensions::LocalPathStorageDriver> {
        self.inner.as_local_path()
    }

    fn as_native_thumbnail(&self) -> Option<&dyn super::extensions::NativeThumbnailStorageDriver> {
        self.inner.as_native_thumbnail()
    }

    fn as_multipart(&self) -> Option<&dyn super::multipart::MultipartStorageDriver> {
        self.inner.as_multipart()
    }
}

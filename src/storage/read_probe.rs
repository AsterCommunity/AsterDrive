//! Reusable, read-only probes for deciding whether stored objects can be recovered.
//!
//! This module deliberately does not know about storage policies, database rows,
//! background tasks, or HTTP responses. Product services select representative
//! objects and decide how a probe result affects their workflow.

use std::time::Duration;

use aster_drive_storage::{StorageDriver, StorageError, StorageErrorKind};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Maximum number of object bytes read by the default recoverability probe.
pub const DEFAULT_READ_PROBE_BYTES: u64 = 64 * 1024;

/// Maximum wall-clock duration of one default object probe.
pub const DEFAULT_READ_PROBE_TIMEOUT: Duration = Duration::from_secs(15);

/// Immutable object identity and size expected by a read-only probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageReadProbeTarget<'a> {
    /// Connector-relative object path selected by the caller.
    pub path: &'a str,
    /// Authoritative object size recorded by the caller.
    pub expected_size: u64,
}

/// Resource limits applied to one read-only object probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageReadProbeOptions {
    /// Maximum number of bytes fetched after metadata validation.
    pub max_read_bytes: u64,
    /// Total timeout shared by metadata lookup and the bounded read.
    pub timeout: Duration,
}

impl Default for StorageReadProbeOptions {
    fn default() -> Self {
        Self {
            max_read_bytes: DEFAULT_READ_PROBE_BYTES,
            timeout: DEFAULT_READ_PROBE_TIMEOUT,
        }
    }
}

/// Stable outcome of a read-only storage object probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum StorageReadProbeStatus {
    /// Metadata and the bounded content read matched the expected object.
    Readable,
    /// The driver conclusively reported that the selected object is absent.
    Missing,
    /// Authentication, authorization, configuration, or integrity blocks recovery.
    Blocked,
    /// A temporary or unclassified failure prevented a reliable conclusion.
    Indeterminate,
}

/// Structured evidence returned by a read-only storage object probe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageReadProbeResult {
    /// High-level recoverability classification used by product services.
    pub status: StorageReadProbeStatus,
    /// Stable storage error kind when the probe did not succeed.
    pub error_kind: Option<StorageErrorKind>,
    /// Object size reported by the driver when metadata was available.
    pub observed_size: Option<u64>,
    /// Number of content bytes read after metadata validation.
    pub bytes_read: u64,
    /// Whether repeating the same probe may reasonably produce a different result.
    pub retryable: bool,
    /// Secret-free diagnostic suitable for administrator-facing APIs and task evidence.
    pub diagnostic: Option<String>,
}

impl StorageReadProbeResult {
    /// Creates a successful probe result from validated metadata and content bytes.
    fn readable(observed_size: u64, bytes_read: u64) -> Self {
        Self {
            status: StorageReadProbeStatus::Readable,
            error_kind: None,
            observed_size: Some(observed_size),
            bytes_read,
            retryable: false,
            diagnostic: None,
        }
    }

    /// Classifies one structured storage failure without inspecting its message.
    fn from_storage_error(error: StorageError, observed_size: Option<u64>) -> Self {
        let kind = error.kind();
        let status = match kind {
            StorageErrorKind::NotFound => StorageReadProbeStatus::Missing,
            StorageErrorKind::Auth
            | StorageErrorKind::Misconfigured
            | StorageErrorKind::Permission
            | StorageErrorKind::Precondition => StorageReadProbeStatus::Blocked,
            StorageErrorKind::RateLimited
            | StorageErrorKind::Transient
            | StorageErrorKind::Unsupported
            | StorageErrorKind::Unknown => StorageReadProbeStatus::Indeterminate,
        };
        Self {
            status,
            error_kind: Some(kind),
            observed_size,
            bytes_read: 0,
            retryable: matches!(
                kind,
                StorageErrorKind::RateLimited | StorageErrorKind::Transient
            ),
            diagnostic: Some(error.message().to_string()),
        }
    }

    /// Creates a blocked integrity result when metadata or bounded content is inconsistent.
    fn integrity_failure(message: impl Into<String>, observed_size: Option<u64>) -> Self {
        Self {
            status: StorageReadProbeStatus::Blocked,
            error_kind: Some(StorageErrorKind::Precondition),
            observed_size,
            bytes_read: 0,
            retryable: false,
            diagnostic: Some(message.into()),
        }
    }

    /// Creates an indeterminate timeout result without exposing connector configuration.
    fn timed_out(timeout: Duration) -> Self {
        Self {
            status: StorageReadProbeStatus::Indeterminate,
            error_kind: Some(StorageErrorKind::Transient),
            observed_size: None,
            bytes_read: 0,
            retryable: true,
            diagnostic: Some(format!(
                "storage read probe timed out after {} ms",
                timeout.as_millis()
            )),
        }
    }
}

/// Probes one object using metadata plus a bounded content read and no remote mutation.
///
/// A non-empty object is readable only after exactly `min(expected_size,
/// max_read_bytes)` bytes have been consumed. A zero-byte object is validated by
/// metadata alone because there is no content byte to fetch. The total operation
/// is bounded by `options.timeout` and never calls `put`, `delete`, or copy APIs.
pub async fn probe_storage_object_readability(
    driver: &dyn StorageDriver,
    target: StorageReadProbeTarget<'_>,
    options: StorageReadProbeOptions,
) -> StorageReadProbeResult {
    if options.max_read_bytes == 0 && target.expected_size > 0 {
        return StorageReadProbeResult::integrity_failure(
            "storage read probe max_read_bytes must be greater than zero for non-empty objects",
            None,
        );
    }

    match tokio::time::timeout(options.timeout, async {
        let metadata = driver.metadata(target.path).await?;
        if metadata.size != target.expected_size {
            return Ok(StorageReadProbeResult::integrity_failure(
                format!(
                    "storage object size mismatch: expected {}, observed {}",
                    target.expected_size, metadata.size
                ),
                Some(metadata.size),
            ));
        }
        if target.expected_size == 0 {
            return Ok(StorageReadProbeResult::readable(metadata.size, 0));
        }

        let expected_read = target.expected_size.min(options.max_read_bytes);
        let stream = driver
            .get_range(target.path, 0, Some(expected_read))
            .await?;
        let mut bytes = Vec::new();
        stream
            .take(expected_read)
            .read_to_end(&mut bytes)
            .await
            .map_err(|error| {
                StorageError::new(
                    StorageErrorKind::Transient,
                    format!("read storage probe range: {error}"),
                )
            })?;
        let bytes_read = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if bytes_read != expected_read {
            return Ok(StorageReadProbeResult::integrity_failure(
                format!(
                    "storage object ended during read probe: expected {expected_read} bytes, read {bytes_read}"
                ),
                Some(metadata.size),
            ));
        }
        Ok(StorageReadProbeResult::readable(metadata.size, bytes_read))
    })
    .await
    {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => StorageReadProbeResult::from_storage_error(error, None),
        Err(_) => StorageReadProbeResult::timed_out(options.timeout),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use tokio::io::AsyncRead;

    use super::*;
    use aster_drive_storage::BlobMetadata;

    struct ProbeDriver {
        metadata: Result<BlobMetadata, StorageError>,
        data: Vec<u8>,
        range_error: Option<StorageError>,
        delay: Duration,
        put_calls: AtomicUsize,
        delete_calls: AtomicUsize,
        range_calls: AtomicUsize,
    }

    impl ProbeDriver {
        /// Builds a successful in-memory driver whose metadata matches its content.
        fn readable(data: &[u8]) -> Self {
            Self {
                metadata: Ok(BlobMetadata {
                    size: data.len() as u64,
                    content_type: None,
                }),
                data: data.to_vec(),
                range_error: None,
                delay: Duration::ZERO,
                put_calls: AtomicUsize::new(0),
                delete_calls: AtomicUsize::new(0),
                range_calls: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl StorageDriver for ProbeDriver {
        async fn put(&self, path: &str, _data: &[u8]) -> aster_drive_storage::Result<String> {
            self.put_calls.fetch_add(1, Ordering::SeqCst);
            Ok(path.to_string())
        }

        async fn get(&self, _path: &str) -> aster_drive_storage::Result<Vec<u8>> {
            Ok(self.data.clone())
        }

        async fn get_stream(
            &self,
            _path: &str,
        ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
            Ok(Box::new(std::io::Cursor::new(self.data.clone())))
        }

        async fn get_range(
            &self,
            _path: &str,
            _offset: u64,
            length: Option<u64>,
        ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
            self.range_calls.fetch_add(1, Ordering::SeqCst);
            if !self.delay.is_zero() {
                tokio::time::sleep(self.delay).await;
            }
            if let Some(error) = self.range_error.clone() {
                return Err(error);
            }
            let limit = length.unwrap_or(self.data.len() as u64) as usize;
            Ok(Box::new(std::io::Cursor::new(
                self.data[..self.data.len().min(limit)].to_vec(),
            )))
        }

        async fn delete(&self, _path: &str) -> aster_drive_storage::Result<()> {
            self.delete_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn exists(&self, _path: &str) -> aster_drive_storage::Result<bool> {
            Ok(true)
        }

        async fn metadata(&self, _path: &str) -> aster_drive_storage::Result<BlobMetadata> {
            self.metadata.clone()
        }
    }

    #[tokio::test]
    async fn reads_only_the_configured_prefix_without_mutating_storage() {
        let driver = ProbeDriver::readable(&vec![7; 128 * 1024]);
        let result = probe_storage_object_readability(
            &driver,
            StorageReadProbeTarget {
                path: "blob",
                expected_size: 128 * 1024,
            },
            StorageReadProbeOptions {
                max_read_bytes: 4096,
                timeout: Duration::from_secs(1),
            },
        )
        .await;

        assert_eq!(result.status, StorageReadProbeStatus::Readable);
        assert_eq!(result.bytes_read, 4096);
        assert_eq!(driver.range_calls.load(Ordering::SeqCst), 1);
        assert_eq!(driver.put_calls.load(Ordering::SeqCst), 0);
        assert_eq!(driver.delete_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn zero_byte_object_uses_metadata_without_opening_a_range() {
        let driver = ProbeDriver::readable(&[]);
        let result = probe_storage_object_readability(
            &driver,
            StorageReadProbeTarget {
                path: "empty",
                expected_size: 0,
            },
            StorageReadProbeOptions::default(),
        )
        .await;

        assert_eq!(result.status, StorageReadProbeStatus::Readable);
        assert_eq!(result.bytes_read, 0);
        assert_eq!(driver.range_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn metadata_size_mismatch_is_blocked_without_reading_content() {
        let mut driver = ProbeDriver::readable(b"short");
        driver.metadata = Ok(BlobMetadata {
            size: 99,
            content_type: None,
        });
        let result = probe_storage_object_readability(
            &driver,
            StorageReadProbeTarget {
                path: "blob",
                expected_size: 5,
            },
            StorageReadProbeOptions::default(),
        )
        .await;

        assert_eq!(result.status, StorageReadProbeStatus::Blocked);
        assert_eq!(result.error_kind, Some(StorageErrorKind::Precondition));
        assert_eq!(result.observed_size, Some(99));
        assert_eq!(driver.range_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn structured_errors_map_to_stable_probe_statuses() {
        for (kind, expected) in [
            (StorageErrorKind::NotFound, StorageReadProbeStatus::Missing),
            (
                StorageErrorKind::Permission,
                StorageReadProbeStatus::Blocked,
            ),
            (
                StorageErrorKind::Transient,
                StorageReadProbeStatus::Indeterminate,
            ),
            (
                StorageErrorKind::Unknown,
                StorageReadProbeStatus::Indeterminate,
            ),
        ] {
            let mut driver = ProbeDriver::readable(b"data");
            driver.metadata = Err(StorageError::new(kind, "probe failed"));
            let result = probe_storage_object_readability(
                &driver,
                StorageReadProbeTarget {
                    path: "blob",
                    expected_size: 4,
                },
                StorageReadProbeOptions::default(),
            )
            .await;
            assert_eq!(result.status, expected, "unexpected status for {kind:?}");
            assert_eq!(result.error_kind, Some(kind));
        }
    }

    #[tokio::test]
    async fn total_timeout_is_indeterminate_and_retryable() {
        let mut driver = ProbeDriver::readable(b"data");
        driver.delay = Duration::from_millis(50);
        let result = probe_storage_object_readability(
            &driver,
            StorageReadProbeTarget {
                path: "blob",
                expected_size: 4,
            },
            StorageReadProbeOptions {
                max_read_bytes: 4,
                timeout: Duration::from_millis(5),
            },
        )
        .await;

        assert_eq!(result.status, StorageReadProbeStatus::Indeterminate);
        assert_eq!(result.error_kind, Some(StorageErrorKind::Transient));
        assert!(result.retryable);
    }

    #[tokio::test]
    async fn truncated_range_is_an_integrity_failure() {
        let mut driver = ProbeDriver::readable(b"tiny");
        driver.metadata = Ok(BlobMetadata {
            size: 8,
            content_type: None,
        });
        let result = probe_storage_object_readability(
            &driver,
            StorageReadProbeTarget {
                path: "blob",
                expected_size: 8,
            },
            StorageReadProbeOptions {
                max_read_bytes: 8,
                timeout: Duration::from_secs(1),
            },
        )
        .await;

        assert_eq!(result.status, StorageReadProbeStatus::Blocked);
        assert_eq!(result.error_kind, Some(StorageErrorKind::Precondition));
    }
}

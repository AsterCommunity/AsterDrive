use std::collections::BTreeSet;
use std::sync::Arc;

use bytes::Bytes;
use tokio::io::AsyncReadExt;

use crate::config::operations;
use crate::db::repository::file_repo;
use crate::entities::file_blob;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::storage::{StorageDriver, StorageErrorKind};
use crate::utils::numbers::u64_to_usize;

use crate::services::media::processing::shared::{
    ThumbnailContext, ThumbnailData, requires_server_side_source_limit,
};

const MAX_CACHED_THUMBNAIL_BYTES: u64 = 16 * 1024 * 1024;

pub async fn delete_thumbnail(state: &PrimaryAppState, blob: &file_blob::Model) -> Result<()> {
    let policy = state.policy_snapshot().get_policy_or_err(blob.policy_id)?;
    let driver = state.driver_registry().get_driver(&policy)?;
    delete_thumbnail_with_driver(state, blob, driver).await
}

pub(crate) async fn delete_thumbnail_with_driver(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    driver: Arc<dyn StorageDriver>,
) -> Result<()> {
    let mut paths = BTreeSet::new();
    if let Some(path) = blob.thumbnail_path.as_ref() {
        paths.insert(path.clone());
    }
    for path in crate::services::media::processing::shared::known_thumbnail_cache_paths(
        &blob.hash,
        operations::thumbnail_max_dimension(state.runtime_config()),
    ) {
        paths.insert(path);
    }
    for path in crate::services::media::processing::shared::known_image_preview_cache_paths(
        &blob.hash,
        operations::image_preview_max_dimension(state.runtime_config()),
    ) {
        paths.insert(path);
    }

    for path in paths {
        if driver.exists(&path).await? {
            driver.delete(&path).await?;
        }
    }

    if let Err(error) = file_repo::clear_thumbnail_metadata(state.writer_db(), blob.id).await {
        tracing::warn!(
            blob_id = blob.id,
            "failed to clear thumbnail metadata: {error}"
        );
    }
    Ok(())
}

pub(super) async fn load_thumbnail_if_exists_with_context(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    ctx: &ThumbnailContext,
) -> Result<Option<ThumbnailData>> {
    if requires_server_side_source_limit(&ctx.processor) {
        crate::services::files::thumbnail::ensure_source_size_supported(
            blob,
            operations::thumbnail_max_source_bytes(state.runtime_config()),
        )?;
    }

    let expected_processor = ctx.processor.thumbnail_processor();
    let expected_version = ctx.processor.thumbnail_version(state.runtime_config());
    if (blob.thumbnail_processor.as_deref() != Some(expected_processor)
        || blob.thumbnail_version.as_deref() != Some(expected_version.as_str()))
        && (blob.thumbnail_path.is_some()
            || blob.thumbnail_processor.is_some()
            || blob.thumbnail_version.is_some())
    {
        tracing::debug!(
            blob_id = blob.id,
            processor = ctx.processor.kind().as_str(),
            persisted_thumbnail_processor = blob.thumbnail_processor.as_deref(),
            persisted_thumbnail_version = blob.thumbnail_version.as_deref(),
            expected_thumbnail_processor = expected_processor,
            expected_thumbnail_version = expected_version,
            "clearing stale thumbnail metadata before loading"
        );
        clear_thumbnail_metadata(state, blob).await;
    }

    if blob.thumbnail_processor.as_deref() == Some(expected_processor)
        && blob.thumbnail_version.as_deref() == Some(expected_version.as_str())
        && let Some(path) = blob.thumbnail_path.as_deref()
        && let Some(data) = load_thumbnail_from_path(state, blob, &ctx.driver, path, true).await?
    {
        tracing::debug!(
            blob_id = blob.id,
            processor = ctx.processor.kind().as_str(),
            thumbnail_path = path,
            thumbnail_processor = expected_processor,
            thumbnail_version = expected_version,
            cache_source = "persisted_metadata",
            "thumbnail cache hit"
        );
        return Ok(Some(ThumbnailData {
            data,
            thumbnail_processor: expected_processor.to_string(),
            thumbnail_version: expected_version.clone(),
        }));
    }

    let expected_path = ctx.processor.cache_path(&blob.hash, state.runtime_config());
    if let Some(data) =
        load_thumbnail_from_path(state, blob, &ctx.driver, &expected_path, false).await?
    {
        tracing::debug!(
            blob_id = blob.id,
            processor = ctx.processor.kind().as_str(),
            thumbnail_path = expected_path,
            thumbnail_processor = expected_processor,
            thumbnail_version = expected_version,
            cache_source = "computed_path",
            "thumbnail cache hit"
        );
        persist_thumbnail_metadata(
            state,
            blob,
            &expected_path,
            expected_processor,
            &expected_version,
        )
        .await;
        return Ok(Some(ThumbnailData {
            data,
            thumbnail_processor: expected_processor.to_string(),
            thumbnail_version: expected_version,
        }));
    }

    Ok(None)
}

pub(super) async fn load_thumbnail_from_path(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    driver: &Arc<dyn StorageDriver>,
    path: &str,
    clear_metadata_on_missing: bool,
) -> Result<Option<Bytes>> {
    let thumbnail = read_thumbnail_from_path(blob.id, driver, path).await?;
    if thumbnail.is_none() && clear_metadata_on_missing {
        clear_thumbnail_metadata(state, blob).await;
    }
    Ok(thumbnail)
}

async fn read_thumbnail_from_path(
    blob_id: i64,
    driver: &Arc<dyn StorageDriver>,
    path: &str,
) -> Result<Option<Bytes>> {
    let max_cached_thumbnail_bytes =
        u64_to_usize(MAX_CACHED_THUMBNAIL_BYTES, "cached thumbnail size limit")?;
    let range_limit = MAX_CACHED_THUMBNAIL_BYTES
        .checked_add(1)
        .ok_or_else(|| AsterError::internal_error("cached thumbnail range limit overflow"))?;

    let stream = match driver.get_range(path, 0, Some(range_limit)).await {
        Ok(stream) => stream,
        Err(error) if error.storage_error_kind() == Some(StorageErrorKind::NotFound) => {
            return Ok(None);
        }
        Err(error) => match driver.exists(path).await {
            Ok(false) => return Ok(None),
            Ok(true) => return Err(error),
            Err(exists_error) => {
                tracing::warn!(
                    blob_id,
                    path,
                    "thumbnail range read failed and existence recheck also failed: {exists_error}"
                );
                return Err(error);
            }
        },
    };

    let mut data = Vec::new();
    stream
        .take(range_limit)
        .read_to_end(&mut data)
        .await
        .map_aster_err_ctx(
            "read cached thumbnail range",
            AsterError::storage_driver_error,
        )?;

    if data.len() > max_cached_thumbnail_bytes {
        tracing::warn!(
            blob_id,
            path,
            size = data.len(),
            max_size = MAX_CACHED_THUMBNAIL_BYTES,
            "ignoring oversized cached thumbnail"
        );
        return Ok(None);
    }
    Ok(Some(Bytes::from(data)))
}

async fn clear_thumbnail_metadata(state: &PrimaryAppState, blob: &file_blob::Model) {
    if let Err(error) = file_repo::clear_thumbnail_metadata(state.writer_db(), blob.id).await {
        tracing::warn!(
            blob_id = blob.id,
            "failed to clear stale thumbnail metadata: {error}"
        );
    }
}

pub(super) async fn persist_thumbnail_metadata(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    path: &str,
    processor: &str,
    version: &str,
) {
    if let Err(error) =
        file_repo::set_thumbnail_metadata(state.writer_db(), blob.id, path, processor, version)
            .await
    {
        tracing::warn!(
            blob_id = blob.id,
            path,
            "failed to persist thumbnail metadata: {error}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_CACHED_THUMBNAIL_BYTES, read_thumbnail_from_path};
    use crate::errors::{AsterError, Result};
    use crate::storage::error::storage_driver_error;
    use crate::storage::{BlobMetadata, StorageDriver, StorageErrorKind};
    use crate::utils::numbers::u64_to_usize;
    use async_trait::async_trait;
    use bytes::Bytes;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tokio::io::{AsyncRead, AsyncReadExt};

    struct ThumbnailCacheDriver {
        range_error_kind: Option<StorageErrorKind>,
        data_len: usize,
        expected_offset: u64,
        expected_length: Option<u64>,
        exists_result: std::result::Result<bool, StorageErrorKind>,
        exists_calls: AtomicUsize,
        get_calls: AtomicUsize,
        range_calls: AtomicUsize,
        metadata_calls: AtomicUsize,
    }

    impl ThumbnailCacheDriver {
        fn new(data_len: usize) -> Self {
            Self {
                range_error_kind: None,
                data_len,
                expected_offset: 0,
                expected_length: Some(
                    MAX_CACHED_THUMBNAIL_BYTES
                        .checked_add(1)
                        .expect("thumbnail range limit should not overflow"),
                ),
                exists_result: Ok(false),
                exists_calls: AtomicUsize::new(0),
                get_calls: AtomicUsize::new(0),
                range_calls: AtomicUsize::new(0),
                metadata_calls: AtomicUsize::new(0),
            }
        }

        fn not_found() -> Self {
            Self {
                range_error_kind: Some(StorageErrorKind::NotFound),
                ..Self::new(0)
            }
        }

        fn with_range_error(kind: StorageErrorKind, exists_result: bool) -> Self {
            Self {
                range_error_kind: Some(kind),
                exists_result: Ok(exists_result),
                ..Self::new(0)
            }
        }

        fn with_range_error_and_exists_error(
            kind: StorageErrorKind,
            exists_error_kind: StorageErrorKind,
        ) -> Self {
            Self {
                range_error_kind: Some(kind),
                exists_result: Err(exists_error_kind),
                ..Self::new(0)
            }
        }

        fn with_expected_range(
            data_len: usize,
            expected_offset: u64,
            expected_length: Option<u64>,
        ) -> Self {
            Self {
                expected_offset,
                expected_length,
                ..Self::new(data_len)
            }
        }
    }

    fn unsupported_test_call(method: &str) -> AsterError {
        storage_driver_error(
            StorageErrorKind::Unsupported,
            format!("thumbnail cache test driver does not support {method}"),
        )
    }

    #[async_trait]
    impl StorageDriver for ThumbnailCacheDriver {
        async fn put(&self, _path: &str, _data: &[u8]) -> Result<String> {
            Err(unsupported_test_call("put"))
        }

        async fn get(&self, _path: &str) -> Result<Vec<u8>> {
            self.get_calls.fetch_add(1, Ordering::SeqCst);
            Ok(vec![b'x'; self.data_len])
        }

        async fn get_stream(&self, _path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
            Err(unsupported_test_call("get_stream"))
        }

        async fn get_range(
            &self,
            _path: &str,
            offset: u64,
            length: Option<u64>,
        ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
            self.range_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(kind) = self.range_error_kind {
                return Err(storage_driver_error(kind, "thumbnail range read failed"));
            }
            assert_eq!(offset, self.expected_offset);
            assert_eq!(length, self.expected_length);
            let offset = u64_to_usize(offset, "test thumbnail range offset")
                .expect("thumbnail range offset should fit usize");
            let length = length
                .map(|value| {
                    u64_to_usize(value, "test thumbnail range length")
                        .expect("thumbnail range length should fit usize")
                })
                .unwrap_or(self.data_len);
            let end = offset.saturating_add(length).min(self.data_len);
            let returned_len = end.saturating_sub(offset);
            Ok(Box::new(std::io::Cursor::new(vec![b'x'; returned_len])))
        }

        async fn delete(&self, _path: &str) -> Result<()> {
            Err(unsupported_test_call("delete"))
        }

        async fn exists(&self, _path: &str) -> Result<bool> {
            self.exists_calls.fetch_add(1, Ordering::SeqCst);
            self.exists_result
                .map_err(|kind| storage_driver_error(kind, "thumbnail existence check failed"))
        }

        async fn metadata(&self, _path: &str) -> Result<BlobMetadata> {
            self.metadata_calls.fetch_add(1, Ordering::SeqCst);
            Err(unsupported_test_call("metadata"))
        }
    }

    async fn read_cache_with_driver(driver: &Arc<ThumbnailCacheDriver>) -> Option<Bytes> {
        read_thumbnail_from_path(1, &(driver.clone() as Arc<dyn StorageDriver>), "thumb.webp")
            .await
            .expect("thumbnail cache read should not fail")
    }

    fn assert_no_metadata_or_full_get(driver: &ThumbnailCacheDriver) {
        assert_eq!(driver.metadata_calls.load(Ordering::SeqCst), 0);
        assert_eq!(driver.get_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn read_thumbnail_from_path_treats_not_found_range_as_miss_without_exists_probe() {
        let driver = Arc::new(ThumbnailCacheDriver::not_found());

        let loaded = read_cache_with_driver(&driver).await;

        assert!(loaded.is_none());
        assert_eq!(driver.exists_calls.load(Ordering::SeqCst), 0);
        assert_no_metadata_or_full_get(&driver);
        assert_eq!(driver.range_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn read_thumbnail_from_path_treats_range_error_as_miss_when_exists_probe_is_false() {
        let driver = Arc::new(ThumbnailCacheDriver::with_range_error(
            StorageErrorKind::Transient,
            false,
        ));

        let loaded = read_cache_with_driver(&driver).await;

        assert!(loaded.is_none());
        assert_eq!(driver.exists_calls.load(Ordering::SeqCst), 1);
        assert_no_metadata_or_full_get(&driver);
        assert_eq!(driver.range_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn read_thumbnail_from_path_returns_range_error_when_exists_probe_is_true() {
        let driver = Arc::new(ThumbnailCacheDriver::with_range_error(
            StorageErrorKind::Transient,
            true,
        ));

        let error =
            read_thumbnail_from_path(1, &(driver.clone() as Arc<dyn StorageDriver>), "thumb.webp")
                .await
                .expect_err("range error should be preserved when object still exists");

        assert_eq!(
            error.storage_error_kind(),
            Some(StorageErrorKind::Transient)
        );
        assert_eq!(driver.exists_calls.load(Ordering::SeqCst), 1);
        assert_no_metadata_or_full_get(&driver);
        assert_eq!(driver.range_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn read_thumbnail_from_path_preserves_range_error_when_exists_probe_fails() {
        let driver = Arc::new(ThumbnailCacheDriver::with_range_error_and_exists_error(
            StorageErrorKind::Transient,
            StorageErrorKind::Permission,
        ));

        let error =
            read_thumbnail_from_path(1, &(driver.clone() as Arc<dyn StorageDriver>), "thumb.webp")
                .await
                .expect_err("range error should be preserved when existence probe fails");

        assert_eq!(
            error.storage_error_kind(),
            Some(StorageErrorKind::Transient)
        );
        assert_eq!(driver.exists_calls.load(Ordering::SeqCst), 1);
        assert_no_metadata_or_full_get(&driver);
        assert_eq!(driver.range_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn read_thumbnail_from_path_accepts_empty_cache_entry() {
        let driver = Arc::new(ThumbnailCacheDriver::new(0));

        let loaded = read_cache_with_driver(&driver).await;

        assert_eq!(loaded.expect("cache hit expected").len(), 0);
        assert_no_metadata_or_full_get(&driver);
        assert_eq!(driver.range_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn read_thumbnail_from_path_accepts_cache_entry_below_size_limit() {
        let max_size = u64_to_usize(MAX_CACHED_THUMBNAIL_BYTES, "test thumbnail limit")
            .expect("thumbnail limit should fit usize");
        let driver = Arc::new(ThumbnailCacheDriver::new(max_size - 1));

        let loaded = read_cache_with_driver(&driver).await;

        assert_eq!(loaded.expect("cache hit expected").len(), max_size - 1);
        assert_no_metadata_or_full_get(&driver);
        assert_eq!(driver.range_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn read_thumbnail_from_path_accepts_cache_entry_at_size_limit() {
        let max_size = u64_to_usize(MAX_CACHED_THUMBNAIL_BYTES, "test thumbnail limit")
            .expect("thumbnail limit should fit usize");
        let driver = Arc::new(ThumbnailCacheDriver::new(max_size));

        let loaded = read_cache_with_driver(&driver).await;

        assert_eq!(loaded.expect("cache hit expected").len(), max_size);
        assert_no_metadata_or_full_get(&driver);
        assert_eq!(driver.range_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn read_thumbnail_from_path_rejects_cache_entry_above_read_limit() {
        let max_size = u64_to_usize(MAX_CACHED_THUMBNAIL_BYTES, "test thumbnail limit")
            .expect("thumbnail limit should fit usize");
        let driver = Arc::new(ThumbnailCacheDriver::new(max_size + 1));

        let loaded = read_cache_with_driver(&driver).await;

        assert!(loaded.is_none());
        assert_no_metadata_or_full_get(&driver);
        assert_eq!(driver.range_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn thumbnail_cache_driver_range_respects_offset_and_length_boundaries() {
        let cases = [
            (10, 0, Some(0), 0),
            (10, 0, Some(4), 4),
            (10, 6, Some(4), 4),
            (10, 6, Some(99), 4),
            (10, 10, Some(4), 0),
            (10, 12, Some(4), 0),
            (10, 3, None, 7),
        ];

        for (data_len, offset, length, expected_len) in cases {
            let driver = ThumbnailCacheDriver::with_expected_range(data_len, offset, length);
            let mut stream = driver
                .get_range("thumb.webp", offset, length)
                .await
                .expect("range read should succeed");
            let mut data = Vec::new();
            stream
                .read_to_end(&mut data)
                .await
                .expect("range stream should read");

            assert_eq!(data.len(), expected_len);
            assert_eq!(driver.range_calls.load(Ordering::SeqCst), 1);
            assert_eq!(driver.metadata_calls.load(Ordering::SeqCst), 0);
        }
    }
}

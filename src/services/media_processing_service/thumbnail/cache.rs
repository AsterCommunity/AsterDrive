use std::collections::BTreeSet;
use std::sync::Arc;

use crate::config::operations;
use crate::db::repository::file_repo;
use crate::entities::file_blob;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::storage::{StorageDriver, StorageErrorKind};

use crate::services::media_processing_service::shared::{
    ThumbnailContext, ThumbnailData, requires_server_side_source_limit,
};

pub async fn delete_thumbnail(state: &PrimaryAppState, blob: &file_blob::Model) -> Result<()> {
    let policy = state.policy_snapshot.get_policy_or_err(blob.policy_id)?;
    let driver = state.driver_registry.get_driver(&policy)?;

    let mut paths = BTreeSet::new();
    if let Some(path) = blob.thumbnail_path.as_ref() {
        paths.insert(path.clone());
    }
    for path in
        crate::services::media_processing_service::shared::known_thumbnail_cache_paths(&blob.hash)
    {
        paths.insert(path);
    }
    for path in crate::services::media_processing_service::shared::known_image_preview_cache_paths(
        &blob.hash,
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
        crate::services::thumbnail_service::ensure_source_size_supported(
            blob,
            operations::thumbnail_max_source_bytes(&state.runtime_config),
        )?;
    }

    let expected_processor = ctx.processor.thumbnail_processor();
    let expected_version = ctx.processor.thumbnail_version();
    if (blob.thumbnail_processor.as_deref() != Some(expected_processor)
        || blob.thumbnail_version.as_deref() != Some(expected_version))
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
        && blob.thumbnail_version.as_deref() == Some(expected_version)
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
            thumbnail_version: expected_version.to_string(),
        }));
    }

    let expected_path = ctx.processor.cache_path(&blob.hash);
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
            expected_version,
        )
        .await;
        return Ok(Some(ThumbnailData {
            data,
            thumbnail_processor: expected_processor.to_string(),
            thumbnail_version: expected_version.to_string(),
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
) -> Result<Option<Vec<u8>>> {
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
) -> Result<Option<Vec<u8>>> {
    match driver.get(path).await {
        Ok(data) => Ok(Some(data)),
        Err(error) if error.storage_error_kind() == Some(StorageErrorKind::NotFound) => Ok(None),
        Err(error) => match driver.exists(path).await {
            Ok(false) => Ok(None),
            Ok(true) => Err(error),
            Err(exists_error) => {
                tracing::warn!(
                    blob_id,
                    path,
                    "thumbnail get failed and existence recheck also failed: {exists_error}"
                );
                Err(error)
            }
        },
    }
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
    use super::read_thumbnail_from_path;
    use crate::errors::Result;
    use crate::storage::StorageErrorKind;
    use crate::storage::error::storage_driver_error;
    use crate::storage::{BlobMetadata, StorageDriver};
    use async_trait::async_trait;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tokio::io::AsyncRead;

    struct MissingThumbnailDriver {
        exists_calls: AtomicUsize,
    }

    #[async_trait]
    impl StorageDriver for MissingThumbnailDriver {
        async fn put(&self, _path: &str, _data: &[u8]) -> Result<String> {
            unreachable!()
        }

        async fn get(&self, _path: &str) -> Result<Vec<u8>> {
            Err(storage_driver_error(
                StorageErrorKind::NotFound,
                "thumbnail not found",
            ))
        }

        async fn get_stream(&self, _path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
            unreachable!()
        }

        async fn delete(&self, _path: &str) -> Result<()> {
            unreachable!()
        }

        async fn exists(&self, _path: &str) -> Result<bool> {
            self.exists_calls.fetch_add(1, Ordering::SeqCst);
            Ok(false)
        }

        async fn metadata(&self, _path: &str) -> Result<BlobMetadata> {
            unreachable!()
        }
    }

    #[tokio::test]
    async fn read_thumbnail_from_path_treats_not_found_get_as_miss_without_exists_probe() {
        let driver = Arc::new(MissingThumbnailDriver {
            exists_calls: AtomicUsize::new(0),
        });

        let loaded =
            read_thumbnail_from_path(1, &(driver.clone() as Arc<dyn StorageDriver>), "thumb.webp")
                .await
                .expect("not found should be a cache miss");

        assert!(loaded.is_none());
        assert_eq!(driver.exists_calls.load(Ordering::SeqCst), 0);
    }
}

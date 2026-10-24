use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use aster_drive_metrics::MetricsRecorder;
use aster_drive_storage::{
    StorageDriver, StorageErrorKind, StreamUploadAttempt, StreamUploadDriver,
};
use bytes::Bytes;
use futures::{Stream, StreamExt};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use super::{StreamUploadMetricsGuard, cleanup_stream_upload_attempt};
use crate::errors::{AsterError, Result};

const RELAY_BUFFER_SIZE: usize = 64 * 1024;
const STREAM_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(15 * 60);

pub(crate) type FollowerUploadBody = Pin<Box<dyn Stream<Item = Result<Bytes>> + 'static>>;

pub(crate) struct FollowerObjectWriteRequest<'a> {
    pub binding_id: i64,
    pub object_key: &'a str,
    pub storage_path: &'a str,
    pub content_length: i64,
    pub expected_sha256: Option<&'a str>,
}

#[derive(Clone, Copy)]
struct RelayStageContext {
    stage_abort_join: &'static str,
    timeout_abort_join: &'static str,
    timeout_error: &'static str,
    relay_join_error: &'static str,
}

async fn stage_with_relay<T, F>(
    stream_driver: &dyn StreamUploadDriver,
    attempt: &StreamUploadAttempt,
    reader: tokio::io::DuplexStream,
    relay: F,
    context: RelayStageContext,
    timeout: Duration,
) -> Result<T>
where
    T: 'static,
    F: Future<Output = Result<T>> + 'static,
{
    tokio::task::LocalSet::new()
        .run_until(async move {
            let relay_task = tokio::task::spawn_local(relay);
            let stage = stream_driver.stage_attempt(attempt, Box::new(reader));
            match tokio::time::timeout(timeout, stage).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    relay_task.abort();
                    match relay_task.await {
                        Ok(Err(relay_error)) => return Err(relay_error),
                        Ok(Ok(_)) => {}
                        Err(join_error) if !join_error.is_cancelled() => {
                            tracing::warn!(
                                context = context.stage_abort_join,
                                "failed to join aborted relay task: {join_error}"
                            );
                        }
                        Err(_) => {}
                    }
                    return Err(error.into());
                }
                Err(_) => {
                    relay_task.abort();
                    log_aborted_relay_join(relay_task, context.timeout_abort_join).await;
                    return Err(AsterError::storage_driver_error(context.timeout_error));
                }
            }
            relay_task.await.map_err(|error| {
                AsterError::storage_driver_error(format!("{}: {error}", context.relay_join_error))
            })?
        })
        .await
}

pub(crate) async fn write_follower_object(
    driver: Arc<dyn StorageDriver>,
    metrics: &dyn MetricsRecorder,
    request: FollowerObjectWriteRequest<'_>,
    mut payload: FollowerUploadBody,
) -> Result<String> {
    let FollowerObjectWriteRequest {
        binding_id,
        object_key,
        storage_path,
        content_length,
        expected_sha256,
    } = request;
    let stream_driver = driver.extensions().stream_upload.ok_or_else(|| {
        crate::errors::storage_driver_error(
            StorageErrorKind::Unsupported,
            "ingress target does not support stream upload",
        )
    })?;
    // Default provider staging may write its `storage_path` immediately. Give
    // every attempt a unique unpublished key so a bad digest can never replace
    // the existing object; only copy it to the final key after verification.
    let attempt_path = format!(
        "{storage_path}.aster-attempt-{}",
        aster_forge_utils::id::new_uuid()
    );
    let attempt = StreamUploadAttempt::new(&attempt_path, content_length)?;
    let _attempt_metrics = StreamUploadMetricsGuard::new(metrics, content_length);
    let (writer, reader) = tokio::io::duplex(RELAY_BUFFER_SIZE);
    let relay = async move {
        let mut writer = writer;
        let mut hasher = Sha256::new();
        while let Some(chunk) = payload.next().await {
            let chunk = chunk?;
            hasher.update(&chunk);
            writer.write_all(&chunk).await.map_err(|error| {
                AsterError::storage_driver_error(format!("relay upload payload: {error}"))
            })?;
        }
        writer.shutdown().await.map_err(|error| {
            AsterError::storage_driver_error(format!("shutdown relay upload payload: {error}"))
        })?;
        Ok::<String, AsterError>(format!("\"{}\"", hex::encode(hasher.finalize())))
    };
    let stage_result = stage_with_relay(
        stream_driver,
        &attempt,
        reader,
        relay,
        RelayStageContext {
            stage_abort_join: "follower relay after stage error",
            timeout_abort_join: "timed out follower relay",
            timeout_error: "stream upload attempt timed out",
            relay_join_error: "relay upload task failed",
        },
        STREAM_ATTEMPT_TIMEOUT,
    )
    .await;

    let etag = match stage_result {
        Ok(etag) => etag,
        Err(error) => {
            cleanup_stream_upload_attempt(stream_driver, &attempt, metrics, binding_id, object_key)
                .await;
            cleanup_unpublished_attempt(driver.as_ref(), &attempt_path, binding_id, object_key)
                .await;
            return Err(error);
        }
    };

    let actual_sha256 = etag.trim_matches('"');
    if expected_sha256.is_some_and(|expected| expected != actual_sha256) {
        tracing::warn!(
            binding_id,
            object_key,
            expected = ?expected_sha256,
            actual = actual_sha256,
            "follower object digest mismatch; rejecting upload before publication"
        );
        cleanup_stream_upload_attempt(stream_driver, &attempt, metrics, binding_id, object_key)
            .await;
        cleanup_unpublished_attempt(driver.as_ref(), &attempt_path, binding_id, object_key).await;
        return Err(AsterError::validation_error("object body digest mismatch"));
    }

    if let Err(error) = stream_driver.commit_attempt(&attempt).await {
        cleanup_stream_upload_attempt(stream_driver, &attempt, metrics, binding_id, object_key)
            .await;
        cleanup_unpublished_attempt(driver.as_ref(), &attempt_path, binding_id, object_key).await;
        return Err(error.into());
    }
    if let Err(error) = publish_staged_object(driver.as_ref(), &attempt_path, storage_path).await {
        cleanup_unpublished_attempt(driver.as_ref(), &attempt_path, binding_id, object_key).await;
        return Err(error.into());
    }
    cleanup_unpublished_attempt(driver.as_ref(), &attempt_path, binding_id, object_key).await;
    metrics.record_stream_upload_attempt("commit", "success");
    Ok(etag)
}

async fn cleanup_unpublished_attempt(
    driver: &dyn StorageDriver,
    attempt_path: &str,
    binding_id: i64,
    object_key: &str,
) {
    if let Err(error) = driver.delete(attempt_path).await
        && error.kind() != StorageErrorKind::NotFound
    {
        tracing::warn!(
            binding_id,
            object_key,
            attempt_path,
            "failed to cleanup unpublished follower upload attempt: {error}"
        );
    }
}

pub(crate) async fn compose_follower_objects(
    driver: Arc<dyn StorageDriver>,
    metrics: &dyn MetricsRecorder,
    binding_id: i64,
    target_key: &str,
    target_storage_path: &str,
    part_storage_paths: Vec<String>,
    expected_size: i64,
) -> Result<u64> {
    let stream_driver = driver.extensions().stream_upload.ok_or_else(|| {
        crate::errors::storage_driver_error(
            StorageErrorKind::Unsupported,
            "ingress target does not support stream upload",
        )
    })?;
    let expected_size_u64 =
        aster_forge_utils::numbers::i64_to_u64(expected_size, "compose expected_size")?;
    let attempt_path = format!(
        "{target_storage_path}.aster-attempt-{}",
        aster_forge_utils::id::new_uuid()
    );
    let attempt = StreamUploadAttempt::new(&attempt_path, expected_size)?;
    let _attempt_metrics = StreamUploadMetricsGuard::new(metrics, expected_size);
    let read_driver = Arc::clone(&driver);
    let cleanup_part_storage_paths = part_storage_paths.clone();
    let (writer, reader) = tokio::io::duplex(RELAY_BUFFER_SIZE);

    let relay = async move {
        let mut writer = writer;
        let mut bytes_written = 0_u64;
        for source_path in part_storage_paths {
            let mut stream = read_driver.get_stream(&source_path).await?;
            let copied = tokio::io::copy(&mut stream, &mut writer)
                .await
                .map_err(|error| {
                    AsterError::storage_driver_error(format!(
                        "relay composed object stream: {error}"
                    ))
                })?;
            bytes_written = bytes_written.checked_add(copied).ok_or_else(|| {
                AsterError::storage_driver_error("compose bytes written overflow")
            })?;
        }
        writer.shutdown().await.map_err(|error| {
            AsterError::storage_driver_error(format!("shutdown compose stream: {error}"))
        })?;
        Ok::<u64, AsterError>(bytes_written)
    };
    let stage_result = stage_with_relay(
        stream_driver,
        &attempt,
        reader,
        relay,
        RelayStageContext {
            stage_abort_join: "compose relay after stage error",
            timeout_abort_join: "timed out compose relay",
            timeout_error: "compose stream upload attempt timed out",
            relay_join_error: "compose relay task failed",
        },
        STREAM_ATTEMPT_TIMEOUT,
    )
    .await;

    let bytes_written = match stage_result {
        Ok(bytes_written) if bytes_written == expected_size_u64 => bytes_written,
        Ok(bytes_written) => {
            cleanup_stream_upload_attempt(stream_driver, &attempt, metrics, binding_id, target_key)
                .await;
            cleanup_unpublished_attempt(driver.as_ref(), &attempt_path, binding_id, target_key)
                .await;
            return Err(AsterError::storage_driver_error(format!(
                "compose size mismatch: expected {expected_size_u64} bytes, got {bytes_written}"
            )));
        }
        Err(error) => {
            cleanup_stream_upload_attempt(stream_driver, &attempt, metrics, binding_id, target_key)
                .await;
            cleanup_unpublished_attempt(driver.as_ref(), &attempt_path, binding_id, target_key)
                .await;
            return Err(error);
        }
    };

    if let Err(error) = stream_driver.commit_attempt(&attempt).await {
        cleanup_stream_upload_attempt(stream_driver, &attempt, metrics, binding_id, target_key)
            .await;
        cleanup_unpublished_attempt(driver.as_ref(), &attempt_path, binding_id, target_key).await;
        return Err(error.into());
    }
    if let Err(error) =
        publish_staged_object(driver.as_ref(), &attempt_path, target_storage_path).await
    {
        cleanup_unpublished_attempt(driver.as_ref(), &attempt_path, binding_id, target_key).await;
        return Err(error.into());
    }
    cleanup_unpublished_attempt(driver.as_ref(), &attempt_path, binding_id, target_key).await;
    metrics.record_stream_upload_attempt("commit", "success");

    for storage_path in cleanup_part_storage_paths {
        if let Err(error) = driver.delete(&storage_path).await {
            tracing::warn!(storage_path, "failed to cleanup composed part: {error}");
        }
    }
    Ok(bytes_written)
}

/// Publishes only after digest/size verification. Native rename avoids a second
/// full-object transfer for Local/SFTP; generic copy remains outside the
/// staging timeout and is the fallback for other drivers. The fallback is not
/// protected by `STREAM_ATTEMPT_TIMEOUT`.
async fn publish_staged_object(
    driver: &dyn StorageDriver,
    staging_path: &str,
    storage_path: &str,
) -> aster_drive_storage::Result<String> {
    if let Some(publisher) = driver.extensions().native_publish {
        publisher.publish_staged(staging_path, storage_path).await
    } else {
        driver.copy_object(staging_path, storage_path).await
    }
}

async fn log_aborted_relay_join<T>(task: tokio::task::JoinHandle<T>, context: &'static str) {
    if let Err(error) = task.await
        && !error.is_cancelled()
    {
        tracing::warn!(context, "failed to join aborted relay task: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FollowerObjectWriteRequest, FollowerUploadBody, RelayStageContext,
        compose_follower_objects, publish_staged_object, stage_with_relay, write_follower_object,
    };
    use crate::errors::AsterError;
    use crate::storage::drivers::local::LocalDriver;
    use aster_drive_metrics::NoopMetrics;
    use aster_drive_storage::{
        BlobMetadata, NativePublishStorageDriver, StorageDriver, StorageDriverExtensions,
        StorageErrorKind, StreamUploadDriver, storage_driver_error,
    };
    use bytes::Bytes;
    use sha2::Digest;
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    fn test_root(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "aster-follower-stream-{label}-{}-{}",
            std::process::id(),
            uuid::Uuid::now_v7()
        ))
    }

    fn assert_no_attempt_files(root: &Path) {
        let entries = std::fs::read_dir(root)
            .unwrap()
            .map(|entry| entry.unwrap())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(
            entries.iter().all(|name| !name.contains(".aster-attempt-")),
            "attempt files remain: {entries:?}"
        );
    }

    struct PublishProbe {
        native: bool,
        error: Option<StorageErrorKind>,
        publishes: AtomicUsize,
        copies: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl StorageDriver for PublishProbe {
        async fn put(&self, _: &str, _: &[u8]) -> aster_drive_storage::Result<String> {
            unreachable!("publish must not read or upload object contents")
        }
        async fn get(&self, _: &str) -> aster_drive_storage::Result<Vec<u8>> {
            unreachable!("publish must not read or upload object contents")
        }
        async fn get_stream(
            &self,
            _: &str,
        ) -> aster_drive_storage::Result<Box<dyn tokio::io::AsyncRead + Unpin + Send>> {
            unreachable!("publish must not read or upload object contents")
        }
        async fn delete(&self, _: &str) -> aster_drive_storage::Result<()> {
            unreachable!("publication dispatch must not delete the final object")
        }
        async fn exists(&self, _: &str) -> aster_drive_storage::Result<bool> {
            unreachable!()
        }
        async fn metadata(&self, _: &str) -> aster_drive_storage::Result<BlobMetadata> {
            unreachable!()
        }
        async fn copy_object(
            &self,
            source: &str,
            target: &str,
        ) -> aster_drive_storage::Result<String> {
            assert_eq!((source, target), ("staged", "target"));
            self.copies.fetch_add(1, Ordering::SeqCst);
            Ok(target.to_owned())
        }
        fn extensions(&self) -> StorageDriverExtensions<'_> {
            StorageDriverExtensions {
                native_publish: self.native.then_some(self),
                ..Default::default()
            }
        }
    }

    #[async_trait::async_trait]
    impl NativePublishStorageDriver for PublishProbe {
        async fn publish_staged(
            &self,
            source: &str,
            target: &str,
        ) -> aster_drive_storage::Result<String> {
            assert_eq!((source, target), ("staged", "target"));
            self.publishes.fetch_add(1, Ordering::SeqCst);
            match self.error {
                Some(kind) => Err(storage_driver_error(kind, "injected publish failure")),
                None => Ok(target.to_owned()),
            }
        }
    }

    #[tokio::test]
    async fn publication_prefers_native_and_only_copies_when_capability_is_absent() {
        for native in [true, false] {
            let driver = PublishProbe {
                native,
                error: None,
                publishes: AtomicUsize::new(0),
                copies: AtomicUsize::new(0),
            };
            assert_eq!(
                publish_staged_object(&driver, "staged", "target")
                    .await
                    .unwrap(),
                "target"
            );
            assert_eq!(driver.publishes.load(Ordering::SeqCst), usize::from(native));
            assert_eq!(driver.copies.load(Ordering::SeqCst), usize::from(!native));
        }
    }

    #[tokio::test]
    async fn native_publication_errors_never_retry_with_copy() {
        for kind in [
            StorageErrorKind::Unsupported,
            StorageErrorKind::Transient,
            StorageErrorKind::Permission,
        ] {
            let driver = PublishProbe {
                native: true,
                error: Some(kind),
                publishes: AtomicUsize::new(0),
                copies: AtomicUsize::new(0),
            };
            assert_eq!(
                publish_staged_object(&driver, "staged", "target")
                    .await
                    .unwrap_err()
                    .kind(),
                kind
            );
            assert_eq!(driver.publishes.load(Ordering::SeqCst), 1);
            assert_eq!(driver.copies.load(Ordering::SeqCst), 0);
        }
    }

    #[test]
    fn attempt_file_assertion_detects_staging_and_committed_attempt_names() {
        let root = test_root("attempt-assertion");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("object.bin"), b"original").unwrap();
        assert_no_attempt_files(&root);
        for name in [".aster-attempt-test.tmp", "object.bin.aster-attempt-test"] {
            let path = root.join(name);
            std::fs::write(&path, b"attempt").unwrap();
            assert!(std::panic::catch_unwind(|| assert_no_attempt_files(&root)).is_err());
            std::fs::remove_file(path).unwrap();
        }
        assert_no_attempt_files(&root);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn successful_local_publish_renames_verified_attempt() {
        let root = test_root("successful-publish");
        tokio::fs::create_dir_all(&root).await.unwrap();
        let driver = Arc::new(LocalDriver::new(root.to_str().unwrap()).unwrap());
        let payload: FollowerUploadBody = Box::pin(futures::stream::once(async {
            Ok(Bytes::from_static(b"published"))
        }));

        write_follower_object(
            driver.clone(),
            &NoopMetrics::new(),
            FollowerObjectWriteRequest {
                binding_id: 7,
                object_key: "object.bin",
                storage_path: "object.bin",
                content_length: 9,
                expected_sha256: Some(&hex::encode(sha2::Sha256::digest(b"published"))),
            },
            payload,
        )
        .await
        .unwrap();

        assert_eq!(driver.get("object.bin").await.unwrap(), b"published");
        assert_no_attempt_files(&root);
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn compose_uses_native_local_publish_and_cleans_parts() {
        let root = test_root("compose-publish");
        tokio::fs::create_dir_all(&root).await.unwrap();
        let driver = Arc::new(LocalDriver::new(root.to_str().unwrap()).unwrap());
        driver.put("part-a", b"hello ").await.unwrap();
        driver.put("part-b", b"world").await.unwrap();

        let bytes = compose_follower_objects(
            driver.clone(),
            &NoopMetrics::new(),
            7,
            "object.bin",
            "object.bin",
            vec!["part-a".to_owned(), "part-b".to_owned()],
            11,
        )
        .await
        .unwrap();

        assert_eq!(bytes, 11);
        assert_eq!(driver.get("object.bin").await.unwrap(), b"hello world");
        assert!(!driver.exists("part-a").await.unwrap());
        assert!(!driver.exists("part-b").await.unwrap());
        assert_no_attempt_files(&root);
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn native_local_publish_moves_empty_and_existing_objects_without_copy() {
        let root = test_root("native-rename");
        tokio::fs::create_dir_all(&root).await.unwrap();
        let driver = LocalDriver::new(root.to_str().unwrap()).unwrap();
        for bytes in [b"".as_slice(), b"replacement".as_slice()] {
            driver.put("staged", bytes).await.unwrap();
            driver.put("target", b"original").await.unwrap();
            #[cfg(unix)]
            let inode = {
                use std::os::unix::fs::MetadataExt;
                tokio::fs::metadata(root.join("staged"))
                    .await
                    .unwrap()
                    .ino()
            };
            publish_staged_object(&driver, "staged", "target")
                .await
                .unwrap();
            assert_eq!(driver.get("target").await.unwrap(), bytes);
            assert!(!driver.exists("staged").await.unwrap());
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                assert_eq!(
                    tokio::fs::metadata(root.join("target"))
                        .await
                        .unwrap()
                        .ino(),
                    inode
                );
            }
        }
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn compose_size_mismatch_preserves_original_and_parts_and_cleans_attempt() {
        let root = test_root("compose-size");
        tokio::fs::create_dir_all(&root).await.unwrap();
        let driver = Arc::new(LocalDriver::new(root.to_str().unwrap()).unwrap());
        driver.put("part", b"new").await.unwrap();
        driver.put("object.bin", b"original").await.unwrap();
        for expected_size in [2, 4] {
            compose_follower_objects(
                driver.clone(),
                &NoopMetrics::new(),
                7,
                "object.bin",
                "object.bin",
                vec!["part".to_owned()],
                expected_size,
            )
            .await
            .unwrap_err();
            assert_eq!(driver.get("object.bin").await.unwrap(), b"original");
            assert_eq!(driver.get("part").await.unwrap(), b"new");
            assert_no_attempt_files(&root);
        }
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn payload_error_aborts_stage_and_removes_local_staging() {
        let root = test_root("payload-error");
        tokio::fs::create_dir_all(&root).await.unwrap();
        let driver = Arc::new(LocalDriver::new(root.to_str().unwrap()).unwrap());
        let payload: FollowerUploadBody = Box::pin(futures::stream::iter([
            Ok(Bytes::from_static(b"partial")),
            Err(AsterError::validation_error("injected payload failure")),
        ]));

        let error = write_follower_object(
            driver.clone(),
            &NoopMetrics::new(),
            FollowerObjectWriteRequest {
                binding_id: 7,
                object_key: "object.bin",
                storage_path: "object.bin",
                content_length: 64,
                expected_sha256: None,
            },
            payload,
        )
        .await
        .expect_err("payload failure must abort the current attempt");
        assert!(error.message().contains("injected payload failure"));

        assert!(!driver.exists("object.bin").await.unwrap());
        assert_no_attempt_files(&root);
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn publish_error_cleans_attempt_and_preserves_existing_directory() {
        let root = test_root("commit-error");
        tokio::fs::create_dir_all(root.join("object.bin"))
            .await
            .unwrap();
        let driver = Arc::new(LocalDriver::new(root.to_str().unwrap()).unwrap());
        let payload: FollowerUploadBody = Box::pin(futures::stream::once(async {
            Ok(Bytes::from_static(b"valid"))
        }));

        write_follower_object(
            driver,
            &NoopMetrics::new(),
            FollowerObjectWriteRequest {
                binding_id: 7,
                object_key: "object.bin",
                storage_path: "object.bin",
                content_length: 5,
                expected_sha256: None,
            },
            payload,
        )
        .await
        .expect_err("publishing over a directory must fail and abort staging");

        assert!(
            tokio::fs::metadata(root.join("object.bin"))
                .await
                .unwrap()
                .is_dir()
        );
        assert_no_attempt_files(&root);
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn digest_mismatch_preserves_existing_object_and_cleans_attempt() {
        let root = test_root("digest-mismatch");
        tokio::fs::create_dir_all(&root).await.unwrap();
        tokio::fs::write(root.join("object.bin"), b"original")
            .await
            .unwrap();
        let driver = Arc::new(LocalDriver::new(root.to_str().unwrap()).unwrap());
        let payload: FollowerUploadBody = Box::pin(futures::stream::once(async {
            Ok(Bytes::from_static(b"tampered"))
        }));

        write_follower_object(
            driver.clone(),
            &NoopMetrics::new(),
            FollowerObjectWriteRequest {
                binding_id: 7,
                object_key: "object.bin",
                storage_path: "object.bin",
                content_length: 8,
                expected_sha256: Some(&hex::encode(sha2::Sha256::digest(b"expected"))),
            },
            payload,
        )
        .await
        .expect_err("mismatching bytes must not publish");

        assert_eq!(
            tokio::fs::read(root.join("object.bin")).await.unwrap(),
            b"original"
        );
        assert_no_attempt_files(&root);
        let entries = std::fs::read_dir(&root).unwrap().count();
        assert_eq!(entries, 1, "unpublished attempt object must be removed");
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn stage_with_relay_timeout_aborts_put_and_compose_attempts() {
        let root = test_root("timeout");
        tokio::fs::create_dir_all(&root).await.unwrap();
        let driver = LocalDriver::new(root.to_str().unwrap()).unwrap();

        {
            let attempt_path = "put.bin";
            let attempt = aster_drive_storage::StreamUploadAttempt::new(attempt_path, 5).unwrap();
            let (_writer, reader) = tokio::io::duplex(64);
            let result = stage_with_relay(
                &driver,
                &attempt,
                reader,
                futures::future::pending::<std::result::Result<String, AsterError>>(),
                RelayStageContext {
                    stage_abort_join: "test put stage abort",
                    timeout_abort_join: "test put timeout abort",
                    timeout_error: "test put timeout",
                    relay_join_error: "test put relay join",
                },
                Duration::from_millis(1),
            )
            .await;
            assert!(result.is_err());
            assert!(result.unwrap_err().message().contains("test put timeout"));
            assert_eq!(
                driver.abort_attempt(&attempt).await.unwrap(),
                aster_drive_storage::StreamUploadCleanup::Cleaned
            );
            assert!(!driver.exists(&attempt.staging_path).await.unwrap());
        }

        let attempt = aster_drive_storage::StreamUploadAttempt::new("compose.bin", 5).unwrap();
        let (_writer, reader) = tokio::io::duplex(64);
        let result = stage_with_relay(
            &driver,
            &attempt,
            reader,
            futures::future::pending::<std::result::Result<u64, AsterError>>(),
            RelayStageContext {
                stage_abort_join: "test compose stage abort",
                timeout_abort_join: "test compose timeout abort",
                timeout_error: "test compose timeout",
                relay_join_error: "test compose relay join",
            },
            Duration::from_millis(1),
        )
        .await;
        assert!(
            result
                .unwrap_err()
                .message()
                .contains("test compose timeout")
        );
        assert_eq!(
            driver.abort_attempt(&attempt).await.unwrap(),
            aster_drive_storage::StreamUploadCleanup::Cleaned
        );
        assert!(!driver.exists(&attempt.staging_path).await.unwrap());
        assert_no_attempt_files(&root);
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
}

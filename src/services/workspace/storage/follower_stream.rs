use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use aster_drive_metrics::MetricsRecorder;
use aster_drive_storage::{StorageDriver, StorageErrorKind, StreamUploadAttempt};
use bytes::Bytes;
use futures::{Stream, StreamExt};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use super::{StreamUploadMetricsGuard, cleanup_stream_upload_attempt};
use crate::errors::{AsterError, Result};

const RELAY_BUFFER_SIZE: usize = 64 * 1024;
const STREAM_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(15 * 60);

pub(crate) type FollowerUploadBody = Pin<Box<dyn Stream<Item = Result<Bytes>> + 'static>>;

pub(crate) async fn write_follower_object(
    driver: Arc<dyn StorageDriver>,
    metrics: &dyn MetricsRecorder,
    binding_id: i64,
    object_key: &str,
    storage_path: &str,
    content_length: i64,
    mut payload: FollowerUploadBody,
) -> Result<String> {
    let stream_driver = driver.extensions().stream_upload.ok_or_else(|| {
        crate::errors::storage_driver_error(
            StorageErrorKind::Unsupported,
            "ingress target does not support stream upload",
        )
    })?;
    let attempt = StreamUploadAttempt::new(storage_path, content_length)?;
    let _attempt_metrics = StreamUploadMetricsGuard::new(metrics, content_length);
    let attempt_for_upload = attempt.clone();
    let (writer, reader) = tokio::io::duplex(RELAY_BUFFER_SIZE);

    let stage_result = tokio::task::LocalSet::new()
        .run_until(async move {
            let relay_task = tokio::task::spawn_local(async move {
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
                    AsterError::storage_driver_error(format!(
                        "shutdown relay upload payload: {error}"
                    ))
                })?;
                Ok::<String, AsterError>(format!("\"{}\"", hex::encode(hasher.finalize())))
            });

            let stage = stream_driver.stage_attempt(&attempt_for_upload, Box::new(reader));
            match tokio::time::timeout(STREAM_ATTEMPT_TIMEOUT, stage).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    relay_task.abort();
                    log_aborted_relay_join(relay_task, "follower relay after stage error").await;
                    return Err(error.into());
                }
                Err(_) => {
                    relay_task.abort();
                    log_aborted_relay_join(relay_task, "timed out follower relay").await;
                    return Err(AsterError::storage_driver_error(
                        "stream upload attempt timed out",
                    ));
                }
            }

            relay_task.await.map_err(|error| {
                AsterError::storage_driver_error(format!("relay upload task failed: {error}"))
            })?
        })
        .await;

    let etag = match stage_result {
        Ok(etag) => etag,
        Err(error) => {
            cleanup_stream_upload_attempt(stream_driver, &attempt, metrics, binding_id, object_key)
                .await;
            return Err(error);
        }
    };

    if let Err(error) = stream_driver.commit_attempt(&attempt).await {
        cleanup_stream_upload_attempt(stream_driver, &attempt, metrics, binding_id, object_key)
            .await;
        return Err(error.into());
    }
    metrics.record_stream_upload_attempt("commit", "success");
    Ok(etag)
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
    let attempt = StreamUploadAttempt::new(target_storage_path, expected_size)?;
    let _attempt_metrics = StreamUploadMetricsGuard::new(metrics, expected_size);
    let attempt_for_upload = attempt.clone();
    let read_driver = Arc::clone(&driver);
    let cleanup_part_storage_paths = part_storage_paths.clone();
    let (writer, reader) = tokio::io::duplex(RELAY_BUFFER_SIZE);

    let stage_result = tokio::task::LocalSet::new()
        .run_until(async move {
            let relay_task = tokio::task::spawn_local(async move {
                let mut writer = writer;
                let mut bytes_written = 0_u64;
                for source_path in part_storage_paths {
                    let mut stream = read_driver.get_stream(&source_path).await?;
                    let copied =
                        tokio::io::copy(&mut stream, &mut writer)
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
            });

            let stage = stream_driver.stage_attempt(&attempt_for_upload, Box::new(reader));
            match tokio::time::timeout(STREAM_ATTEMPT_TIMEOUT, stage).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    relay_task.abort();
                    log_aborted_relay_join(relay_task, "compose relay after stage error").await;
                    return Err(error.into());
                }
                Err(_) => {
                    relay_task.abort();
                    log_aborted_relay_join(relay_task, "timed out compose relay").await;
                    return Err(AsterError::storage_driver_error(
                        "compose stream upload attempt timed out",
                    ));
                }
            }

            relay_task.await.map_err(|error| {
                AsterError::storage_driver_error(format!("compose relay task failed: {error}"))
            })?
        })
        .await;

    let bytes_written = match stage_result {
        Ok(bytes_written) if bytes_written == expected_size_u64 => bytes_written,
        Ok(bytes_written) => {
            cleanup_stream_upload_attempt(stream_driver, &attempt, metrics, binding_id, target_key)
                .await;
            return Err(AsterError::storage_driver_error(format!(
                "compose size mismatch: expected {expected_size_u64} bytes, got {bytes_written}"
            )));
        }
        Err(error) => {
            cleanup_stream_upload_attempt(stream_driver, &attempt, metrics, binding_id, target_key)
                .await;
            return Err(error);
        }
    };

    if let Err(error) = stream_driver.commit_attempt(&attempt).await {
        cleanup_stream_upload_attempt(stream_driver, &attempt, metrics, binding_id, target_key)
            .await;
        return Err(error.into());
    }
    metrics.record_stream_upload_attempt("commit", "success");

    for storage_path in cleanup_part_storage_paths {
        if let Err(error) = driver.delete(&storage_path).await {
            tracing::warn!(storage_path, "failed to cleanup composed part: {error}");
        }
    }
    Ok(bytes_written)
}

async fn log_aborted_relay_join<T>(task: tokio::task::JoinHandle<T>, context: &'static str) {
    if let Err(error) = task.await
        && !error.is_cancelled()
    {
        tracing::warn!(context, "failed to join aborted relay task: {error}");
    }
}

use actix_web::web;
use futures::StreamExt;
use tokio::io::AsyncWriteExt;

use crate::api::api_error_code::ApiErrorCode;
use crate::errors::{AsterError, MapAsterErr, Result, file_upload_error_with_code};
use crate::runtime::PrimaryAppState;
use crate::services::workspace::storage::{
    StorePreuploadedNondedupParams, StreamUploadMetricsGuard, abort_direct_stream_attempt,
    check_quota, cleanup_preuploaded_blob_upload, prepare_non_dedup_blob_upload,
    store_preuploaded_nondedup,
};
use aster_drive_model::entities::file;
use aster_drive_storage::{BlobMetadata, StreamUploadAttempt};
use aster_forge_utils::numbers::u64_to_i64;

pub(crate) struct StreamIngestParams<'a> {
    pub scope: crate::services::workspace::storage::WorkspaceStorageScope,
    pub folder_id: Option<i64>,
    pub filename: &'a str,
    pub mime_type: &'a str,
    pub policy: &'a aster_drive_model::entities::storage_policy::Model,
    pub declared_size: i64,
    pub actor_username: Option<&'a str>,
    pub upload_id: &'a str,
}

pub(crate) async fn ingest_stream(
    state: &PrimaryAppState,
    payload: web::Payload,
    params: StreamIngestParams<'_>,
) -> Result<file::Model> {
    let StreamIngestParams {
        scope,
        folder_id,
        filename,
        mime_type,
        policy,
        declared_size,
        actor_username,
        upload_id,
    } = params;
    const RELAY_DIRECT_BUFFER_SIZE: usize = 64 * 1024;
    const STREAM_ATTEMPT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15 * 60);

    if policy.max_file_size > 0 && declared_size > policy.max_file_size {
        return Err(AsterError::file_too_large(format!(
            "file size {} exceeds limit {}",
            declared_size, policy.max_file_size
        )));
    }

    check_quota(state.writer_db(), scope, declared_size).await?;
    let driver = state.driver_registry().get_driver(policy)?;

    let filename = aster_forge_validation::filename::normalize_validate_name(filename)?;
    let mut payload = payload;
    let prepared_upload = prepare_non_dedup_blob_upload(
        state.driver_registry().connectors(),
        policy,
        declared_size,
        Some(&filename),
    )?;
    let storage_path = prepared_upload.storage_path().to_string();
    let attempt = StreamUploadAttempt::new(&storage_path, declared_size)?;
    let _attempt_metrics = StreamUploadMetricsGuard::new(state.metrics.as_ref(), declared_size);
    let attempt_for_upload = attempt.clone();

    let (writer, reader) = tokio::io::duplex(RELAY_DIRECT_BUFFER_SIZE);
    let upload_driver = driver.clone();
    let stream_driver = upload_driver.extensions().stream_upload.ok_or_else(|| {
        crate::errors::AsterError::storage_driver_error("stream upload not supported")
    })?;
    let relay_outcome = tokio::task::LocalSet::new()
        .run_until(async move {
            let relay_task = tokio::task::spawn_local(async move {
                let mut writer = writer;
                while let Some(chunk) = payload.next().await {
                    let chunk = chunk.map_aster_err(upload_field_read_failed)?;
                    writer.write_all(&chunk).await.map_aster_err_ctx(
                        "relay direct write",
                        upload_direct_relay_write_failed,
                    )?;
                }
                writer.shutdown().await.map_aster_err_ctx(
                    "relay direct shutdown",
                    upload_direct_relay_shutdown_failed,
                )?;
                Ok::<(), AsterError>(())
            });

            let upload_result = stream_driver.stage_attempt(&attempt_for_upload, Box::new(reader));
            let upload_result =
                match tokio::time::timeout(STREAM_ATTEMPT_TIMEOUT, upload_result).await {
                    Ok(result) => result.map_err(AsterError::from),
                    Err(_) => {
                        relay_task.abort();
                        if let Err(error) = relay_task.await
                            && !error.is_cancelled()
                        {
                            tracing::warn!("failed to join aborted direct relay task: {error}");
                        }
                        return Err(AsterError::storage_driver_error(
                            "streaming direct upload attempt timed out",
                        ));
                    }
                };
            if let Err(error) = upload_result {
                relay_task.abort();
                if let Err(join_error) = relay_task.await
                    && !join_error.is_cancelled()
                {
                    tracing::warn!(
                        "failed to join direct relay task after stage error: {join_error}"
                    );
                }
                return Ok::<(Result<()>, Result<()>), AsterError>((
                    Err(error),
                    Err(AsterError::storage_driver_error(
                        "streaming direct upload stage failed",
                    )),
                ));
            }
            let relay_result = relay_task.await.map_err(|err| {
                file_upload_error_with_code(
                    ApiErrorCode::UploadDirectRelayTaskFailed,
                    format!("relay direct task failed: {err}"),
                )
            })?;

            Ok::<(Result<()>, Result<()>), AsterError>((upload_result, relay_result))
        })
        .await;

    let (upload_result, relay_result) = match relay_outcome {
        Ok(outcome) => outcome,
        Err(error) => {
            abort_direct_stream_attempt(
                stream_driver,
                &attempt,
                state.metrics.as_ref(),
                driver.as_ref(),
                &prepared_upload,
                "direct stream orchestration error",
            )
            .await;
            return Err(error);
        }
    };

    if let Err(err) = upload_result {
        abort_direct_stream_attempt(
            stream_driver,
            &attempt,
            state.metrics.as_ref(),
            driver.as_ref(),
            &prepared_upload,
            "direct stream upload error",
        )
        .await;
        return Err(err);
    }

    if let Err(err) = relay_result {
        abort_direct_stream_attempt(
            stream_driver,
            &attempt,
            state.metrics.as_ref(),
            driver.as_ref(),
            &prepared_upload,
            "direct stream relay error",
        )
        .await;
        return Err(err);
    }

    if let Err(err) = stream_driver.commit_attempt(&attempt).await {
        abort_direct_stream_attempt(
            stream_driver,
            &attempt,
            state.metrics.as_ref(),
            driver.as_ref(),
            &prepared_upload,
            "direct stream commit error",
        )
        .await;
        return Err(err.into());
    }
    state
        .metrics
        .record_stream_upload_attempt("commit", "success");

    let metadata = match driver.metadata(&storage_path).await {
        Ok(metadata) => metadata,
        Err(err) => {
            cleanup_preuploaded_blob_upload(
                driver.as_ref(),
                &prepared_upload,
                "direct stream metadata error",
            )
            .await;
            return Err(err.into());
        }
    };
    let actual_size = match validate_streaming_direct_uploaded_size(metadata, declared_size, policy)
    {
        Ok(actual_size) => actual_size,
        Err(err) => {
            cleanup_preuploaded_blob_upload(
                driver.as_ref(),
                &prepared_upload,
                "direct stream size validation failure",
            )
            .await;
            return Err(err);
        }
    };
    if let Err(err) = check_quota(state.writer_db(), scope, actual_size).await {
        cleanup_preuploaded_blob_upload(
            driver.as_ref(),
            &prepared_upload,
            "direct stream quota validation failure",
        )
        .await;
        return Err(err);
    }

    store_preuploaded_nondedup(
        state,
        StorePreuploadedNondedupParams {
            scope,
            folder_id,
            filename: &filename,
            mime_type: Some(mime_type),
            size: actual_size,
            existing_file_id: None,
            lock_credentials: crate::services::files::lock::LockMutationCredentials::None,
            policy,
            preuploaded_blob: prepared_upload,
            actor_username,
            complete_upload_id: Some(upload_id),
        },
    )
    .await
}

fn upload_field_read_failed(message: String) -> AsterError {
    file_upload_error_with_code(ApiErrorCode::UploadFieldReadFailed, message)
}

fn upload_direct_relay_write_failed(message: String) -> AsterError {
    file_upload_error_with_code(ApiErrorCode::UploadDirectRelayWriteFailed, message)
}

fn upload_direct_relay_shutdown_failed(message: String) -> AsterError {
    file_upload_error_with_code(ApiErrorCode::UploadDirectRelayShutdownFailed, message)
}

fn upload_size_mismatch_error(declared_size: i64, actual_size: i64) -> AsterError {
    AsterError::validation_error(format!(
        "size mismatch: declared {declared_size} bytes, received {actual_size} bytes"
    ))
}

fn validate_streaming_direct_uploaded_size(
    metadata: BlobMetadata,
    declared_size: i64,
    policy: &aster_drive_model::entities::storage_policy::Model,
) -> Result<i64> {
    let actual_size = u64_to_i64(metadata.size, "streaming direct uploaded size")?;
    if actual_size != declared_size {
        return Err(upload_size_mismatch_error(declared_size, actual_size));
    }
    if policy.max_file_size > 0 && actual_size > policy.max_file_size {
        return Err(AsterError::file_too_large(format!(
            "file size {} exceeds limit {}",
            actual_size, policy.max_file_size
        )));
    }
    Ok(actual_size)
}

#[cfg(test)]
mod tests {
    use super::validate_streaming_direct_uploaded_size;
    use aster_drive_metrics::MetricsRecorder;
    use aster_drive_storage::BlobMetadata;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct RecordingMetrics {
        events: Mutex<Vec<String>>,
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
            self.events
                .lock()
                .unwrap()
                .push(format!("bytes:{kind}:{bytes}"));
        }

        fn adjust_stream_upload_active(&self, delta: i64) {
            *self.active.lock().unwrap() += delta;
        }
    }

    fn policy_with_max_file_size(
        max_file_size: i64,
    ) -> aster_drive_model::entities::storage_policy::Model {
        let mut policy = crate::storage::connectors::test_support::s3_policy(
            "https://s3.example.test",
            "test-bucket",
            "",
            aster_drive_model::types::ObjectStorageUploadStrategy::Presigned,
            aster_drive_model::types::ObjectStorageDownloadStrategy::RelayStream,
        );
        policy.max_file_size = max_file_size;
        policy.is_default = true;
        policy.chunk_size = 5_242_880;
        policy
    }

    #[test]
    fn direct_stream_metrics_guard_records_lifecycle_and_releases_active() {
        let recorder = Arc::new(RecordingMetrics::default());
        {
            let _guard = super::StreamUploadMetricsGuard::new(recorder.as_ref(), 4096);
            assert_eq!(*recorder.active.lock().unwrap(), 1);
        }

        assert_eq!(*recorder.active.lock().unwrap(), 0);
        assert_eq!(
            recorder.events.lock().unwrap().as_slice(),
            ["attempt:started", "bytes:expected:4096"]
        );
    }

    #[test]
    fn validate_streaming_direct_uploaded_size_rejects_declared_size_mismatch() {
        let policy = policy_with_max_file_size(0);
        let error = validate_streaming_direct_uploaded_size(
            BlobMetadata {
                size: 10,
                content_type: None,
            },
            1,
            &policy,
        )
        .expect_err("actual uploaded size must match declared size");

        assert!(error.message().contains("size mismatch"));
    }

    #[test]
    fn validate_streaming_direct_uploaded_size_accepts_exact_policy_boundary() {
        let policy = policy_with_max_file_size(10);
        let actual_size = validate_streaming_direct_uploaded_size(
            BlobMetadata {
                size: 10,
                content_type: None,
            },
            10,
            &policy,
        )
        .expect("actual size equal to max_file_size should be accepted");

        assert_eq!(actual_size, 10);
    }

    #[test]
    fn validate_streaming_direct_uploaded_size_accepts_unlimited_policy() {
        let policy = policy_with_max_file_size(0);
        let actual_size = validate_streaming_direct_uploaded_size(
            BlobMetadata {
                size: 1024,
                content_type: None,
            },
            1024,
            &policy,
        )
        .expect("max_file_size 0 should allow any matching declared size");

        assert_eq!(actual_size, 1024);
    }

    #[test]
    fn validate_streaming_direct_uploaded_size_checks_policy_against_actual_size() {
        let policy = policy_with_max_file_size(8);
        let error = validate_streaming_direct_uploaded_size(
            BlobMetadata {
                size: 10,
                content_type: None,
            },
            10,
            &policy,
        )
        .expect_err("actual uploaded size must respect policy max_file_size");

        assert!(error.message().contains("exceeds limit 8"));
    }

    #[test]
    fn validate_streaming_direct_uploaded_size_rejects_metadata_size_outside_i64() {
        let policy = policy_with_max_file_size(0);
        let error = validate_streaming_direct_uploaded_size(
            BlobMetadata {
                size: i64::MAX as u64 + 1,
                content_type: None,
            },
            i64::MAX,
            &policy,
        )
        .expect_err("metadata size outside i64 must be rejected");

        assert!(error.message().contains("streaming direct uploaded size"));
    }
}

//! 上传服务子模块：`shared`。

use chrono::{DateTime, Utc};
use sea_orm::{ConnectionTrait, Set};
use std::future::Future;
use std::time::Instant;

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{file_repo, upload_session_repo};
use crate::entities::{file, upload_session};
use crate::errors::{
    AsterError, Result, chunk_upload_error_with_code, upload_assembly_error_with_code,
    validation_error_with_code,
};
use crate::runtime::SharedRuntimeState;
use crate::storage::MultipartStorageDriver;
use crate::storage::StorageErrorKind;
use crate::types::UploadSessionStatus;
use crate::utils::{id, paths};

const INIT_MULTIPART_ABORT_MAX_ATTEMPTS: u32 = 3;
const INIT_MULTIPART_ABORT_INITIAL_BACKOFF_MS: u64 = 50;

pub(super) use id::UniqueUuidAttempt;

pub(super) async fn with_unique_upload_id<F, Fut, T>(mut try_upload_id: F) -> Result<T>
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<UniqueUuidAttempt<T>>>,
{
    id::with_unique_uuid("upload session", |candidate| {
        try_upload_id(candidate.to_string())
    })
    .await
}

pub(super) async fn delete_upload_session_record_after_init_error<C: ConnectionTrait>(
    db: &C,
    upload_id: &str,
    context: &str,
) {
    if let Err(error) = upload_session_repo::delete(db, upload_id).await {
        tracing::warn!(
            upload_id,
            "failed to delete upload session after {context}: {error}"
        );
    }
}

pub(super) async fn abort_created_multipart_upload_after_init_error(
    multipart: &dyn MultipartStorageDriver,
    temp_key: &str,
    multipart_id: &str,
    upload_id: &str,
    context: &str,
) -> Result<()> {
    let mut last_error = None;
    for attempt in 1..=INIT_MULTIPART_ABORT_MAX_ATTEMPTS {
        match multipart
            .abort_multipart_upload(temp_key, multipart_id)
            .await
        {
            Ok(()) => return Ok(()),
            Err(error) if error.storage_error_kind() == Some(StorageErrorKind::NotFound) => {
                return Ok(());
            }
            Err(error) => {
                if attempt == INIT_MULTIPART_ABORT_MAX_ATTEMPTS {
                    last_error = Some(error);
                    break;
                }
                let backoff_ms = INIT_MULTIPART_ABORT_INITIAL_BACKOFF_MS * (1_u64 << (attempt - 1));
                tracing::warn!(
                    upload_id,
                    temp_key,
                    multipart_id,
                    attempt,
                    max_attempts = INIT_MULTIPART_ABORT_MAX_ATTEMPTS,
                    backoff_ms,
                    "failed to abort multipart upload after {context}, retrying: {error}"
                );
                tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
            }
        }
    }

    let error = last_error.unwrap_or_else(|| {
        AsterError::storage_driver_error("multipart abort failed without an error")
    });
    tracing::warn!(
        upload_id,
        temp_key,
        multipart_id,
        max_attempts = INIT_MULTIPART_ABORT_MAX_ATTEMPTS,
        "failed to abort multipart upload after {context}: {error}"
    );
    Err(error)
}

pub(super) fn upload_session_chunk_unavailable_error(
    session: &upload_session::Model,
) -> AsterError {
    match session.status {
        UploadSessionStatus::Failed => {
            AsterError::upload_session_expired("session was canceled or failed")
        }
        UploadSessionStatus::Assembling => {
            AsterError::upload_session_expired("session is assembling and no longer accepts chunks")
        }
        UploadSessionStatus::Completed => {
            AsterError::upload_session_expired("session already completed")
        }
        UploadSessionStatus::Presigned => validation_error_with_code(
            ApiErrorCode::UploadChunkTransportMismatch,
            "session does not accept relay chunk uploads",
        ),
        UploadSessionStatus::Uploading => {
            AsterError::upload_session_not_found(format!("session {}", session.id))
        }
    }
}

pub(super) fn expected_chunk_size_for_upload(
    session: &upload_session::Model,
    chunk_number: i32,
) -> Result<i64> {
    if session.total_chunks <= 0 || session.chunk_size <= 0 {
        return Err(chunk_upload_error_with_code(
            ApiErrorCode::UploadChunkSessionInvalid,
            format!(
                "invalid upload session chunk metadata: total_chunks={}, chunk_size={}",
                session.total_chunks, session.chunk_size
            ),
        ));
    }

    if chunk_number < session.total_chunks - 1 {
        return Ok(session.chunk_size);
    }

    let preceding = session.chunk_size * i64::from(session.total_chunks - 1);
    let expected = session.total_size - preceding;
    if expected <= 0 {
        return Err(chunk_upload_error_with_code(
            ApiErrorCode::UploadChunkSessionInvalid,
            format!(
                "invalid final chunk size for upload {}: total_size={}, preceding={preceding}",
                session.id, session.total_size
            ),
        ));
    }
    Ok(expected)
}

pub(super) fn upload_session_status_label(status: UploadSessionStatus) -> &'static str {
    match status {
        UploadSessionStatus::Uploading => "uploading",
        UploadSessionStatus::Assembling => "assembling",
        UploadSessionStatus::Completed => "completed",
        UploadSessionStatus::Failed => "failed",
        UploadSessionStatus::Presigned => "presigned",
    }
}

pub(super) async fn transition_upload_session_to_assembling<C: ConnectionTrait>(
    db: &C,
    upload_id: &str,
    actual_status: UploadSessionStatus,
    expected_status: UploadSessionStatus,
) -> Result<()> {
    let now = Utc::now();
    let transitioned = upload_session_repo::try_transition_status_before_expiry(
        db,
        upload_id,
        expected_status,
        UploadSessionStatus::Assembling,
        now,
    )
    .await?;
    if !transitioned {
        if let Ok(session) = upload_session_repo::find_by_id(db, upload_id).await {
            if session.status == expected_status && session.expires_at <= now {
                return Err(AsterError::upload_session_expired("session expired"));
            }
            return Err(upload_assembly_error_with_code(
                ApiErrorCode::UploadStatusConflict,
                format!(
                    "session status is '{:?}', expected '{}'",
                    session.status,
                    upload_session_status_label(expected_status)
                ),
            ));
        }
        return Err(upload_assembly_error_with_code(
            ApiErrorCode::UploadStatusConflict,
            format!(
                "session status is '{:?}', expected '{}'",
                actual_status,
                upload_session_status_label(expected_status)
            ),
        ));
    }
    Ok(())
}

pub(super) async fn run_upload_completion_stage<C, Fut>(
    db: &C,
    session: &upload_session::Model,
    expected_status: UploadSessionStatus,
    success_log_message: &'static str,
    completion_future: Fut,
) -> Result<file::Model>
where
    C: ConnectionTrait,
    Fut: Future<Output = Result<file::Model>>,
{
    let upload_id = session.id.as_str();
    let stage_started_at = Instant::now();
    let transition_started_at = Instant::now();
    transition_upload_session_to_assembling(db, upload_id, session.status, expected_status).await?;
    let transition_elapsed_ms = transition_started_at.elapsed().as_millis();

    let completion_started_at = Instant::now();
    match completion_future.await {
        Ok(file) => {
            let completion_elapsed_ms = completion_started_at.elapsed().as_millis();
            let total_elapsed_ms = stage_started_at.elapsed().as_millis();
            tracing::debug!(
                upload_id,
                file_id = file.id,
                blob_id = file.blob_id,
                size = file.size,
                transition_elapsed_ms,
                completion_elapsed_ms,
                total_elapsed_ms,
                "{}",
                success_log_message
            );
            Ok(file)
        }
        Err(error) => {
            let completion_elapsed_ms = completion_started_at.elapsed().as_millis();
            handle_completion_error(db, upload_id, expected_status, &error).await;
            tracing::debug!(
                upload_id,
                expected_status = ?expected_status,
                transition_elapsed_ms,
                completion_elapsed_ms,
                total_elapsed_ms = stage_started_at.elapsed().as_millis(),
                error_code = error.code(),
                "upload completion stage failed"
            );
            Err(error)
        }
    }
}

pub(super) async fn cleanup_upload_temp_dir(state: &impl SharedRuntimeState, upload_id: &str) {
    let temp_dir = paths::upload_temp_dir(&state.config().server.upload_temp_dir, upload_id);
    crate::utils::cleanup_temp_dir(&temp_dir).await;
}

/// 根据 session 查找已完成的文件（幂等重试用）
pub(super) async fn find_file_by_session<C: ConnectionTrait>(
    db: &C,
    session: &upload_session::Model,
) -> Result<file::Model> {
    let file_id = session.file_id.ok_or_else(|| {
        upload_assembly_error_with_code(
            ApiErrorCode::UploadCompletedFileMissing,
            "upload already completed but file_id not found; please refresh",
        )
    })?;
    file_repo::find_by_id(db, file_id).await
}

/// 将 session 标记为 Failed（best-effort，失败只记录日志）
pub(super) async fn mark_session_failed<C: ConnectionTrait>(db: &C, upload_id: &str) {
    if let Ok(session) = upload_session_repo::find_by_id(db, upload_id).await {
        let mut active: upload_session::ActiveModel = session.into();
        active.status = Set(UploadSessionStatus::Failed);
        active.updated_at = Set(Utc::now());
        if let Err(error) = upload_session_repo::update(db, active).await {
            tracing::warn!("failed to mark session {upload_id} as failed: {error}");
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UploadStorageErrorClass {
    Retryable,
    RequiresIntervention,
    NotFound,
    Terminal,
}

pub(super) fn classify_upload_storage_error(error: &AsterError) -> UploadStorageErrorClass {
    match error {
        AsterError::RecordNotFound(_) => UploadStorageErrorClass::NotFound,
        AsterError::PreconditionFailed(_)
        | AsterError::ValidationError(_)
        | AsterError::UnsupportedDriver(_) => UploadStorageErrorClass::RequiresIntervention,
        AsterError::StorageDriverError(_) => match error.storage_error_kind() {
            Some(StorageErrorKind::Transient | StorageErrorKind::RateLimited) => {
                UploadStorageErrorClass::Retryable
            }
            Some(StorageErrorKind::Auth)
            | Some(StorageErrorKind::Permission)
            | Some(StorageErrorKind::Misconfigured)
            | Some(StorageErrorKind::Unsupported)
            | Some(StorageErrorKind::Precondition) => UploadStorageErrorClass::RequiresIntervention,
            Some(StorageErrorKind::NotFound) => UploadStorageErrorClass::NotFound,
            Some(StorageErrorKind::Unknown) | None => UploadStorageErrorClass::Terminal,
        },
        _ => UploadStorageErrorClass::Terminal,
    }
}

pub(super) fn upload_completion_error_is_retryable(error: &AsterError) -> bool {
    matches!(
        classify_upload_storage_error(error),
        UploadStorageErrorClass::Retryable
    )
}

pub(super) fn upload_storage_error_class_label(class: UploadStorageErrorClass) -> &'static str {
    match class {
        UploadStorageErrorClass::Retryable => "retryable",
        UploadStorageErrorClass::RequiresIntervention => "requires_operator_intervention",
        UploadStorageErrorClass::NotFound => "not_found",
        UploadStorageErrorClass::Terminal => "terminal",
    }
}

async fn restore_session_status_after_retryable_completion_error<C: ConnectionTrait>(
    db: &C,
    upload_id: &str,
    restore_status: UploadSessionStatus,
) -> Result<()> {
    if upload_session_repo::try_transition_status(
        db,
        upload_id,
        UploadSessionStatus::Assembling,
        restore_status,
    )
    .await?
    {
        return Ok(());
    }

    match upload_session_repo::find_by_id(db, upload_id).await {
        Ok(session) if session.status == restore_status => {}
        Ok(session) if session.status == UploadSessionStatus::Assembling => {
            tracing::warn!(
                upload_id,
                restore_status = ?restore_status,
                current_status = ?session.status,
                "upload session remained in assembling after a retryable completion error"
            );
        }
        Ok(session) => {
            tracing::debug!(
                upload_id,
                restore_status = ?restore_status,
                current_status = ?session.status,
                "upload session status changed before retryable completion error recovery finished"
            );
        }
        Err(error) => {
            tracing::warn!(
                upload_id,
                restore_status = ?restore_status,
                "failed to reload upload session after retryable completion error: {error}"
            );
        }
    }

    Ok(())
}

pub(super) async fn handle_completion_error<C: ConnectionTrait>(
    db: &C,
    upload_id: &str,
    restore_status: UploadSessionStatus,
    error: &AsterError,
) {
    if upload_completion_error_is_retryable(error) {
        if let Err(restore_error) =
            restore_session_status_after_retryable_completion_error(db, upload_id, restore_status)
                .await
        {
            tracing::warn!(
                upload_id,
                restore_status = ?restore_status,
                "failed to restore upload session after retryable completion error: {restore_error}"
            );
        }
        return;
    }

    mark_session_failed(db, upload_id).await;
}

pub(super) async fn mark_session_failed_with_expiration<C: ConnectionTrait>(
    db: &C,
    upload_id: &str,
    expires_at: DateTime<Utc>,
) -> Result<()> {
    let session = upload_session_repo::find_by_id(db, upload_id).await?;
    let mut active: upload_session::ActiveModel = session.into();
    active.status = Set(UploadSessionStatus::Failed);
    active.expires_at = Set(expires_at);
    active.updated_at = Set(Utc::now());
    upload_session_repo::update(db, active).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::UploadSessionStatus;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct NotFoundAbortMultipart {
        abort_calls: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl MultipartStorageDriver for NotFoundAbortMultipart {
        async fn create_multipart_upload(&self, _path: &str) -> Result<String> {
            panic!("not used")
        }

        async fn presigned_upload_part_url(
            &self,
            _path: &str,
            _upload_id: &str,
            _part_number: i32,
            _expires: std::time::Duration,
        ) -> Result<String> {
            panic!("not used")
        }

        async fn complete_multipart_upload(
            &self,
            _path: &str,
            _upload_id: &str,
            _parts: Vec<(i32, String)>,
        ) -> Result<()> {
            panic!("not used")
        }

        async fn upload_multipart_part(
            &self,
            _path: &str,
            _upload_id: &str,
            _part_number: i32,
            _data: &[u8],
        ) -> Result<String> {
            panic!("not used")
        }

        async fn abort_multipart_upload(&self, _path: &str, _upload_id: &str) -> Result<()> {
            self.abort_calls.fetch_add(1, Ordering::SeqCst);
            Err(AsterError::storage_driver_error(
                "S3 abort_multipart_upload failed: NoSuchUpload",
            ))
        }

        async fn list_uploaded_part_details(
            &self,
            _path: &str,
            _upload_id: &str,
        ) -> Result<Vec<crate::storage::traits::UploadedMultipartPart>> {
            panic!("not used")
        }
    }

    fn mock_session(total_size: i64, chunk_size: i64, total_chunks: i32) -> upload_session::Model {
        upload_session::Model {
            id: "test-session".to_string(),
            user_id: 1,
            team_id: None,
            frontend_client_id: None,
            filename: "test.bin".to_string(),
            total_size,
            chunk_size,
            total_chunks,
            received_count: 0,
            folder_id: None,
            policy_id: 1,
            status: UploadSessionStatus::Uploading,
            s3_temp_key: None,
            s3_multipart_id: None,
            file_id: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        }
    }

    #[test]
    fn expected_chunk_size_non_final() {
        // 5 chunks, 1MB each, total 5MB
        let session = mock_session(5_242_880, 1_048_576, 5);
        let size = expected_chunk_size_for_upload(&session, 0).unwrap();
        assert_eq!(size, 1_048_576);
        let size = expected_chunk_size_for_upload(&session, 3).unwrap();
        assert_eq!(size, 1_048_576);
    }

    #[test]
    fn expected_chunk_size_final() {
        // 5 chunks, 1MB each, total 5MB-1 (non-even division)
        let total = 1_048_576 * 4 + 500_000;
        let session = mock_session(total, 1_048_576, 5);
        let size = expected_chunk_size_for_upload(&session, 4).unwrap();
        assert_eq!(size, 500_000);
    }

    #[test]
    fn expected_chunk_size_invalid_metadata() {
        let session = mock_session(100, 0, 10); // chunk_size = 0
        let err = expected_chunk_size_for_upload(&session, 0).unwrap_err();
        assert_eq!(err.code(), "E056"); // ChunkUploadFailed
        assert_eq!(
            err.api_error_code_override(),
            Some(ApiErrorCode::UploadChunkSessionInvalid)
        );
    }

    #[test]
    fn expected_chunk_size_final_negative() {
        // total_size < preceding (corrupted session)
        let session = mock_session(1_000_000, 1_048_576, 2); // 2 chunks, but total < 1 chunk
        let err = expected_chunk_size_for_upload(&session, 1).unwrap_err();
        assert_eq!(err.code(), "E056");
        assert_eq!(
            err.api_error_code_override(),
            Some(ApiErrorCode::UploadChunkSessionInvalid)
        );
    }

    #[test]
    fn upload_session_chunk_unavailable_error_matrix() {
        use UploadSessionStatus::*;

        let mut session = mock_session(100, 10, 10);

        session.status = Failed;
        let e = upload_session_chunk_unavailable_error(&session);
        assert_eq!(e.code(), "E055"); // UploadSessionExpired

        session.status = Assembling;
        let e = upload_session_chunk_unavailable_error(&session);
        assert_eq!(e.code(), "E055");

        session.status = Completed;
        let e = upload_session_chunk_unavailable_error(&session);
        assert_eq!(e.code(), "E055");

        session.status = Presigned;
        let e = upload_session_chunk_unavailable_error(&session);
        assert_eq!(e.code(), "E005"); // ValidationError
        assert_eq!(
            e.api_error_code_override(),
            Some(ApiErrorCode::UploadChunkTransportMismatch)
        );

        session.status = Uploading;
        let e = upload_session_chunk_unavailable_error(&session);
        assert_eq!(e.code(), "E054"); // UploadSessionNotFound
    }

    #[test]
    fn upload_session_status_label_mapping() {
        use UploadSessionStatus::*;
        assert_eq!(upload_session_status_label(Uploading), "uploading");
        assert_eq!(upload_session_status_label(Assembling), "assembling");
        assert_eq!(upload_session_status_label(Completed), "completed");
        assert_eq!(upload_session_status_label(Failed), "failed");
        assert_eq!(upload_session_status_label(Presigned), "presigned");
    }

    #[test]
    fn classify_upload_storage_error_marks_transient_remote_failures_retryable() {
        let error = AsterError::storage_driver_error(
            "remote storage request failed: error sending request for url (http://127.0.0.1:9)",
        );
        assert_eq!(
            classify_upload_storage_error(&error),
            UploadStorageErrorClass::Retryable
        );
    }

    #[test]
    fn classify_upload_storage_error_marks_remote_auth_failures_as_intervention() {
        let error = AsterError::storage_driver_error(
            "remote node authentication failed: delete remote storage object: denied",
        );
        assert_eq!(
            classify_upload_storage_error(&error),
            UploadStorageErrorClass::RequiresIntervention
        );
    }

    #[test]
    fn classify_upload_storage_error_marks_precondition_failures_as_intervention() {
        let error = AsterError::precondition_failed("remote node #1 is disabled");
        assert_eq!(
            classify_upload_storage_error(&error),
            UploadStorageErrorClass::RequiresIntervention
        );
    }

    #[test]
    fn classify_upload_storage_error_marks_not_found_errors() {
        let error =
            AsterError::storage_driver_error("S3 abort_multipart_upload failed: NoSuchUpload");
        assert_eq!(
            classify_upload_storage_error(&error),
            UploadStorageErrorClass::NotFound
        );
    }

    #[tokio::test]
    async fn abort_created_multipart_upload_treats_not_found_as_success() {
        let multipart = NotFoundAbortMultipart {
            abort_calls: AtomicUsize::new(0),
        };

        abort_created_multipart_upload_after_init_error(
            &multipart,
            "tmp/upload",
            "multipart-1",
            "upload-1",
            "test cleanup",
        )
        .await
        .expect("not found multipart abort should be treated as already cleaned up");

        assert_eq!(multipart.abort_calls.load(Ordering::SeqCst), 1);
    }
}

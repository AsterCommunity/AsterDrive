use chrono::Utc;

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::upload_session_part_repo;
use crate::entities::{file, storage_policy, upload_session};
use crate::errors::{AsterError, Result, upload_assembly_error_with_code};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::files::upload::shared::{
    run_upload_completion_stage, upload_completion_error_is_retryable,
};
use crate::services::workspace::scope::WorkspaceStorageScope;
use crate::services::workspace::storage;
use crate::storage::StorageDriver;
use crate::storage::traits::multipart::{MultipartStorageDriver, UploadedMultipartPart};
use crate::types::UploadSessionStatus;
use crate::utils::numbers::u64_to_i64;

use super::contract::{VerifiedUploadedBlob, cleanup_verified_upload_after_db_failure};

pub(super) async fn complete_presigned_upload(
    state: &PrimaryAppState,
    session: upload_session::Model,
    actor_username: Option<&str>,
) -> Result<file::Model> {
    // presigned 单文件的 complete 阶段，本质是“确认对象存在且大小正确”，
    // 然后把 temp_key 直接认领成正式 blob。
    let db = state.writer_db();
    // Historical column name: this key is used by any presigned single-object
    // upload transport, not only S3-compatible drivers.
    let temp_key = session.object_temp_key.as_deref().ok_or_else(|| {
        upload_assembly_error_with_code(
            ApiErrorCode::UploadSessionCorrupted,
            "missing object_temp_key",
        )
    })?;

    let policy = state
        .policy_snapshot()
        .get_policy_or_err(session.policy_id)?;
    let driver = state.driver_registry().get_driver(&policy)?;
    let actual_size = ensure_uploaded_object_size(
        driver.as_ref(),
        temp_key,
        session.total_size,
        "uploaded object not found - upload may not have completed",
    )
    .await?;

    tracing::debug!(
        upload_id = %session.id,
        status = ?session.status,
        policy_id = policy.id,
        "completing presigned upload session"
    );
    run_upload_completion_stage(
        db,
        &session,
        UploadSessionStatus::Presigned,
        "completed presigned upload session",
        async {
            let (final_key, actual_size) = copy_presigned_object_to_final_key(
                driver.as_ref(),
                temp_key,
                session.total_size,
                actual_size,
            )
            .await?;
            let verified = VerifiedUploadedBlob::copied_opaque_object(
                actual_size,
                policy.id,
                final_key,
                opaque_upload_file_hash(&policy, &session)?,
            )?;
            let file = finalize_verified_opaque_upload_session(
                state,
                &session,
                driver.as_ref(),
                &verified,
                actor_username,
            )
            .await?;
            if verified.storage_path() != temp_key
                && let Err(error) = driver.delete(temp_key).await
            {
                tracing::warn!(
                    upload_id = %session.id,
                    temp_key = %temp_key,
                    final_key = %verified.storage_path(),
                    "failed to delete presigned temp object after final copy: {error}"
                );
            }
            Ok(file)
        },
    )
    .await
}

/// 完成 presigned object multipart 上传：complete multipart → 直接建文件记录
pub(super) async fn complete_presigned_multipart(
    state: &PrimaryAppState,
    session: upload_session::Model,
    parts: Vec<(i32, String)>,
    actor_username: Option<&str>,
) -> Result<file::Model> {
    complete_object_multipart_upload_session(
        state,
        session,
        UploadSessionStatus::Presigned,
        parts,
        "uploaded object not found after multipart complete - assembly may have failed",
        actor_username,
    )
    .await
}

/// 完成 relay multipart 上传：直接使用服务端保存的 parts 完成 multipart。
pub(super) async fn complete_relay_multipart(
    state: &PrimaryAppState,
    session: upload_session::Model,
    actor_username: Option<&str>,
) -> Result<file::Model> {
    let db = state.writer_db();
    let parts = upload_session_part_repo::list_by_upload(db, &session.id).await?;
    let expected_parts =
        crate::utils::numbers::i32_to_usize(session.total_chunks, "upload session total_chunks")?;
    if parts.len() != expected_parts {
        return Err(upload_assembly_error_with_code(
            ApiErrorCode::UploadIncompleteParts,
            format!(
                "expected {} parts, got {}",
                session.total_chunks,
                parts.len()
            ),
        ));
    }

    for (expected, part) in (1..=session.total_chunks).zip(parts.iter()) {
        if part.part_number != expected {
            return Err(upload_assembly_error_with_code(
                ApiErrorCode::UploadMissingPart,
                format!(
                    "missing uploaded part {}; got {:?}",
                    expected, part.part_number
                ),
            ));
        }
    }

    let completed_parts = parts
        .into_iter()
        .map(|part| (part.part_number, part.etag))
        .collect();
    complete_object_multipart_upload_session(
        state,
        session,
        UploadSessionStatus::Uploading,
        completed_parts,
        "uploaded object not found after relay multipart complete - assembly may have failed",
        actor_username,
    )
    .await
}

async fn ensure_uploaded_object_size(
    driver: &dyn StorageDriver,
    temp_key: &str,
    declared_size: i64,
    missing_message: &str,
) -> Result<i64> {
    let meta = match driver.metadata(temp_key).await {
        Ok(meta) => meta,
        Err(error) => match driver.exists(temp_key).await {
            Ok(false) => {
                return Err(upload_assembly_error_with_code(
                    ApiErrorCode::UploadTempObjectMissing,
                    missing_message,
                ));
            }
            Ok(true) => return Err(error),
            Err(exists_error) => {
                tracing::warn!(
                    temp_key = %temp_key,
                    "failed to verify uploaded temp object existence after metadata error: metadata_error={error}, exists_error={exists_error}"
                );
                return Err(error);
            }
        },
    };
    let actual_size = u64_to_i64(meta.size, "blob_size")?;

    if actual_size != declared_size {
        if let Err(error) = driver.delete(temp_key).await {
            tracing::warn!("failed to delete uploaded temp object: {error}");
        }
        return Err(upload_assembly_error_with_code(
            ApiErrorCode::UploadTempObjectSizeMismatch,
            format!(
                "size mismatch: declared {} but uploaded {}",
                declared_size, actual_size
            ),
        ));
    }

    Ok(actual_size)
}

async fn finalize_verified_opaque_upload_session(
    state: &PrimaryAppState,
    session: &upload_session::Model,
    driver: &dyn StorageDriver,
    verified: &VerifiedUploadedBlob,
    actor_username: Option<&str>,
) -> Result<file::Model> {
    // 直传模式不会经过本地 assembled 文件，complete 阶段只负责把已经存在的对象
    // 记成 blob + file，并原子更新配额和 session 状态。
    let file_hash = verified.file_hash().ok_or_else(|| {
        upload_assembly_error_with_code(
            ApiErrorCode::UploadSessionCorrupted,
            "verified opaque upload is missing blob hash",
        )
    })?;
    let result = storage::finalize_upload_session_file(
        state,
        storage::FinalizeUploadSessionFileParams {
            session,
            file_hash,
            size: verified.size(),
            policy_id: verified.policy_id(),
            storage_path: verified.storage_path(),
            now: Utc::now(),
            actor_username,
        },
    )
    .await;
    if result.is_err() {
        cleanup_verified_upload_after_db_failure(
            driver,
            verified,
            "opaque upload DB finalize error",
        )
        .await;
    }
    result
}

fn opaque_upload_file_hash(
    policy: &storage_policy::Model,
    session: &upload_session::Model,
) -> Result<String> {
    Ok(format!(
        "{}-{}",
        opaque_blob_hash_prefix(policy)?,
        session.id
    ))
}

fn opaque_blob_hash_prefix(policy: &storage_policy::Model) -> Result<&'static str> {
    storage::resolve_policy_upload_transport(policy)?
        .opaque_blob_hash_prefix()
        .ok_or_else(|| {
            upload_assembly_error_with_code(
                ApiErrorCode::UploadSessionCorrupted,
                format!(
                    "storage policy driver '{}' cannot finalize opaque upload sessions without an opaque hash prefix",
                    policy.driver_type.as_str()
                ),
            )
        })
}

fn presigned_final_storage_path() -> String {
    format!("files/{}", uuid::Uuid::new_v4())
}

async fn copy_presigned_object_to_final_key(
    driver: &dyn StorageDriver,
    temp_key: &str,
    declared_size: i64,
    verified_temp_size: i64,
) -> Result<(String, i64)> {
    let requested_final_key = presigned_final_storage_path();
    let final_key = driver.copy_object(temp_key, &requested_final_key).await?;
    let final_size = ensure_uploaded_object_size(
        driver,
        &final_key,
        declared_size,
        "final uploaded object not found after presigned copy",
    )
    .await?;
    if final_size != verified_temp_size {
        return Err(upload_assembly_error_with_code(
            ApiErrorCode::UploadFinalObjectSizeMismatch,
            format!(
                "final object size mismatch: temp object {} bytes, final object {} bytes",
                verified_temp_size, final_size
            ),
        ));
    }
    Ok((final_key, final_size))
}

async fn complete_object_multipart_upload_session(
    state: &PrimaryAppState,
    session: upload_session::Model,
    expected_status: UploadSessionStatus,
    mut completed_parts: Vec<(i32, String)>,
    missing_message: &str,
    actor_username: Option<&str>,
) -> Result<file::Model> {
    let db = state.writer_db();
    // Historical column names: object multipart support is shared by S3-like
    // drivers and remote providers even though the DB fields still say `s3`.
    let temp_key = session.object_temp_key.as_deref().ok_or_else(|| {
        upload_assembly_error_with_code(
            ApiErrorCode::UploadSessionCorrupted,
            "missing object_temp_key",
        )
    })?;
    let multipart_id = session.object_multipart_id.as_deref().ok_or_else(|| {
        upload_assembly_error_with_code(
            ApiErrorCode::UploadSessionCorrupted,
            "missing object_multipart_id",
        )
    })?;

    let policy = state
        .policy_snapshot()
        .get_policy_or_err(session.policy_id)?;
    let driver = state.driver_registry().get_driver(&policy)?;
    let multipart = state.driver_registry().get_multipart_driver(&policy)?;
    let driver_ref: &dyn StorageDriver = driver.as_ref();

    tracing::debug!(
        upload_id = %session.id,
        status = ?session.status,
        expected_status = ?expected_status,
        policy_id = policy.id,
        part_count = completed_parts.len(),
        "completing multipart upload session"
    );

    run_upload_completion_stage(
        db,
        &session,
        expected_status,
        "completed multipart upload session",
        async {
            completed_parts.sort_by_key(|(part_number, _)| *part_number);
            let actual_part_size = verify_uploaded_multipart_parts(
                multipart.as_ref(),
                temp_key,
                multipart_id,
                &session,
                &completed_parts,
            )
            .await;
            let actual_part_size = match actual_part_size {
                Ok(actual_part_size) => actual_part_size,
                Err(error) => {
                    if should_abort_multipart_after_preflight_error(&error) {
                        abort_multipart_upload_after_preflight_failure(
                            multipart.as_ref(),
                            temp_key,
                            multipart_id,
                            &session.id,
                        )
                        .await;
                    }
                    return Err(error);
                }
            };

            if let Err(error) =
                storage::check_quota(db, workspace_scope_from_session(&session), actual_part_size)
                    .await
            {
                if should_abort_multipart_after_preflight_error(&error) {
                    abort_multipart_upload_after_preflight_failure(
                        multipart.as_ref(),
                        temp_key,
                        multipart_id,
                        &session.id,
                    )
                    .await;
                }
                return Err(error);
            }

            // multipart complete 之前要先把 part 列表排序；驱动层依赖有序 part 序列。
            if let Err(error) = multipart
                .complete_multipart_upload(temp_key, multipart_id, completed_parts)
                .await
            {
                // 远端节点可能已经完成了 multipart，但最终响应在返回前丢了。
                // 这时继续按已落盘对象收尾，避免把可恢复的上传直接打成 failed。
                if upload_completion_error_is_retryable(&error)
                    && let Ok(actual_size) = ensure_uploaded_object_size(
                        driver_ref,
                        temp_key,
                        session.total_size,
                        missing_message,
                    )
                    .await
                {
                    let verified = VerifiedUploadedBlob::completed_multipart_object(
                        actual_size,
                        policy.id,
                        temp_key.to_string(),
                        opaque_upload_file_hash(&policy, &session)?,
                    )?;
                    return finalize_verified_opaque_upload_session(
                        state,
                        &session,
                        driver_ref,
                        &verified,
                        actor_username,
                    )
                    .await;
                }
                return Err(error);
            }

            let actual_size = ensure_uploaded_object_size(
                driver_ref,
                temp_key,
                session.total_size,
                missing_message,
            )
            .await?;

            let verified = VerifiedUploadedBlob::completed_multipart_object(
                actual_size,
                policy.id,
                temp_key.to_string(),
                opaque_upload_file_hash(&policy, &session)?,
            )?;
            finalize_verified_opaque_upload_session(
                state,
                &session,
                driver_ref,
                &verified,
                actor_username,
            )
            .await
        },
    )
    .await
}

fn should_abort_multipart_after_preflight_error(error: &AsterError) -> bool {
    matches!(error, AsterError::StorageQuotaExceeded(_))
        || matches!(
            error.api_error_code(),
            ApiErrorCode::UploadIncompleteParts
                | ApiErrorCode::UploadMissingPart
                | ApiErrorCode::UploadTempObjectSizeMismatch
        )
}

async fn verify_uploaded_multipart_parts(
    multipart: &dyn MultipartStorageDriver,
    temp_key: &str,
    multipart_id: &str,
    session: &upload_session::Model,
    completed_parts: &[(i32, String)],
) -> Result<i64> {
    let mut uploaded_parts = multipart
        .list_uploaded_part_details(temp_key, multipart_id)
        .await?;
    uploaded_parts.sort_by_key(|part| part.part_number);
    validate_uploaded_part_numbers(session, completed_parts, &uploaded_parts)?;
    let actual_size = sum_uploaded_part_sizes(&uploaded_parts)?;
    if actual_size != session.total_size {
        return Err(upload_assembly_error_with_code(
            ApiErrorCode::UploadTempObjectSizeMismatch,
            format!(
                "multipart size mismatch: declared {} but uploaded parts total {}",
                session.total_size, actual_size
            ),
        ));
    }
    Ok(actual_size)
}

fn validate_uploaded_part_numbers(
    session: &upload_session::Model,
    completed_parts: &[(i32, String)],
    uploaded_parts: &[UploadedMultipartPart],
) -> Result<()> {
    if completed_parts.len() != uploaded_parts.len() {
        return Err(upload_assembly_error_with_code(
            ApiErrorCode::UploadIncompleteParts,
            format!(
                "expected {} completed parts, got {} uploaded parts",
                completed_parts.len(),
                uploaded_parts.len()
            ),
        ));
    }

    let expected_parts =
        crate::utils::numbers::i32_to_usize(session.total_chunks, "upload session total_chunks")?;
    if completed_parts.len() != expected_parts {
        return Err(upload_assembly_error_with_code(
            ApiErrorCode::UploadIncompleteParts,
            format!(
                "expected {} parts, got {}",
                session.total_chunks,
                completed_parts.len()
            ),
        ));
    }

    for (expected_part_number, ((completed_part_number, _), uploaded_part)) in
        (1..=session.total_chunks).zip(completed_parts.iter().zip(uploaded_parts))
    {
        if *completed_part_number != expected_part_number {
            return Err(upload_assembly_error_with_code(
                ApiErrorCode::UploadMissingPart,
                format!(
                    "missing completed part {}; got {}",
                    expected_part_number, completed_part_number
                ),
            ));
        }
        if uploaded_part.part_number != expected_part_number {
            return Err(upload_assembly_error_with_code(
                ApiErrorCode::UploadMissingPart,
                format!(
                    "missing uploaded part {}; got {}",
                    expected_part_number, uploaded_part.part_number
                ),
            ));
        }
    }

    Ok(())
}

fn sum_uploaded_part_sizes(uploaded_parts: &[UploadedMultipartPart]) -> Result<i64> {
    uploaded_parts.iter().try_fold(0i64, |total, part| {
        if part.size < 0 {
            return Err(upload_assembly_error_with_code(
                ApiErrorCode::UploadTempObjectSizeMismatch,
                format!(
                    "multipart part {} has invalid negative size {}",
                    part.part_number, part.size
                ),
            ));
        }
        total.checked_add(part.size).ok_or_else(|| {
            upload_assembly_error_with_code(
                ApiErrorCode::UploadTempObjectSizeMismatch,
                "multipart uploaded part size total overflow",
            )
        })
    })
}

async fn abort_multipart_upload_after_preflight_failure(
    multipart: &dyn MultipartStorageDriver,
    temp_key: &str,
    multipart_id: &str,
    upload_id: &str,
) {
    if let Err(error) = multipart
        .abort_multipart_upload(temp_key, multipart_id)
        .await
    {
        tracing::warn!(
            upload_id = %upload_id,
            temp_key = %temp_key,
            "failed to abort multipart upload after preflight failure: {error}"
        );
    }
}

fn workspace_scope_from_session(session: &upload_session::Model) -> WorkspaceStorageScope {
    match session.team_id {
        Some(team_id) => WorkspaceStorageScope::Team {
            team_id,
            actor_user_id: session.user_id,
        },
        None => WorkspaceStorageScope::Personal {
            user_id: session.user_id,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{
        copy_presigned_object_to_final_key, ensure_uploaded_object_size,
        should_abort_multipart_after_preflight_error, sum_uploaded_part_sizes,
        validate_uploaded_part_numbers, verify_uploaded_multipart_parts,
    };
    use crate::api::api_error_code::ApiErrorCode;
    use crate::entities::upload_session;
    use crate::errors::{AsterError, Result, upload_assembly_error_with_code};
    use crate::storage::traits::UploadedMultipartPart;
    use crate::storage::{BlobMetadata, MultipartStorageDriver, StorageDriver};
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::io::AsyncRead;

    #[derive(Default)]
    struct CountingCopyDriver {
        metadata_paths: Mutex<Vec<String>>,
    }

    struct SizeMismatchDriver {
        size: u64,
        deleted_paths: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl StorageDriver for CountingCopyDriver {
        async fn put(&self, _path: &str, _data: &[u8]) -> Result<String> {
            unreachable!()
        }

        async fn get(&self, _path: &str) -> Result<Vec<u8>> {
            unreachable!()
        }

        async fn get_stream(&self, _path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
            unreachable!()
        }

        async fn delete(&self, _path: &str) -> Result<()> {
            unreachable!()
        }

        async fn exists(&self, _path: &str) -> Result<bool> {
            unreachable!("metadata errors should not trigger exists in this success path")
        }

        async fn metadata(&self, path: &str) -> Result<BlobMetadata> {
            self.metadata_paths
                .lock()
                .expect("metadata paths lock should not be poisoned")
                .push(path.to_string());
            Ok(BlobMetadata {
                size: 12,
                content_type: None,
            })
        }

        async fn copy_object(&self, _src_path: &str, dest_path: &str) -> Result<String> {
            Ok(dest_path.to_string())
        }
    }

    #[async_trait]
    impl StorageDriver for SizeMismatchDriver {
        async fn put(&self, _path: &str, _data: &[u8]) -> Result<String> {
            unreachable!()
        }

        async fn get(&self, _path: &str) -> Result<Vec<u8>> {
            unreachable!()
        }

        async fn get_stream(&self, _path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
            unreachable!()
        }

        async fn delete(&self, path: &str) -> Result<()> {
            self.deleted_paths
                .lock()
                .expect("deleted paths lock should not be poisoned")
                .push(path.to_string());
            Ok(())
        }

        async fn exists(&self, _path: &str) -> Result<bool> {
            unreachable!("metadata succeeds in this test")
        }

        async fn metadata(&self, _path: &str) -> Result<BlobMetadata> {
            Ok(BlobMetadata {
                size: self.size,
                content_type: None,
            })
        }

        async fn copy_object(&self, _src_path: &str, _dest_path: &str) -> Result<String> {
            unreachable!()
        }
    }

    struct ListingMultipartDriver {
        parts: Vec<UploadedMultipartPart>,
    }

    #[async_trait]
    impl MultipartStorageDriver for ListingMultipartDriver {
        async fn create_multipart_upload(&self, _path: &str) -> Result<String> {
            unreachable!()
        }

        async fn presigned_upload_part_url(
            &self,
            _path: &str,
            _upload_id: &str,
            _part_number: i32,
            _expires: Duration,
        ) -> Result<String> {
            unreachable!()
        }

        async fn complete_multipart_upload(
            &self,
            _path: &str,
            _upload_id: &str,
            _parts: Vec<(i32, String)>,
        ) -> Result<()> {
            unreachable!()
        }

        async fn upload_multipart_part(
            &self,
            _path: &str,
            _upload_id: &str,
            _part_number: i32,
            _data: &[u8],
        ) -> Result<String> {
            unreachable!()
        }

        async fn abort_multipart_upload(&self, _path: &str, _upload_id: &str) -> Result<()> {
            unreachable!()
        }

        async fn list_uploaded_part_details(
            &self,
            _path: &str,
            _upload_id: &str,
        ) -> Result<Vec<UploadedMultipartPart>> {
            Ok(self.parts.clone())
        }
    }

    #[tokio::test]
    async fn copy_presigned_object_to_final_key_reuses_verified_temp_size() {
        let driver = Arc::new(CountingCopyDriver::default());

        let (final_key, final_size) =
            copy_presigned_object_to_final_key(driver.as_ref(), "temp/object", 12, 12)
                .await
                .expect("copy should succeed");

        assert_eq!(final_size, 12);
        assert_ne!(final_key, "temp/object");
        let metadata_paths = driver
            .metadata_paths
            .lock()
            .expect("metadata paths lock should not be poisoned")
            .clone();
        assert_eq!(
            metadata_paths,
            vec![final_key],
            "temp object metadata should be reused instead of fetched again"
        );
    }

    #[tokio::test]
    async fn ensure_uploaded_object_size_rejects_mismatch_and_deletes_temp_object() {
        let driver = SizeMismatchDriver {
            size: 7,
            deleted_paths: Mutex::new(Vec::new()),
        };

        let error = ensure_uploaded_object_size(&driver, "temp/object", 12, "missing object")
            .await
            .expect_err("metadata size mismatch should fail");

        assert_eq!(
            error.api_error_code_override(),
            Some(ApiErrorCode::UploadTempObjectSizeMismatch)
        );
        let deleted_paths = driver
            .deleted_paths
            .lock()
            .expect("deleted paths lock should not be poisoned")
            .clone();
        assert_eq!(deleted_paths, vec!["temp/object"]);
    }

    fn test_session(total_size: i64, total_chunks: i32) -> upload_session::Model {
        upload_session::Model {
            id: "session".to_string(),
            user_id: 1,
            team_id: None,
            frontend_client_id: None,
            filename: "file.bin".to_string(),
            total_size,
            chunk_size: 5,
            total_chunks,
            received_count: 0,
            folder_id: None,
            policy_id: 1,
            status: crate::types::UploadSessionStatus::Assembling,
            object_temp_key: Some("temp".to_string()),
            object_multipart_id: Some("multipart".to_string()),
            file_id: None,
            created_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn validate_uploaded_part_numbers_rejects_extra_uploaded_part() {
        let session = test_session(10, 2);
        let completed_parts = vec![(1, "etag-1".to_string()), (2, "etag-2".to_string())];
        let uploaded_parts = vec![
            UploadedMultipartPart {
                part_number: 1,
                size: 5,
            },
            UploadedMultipartPart {
                part_number: 2,
                size: 5,
            },
            UploadedMultipartPart {
                part_number: 3,
                size: 5,
            },
        ];

        let error = validate_uploaded_part_numbers(&session, &completed_parts, &uploaded_parts)
            .expect_err("extra provider part should be rejected");
        assert!(error.to_string().contains("uploaded parts"));
    }

    #[test]
    fn validate_uploaded_part_numbers_requires_sequential_parts() {
        let session = test_session(10, 2);
        let completed_parts = vec![(2, "etag-2".to_string()), (3, "etag-3".to_string())];
        let uploaded_parts = vec![
            UploadedMultipartPart {
                part_number: 2,
                size: 5,
            },
            UploadedMultipartPart {
                part_number: 3,
                size: 5,
            },
        ];

        let error = validate_uploaded_part_numbers(&session, &completed_parts, &uploaded_parts)
            .expect_err("non-sequential parts should be rejected");
        assert!(error.to_string().contains("missing completed part 1"));
    }

    #[tokio::test]
    async fn verify_uploaded_multipart_parts_accepts_exact_provider_sizes() {
        let session = test_session(12, 3);
        let completed_parts = vec![
            (1, "etag-1".to_string()),
            (2, "etag-2".to_string()),
            (3, "etag-3".to_string()),
        ];
        let multipart = ListingMultipartDriver {
            parts: vec![
                UploadedMultipartPart {
                    part_number: 3,
                    size: 2,
                },
                UploadedMultipartPart {
                    part_number: 1,
                    size: 5,
                },
                UploadedMultipartPart {
                    part_number: 2,
                    size: 5,
                },
            ],
        };

        let actual_size = verify_uploaded_multipart_parts(
            &multipart,
            "temp-key",
            "multipart-id",
            &session,
            &completed_parts,
        )
        .await
        .expect("exact provider part sizes should verify");

        assert_eq!(actual_size, 12);
    }

    #[tokio::test]
    async fn verify_uploaded_multipart_parts_rejects_declared_size_mismatch() {
        let session = test_session(11, 2);
        let completed_parts = vec![(1, "etag-1".to_string()), (2, "etag-2".to_string())];
        let multipart = ListingMultipartDriver {
            parts: vec![
                UploadedMultipartPart {
                    part_number: 1,
                    size: 5,
                },
                UploadedMultipartPart {
                    part_number: 2,
                    size: 7,
                },
            ],
        };

        let error = verify_uploaded_multipart_parts(
            &multipart,
            "temp-key",
            "multipart-id",
            &session,
            &completed_parts,
        )
        .await
        .expect_err("provider size sum must match declared session size");

        assert!(error.to_string().contains("multipart size mismatch"));
    }

    #[test]
    fn sum_uploaded_part_sizes_rejects_overflow() {
        let parts = vec![
            UploadedMultipartPart {
                part_number: 1,
                size: i64::MAX,
            },
            UploadedMultipartPart {
                part_number: 2,
                size: 1,
            },
        ];

        let error = sum_uploaded_part_sizes(&parts).expect_err("overflow should be rejected");
        assert!(error.to_string().contains("overflow"));
    }

    #[test]
    fn sum_uploaded_part_sizes_rejects_negative_size() {
        let parts = vec![UploadedMultipartPart {
            part_number: 1,
            size: -1,
        }];

        let error = sum_uploaded_part_sizes(&parts).expect_err("negative size should be rejected");
        assert!(error.to_string().contains("negative size"));
    }

    #[test]
    fn preflight_abort_policy_keeps_transient_provider_errors_retryable() {
        let transient = AsterError::storage_driver_error("provider list_parts failed");
        assert!(!should_abort_multipart_after_preflight_error(&transient));
    }

    #[test]
    fn preflight_abort_policy_aborts_deterministic_consistency_errors() {
        let mismatch = upload_assembly_error_with_code(
            ApiErrorCode::UploadTempObjectSizeMismatch,
            "multipart size mismatch",
        );
        let quota = AsterError::storage_quota_exceeded("quota exceeded");

        assert!(should_abort_multipart_after_preflight_error(&mismatch));
        assert!(should_abort_multipart_after_preflight_error(&quota));
    }
}

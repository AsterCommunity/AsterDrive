use chrono::Utc;

use crate::api::subcode::ApiSubcode;
use crate::db::repository::upload_session_part_repo;
use crate::entities::{file, upload_session};
use crate::errors::{Result, upload_assembly_error_with_subcode};
use crate::runtime::PrimaryAppState;
use crate::services::upload_service::shared::{
    run_upload_completion_stage, upload_completion_error_is_retryable,
};
use crate::services::workspace_storage_service;
use crate::storage::driver::StorageDriver;
use crate::types::UploadSessionStatus;
use crate::utils::numbers::u64_to_i64;

pub(super) async fn complete_presigned_upload(
    state: &PrimaryAppState,
    session: upload_session::Model,
    actor_username: Option<&str>,
) -> Result<file::Model> {
    // presigned 单文件的 complete 阶段，本质是“确认对象存在且大小正确”，
    // 然后把 temp_key 直接认领成正式 blob。
    let db = &state.db;
    let temp_key = session.s3_temp_key.as_deref().ok_or_else(|| {
        upload_assembly_error_with_subcode(
            ApiSubcode::UploadSessionCorrupted,
            "missing s3_temp_key",
        )
    })?;

    let policy = state.policy_snapshot.get_policy_or_err(session.policy_id)?;
    let driver = state.driver_registry.get_driver(&policy)?;
    let actual_size = ensure_uploaded_s3_object_size(
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
            let (final_key, actual_size) =
                copy_presigned_object_to_final_key(
                    driver.as_ref(),
                    temp_key,
                    session.total_size,
                    actual_size,
                )
                .await?;
            let file = match finalize_s3_upload_session(
                state,
                &session,
                policy.id,
                &final_key,
                actual_size,
                actor_username,
            )
            .await
            {
                Ok(file) => file,
                Err(error) => {
                    if let Err(cleanup_error) = driver.delete(&final_key).await {
                        tracing::warn!(
                            upload_id = %session.id,
                            final_key = %final_key,
                            "failed to delete copied presigned object after DB finalize error: {cleanup_error}"
                        );
                    }
                    return Err(error);
                }
            };
            if final_key != temp_key
                && let Err(error) = driver.delete(temp_key).await
            {
                tracing::warn!(
                    upload_id = %session.id,
                    temp_key = %temp_key,
                    final_key = %final_key,
                    "failed to delete presigned temp object after final copy: {error}"
                );
            }
            Ok(file)
        },
    )
    .await
}

/// 完成 presigned multipart 上传：complete multipart → 直接建文件记录
pub(super) async fn complete_s3_multipart(
    state: &PrimaryAppState,
    session: upload_session::Model,
    parts: Vec<(i32, String)>,
    actor_username: Option<&str>,
) -> Result<file::Model> {
    complete_s3_multipart_upload_session(
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
pub(super) async fn complete_s3_relay_multipart(
    state: &PrimaryAppState,
    session: upload_session::Model,
    actor_username: Option<&str>,
) -> Result<file::Model> {
    let db = &state.db;
    let parts = upload_session_part_repo::list_by_upload(db, &session.id).await?;
    let expected_parts =
        crate::utils::numbers::i32_to_usize(session.total_chunks, "upload session total_chunks")?;
    if parts.len() != expected_parts {
        return Err(upload_assembly_error_with_subcode(
            ApiSubcode::UploadIncompleteParts,
            format!(
                "expected {} parts, got {}",
                session.total_chunks,
                parts.len()
            ),
        ));
    }

    for (expected, part) in (1..=session.total_chunks).zip(parts.iter()) {
        if part.part_number != expected {
            return Err(upload_assembly_error_with_subcode(
                ApiSubcode::UploadMissingPart,
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
    complete_s3_multipart_upload_session(
        state,
        session,
        UploadSessionStatus::Uploading,
        completed_parts,
        "uploaded object not found after relay multipart complete - assembly may have failed",
        actor_username,
    )
    .await
}

async fn ensure_uploaded_s3_object_size(
    driver: &dyn StorageDriver,
    temp_key: &str,
    declared_size: i64,
    missing_message: &str,
) -> Result<i64> {
    let meta = match driver.metadata(temp_key).await {
        Ok(meta) => meta,
        Err(error) => match driver.exists(temp_key).await {
            Ok(false) => {
                return Err(upload_assembly_error_with_subcode(
                    ApiSubcode::UploadTempObjectMissing,
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
        return Err(upload_assembly_error_with_subcode(
            ApiSubcode::UploadTempObjectSizeMismatch,
            format!(
                "size mismatch: declared {} but uploaded {}",
                declared_size, actual_size
            ),
        ));
    }

    Ok(actual_size)
}

async fn finalize_s3_upload_session(
    state: &PrimaryAppState,
    session: &upload_session::Model,
    policy_id: i64,
    storage_path: &str,
    size: i64,
    actor_username: Option<&str>,
) -> Result<file::Model> {
    // 直传模式不会经过本地 assembled 文件，complete 阶段只负责把已经存在的对象
    // 记成 blob + file，并原子更新配额和 session 状态。
    workspace_storage_service::finalize_upload_session_file(
        state,
        workspace_storage_service::FinalizeUploadSessionFileParams {
            session,
            file_hash: &format!("s3-{}", session.id),
            size,
            policy_id,
            storage_path,
            now: Utc::now(),
            actor_username,
        },
    )
    .await
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
    let final_size = ensure_uploaded_s3_object_size(
        driver,
        &final_key,
        declared_size,
        "final uploaded object not found after presigned copy",
    )
    .await?;
    if final_size != verified_temp_size {
        return Err(upload_assembly_error_with_subcode(
            ApiSubcode::UploadFinalObjectSizeMismatch,
            format!(
                "final object size mismatch: temp object {} bytes, final object {} bytes",
                verified_temp_size, final_size
            ),
        ));
    }
    Ok((final_key, final_size))
}

async fn complete_s3_multipart_upload_session(
    state: &PrimaryAppState,
    session: upload_session::Model,
    expected_status: UploadSessionStatus,
    mut completed_parts: Vec<(i32, String)>,
    missing_message: &str,
    actor_username: Option<&str>,
) -> Result<file::Model> {
    let db = &state.db;
    let temp_key = session.s3_temp_key.as_deref().ok_or_else(|| {
        upload_assembly_error_with_subcode(
            ApiSubcode::UploadSessionCorrupted,
            "missing s3_temp_key",
        )
    })?;
    let multipart_id = session.s3_multipart_id.as_deref().ok_or_else(|| {
        upload_assembly_error_with_subcode(
            ApiSubcode::UploadSessionCorrupted,
            "missing s3_multipart_id",
        )
    })?;

    let policy = state.policy_snapshot.get_policy_or_err(session.policy_id)?;
    let driver = state.driver_registry.get_driver(&policy)?;
    let multipart = state.driver_registry.get_multipart_driver(&policy)?;
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
            // multipart complete 之前要先把 part 列表排序；驱动层依赖有序 part 序列。
            if let Err(error) = multipart
                .complete_multipart_upload(temp_key, multipart_id, completed_parts)
                .await
            {
                // 远端节点可能已经完成了 multipart，但最终响应在返回前丢了。
                // 这时继续按已落盘对象收尾，避免把可恢复的上传直接打成 failed。
                if upload_completion_error_is_retryable(&error)
                    && let Ok(actual_size) = ensure_uploaded_s3_object_size(
                        driver_ref,
                        temp_key,
                        session.total_size,
                        missing_message,
                    )
                    .await
                {
                    return finalize_s3_upload_session(
                        state,
                        &session,
                        policy.id,
                        temp_key,
                        actual_size,
                        actor_username,
                    )
                    .await;
                }
                return Err(error);
            }

            let actual_size = ensure_uploaded_s3_object_size(
                driver_ref,
                temp_key,
                session.total_size,
                missing_message,
            )
            .await?;

            finalize_s3_upload_session(
                state,
                &session,
                policy.id,
                temp_key,
                actual_size,
                actor_username,
            )
            .await
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::copy_presigned_object_to_final_key;
    use crate::errors::Result;
    use crate::storage::driver::{BlobMetadata, StorageDriver};
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};
    use tokio::io::AsyncRead;

    #[derive(Default)]
    struct CountingCopyDriver {
        metadata_paths: Mutex<Vec<String>>,
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
}

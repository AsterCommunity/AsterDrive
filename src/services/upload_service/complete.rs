//! 上传完成阶段。
//!
//! 这里把各种“临时上传状态”收口成正式文件：
//! - 本地 chunk 文件组装
//! - presigned 单文件确认
//! - presigned multipart 完成
//! - relay multipart 完成
//!
//! 目标都是在最后统一落到 `workspace_storage_service` 的文件创建语义上。

mod chunked;

use std::time::Instant;

use chrono::Utc;

use crate::db::repository::upload_session_part_repo;
use crate::entities::{file, upload_session};
use crate::errors::{
    AsterError, Result, upload_assembly_error_with_subcode, validation_error_with_subcode,
};
use crate::runtime::PrimaryAppState;
use crate::services::upload_service::scope::{load_upload_session, personal_scope, team_scope};
use crate::services::upload_service::shared::{
    find_file_by_session, run_upload_completion_stage, upload_completion_error_is_retryable,
};
use crate::services::{
    audit_service::{self, AuditContext},
    workspace_models::FileInfo,
    workspace_storage_service::{self},
};
use crate::storage::driver::StorageDriver;
use crate::types::UploadSessionStatus;
use crate::utils::numbers::u64_to_i64;

use self::chunked::complete_chunked_upload_with_actor_username;

#[derive(Clone, Copy, Default)]
struct CompleteUploadHints<'a> {
    actor_username: Option<&'a str>,
}

#[derive(Debug)]
enum CompletionPlan {
    ReturnCompleted,
    CompletePresigned,
    CompletePresignedMultipart { parts: Vec<(i32, String)> },
    CompleteRelayMultipart,
    AssembleChunks,
}

/// 完成分片上传：组装 → 按策略决定是否计算 hash / 去重 → 写入最终存储
async fn complete_upload_impl(
    state: &PrimaryAppState,
    session: upload_session::Model,
    parts: Option<Vec<(i32, String)>>,
) -> Result<file::Model> {
    complete_upload_impl_with_hints(state, session, parts, CompleteUploadHints::default()).await
}

async fn complete_upload_impl_with_hints(
    state: &PrimaryAppState,
    session: upload_session::Model,
    parts: Option<Vec<(i32, String)>>,
    hints: CompleteUploadHints<'_>,
) -> Result<file::Model> {
    tracing::debug!(
        upload_id = %session.id,
        status = ?session.status,
        received_count = session.received_count,
        total_chunks = session.total_chunks,
        has_parts = parts.as_ref().is_some_and(|items| !items.is_empty()),
        "completing upload session"
    );

    let upload_id = session.id.clone();
    let completed_retry = session.status == UploadSessionStatus::Completed;
    let complete_started_at = Instant::now();
    let plan = determine_completion_plan(&session, parts)?;
    let plan_label = completion_plan_label(&plan);
    let result = match plan {
        CompletionPlan::ReturnCompleted => find_file_by_session(&state.db, &session).await,
        CompletionPlan::CompletePresigned => {
            complete_presigned_upload(state, session, hints.actor_username).await
        }
        CompletionPlan::CompletePresignedMultipart { parts } => {
            complete_s3_multipart(state, session, parts, hints.actor_username).await
        }
        CompletionPlan::CompleteRelayMultipart => {
            complete_s3_relay_multipart(state, session, hints.actor_username).await
        }
        CompletionPlan::AssembleChunks => {
            complete_chunked_upload_with_actor_username(state, session, hints.actor_username).await
        }
    };
    tracing::debug!(
        upload_id = %upload_id,
        plan = plan_label,
        completed_retry,
        elapsed_ms = complete_started_at.elapsed().as_millis(),
        success = result.is_ok(),
        "upload completion plan finished"
    );
    result
}

fn determine_completion_plan(
    session: &upload_session::Model,
    parts: Option<Vec<(i32, String)>>,
) -> Result<CompletionPlan> {
    if session.status == UploadSessionStatus::Completed {
        return Ok(CompletionPlan::ReturnCompleted);
    }

    if session.status == UploadSessionStatus::Assembling {
        return Err(AsterError::upload_assembling(
            "upload is being processed, please wait and retry in a few seconds",
        ));
    }

    if session.expires_at <= Utc::now() {
        return Err(AsterError::upload_session_expired("session expired"));
    }

    if session.status == UploadSessionStatus::Failed {
        return Err(upload_assembly_error_with_subcode(
            "upload.previous_failure",
            "upload assembly failed previously; please start a new upload",
        ));
    }

    if session.status == UploadSessionStatus::Presigned {
        if session.s3_multipart_id.is_some() {
            let parts = parts.ok_or_else(|| {
                validation_error_with_subcode(
                    "upload.parts_required",
                    "parts required for multipart upload completion",
                )
            })?;
            return Ok(CompletionPlan::CompletePresignedMultipart { parts });
        }

        // presigned 单文件没有分片清单，只需要校验 temp object 真实存在且大小匹配。
        return Ok(CompletionPlan::CompletePresigned);
    }

    if session.status == UploadSessionStatus::Uploading && session.s3_multipart_id.is_some() {
        // relay multipart 的 completed parts 由服务端在 chunk 阶段自行收集，
        // complete 时无需客户端再次回传。
        return Ok(CompletionPlan::CompleteRelayMultipart);
    }

    if session.received_count != session.total_chunks {
        return Err(upload_assembly_error_with_subcode(
            "upload.incomplete_chunks",
            format!(
                "expected {} chunks, got {}",
                session.total_chunks, session.received_count
            ),
        ));
    }

    Ok(CompletionPlan::AssembleChunks)
}

fn completion_plan_label(plan: &CompletionPlan) -> &'static str {
    match plan {
        CompletionPlan::ReturnCompleted => "return_completed",
        CompletionPlan::CompletePresigned => "complete_presigned",
        CompletionPlan::CompletePresignedMultipart { .. } => "complete_presigned_multipart",
        CompletionPlan::CompleteRelayMultipart => "complete_relay_multipart",
        CompletionPlan::AssembleChunks => "assemble_chunks",
    }
}

pub async fn complete_upload(
    state: &PrimaryAppState,
    upload_id: &str,
    user_id: i64,
    parts: Option<Vec<(i32, String)>>,
) -> Result<FileInfo> {
    let load_started_at = Instant::now();
    let session = load_upload_session(state, personal_scope(user_id), upload_id).await?;
    tracing::debug!(
        upload_id,
        user_id,
        elapsed_ms = load_started_at.elapsed().as_millis(),
        "loaded upload session for completion"
    );
    complete_upload_impl(state, session, parts)
        .await
        .map(FileInfo::from)
}

pub async fn complete_upload_with_audit(
    state: &PrimaryAppState,
    upload_id: &str,
    user_id: i64,
    parts: Option<Vec<(i32, String)>>,
    audit_ctx: &AuditContext,
) -> Result<FileInfo> {
    let load_started_at = Instant::now();
    let session = load_upload_session(state, personal_scope(user_id), upload_id).await?;
    tracing::debug!(
        upload_id,
        user_id,
        elapsed_ms = load_started_at.elapsed().as_millis(),
        "loaded upload session for audited completion"
    );
    let scope = personal_scope(user_id);
    let actor_username = if should_log_upload_completion(&session) {
        Some(workspace_storage_service::load_scope_actor_username_cached(state, scope).await?)
    } else {
        None
    };
    complete_upload_impl_with_audit(
        state,
        session,
        parts,
        audit_ctx,
        CompleteUploadHints {
            actor_username: actor_username.as_deref(),
        },
    )
    .await
}

pub async fn complete_upload_for_team(
    state: &PrimaryAppState,
    team_id: i64,
    upload_id: &str,
    user_id: i64,
    parts: Option<Vec<(i32, String)>>,
) -> Result<FileInfo> {
    let load_started_at = Instant::now();
    let session = load_upload_session(state, team_scope(team_id, user_id), upload_id).await?;
    tracing::debug!(
        upload_id,
        team_id,
        user_id,
        elapsed_ms = load_started_at.elapsed().as_millis(),
        "loaded team upload session for completion"
    );
    complete_upload_impl(state, session, parts)
        .await
        .map(FileInfo::from)
}

pub async fn complete_upload_for_team_with_audit(
    state: &PrimaryAppState,
    team_id: i64,
    upload_id: &str,
    user_id: i64,
    parts: Option<Vec<(i32, String)>>,
    audit_ctx: &AuditContext,
) -> Result<FileInfo> {
    let load_started_at = Instant::now();
    let session = load_upload_session(state, team_scope(team_id, user_id), upload_id).await?;
    tracing::debug!(
        upload_id,
        team_id,
        user_id,
        elapsed_ms = load_started_at.elapsed().as_millis(),
        "loaded team upload session for audited completion"
    );
    let scope = team_scope(team_id, user_id);
    let actor_username = if should_log_upload_completion(&session) {
        Some(workspace_storage_service::load_scope_actor_username_cached(state, scope).await?)
    } else {
        None
    };
    complete_upload_impl_with_audit(
        state,
        session,
        parts,
        audit_ctx,
        CompleteUploadHints {
            actor_username: actor_username.as_deref(),
        },
    )
    .await
}

async fn complete_upload_impl_with_audit(
    state: &PrimaryAppState,
    session: upload_session::Model,
    parts: Option<Vec<(i32, String)>>,
    audit_ctx: &AuditContext,
    hints: CompleteUploadHints<'_>,
) -> Result<FileInfo> {
    let should_log = should_log_upload_completion(&session);
    let upload_id = session.id.clone();
    let complete_started_at = Instant::now();
    let file = complete_upload_impl_with_hints(state, session, parts, hints).await?;
    let complete_elapsed_ms = complete_started_at.elapsed().as_millis();
    if should_log {
        let audit_started_at = Instant::now();
        audit_service::log(
            state,
            audit_ctx,
            audit_service::AuditAction::FileUpload,
            Some("file"),
            Some(file.id),
            Some(&file.name),
            None,
        )
        .await;
        tracing::debug!(
            upload_id = %upload_id,
            file_id = file.id,
            complete_elapsed_ms,
            audit_elapsed_ms = audit_started_at.elapsed().as_millis(),
            total_elapsed_ms = complete_started_at.elapsed().as_millis(),
            "audited upload completion finished"
        );
    } else {
        tracing::debug!(
            upload_id = %upload_id,
            file_id = file.id,
            complete_elapsed_ms,
            total_elapsed_ms = complete_started_at.elapsed().as_millis(),
            "upload completion returned completed session without audit"
        );
    }
    Ok(file.into())
}

fn should_log_upload_completion(session: &upload_session::Model) -> bool {
    session.status != UploadSessionStatus::Completed
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
                    "upload.temp_object_missing",
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
            "upload.temp_object_size_mismatch",
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
) -> Result<(String, i64)> {
    ensure_uploaded_s3_object_size(
        driver,
        temp_key,
        declared_size,
        "uploaded object not found - upload may not have completed",
    )
    .await?;

    let requested_final_key = presigned_final_storage_path();
    let final_key = driver.copy_object(temp_key, &requested_final_key).await?;
    let final_size = ensure_uploaded_s3_object_size(
        driver,
        &final_key,
        declared_size,
        "final uploaded object not found after presigned copy",
    )
    .await?;
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
    let temp_key = session
        .s3_temp_key
        .as_deref()
        .ok_or_else(|| {
            upload_assembly_error_with_subcode("upload.session_corrupted", "missing s3_temp_key")
        })?
        .to_string();
    let multipart_id = session
        .s3_multipart_id
        .as_deref()
        .ok_or_else(|| {
            upload_assembly_error_with_subcode(
                "upload.session_corrupted",
                "missing s3_multipart_id",
            )
        })?
        .to_string();

    let policy = state.policy_snapshot.get_policy_or_err(session.policy_id)?;
    let driver = state.driver_registry.get_driver(&policy)?;
    let multipart = state.driver_registry.get_multipart_driver(&policy)?;
    let driver_ref: &dyn StorageDriver = driver.as_ref();
    let upload_id = session.id.clone();

    tracing::debug!(
        upload_id = %upload_id,
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
                .complete_multipart_upload(&temp_key, &multipart_id, completed_parts)
                .await
            {
                // 远端节点可能已经完成了 multipart，但最终响应在返回前丢了。
                // 这时继续按已落盘对象收尾，避免把可恢复的上传直接打成 failed。
                if upload_completion_error_is_retryable(&error)
                    && let Ok(actual_size) = ensure_uploaded_s3_object_size(
                        driver_ref,
                        &temp_key,
                        session.total_size,
                        missing_message,
                    )
                    .await
                {
                    return finalize_s3_upload_session(
                        state,
                        &session,
                        policy.id,
                        &temp_key,
                        actual_size,
                        actor_username,
                    )
                    .await;
                }
                return Err(error);
            }

            let actual_size = ensure_uploaded_s3_object_size(
                driver_ref,
                &temp_key,
                session.total_size,
                missing_message,
            )
            .await?;

            finalize_s3_upload_session(
                state,
                &session,
                policy.id,
                &temp_key,
                actual_size,
                actor_username,
            )
            .await
        },
    )
    .await
}

/// 完成 presigned 上传：校验预上传对象 → 直接建文件记录
async fn complete_presigned_upload(
    state: &PrimaryAppState,
    session: upload_session::Model,
    actor_username: Option<&str>,
) -> Result<file::Model> {
    // presigned 单文件的 complete 阶段，本质是“确认对象存在且大小正确”，
    // 然后把 temp_key 直接认领成正式 blob。
    let db = &state.db;
    let temp_key = session
        .s3_temp_key
        .as_deref()
        .ok_or_else(|| {
            upload_assembly_error_with_subcode("upload.session_corrupted", "missing s3_temp_key")
        })?
        .to_string();

    let policy = state.policy_snapshot.get_policy_or_err(session.policy_id)?;
    let driver = state.driver_registry.get_driver(&policy)?;
    ensure_uploaded_s3_object_size(
        driver.as_ref(),
        &temp_key,
        session.total_size,
        "uploaded object not found - upload may not have completed",
    )
    .await?;

    let upload_id = session.id.clone();
    tracing::debug!(
        upload_id = %upload_id,
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
                copy_presigned_object_to_final_key(driver.as_ref(), &temp_key, session.total_size)
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
                && let Err(error) = driver.delete(&temp_key).await
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
async fn complete_s3_multipart(
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
async fn complete_s3_relay_multipart(
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
            "upload.incomplete_parts",
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
                "upload.missing_part",
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

#[cfg(test)]
mod tests {
    use super::{CompletionPlan, determine_completion_plan, should_log_upload_completion};
    use crate::entities::upload_session;
    use crate::types::UploadSessionStatus;

    fn mock_session(status: UploadSessionStatus) -> upload_session::Model {
        upload_session::Model {
            id: "test-upload".to_string(),
            user_id: 1,
            team_id: None,
            filename: "demo.bin".to_string(),
            total_size: 12,
            chunk_size: 4,
            total_chunks: 3,
            received_count: 3,
            folder_id: None,
            policy_id: 1,
            status,
            s3_temp_key: None,
            s3_multipart_id: None,
            file_id: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        }
    }

    #[test]
    fn determine_completion_plan_marks_previous_failure_with_subcode() {
        let err = determine_completion_plan(&mock_session(UploadSessionStatus::Failed), None)
            .expect_err("failed session should not continue");

        assert_eq!(err.code(), "E057");
        assert_eq!(err.api_error_subcode(), Some("upload.previous_failure"));
    }

    #[test]
    fn determine_completion_plan_rejects_expired_active_session() {
        let mut session = mock_session(UploadSessionStatus::Presigned);
        session.expires_at = chrono::Utc::now() - chrono::Duration::seconds(1);

        let err =
            determine_completion_plan(&session, None).expect_err("expired session should fail");

        assert_eq!(err.code(), "E055");
    }

    #[test]
    fn determine_completion_plan_requires_parts_for_presigned_multipart() {
        let mut session = mock_session(UploadSessionStatus::Presigned);
        session.s3_multipart_id = Some("mp-1".to_string());

        let err =
            determine_completion_plan(&session, None).expect_err("multipart complete needs parts");

        assert_eq!(err.code(), "E005");
        assert_eq!(err.api_error_subcode(), Some("upload.parts_required"));
    }

    #[test]
    fn determine_completion_plan_marks_incomplete_chunks_with_subcode() {
        let mut session = mock_session(UploadSessionStatus::Uploading);
        session.received_count = 2;

        let err =
            determine_completion_plan(&session, None).expect_err("missing chunks should fail");

        assert_eq!(err.code(), "E057");
        assert_eq!(err.api_error_subcode(), Some("upload.incomplete_chunks"));
    }

    #[test]
    fn determine_completion_plan_returns_chunk_assembly_when_all_chunks_arrived() {
        let plan =
            determine_completion_plan(&mock_session(UploadSessionStatus::Uploading), None).unwrap();
        assert!(matches!(plan, CompletionPlan::AssembleChunks));
    }

    #[test]
    fn should_log_upload_completion_skips_completed_retry() {
        assert!(!should_log_upload_completion(&mock_session(
            UploadSessionStatus::Completed
        )));
        assert!(should_log_upload_completion(&mock_session(
            UploadSessionStatus::Presigned
        )));
        assert!(should_log_upload_completion(&mock_session(
            UploadSessionStatus::Uploading
        )));
    }
}

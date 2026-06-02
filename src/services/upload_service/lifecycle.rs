//! 上传服务子模块：`lifecycle`。

use chrono::{Duration, Utc};

use crate::db::repository::upload_session_repo;
use crate::entities::upload_session;
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::services::upload_service::scope::{load_upload_session, personal_scope, team_scope};
use crate::services::upload_service::shared::{
    UploadStorageErrorClass, classify_upload_storage_error, cleanup_upload_temp_dir,
    mark_session_failed_with_expiration, upload_storage_error_class_label,
};
use crate::storage::StorageDriver;
use crate::types::UploadSessionStatus;
use crate::utils::numbers::usize_to_u32;

const DEFERRED_UPLOAD_SESSION_CLEANUP_GRACE_SECS: i64 = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UploadRemoteCleanupOutcome {
    Complete,
    DeferredRetry,
    DeferredIntervention,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ForceCleanupByPolicyResult {
    pub cleaned: u64,
    pub deferred_temp_keys: Vec<String>,
    pub deferred_multipart_uploads: Vec<(String, String)>,
}

impl UploadRemoteCleanupOutcome {
    fn is_complete(self) -> bool {
        matches!(self, Self::Complete)
    }
}

fn blocked_cleanup_outcome(error: &AsterError) -> UploadRemoteCleanupOutcome {
    match classify_upload_storage_error(error) {
        UploadStorageErrorClass::Retryable => UploadRemoteCleanupOutcome::DeferredRetry,
        UploadStorageErrorClass::RequiresIntervention | UploadStorageErrorClass::Terminal => {
            UploadRemoteCleanupOutcome::DeferredIntervention
        }
        UploadStorageErrorClass::NotFound => UploadRemoteCleanupOutcome::Complete,
    }
}

fn log_blocked_remote_cleanup(
    session_id: &str,
    temp_key: Option<&str>,
    context: &str,
    error: &AsterError,
) -> UploadRemoteCleanupOutcome {
    let outcome = blocked_cleanup_outcome(error);
    if outcome.is_complete() {
        tracing::warn!(
            session_id,
            temp_key = temp_key.unwrap_or_default(),
            "{context}: remote object is already absent: {error}"
        );
        return outcome;
    }

    tracing::warn!(
        session_id,
        temp_key = temp_key.unwrap_or_default(),
        error_class = upload_storage_error_class_label(classify_upload_storage_error(error)),
        "{context}: remote cleanup is blocked, keeping session for follow-up: {error}"
    );
    outcome
}

async fn delete_temp_object_for_cleanup(
    driver: &dyn StorageDriver,
    session_id: &str,
    temp_key: &str,
    context: &str,
) -> UploadRemoteCleanupOutcome {
    match driver.delete(temp_key).await {
        Ok(()) => UploadRemoteCleanupOutcome::Complete,
        Err(error) => match driver.exists(temp_key).await {
            Ok(false) => {
                tracing::warn!(
                    session_id,
                    temp_key = %temp_key,
                    "{context}: delete returned error but object is already absent: {error}"
                );
                UploadRemoteCleanupOutcome::Complete
            }
            Ok(true) => log_blocked_remote_cleanup(session_id, Some(temp_key), context, &error),
            Err(exists_error) => {
                let outcome = blocked_cleanup_outcome(&error);
                tracing::warn!(
                    session_id,
                    temp_key = %temp_key,
                    error_class = upload_storage_error_class_label(classify_upload_storage_error(&error)),
                    "{context}: failed to delete temp object and verify existence, keeping session for follow-up: delete_error={error}, exists_error={exists_error}"
                );
                outcome
            }
        },
    }
}

async fn cleanup_remote_upload_state(
    state: &PrimaryAppState,
    session: &upload_session::Model,
) -> UploadRemoteCleanupOutcome {
    let Some(temp_key) = session.s3_temp_key.as_deref() else {
        return UploadRemoteCleanupOutcome::Complete;
    };

    let Some(policy) = state.policy_snapshot.get_policy(session.policy_id) else {
        tracing::warn!(
            session_id = %session.id,
            policy_id = session.policy_id,
            "failed to load storage policy for upload cleanup, keeping session for operator follow-up"
        );
        return UploadRemoteCleanupOutcome::DeferredIntervention;
    };

    let driver = match state.driver_registry.get_driver(&policy) {
        Ok(driver) => driver,
        Err(error) => {
            tracing::warn!(
                session_id = %session.id,
                policy_id = session.policy_id,
                error_class = upload_storage_error_class_label(classify_upload_storage_error(&error)),
                "failed to resolve storage driver for upload cleanup, keeping session for follow-up: {error}"
            );
            return blocked_cleanup_outcome(&error);
        }
    };

    if let Some(multipart_id) = session.s3_multipart_id.as_deref() {
        if let Some(multipart) = driver.as_multipart()
            && let Err(error) = multipart
                .abort_multipart_upload(temp_key, multipart_id)
                .await
        {
            let outcome = log_blocked_remote_cleanup(
                &session.id,
                Some(temp_key),
                "failed to abort multipart upload",
                &error,
            );
            if !outcome.is_complete() {
                return outcome;
            }
        }

        return delete_temp_object_for_cleanup(
            driver.as_ref(),
            &session.id,
            temp_key,
            "multipart upload cleanup",
        )
        .await;
    }

    delete_temp_object_for_cleanup(driver.as_ref(), &session.id, temp_key, "upload cleanup").await
}

async fn defer_upload_session_cleanup(
    state: &PrimaryAppState,
    upload_id: &str,
    reason: &str,
) -> Result<()> {
    let expires_at = Utc::now() + Duration::seconds(DEFERRED_UPLOAD_SESSION_CLEANUP_GRACE_SECS);
    mark_session_failed_with_expiration(state.writer_db(), upload_id, expires_at).await?;
    cleanup_upload_temp_dir(state, upload_id).await;
    tracing::debug!(
        upload_id,
        expires_at = %expires_at,
        reason,
        "deferred upload session cleanup"
    );
    Ok(())
}

/// 取消上传
async fn cancel_upload_impl(state: &PrimaryAppState, session: upload_session::Model) -> Result<()> {
    let upload_id = session.id.as_str();
    tracing::debug!(
        upload_id,
        status = ?session.status,
        policy_id = session.policy_id,
        has_temp_key = session.s3_temp_key.is_some(),
        has_multipart_id = session.s3_multipart_id.is_some(),
        "canceling upload session"
    );

    let defer_active_multipart_cleanup = session.s3_multipart_id.is_some()
        && matches!(session.status, UploadSessionStatus::Assembling);
    if defer_active_multipart_cleanup {
        defer_upload_session_cleanup(
            state,
            upload_id,
            "canceled assembling multipart upload session",
        )
        .await?;
        return Ok(());
    }

    let cleanup_outcome = cleanup_remote_upload_state(state, &session).await;
    if !cleanup_outcome.is_complete() {
        return defer_upload_session_cleanup(state, upload_id, "canceled upload cleanup blocked")
            .await;
    }

    cleanup_upload_temp_dir(state, upload_id).await;
    upload_session_repo::delete(state.writer_db(), upload_id).await?;
    tracing::debug!(upload_id, "canceled upload session");
    Ok(())
}

pub async fn cancel_upload(state: &PrimaryAppState, upload_id: &str, user_id: i64) -> Result<()> {
    let session = load_upload_session(state, personal_scope(user_id), upload_id).await?;
    let mode = upload_session_mode_label(&session);
    cancel_upload_impl(state, session)
        .await
        .inspect(|_| record_upload_cancel_metric(state, mode, true))
        .inspect_err(|_| record_upload_cancel_metric(state, mode, false))
}

pub async fn cancel_upload_for_team(
    state: &PrimaryAppState,
    team_id: i64,
    upload_id: &str,
    user_id: i64,
) -> Result<()> {
    let session = load_upload_session(state, team_scope(team_id, user_id), upload_id).await?;
    let mode = upload_session_mode_label(&session);
    cancel_upload_impl(state, session)
        .await
        .inspect(|_| record_upload_cancel_metric(state, mode, true))
        .inspect_err(|_| record_upload_cancel_metric(state, mode, false))
}

fn upload_session_mode_label(session: &upload_session::Model) -> &'static str {
    if session.status == UploadSessionStatus::Presigned {
        if session.s3_multipart_id.is_some() {
            return "presigned_multipart";
        }
        return "presigned";
    }
    if session.s3_multipart_id.is_some() {
        return "presigned_multipart";
    }
    "chunked"
}

fn record_upload_cancel_metric(state: &PrimaryAppState, mode: &'static str, success: bool) {
    state.metrics.record_upload_session_event(
        mode,
        "cancel",
        if success { "success" } else { "failure" },
    );
}

pub async fn force_cleanup_by_policy(
    state: &PrimaryAppState,
    policy_id: i64,
) -> Result<ForceCleanupByPolicyResult> {
    let sessions = upload_session_repo::find_by_policy(state.writer_db(), policy_id).await?;
    let mut result = ForceCleanupByPolicyResult::default();

    for session in &sessions {
        if let Some(temp_key) = session.s3_temp_key.as_ref() {
            result.deferred_temp_keys.push(temp_key.clone());
            if let Some(multipart_id) = session.s3_multipart_id.as_ref() {
                result
                    .deferred_multipart_uploads
                    .push((temp_key.clone(), multipart_id.clone()));
            }
        }

        let cleanup_outcome = cleanup_remote_upload_state(state, session).await;
        if !cleanup_outcome.is_complete() {
            return Err(AsterError::validation_error(format!(
                "cannot force delete policy: upload session {} still has remote cleanup pending",
                session.id
            )));
        }
    }

    for session in sessions {
        cleanup_upload_temp_dir(state, &session.id).await;
        upload_session_repo::delete(state.writer_db(), &session.id).await?;
        result.cleaned += 1;
    }

    Ok(result)
}

/// 清理过期的上传 session（后台任务调用）
pub async fn cleanup_expired(state: &PrimaryAppState) -> Result<u32> {
    let expired = upload_session_repo::find_expired(state.writer_db()).await?;
    let mut cleaned = 0usize;
    for session in expired {
        if session.status == UploadSessionStatus::Assembling {
            tracing::debug!(
                session_id = %session.id,
                "skipping expired upload session because assembly is in progress"
            );
            continue;
        }
        let cleanup_outcome = cleanup_remote_upload_state(state, &session).await;
        cleanup_upload_temp_dir(state, &session.id).await;
        if !cleanup_outcome.is_complete() {
            tracing::warn!(
                session_id = %session.id,
                status = ?session.status,
                cleanup_outcome = ?cleanup_outcome,
                "expired upload cleanup is incomplete, keeping session for follow-up"
            );
            continue;
        }

        if let Err(error) = upload_session_repo::delete(state.writer_db(), &session.id).await {
            tracing::warn!(
                "failed to delete expired upload session {}: {error}",
                session.id
            );
            continue;
        }
        cleaned += 1;
    }
    let count = usize_to_u32(cleaned, "expired upload session cleanup count")?;
    if count > 0 {
        tracing::info!("cleaned up {count} expired upload sessions");
    }
    Ok(count)
}

//! 上传完成阶段。
//!
//! 这里把各种“临时上传状态”收口成正式文件：
//! - 本地 chunk 文件组装
//! - presigned 单文件确认
//! - presigned object multipart 完成
//! - relay object multipart 完成
//!
//! 目标都是在最后统一落到 `workspace_storage_service` 的文件创建语义上。

mod audit;
mod chunked;
mod object_multipart;
mod plan;
#[cfg(test)]
mod tests;

use std::time::Instant;

use crate::entities::{file, upload_session};
use crate::errors::Result;
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::audit_service::AuditContext;
use crate::services::upload_service::scope::{load_upload_session, personal_scope, team_scope};
use crate::services::upload_service::shared::find_file_by_session;
use crate::services::{workspace_models::FileInfo, workspace_storage_service};
use crate::types::UploadSessionStatus;

use self::audit::{
    CompleteUploadHints, complete_upload_impl_with_audit, should_log_upload_completion,
};
use self::chunked::complete_chunked_upload_with_actor_username;
use self::object_multipart::{
    complete_presigned_multipart, complete_presigned_upload, complete_relay_multipart,
};
use self::plan::{CompletionPlan, completion_plan_label, determine_completion_plan};

const UNRESOLVED_UPLOAD_ACTOR_USERNAME: &str = "<unresolved>";

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
    let mode = upload_mode_label_from_completion_plan(&plan);
    let result = match plan {
        CompletionPlan::ReturnCompleted => find_file_by_session(state.writer_db(), &session).await,
        CompletionPlan::CompletePresigned => {
            complete_presigned_upload(state, session, hints.actor_username).await
        }
        CompletionPlan::CompletePresignedMultipart { parts } => {
            complete_presigned_multipart(state, session, parts, hints.actor_username).await
        }
        CompletionPlan::CompleteRelayMultipart => {
            complete_relay_multipart(state, session, hints.actor_username).await
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
    record_upload_completion_metric(state, mode, result.is_ok());
    result
}

fn record_upload_completion_metric(
    state: &impl SharedRuntimeState,
    mode: &'static str,
    success: bool,
) {
    let status = if success { "success" } else { "failure" };
    state
        .metrics()
        .record_upload_session_event(mode, "complete", status);
    state.metrics().record_file_upload(mode, status);
}

fn upload_mode_label_from_completion_plan(plan: &CompletionPlan) -> &'static str {
    match plan {
        CompletionPlan::CompletePresigned => "presigned",
        CompletionPlan::CompletePresignedMultipart { .. }
        | CompletionPlan::CompleteRelayMultipart => "presigned_multipart",
        CompletionPlan::AssembleChunks => "chunked",
        CompletionPlan::ReturnCompleted => "completed_retry",
    }
}

async fn load_upload_actor_username_best_effort(
    state: &PrimaryAppState,
    scope: crate::services::workspace_storage_service::WorkspaceStorageScope,
    upload_id: &str,
) -> String {
    match workspace_storage_service::load_scope_actor_username_cached(state, scope).await {
        Ok(username) => username,
        Err(error) => {
            tracing::warn!(
                upload_id,
                user_id = scope.actor_user_id(),
                "failed to load actor_username for upload finalization, continuing without attribution: {error}"
            );
            UNRESOLVED_UPLOAD_ACTOR_USERNAME.to_string()
        }
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
    let scope = personal_scope(user_id);
    let load_started_at = Instant::now();
    let session = load_upload_session(state, scope, upload_id).await?;
    tracing::debug!(
        upload_id,
        user_id,
        elapsed_ms = load_started_at.elapsed().as_millis(),
        "loaded upload session for audited completion"
    );
    let actor_username = if should_log_upload_completion(&session) {
        Some(load_upload_actor_username_best_effort(state, scope, &session.id).await)
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
    let scope = team_scope(team_id, user_id);
    let load_started_at = Instant::now();
    let session = load_upload_session(state, scope, upload_id).await?;
    tracing::debug!(
        upload_id,
        team_id,
        user_id,
        elapsed_ms = load_started_at.elapsed().as_millis(),
        "loaded team upload session for audited completion"
    );
    let actor_username = if should_log_upload_completion(&session) {
        Some(load_upload_actor_username_best_effort(state, scope, &session.id).await)
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

//! 上传完成阶段。
//!
//! 这里把各种“临时上传状态”收口成正式文件：
//! - offset staging file 校验，或兼容旧 session 的本地 chunk 文件组装
//! - presigned 单文件确认
//! - presigned object multipart 完成
//! - relay object multipart 完成
//!
//! 目标都是在最后统一落到 `workspace::storage` 的文件创建语义上。

mod audit;
mod chunked;
mod contract;
mod object_multipart;
mod plan;
mod provider_resumable;
#[cfg(test)]
mod tests;

use std::time::Instant;

use crate::entities::{file, upload_session};
use crate::errors::Result;
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::files::upload::kind::resolve_upload_session_kind;
use crate::services::files::upload::scope::{load_upload_session, personal_scope, team_scope};
use crate::services::files::upload::shared::find_file_by_session;
use crate::services::ops::audit::AuditContext;
use crate::services::{workspace::models::FileInfo, workspace::storage};
use crate::types::UploadSessionStatus;

use self::audit::{
    CompleteUploadHints, complete_upload_impl_with_audit, should_log_upload_completion,
};
use self::chunked::complete_chunked_upload_with_actor_username;
use self::object_multipart::{
    complete_presigned_multipart, complete_presigned_upload, complete_relay_multipart,
};
use self::plan::{CompletionPlan, completion_plan_label, determine_completion_plan};
use self::provider_resumable::complete_provider_resumable_upload;

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
    // Check terminal states before classifying legacy provider fields. A pre-migration row can
    // retain an old multipart id even after its policy snapshot has changed.
    let is_terminal = matches!(
        session.status,
        UploadSessionStatus::Completed
            | UploadSessionStatus::Assembling
            | UploadSessionStatus::Failed
    );
    let session_kind = if is_terminal {
        crate::types::UploadSessionKind::LegacyChunkFiles
    } else {
        resolve_upload_session_kind(state, &session).await?
    };
    let plan = determine_completion_plan(&session, session_kind, parts)?;
    let plan_label = completion_plan_label(&plan);
    let mode = if is_terminal {
        "completed_retry"
    } else {
        session_kind.as_str()
    };
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
        CompletionPlan::CompleteProviderResumable => {
            complete_provider_resumable_upload(state, session, hints.actor_username).await
        }
        CompletionPlan::CompleteChunked => {
            complete_chunked_upload_with_actor_username(
                state,
                session,
                session_kind,
                hints.actor_username,
            )
            .await
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

async fn load_upload_actor_username_best_effort(
    state: &PrimaryAppState,
    scope: crate::services::workspace::storage::WorkspaceStorageScope,
    upload_id: &str,
) -> String {
    match storage::load_scope_actor_username_cached(state, scope).await {
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

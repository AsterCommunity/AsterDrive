//! 后台任务服务子模块：`runtime`。

use chrono::{DateTime, Utc};
use sea_orm::Set;

use crate::db::repository::background_task_repo;
use crate::entities::background_task;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::types::{BackgroundTaskKind, BackgroundTaskStatus};

use super::types::{
    RuntimeTaskPayload, RuntimeTaskResult, serialize_task_payload, serialize_task_result,
};
use super::{task_expiration_from, truncate_error, truncate_status_text};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeTaskRunOutcome {
    Quiet,
    Succeeded {
        summary: Option<String>,
    },
    Failed {
        summary: Option<String>,
        error: String,
    },
}

impl RuntimeTaskRunOutcome {
    pub fn quiet() -> Self {
        Self::Quiet
    }

    pub fn succeeded(summary: Option<String>) -> Self {
        Self::Succeeded { summary }
    }

    pub fn failed(summary: Option<String>, error: impl Into<String>) -> Self {
        Self::Failed {
            summary,
            error: error.into(),
        }
    }

    fn should_record(&self) -> bool {
        !matches!(self, Self::Quiet)
    }

    fn status(&self) -> BackgroundTaskStatus {
        match self {
            Self::Quiet | Self::Succeeded { .. } => BackgroundTaskStatus::Succeeded,
            Self::Failed { .. } => BackgroundTaskStatus::Failed,
        }
    }

    fn summary(&self) -> Option<&str> {
        match self {
            Self::Quiet => None,
            Self::Succeeded { summary } | Self::Failed { summary, .. } => summary.as_deref(),
        }
    }

    fn error(&self) -> Option<&str> {
        match self {
            Self::Failed { error, .. } => Some(error.as_str()),
            Self::Quiet | Self::Succeeded { .. } => None,
        }
    }
}

pub async fn record_runtime_task_run(
    state: &PrimaryAppState,
    task_name: &str,
    started_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
    outcome: &RuntimeTaskRunOutcome,
) -> Result<Option<background_task::Model>> {
    if !outcome.should_record() {
        // Quiet 代表“这轮什么都没发生，不值得留痕”。
        // 比如 background-task-dispatch 空轮询时，不会每 5 秒往任务表里灌一行噪音数据。
        return Ok(None);
    }

    let payload_json = serialize_task_payload(&RuntimeTaskPayload {
        task_name: task_name.to_string(),
    })?;
    let summary = outcome.summary().map(truncate_status_text);
    let last_error = outcome.error().map(truncate_error);
    let result_json = serialize_task_result(&RuntimeTaskResult {
        duration_ms: (finished_at - started_at).num_milliseconds().max(0),
        summary: summary.clone(),
    })?;

    // 系统周期任务和用户后台任务共用 background_task 表。
    // 区别在于 runtime 任务的 kind 是 SystemRuntime，它们只是执行事件记录，
    // 不会再被 dispatcher 拿去执行。
    let task = background_task_repo::create(
        &state.db,
        background_task::ActiveModel {
            kind: Set(BackgroundTaskKind::SystemRuntime),
            status: Set(outcome.status()),
            creator_user_id: Set(None),
            team_id: Set(None),
            share_id: Set(None),
            display_name: Set(runtime_task_display_name(task_name)),
            payload_json: Set(payload_json),
            result_json: Set(Some(result_json)),
            steps_json: Set(None),
            progress_current: Set(if matches!(outcome, RuntimeTaskRunOutcome::Failed { .. }) {
                0
            } else {
                1
            }),
            progress_total: Set(1),
            status_text: Set(summary),
            attempt_count: Set(0),
            max_attempts: Set(1),
            next_run_at: Set(finished_at),
            processing_token: Set(0),
            processing_started_at: Set(None),
            last_heartbeat_at: Set(None),
            lease_expires_at: Set(None),
            started_at: Set(Some(started_at)),
            finished_at: Set(Some(finished_at)),
            last_error: Set(last_error),
            expires_at: Set(task_expiration_from(state, finished_at)),
            created_at: Set(started_at),
            updated_at: Set(finished_at),
            ..Default::default()
        },
    )
    .await?;

    Ok(Some(task))
}

fn runtime_task_display_name(task_name: &str) -> String {
    match task_name {
        "mail-outbox-dispatch" => "Mail outbox dispatch".to_string(),
        "background-task-dispatch" => "Background task dispatch".to_string(),
        "upload-cleanup" => "Upload cleanup".to_string(),
        "completed-upload-cleanup" => "Completed upload cleanup".to_string(),
        "blob-reconcile" => "Blob reconcile".to_string(),
        "system-health-check" => "System health check".to_string(),
        "remote-node-health-test" => "Remote node health test".to_string(),
        "trash-cleanup" => "Trash cleanup".to_string(),
        "team-archive-cleanup" => "Team archive cleanup".to_string(),
        "lock-cleanup" => "Lock cleanup".to_string(),
        "audit-cleanup" => "Audit log cleanup".to_string(),
        "task-cleanup" => "Task artifact cleanup".to_string(),
        "wopi-session-cleanup" => "WOPI session cleanup".to_string(),
        _ => task_name.replace('-', " "),
    }
}

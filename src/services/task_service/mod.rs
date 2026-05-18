//! 持久化后台任务子系统。
//!
//! 这里既管理用户可见的异步任务，也记录系统周期任务的执行结果。关键设计点是：
//! 任务状态留在数据库里，dispatcher 通过 lease + fencing token 防止旧 worker
//! 覆盖新 worker 的结果。

mod archive;
mod dispatch;
mod retry;
mod runtime;
mod steps;
mod storage_policy_cleanup;
mod thumbnail;
mod types;

use chrono::{Duration, Utc};
use parking_lot::Mutex;
use sea_orm::{DatabaseConnection, Set};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration as StdDuration, Instant};

use crate::api::pagination::{AdminTaskSortBy, OffsetPage, SortOrder};
use crate::api::subcode::ApiSubcode;
use crate::config::operations;
use crate::db::repository::background_task_repo;
use crate::entities::background_task;
use crate::errors::{AsterError, Result, precondition_failed_with_subcode};
use crate::runtime::PrimaryAppState;
use crate::services::{
    audit_service::{self, AuditContext},
    profile_service, user_service,
    workspace_storage_service::{self, WorkspaceStorageScope},
};
use crate::types::{BackgroundTaskKind, BackgroundTaskStatus, StoredTaskResult};
use crate::utils::numbers::{i64_to_i32, i64_to_u64};

pub(crate) use archive::ensure_archive_preview_task;
pub(crate) use archive::{
    create_archive_compress_task_in_scope, create_archive_extract_task_in_scope,
    prepare_archive_download_in_scope, stream_archive_download_in_scope,
};
pub use dispatch::{DispatchStats, cleanup_expired, dispatch_due, drain};
pub use runtime::{RuntimeTaskRunOutcome, record_runtime_task_run};
use steps::{initial_task_steps, parse_task_steps_json, serialize_task_steps};
pub(crate) use storage_policy_cleanup::create_storage_policy_temp_cleanup_task;
pub(crate) use thumbnail::ensure_thumbnail_task;
pub use types::{
    ArchiveCompressTaskPayload, ArchiveCompressTaskResult, ArchiveExtractTaskPayload,
    ArchiveExtractTaskResult, ArchivePreviewTaskPayload, ArchivePreviewTaskResult,
    CreateArchiveCompressTaskParams, CreateArchiveExtractTaskParams, CreateArchiveTaskParams,
    RuntimeSystemHealthComponent, RuntimeSystemHealthResult, RuntimeSystemHealthStatus,
    RuntimeTaskPayload, RuntimeTaskResult, TaskInfo, TaskPayload, TaskResult, TaskStepInfo,
    TaskStepStatus, ThumbnailGenerateTaskPayload, ThumbnailGenerateTaskResult,
};
use types::{parse_task_payload_info, parse_task_result_info, serialize_task_payload};

pub(super) const DEFAULT_TASK_RETENTION_HOURS: i64 = 24;
pub(super) const TASK_HEARTBEAT_INTERVAL_SECS: u64 = 10;
pub(super) const TASK_PROCESSING_STALE_SECS: i64 = 60;
pub(super) const TASK_LAST_ERROR_MAX_LEN: usize = 1024;
pub(super) const TASK_STATUS_TEXT_MAX_LEN: usize = 255;
pub(super) const TASK_DRAIN_MAX_ROUNDS: usize = 32;
const TASK_LEASE_LOST_MESSAGE_PREFIX: &str = "background task lease lost";
const TASK_LEASE_RENEWAL_TIMEOUT_MESSAGE_PREFIX: &str = "background task lease renewal timed out";

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct AdminTaskListFilters {
    pub(crate) kind: Option<BackgroundTaskKind>,
    pub(crate) status: Option<BackgroundTaskStatus>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AdminTaskCleanupFilters {
    pub(crate) finished_before: chrono::DateTime<chrono::Utc>,
    pub(crate) kind: Option<BackgroundTaskKind>,
    pub(crate) status: Option<BackgroundTaskStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TaskLease {
    // processing_token 是持久化的 fencing token，不跟进程生命周期绑定。
    // 任务每次被重新认领时都会拿到更大的 token，旧 worker 之后的写入必须被拒绝。
    // 这里的 lease 只表达“当前这次处理资格”，不表达任务本身的业务内容。
    pub(super) task_id: i64,
    pub(super) processing_token: i64,
}

impl TaskLease {
    pub(super) fn new(task_id: i64, processing_token: i64) -> Self {
        Self {
            task_id,
            processing_token,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct TaskLeaseGuard {
    // TaskLeaseGuard 是进程内的“租约看门狗”：
    // 1. 它持有当前 worker 的 task_id + processing_token；
    // 2. 只要心跳、进度写库、完成写库任意一次成功，就刷新 last_renewed_at；
    // 3. 如果 token 不再匹配，或者连续太久没有任何成功续约，就让当前执行流自我终止。
    //
    // processing_token 负责“防旧 worker 回写数据库”；
    // lease guard 负责“防旧 worker 在本地继续做副作用”。
    lease: TaskLease,
    renewal_timeout: StdDuration,
    state: Arc<Mutex<TaskLeaseGuardState>>,
}

#[derive(Debug)]
struct TaskLeaseGuardState {
    last_renewed_at: Instant,
    termination: Option<TaskLeaseTermination>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskLeaseTermination {
    Lost,
    RenewalTimedOut,
}

impl TaskLeaseGuard {
    pub(super) fn new(lease: TaskLease) -> Self {
        Self::with_renewal_timeout(lease, task_lease_renewal_timeout())
    }

    pub(super) fn with_renewal_timeout(lease: TaskLease, renewal_timeout: StdDuration) -> Self {
        Self {
            lease,
            renewal_timeout,
            state: Arc::new(Mutex::new(TaskLeaseGuardState {
                last_renewed_at: Instant::now(),
                termination: None,
            })),
        }
    }

    pub(super) fn lease(&self) -> TaskLease {
        self.lease
    }

    pub(super) fn record_renewed(&self) {
        let mut state = self.state.lock();
        if state.termination.is_none() {
            state.last_renewed_at = Instant::now();
        }
    }

    pub(super) fn mark_lost(&self) -> AsterError {
        let mut state = self.state.lock();
        state.termination = Some(TaskLeaseTermination::Lost);
        task_lease_lost(self.lease)
    }

    pub(super) fn ensure_active(&self) -> Result<()> {
        let mut state = self.state.lock();
        match state.termination {
            Some(TaskLeaseTermination::Lost) => return Err(task_lease_lost(self.lease)),
            Some(TaskLeaseTermination::RenewalTimedOut) => {
                return Err(task_lease_renewal_timed_out(self.lease));
            }
            None => {}
        }
        if state.last_renewed_at.elapsed() >= self.renewal_timeout {
            state.termination = Some(TaskLeaseTermination::RenewalTimedOut);
            return Err(task_lease_renewal_timed_out(self.lease));
        }
        Ok(())
    }
}

pub(crate) async fn list_tasks_paginated_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    limit: u64,
    offset: u64,
) -> Result<OffsetPage<TaskInfo>> {
    workspace_storage_service::require_scope_access(state, scope).await?;

    let limit = limit.clamp(1, operations::task_list_max_limit(&state.runtime_config));
    let (tasks, total) = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            background_task_repo::find_paginated_personal(&state.db, user_id, limit, offset).await?
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            background_task_repo::find_paginated_team(&state.db, team_id, limit, offset).await?
        }
    };

    let items = build_task_infos(state, tasks).await?;

    Ok(OffsetPage::new(items, total, limit, offset))
}

pub(crate) async fn list_tasks_paginated_for_admin(
    state: &PrimaryAppState,
    limit: u64,
    offset: u64,
    filters: AdminTaskListFilters,
    sort_by: AdminTaskSortBy,
    sort_order: SortOrder,
) -> Result<OffsetPage<TaskInfo>> {
    let limit = limit.clamp(1, operations::task_list_max_limit(&state.runtime_config));
    let (tasks, total) = background_task_repo::find_paginated_all_filtered(
        &state.db,
        limit,
        offset,
        &background_task_repo::AdminTaskFilters {
            kind: filters.kind,
            status: filters.status,
        },
        sort_by,
        sort_order,
    )
    .await?;

    let items = build_task_infos(state, tasks).await?;

    Ok(OffsetPage::new(items, total, limit, offset))
}

pub(crate) async fn cleanup_tasks_for_admin(
    state: &PrimaryAppState,
    filters: AdminTaskCleanupFilters,
) -> Result<u64> {
    validate_admin_task_cleanup_status(filters.status)?;
    background_task_repo::delete_terminal_by_filters(
        &state.db,
        &background_task_repo::TerminalTaskCleanupFilters {
            finished_before: filters.finished_before,
            kind: filters.kind,
            status: filters.status,
        },
    )
    .await
}

pub(crate) async fn get_task_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    task_id: i64,
) -> Result<TaskInfo> {
    workspace_storage_service::require_scope_access(state, scope).await?;
    let task = background_task_repo::find_by_id(&state.db, task_id).await?;
    ensure_task_in_scope(&task, scope)?;
    build_task_info_with_lookup(state, task).await
}

pub(crate) async fn retry_task_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    task_id: i64,
) -> Result<TaskInfo> {
    workspace_storage_service::require_scope_access(state, scope).await?;
    let task = background_task_repo::find_by_id(&state.db, task_id).await?;
    ensure_task_in_scope(&task, scope)?;

    if task.status != BackgroundTaskStatus::Failed {
        return Err(AsterError::validation_error(
            "only failed tasks can be retried",
        ));
    }
    if !task_can_retry(&task) {
        return Err(AsterError::validation_error(
            "this task failure cannot be retried",
        ));
    }

    cleanup_task_temp_dir_for_task(state, task.id).await?;
    // 手动重试会复用同一条任务记录，而不是新建“子任务”。
    // 这样前端和审计侧只需要跟踪一个稳定 task_id。
    let steps_json = serialize_task_steps(&initial_task_steps(task.kind))?;
    let max_attempts = configured_task_max_attempts(state, task.kind);

    let now = Utc::now();
    if !background_task_repo::reset_for_manual_retry(
        &state.db,
        task.id,
        now,
        max_attempts,
        Some(steps_json.as_ref()),
    )
    .await?
    {
        return Err(AsterError::internal_error(format!(
            "failed to reset task #{} for retry",
            task.id
        )));
    }
    state.wake_background_task_dispatcher();

    get_task_in_scope(state, scope, task_id).await
}

pub(crate) async fn retry_task_in_scope_with_audit(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    task_id: i64,
    audit_ctx: &AuditContext,
) -> Result<TaskInfo> {
    let previous = get_task_in_scope(state, scope, task_id).await?;
    let task = retry_task_in_scope(state, scope, task_id).await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::TaskRetry,
        crate::services::audit_service::AuditEntityType::Task,
        Some(task.id),
        Some(&task.display_name),
        audit_service::details(audit_service::TaskRetryAuditDetails {
            kind: format!("{:?}", previous.kind),
            previous_attempt_count: previous.attempt_count,
        }),
    )
    .await;
    Ok(task)
}

async fn build_task_infos(
    state: &PrimaryAppState,
    tasks: Vec<background_task::Model>,
) -> Result<Vec<TaskInfo>> {
    let creator_ids: Vec<i64> = tasks
        .iter()
        .filter_map(|task| task.creator_user_id)
        .collect();
    let creators = user_service::user_summaries_by_ids(
        state,
        &creator_ids,
        profile_service::AvatarAudience::AdminUser,
    )
    .await?;

    tasks
        .into_iter()
        .map(|task| build_task_info(task, &creators))
        .collect()
}

async fn build_task_info_with_lookup(
    state: &PrimaryAppState,
    task: background_task::Model,
) -> Result<TaskInfo> {
    let creator = match task.creator_user_id {
        Some(user_id) => {
            user_service::user_summary_by_id(
                state,
                user_id,
                profile_service::AvatarAudience::AdminUser,
            )
            .await?
        }
        None => None,
    };
    build_task_info_with_creator(task, creator)
}

fn build_task_info(
    task: background_task::Model,
    creators: &HashMap<i64, user_service::UserSummary>,
) -> Result<TaskInfo> {
    let creator = task
        .creator_user_id
        .and_then(|user_id| creators.get(&user_id).cloned());
    build_task_info_with_creator(task, creator)
}

fn build_task_info_with_creator(
    task: background_task::Model,
    creator: Option<user_service::UserSummary>,
) -> Result<TaskInfo> {
    // 数据库存的是通用 JSON 负载和步骤快照；这里统一把它们解包成 API 可读结构，
    // 让列表页和详情页不必了解任务种类内部的存储格式。
    let progress_percent = if task.progress_total <= 0 {
        if task.status == BackgroundTaskStatus::Succeeded {
            100
        } else {
            0
        }
    } else {
        i64_to_i32(
            ((task.progress_current.saturating_mul(100)) / task.progress_total).clamp(0, 100),
            "task progress percent",
        )?
    };
    let kind = task.kind;
    let payload = parse_task_payload_info(&task)?;
    let result = parse_task_result_info(&task)?;
    let steps = parse_task_steps_json(task.steps_json.as_ref().map(|raw| raw.as_ref()), kind)?;
    let can_retry = task_can_retry(&task);

    Ok(TaskInfo {
        id: task.id,
        kind,
        status: task.status,
        display_name: task.display_name,
        creator,
        team_id: task.team_id,
        share_id: task.share_id,
        progress_current: task.progress_current,
        progress_total: task.progress_total,
        progress_percent,
        status_text: task.status_text,
        attempt_count: task.attempt_count,
        max_attempts: task.max_attempts,
        last_error: task
            .last_error
            .map(|error| crate::errors::task_error_display_message(&error).to_string()),
        payload,
        result,
        steps,
        can_retry,
        lease_expires_at: task.lease_expires_at,
        started_at: task.started_at,
        finished_at: task.finished_at,
        expires_at: task.expires_at,
        created_at: task.created_at,
        updated_at: task.updated_at,
    })
}

fn task_can_retry(task: &background_task::Model) -> bool {
    task.status == BackgroundTaskStatus::Failed && task.failure_can_retry.unwrap_or(true)
}

pub(super) async fn create_task_record<T: Serialize>(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    kind: BackgroundTaskKind,
    display_name: &str,
    payload: &T,
) -> Result<background_task::Model> {
    let now = Utc::now();
    let payload_json = serialize_task_payload(payload)?;
    let steps_json = serialize_task_steps(&initial_task_steps(kind))?;

    // expires_at 代表“任务临时产物何时可以清理”，不是“任务记录何时删库”。
    // 我们保留 background_task 行作为历史留档；真正会按这个时间被清掉的是
    // temp/tasks/{task_id}/... 下面的中间产物。
    // 用户可见的新后台任务都应该走这个入口创建，这样 archive / 未来的
    // download_background 一类任务能自动唤醒 dispatcher，而不是等空闲退避 timer。
    let task = background_task_repo::create(
        &state.db,
        background_task::ActiveModel {
            kind: Set(kind),
            status: Set(BackgroundTaskStatus::Pending),
            creator_user_id: Set(Some(scope.actor_user_id())),
            team_id: Set(scope.team_id()),
            share_id: Set(None),
            display_name: Set(display_name.to_string()),
            payload_json: Set(payload_json),
            result_json: Set(None),
            steps_json: Set(Some(steps_json)),
            progress_current: Set(0),
            progress_total: Set(0),
            status_text: Set(None),
            attempt_count: Set(0),
            max_attempts: Set(configured_task_max_attempts(state, kind)),
            next_run_at: Set(now),
            processing_token: Set(0),
            processing_started_at: Set(None),
            last_heartbeat_at: Set(None),
            lease_expires_at: Set(None),
            started_at: Set(None),
            finished_at: Set(None),
            last_error: Set(None),
            expires_at: Set(task_expiration_from(state, now)),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await?;
    state.wake_background_task_dispatcher();
    Ok(task)
}

pub(super) fn task_scope(task: &background_task::Model) -> Result<WorkspaceStorageScope> {
    let actor_user_id = task.creator_user_id.ok_or_else(|| {
        AsterError::internal_error(format!("task #{} is missing creator_user_id", task.id))
    })?;
    Ok(match task.team_id {
        Some(team_id) => WorkspaceStorageScope::Team {
            team_id,
            actor_user_id,
        },
        None => WorkspaceStorageScope::Personal {
            user_id: actor_user_id,
        },
    })
}

pub(super) async fn mark_task_progress(
    state: &PrimaryAppState,
    lease_guard: &TaskLeaseGuard,
    current: i64,
    total: i64,
    status_text: Option<&str>,
    steps: &[TaskStepInfo],
) -> Result<()> {
    update_task_progress_db(&state.db, lease_guard, current, total, status_text, steps).await
}

pub(super) async fn update_task_progress_db(
    db: &DatabaseConnection,
    lease_guard: &TaskLeaseGuard,
    current: i64,
    total: i64,
    status_text: Option<&str>,
    steps: &[TaskStepInfo],
) -> Result<()> {
    let status_text = status_text.map(truncate_status_text);
    let steps_json = serialize_task_steps(steps)?;
    let lease = lease_guard.lease();
    let now = Utc::now();
    if background_task_repo::mark_progress(
        db,
        background_task_repo::TaskProgressUpdate {
            id: lease.task_id,
            processing_token: lease.processing_token,
            now,
            lease_expires_at: task_lease_expires_at(now),
            current,
            total,
            status_text: status_text.as_deref(),
            steps_json: Some(steps_json.as_ref()),
        },
    )
    .await?
    {
        lease_guard.record_renewed();
        Ok(())
    } else {
        Err(lease_guard.mark_lost())
    }
}

pub(super) async fn mark_task_succeeded(
    state: &PrimaryAppState,
    lease_guard: &TaskLeaseGuard,
    result_json: Option<&StoredTaskResult>,
    current: i64,
    total: i64,
    status_text: Option<&str>,
    steps: &[TaskStepInfo],
) -> Result<()> {
    let now = Utc::now();
    let status_text = status_text.map(truncate_status_text);
    let steps_json = serialize_task_steps(steps)?;
    let lease = lease_guard.lease();
    if background_task_repo::mark_succeeded(
        &state.db,
        background_task_repo::TaskSuccessUpdate {
            id: lease.task_id,
            processing_token: lease.processing_token,
            result_json: result_json.map(AsRef::as_ref),
            steps_json: Some(steps_json.as_ref()),
            current,
            total,
            status_text: status_text.as_deref(),
            finished_at: now,
            expires_at: task_expiration_from(state, now),
        },
    )
    .await?
    {
        lease_guard.record_renewed();
        Ok(())
    } else {
        Err(lease_guard.mark_lost())
    }
}

pub(super) async fn prepare_task_temp_dir(
    state: &PrimaryAppState,
    lease: TaskLease,
) -> Result<String> {
    // 临时目录按 task_id/token 隔离：
    // temp/tasks/{task_id}/{processing_token}
    //
    // 这样任务被 stale reclaim 后，新旧 worker 不会写进同一个目录。
    // 这里也只清当前 lease 的 token 目录，避免旧 worker 启动时把新 lease 的产物删掉。
    cleanup_task_temp_dir_for_lease(state, lease).await?;
    let task_temp_dir = crate::utils::paths::task_token_temp_dir(
        &state.config.server.temp_dir,
        lease.task_id,
        lease.processing_token,
    );
    tokio::fs::create_dir_all(&task_temp_dir)
        .await
        .map_err(|error| {
            AsterError::storage_driver_error(format!("create task temp dir: {error}"))
        })?;
    Ok(task_temp_dir)
}

pub(super) async fn cleanup_task_temp_dir_for_lease(
    state: &PrimaryAppState,
    lease: TaskLease,
) -> Result<()> {
    crate::utils::cleanup_temp_dir(&crate::utils::paths::task_token_temp_dir(
        &state.config.server.temp_dir,
        lease.task_id,
        lease.processing_token,
    ))
    .await;
    Ok(())
}

pub(super) async fn cleanup_task_temp_dir_for_task(
    state: &PrimaryAppState,
    task_id: i64,
) -> Result<()> {
    // 成功路径会删整个任务根目录，因为到这里说明已经没有活跃 lease 需要保留产物了。
    // 如果任务在失败/崩溃/重启中断时没走到这里，后续由 task-cleanup 周期任务兜底清理。
    crate::utils::cleanup_temp_dir(&crate::utils::paths::task_temp_dir(
        &state.config.server.temp_dir,
        task_id,
    ))
    .await;
    Ok(())
}

fn ensure_task_in_scope(task: &background_task::Model, scope: WorkspaceStorageScope) -> Result<()> {
    match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            if task.team_id.is_some() {
                return Err(AsterError::auth_forbidden(
                    "task belongs to a team workspace",
                ));
            }
            let creator_user_id = task.creator_user_id.ok_or_else(|| {
                AsterError::internal_error(format!("task #{} is missing creator_user_id", task.id))
            })?;
            crate::utils::verify_owner(creator_user_id, user_id, "task")?;
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            if task.team_id != Some(team_id) {
                return Err(AsterError::auth_forbidden("task is outside team workspace"));
            }
        }
    }

    Ok(())
}

pub(super) fn task_expiration_from(
    state: &PrimaryAppState,
    now: chrono::DateTime<chrono::Utc>,
) -> chrono::DateTime<chrono::Utc> {
    now + Duration::hours(load_task_retention_hours(state))
}

pub(super) fn task_lease_expires_at(
    now: chrono::DateTime<chrono::Utc>,
) -> chrono::DateTime<chrono::Utc> {
    now + Duration::seconds(TASK_PROCESSING_STALE_SECS.max(1))
}

fn configured_task_max_attempts(state: &PrimaryAppState, kind: BackgroundTaskKind) -> i32 {
    match kind {
        BackgroundTaskKind::SystemRuntime | BackgroundTaskKind::ThumbnailGenerate => 1,
        BackgroundTaskKind::ArchiveCompress
        | BackgroundTaskKind::ArchiveExtract
        | BackgroundTaskKind::ArchivePreviewGenerate
        | BackgroundTaskKind::StoragePolicyTempCleanup => {
            operations::background_task_max_attempts(&state.runtime_config)
        }
    }
}

fn validate_admin_task_cleanup_status(status: Option<BackgroundTaskStatus>) -> Result<()> {
    if status.is_some_and(|value| !value.is_terminal()) {
        return Err(AsterError::validation_error(
            "only completed task statuses can be cleaned up",
        ));
    }
    Ok(())
}

fn load_task_retention_hours(state: &PrimaryAppState) -> i64 {
    let Some(raw) = state.runtime_config.get("task_retention_hours") else {
        return DEFAULT_TASK_RETENTION_HOURS;
    };
    match raw.parse::<i64>() {
        Ok(hours) if hours > 0 => hours,
        _ => {
            tracing::warn!(
                "invalid task_retention_hours value '{}', using default",
                raw
            );
            DEFAULT_TASK_RETENTION_HOURS
        }
    }
}

pub(super) fn task_lease_lost(lease: TaskLease) -> AsterError {
    precondition_failed_with_subcode(
        ApiSubcode::TaskLeaseLost,
        format!(
            "{TASK_LEASE_LOST_MESSAGE_PREFIX} for task #{} with token {}",
            lease.task_id, lease.processing_token
        ),
    )
}

pub(super) fn task_lease_renewal_timed_out(lease: TaskLease) -> AsterError {
    precondition_failed_with_subcode(
        ApiSubcode::TaskLeaseRenewalTimedOut,
        format!(
            "{TASK_LEASE_RENEWAL_TIMEOUT_MESSAGE_PREFIX} for task #{} with token {}",
            lease.task_id, lease.processing_token
        ),
    )
}

pub(super) fn is_task_lease_lost(error: &AsterError) -> bool {
    error.api_error_subcode() == Some(ApiSubcode::TaskLeaseLost)
}

pub(super) fn is_task_lease_renewal_timed_out(error: &AsterError) -> bool {
    error.api_error_subcode() == Some(ApiSubcode::TaskLeaseRenewalTimedOut)
}

fn task_lease_renewal_timeout() -> StdDuration {
    let stale_secs = i64_to_u64(
        TASK_PROCESSING_STALE_SECS.max(1),
        "task processing stale seconds",
    )
    .unwrap_or(u64::MAX);
    let heartbeat_secs = TASK_HEARTBEAT_INTERVAL_SECS.max(1);
    StdDuration::from_secs(stale_secs.saturating_sub(heartbeat_secs).max(1))
}

pub(super) fn truncate_status_text(value: &str) -> String {
    value.chars().take(TASK_STATUS_TEXT_MAX_LEN).collect()
}

pub(super) fn truncate_error(error: &str) -> String {
    error.chars().take(TASK_LAST_ERROR_MAX_LEN).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        build_task_info_with_creator, ensure_task_in_scope, truncate_error,
        validate_admin_task_cleanup_status,
    };
    use crate::api::subcode::ApiSubcode;
    use crate::entities::background_task;
    use crate::services::workspace_storage_service::WorkspaceStorageScope;
    use crate::types::{BackgroundTaskKind, BackgroundTaskStatus, StoredTaskPayload};
    use chrono::{Duration, Utc};

    #[test]
    fn validate_admin_task_cleanup_status_accepts_terminal_statuses() {
        for status in [
            BackgroundTaskStatus::Succeeded,
            BackgroundTaskStatus::Failed,
            BackgroundTaskStatus::Canceled,
        ] {
            validate_admin_task_cleanup_status(Some(status))
                .expect("terminal task cleanup status should be accepted");
        }
    }

    #[test]
    fn validate_admin_task_cleanup_status_rejects_active_statuses() {
        let error = validate_admin_task_cleanup_status(Some(BackgroundTaskStatus::Processing))
            .expect_err("active task cleanup status should be rejected");
        assert!(error.message().contains("completed task statuses"));
    }

    #[test]
    fn task_info_hides_persisted_error_subcode_wrapper() {
        let now = Utc::now();
        let task = background_task::Model {
            id: 41,
            kind: BackgroundTaskKind::ArchiveCompress,
            status: BackgroundTaskStatus::Failed,
            creator_user_id: Some(7),
            team_id: None,
            share_id: None,
            display_name: "failed task".to_string(),
            payload_json: StoredTaskPayload(
                serde_json::json!({
                    "file_ids": [],
                    "folder_ids": [],
                    "archive_name": "files.zip",
                    "target_folder_id": null
                })
                .to_string(),
            ),
            result_json: None,
            steps_json: None,
            progress_current: 0,
            progress_total: 0,
            status_text: None,
            attempt_count: 1,
            max_attempts: 1,
            next_run_at: now,
            processing_token: 0,
            processing_started_at: None,
            last_heartbeat_at: None,
            lease_expires_at: None,
            started_at: None,
            finished_at: Some(now),
            last_error: Some(crate::errors::encode_api_error_subcode_message(
                ApiSubcode::ArchivePreviewInvalidZip,
                "invalid zip archive".to_string(),
            )),
            failure_can_retry: Some(false),
            expires_at: now + Duration::hours(1),
            created_at: now,
            updated_at: now,
        };

        let info = build_task_info_with_creator(task, None).expect("task info should build");

        assert_eq!(info.last_error.as_deref(), Some("invalid zip archive"));
    }

    #[test]
    fn task_error_encoding_preserves_subcode_before_truncation() {
        let error = crate::errors::validation_error_with_subcode(
            ApiSubcode::ArchivePreviewRejected,
            "archive contains unsafe path",
        );
        let stored = truncate_error(&crate::errors::encode_task_error_for_storage(&error));

        assert_eq!(
            crate::errors::task_error_subcode_from_storage(&stored),
            Some(ApiSubcode::ArchivePreviewRejected)
        );
        assert_eq!(
            crate::errors::task_error_display_message(&stored),
            "archive contains unsafe path"
        );
    }

    #[test]
    fn personal_task_scope_rejects_missing_creator_without_zero_sentinel() {
        let now = Utc::now();
        let task = background_task::Model {
            id: 42,
            kind: BackgroundTaskKind::ArchiveCompress,
            status: BackgroundTaskStatus::Failed,
            creator_user_id: None,
            team_id: None,
            share_id: None,
            display_name: "missing creator".to_string(),
            payload_json: StoredTaskPayload("{}".to_string()),
            result_json: None,
            steps_json: None,
            progress_current: 0,
            progress_total: 0,
            status_text: None,
            attempt_count: 0,
            max_attempts: 1,
            next_run_at: now,
            processing_token: 0,
            processing_started_at: None,
            last_heartbeat_at: None,
            lease_expires_at: None,
            started_at: None,
            finished_at: None,
            last_error: None,
            failure_can_retry: None,
            expires_at: now + Duration::hours(1),
            created_at: now,
            updated_at: now,
        };

        let error = ensure_task_in_scope(&task, WorkspaceStorageScope::Personal { user_id: 7 })
            .expect_err("missing creator must not be coerced to user id 0");

        assert_eq!(error.code(), "E004");
        assert!(error.message().contains("missing creator_user_id"));
    }

    #[test]
    fn team_task_scope_accepts_missing_creator_without_actor_sentinel() {
        let now = Utc::now();
        let task = background_task::Model {
            id: 43,
            kind: BackgroundTaskKind::ArchiveCompress,
            status: BackgroundTaskStatus::Failed,
            creator_user_id: None,
            team_id: Some(9),
            share_id: None,
            display_name: "team task".to_string(),
            payload_json: StoredTaskPayload("{}".to_string()),
            result_json: None,
            steps_json: None,
            progress_current: 0,
            progress_total: 0,
            status_text: None,
            attempt_count: 0,
            max_attempts: 1,
            next_run_at: now,
            processing_token: 0,
            processing_started_at: None,
            last_heartbeat_at: None,
            lease_expires_at: None,
            started_at: None,
            finished_at: None,
            last_error: None,
            failure_can_retry: None,
            expires_at: now + Duration::hours(1),
            created_at: now,
            updated_at: now,
        };

        ensure_task_in_scope(
            &task,
            WorkspaceStorageScope::Team {
                team_id: 9,
                actor_user_id: 7,
            },
        )
        .expect("team task scope should not require a fake creator user id");
    }
}

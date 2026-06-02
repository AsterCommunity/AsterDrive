//! 持久化后台任务子系统。
//!
//! 这里既管理用户可见的异步任务，也记录系统周期任务的执行结果。关键设计点是：
//! 任务状态留在数据库里，dispatcher 通过 lease + fencing token 防止旧 worker
//! 覆盖新 worker 的结果。
//!
//! ## 架构约束
//!
//! 后台任务的业务模块不要直接拼 `background_task::ActiveModel`，也不要手写
//! `payload_json` / `result_json`。任务类型、payload/result 编解码、初始 steps、
//! lane、max attempts、retry policy 和 process 入口都应先落到 `spec::BackgroundTaskSpec`，
//! 再通过 `registry` 和本模块的 typed create helpers 使用。
//!
//! 新增一种后台任务时，正常流程是：
//! 1. 在 `types.rs` 定义强类型 payload/result。
//! 2. 在 `spec.rs` 实现一个 `BackgroundTaskSpec`，声明 kind、steps、lane、process 等。
//! 3. 在 `registry.rs` 注册该 spec。
//! 4. 创建任务时使用 `create_typed_task_record` 或 `insert_typed_task_record`。
//!
//! 这样做是为了避免同一种任务同时存在“强类型路径”和“手写 JSON/ActiveModel 路径”。
//! 如果绕过这些入口，后续很容易出现 payload 缺字段、steps 不一致、max attempts
//! 和 dispatcher 行为分叉的问题。
//!
//! ## 导出约定
//!
//! 根模块只保留任务查询/重试、dispatcher、system runtime 这类稳定服务入口。
//! payload/result/列表 DTO 统一从 `task_service::types` 导入；各任务族的创建入口
//! 从对应子模块导入，例如 `task_service::archive`、`task_service::thumbnail`、
//! `task_service::storage_migration`。不要把新的任务类型或内部 helper 继续 re-export
//! 到根模块。

pub(crate) mod archive;
pub(crate) mod blob_maintenance;
pub(crate) mod dispatch;
pub(crate) mod media_metadata;
pub(crate) mod offline_download;
mod presentation;
mod registry;
mod retry;
pub(crate) mod runtime;
mod spec;
mod steps;
pub(crate) mod storage_migration;
pub(crate) mod storage_policy_cleanup;
pub(crate) mod thumbnail;
pub(crate) mod trash;
pub mod types;

use chrono::{Duration, Utc};
use parking_lot::Mutex;
use sea_orm::{ConnectionTrait, DatabaseConnection, Set};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration as StdDuration, Instant};
use tokio_util::sync::CancellationToken;

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
use crate::types::{BackgroundTaskKind, BackgroundTaskStatus, StoredTaskResult, StoredTaskSteps};
use crate::utils::numbers::{i64_to_i32, i64_to_u64};

pub use dispatch::{DispatchStats, cleanup_expired, dispatch_due, drain};
use registry::{build_task_presentation, decode_task_payload, decode_task_result};
pub use runtime::{RuntimeTaskRunOutcome, SystemRuntimeTaskKind, record_runtime_task_run};
use spec::BackgroundTaskSpec;
use steps::{parse_task_steps_json, serialize_task_steps};
use types::{TaskInfo, TaskPresentation, TaskResult, TaskStepInfo};

pub(super) const DEFAULT_TASK_RETENTION_HOURS: i64 = 24;
pub(super) const TASK_HEARTBEAT_INTERVAL_SECS: u64 = 10;
pub(super) const TASK_PROCESSING_STALE_SECS: i64 = 60;
pub(super) const TASK_DISPLAY_NAME_MAX_LEN: usize = 512;
pub(super) const TASK_LAST_ERROR_MAX_LEN: usize = 1024;
pub(super) const TASK_STATUS_TEXT_MAX_LEN: usize = 255;
pub(super) const TASK_DRAIN_MAX_ROUNDS: usize = 32;
const TASK_LEASE_LOST_MESSAGE_PREFIX: &str = "background task lease lost";
const TASK_LEASE_RENEWAL_TIMEOUT_MESSAGE_PREFIX: &str = "background task lease renewal timed out";
const TASK_WORKER_SHUTDOWN_REQUESTED_MESSAGE_PREFIX: &str =
    "background task worker shutdown requested";

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
    shutdown_token: Option<CancellationToken>,
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
    ShutdownRequested,
}

impl TaskLeaseGuard {
    pub(super) fn new(lease: TaskLease) -> Self {
        Self::with_renewal_timeout(lease, task_lease_renewal_timeout())
    }

    pub(super) fn with_renewal_timeout(lease: TaskLease, renewal_timeout: StdDuration) -> Self {
        Self {
            lease,
            renewal_timeout,
            shutdown_token: None,
            state: Arc::new(Mutex::new(TaskLeaseGuardState {
                last_renewed_at: Instant::now(),
                termination: None,
            })),
        }
    }

    fn with_shutdown_token(lease: TaskLease, shutdown_token: CancellationToken) -> Self {
        Self {
            shutdown_token: Some(shutdown_token),
            ..Self::new(lease)
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

    fn mark_shutdown_requested(&self) -> AsterError {
        let mut state = self.state.lock();
        state.termination = Some(TaskLeaseTermination::ShutdownRequested);
        task_worker_shutdown_requested(self.lease)
    }

    pub(super) fn ensure_active(&self) -> Result<()> {
        let mut state = self.state.lock();
        match state.termination {
            Some(TaskLeaseTermination::Lost) => return Err(task_lease_lost(self.lease)),
            Some(TaskLeaseTermination::RenewalTimedOut) => {
                return Err(task_lease_renewal_timed_out(self.lease));
            }
            Some(TaskLeaseTermination::ShutdownRequested) => {
                return Err(task_worker_shutdown_requested(self.lease));
            }
            None => {}
        }
        if self
            .shutdown_token
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
        {
            state.termination = Some(TaskLeaseTermination::ShutdownRequested);
            return Err(task_worker_shutdown_requested(self.lease));
        }
        if state.last_renewed_at.elapsed() >= self.renewal_timeout {
            state.termination = Some(TaskLeaseTermination::RenewalTimedOut);
            return Err(task_lease_renewal_timed_out(self.lease));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub(super) struct TaskExecutionContext {
    // Public task implementations should depend on this context, not on a bare
    // lease guard. The context keeps shutdown cancellation and lease fencing
    // together, so deep helper code cannot accidentally keep running after the
    // dispatcher has started graceful shutdown.
    //
    // The same cancellation token is intentionally held twice:
    // - TaskLeaseGuard uses it from synchronous ensure_active() checkpoints.
    // - TaskExecutionContext uses it in async select! paths, such as sleeps and
    //   stream reads, where waiting must be interrupted immediately.
    lease_guard: TaskLeaseGuard,
    shutdown_token: CancellationToken,
}

impl TaskExecutionContext {
    pub(super) fn new(lease: TaskLease, shutdown_token: CancellationToken) -> Self {
        Self {
            lease_guard: TaskLeaseGuard::with_shutdown_token(lease, shutdown_token.clone()),
            shutdown_token,
        }
    }

    #[cfg(test)]
    pub(super) fn with_lease_guard(
        lease_guard: TaskLeaseGuard,
        shutdown_token: CancellationToken,
    ) -> Self {
        Self {
            lease_guard,
            shutdown_token,
        }
    }

    pub(super) fn lease_guard(&self) -> &TaskLeaseGuard {
        &self.lease_guard
    }

    pub(super) fn ensure_active(&self) -> Result<()> {
        self.lease_guard.ensure_active()
    }

    pub(super) fn storage_operation_context(
        &self,
    ) -> workspace_storage_service::StorageOperationContext {
        workspace_storage_service::StorageOperationContext::new(TaskStorageCancellationCheck {
            context: self.clone(),
        })
    }

    pub(super) async fn sleep_or_shutdown(&self, duration: StdDuration) -> Result<()> {
        self.lease_guard.ensure_active()?;

        tokio::select! {
            biased;
            _ = self.shutdown_token.cancelled() => Err(self.lease_guard.mark_shutdown_requested()),
            _ = tokio::time::sleep(duration) => Ok(()),
        }
    }

    pub(super) async fn shutdown_requested(&self) -> Result<()> {
        self.shutdown_token.cancelled().await;
        Err(self.lease_guard.mark_shutdown_requested())
    }
}

#[derive(Clone)]
struct TaskStorageCancellationCheck {
    context: TaskExecutionContext,
}

impl workspace_storage_service::StorageCancellationCheck for TaskStorageCancellationCheck {
    fn checkpoint(&self) -> Result<()> {
        self.context.ensure_active()
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
            background_task_repo::find_paginated_personal(state.writer_db(), user_id, limit, offset)
                .await?
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            background_task_repo::find_paginated_team(state.writer_db(), team_id, limit, offset)
                .await?
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
        state.writer_db(),
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
        state.writer_db(),
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
    workspace_storage_service::require_scope_access_with_db(state, state.writer_db(), scope)
        .await?;
    let task = background_task_repo::find_by_id(state.writer_db(), task_id).await?;
    ensure_task_in_scope(&task, scope)?;
    build_task_info_with_lookup(state, task).await
}

pub(crate) async fn retry_task_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    task_id: i64,
) -> Result<TaskInfo> {
    workspace_storage_service::require_scope_access_with_db(state, state.writer_db(), scope)
        .await?;
    let task = background_task_repo::find_by_id(state.writer_db(), task_id).await?;
    ensure_task_in_scope(&task, scope)?;
    retry_task_record(state, &task).await?;

    get_task_in_scope(state, scope, task_id).await
}

async fn retry_task_record(state: &PrimaryAppState, task: &background_task::Model) -> Result<()> {
    if task.status != BackgroundTaskStatus::Failed {
        return Err(AsterError::validation_error(
            "only failed tasks can be retried",
        ));
    }
    if !task_can_retry(task) {
        return Err(AsterError::validation_error(
            "this task failure cannot be retried",
        ));
    }

    cleanup_task_temp_dir_for_task_kind(state, task.kind, task.id).await?;
    // 手动重试会复用同一条任务记录，而不是新建“子任务”。
    // 这样前端和审计侧只需要跟踪一个稳定 task_id。
    let steps_json = serialize_task_steps(&registry::initial_task_steps(task.kind))?;
    let max_attempts = registry::max_attempts(state, task.kind);

    let now = Utc::now();
    if !background_task_repo::reset_for_manual_retry(
        state.writer_db(),
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
    Ok(())
}

pub(crate) async fn retry_task_in_scope_with_audit(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    task_id: i64,
    audit_ctx: &AuditContext,
) -> Result<TaskInfo> {
    let previous = get_task_in_scope(state, scope, task_id).await?;
    let task = retry_task_in_scope(state, scope, task_id).await?;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::TaskRetry,
        crate::services::audit_service::AuditEntityType::Task,
        Some(task.id),
        Some(&task.display_name),
        || {
            audit_service::details(audit_service::TaskRetryAuditDetails {
                kind: format!("{:?}", previous.kind),
                previous_attempt_count: previous.attempt_count,
            })
        },
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
    let payload = decode_task_payload(&task)?;
    let result = decode_task_result_or_none(&task);
    let can_retry = task_can_retry(&task);
    let status_text = task.status_text;
    let presentation_context = task_presentation_context(kind, task.runtime_json.as_ref());
    let presentation = build_task_presentation(
        kind,
        &payload,
        result.as_ref(),
        task.status,
        presentation_context,
    )?;
    let steps = parse_task_steps_json(task.steps_json.as_ref().map(|raw| raw.as_ref()), kind)?;

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
        status_text,
        presentation,
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

pub(crate) fn build_task_presentation_for_model(
    task: &background_task::Model,
) -> Result<Option<TaskPresentation>> {
    let payload = decode_task_payload(task)?;
    let result = decode_task_result_or_none(task);
    build_task_presentation(
        task.kind,
        &payload,
        result.as_ref(),
        task.status,
        task_presentation_context(task.kind, task.runtime_json.as_ref()),
    )
}

fn task_presentation_context(
    kind: BackgroundTaskKind,
    runtime_json: Option<&crate::types::StoredTaskRuntime>,
) -> presentation::TaskPresentationContext {
    match kind {
        BackgroundTaskKind::OfflineDownload => presentation::TaskPresentationContext {
            selected_offline_download_engine: offline_download::selected_engine_from_runtime_json(
                runtime_json.map(|value| value.as_ref()),
            ),
        },
        _ => presentation::TaskPresentationContext::default(),
    }
}

fn decode_task_result_or_none(task: &background_task::Model) -> Option<TaskResult> {
    match decode_task_result(task) {
        Ok(result) => result,
        Err(error) => {
            tracing::warn!(
                task_id = task.id,
                error = %error,
                "failed to decode background task result; continuing without result"
            );
            None
        }
    }
}

fn task_can_retry(task: &background_task::Model) -> bool {
    task.status == BackgroundTaskStatus::Failed && task.failure_can_retry.unwrap_or(true)
}

pub(in crate::services::task_service) struct TypedTaskCreate<S: BackgroundTaskSpec> {
    display_name: String,
    payload: S::Payload,
    creator_user_id: Option<i64>,
    team_id: Option<i64>,
    status: BackgroundTaskStatus,
    result_json: Option<StoredTaskResult>,
    include_steps: bool,
    progress_current: i64,
    progress_total: i64,
    status_text: Option<String>,
    next_run_at: chrono::DateTime<Utc>,
    started_at: Option<chrono::DateTime<Utc>>,
    finished_at: Option<chrono::DateTime<Utc>>,
    last_error: Option<String>,
    failure_can_retry: Option<bool>,
    expires_at_anchor: chrono::DateTime<Utc>,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
}

impl<S: BackgroundTaskSpec> TypedTaskCreate<S> {
    /// 构造一条后台任务记录的 typed insert request。
    ///
    /// 这里故意只接收 `S::Payload`，不接收已经序列化好的 JSON。这样所有创建路径
    /// 都会通过 `BackgroundTaskSpec` 获得 kind、payload 编码、steps 和 max attempts。
    /// 普通用户可见任务优先用 `create_typed_task_record`；需要事务、延迟调度、
    /// runtime 记录或特殊状态时，用这个 builder 再交给 `insert_typed_task_record`。
    ///
    /// 如果你发现自己准备在业务模块里直接调用 `background_task_repo::create`，
    /// 先停一下：大概率应该给这个 builder 增加一个很小的 override，而不是复制
    /// 一整份 `background_task::ActiveModel`。
    pub(in crate::services::task_service) fn new(
        display_name: impl Into<String>,
        payload: S::Payload,
    ) -> Self {
        let now = Utc::now();
        Self {
            display_name: display_name.into(),
            payload,
            creator_user_id: None,
            team_id: None,
            status: BackgroundTaskStatus::Pending,
            result_json: None,
            include_steps: true,
            progress_current: 0,
            progress_total: 0,
            status_text: None,
            next_run_at: now,
            started_at: None,
            finished_at: None,
            last_error: None,
            failure_can_retry: None,
            expires_at_anchor: now,
            created_at: now,
            updated_at: now,
        }
    }

    pub(in crate::services::task_service) fn in_scope(
        mut self,
        scope: WorkspaceStorageScope,
    ) -> Self {
        self.creator_user_id = Some(scope.actor_user_id());
        self.team_id = scope.team_id();
        self
    }

    pub(in crate::services::task_service) fn creator_user_id(
        mut self,
        creator_user_id: Option<i64>,
    ) -> Self {
        self.creator_user_id = creator_user_id;
        self
    }

    pub(in crate::services::task_service) fn next_run_at(
        mut self,
        next_run_at: chrono::DateTime<Utc>,
    ) -> Self {
        self.next_run_at = next_run_at;
        self.expires_at_anchor = next_run_at;
        self
    }

    pub(in crate::services::task_service) fn progress(mut self, current: i64, total: i64) -> Self {
        self.progress_current = current;
        self.progress_total = total;
        self
    }

    pub(in crate::services::task_service) fn status_text(mut self, status_text: String) -> Self {
        self.status_text = Some(status_text);
        self
    }

    pub(in crate::services::task_service) fn status(
        mut self,
        status: BackgroundTaskStatus,
    ) -> Self {
        self.status = status;
        self
    }

    pub(in crate::services::task_service) fn result(mut self, result: &S::Result) -> Result<Self> {
        self.result_json = Some(spec::serialize_result::<S>(result)?);
        Ok(self)
    }

    pub(in crate::services::task_service) fn without_steps(mut self) -> Self {
        self.include_steps = false;
        self
    }

    pub(in crate::services::task_service) fn started_at(
        mut self,
        started_at: chrono::DateTime<Utc>,
    ) -> Self {
        self.started_at = Some(started_at);
        self.created_at = started_at;
        self
    }

    pub(in crate::services::task_service) fn finished_at(
        mut self,
        finished_at: chrono::DateTime<Utc>,
    ) -> Self {
        self.finished_at = Some(finished_at);
        self.next_run_at = finished_at;
        self.expires_at_anchor = finished_at;
        self.updated_at = finished_at;
        self
    }

    pub(in crate::services::task_service) fn last_error(
        mut self,
        last_error: Option<String>,
    ) -> Self {
        self.last_error = last_error;
        self
    }

    pub(in crate::services::task_service) fn failure_can_retry(
        mut self,
        failure_can_retry: Option<bool>,
    ) -> Self {
        self.failure_can_retry = failure_can_retry;
        self
    }

    fn steps_json(&self) -> Result<Option<StoredTaskSteps>> {
        if self.include_steps {
            serialize_task_steps(&registry::initial_task_steps(S::KIND)).map(Some)
        } else {
            Ok(None)
        }
    }

    fn status_text_for_insert(&self) -> Option<String> {
        self.status_text.as_deref().map(truncate_status_text)
    }

    fn last_error_for_insert(&self) -> Option<String> {
        self.last_error.as_deref().map(truncate_error)
    }

    fn into_active_model(self, state: &PrimaryAppState) -> Result<background_task::ActiveModel> {
        let payload_json = spec::serialize_payload::<S>(&self.payload)?;
        let steps_json = self.steps_json()?;
        let status_text = self.status_text_for_insert();
        let last_error = self.last_error_for_insert();

        Ok(background_task::ActiveModel {
            kind: Set(S::KIND),
            status: Set(self.status),
            creator_user_id: Set(self.creator_user_id),
            team_id: Set(self.team_id),
            share_id: Set(None),
            display_name: Set(truncate_display_name(&self.display_name)),
            payload_json: Set(payload_json),
            result_json: Set(self.result_json),
            runtime_json: Set(None),
            steps_json: Set(steps_json),
            progress_current: Set(self.progress_current),
            progress_total: Set(self.progress_total),
            status_text: Set(status_text),
            attempt_count: Set(0),
            max_attempts: Set(registry::max_attempts(state, S::KIND)),
            next_run_at: Set(self.next_run_at),
            processing_token: Set(0),
            processing_started_at: Set(None),
            last_heartbeat_at: Set(None),
            lease_expires_at: Set(None),
            started_at: Set(self.started_at),
            finished_at: Set(self.finished_at),
            last_error: Set(last_error),
            failure_can_retry: Set(self.failure_can_retry),
            expires_at: Set(task_expiration_from(state, self.expires_at_anchor)),
            created_at: Set(self.created_at),
            updated_at: Set(self.updated_at),
            ..Default::default()
        })
    }
}

pub(in crate::services::task_service) async fn insert_typed_task_record<
    C: ConnectionTrait,
    S: BackgroundTaskSpec,
>(
    state: &PrimaryAppState,
    db: &C,
    request: TypedTaskCreate<S>,
) -> Result<background_task::Model> {
    background_task_repo::create(db, request.into_active_model(state)?).await
}

pub(in crate::services::task_service) async fn create_typed_task_record<S: BackgroundTaskSpec>(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    display_name: &str,
    payload: &S::Payload,
) -> Result<background_task::Model> {
    let task = insert_typed_task_record(
        state,
        state.writer_db(),
        TypedTaskCreate::<S>::new(display_name, payload.clone()).in_scope(scope),
    )
    .await?;
    state.wake_background_task_dispatcher();
    Ok(task)
}

pub(super) fn truncate_display_name(value: &str) -> String {
    crate::utils::truncate_utf8_to_max_bytes(value, TASK_DISPLAY_NAME_MAX_LEN)
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
    update_task_progress_db(
        state.writer_db(),
        lease_guard,
        current,
        total,
        status_text,
        steps,
    )
    .await
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

pub(super) async fn set_task_runtime_json(
    state: &PrimaryAppState,
    lease_guard: &TaskLeaseGuard,
    runtime_json: Option<&str>,
) -> Result<()> {
    let lease = lease_guard.lease();
    let now = Utc::now();
    if background_task_repo::set_runtime_json(
        state.writer_db(),
        lease.task_id,
        lease.processing_token,
        runtime_json,
        now,
    )
    .await?
    {
        lease_guard.record_renewed();
        Ok(())
    } else {
        Err(lease_guard.mark_lost())
    }
}

pub(super) async fn set_task_display_name(
    state: &PrimaryAppState,
    lease_guard: &TaskLeaseGuard,
    display_name: &str,
) -> Result<()> {
    let lease = lease_guard.lease();
    let now = Utc::now();
    let display_name = truncate_display_name(display_name);
    if background_task_repo::set_display_name(
        state.writer_db(),
        lease.task_id,
        lease.processing_token,
        &display_name,
        now,
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
        state.writer_db(),
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
    prepare_task_temp_dir_in_root(&state.config.server.temp_dir, lease).await
}

pub(super) async fn prepare_task_temp_dir_in_root(
    temp_root: &str,
    lease: TaskLease,
) -> Result<String> {
    // 临时目录按 task_id/token 隔离：
    // temp/tasks/{task_id}/{processing_token}
    //
    // 这样任务被 stale reclaim 后，新旧 worker 不会写进同一个目录。
    // 这里也只清当前 lease 的 token 目录，避免旧 worker 启动时把新 lease 的产物删掉。
    cleanup_task_temp_dir_for_lease_in_root(temp_root, lease).await?;
    let task_temp_dir =
        crate::utils::paths::task_token_temp_dir(temp_root, lease.task_id, lease.processing_token);
    tokio::fs::create_dir_all(&task_temp_dir)
        .await
        .map_err(|error| {
            AsterError::storage_driver_error(format!("create task temp dir: {error}"))
        })?;
    Ok(task_temp_dir)
}

pub(super) async fn cleanup_task_temp_dir_for_lease_in_root(
    temp_root: &str,
    lease: TaskLease,
) -> Result<()> {
    crate::utils::cleanup_temp_dir(&crate::utils::paths::task_token_temp_dir(
        temp_root,
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
    cleanup_task_temp_dir_for_task_in_root(&state.config.server.temp_dir, task_id).await
}

pub(super) async fn cleanup_task_temp_dir_for_task_kind(
    state: &PrimaryAppState,
    kind: BackgroundTaskKind,
    task_id: i64,
) -> Result<()> {
    cleanup_task_temp_dir_for_task(state, task_id).await?;
    if kind == BackgroundTaskKind::OfflineDownload
        && let Some(temp_root) = operations::offline_download_temp_dir(&state.runtime_config)
        && temp_root != state.config.server.temp_dir
    {
        cleanup_task_temp_dir_for_task_in_root(&temp_root, task_id).await?;
    }
    Ok(())
}

pub(super) async fn cleanup_task_temp_dir_for_task_in_root(
    temp_root: &str,
    task_id: i64,
) -> Result<()> {
    // 成功路径会删整个任务根目录，因为到这里说明已经没有活跃 lease 需要保留产物了。
    // 如果任务在失败/崩溃/重启中断时没走到这里，后续由 task-cleanup 周期任务兜底清理。
    crate::utils::cleanup_temp_dir(&crate::utils::paths::task_temp_dir(temp_root, task_id)).await;
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

pub(super) fn task_worker_shutdown_requested(lease: TaskLease) -> AsterError {
    precondition_failed_with_subcode(
        ApiSubcode::TaskWorkerShutdownRequested,
        format!(
            "{TASK_WORKER_SHUTDOWN_REQUESTED_MESSAGE_PREFIX} for task #{} with token {}",
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

pub(super) fn is_task_worker_shutdown_requested(error: &AsterError) -> bool {
    error.api_error_subcode() == Some(ApiSubcode::TaskWorkerShutdownRequested)
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
        TASK_DISPLAY_NAME_MAX_LEN, TASK_LAST_ERROR_MAX_LEN, TASK_STATUS_TEXT_MAX_LEN,
        TypedTaskCreate, build_task_info_with_creator, ensure_task_in_scope, truncate_display_name,
        truncate_error, validate_admin_task_cleanup_status,
    };
    use crate::api::subcode::ApiSubcode;
    use crate::config::operations::OfflineDownloadEngine;
    use crate::entities::background_task;
    use crate::services::task_service::spec::{self, SystemRuntimeTask};
    use crate::services::task_service::types::{
        RuntimeSystemHealthComponent, RuntimeSystemHealthResult, RuntimeSystemHealthStatus,
        RuntimeTaskResult, TaskPresentationCode, TaskResult,
    };
    use crate::services::workspace_storage_service::WorkspaceStorageScope;
    use crate::types::{
        BackgroundTaskKind, BackgroundTaskStatus, StoredTaskPayload, StoredTaskResult,
        StoredTaskRuntime,
    };
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
            runtime_json: None,
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
                ApiSubcode::ArchivePreviewInvalidArchive,
                "invalid archive".to_string(),
            )),
            failure_can_retry: Some(false),
            expires_at: now + Duration::hours(1),
            created_at: now,
            updated_at: now,
        };

        let info = build_task_info_with_creator(task, None).expect("task info should build");

        assert_eq!(info.last_error.as_deref(), Some("invalid archive"));
    }

    #[test]
    fn task_info_includes_structured_thumbnail_presentation_from_result() {
        let now = Utc::now();
        let task = background_task::Model {
            id: 44,
            kind: BackgroundTaskKind::ThumbnailGenerate,
            status: BackgroundTaskStatus::Succeeded,
            creator_user_id: None,
            team_id: None,
            share_id: None,
            display_name: "Generate thumbnail for blob #42 via AsterDrive built-in".to_string(),
            payload_json: StoredTaskPayload(
                serde_json::json!({
                    "blob_id": 42,
                    "blob_hash": "hash-42",
                    "source_file_name": "",
                    "source_mime_type": "image/png",
                    "processor": "images"
                })
                .to_string(),
            ),
            result_json: Some(StoredTaskResult(
                serde_json::json!({
                    "blob_id": 42,
                    "thumbnail_path": "thumbnails/42.webp",
                    "thumbnail_processor": "images",
                    "thumbnail_version": "1",
                    "processor": "images",
                    "reused_existing_thumbnail": false
                })
                .to_string(),
            )),
            runtime_json: None,
            steps_json: None,
            progress_current: 4,
            progress_total: 4,
            status_text: Some("backend changed this sentence".to_string()),
            attempt_count: 1,
            max_attempts: 1,
            next_run_at: now,
            processing_token: 0,
            processing_started_at: None,
            last_heartbeat_at: None,
            lease_expires_at: None,
            started_at: Some(now),
            finished_at: Some(now),
            last_error: None,
            failure_can_retry: None,
            expires_at: now + Duration::hours(1),
            created_at: now,
            updated_at: now,
        };

        let info = build_task_info_with_creator(task, None).expect("task info should build");
        let presentation = info
            .presentation
            .as_ref()
            .expect("presentation should exist");
        let title = presentation.title.as_ref().expect("title should exist");
        let status = presentation.status.as_ref().expect("status should exist");

        assert_eq!(
            title.code,
            TaskPresentationCode::TaskNameThumbnailGenerateBlobWithProcessor
        );
        assert_eq!(title.params["blobId"], serde_json::json!(42));
        assert_eq!(title.params["processor"], serde_json::json!("images"));
        assert_eq!(status.code, TaskPresentationCode::StatusTextThumbnailReady);
    }

    #[test]
    fn task_info_includes_offline_download_engine_presentation_from_runtime() {
        let now = Utc::now();
        let task = background_task::Model {
            id: 46,
            kind: BackgroundTaskKind::OfflineDownload,
            status: BackgroundTaskStatus::Processing,
            creator_user_id: Some(7),
            team_id: None,
            share_id: None,
            display_name: "Import from https://example.com/file.bin via aria2".to_string(),
            payload_json: StoredTaskPayload(
                serde_json::json!({
                    "url": "https://example.com/file.bin",
                    "target_folder_id": 34,
                    "source_display_url": "https://example.com/file.bin"
                })
                .to_string(),
            ),
            result_json: None,
            runtime_json: Some(StoredTaskRuntime(
                serde_json::json!({
                    "engine": "aria2",
                    "aria2": {
                        "gid": "abc123",
                        "processing_token": 9
                    }
                })
                .to_string(),
            )),
            steps_json: None,
            progress_current: 0,
            progress_total: 0,
            status_text: Some("Downloading source file".to_string()),
            attempt_count: 1,
            max_attempts: 3,
            next_run_at: now,
            processing_token: 9,
            processing_started_at: Some(now),
            last_heartbeat_at: Some(now),
            lease_expires_at: Some(now + Duration::minutes(1)),
            started_at: Some(now),
            finished_at: None,
            last_error: None,
            failure_can_retry: None,
            expires_at: now + Duration::hours(1),
            created_at: now,
            updated_at: now,
        };

        let info = build_task_info_with_creator(task, None).expect("task info should build");
        let title = info
            .presentation
            .as_ref()
            .and_then(|presentation| presentation.title.as_ref())
            .expect("title should exist");

        assert_eq!(
            title.code,
            TaskPresentationCode::TaskNameOfflineDownloadTargetFolderWithEngine
        );
        assert_eq!(title.params["targetFolderId"], serde_json::json!(34));
        assert_eq!(title.params["engine"], serde_json::json!("aria2"));
    }

    #[test]
    fn task_info_includes_offline_download_engine_presentation_from_result() {
        let now = Utc::now();
        let task = background_task::Model {
            id: 48,
            kind: BackgroundTaskKind::OfflineDownload,
            status: BackgroundTaskStatus::Succeeded,
            creator_user_id: Some(7),
            team_id: None,
            share_id: None,
            display_name: "Import report.html from link via AsterDrive built-in".to_string(),
            payload_json: StoredTaskPayload(
                serde_json::json!({
                    "url": "https://example.com/report.html",
                    "filename": "report.html",
                    "source_display_url": "https://example.com/report.html"
                })
                .to_string(),
            ),
            result_json: Some(StoredTaskResult(
                serde_json::json!({
                    "file_id": 72,
                    "file_name": "report.html",
                    "folder_id": null,
                    "file_path": "/report.html",
                    "source_display_url": "https://example.com/report.html",
                    "content_length": 128,
                    "sha256": "0".repeat(64),
                    "download_engine": "builtin"
                })
                .to_string(),
            )),
            runtime_json: None,
            steps_json: None,
            progress_current: 128,
            progress_total: 128,
            status_text: Some("Offline download imported".to_string()),
            attempt_count: 1,
            max_attempts: 3,
            next_run_at: now,
            processing_token: 0,
            processing_started_at: Some(now),
            last_heartbeat_at: Some(now),
            lease_expires_at: None,
            started_at: Some(now),
            finished_at: Some(now),
            last_error: None,
            failure_can_retry: None,
            expires_at: now + Duration::hours(1),
            created_at: now,
            updated_at: now,
        };

        let info = build_task_info_with_creator(task, None).expect("task info should build");
        let title = info
            .presentation
            .as_ref()
            .and_then(|presentation| presentation.title.as_ref())
            .expect("title should exist");

        assert_eq!(
            title.code,
            TaskPresentationCode::TaskNameOfflineDownloadSourceWithEngine
        );
        assert_eq!(title.params["filename"], serde_json::json!("report.html"));
        assert_eq!(title.params["engine"], serde_json::json!("builtin"));

        let result = match info.result.expect("result should exist") {
            TaskResult::OfflineDownload(result) => result,
            other => panic!("unexpected result: {other:?}"),
        };
        assert_eq!(result.download_engine, Some(OfflineDownloadEngine::Builtin));
    }

    #[test]
    fn task_info_leaves_runtime_summary_status_to_fallback() {
        let now = Utc::now();
        let task = background_task::Model {
            id: 45,
            kind: BackgroundTaskKind::SystemRuntime,
            status: BackgroundTaskStatus::Succeeded,
            creator_user_id: None,
            team_id: None,
            share_id: None,
            display_name: "Completed upload cleanup".to_string(),
            payload_json: StoredTaskPayload(
                serde_json::json!({
                    "task_name": "completed-upload-cleanup"
                })
                .to_string(),
            ),
            result_json: Some(StoredTaskResult(
                serde_json::json!({
                    "duration_ms": 12,
                    "summary": "deleted 3 completed sessions (1 broken)"
                })
                .to_string(),
            )),
            runtime_json: None,
            steps_json: None,
            progress_current: 1,
            progress_total: 1,
            status_text: Some("deleted 3 completed sessions (1 broken)".to_string()),
            attempt_count: 0,
            max_attempts: 1,
            next_run_at: now,
            processing_token: 0,
            processing_started_at: None,
            last_heartbeat_at: None,
            lease_expires_at: None,
            started_at: Some(now),
            finished_at: Some(now),
            last_error: None,
            failure_can_retry: None,
            expires_at: now + Duration::hours(1),
            created_at: now,
            updated_at: now,
        };

        let info = build_task_info_with_creator(task, None).expect("task info should build");
        let presentation = info
            .presentation
            .as_ref()
            .expect("presentation should exist");
        let title = presentation.title.as_ref().expect("title should exist");

        assert_eq!(
            title.code,
            TaskPresentationCode::RuntimeTaskCompletedUploadCleanup
        );
        assert!(title.params.is_empty());
        assert!(
            presentation.status.is_none(),
            "runtime text summaries are legacy display text and should use frontend fallback"
        );
    }

    #[test]
    fn task_info_tolerates_legacy_runtime_result_without_duration() {
        let now = Utc::now();
        let task = background_task::Model {
            id: 49,
            kind: BackgroundTaskKind::SystemRuntime,
            status: BackgroundTaskStatus::Succeeded,
            creator_user_id: None,
            team_id: None,
            share_id: None,
            display_name: "Completed upload cleanup".to_string(),
            payload_json: StoredTaskPayload(
                serde_json::json!({
                    "task_name": "completed-upload-cleanup"
                })
                .to_string(),
            ),
            result_json: Some(StoredTaskResult(
                serde_json::json!({
                    "summary": "legacy summary only"
                })
                .to_string(),
            )),
            runtime_json: None,
            steps_json: None,
            progress_current: 1,
            progress_total: 1,
            status_text: Some("legacy summary only".to_string()),
            attempt_count: 0,
            max_attempts: 1,
            next_run_at: now,
            processing_token: 0,
            processing_started_at: None,
            last_heartbeat_at: None,
            lease_expires_at: None,
            started_at: Some(now),
            finished_at: Some(now),
            last_error: None,
            failure_can_retry: None,
            expires_at: now + Duration::hours(1),
            created_at: now,
            updated_at: now,
        };

        let info = build_task_info_with_creator(task, None).expect("task info should build");

        assert_eq!(info.status_text.as_deref(), Some("legacy summary only"));
        assert!(info.result.is_none());
        assert_eq!(
            info.presentation
                .as_ref()
                .and_then(|presentation| presentation.title.as_ref())
                .map(|title| title.code),
            Some(TaskPresentationCode::RuntimeTaskCompletedUploadCleanup)
        );
        assert!(
            info.presentation
                .as_ref()
                .and_then(|presentation| presentation.status.as_ref())
                .is_none(),
            "legacy text summaries should use frontend status_text fallback"
        );
    }

    #[test]
    fn task_info_leaves_legacy_runtime_task_name_to_raw_title_fallback() {
        let now = Utc::now();
        let task = background_task::Model {
            id: 47,
            kind: BackgroundTaskKind::SystemRuntime,
            status: BackgroundTaskStatus::Succeeded,
            creator_user_id: None,
            team_id: None,
            share_id: None,
            display_name: "Legacy runtime task".to_string(),
            payload_json: StoredTaskPayload(
                serde_json::json!({
                    "task_name": "legacy-runtime-task"
                })
                .to_string(),
            ),
            result_json: Some(StoredTaskResult(
                serde_json::json!({
                    "duration_ms": 12,
                    "summary": "legacy summary"
                })
                .to_string(),
            )),
            runtime_json: None,
            steps_json: None,
            progress_current: 1,
            progress_total: 1,
            status_text: Some("legacy summary".to_string()),
            attempt_count: 0,
            max_attempts: 1,
            next_run_at: now,
            processing_token: 0,
            processing_started_at: None,
            last_heartbeat_at: None,
            lease_expires_at: None,
            started_at: Some(now),
            finished_at: Some(now),
            last_error: None,
            failure_can_retry: None,
            expires_at: now + Duration::hours(1),
            created_at: now,
            updated_at: now,
        };

        let info = build_task_info_with_creator(task, None).expect("task info should build");
        let presentation = info
            .presentation
            .as_ref()
            .expect("presentation should exist");

        assert!(
            presentation.title.is_none(),
            "unknown legacy runtime task names should not be parsed into structured title codes"
        );
        assert!(
            presentation.status.is_none(),
            "runtime text summaries are legacy display text and should use frontend fallback"
        );
    }

    #[test]
    fn task_presentation_does_not_parse_static_status_text_for_business_tasks() {
        let now = Utc::now();
        let task = background_task::Model {
            id: 46,
            kind: BackgroundTaskKind::StoragePolicyTempCleanup,
            status: BackgroundTaskStatus::Pending,
            creator_user_id: None,
            team_id: None,
            share_id: None,
            display_name: "Clean deleted storage policy #7 temporary uploads".to_string(),
            payload_json: StoredTaskPayload(
                serde_json::json!({
                    "policy": {
                        "id": 7,
                        "name": "Deleted policy",
                        "driver_type": "local",
                        "endpoint": "",
                        "bucket": "",
                        "access_key": "",
                        "secret_key": "",
                        "base_path": "/tmp/storage",
                        "remote_node_id": null,
                        "max_file_size": 0,
                        "allowed_types": "[]",
                        "options": "{}",
                        "is_default": false,
                        "chunk_size": 5242880
                    },
                    "remote_node": null,
                    "temp_keys": ["uploads/a.tmp"],
                    "multipart_uploads": []
                })
                .to_string(),
            ),
            result_json: None,
            runtime_json: None,
            steps_json: None,
            progress_current: 0,
            progress_total: 0,
            status_text: Some("backend can freely rename this sentence".to_string()),
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

        let info = build_task_info_with_creator(task, None).expect("task info should build");
        let status = info
            .presentation
            .as_ref()
            .and_then(|presentation| presentation.status.as_ref())
            .expect("pending temp cleanup should have a structured status");

        assert_eq!(
            status.code,
            TaskPresentationCode::StatusTextWaitingPresignedUrlExpiry
        );
    }

    #[test]
    fn task_info_uses_structured_system_health_issue_presentation() {
        let now = Utc::now();
        let task = background_task::Model {
            id: 48,
            kind: BackgroundTaskKind::SystemRuntime,
            status: BackgroundTaskStatus::Succeeded,
            creator_user_id: None,
            team_id: None,
            share_id: None,
            display_name: "System health check".to_string(),
            payload_json: StoredTaskPayload(
                serde_json::json!({
                    "task_name": "system-health-check"
                })
                .to_string(),
            ),
            result_json: Some(
                spec::serialize_result::<SystemRuntimeTask>(&RuntimeTaskResult::from_timestamps(
                    now - Duration::milliseconds(12),
                    now,
                    None,
                    Some(RuntimeSystemHealthResult {
                        status: RuntimeSystemHealthStatus::Degraded,
                        components: vec![
                            RuntimeSystemHealthComponent {
                                name: "database".to_string(),
                                status: RuntimeSystemHealthStatus::Degraded,
                                message: "lagging".to_string(),
                            },
                            RuntimeSystemHealthComponent {
                                name: "cache".to_string(),
                                status: RuntimeSystemHealthStatus::Healthy,
                                message: String::new(),
                            },
                        ],
                    }),
                ))
                .expect("runtime result should serialize"),
            ),
            runtime_json: None,
            steps_json: None,
            progress_current: 1,
            progress_total: 1,
            status_text: Some("database=degraded: lagging".to_string()),
            attempt_count: 0,
            max_attempts: 1,
            next_run_at: now,
            processing_token: 0,
            processing_started_at: None,
            last_heartbeat_at: None,
            lease_expires_at: None,
            started_at: Some(now),
            finished_at: Some(now),
            last_error: None,
            failure_can_retry: None,
            expires_at: now + Duration::hours(1),
            created_at: now,
            updated_at: now,
        };

        let info = build_task_info_with_creator(task, None).expect("task info should build");
        let presentation = info
            .presentation
            .as_ref()
            .expect("presentation should exist");
        let status = presentation.status.as_ref().expect("status should exist");

        assert_eq!(
            status.code,
            TaskPresentationCode::RuntimeSystemHealthIssueDetail
        );
        assert_eq!(status.params["status"], serde_json::json!("degraded"));
        let components = status.params["components"]
            .as_array()
            .expect("components should be an array");
        assert_eq!(components.len(), 1);
        assert_eq!(components[0]["name"], serde_json::json!("database"));
        assert_eq!(components[0]["status"], serde_json::json!("degraded"));
        assert_eq!(components[0]["message"], serde_json::json!("lagging"));
    }

    #[test]
    fn decode_result_as_checks_kind_before_absent_result() {
        let now = Utc::now();
        let task = background_task::Model {
            id: 50,
            kind: BackgroundTaskKind::ThumbnailGenerate,
            status: BackgroundTaskStatus::Succeeded,
            creator_user_id: None,
            team_id: None,
            share_id: None,
            display_name: "wrong kind".to_string(),
            payload_json: StoredTaskPayload(
                serde_json::json!({
                    "blob_id": 42,
                    "blob_hash": "hash-42",
                    "source_file_name": "",
                    "source_mime_type": "image/png",
                    "processor": "images"
                })
                .to_string(),
            ),
            result_json: None,
            runtime_json: None,
            steps_json: None,
            progress_current: 1,
            progress_total: 1,
            status_text: None,
            attempt_count: 0,
            max_attempts: 1,
            next_run_at: now,
            processing_token: 0,
            processing_started_at: None,
            last_heartbeat_at: None,
            lease_expires_at: None,
            started_at: Some(now),
            finished_at: Some(now),
            last_error: None,
            failure_can_retry: None,
            expires_at: now + Duration::hours(1),
            created_at: now,
            updated_at: now,
        };

        let error = spec::decode_result_as::<SystemRuntimeTask>(&task)
            .expect_err("kind mismatch should be reported even without result json");

        assert!(error.message().contains("task #50 kind mismatch"));
        assert!(error.message().contains("expected system_runtime"));
        assert!(error.message().contains("got thumbnail_generate"));
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
    fn typed_task_create_truncates_status_text_and_last_error() {
        let long_status_text = "s".repeat(TASK_STATUS_TEXT_MAX_LEN + 7);
        let long_error = "e".repeat(TASK_LAST_ERROR_MAX_LEN + 7);
        let payload = crate::services::task_service::types::ThumbnailGenerateTaskPayload {
            blob_id: 42,
            blob_hash: "hash-42".to_string(),
            source_file_name: "image.png".to_string(),
            source_mime_type: "image/png".to_string(),
            processor: crate::types::MediaProcessorKind::Images,
        };
        let create =
            TypedTaskCreate::<crate::services::task_service::spec::ThumbnailGenerateTask>::new(
                "thumbnail",
                payload,
            )
            .status_text(long_status_text)
            .last_error(Some(long_error));

        assert_eq!(
            create.status_text_for_insert().as_deref(),
            Some("s".repeat(TASK_STATUS_TEXT_MAX_LEN).as_str())
        );
        assert_eq!(
            create.last_error_for_insert().as_deref(),
            Some("e".repeat(TASK_LAST_ERROR_MAX_LEN).as_str())
        );
    }

    #[test]
    fn truncate_display_name_keeps_utf8_boundary() {
        let value = format!("{}猫", "a".repeat(TASK_DISPLAY_NAME_MAX_LEN - 1));

        let truncated = truncate_display_name(&value);

        assert_eq!(truncated.len(), TASK_DISPLAY_NAME_MAX_LEN - 1);
        assert!(truncated.is_char_boundary(truncated.len()));
    }

    #[test]
    fn truncate_display_name_keeps_exact_ascii_limit() {
        let value = "a".repeat(TASK_DISPLAY_NAME_MAX_LEN);

        assert_eq!(truncate_display_name(&value), value);
    }

    #[test]
    fn truncate_display_name_truncates_ascii_over_limit() {
        let value = "a".repeat(TASK_DISPLAY_NAME_MAX_LEN + 1);

        let truncated = truncate_display_name(&value);

        assert_eq!(truncated.len(), TASK_DISPLAY_NAME_MAX_LEN);
        assert_eq!(truncated, "a".repeat(TASK_DISPLAY_NAME_MAX_LEN));
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
            runtime_json: None,
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
            runtime_json: None,
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

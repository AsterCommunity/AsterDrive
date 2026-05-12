//! 仓储模块：`background_task_repo`。

use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveEnum, ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, EntityTrait, ExprTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Select, sea_query::Expr,
};

use crate::api::pagination::{AdminTaskSortBy, SortOrder};
use crate::db::repository::pagination_repo::fetch_offset_page;
use crate::db::repository::sort::{order_by_column_with_id, order_by_id};
use crate::entities::background_task::{self, Entity as BackgroundTask};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::types::{BackgroundTaskKind, BackgroundTaskStatus, StoredTaskPayload};

pub async fn create<C: ConnectionTrait>(
    db: &C,
    model: background_task::ActiveModel,
) -> Result<background_task::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn find_by_id<C: ConnectionTrait>(db: &C, id: i64) -> Result<background_task::Model> {
    BackgroundTask::find_by_id(id)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::record_not_found(format!("task #{id}")))
}

pub async fn find_paginated_personal<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    limit: u64,
    offset: u64,
) -> Result<(Vec<background_task::Model>, u64)> {
    fetch_offset_page(
        db,
        BackgroundTask::find()
            .filter(background_task::Column::CreatorUserId.eq(user_id))
            .filter(background_task::Column::TeamId.is_null())
            .order_by_desc(background_task::Column::CreatedAt),
        limit,
        offset,
    )
    .await
}

pub async fn find_paginated_team<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    limit: u64,
    offset: u64,
) -> Result<(Vec<background_task::Model>, u64)> {
    fetch_offset_page(
        db,
        BackgroundTask::find()
            .filter(background_task::Column::TeamId.eq(team_id))
            .order_by_desc(background_task::Column::CreatedAt),
        limit,
        offset,
    )
    .await
}

pub async fn find_paginated_all<C: ConnectionTrait>(
    db: &C,
    limit: u64,
    offset: u64,
) -> Result<(Vec<background_task::Model>, u64)> {
    find_paginated_all_filtered(
        db,
        limit,
        offset,
        &AdminTaskFilters::default(),
        AdminTaskSortBy::UpdatedAt,
        SortOrder::Desc,
    )
    .await
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AdminTaskFilters {
    pub kind: Option<BackgroundTaskKind>,
    pub status: Option<BackgroundTaskStatus>,
}

#[derive(Debug, Clone, Copy)]
pub struct TerminalTaskCleanupFilters {
    pub finished_before: DateTime<Utc>,
    pub kind: Option<BackgroundTaskKind>,
    pub status: Option<BackgroundTaskStatus>,
}

pub async fn find_paginated_all_filtered<C: ConnectionTrait>(
    db: &C,
    limit: u64,
    offset: u64,
    filters: &AdminTaskFilters,
    sort_by: AdminTaskSortBy,
    sort_order: SortOrder,
) -> Result<(Vec<background_task::Model>, u64)> {
    fetch_offset_page(
        db,
        apply_admin_task_sort(
            apply_admin_filters(BackgroundTask::find(), filters),
            sort_by,
            sort_order,
        ),
        limit,
        offset,
    )
    .await
}

fn apply_admin_task_sort(
    query: Select<BackgroundTask>,
    sort_by: AdminTaskSortBy,
    sort_order: SortOrder,
) -> Select<BackgroundTask> {
    match sort_by {
        AdminTaskSortBy::Id => order_by_id(query, background_task::Column::Id, sort_order),
        AdminTaskSortBy::DisplayName => order_by_column_with_id(
            query,
            background_task::Column::DisplayName,
            sort_order,
            background_task::Column::Id,
        ),
        AdminTaskSortBy::Kind => order_by_column_with_id(
            query,
            background_task::Column::Kind,
            sort_order,
            background_task::Column::Id,
        ),
        AdminTaskSortBy::Status => order_by_column_with_id(
            query,
            background_task::Column::Status,
            sort_order,
            background_task::Column::Id,
        ),
        AdminTaskSortBy::Progress => order_by_column_with_id(
            query,
            background_task::Column::ProgressCurrent,
            sort_order,
            background_task::Column::Id,
        ),
        AdminTaskSortBy::CreatedAt => order_by_column_with_id(
            query,
            background_task::Column::CreatedAt,
            sort_order,
            background_task::Column::Id,
        ),
        AdminTaskSortBy::UpdatedAt => order_by_column_with_id(
            query,
            background_task::Column::UpdatedAt,
            sort_order,
            background_task::Column::Id,
        ),
        AdminTaskSortBy::StartedAt => order_by_column_with_id(
            query,
            background_task::Column::StartedAt,
            sort_order,
            background_task::Column::Id,
        ),
        AdminTaskSortBy::FinishedAt => order_by_column_with_id(
            query,
            background_task::Column::FinishedAt,
            sort_order,
            background_task::Column::Id,
        ),
    }
}

pub async fn list_recent<C: ConnectionTrait>(
    db: &C,
    limit: u64,
) -> Result<Vec<background_task::Model>> {
    BackgroundTask::find()
        .order_by_desc(background_task::Column::UpdatedAt)
        .limit(limit)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_latest_system_runtime_by_task_name<C: ConnectionTrait>(
    db: &C,
    task_name: &str,
) -> Result<Option<background_task::Model>> {
    let payload_json = serde_json::to_string(&serde_json::json!({
        "task_name": task_name,
    }))
    .map(StoredTaskPayload)
    .map_aster_err_ctx("serialize runtime task payload", AsterError::internal_error)?;

    BackgroundTask::find()
        .filter(background_task::Column::Kind.eq(BackgroundTaskKind::SystemRuntime))
        .filter(background_task::Column::PayloadJson.eq(payload_json))
        .order_by_desc(background_task::Column::UpdatedAt)
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn count_processing<C: ConnectionTrait>(db: &C) -> Result<u64> {
    BackgroundTask::find()
        .filter(background_task::Column::Status.eq(BackgroundTaskStatus::Processing))
        .count(db)
        .await
        .map_err(AsterError::from)
}

pub async fn count_active_processing_by_kinds<C: ConnectionTrait>(
    db: &C,
    now: DateTime<Utc>,
    kinds: &[BackgroundTaskKind],
) -> Result<u64> {
    if kinds.is_empty() {
        return Ok(0);
    }

    let count = BackgroundTask::find()
        .select_only()
        .column_as(
            Expr::col(background_task::Column::Id).count(),
            "active_processing_count",
        )
        .filter(active_processing_by_kinds_condition(now, kinds))
        .into_tuple::<i64>()
        .one(db)
        .await
        .map_err(AsterError::from)?
        .unwrap_or(0);

    crate::utils::numbers::i64_to_u64(count, "active processing task count")
}

pub async fn find_latest_by_kind_and_display_name<C: ConnectionTrait>(
    db: &C,
    kind: BackgroundTaskKind,
    display_name: &str,
) -> Result<Option<background_task::Model>> {
    BackgroundTask::find()
        .filter(background_task::Column::Kind.eq(kind))
        .filter(background_task::Column::DisplayName.eq(display_name))
        .order_by_desc(background_task::Column::CreatedAt)
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn list_claimable<C: ConnectionTrait>(
    db: &C,
    now: DateTime<Utc>,
    stale_before: DateTime<Utc>,
    limit: u64,
) -> Result<Vec<background_task::Model>> {
    list_claimable_query(now, stale_before, limit)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn list_claimable_by_kinds<C: ConnectionTrait>(
    db: &C,
    now: DateTime<Utc>,
    stale_before: DateTime<Utc>,
    kinds: &[BackgroundTaskKind],
    limit: u64,
) -> Result<Vec<background_task::Model>> {
    if kinds.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }

    list_claimable_query(now, stale_before, limit)
        .filter(background_task::Column::Kind.is_in(kinds.iter().copied()))
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn try_claim<C: ConnectionTrait>(
    db: &C,
    id: i64,
    expected_processing_token: i64,
    now: DateTime<Utc>,
    stale_before: DateTime<Utc>,
    next_processing_token: i64,
    lease_expires_at: DateTime<Utc>,
) -> Result<bool> {
    // try_claim 是一条 compare-and-swap：
    // 只有当 id 命中、旧 processing_token 仍匹配、并且任务此刻仍满足 claimable 条件时，
    // 才会把任务推进到 Processing，并原子地把 token 递增到 next_processing_token。
    //
    // 这样多个 dispatcher 并发捞到同一条任务时，只有一个能成功认领。
    let result = BackgroundTask::update_many()
        .col_expr(
            background_task::Column::Status,
            Expr::value(BackgroundTaskStatus::Processing.to_value()),
        )
        .col_expr(
            background_task::Column::ProcessingStartedAt,
            Expr::value(Some(now)),
        )
        .col_expr(
            background_task::Column::LastHeartbeatAt,
            Expr::value(Some(now)),
        )
        .col_expr(
            background_task::Column::ProcessingToken,
            Expr::value(next_processing_token),
        )
        .col_expr(
            background_task::Column::LeaseExpiresAt,
            Expr::value(Some(lease_expires_at)),
        )
        .col_expr(
            background_task::Column::StartedAt,
            Expr::col(background_task::Column::StartedAt).if_null(now),
        )
        .col_expr(background_task::Column::UpdatedAt, Expr::value(now))
        .filter(background_task::Column::Id.eq(id))
        .filter(background_task::Column::ProcessingToken.eq(expected_processing_token))
        .filter(claimable_condition(now, stale_before))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub struct TaskProgressUpdate<'a> {
    pub id: i64,
    pub processing_token: i64,
    pub now: DateTime<Utc>,
    pub lease_expires_at: DateTime<Utc>,
    pub current: i64,
    pub total: i64,
    pub status_text: Option<&'a str>,
    pub steps_json: Option<&'a str>,
}

pub async fn mark_progress<C: ConnectionTrait>(
    db: &C,
    update: TaskProgressUpdate<'_>,
) -> Result<bool> {
    let mut statement = BackgroundTask::update_many()
        .col_expr(
            background_task::Column::ProgressCurrent,
            Expr::value(update.current),
        )
        .col_expr(
            background_task::Column::ProgressTotal,
            Expr::value(update.total),
        )
        .col_expr(
            background_task::Column::StatusText,
            Expr::value(update.status_text.map(str::to_string)),
        )
        .col_expr(
            background_task::Column::LastHeartbeatAt,
            Expr::value(Some(update.now)),
        )
        .col_expr(
            background_task::Column::LeaseExpiresAt,
            Expr::value(Some(update.lease_expires_at)),
        )
        .col_expr(background_task::Column::UpdatedAt, Expr::value(update.now))
        .filter(background_task::Column::Id.eq(update.id))
        .filter(background_task::Column::Status.eq(BackgroundTaskStatus::Processing))
        .filter(background_task::Column::ProcessingToken.eq(update.processing_token));
    if let Some(steps_json) = update.steps_json {
        statement = statement.col_expr(
            background_task::Column::StepsJson,
            Expr::value(Some(steps_json.to_string())),
        );
    }
    let result = statement.exec(db).await.map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub struct TaskSuccessUpdate<'a> {
    pub id: i64,
    pub processing_token: i64,
    pub result_json: Option<&'a str>,
    pub steps_json: Option<&'a str>,
    pub current: i64,
    pub total: i64,
    pub status_text: Option<&'a str>,
    pub finished_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

pub async fn mark_succeeded<C: ConnectionTrait>(
    db: &C,
    success: TaskSuccessUpdate<'_>,
) -> Result<bool> {
    let mut update = BackgroundTask::update_many()
        .col_expr(
            background_task::Column::Status,
            Expr::value(BackgroundTaskStatus::Succeeded.to_value()),
        )
        .col_expr(
            background_task::Column::ResultJson,
            Expr::value(success.result_json.map(str::to_string)),
        )
        .col_expr(
            background_task::Column::ProgressCurrent,
            Expr::value(success.current),
        )
        .col_expr(
            background_task::Column::ProgressTotal,
            Expr::value(success.total),
        )
        .col_expr(
            background_task::Column::StatusText,
            Expr::value(success.status_text.map(str::to_string)),
        )
        .col_expr(
            background_task::Column::ProcessingStartedAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::LastHeartbeatAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::LeaseExpiresAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::FinishedAt,
            Expr::value(Some(success.finished_at)),
        )
        .col_expr(
            background_task::Column::ExpiresAt,
            Expr::value(success.expires_at),
        )
        .col_expr(
            background_task::Column::UpdatedAt,
            Expr::value(success.finished_at),
        )
        .filter(background_task::Column::Id.eq(success.id))
        .filter(background_task::Column::Status.eq(BackgroundTaskStatus::Processing))
        .filter(background_task::Column::ProcessingToken.eq(success.processing_token));
    if let Some(steps_json) = success.steps_json {
        update = update.col_expr(
            background_task::Column::StepsJson,
            Expr::value(Some(steps_json.to_string())),
        );
    }
    let result = update.exec(db).await.map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn mark_retry<C: ConnectionTrait>(
    db: &C,
    id: i64,
    processing_token: i64,
    attempt_count: i32,
    next_run_at: DateTime<Utc>,
    last_error: &str,
    steps_json: Option<&str>,
) -> Result<bool> {
    let mut update = BackgroundTask::update_many()
        .col_expr(
            background_task::Column::Status,
            Expr::value(BackgroundTaskStatus::Retry.to_value()),
        )
        .col_expr(
            background_task::Column::AttemptCount,
            Expr::value(attempt_count),
        )
        .col_expr(background_task::Column::NextRunAt, Expr::value(next_run_at))
        .col_expr(
            background_task::Column::ProcessingStartedAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::LastHeartbeatAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::LeaseExpiresAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::StatusText,
            Expr::value(Option::<String>::None),
        )
        .col_expr(
            background_task::Column::LastError,
            Expr::value(Some(last_error.to_string())),
        )
        .col_expr(background_task::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(background_task::Column::Id.eq(id))
        .filter(background_task::Column::Status.eq(BackgroundTaskStatus::Processing))
        .filter(background_task::Column::ProcessingToken.eq(processing_token));
    if let Some(steps_json) = steps_json {
        update = update.col_expr(
            background_task::Column::StepsJson,
            Expr::value(Some(steps_json.to_string())),
        );
    }
    let result = update.exec(db).await.map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub struct TaskFailureUpdate<'a> {
    pub id: i64,
    pub processing_token: i64,
    pub attempt_count: i32,
    pub last_error: &'a str,
    pub finished_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub steps_json: Option<&'a str>,
    pub failure_can_retry: bool,
}

pub async fn mark_failed<C: ConnectionTrait>(
    db: &C,
    update: TaskFailureUpdate<'_>,
) -> Result<bool> {
    let mut statement = BackgroundTask::update_many()
        .col_expr(
            background_task::Column::Status,
            Expr::value(BackgroundTaskStatus::Failed.to_value()),
        )
        .col_expr(
            background_task::Column::AttemptCount,
            Expr::value(update.attempt_count),
        )
        .col_expr(
            background_task::Column::ProcessingStartedAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::LastHeartbeatAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::LeaseExpiresAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::StatusText,
            Expr::value(Option::<String>::None),
        )
        .col_expr(
            background_task::Column::LastError,
            Expr::value(Some(update.last_error.to_string())),
        )
        .col_expr(
            background_task::Column::FailureCanRetry,
            Expr::value(Some(update.failure_can_retry)),
        )
        .col_expr(
            background_task::Column::FinishedAt,
            Expr::value(Some(update.finished_at)),
        )
        .col_expr(
            background_task::Column::ExpiresAt,
            Expr::value(update.expires_at),
        )
        .col_expr(
            background_task::Column::UpdatedAt,
            Expr::value(update.finished_at),
        )
        .filter(background_task::Column::Id.eq(update.id))
        .filter(background_task::Column::Status.eq(BackgroundTaskStatus::Processing))
        .filter(background_task::Column::ProcessingToken.eq(update.processing_token));
    if let Some(steps_json) = update.steps_json {
        statement = statement.col_expr(
            background_task::Column::StepsJson,
            Expr::value(Some(steps_json.to_string())),
        );
    }
    let result = statement.exec(db).await.map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn reset_for_manual_retry<C: ConnectionTrait>(
    db: &C,
    id: i64,
    now: DateTime<Utc>,
    max_attempts: i32,
    steps_json: Option<&str>,
) -> Result<bool> {
    let mut update = BackgroundTask::update_many()
        .col_expr(
            background_task::Column::Status,
            Expr::value(BackgroundTaskStatus::Pending.to_value()),
        )
        .col_expr(background_task::Column::AttemptCount, Expr::value(0))
        .col_expr(background_task::Column::ProgressCurrent, Expr::value(0))
        .col_expr(
            background_task::Column::MaxAttempts,
            Expr::value(max_attempts),
        )
        .col_expr(background_task::Column::NextRunAt, Expr::value(now))
        .col_expr(
            background_task::Column::ProcessingStartedAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::LastHeartbeatAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::LeaseExpiresAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::StartedAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::FinishedAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::StatusText,
            Expr::value(Option::<String>::None),
        )
        .col_expr(
            background_task::Column::LastError,
            Expr::value(Option::<String>::None),
        )
        .col_expr(
            background_task::Column::ResultJson,
            Expr::value(Option::<String>::None),
        )
        .col_expr(
            background_task::Column::FailureCanRetry,
            Expr::value(Option::<bool>::None),
        )
        .col_expr(background_task::Column::UpdatedAt, Expr::value(now))
        .filter(background_task::Column::Id.eq(id))
        .filter(background_task::Column::Status.eq(BackgroundTaskStatus::Failed));
    if let Some(steps_json) = steps_json {
        update = update.col_expr(
            background_task::Column::StepsJson,
            Expr::value(Some(steps_json.to_string())),
        );
    }
    let result = update.exec(db).await.map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn touch_heartbeat<C: ConnectionTrait>(
    db: &C,
    id: i64,
    processing_token: i64,
    now: DateTime<Utc>,
    lease_expires_at: DateTime<Utc>,
) -> Result<bool> {
    // heartbeat 也带 token 条件。
    // 如果返回 false，说明任务虽然还在表里，但这条 worker 的 lease 已经过期了。
    let result = BackgroundTask::update_many()
        .col_expr(
            background_task::Column::LastHeartbeatAt,
            Expr::value(Some(now)),
        )
        .col_expr(
            background_task::Column::LeaseExpiresAt,
            Expr::value(Some(lease_expires_at)),
        )
        .filter(background_task::Column::Id.eq(id))
        .filter(background_task::Column::Status.eq(BackgroundTaskStatus::Processing))
        .filter(background_task::Column::ProcessingToken.eq(processing_token))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn list_expired_terminal<C: ConnectionTrait>(
    db: &C,
    now: DateTime<Utc>,
    limit: u64,
) -> Result<Vec<background_task::Model>> {
    BackgroundTask::find()
        .filter(background_task::Column::ExpiresAt.lte(now))
        .filter(background_task::Column::Status.is_in([
            BackgroundTaskStatus::Succeeded,
            BackgroundTaskStatus::Failed,
            BackgroundTaskStatus::Canceled,
        ]))
        .order_by_asc(background_task::Column::ExpiresAt)
        .limit(limit)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn delete_many<C: ConnectionTrait>(db: &C, ids: &[i64]) -> Result<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
    Ok(BackgroundTask::delete_many()
        .filter(background_task::Column::Id.is_in(ids.iter().copied()))
        .exec(db)
        .await
        .map_err(AsterError::from)?
        .rows_affected)
}

pub async fn delete_terminal_by_filters<C: ConnectionTrait>(
    db: &C,
    filters: &TerminalTaskCleanupFilters,
) -> Result<u64> {
    Ok(BackgroundTask::delete_many()
        .filter(terminal_cleanup_condition(filters))
        .exec(db)
        .await
        .map_err(AsterError::from)?
        .rows_affected)
}

fn apply_admin_filters(
    mut query: Select<BackgroundTask>,
    filters: &AdminTaskFilters,
) -> Select<BackgroundTask> {
    if let Some(kind) = filters.kind {
        query = query.filter(background_task::Column::Kind.eq(kind));
    }
    if let Some(status) = filters.status {
        query = query.filter(background_task::Column::Status.eq(status));
    }
    query
}

fn terminal_cleanup_condition(filters: &TerminalTaskCleanupFilters) -> Condition {
    let mut condition = Condition::all();
    condition = condition.add(match filters.status {
        Some(status) => background_task::Column::Status.eq(status),
        None => background_task::Column::Status.is_in([
            BackgroundTaskStatus::Succeeded,
            BackgroundTaskStatus::Failed,
            BackgroundTaskStatus::Canceled,
        ]),
    });
    if let Some(kind) = filters.kind {
        condition = condition.add(background_task::Column::Kind.eq(kind));
    }
    condition.add(
        Condition::any()
            .add(background_task::Column::FinishedAt.lte(filters.finished_before))
            .add(
                Condition::all()
                    .add(background_task::Column::FinishedAt.is_null())
                    .add(background_task::Column::UpdatedAt.lte(filters.finished_before)),
            ),
    )
}

fn list_claimable_query(
    now: DateTime<Utc>,
    stale_before: DateTime<Utc>,
    limit: u64,
) -> Select<BackgroundTask> {
    BackgroundTask::find()
        .filter(claimable_condition(now, stale_before))
        .order_by_asc(background_task::Column::CreatedAt)
        .order_by_asc(background_task::Column::Id)
        .limit(limit)
}

fn active_processing_by_kinds_condition(
    now: DateTime<Utc>,
    kinds: &[BackgroundTaskKind],
) -> Condition {
    Condition::all()
        .add(background_task::Column::Status.eq(BackgroundTaskStatus::Processing))
        .add(background_task::Column::LeaseExpiresAt.is_not_null())
        .add(background_task::Column::LeaseExpiresAt.gt(now))
        .add(background_task::Column::Kind.is_in(kinds.iter().copied()))
}

fn claimable_condition(now: DateTime<Utc>, _stale_before: DateTime<Utc>) -> Condition {
    // 可认领任务有两类：
    // 1. Pending / Retry 且 next_run_at 已到；
    // 2. 仍显示 Processing，但已经 stale，可被新 worker 硬接管。
    Condition::any()
        .add(
            Condition::all()
                .add(
                    background_task::Column::Status
                        .is_in([BackgroundTaskStatus::Pending, BackgroundTaskStatus::Retry]),
                )
                .add(background_task::Column::NextRunAt.lte(now)),
        )
        .add(processing_stale_condition(now))
}

fn processing_stale_condition(now: DateTime<Utc>) -> Condition {
    Condition::all()
        .add(background_task::Column::Status.eq(BackgroundTaskStatus::Processing))
        .add(background_task::Column::LeaseExpiresAt.is_not_null())
        .add(background_task::Column::LeaseExpiresAt.lte(now))
}

#[cfg(test)]
mod tests {
    use super::{
        AdminTaskFilters, AdminTaskSortBy, SortOrder, TerminalTaskCleanupFilters,
        count_active_processing_by_kinds, delete_terminal_by_filters, find_paginated_all_filtered,
        list_claimable_by_kinds,
    };
    use crate::config::DatabaseConfig;
    use crate::entities::background_task;
    use crate::types::{
        BackgroundTaskKind, BackgroundTaskStatus, StoredTaskPayload, StoredTaskSteps,
    };
    use chrono::{Duration, Utc};
    use migration::Migrator;
    use sea_orm::{ActiveModelTrait, EntityTrait, Set};

    async fn build_test_db() -> sea_orm::DatabaseConnection {
        let db = crate::db::connect(&DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            pool_size: 1,
            retry_count: 0,
        })
        .await
        .expect("background task repo test DB should connect");
        Migrator::up(&db, None)
            .await
            .expect("background task repo test migrations should succeed");
        db
    }

    async fn insert_task(
        db: &sea_orm::DatabaseConnection,
        kind: BackgroundTaskKind,
        status: BackgroundTaskStatus,
        finished_at: Option<chrono::DateTime<chrono::Utc>>,
        updated_at: chrono::DateTime<chrono::Utc>,
    ) -> background_task::Model {
        let created_at = updated_at - Duration::hours(1);
        let task_name = match kind {
            BackgroundTaskKind::ArchiveCompress => "archive-compress",
            BackgroundTaskKind::ArchiveExtract => "archive-extract",
            BackgroundTaskKind::ThumbnailGenerate => "thumbnail-generate",
            BackgroundTaskKind::SystemRuntime => "task-cleanup",
        };
        let payload_json = match kind {
            BackgroundTaskKind::ArchiveCompress => serde_json::json!({
                "file_ids": [],
                "folder_ids": [],
                "archive_name": "repo-test.zip",
                "target_folder_id": null,
            }),
            BackgroundTaskKind::ArchiveExtract => serde_json::json!({
                "file_id": 1,
                "source_file_name": "repo-test.zip",
                "target_folder_id": null,
                "output_folder_name": "repo-test",
            }),
            BackgroundTaskKind::ThumbnailGenerate => serde_json::json!({
                "blob_id": 1,
                "blob_hash": "hash",
                "source_file_name": "repo-test.png",
                "source_mime_type": "image/png",
                "processor": "image_magick",
            }),
            BackgroundTaskKind::SystemRuntime => serde_json::json!({
                "task_name": task_name,
            }),
        };

        background_task::ActiveModel {
            kind: Set(kind),
            status: Set(status),
            creator_user_id: Set(Some(7)),
            team_id: Set(None),
            share_id: Set(None),
            display_name: Set(format!("{kind:?}-{status:?}")),
            payload_json: Set(StoredTaskPayload(payload_json.to_string())),
            result_json: Set(None),
            steps_json: Set(Some(StoredTaskSteps("[]".to_string()))),
            progress_current: Set(0),
            progress_total: Set(0),
            status_text: Set(None),
            attempt_count: Set(0),
            max_attempts: Set(1),
            next_run_at: Set(created_at),
            processing_token: Set(0),
            processing_started_at: Set(None),
            last_heartbeat_at: Set(None),
            lease_expires_at: Set(None),
            started_at: Set(None),
            finished_at: Set(finished_at),
            last_error: Set(None),
            failure_can_retry: Set(None),
            expires_at: Set(updated_at + Duration::hours(24)),
            created_at: Set(created_at),
            updated_at: Set(updated_at),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("background task test row should insert")
    }

    async fn set_task_lease(
        db: &sea_orm::DatabaseConnection,
        task: background_task::Model,
        lease_expires_at: chrono::DateTime<chrono::Utc>,
    ) -> background_task::Model {
        let mut active: background_task::ActiveModel = task.into();
        active.lease_expires_at = Set(Some(lease_expires_at));
        active
            .update(db)
            .await
            .expect("background task test lease should update")
    }

    #[tokio::test]
    async fn find_paginated_all_filtered_applies_kind_and_status() {
        let db = build_test_db().await;
        let now = Utc::now();
        let wanted = insert_task(
            &db,
            BackgroundTaskKind::ArchiveExtract,
            BackgroundTaskStatus::Failed,
            Some(now - Duration::hours(2)),
            now - Duration::minutes(5),
        )
        .await;
        insert_task(
            &db,
            BackgroundTaskKind::ArchiveExtract,
            BackgroundTaskStatus::Processing,
            None,
            now - Duration::minutes(4),
        )
        .await;
        insert_task(
            &db,
            BackgroundTaskKind::SystemRuntime,
            BackgroundTaskStatus::Failed,
            Some(now - Duration::hours(3)),
            now - Duration::minutes(3),
        )
        .await;

        let (items, total) = find_paginated_all_filtered(
            &db,
            20,
            0,
            &AdminTaskFilters {
                kind: Some(BackgroundTaskKind::ArchiveExtract),
                status: Some(BackgroundTaskStatus::Failed),
            },
            AdminTaskSortBy::UpdatedAt,
            SortOrder::Desc,
        )
        .await
        .expect("filtered admin task query should succeed");

        assert_eq!(total, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, wanted.id);
    }

    #[tokio::test]
    async fn count_active_processing_by_kinds_only_counts_unexpired_leases() {
        let db = build_test_db().await;
        let now = Utc::now();
        let active_archive = insert_task(
            &db,
            BackgroundTaskKind::ArchiveCompress,
            BackgroundTaskStatus::Processing,
            None,
            now - Duration::minutes(5),
        )
        .await;
        set_task_lease(&db, active_archive, now + Duration::seconds(30)).await;
        let stale_archive = insert_task(
            &db,
            BackgroundTaskKind::ArchiveExtract,
            BackgroundTaskStatus::Processing,
            None,
            now - Duration::minutes(4),
        )
        .await;
        set_task_lease(&db, stale_archive, now - Duration::seconds(1)).await;
        insert_task(
            &db,
            BackgroundTaskKind::ArchiveExtract,
            BackgroundTaskStatus::Processing,
            None,
            now - Duration::minutes(3),
        )
        .await;
        let active_thumbnail = insert_task(
            &db,
            BackgroundTaskKind::ThumbnailGenerate,
            BackgroundTaskStatus::Processing,
            None,
            now - Duration::minutes(2),
        )
        .await;
        set_task_lease(&db, active_thumbnail, now + Duration::seconds(30)).await;

        let archive_active = count_active_processing_by_kinds(
            &db,
            now,
            &[
                BackgroundTaskKind::ArchiveCompress,
                BackgroundTaskKind::ArchiveExtract,
            ],
        )
        .await
        .expect("archive active count should succeed");
        let thumbnail_active =
            count_active_processing_by_kinds(&db, now, &[BackgroundTaskKind::ThumbnailGenerate])
                .await
                .expect("thumbnail active count should succeed");

        assert_eq!(archive_active, 1);
        assert_eq!(thumbnail_active, 1);
    }

    #[tokio::test]
    async fn list_claimable_by_kinds_filters_lane_kinds() {
        let db = build_test_db().await;
        let now = Utc::now();
        let archive = insert_task(
            &db,
            BackgroundTaskKind::ArchiveExtract,
            BackgroundTaskStatus::Pending,
            None,
            now - Duration::minutes(5),
        )
        .await;
        let thumbnail = insert_task(
            &db,
            BackgroundTaskKind::ThumbnailGenerate,
            BackgroundTaskStatus::Pending,
            None,
            now - Duration::minutes(4),
        )
        .await;

        let claimable = list_claimable_by_kinds(
            &db,
            now,
            now - Duration::seconds(60),
            &[
                BackgroundTaskKind::ArchiveCompress,
                BackgroundTaskKind::ArchiveExtract,
            ],
            10,
        )
        .await
        .expect("claimable kind filter should succeed");
        let ids = claimable
            .into_iter()
            .map(|task| task.id)
            .collect::<Vec<_>>();

        assert!(ids.contains(&archive.id));
        assert!(!ids.contains(&thumbnail.id));
    }

    #[tokio::test]
    async fn delete_terminal_by_filters_only_removes_matching_completed_tasks() {
        let db = build_test_db().await;
        let now = Utc::now();
        let old_succeeded = insert_task(
            &db,
            BackgroundTaskKind::SystemRuntime,
            BackgroundTaskStatus::Succeeded,
            Some(now - Duration::hours(72)),
            now - Duration::hours(72),
        )
        .await;
        let old_failed = insert_task(
            &db,
            BackgroundTaskKind::SystemRuntime,
            BackgroundTaskStatus::Failed,
            Some(now - Duration::hours(60)),
            now - Duration::hours(60),
        )
        .await;
        let recent_failed = insert_task(
            &db,
            BackgroundTaskKind::SystemRuntime,
            BackgroundTaskStatus::Failed,
            Some(now - Duration::hours(4)),
            now - Duration::hours(4),
        )
        .await;
        let other_kind = insert_task(
            &db,
            BackgroundTaskKind::ArchiveExtract,
            BackgroundTaskStatus::Failed,
            Some(now - Duration::hours(60)),
            now - Duration::hours(60),
        )
        .await;
        let active_task = insert_task(
            &db,
            BackgroundTaskKind::SystemRuntime,
            BackgroundTaskStatus::Processing,
            None,
            now - Duration::hours(80),
        )
        .await;

        let removed = delete_terminal_by_filters(
            &db,
            &TerminalTaskCleanupFilters {
                finished_before: now - Duration::hours(24),
                kind: Some(BackgroundTaskKind::SystemRuntime),
                status: Some(BackgroundTaskStatus::Failed),
            },
        )
        .await
        .expect("task cleanup delete should succeed");

        assert_eq!(removed, 1);

        let remaining_ids = background_task::Entity::find()
            .all(&db)
            .await
            .expect("remaining tasks should load")
            .into_iter()
            .map(|task| task.id)
            .collect::<Vec<_>>();

        assert!(remaining_ids.contains(&old_succeeded.id));
        assert!(!remaining_ids.contains(&old_failed.id));
        assert!(remaining_ids.contains(&recent_failed.id));
        assert!(remaining_ids.contains(&other_kind.id));
        assert!(remaining_ids.contains(&active_task.id));
    }
}

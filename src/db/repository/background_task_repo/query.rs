use chrono::{DateTime, Utc};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, ExprTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Select, sea_query::Expr,
};

use super::common::{AdminTaskFilters, active_processing_by_kinds_condition, apply_admin_filters};
use crate::api::pagination::AdminTaskSortBy;
use crate::errors::{AsterError, Result};
use aster_drive_model::entities::background_task::{self, Entity as BackgroundTask};
use aster_drive_model::types::{BackgroundTaskKind, BackgroundTaskStatus, StoredTaskPayload};
use aster_forge_api::SortOrder;
use aster_forge_db::pagination::fetch_offset_page;
use aster_forge_db::sort::{order_by_column_with_id, order_by_id};

/// Loads a background task by id, returning `record_not_found` when absent.
pub async fn find_by_id(db: &DatabaseConnection, id: i64) -> Result<background_task::Model> {
    BackgroundTask::find_by_id(id)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::record_not_found(format!("task #{id}")))
}

/// The caller holds the workspace namespace lock through admission and mutation.
/// A current shared read also sees admissions committed after a protocol adapter's
/// earlier MySQL REPEATABLE READ snapshot. Dispatchers may skip locked rows briefly;
/// worker polling must continue until the transaction releases them.
pub async fn find_active_by_kind_in_workspace<C: ConnectionTrait>(
    db: &C,
    kind: BackgroundTaskKind,
    workspace_type: aster_drive_model::types::LockWorkspaceType,
    workspace_id: i64,
) -> Result<Option<background_task::Model>> {
    use aster_drive_model::types::LockWorkspaceType;
    let mut query = BackgroundTask::find()
        .filter(background_task::Column::Kind.eq(kind))
        .filter(background_task::Column::Status.is_in([
            BackgroundTaskStatus::Pending,
            BackgroundTaskStatus::Processing,
            BackgroundTaskStatus::Retry,
            BackgroundTaskStatus::Canceling,
        ]));
    query = match workspace_type {
        LockWorkspaceType::Personal => query
            .filter(background_task::Column::CreatorUserId.eq(workspace_id))
            .filter(background_task::Column::TeamId.is_null()),
        LockWorkspaceType::Team => query.filter(background_task::Column::TeamId.eq(workspace_id)),
    };
    query.lock_shared().one(db).await.map_err(AsterError::from)
}

/// Loads a task by id under an exclusive row lock, inside the caller's transaction.
/// Returns `None` when no row matches.
pub async fn find_by_id_for_update<C: ConnectionTrait>(
    db: &C,
    id: i64,
) -> Result<Option<background_task::Model>> {
    BackgroundTask::find_by_id(id)
        .lock_exclusive()
        .one(db)
        .await
        .map_err(AsterError::from)
}

/// Finds a task by its dedupe key, regardless of status. Returns `None` when the
/// key is unset on every row or no row carries it.
pub async fn find_by_dedupe_key<C: ConnectionTrait>(
    db: &C,
    dedupe_key: &str,
) -> Result<Option<background_task::Model>> {
    BackgroundTask::find()
        .filter(background_task::Column::DedupeKey.eq(dedupe_key))
        .one(db)
        .await
        .map_err(AsterError::from)
}

/// Returns one offset page of the user's personal tasks (personal scope only,
/// `team_id` NULL), newest first, together with the total matching count.
pub async fn find_paginated_personal(
    db: &DatabaseConnection,
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

/// Returns one offset page of a team's tasks, newest first, together with the
/// total matching count.
pub async fn find_paginated_team(
    db: &DatabaseConnection,
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

/// Returns one offset page of all tasks with default admin filters and
/// `UpdatedAt` descending order, together with the total matching count.
pub async fn find_paginated_all(
    db: &DatabaseConnection,
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

/// Returns one offset page of all tasks after applying admin filters, ordered by
/// the requested sort key (always tie-broken by id) and direction, together with
/// the total matching count.
pub async fn find_paginated_all_filtered(
    db: &DatabaseConnection,
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

/// Returns the most recently updated tasks (no scope filter), at most `limit` rows.
pub async fn list_recent(
    db: &DatabaseConnection,
    limit: u64,
) -> Result<Vec<background_task::Model>> {
    BackgroundTask::find()
        .order_by_desc(background_task::Column::UpdatedAt)
        .limit(limit)
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// Finds the most recently updated `SystemRuntime` task whose stored payload
/// matches `payload_json` exactly. Returns `None` when no runtime task carries it.
pub async fn find_latest_system_runtime_by_payload(
    db: &DatabaseConnection,
    payload_json: &StoredTaskPayload,
) -> Result<Option<background_task::Model>> {
    BackgroundTask::find()
        .filter(background_task::Column::Kind.eq(BackgroundTaskKind::SystemRuntime))
        .filter(background_task::Column::PayloadJson.eq(payload_json.clone()))
        .order_by_desc(background_task::Column::UpdatedAt)
        .one(db)
        .await
        .map_err(AsterError::from)
}

/// Counts tasks currently in `Processing` or `Canceling` status, across all kinds.
pub async fn count_processing(db: &DatabaseConnection) -> Result<u64> {
    BackgroundTask::find()
        .filter(background_task::Column::Status.is_in([
            BackgroundTaskStatus::Processing,
            BackgroundTaskStatus::Canceling,
        ]))
        .count(db)
        .await
        .map_err(AsterError::from)
}

/// Counts tasks waiting to be claimed (`Pending` or `Retry`), including those
/// whose `next_run_at` has not been reached yet.
pub async fn count_pending_or_retry(db: &DatabaseConnection) -> Result<u64> {
    BackgroundTask::find()
        .filter(
            background_task::Column::Status
                .is_in([BackgroundTaskStatus::Pending, BackgroundTaskStatus::Retry]),
        )
        .count(db)
        .await
        .map_err(AsterError::from)
}

/// Counts tasks in `Processing` or `Canceling` status whose lease is still valid
/// (non-null and not yet expired at `now`) for the given kinds. An empty kind
/// list short-circuits to zero.
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

    Ok(aster_forge_utils::numbers::i64_to_u64(
        count,
        "active processing task count",
    )?)
}

/// Finds the most recently created task for the given kind and display name,
/// across all statuses. Returns `None` when no task matches.
pub async fn find_latest_by_kind_and_display_name(
    db: &DatabaseConnection,
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

/// Returns the age in seconds of the oldest `Pending` or `Retry` task, measured
/// from its creation time to now. Returns `0.0` when no such task exists.
pub async fn oldest_pending_age_seconds(db: &DatabaseConnection) -> Result<f64> {
    let oldest = BackgroundTask::find()
        .select_only()
        .column_as(
            Expr::col(background_task::Column::CreatedAt).min(),
            "oldest",
        )
        .filter(
            background_task::Column::Status
                .is_in([BackgroundTaskStatus::Pending, BackgroundTaskStatus::Retry]),
        )
        .into_tuple::<Option<DateTime<Utc>>>()
        .one(db)
        .await?
        .flatten();
    Ok(oldest.map_or(0.0, |time| {
        Utc::now()
            .signed_duration_since(time)
            .to_std()
            .unwrap_or_default()
            .as_secs_f64()
    }))
}

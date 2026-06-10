use chrono::{DateTime, Utc};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, ExprTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Select, sea_query::Expr,
};

use super::common::{AdminTaskFilters, active_processing_by_kinds_condition, apply_admin_filters};
use crate::api::pagination::{AdminTaskSortBy, SortOrder};
use crate::db::repository::pagination_repo::fetch_offset_page;
use crate::db::repository::sort::{order_by_column_with_id, order_by_id};
use crate::entities::background_task::{self, Entity as BackgroundTask};
use crate::errors::{AsterError, Result};
use crate::types::{BackgroundTaskKind, BackgroundTaskStatus, StoredTaskPayload};

pub async fn find_by_id(db: &DatabaseConnection, id: i64) -> Result<background_task::Model> {
    BackgroundTask::find_by_id(id)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::record_not_found(format!("task #{id}")))
}

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

pub async fn count_processing(db: &DatabaseConnection) -> Result<u64> {
    BackgroundTask::find()
        .filter(background_task::Column::Status.eq(BackgroundTaskStatus::Processing))
        .count(db)
        .await
        .map_err(AsterError::from)
}

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

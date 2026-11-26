//! Durable business activity persistence. Callers own the transaction and authorization.
use crate::errors::{AsterError, Result};
use aster_drive_model::entities::workspace_activity::{self, Entity};
use chrono::{DateTime, Utc};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};

/// Appends a single activity event without returning the inserted row; the
/// caller owns the transaction.
pub async fn append<C: ConnectionTrait>(
    db: &C,
    model: workspace_activity::ActiveModel,
) -> Result<()> {
    Entity::insert(model)
        .exec_without_returning(db)
        .await
        .map(|_| ())
        .map_err(AsterError::from)
}

/// Keep each statement below conservative SQLite bind limits (14 values per event).
pub async fn append_many<C: ConnectionTrait>(
    db: &C,
    models: Vec<workspace_activity::ActiveModel>,
) -> Result<()> {
    let mut rows = models.into_iter();
    loop {
        let chunk = rows.by_ref().take(50).collect::<Vec<_>>();
        if chunk.is_empty() {
            break;
        }
        Entity::insert_many(chunk)
            .exec_without_returning(db)
            .await
            .map_err(AsterError::from)?;
    }
    Ok(())
}

/// Filter and paging input for activity listing; exactly one workspace scope must be set.
pub struct ActivityQuery<'a> {
    /// Team scope: events recorded on team workspaces.
    pub team_id: Option<i64>,
    /// Personal scope: events with no team and this owner. Ignored when `team_id` is set.
    pub owner_user_id: Option<i64>,
    /// Restricts the stream to one entity, given as (entity type, id).
    pub entity: Option<(&'a str, i64)>,
    /// Exclusive upper bound on event id (strictly older events only).
    pub before_id: Option<i64>,
    /// Inclusive upper bound on event id.
    pub at_most_id: Option<i64>,
    /// Maximum events to return, capped at 200 by the repository.
    pub limit: u64,
}

/// Lists activity events newest-first under the query's scope and filters.
/// Requires `team_id` or `owner_user_id`; without a workspace scope the call
/// is rejected as forbidden.
pub async fn list<C: ConnectionTrait>(
    db: &C,
    input: ActivityQuery<'_>,
) -> Result<Vec<workspace_activity::Model>> {
    let mut q = Entity::find();
    if let Some(team_id) = input.team_id {
        q = q.filter(workspace_activity::Column::TeamId.eq(team_id));
    } else if let Some(owner) = input.owner_user_id {
        q = q
            .filter(workspace_activity::Column::TeamId.is_null())
            .filter(workspace_activity::Column::OwnerUserId.eq(owner));
    } else {
        return Err(AsterError::auth_forbidden("workspace scope is required"));
    }
    if let Some((kind, id)) = input.entity {
        q = q
            .filter(workspace_activity::Column::EntityType.eq(kind))
            .filter(workspace_activity::Column::EntityId.eq(id));
    }
    if let Some(id) = input.before_id {
        q = q.filter(workspace_activity::Column::Id.lt(id));
    }
    if let Some(id) = input.at_most_id {
        q = q.filter(workspace_activity::Column::Id.lte(id));
    }
    q.order_by_desc(workspace_activity::Column::Id)
        .limit(input.limit.min(201))
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// Returns a snapshot checkpoint for a team export: the newest event id and the
/// number of events up to it, capped at `max_rows + 1` so callers can detect
/// truncation. Returns `None` when the team has no events.
pub async fn export_snapshot<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    max_rows: u64,
) -> Result<Option<(i64, u64)>> {
    use sea_orm::PaginatorTrait;
    let max = Entity::find()
        .select_only()
        .column(workspace_activity::Column::Id)
        .filter(workspace_activity::Column::TeamId.eq(team_id))
        .order_by_desc(workspace_activity::Column::Id)
        .into_tuple::<i64>()
        .one(db)
        .await?;
    let Some(max) = max else {
        return Ok(None);
    };
    let count = Entity::find()
        .select_only()
        .column(workspace_activity::Column::Id)
        .filter(workspace_activity::Column::TeamId.eq(team_id))
        .filter(workspace_activity::Column::Id.lte(max))
        .limit(max_rows.saturating_add(1))
        .count(db)
        .await?;
    Ok(Some((max, count)))
}

/// Removes events created before `cutoff` in bounded batches (400 ids each),
/// yielding between batches so foreground requests keep scheduling, and returns
/// the total number of removed rows.
pub async fn delete_before<C: ConnectionTrait>(db: &C, cutoff: DateTime<Utc>) -> Result<u64> {
    let mut deleted = 0u64;
    loop {
        let ids = Entity::find()
            .select_only()
            .column(workspace_activity::Column::Id)
            .filter(workspace_activity::Column::CreatedAt.lt(cutoff))
            .order_by_asc(workspace_activity::Column::CreatedAt)
            .order_by_asc(workspace_activity::Column::Id)
            .limit(400)
            .into_tuple::<i64>()
            .all(db)
            .await?;
        if ids.is_empty() {
            return Ok(deleted);
        }
        deleted += Entity::delete_many()
            .filter(workspace_activity::Column::Id.is_in(ids))
            .exec(db)
            .await?
            .rows_affected;
        tokio::task::yield_now().await;
    }
}

/// Aggregate in the DB rather than transferring every activity to the overview service.
/// Explicit UTC boundaries preserve local-day and DST semantics across all DB backends.
pub async fn action_counts_in_windows<C: ConnectionTrait>(
    db: &C,
    windows: &[(DateTime<Utc>, DateTime<Utc>)],
) -> Result<Vec<(i32, String, i64)>> {
    use sea_orm::{
        Condition,
        sea_query::{Alias, Expr},
    };
    let Some((first_start, first_end)) = windows.first() else {
        return Ok(Vec::new());
    };
    let condition = |start: DateTime<Utc>, end: DateTime<Utc>| {
        Condition::all()
            .add(workspace_activity::Column::CreatedAt.gte(start))
            .add(workspace_activity::Column::CreatedAt.lt(end))
    };
    let mut bucket = Expr::case(condition(*first_start, *first_end), 0i32);
    let (mut start, mut end) = (*first_start, *first_end);
    for (index, (window_start, window_end)) in windows.iter().enumerate().skip(1) {
        let index = i32::try_from(index)
            .map_err(|_| AsterError::validation_error("too many activity report windows"))?;
        bucket = bucket.case(condition(*window_start, *window_end), index);
        start = start.min(*window_start);
        end = end.max(*window_end);
    }
    let is_deletion = || {
        Condition::any()
            .add(workspace_activity::Column::Action.eq("file_delete"))
            .add(workspace_activity::Column::Action.eq("folder_delete"))
    };
    // One deletion operation may touch files and folders; collapse both actions before
    // grouping, and count its operation once rather than counting every descendant.
    let action = Expr::case(is_deletion(), "file_delete")
        .finally(Expr::col(workspace_activity::Column::Action));
    let identity = Expr::case(
        is_deletion(),
        Expr::col(workspace_activity::Column::OperationId),
    )
    .finally(Expr::col(workspace_activity::Column::EventId));
    Entity::find()
        .select_only()
        .expr_as(bucket.finally(-1i32), "day_index")
        .expr_as(action, "report_action")
        .expr_as(
            sea_orm::ExprTrait::count_distinct(Expr::expr(identity)),
            "event_count",
        )
        .filter(condition(start, end))
        .group_by(Expr::col(Alias::new("day_index")))
        .group_by(Expr::col(Alias::new("report_action")))
        .into_tuple::<(i32, String, i64)>()
        .all(db)
        .await
        .map_err(AsterError::from)
}

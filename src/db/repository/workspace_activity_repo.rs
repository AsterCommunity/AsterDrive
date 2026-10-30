//! Durable business activity persistence. Callers own the transaction and authorization.
use crate::errors::{AsterError, Result};
use aster_drive_model::entities::workspace_activity::{self, Entity};
use chrono::{DateTime, Utc};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};

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

pub struct ActivityQuery<'a> {
    pub team_id: Option<i64>,
    pub owner_user_id: Option<i64>,
    pub entity: Option<(&'a str, i64)>,
    pub before_id: Option<i64>,
    pub at_most_id: Option<i64>,
    pub limit: u64,
}

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
        // Each statement releases its DB lock; give foreground requests a scheduling point.
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

//! Durable business activity persistence. Callers own the transaction and authorization.
use crate::errors::{AsterError, Result};
use aster_drive_model::entities::workspace_activity::{self, Entity};
use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect,
};

pub async fn append<C: ConnectionTrait>(
    db: &C,
    model: workspace_activity::ActiveModel,
) -> Result<workspace_activity::Model> {
    model.insert(db).await.map_err(AsterError::from)
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
) -> Result<Option<(i64, u64)>> {
    use sea_orm::PaginatorTrait;
    let max = Entity::find()
        .filter(workspace_activity::Column::TeamId.eq(team_id))
        .order_by_desc(workspace_activity::Column::Id)
        .one(db)
        .await?;
    let Some(max) = max else {
        return Ok(None);
    };
    let count = Entity::find()
        .filter(workspace_activity::Column::TeamId.eq(team_id))
        .filter(workspace_activity::Column::Id.lte(max.id))
        .count(db)
        .await?;
    Ok(Some((max.id, count)))
}

pub async fn delete_before<C: ConnectionTrait>(db: &C, cutoff: DateTime<Utc>) -> Result<u64> {
    Ok(Entity::delete_many()
        .filter(workspace_activity::Column::CreatedAt.lt(cutoff))
        .exec(db)
        .await
        .map_err(AsterError::from)?
        .rows_affected)
}

pub async fn action_page_in_range<C: ConnectionTrait>(
    db: &C,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    after: Option<i64>,
    limit: u64,
) -> Result<Vec<(i64, String, DateTime<Utc>)>> {
    let mut query = Entity::find()
        .select_only()
        .columns([
            workspace_activity::Column::Id,
            workspace_activity::Column::Action,
            workspace_activity::Column::CreatedAt,
        ])
        .filter(workspace_activity::Column::CreatedAt.gte(start))
        .filter(workspace_activity::Column::CreatedAt.lt(end));
    if let Some(id) = after {
        query = query.filter(workspace_activity::Column::Id.gt(id));
    }
    query
        .order_by_asc(workspace_activity::Column::Id)
        .limit(limit)
        .into_tuple()
        .all(db)
        .await
        .map_err(AsterError::from)
}

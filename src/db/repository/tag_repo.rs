//! 仓储模块：`tag_repo`。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait,
    IntoActiveModel, PaginatorTrait, QueryFilter, QueryOrder, Set, sea_query::Expr,
};

use crate::db::repository::{pagination_repo, search_query::lower_like_condition};
use crate::entities::tag::{self, Entity as Tag};
use crate::errors::{AsterError, Result};
use crate::types::TagScopeType;

pub async fn create(
    db: &DatabaseConnection,
    scope_type: TagScopeType,
    owner_user_id: Option<i64>,
    team_id: Option<i64>,
    name: &str,
    normalized_name: &str,
    color: &str,
) -> Result<tag::Model> {
    let now = Utc::now();
    tag::ActiveModel {
        scope_type: Set(scope_type),
        owner_user_id: Set(owner_user_id),
        team_id: Set(team_id),
        name: Set(name.to_string()),
        normalized_name: Set(normalized_name.to_string()),
        color: Set(color.to_string()),
        sort_order: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(db)
    .await
    .map_err(AsterError::from)
}

pub async fn find_by_id(db: &DatabaseConnection, id: i64) -> Result<tag::Model> {
    Tag::find_by_id(id)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::record_not_found("tag not found"))
}

pub async fn find_by_ids(db: &DatabaseConnection, ids: &[i64]) -> Result<Vec<tag::Model>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }

    Tag::find()
        .filter(tag::Column::Id.is_in(ids.iter().copied()))
        .order_by_asc(tag::Column::SortOrder)
        .order_by_asc(tag::Column::Name)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn list_personal(db: &DatabaseConnection, owner_user_id: i64) -> Result<Vec<tag::Model>> {
    Tag::find()
        .filter(tag::Column::ScopeType.eq(TagScopeType::Personal))
        .filter(tag::Column::OwnerUserId.eq(owner_user_id))
        .order_by_asc(tag::Column::SortOrder)
        .order_by_asc(tag::Column::Name)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn list_personal_page(
    db: &DatabaseConnection,
    owner_user_id: i64,
    limit: u64,
    offset: u64,
    search: Option<&str>,
) -> Result<(Vec<tag::Model>, u64)> {
    let mut query = Tag::find()
        .filter(tag::Column::ScopeType.eq(TagScopeType::Personal))
        .filter(tag::Column::OwnerUserId.eq(owner_user_id))
        .order_by_asc(tag::Column::SortOrder)
        .order_by_asc(tag::Column::Name);

    if let Some(search) = search.map(str::trim).filter(|search| !search.is_empty()) {
        query = query.filter(lower_like_condition(tag::Column::NormalizedName, search));
    }

    pagination_repo::fetch_offset_page(db, query, limit, offset).await
}

pub async fn list_team(db: &DatabaseConnection, team_id: i64) -> Result<Vec<tag::Model>> {
    Tag::find()
        .filter(tag::Column::ScopeType.eq(TagScopeType::Team))
        .filter(tag::Column::TeamId.eq(team_id))
        .order_by_asc(tag::Column::SortOrder)
        .order_by_asc(tag::Column::Name)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn list_team_page(
    db: &DatabaseConnection,
    team_id: i64,
    limit: u64,
    offset: u64,
    search: Option<&str>,
) -> Result<(Vec<tag::Model>, u64)> {
    let mut query = Tag::find()
        .filter(tag::Column::ScopeType.eq(TagScopeType::Team))
        .filter(tag::Column::TeamId.eq(team_id))
        .order_by_asc(tag::Column::SortOrder)
        .order_by_asc(tag::Column::Name);

    if let Some(search) = search.map(str::trim).filter(|search| !search.is_empty()) {
        query = query.filter(lower_like_condition(tag::Column::NormalizedName, search));
    }

    pagination_repo::fetch_offset_page(db, query, limit, offset).await
}

pub async fn find_personal_by_normalized_name(
    db: &DatabaseConnection,
    owner_user_id: i64,
    normalized_name: &str,
) -> Result<Option<tag::Model>> {
    Tag::find()
        .filter(tag::Column::ScopeType.eq(TagScopeType::Personal))
        .filter(tag::Column::OwnerUserId.eq(owner_user_id))
        .filter(tag::Column::NormalizedName.eq(normalized_name))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_team_by_normalized_name(
    db: &DatabaseConnection,
    team_id: i64,
    normalized_name: &str,
) -> Result<Option<tag::Model>> {
    Tag::find()
        .filter(tag::Column::ScopeType.eq(TagScopeType::Team))
        .filter(tag::Column::TeamId.eq(team_id))
        .filter(tag::Column::NormalizedName.eq(normalized_name))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn update(
    db: &DatabaseConnection,
    tag: tag::Model,
    name: Option<&str>,
    normalized_name: Option<&str>,
    color: Option<&str>,
) -> Result<tag::Model> {
    let mut active = tag.into_active_model();
    if let Some(name) = name {
        active.name = Set(name.to_string());
    }
    if let Some(normalized_name) = normalized_name {
        active.normalized_name = Set(normalized_name.to_string());
    }
    if let Some(color) = color {
        active.color = Set(color.to_string());
    }
    active.updated_at = Set(Utc::now());
    active.update(db).await.map_err(AsterError::from)
}

pub async fn delete<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    Tag::delete_by_id(id)
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

pub async fn count(db: &DatabaseConnection) -> Result<u64> {
    Tag::find().count(db).await.map_err(AsterError::from)
}

pub async fn touch(db: &DatabaseConnection, id: i64) -> Result<()> {
    Tag::update_many()
        .col_expr(tag::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(tag::Column::Id.eq(id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

//! 仓储模块：`managed_follower_repo`。

use crate::api::pagination::{AdminRemoteNodeSortBy, SortOrder};
use crate::db::repository::pagination_repo::fetch_offset_page;
use crate::db::repository::sort::{order_by_column_with_id, order_by_id};
use crate::entities::managed_follower::{self, Entity as ManagedFollower};
use crate::errors::{AsterError, Result};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, Select,
    Set,
};

pub async fn find_by_id<C: ConnectionTrait>(db: &C, id: i64) -> Result<managed_follower::Model> {
    ManagedFollower::find_by_id(id)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::record_not_found(format!("managed_follower #{id}")))
}

pub async fn find_by_access_key<C: ConnectionTrait>(
    db: &C,
    access_key: &str,
) -> Result<Option<managed_follower::Model>> {
    ManagedFollower::find()
        .filter(managed_follower::Column::AccessKey.eq(access_key))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_all<C: ConnectionTrait>(db: &C) -> Result<Vec<managed_follower::Model>> {
    ManagedFollower::find()
        .order_by_desc(managed_follower::Column::CreatedAt)
        .order_by_desc(managed_follower::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_paginated<C: ConnectionTrait>(
    db: &C,
    limit: u64,
    offset: u64,
    sort_by: AdminRemoteNodeSortBy,
    sort_order: SortOrder,
) -> Result<(Vec<managed_follower::Model>, u64)> {
    fetch_offset_page(
        db,
        apply_admin_remote_node_sort(ManagedFollower::find(), sort_by, sort_order),
        limit,
        offset,
    )
    .await
}

fn apply_admin_remote_node_sort(
    query: Select<ManagedFollower>,
    sort_by: AdminRemoteNodeSortBy,
    sort_order: SortOrder,
) -> Select<ManagedFollower> {
    match sort_by {
        AdminRemoteNodeSortBy::Id => order_by_id(query, managed_follower::Column::Id, sort_order),
        AdminRemoteNodeSortBy::Name => order_by_column_with_id(
            query,
            managed_follower::Column::Name,
            sort_order,
            managed_follower::Column::Id,
        ),
        AdminRemoteNodeSortBy::BaseUrl => order_by_column_with_id(
            query,
            managed_follower::Column::BaseUrl,
            sort_order,
            managed_follower::Column::Id,
        ),
        AdminRemoteNodeSortBy::IsEnabled => order_by_column_with_id(
            query,
            managed_follower::Column::IsEnabled,
            sort_order,
            managed_follower::Column::Id,
        ),
        AdminRemoteNodeSortBy::LastCheckedAt => order_by_column_with_id(
            query,
            managed_follower::Column::LastCheckedAt,
            sort_order,
            managed_follower::Column::Id,
        ),
        AdminRemoteNodeSortBy::CreatedAt => order_by_column_with_id(
            query,
            managed_follower::Column::CreatedAt,
            sort_order,
            managed_follower::Column::Id,
        ),
        AdminRemoteNodeSortBy::UpdatedAt => order_by_column_with_id(
            query,
            managed_follower::Column::UpdatedAt,
            sort_order,
            managed_follower::Column::Id,
        ),
    }
}

pub async fn create<C: ConnectionTrait>(
    db: &C,
    model: managed_follower::ActiveModel,
) -> Result<managed_follower::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn update<C: ConnectionTrait>(
    db: &C,
    model: managed_follower::ActiveModel,
) -> Result<managed_follower::Model> {
    model.update(db).await.map_err(AsterError::from)
}

pub async fn delete<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    let result = ManagedFollower::delete_by_id(id)
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    if result.rows_affected == 0 {
        return Err(AsterError::record_not_found(format!(
            "managed_follower #{id}"
        )));
    }
    Ok(())
}

pub async fn touch_probe_result<C: ConnectionTrait>(
    db: &C,
    id: i64,
    last_capabilities: String,
    last_error: String,
    last_checked_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<managed_follower::Model> {
    let existing = find_by_id(db, id).await?;
    let mut active: managed_follower::ActiveModel = existing.into();
    active.last_capabilities = Set(last_capabilities);
    active.last_error = Set(last_error);
    active.last_checked_at = Set(last_checked_at);
    active.updated_at = Set(chrono::Utc::now());
    update(db, active).await
}

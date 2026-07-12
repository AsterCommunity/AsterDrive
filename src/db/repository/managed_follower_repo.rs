//! 仓储模块：`managed_follower_repo`。

use crate::api::pagination::AdminRemoteNodeSortBy;
use crate::entities::managed_follower::{self, Entity as ManagedFollower};
use crate::errors::{AsterError, Result};
use aster_forge_api::SortOrder;
use aster_forge_db::pagination::fetch_offset_page;
use aster_forge_db::sort::{order_by_column_with_id, order_by_id};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, Select, Set,
};

pub async fn find_by_id<C: ConnectionTrait>(db: &C, id: i64) -> Result<managed_follower::Model> {
    ManagedFollower::find_by_id(id)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::record_not_found(format!("managed_follower #{id}")))
}

pub async fn find_by_access_key(
    db: &DatabaseConnection,
    access_key: &str,
) -> Result<Option<managed_follower::Model>> {
    ManagedFollower::find()
        .filter(managed_follower::Column::AccessKey.eq(access_key))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_all(db: &DatabaseConnection) -> Result<Vec<managed_follower::Model>> {
    ManagedFollower::find()
        .order_by_desc(managed_follower::Column::CreatedAt)
        .order_by_desc(managed_follower::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_paginated(
    db: &DatabaseConnection,
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

pub async fn create(
    db: &DatabaseConnection,
    model: managed_follower::ActiveModel,
) -> Result<managed_follower::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn update(
    db: &DatabaseConnection,
    model: managed_follower::ActiveModel,
) -> Result<managed_follower::Model> {
    model.update(db).await.map_err(AsterError::from)
}

pub async fn delete(db: &DatabaseConnection, id: i64) -> Result<()> {
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

pub async fn touch_probe_result(
    db: &DatabaseConnection,
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

pub async fn touch_tunnel_result(
    db: &DatabaseConnection,
    id: i64,
    tunnel_last_error: String,
    tunnel_last_seen_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<managed_follower::Model> {
    let existing = find_by_id(db, id).await?;
    let mut active: managed_follower::ActiveModel = existing.into();
    active.tunnel_last_error = Set(tunnel_last_error);
    active.tunnel_last_seen_at = Set(tunnel_last_seen_at);
    // Tunnel 心跳和错误是运行态遥测，不代表远端节点配置被修改。
    // 保持 updated_at 只用于名称、base_url、transport_mode 等管理面变更；
    // 需要按 tunnel 活跃度排序时应显式使用 tunnel_last_seen_at。
    update(db, active).await
}

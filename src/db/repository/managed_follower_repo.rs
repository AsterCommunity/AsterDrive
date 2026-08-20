//! 仓储模块：`managed_follower_repo`。

use crate::api::pagination::AdminRemoteNodeSortBy;
use crate::errors::{AsterError, Result};
use aster_drive_model::entities::managed_follower::{self, Entity as ManagedFollower};
use aster_forge_api::SortOrder;
use aster_forge_db::pagination::fetch_offset_page;
use aster_forge_db::sort::{order_by_column_with_id, order_by_id};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, Select,
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
        AdminRemoteNodeSortBy::LastProbeAt => order_by_column_with_id(
            query,
            managed_follower::Column::LastProbeAt,
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
    last_probe_error: String,
    last_probe_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<managed_follower::Model> {
    ManagedFollower::update_many()
        .col_expr(
            managed_follower::Column::LastCapabilities,
            Expr::value(last_capabilities),
        )
        .col_expr(
            managed_follower::Column::LastProbeError,
            Expr::value(last_probe_error),
        )
        .col_expr(
            managed_follower::Column::LastProbeAt,
            Expr::value(last_probe_at),
        )
        .filter(managed_follower::Column::Id.eq(id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    find_by_id(db, id).await
}

pub async fn touch_tunnel_success(
    db: &DatabaseConnection,
    id: i64,
    tunnel_last_handshake_at: chrono::DateTime<chrono::Utc>,
) -> Result<managed_follower::Model> {
    ManagedFollower::update_many()
        .col_expr(
            managed_follower::Column::TunnelRuntimeError,
            Expr::value(String::new()),
        )
        .col_expr(
            managed_follower::Column::TunnelLastHandshakeAt,
            Expr::value(Some(tunnel_last_handshake_at)),
        )
        .filter(managed_follower::Column::Id.eq(id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    find_by_id(db, id).await
}

pub async fn touch_tunnel_runtime_error(
    db: &DatabaseConnection,
    id: i64,
    tunnel_runtime_error: String,
) -> Result<managed_follower::Model> {
    ManagedFollower::update_many()
        .col_expr(
            managed_follower::Column::TunnelRuntimeError,
            Expr::value(tunnel_runtime_error),
        )
        .filter(managed_follower::Column::Id.eq(id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    find_by_id(db, id).await
}

pub async fn acknowledge_binding_revision<C: ConnectionTrait>(
    db: &C,
    id: i64,
    applied_revision: i64,
) -> Result<()> {
    ManagedFollower::update_many()
        .col_expr(
            managed_follower::Column::BindingAppliedRevision,
            sea_orm::sea_query::Expr::value(applied_revision),
        )
        .filter(managed_follower::Column::Id.eq(id))
        .filter(managed_follower::Column::BindingAppliedRevision.lt(applied_revision))
        .filter(managed_follower::Column::BindingRevision.gte(applied_revision))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

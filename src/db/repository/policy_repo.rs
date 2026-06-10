//! 仓储模块：`policy_repo`。

use crate::api::pagination::{AdminPolicySortBy, SortOrder};
use crate::db::repository::pagination_repo::fetch_offset_page;
use crate::db::repository::sort::{order_by_column_with_id, order_by_id};
use crate::entities::storage_policy::{self, Entity as StoragePolicy};
use crate::errors::{AsterError, Result};
use crate::types::DriverType;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait,
    ExprTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Select, Set, sea_query::Expr,
};

pub async fn find_by_id<C: ConnectionTrait>(db: &C, id: i64) -> Result<storage_policy::Model> {
    StoragePolicy::find_by_id(id)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::storage_policy_not_found(format!("policy #{id}")))
}

pub async fn lock_by_id<C: ConnectionTrait>(db: &C, id: i64) -> Result<storage_policy::Model> {
    match db.get_database_backend() {
        DbBackend::Postgres | DbBackend::MySql => StoragePolicy::find_by_id(id)
            .lock_exclusive()
            .one(db)
            .await
            .map_err(AsterError::from)?
            .ok_or_else(|| AsterError::storage_policy_not_found(format!("policy #{id}"))),
        DbBackend::Sqlite => find_by_id(db, id).await,
        _ => find_by_id(db, id).await,
    }
}

pub async fn find_default<C: ConnectionTrait>(db: &C) -> Result<Option<storage_policy::Model>> {
    StoragePolicy::find()
        .filter(storage_policy::Column::IsDefault.eq(true))
        .order_by_asc(storage_policy::Column::Id)
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_all<C: ConnectionTrait>(db: &C) -> Result<Vec<storage_policy::Model>> {
    StoragePolicy::find()
        .order_by_asc(storage_policy::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_paginated(
    db: &DatabaseConnection,
    limit: u64,
    offset: u64,
    sort_by: AdminPolicySortBy,
    sort_order: SortOrder,
) -> Result<(Vec<storage_policy::Model>, u64)> {
    fetch_offset_page(
        db,
        apply_admin_policy_sort(StoragePolicy::find(), sort_by, sort_order),
        limit,
        offset,
    )
    .await
}

fn apply_admin_policy_sort(
    query: Select<StoragePolicy>,
    sort_by: AdminPolicySortBy,
    sort_order: SortOrder,
) -> Select<StoragePolicy> {
    match sort_by {
        AdminPolicySortBy::Id => order_by_id(query, storage_policy::Column::Id, sort_order),
        AdminPolicySortBy::Name => order_by_column_with_id(
            query,
            storage_policy::Column::Name,
            sort_order,
            storage_policy::Column::Id,
        ),
        AdminPolicySortBy::DriverType => order_by_column_with_id(
            query,
            storage_policy::Column::DriverType,
            sort_order,
            storage_policy::Column::Id,
        ),
        AdminPolicySortBy::Endpoint => order_by_column_with_id(
            query,
            storage_policy::Column::Endpoint,
            sort_order,
            storage_policy::Column::Id,
        ),
        AdminPolicySortBy::Bucket => order_by_column_with_id(
            query,
            storage_policy::Column::Bucket,
            sort_order,
            storage_policy::Column::Id,
        ),
        AdminPolicySortBy::IsDefault => order_by_column_with_id(
            query,
            storage_policy::Column::IsDefault,
            sort_order,
            storage_policy::Column::Id,
        ),
        AdminPolicySortBy::CreatedAt => order_by_column_with_id(
            query,
            storage_policy::Column::CreatedAt,
            sort_order,
            storage_policy::Column::Id,
        ),
        AdminPolicySortBy::UpdatedAt => order_by_column_with_id(
            query,
            storage_policy::Column::UpdatedAt,
            sort_order,
            storage_policy::Column::Id,
        ),
    }
}

pub async fn count_by_remote_node_id(db: &DatabaseConnection, remote_node_id: i64) -> Result<u64> {
    StoragePolicy::find()
        .filter(storage_policy::Column::RemoteNodeId.eq(remote_node_id))
        .count(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_remote_node_id(
    db: &DatabaseConnection,
    remote_node_id: i64,
) -> Result<Vec<storage_policy::Model>> {
    StoragePolicy::find()
        .filter(storage_policy::Column::RemoteNodeId.eq(remote_node_id))
        .order_by_asc(storage_policy::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn create<C: ConnectionTrait>(
    db: &C,
    model: storage_policy::ActiveModel,
) -> Result<storage_policy::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

/// 清除所有系统策略的 is_default（新 default 设置前调用）
pub async fn clear_system_default<C: ConnectionTrait>(db: &C) -> Result<()> {
    let defaults = StoragePolicy::find()
        .filter(storage_policy::Column::IsDefault.eq(true))
        .all(db)
        .await
        .map_err(AsterError::from)?;
    for m in defaults {
        let mut active: storage_policy::ActiveModel = m.into();
        active.is_default = Set(false);
        active.update(db).await.map_err(AsterError::from)?;
    }
    Ok(())
}

pub async fn set_only_default<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    find_by_id(db, id).await?;

    StoragePolicy::update_many()
        .col_expr(
            storage_policy::Column::IsDefault,
            Expr::case(Expr::col(storage_policy::Column::Id).eq(id), true)
                .finally(false)
                .into(),
        )
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

pub async fn promote_s3_compatible_driver<C: ConnectionTrait>(
    db: &C,
    id: i64,
    source_driver_type: DriverType,
    target_driver_type: DriverType,
    endpoint: String,
) -> Result<()> {
    let policy = lock_by_id(db, id).await?;
    if policy.driver_type != source_driver_type {
        return Err(AsterError::validation_error(format!(
            "storage policy #{id} is not a {} policy",
            source_driver_type.as_str()
        )));
    }

    let mut active: storage_policy::ActiveModel = policy.into();
    active.driver_type = Set(target_driver_type);
    active.endpoint = Set(endpoint);
    active.updated_at = Set(chrono::Utc::now());
    active.update(db).await.map_err(AsterError::from)?;
    Ok(())
}

pub async fn delete<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    let result = StoragePolicy::delete_by_id(id)
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    if result.rows_affected == 0 {
        return Err(AsterError::storage_policy_not_found(format!(
            "policy #{id}"
        )));
    }
    Ok(())
}

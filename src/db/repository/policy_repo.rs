//! 仓储模块：`policy_repo`。

use crate::api::pagination::AdminPolicySortBy;
use crate::errors::{AsterError, Result};
use aster_drive_model::entities::storage_policy::{self, Entity as StoragePolicy};
use aster_forge_api::SortOrder;
use aster_forge_db::pagination::fetch_offset_page;
use aster_forge_db::sort::{order_by_column_with_id, order_by_id};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait,
    ExprTrait, QueryFilter, QueryOrder, QuerySelect, Select, Set, sea_query::Expr,
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
        AdminPolicySortBy::ConnectorId => order_by_column_with_id(
            query,
            storage_policy::Column::ConnectorId,
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

pub async fn promote_connector<C: ConnectionTrait>(
    db: &C,
    policy: storage_policy::Model,
    target_connector_id: String,
    storage_config: aster_drive_model::types::StoredStoragePolicyConfig,
) -> Result<storage_policy::Model> {
    let mut active: storage_policy::ActiveModel = policy.into();
    active.connector_id = Set(target_connector_id);
    active.storage_config = Set(storage_config);
    active.updated_at = Set(chrono::Utc::now());
    active.update(db).await.map_err(AsterError::from)
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

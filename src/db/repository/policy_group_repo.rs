//! 仓储模块：`policy_group_repo`。

use crate::api::pagination::{AdminPolicyGroupSortBy, SortOrder};
use crate::db::repository::pagination_repo::fetch_offset_page;
use crate::db::repository::sort::{order_by_column_with_id, order_by_id};
use crate::entities::{
    storage_policy_group::{self, Entity as StoragePolicyGroup},
    storage_policy_group_item::{self, Entity as StoragePolicyGroupItem},
    user::{self, Entity as User},
};
use crate::errors::{AsterError, Result};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, ExprTrait,
    PaginatorTrait, QueryFilter, QueryOrder, Select, sea_query::Expr,
};

pub async fn find_group_by_id<C: ConnectionTrait>(
    db: &C,
    id: i64,
) -> Result<storage_policy_group::Model> {
    StoragePolicyGroup::find_by_id(id)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::record_not_found(format!("storage_policy_group #{id}")))
}

pub async fn find_default_group<C: ConnectionTrait>(
    db: &C,
) -> Result<Option<storage_policy_group::Model>> {
    StoragePolicyGroup::find()
        .filter(storage_policy_group::Column::IsDefault.eq(true))
        .order_by_asc(storage_policy_group::Column::Id)
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_all_groups<C: ConnectionTrait>(
    db: &C,
) -> Result<Vec<storage_policy_group::Model>> {
    StoragePolicyGroup::find()
        .order_by_asc(storage_policy_group::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_groups_paginated(
    db: &DatabaseConnection,
    limit: u64,
    offset: u64,
    sort_by: AdminPolicyGroupSortBy,
    sort_order: SortOrder,
) -> Result<(Vec<storage_policy_group::Model>, u64)> {
    fetch_offset_page(
        db,
        apply_admin_policy_group_sort(StoragePolicyGroup::find(), sort_by, sort_order),
        limit,
        offset,
    )
    .await
}

fn apply_admin_policy_group_sort(
    query: Select<StoragePolicyGroup>,
    sort_by: AdminPolicyGroupSortBy,
    sort_order: SortOrder,
) -> Select<StoragePolicyGroup> {
    match sort_by {
        AdminPolicyGroupSortBy::Id => {
            order_by_id(query, storage_policy_group::Column::Id, sort_order)
        }
        AdminPolicyGroupSortBy::Name => order_by_column_with_id(
            query,
            storage_policy_group::Column::Name,
            sort_order,
            storage_policy_group::Column::Id,
        ),
        AdminPolicyGroupSortBy::IsEnabled => order_by_column_with_id(
            query,
            storage_policy_group::Column::IsEnabled,
            sort_order,
            storage_policy_group::Column::Id,
        ),
        AdminPolicyGroupSortBy::IsDefault => order_by_column_with_id(
            query,
            storage_policy_group::Column::IsDefault,
            sort_order,
            storage_policy_group::Column::Id,
        ),
        AdminPolicyGroupSortBy::CreatedAt => order_by_column_with_id(
            query,
            storage_policy_group::Column::CreatedAt,
            sort_order,
            storage_policy_group::Column::Id,
        ),
        AdminPolicyGroupSortBy::UpdatedAt => order_by_column_with_id(
            query,
            storage_policy_group::Column::UpdatedAt,
            sort_order,
            storage_policy_group::Column::Id,
        ),
    }
}

pub async fn create_group<C: ConnectionTrait>(
    db: &C,
    model: storage_policy_group::ActiveModel,
) -> Result<storage_policy_group::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn update_group<C: ConnectionTrait>(
    db: &C,
    model: storage_policy_group::ActiveModel,
) -> Result<storage_policy_group::Model> {
    model.update(db).await.map_err(AsterError::from)
}

pub async fn delete_group<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    StoragePolicyGroup::delete_by_id(id)
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

pub async fn set_only_default_group<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    StoragePolicyGroup::update_many()
        .col_expr(
            storage_policy_group::Column::IsDefault,
            Expr::case(Expr::col(storage_policy_group::Column::Id).eq(id), true)
                .finally(false)
                .into(),
        )
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

pub async fn find_group_items<C: ConnectionTrait>(
    db: &C,
    group_id: i64,
) -> Result<Vec<storage_policy_group_item::Model>> {
    StoragePolicyGroupItem::find()
        .filter(storage_policy_group_item::Column::GroupId.eq(group_id))
        .order_by_asc(storage_policy_group_item::Column::Priority)
        .order_by_asc(storage_policy_group_item::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_all_group_items<C: ConnectionTrait>(
    db: &C,
) -> Result<Vec<storage_policy_group_item::Model>> {
    StoragePolicyGroupItem::find()
        .order_by_asc(storage_policy_group_item::Column::GroupId)
        .order_by_asc(storage_policy_group_item::Column::Priority)
        .order_by_asc(storage_policy_group_item::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn create_group_item<C: ConnectionTrait>(
    db: &C,
    model: storage_policy_group_item::ActiveModel,
) -> Result<storage_policy_group_item::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn delete_group_items_by_group<C: ConnectionTrait>(db: &C, group_id: i64) -> Result<u64> {
    let result = StoragePolicyGroupItem::delete_many()
        .filter(storage_policy_group_item::Column::GroupId.eq(group_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

pub async fn count_group_items_by_policy(db: &DatabaseConnection, policy_id: i64) -> Result<u64> {
    StoragePolicyGroupItem::find()
        .filter(storage_policy_group_item::Column::PolicyId.eq(policy_id))
        .count(db)
        .await
        .map_err(AsterError::from)
}

pub async fn count_user_group_assignments<C: ConnectionTrait>(
    db: &C,
    group_id: i64,
) -> Result<u64> {
    User::find()
        .filter(user::Column::PolicyGroupId.eq(group_id))
        .count(db)
        .await
        .map_err(AsterError::from)
}

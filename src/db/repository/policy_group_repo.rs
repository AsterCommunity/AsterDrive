//! 仓储模块：`policy_group_repo`。

use crate::api::pagination::AdminPolicyGroupSortBy;
use crate::errors::{AsterError, Result};
use aster_drive_model::entities::{
    storage_policy_group::{self, Entity as StoragePolicyGroup},
    user::{self, Entity as User},
};
use aster_forge_api::SortOrder;
use aster_forge_db::pagination::fetch_offset_page;
use aster_forge_db::sort::{order_by_column_with_id, order_by_id};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, ExprTrait,
    PaginatorTrait, QueryFilter, QueryOrder, Select, sea_query::Expr,
};

/// Loads a storage policy group by id, returning `record_not_found` when absent.
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

/// Returns the group flagged as default (lowest id wins if the invariant is
/// somehow broken and several exist), or `None` when no default is set.
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

/// Lists every policy group ordered by id.
pub async fn find_all_groups<C: ConnectionTrait>(
    db: &C,
) -> Result<Vec<storage_policy_group::Model>> {
    StoragePolicyGroup::find()
        .order_by_asc(storage_policy_group::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// Returns one offset page of policy groups ordered by the requested admin sort
/// key (tie-broken by id), together with the total matching count.
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

/// Inserts a new policy group from the given active model.
pub async fn create_group<C: ConnectionTrait>(
    db: &C,
    model: storage_policy_group::ActiveModel,
) -> Result<storage_policy_group::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

/// Persists the given group model's changed fields.
pub async fn update_group<C: ConnectionTrait>(
    db: &C,
    model: storage_policy_group::ActiveModel,
) -> Result<storage_policy_group::Model> {
    model.update(db).await.map_err(AsterError::from)
}

/// Deletes a policy group by id; succeeds even when the id is absent.
pub async fn delete_group<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    StoragePolicyGroup::delete_by_id(id)
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

/// Makes the group the only default in a single statement: its `is_default`
/// becomes true and every other group's becomes false. No uniqueness invariant
/// beyond this statement's atomicity is enforced here.
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

/// Counts the users assigned to the group; used before deletion to check the
/// group is not still in use.
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

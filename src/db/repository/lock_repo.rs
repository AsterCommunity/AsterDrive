//! 仓储模块：`lock_repo`。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Select, Set, sea_query::Expr,
};

use crate::api::pagination::AdminLockSortBy;
use crate::errors::{AsterError, Result};
use aster_drive_model::entities::resource_lock::{self, Entity as ResourceLock};
use aster_drive_model::types::{EntityType, LockRootKind};
use aster_forge_api::SortOrder;
use aster_forge_db::pagination::fetch_offset_page;
use aster_forge_db::sort::{order_by_column_with_id, order_by_id};

pub async fn create<C: ConnectionTrait>(
    db: &C,
    model: resource_lock::ActiveModel,
) -> Result<resource_lock::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn find_all(db: &DatabaseConnection) -> Result<Vec<resource_lock::Model>> {
    ResourceLock::find()
        .order_by_asc(resource_lock::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_paginated(
    db: &DatabaseConnection,
    limit: u64,
    offset: u64,
    sort_by: AdminLockSortBy,
    sort_order: SortOrder,
) -> Result<(Vec<resource_lock::Model>, u64)> {
    fetch_offset_page(
        db,
        apply_admin_lock_sort(ResourceLock::find(), sort_by, sort_order),
        limit,
        offset,
    )
    .await
}

fn apply_admin_lock_sort(
    query: Select<ResourceLock>,
    sort_by: AdminLockSortBy,
    sort_order: SortOrder,
) -> Select<ResourceLock> {
    match sort_by {
        AdminLockSortBy::Id => order_by_id(query, resource_lock::Column::Id, sort_order),
        AdminLockSortBy::LockrootPath => order_by_column_with_id(
            query,
            resource_lock::Column::LockrootPath,
            sort_order,
            resource_lock::Column::Id,
        ),
        AdminLockSortBy::RootKind => order_by_column_with_id(
            query,
            resource_lock::Column::RootKind,
            sort_order,
            resource_lock::Column::Id,
        ),
        AdminLockSortBy::HolderUserId => order_by_column_with_id(
            query,
            resource_lock::Column::HolderUserId,
            sort_order,
            resource_lock::Column::Id,
        ),
        AdminLockSortBy::TimeoutAt => order_by_column_with_id(
            query,
            resource_lock::Column::TimeoutAt,
            sort_order,
            resource_lock::Column::Id,
        ),
        AdminLockSortBy::Mode => order_by_column_with_id(
            query,
            resource_lock::Column::Mode,
            sort_order,
            resource_lock::Column::Id,
        ),
        AdminLockSortBy::Depth => order_by_column_with_id(
            query,
            resource_lock::Column::Depth,
            sort_order,
            resource_lock::Column::Id,
        ),
        AdminLockSortBy::CreatedAt => order_by_column_with_id(
            query,
            resource_lock::Column::CreatedAt,
            sort_order,
            resource_lock::Column::Id,
        ),
    }
}

pub async fn find_by_id<C: ConnectionTrait>(
    db: &C,
    id: i64,
) -> Result<Option<resource_lock::Model>> {
    ResourceLock::find_by_id(id)
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_token<C: ConnectionTrait>(
    db: &C,
    token: &str,
) -> Result<Option<resource_lock::Model>> {
    ResourceLock::find()
        .filter(resource_lock::Column::Token.eq(token))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_token_for_update<C: ConnectionTrait>(
    db: &C,
    token: &str,
) -> Result<Option<resource_lock::Model>> {
    ResourceLock::find()
        .filter(resource_lock::Column::Token.eq(token))
        .lock_exclusive()
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_id_for_update<C: ConnectionTrait>(
    db: &C,
    id: i64,
) -> Result<Option<resource_lock::Model>> {
    ResourceLock::find_by_id(id)
        .lock_exclusive()
        .one(db)
        .await
        .map_err(AsterError::from)
}

/// 查询单个资源的第一把锁。
///
/// WebDAV shared lock 允许同一资源存在多把锁；新代码需要完整判断锁集合时，
/// 应优先使用 `find_all_by_entity` / `find_active_by_entity`。
pub async fn find_by_entity<C: ConnectionTrait>(
    db: &C,
    entity_type: EntityType,
    entity_id: i64,
) -> Result<Option<resource_lock::Model>> {
    entity_query(entity_type, entity_id)
        .order_by_asc(resource_lock::Column::Id)
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_all_by_entity<C: ConnectionTrait>(
    db: &C,
    entity_type: EntityType,
    entity_id: i64,
) -> Result<Vec<resource_lock::Model>> {
    entity_query(entity_type, entity_id)
        .order_by_asc(resource_lock::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_all_by_entity_for_update<C: ConnectionTrait>(
    db: &C,
    entity_type: EntityType,
    entity_id: i64,
) -> Result<Vec<resource_lock::Model>> {
    entity_query(entity_type, entity_id)
        .order_by_asc(resource_lock::Column::Id)
        .lock_exclusive()
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// Returns at most one active lock for a resource: the first non-expired row
/// after sorting by `id` ascending. Use `find_all_by_entity` when callers need
/// the full lock set.
pub async fn find_active_by_entity<C: ConnectionTrait>(
    db: &C,
    entity_type: EntityType,
    entity_id: i64,
) -> Result<Option<resource_lock::Model>> {
    let now = Utc::now();
    Ok(find_all_by_entity(db, entity_type, entity_id)
        .await?
        .into_iter()
        .find(|lock| lock.timeout_at.is_none_or(|timeout_at| timeout_at > now)))
}

/// 路径前缀查询（WebDAV deep lock 用）
pub async fn find_by_path_prefix<C: ConnectionTrait>(
    db: &C,
    prefix: &str,
) -> Result<Vec<resource_lock::Model>> {
    ResourceLock::find()
        .filter(resource_lock::Column::LockrootPath.starts_with(prefix))
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_path_prefix_in_namespace<C: ConnectionTrait>(
    db: &C,
    namespace_id: i64,
    prefix: &str,
) -> Result<Vec<resource_lock::Model>> {
    ResourceLock::find()
        .filter(resource_lock::Column::NamespaceId.eq(namespace_id))
        .filter(resource_lock::Column::LockrootPath.starts_with(prefix))
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_path<C: ConnectionTrait>(
    db: &C,
    path: &str,
) -> Result<Vec<resource_lock::Model>> {
    ResourceLock::find()
        .filter(resource_lock::Column::LockrootPath.eq(path))
        .order_by_asc(resource_lock::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn rebind_path<C: ConnectionTrait>(
    db: &C,
    path: &str,
    entity_type: EntityType,
    entity_id: i64,
) -> Result<u64> {
    let (root_kind, folder_id, file_id) = match entity_type {
        EntityType::File => (LockRootKind::File, None, Some(entity_id)),
        EntityType::Folder => (LockRootKind::Folder, Some(entity_id), None),
    };
    let result = ResourceLock::update_many()
        .col_expr(resource_lock::Column::RootKind, Expr::value(root_kind))
        .col_expr(resource_lock::Column::RootFolderId, Expr::value(folder_id))
        .col_expr(resource_lock::Column::RootFileId, Expr::value(file_id))
        .filter(resource_lock::Column::LockrootPath.eq(path))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

pub async fn rebind_path_in_namespace<C: ConnectionTrait>(
    db: &C,
    namespace_id: i64,
    path: &str,
    entity_type: EntityType,
    entity_id: i64,
) -> Result<u64> {
    let (root_kind, folder_id, file_id) = match entity_type {
        EntityType::File => (LockRootKind::File, None, Some(entity_id)),
        EntityType::Folder => (LockRootKind::Folder, Some(entity_id), None),
    };
    let result = ResourceLock::update_many()
        .col_expr(resource_lock::Column::RootKind, Expr::value(root_kind))
        .col_expr(resource_lock::Column::RootFolderId, Expr::value(folder_id))
        .col_expr(resource_lock::Column::RootFileId, Expr::value(file_id))
        .filter(resource_lock::Column::NamespaceId.eq(namespace_id))
        .filter(resource_lock::Column::LockrootPath.eq(path))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

/// 祖先路径查询（WebDAV check 用）
pub async fn find_ancestors<C: ConnectionTrait>(
    db: &C,
    paths: &[String],
) -> Result<Vec<resource_lock::Model>> {
    if paths.is_empty() {
        return Ok(vec![]);
    }
    ResourceLock::find()
        .filter(resource_lock::Column::LockrootPath.is_in(paths.iter().map(|s| s.as_str())))
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_ancestors_in_namespace<C: ConnectionTrait>(
    db: &C,
    namespace_id: i64,
    paths: &[String],
) -> Result<Vec<resource_lock::Model>> {
    if paths.is_empty() {
        return Ok(vec![]);
    }
    ResourceLock::find()
        .filter(resource_lock::Column::NamespaceId.eq(namespace_id))
        .filter(resource_lock::Column::LockrootPath.is_in(paths.iter().map(|s| s.as_str())))
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn delete_by_id<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    ResourceLock::delete_by_id(id)
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

pub async fn delete_by_token<C: ConnectionTrait>(db: &C, token: &str) -> Result<()> {
    ResourceLock::delete_many()
        .filter(resource_lock::Column::Token.eq(token))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

pub async fn delete_by_entity<C: ConnectionTrait>(
    db: &C,
    entity_type: EntityType,
    entity_id: i64,
) -> Result<()> {
    ResourceLock::delete_many()
        .filter(entity_condition(entity_type, entity_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

pub async fn delete_by_entity_and_owner<C: ConnectionTrait>(
    db: &C,
    entity_type: EntityType,
    entity_id: i64,
    owner_id: i64,
) -> Result<()> {
    ResourceLock::delete_many()
        .filter(entity_condition(entity_type, entity_id))
        .filter(resource_lock::Column::HolderUserId.eq(owner_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

pub async fn delete_expired_by_entity_before<C: ConnectionTrait>(
    db: &C,
    entity_type: EntityType,
    entity_id: i64,
    cutoff: chrono::DateTime<Utc>,
) -> Result<u64> {
    let res = ResourceLock::delete_many()
        .filter(entity_condition(entity_type, entity_id))
        .filter(resource_lock::Column::TimeoutAt.is_not_null())
        .filter(resource_lock::Column::TimeoutAt.lte(cutoff))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(res.rows_affected)
}

/// 删除路径前缀匹配的所有锁
pub async fn delete_by_path_prefix<C: ConnectionTrait>(db: &C, prefix: &str) -> Result<u64> {
    let res = ResourceLock::delete_many()
        .filter(resource_lock::Column::LockrootPath.starts_with(prefix))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(res.rows_affected)
}

pub async fn find_expired_before<C: ConnectionTrait>(
    db: &C,
    cutoff: chrono::DateTime<Utc>,
) -> Result<Vec<resource_lock::Model>> {
    ResourceLock::find()
        .filter(resource_lock::Column::TimeoutAt.is_not_null())
        .filter(resource_lock::Column::TimeoutAt.lte(cutoff))
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn delete_expired_before<C: ConnectionTrait>(
    db: &C,
    cutoff: chrono::DateTime<Utc>,
) -> Result<u64> {
    let res = ResourceLock::delete_many()
        .filter(resource_lock::Column::TimeoutAt.is_not_null())
        .filter(resource_lock::Column::TimeoutAt.lte(cutoff))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(res.rows_affected)
}

pub async fn delete_expired_by_namespace_before<C: ConnectionTrait>(
    db: &C,
    namespace_id: i64,
    cutoff: chrono::DateTime<Utc>,
) -> Result<u64> {
    let result = ResourceLock::delete_many()
        .filter(resource_lock::Column::NamespaceId.eq(namespace_id))
        .filter(resource_lock::Column::TimeoutAt.is_not_null())
        .filter(resource_lock::Column::TimeoutAt.lte(cutoff))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

pub async fn find_all_by_namespace_for_update<C: ConnectionTrait>(
    db: &C,
    namespace_id: i64,
) -> Result<Vec<resource_lock::Model>> {
    ResourceLock::find()
        .filter(resource_lock::Column::NamespaceId.eq(namespace_id))
        .order_by_asc(resource_lock::Column::Id)
        .lock_exclusive()
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_all_by_namespace<C: ConnectionTrait>(
    db: &C,
    namespace_id: i64,
) -> Result<Vec<resource_lock::Model>> {
    ResourceLock::find()
        .filter(resource_lock::Column::NamespaceId.eq(namespace_id))
        .order_by_asc(resource_lock::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn refresh<C: ConnectionTrait>(
    db: &C,
    token: &str,
    new_timeout_at: Option<chrono::DateTime<Utc>>,
) -> Result<Option<resource_lock::Model>> {
    let lock = find_by_token(db, token).await?;
    match lock {
        Some(l) => {
            let mut active: resource_lock::ActiveModel = l.into();
            active.timeout_at = Set(new_timeout_at);
            let updated = active.update(db).await.map_err(AsterError::from)?;
            Ok(Some(updated))
        }
        None => Ok(None),
    }
}

/// 查询用户持有的所有资源锁
pub async fn find_by_owner<C: ConnectionTrait>(
    db: &C,
    owner_id: i64,
) -> Result<Vec<resource_lock::Model>> {
    ResourceLock::find()
        .filter(resource_lock::Column::HolderUserId.eq(owner_id))
        .all(db)
        .await
        .map_err(AsterError::from)
}

fn entity_query(entity_type: EntityType, entity_id: i64) -> Select<ResourceLock> {
    ResourceLock::find()
        .filter(entity_condition(entity_type, entity_id))
        .order_by_asc(resource_lock::Column::Id)
}

fn entity_condition(entity_type: EntityType, entity_id: i64) -> Condition {
    match entity_type {
        EntityType::File => Condition::all()
            .add(resource_lock::Column::RootKind.eq(LockRootKind::File))
            .add(resource_lock::Column::RootFileId.eq(entity_id)),
        EntityType::Folder => Condition::all()
            .add(resource_lock::Column::RootKind.eq(LockRootKind::Folder))
            .add(resource_lock::Column::RootFolderId.eq(entity_id)),
    }
}

/// Count active locks owned by a user.
///
/// `timeout_at = NULL` is treated as active for compatibility with legacy or
/// non-WebDAV lock rows.
pub async fn count_active_by_owner<C: ConnectionTrait>(
    db: &C,
    owner_id: i64,
    now: chrono::DateTime<Utc>,
) -> Result<u64> {
    ResourceLock::find()
        .filter(resource_lock::Column::HolderUserId.eq(owner_id))
        .filter(
            Condition::any()
                .add(resource_lock::Column::TimeoutAt.is_null())
                .add(resource_lock::Column::TimeoutAt.gt(now)),
        )
        .count(db)
        .await
        .map_err(AsterError::from)
}

/// 批量删除用户持有的所有资源锁
pub async fn delete_all_by_owner<C: ConnectionTrait>(db: &C, owner_id: i64) -> Result<u64> {
    let res = ResourceLock::delete_many()
        .filter(resource_lock::Column::HolderUserId.eq(owner_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(res.rows_affected)
}

pub async fn delete_by_owner_in_namespace<C: ConnectionTrait>(
    db: &C,
    namespace_id: i64,
    owner_id: i64,
) -> Result<u64> {
    let res = ResourceLock::delete_many()
        .filter(resource_lock::Column::NamespaceId.eq(namespace_id))
        .filter(resource_lock::Column::HolderUserId.eq(owner_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(res.rows_affected)
}

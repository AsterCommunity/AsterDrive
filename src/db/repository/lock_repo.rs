//! 仓储模块：`lock_repo`。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, ExprTrait, QueryFilter,
    QueryOrder, Select, Set, TryInsertResult,
    sea_query::{Expr, Query, SelectStatement},
};

use crate::api::pagination::{AdminLockSortBy, SortOrder};
use crate::db::repository::pagination_repo::fetch_offset_page;
use crate::db::repository::sort::{order_by_column_with_id, order_by_id};
use crate::entities::{
    file, folder,
    resource_lock::{self, Entity as ResourceLock},
};
use crate::errors::{AsterError, Result};
use crate::types::EntityType;

pub async fn create<C: ConnectionTrait>(
    db: &C,
    model: resource_lock::ActiveModel,
) -> Result<resource_lock::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn create_if_unlocked<C: ConnectionTrait>(
    db: &C,
    model: resource_lock::ActiveModel,
    entity_type: EntityType,
    entity_id: i64,
) -> Result<Option<resource_lock::Model>> {
    let inserted = match ResourceLock::insert(model)
        .on_conflict_do_nothing_on([
            resource_lock::Column::EntityType,
            resource_lock::Column::EntityId,
        ])
        .exec(db)
        .await
        .map_err(AsterError::from)?
    {
        TryInsertResult::Inserted(_) => true,
        TryInsertResult::Conflicted => false,
        TryInsertResult::Empty => {
            return Err(AsterError::internal_error(
                "resource lock insert produced empty result",
            ));
        }
    };

    if !inserted {
        return Ok(None);
    }

    find_by_entity(db, entity_type, entity_id)
        .await?
        .map(Some)
        .ok_or_else(|| {
            AsterError::internal_error(format!(
                "resource lock insert could not reload entity lock for {entity_type:?}#{entity_id}"
            ))
        })
}

pub async fn find_all<C: ConnectionTrait>(db: &C) -> Result<Vec<resource_lock::Model>> {
    ResourceLock::find()
        .order_by_asc(resource_lock::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_paginated<C: ConnectionTrait>(
    db: &C,
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
        AdminLockSortBy::Path => order_by_column_with_id(
            query,
            resource_lock::Column::Path,
            sort_order,
            resource_lock::Column::Id,
        ),
        AdminLockSortBy::EntityType => order_by_column_with_id(
            query,
            resource_lock::Column::EntityType,
            sort_order,
            resource_lock::Column::Id,
        ),
        AdminLockSortBy::OwnerId => order_by_column_with_id(
            query,
            resource_lock::Column::OwnerId,
            sort_order,
            resource_lock::Column::Id,
        ),
        AdminLockSortBy::TimeoutAt => order_by_column_with_id(
            query,
            resource_lock::Column::TimeoutAt,
            sort_order,
            resource_lock::Column::Id,
        ),
        AdminLockSortBy::Shared => order_by_column_with_id(
            query,
            resource_lock::Column::Shared,
            sort_order,
            resource_lock::Column::Id,
        ),
        AdminLockSortBy::Deep => order_by_column_with_id(
            query,
            resource_lock::Column::Deep,
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

/// 查询单个资源的锁
pub async fn find_by_entity<C: ConnectionTrait>(
    db: &C,
    entity_type: EntityType,
    entity_id: i64,
) -> Result<Option<resource_lock::Model>> {
    ResourceLock::find()
        .filter(resource_lock::Column::EntityType.eq(entity_type))
        .filter(resource_lock::Column::EntityId.eq(entity_id))
        .one(db)
        .await
        .map_err(AsterError::from)
}

/// 路径前缀查询（WebDAV deep lock 用）
pub async fn find_by_path_prefix<C: ConnectionTrait>(
    db: &C,
    prefix: &str,
) -> Result<Vec<resource_lock::Model>> {
    ResourceLock::find()
        .filter(resource_lock::Column::Path.starts_with(prefix))
        .all(db)
        .await
        .map_err(AsterError::from)
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
        .filter(resource_lock::Column::Path.is_in(paths.iter().map(|s| s.as_str())))
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
        .filter(resource_lock::Column::EntityType.eq(entity_type))
        .filter(resource_lock::Column::EntityId.eq(entity_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

/// 删除路径前缀匹配的所有锁
pub async fn delete_by_path_prefix<C: ConnectionTrait>(db: &C, prefix: &str) -> Result<u64> {
    let res = ResourceLock::delete_many()
        .filter(resource_lock::Column::Path.starts_with(prefix))
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
        .filter(resource_lock::Column::TimeoutAt.lt(cutoff))
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
        .filter(resource_lock::Column::TimeoutAt.lt(cutoff))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(res.rows_affected)
}

pub async fn clear_file_locked_flags_without_locks<C: ConnectionTrait>(
    db: &C,
    file_ids: &[i64],
) -> Result<u64> {
    if file_ids.is_empty() {
        return Ok(0);
    }

    let result = file::Entity::update_many()
        .col_expr(file::Column::IsLocked, Expr::value(false))
        .col_expr(file::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(file::Column::Id.is_in(file_ids.iter().copied()))
        .filter(file::Column::IsLocked.eq(true))
        .filter(Expr::not_exists(lock_exists_for_file_query()))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

pub async fn clear_file_locked_flag_without_lock<C: ConnectionTrait>(
    db: &C,
    file_id: i64,
) -> Result<bool> {
    Ok(clear_file_locked_flags_without_locks(db, &[file_id]).await? == 1)
}

pub async fn clear_folder_locked_flags_without_locks<C: ConnectionTrait>(
    db: &C,
    folder_ids: &[i64],
) -> Result<u64> {
    if folder_ids.is_empty() {
        return Ok(0);
    }

    let result = folder::Entity::update_many()
        .col_expr(folder::Column::IsLocked, Expr::value(false))
        .col_expr(folder::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(folder::Column::Id.is_in(folder_ids.iter().copied()))
        .filter(folder::Column::IsLocked.eq(true))
        .filter(Expr::not_exists(lock_exists_for_folder_query()))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

pub async fn clear_folder_locked_flag_without_lock<C: ConnectionTrait>(
    db: &C,
    folder_id: i64,
) -> Result<bool> {
    Ok(clear_folder_locked_flags_without_locks(db, &[folder_id]).await? == 1)
}

fn lock_exists_for_file_query() -> SelectStatement {
    Query::select()
        .expr(Expr::value(1i32))
        .from(resource_lock::Entity)
        .and_where(
            Expr::col((resource_lock::Entity, resource_lock::Column::EntityType))
                .eq(EntityType::File),
        )
        .and_where(
            Expr::col((resource_lock::Entity, resource_lock::Column::EntityId))
                .eq(Expr::col((file::Entity, file::Column::Id))),
        )
        .to_owned()
}

fn lock_exists_for_folder_query() -> SelectStatement {
    Query::select()
        .expr(Expr::value(1i32))
        .from(resource_lock::Entity)
        .and_where(
            Expr::col((resource_lock::Entity, resource_lock::Column::EntityType))
                .eq(EntityType::Folder),
        )
        .and_where(
            Expr::col((resource_lock::Entity, resource_lock::Column::EntityId))
                .eq(Expr::col((folder::Entity, folder::Column::Id))),
        )
        .to_owned()
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
        .filter(resource_lock::Column::OwnerId.eq(owner_id))
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// 批量删除用户持有的所有资源锁
pub async fn delete_all_by_owner<C: ConnectionTrait>(db: &C, owner_id: i64) -> Result<u64> {
    let res = ResourceLock::delete_many()
        .filter(resource_lock::Column::OwnerId.eq(owner_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(res.rows_affected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use sea_orm::{DbBackend, QueryTrait};

    #[test]
    fn postgres_create_if_unlocked_sql_uses_entity_conflict_target() {
        let sql = ResourceLock::insert(resource_lock::ActiveModel {
            token: Set("urn:uuid:test".to_string()),
            entity_type: Set(EntityType::File),
            entity_id: Set(7),
            path: Set("/docs/report.txt".to_string()),
            owner_id: Set(Some(9)),
            owner_info: Set(None),
            timeout_at: Set(None),
            shared: Set(false),
            deep: Set(false),
            created_at: Set(Utc::now()),
            ..Default::default()
        })
        .on_conflict_do_nothing_on([
            resource_lock::Column::EntityType,
            resource_lock::Column::EntityId,
        ])
        .build(DbBackend::Postgres)
        .to_string();

        assert!(
            sql.contains(r#"ON CONFLICT ("entity_type", "entity_id") DO NOTHING"#),
            "{sql}"
        );
        assert!(!sql.contains(" WHERE "), "{sql}");
    }

    #[test]
    fn postgres_clear_file_locked_flags_sql_requires_absent_replacement_lock() {
        let sql = file::Entity::update_many()
            .col_expr(file::Column::IsLocked, Expr::value(false))
            .filter(file::Column::Id.is_in([7, 9]))
            .filter(Expr::not_exists(lock_exists_for_file_query()))
            .build(DbBackend::Postgres)
            .to_string();

        assert!(sql.contains("NOT EXISTS"), "{sql}");
        assert!(sql.contains(r#""resource_locks""#), "{sql}");
        assert!(sql.contains(r#""entity_type" = 'file'"#), "{sql}");
        assert!(
            sql.contains(r#""resource_locks"."entity_id" = "files"."id""#),
            "{sql}"
        );
    }

    #[test]
    fn postgres_clear_folder_locked_flags_sql_requires_absent_replacement_lock() {
        let sql = folder::Entity::update_many()
            .col_expr(folder::Column::IsLocked, Expr::value(false))
            .filter(folder::Column::Id.is_in([11, 13]))
            .filter(Expr::not_exists(lock_exists_for_folder_query()))
            .build(DbBackend::Postgres)
            .to_string();

        assert!(sql.contains("NOT EXISTS"), "{sql}");
        assert!(sql.contains(r#""resource_locks""#), "{sql}");
        assert!(sql.contains(r#""entity_type" = 'folder'"#), "{sql}");
        assert!(
            sql.contains(r#""resource_locks"."entity_id" = "folders"."id""#),
            "{sql}"
        );
    }
}

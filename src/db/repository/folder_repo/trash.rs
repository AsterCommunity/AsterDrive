//! `folder_repo` 仓储子模块：`trash`。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, EntityTrait, ExprTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, sea_query::Expr,
};

use crate::entities::folder::{self, Entity as Folder};
use crate::errors::{AsterError, Result};

use super::common::{FolderScope, map_bulk_name_db_err, map_name_db_err, scope_condition};
use super::query::find_by_id;

/// 查询顶层已删除文件夹（分页），用 SQL 过滤而非内存过滤
fn top_level_deleted_condition(scope: FolderScope) -> Condition {
    use sea_orm::sea_query::{Alias, Expr, Query};

    let parent_deleted_subquery = Query::select()
        .expr(Expr::val(1i32))
        .from_as(Alias::new("folders"), Alias::new("p"))
        .and_where(
            Expr::col((Alias::new("p"), Alias::new("id")))
                .equals((Alias::new("folders"), folder::Column::ParentId)),
        )
        .and_where(Expr::col((Alias::new("p"), Alias::new("deleted_at"))).is_not_null())
        .to_owned();

    scope_condition(scope)
        .add(folder::Column::DeletedAt.is_not_null())
        .add(
            Condition::any()
                .add(folder::Column::ParentId.is_null())
                .add(Expr::exists(parent_deleted_subquery).not()),
        )
}

async fn find_top_level_deleted_paginated_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: FolderScope,
    limit: u64,
    offset: u64,
) -> Result<(Vec<folder::Model>, u64)> {
    let base = Folder::find().filter(top_level_deleted_condition(scope));

    let total = base.clone().count(db).await.map_err(AsterError::from)?;
    if total == 0 || limit == 0 {
        return Ok((vec![], total));
    }

    let items = base
        .order_by_desc(folder::Column::DeletedAt)
        .order_by_desc(folder::Column::Id)
        .offset(offset)
        .limit(limit)
        .all(db)
        .await
        .map_err(AsterError::from)?;

    Ok((items, total))
}

async fn find_top_level_deleted_cursor_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: FolderScope,
    limit: u64,
    after: Option<(chrono::DateTime<Utc>, i64)>,
) -> Result<(Vec<folder::Model>, u64)> {
    let base_cond = top_level_deleted_condition(scope);
    let base = Folder::find().filter(base_cond.clone());

    let total = base.clone().count(db).await.map_err(AsterError::from)?;
    if total == 0 || limit == 0 {
        return Ok((vec![], total));
    }

    let mut q = Folder::find().filter(base_cond);
    if let Some((after_deleted_at, after_id)) = after {
        q = q.filter(
            Condition::any()
                .add(folder::Column::DeletedAt.lt(after_deleted_at))
                .add(
                    Condition::all()
                        .add(folder::Column::DeletedAt.eq(after_deleted_at))
                        .add(folder::Column::Id.lt(after_id)),
                ),
        );
    }

    let items = q
        .order_by_desc(folder::Column::DeletedAt)
        .order_by_desc(folder::Column::Id)
        .limit(limit)
        .all(db)
        .await
        .map_err(AsterError::from)?;

    Ok((items, total))
}

pub async fn find_top_level_deleted_paginated<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    limit: u64,
    offset: u64,
) -> Result<(Vec<folder::Model>, u64)> {
    find_top_level_deleted_paginated_in_scope(db, FolderScope::Personal { user_id }, limit, offset)
        .await
}

pub async fn find_top_level_deleted_by_team_paginated<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    limit: u64,
    offset: u64,
) -> Result<(Vec<folder::Model>, u64)> {
    find_top_level_deleted_paginated_in_scope(db, FolderScope::Team { team_id }, limit, offset)
        .await
}

pub async fn find_top_level_deleted_cursor<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    limit: u64,
    after: Option<(chrono::DateTime<Utc>, i64)>,
) -> Result<(Vec<folder::Model>, u64)> {
    find_top_level_deleted_cursor_in_scope(db, FolderScope::Personal { user_id }, limit, after)
        .await
}

pub async fn find_top_level_deleted_by_team_cursor<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    limit: u64,
    after: Option<(chrono::DateTime<Utc>, i64)>,
) -> Result<(Vec<folder::Model>, u64)> {
    find_top_level_deleted_cursor_in_scope(db, FolderScope::Team { team_id }, limit, after).await
}

/// 软删除：标记 deleted_at
pub async fn soft_delete<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    let f = find_by_id(db, id).await?;
    let mut active: folder::ActiveModel = f.into();
    active.deleted_at = Set(Some(Utc::now()));
    active.update(db).await.map_err(AsterError::from)?;
    Ok(())
}

/// 批量软删除：一次 UPDATE 标记多个文件夹的 deleted_at
pub async fn soft_delete_many<C: ConnectionTrait>(
    db: &C,
    ids: &[i64],
    now: chrono::DateTime<Utc>,
) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    Folder::update_many()
        .col_expr(folder::Column::DeletedAt, Expr::value(Some(now)))
        .filter(folder::Column::Id.is_in(ids.iter().copied()))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

/// 恢复：清除 deleted_at
pub async fn restore<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    let f = find_by_id(db, id).await?;
    let name = f.name.clone();
    let mut active: folder::ActiveModel = f.into();
    active.deleted_at = Set(None);
    active
        .update(db)
        .await
        .map_err(|err| map_name_db_err(err, &name))?;
    Ok(())
}

/// 批量恢复：一次 UPDATE 清除多个文件夹的 deleted_at
pub async fn restore_many<C: ConnectionTrait>(db: &C, ids: &[i64]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    Folder::update_many()
        .col_expr(
            folder::Column::DeletedAt,
            Expr::value(Option::<chrono::DateTime<Utc>>::None),
        )
        .filter(folder::Column::Id.is_in(ids.iter().copied()))
        .exec(db)
        .await
        .map_err(|err| {
            map_bulk_name_db_err(
                err,
                "one or more folders already exist in their original locations",
            )
        })?;
    Ok(())
}

/// 查询用户回收站中的文件夹（只查顶层被删除的，不含子目录）
pub async fn find_deleted_by_user<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
) -> Result<Vec<folder::Model>> {
    Folder::find()
        .filter(folder::Column::OwnerUserId.eq(user_id))
        .filter(folder::Column::TeamId.is_null())
        .filter(folder::Column::DeletedAt.is_not_null())
        .order_by_desc(folder::Column::DeletedAt)
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// 查询某文件夹下的已删除子文件夹（递归恢复/清理用，避免 N+1）
pub async fn find_deleted_children<C: ConnectionTrait>(
    db: &C,
    parent_id: i64,
) -> Result<Vec<folder::Model>> {
    Folder::find()
        .filter(folder::Column::ParentId.eq(parent_id))
        .filter(folder::Column::DeletedAt.is_not_null())
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// 查询过期的已删除文件夹（自动清理用）
pub async fn find_expired_deleted<C: ConnectionTrait>(
    db: &C,
    before: chrono::DateTime<Utc>,
) -> Result<Vec<folder::Model>> {
    Folder::find()
        .filter(folder::Column::DeletedAt.is_not_null())
        .filter(folder::Column::DeletedAt.lt(before))
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// 查询用户的所有文件夹（含已删除，force_delete 用）
pub async fn find_all_by_user<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
) -> Result<Vec<folder::Model>> {
    Folder::find()
        .filter(folder::Column::OwnerUserId.eq(user_id))
        .filter(folder::Column::TeamId.is_null())
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_all_by_user_paginated<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    after_id: Option<i64>,
    limit: u64,
) -> Result<Vec<folder::Model>> {
    let mut query = Folder::find()
        .filter(folder::Column::OwnerUserId.eq(user_id))
        .filter(folder::Column::TeamId.is_null())
        .order_by_asc(folder::Column::Id)
        .limit(limit);
    if let Some(after_id) = after_id {
        query = query.filter(folder::Column::Id.gt(after_id));
    }
    query.all(db).await.map_err(AsterError::from)
}

pub async fn find_all_by_team<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
) -> Result<Vec<folder::Model>> {
    Folder::find()
        .filter(folder::Column::TeamId.eq(team_id))
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_all_by_team_paginated<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    after_id: Option<i64>,
    limit: u64,
) -> Result<Vec<folder::Model>> {
    let mut query = Folder::find()
        .filter(folder::Column::TeamId.eq(team_id))
        .order_by_asc(folder::Column::Id)
        .limit(limit);
    if let Some(after_id) = after_id {
        query = query.filter(folder::Column::Id.gt(after_id));
    }
    query.all(db).await.map_err(AsterError::from)
}

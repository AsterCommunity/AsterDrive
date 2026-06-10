//! 仓储模块：`share_repo`。

use chrono::Utc;
use std::collections::HashSet;

use crate::api::pagination::{AdminShareSortBy, SortOrder};
use crate::db::repository::pagination_repo::fetch_offset_page;
use crate::db::repository::sort::{order_by_column_with_id, order_by_id};
use crate::entities::share::{self, Entity as Share};
use crate::errors::{AsterError, Result};
use crate::utils::numbers::u64_to_i64;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, EntityTrait,
    ExprTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Select, sea_query::Expr,
};

#[derive(Clone, Copy)]
enum ShareScope {
    Personal { user_id: i64 },
    Team { team_id: i64 },
}

fn scope_condition(scope: ShareScope) -> Condition {
    match scope {
        ShareScope::Personal { user_id } => Condition::all()
            .add(share::Column::UserId.eq(user_id))
            .add(share::Column::TeamId.is_null()),
        ShareScope::Team { team_id } => Condition::all().add(share::Column::TeamId.eq(team_id)),
    }
}

pub async fn find_by_id<C: ConnectionTrait>(db: &C, id: i64) -> Result<share::Model> {
    Share::find_by_id(id)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::share_not_found(format!("share #{id}")))
}

/// 统计所有分享总数
pub async fn count_all(db: &DatabaseConnection) -> Result<u64> {
    Share::find().count(db).await.map_err(AsterError::from)
}

pub async fn find_by_ids<C: ConnectionTrait>(db: &C, ids: &[i64]) -> Result<Vec<share::Model>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    Share::find()
        .filter(share::Column::Id.is_in(ids.iter().copied()))
        .all(db)
        .await
        .map_err(AsterError::from)
}

async fn find_by_ids_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: ShareScope,
    ids: &[i64],
) -> Result<Vec<share::Model>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    Share::find()
        .filter(scope_condition(scope))
        .filter(share::Column::Id.is_in(ids.iter().copied()))
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_ids_in_personal_scope<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    ids: &[i64],
) -> Result<Vec<share::Model>> {
    find_by_ids_in_scope(db, ShareScope::Personal { user_id }, ids).await
}

pub async fn find_by_ids_in_team_scope<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    ids: &[i64],
) -> Result<Vec<share::Model>> {
    find_by_ids_in_scope(db, ShareScope::Team { team_id }, ids).await
}

pub async fn find_by_token<C: ConnectionTrait>(
    db: &C,
    token: &str,
) -> Result<Option<share::Model>> {
    Share::find()
        .filter(share::Column::Token.eq(token))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_user<C: ConnectionTrait>(db: &C, user_id: i64) -> Result<Vec<share::Model>> {
    Share::find()
        .filter(scope_condition(ShareScope::Personal { user_id }))
        .order_by_desc(share::Column::CreatedAt)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_team<C: ConnectionTrait>(db: &C, team_id: i64) -> Result<Vec<share::Model>> {
    Share::find()
        .filter(scope_condition(ShareScope::Team { team_id }))
        .order_by_desc(share::Column::CreatedAt)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_user_paginated(
    db: &DatabaseConnection,
    user_id: i64,
    limit: u64,
    offset: u64,
) -> Result<(Vec<share::Model>, u64)> {
    fetch_offset_page(
        db,
        Share::find()
            .filter(scope_condition(ShareScope::Personal { user_id }))
            .order_by_desc(share::Column::CreatedAt),
        limit,
        offset,
    )
    .await
}

pub async fn find_by_team_paginated(
    db: &DatabaseConnection,
    team_id: i64,
    limit: u64,
    offset: u64,
) -> Result<(Vec<share::Model>, u64)> {
    fetch_offset_page(
        db,
        Share::find()
            .filter(scope_condition(ShareScope::Team { team_id }))
            .order_by_desc(share::Column::CreatedAt),
        limit,
        offset,
    )
    .await
}

pub async fn find_paginated(
    db: &DatabaseConnection,
    limit: u64,
    offset: u64,
    sort_by: AdminShareSortBy,
    sort_order: SortOrder,
) -> Result<(Vec<share::Model>, u64)> {
    fetch_offset_page(
        db,
        apply_admin_share_sort(Share::find(), sort_by, sort_order),
        limit,
        offset,
    )
    .await
}

fn apply_admin_share_sort(
    query: Select<Share>,
    sort_by: AdminShareSortBy,
    sort_order: SortOrder,
) -> Select<Share> {
    match sort_by {
        AdminShareSortBy::Id => order_by_id(query, share::Column::Id, sort_order),
        AdminShareSortBy::Token => {
            order_by_column_with_id(query, share::Column::Token, sort_order, share::Column::Id)
        }
        AdminShareSortBy::UserId => {
            order_by_column_with_id(query, share::Column::UserId, sort_order, share::Column::Id)
        }
        AdminShareSortBy::DownloadCount => order_by_column_with_id(
            query,
            share::Column::DownloadCount,
            sort_order,
            share::Column::Id,
        ),
        AdminShareSortBy::MaxDownloads => order_by_column_with_id(
            query,
            share::Column::MaxDownloads,
            sort_order,
            share::Column::Id,
        ),
        AdminShareSortBy::ExpiresAt => order_by_column_with_id(
            query,
            share::Column::ExpiresAt,
            sort_order,
            share::Column::Id,
        ),
        AdminShareSortBy::CreatedAt => order_by_column_with_id(
            query,
            share::Column::CreatedAt,
            sort_order,
            share::Column::Id,
        ),
        AdminShareSortBy::UpdatedAt => order_by_column_with_id(
            query,
            share::Column::UpdatedAt,
            sort_order,
            share::Column::Id,
        ),
    }
}

/// 查找用户对同一资源是否已有活跃分享
async fn find_active_by_resource_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: ShareScope,
    file_id: Option<i64>,
    folder_id: Option<i64>,
) -> Result<Option<share::Model>> {
    let mut q = Share::find()
        .filter(scope_condition(scope))
        .filter(active_share_condition());
    if let Some(file_id) = file_id {
        q = q.filter(share::Column::FileId.eq(file_id));
    }
    if let Some(folder_id) = folder_id {
        q = q.filter(share::Column::FolderId.eq(folder_id));
    }
    q.one(db).await.map_err(AsterError::from)
}

pub async fn find_active_by_resource<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    file_id: Option<i64>,
    folder_id: Option<i64>,
) -> Result<Option<share::Model>> {
    find_active_by_resource_in_scope(db, ShareScope::Personal { user_id }, file_id, folder_id).await
}

pub async fn find_active_by_team_resource<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    file_id: Option<i64>,
    folder_id: Option<i64>,
) -> Result<Option<share::Model>> {
    find_active_by_resource_in_scope(db, ShareScope::Team { team_id }, file_id, folder_id).await
}

fn active_share_condition() -> Condition {
    Condition::all()
        .add(
            Condition::any()
                .add(share::Column::ExpiresAt.is_null())
                .add(share::Column::ExpiresAt.gte(Utc::now())),
        )
        .add(available_downloads_condition())
}

fn available_downloads_condition() -> Condition {
    Condition::any()
        .add(share::Column::MaxDownloads.eq(0))
        .add(Expr::col(share::Column::DownloadCount).lt(Expr::col(share::Column::MaxDownloads)))
}

async fn find_active_ids_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: ShareScope,
    column: share::Column,
    ids: &[i64],
) -> Result<HashSet<i64>> {
    if ids.is_empty() {
        return Ok(HashSet::new());
    }

    let rows = Share::find()
        .select_only()
        .column(column)
        .filter(scope_condition(scope))
        .filter(column.is_in(ids.iter().copied()))
        .filter(column.is_not_null())
        .filter(active_share_condition())
        .into_tuple::<Option<i64>>()
        .all(db)
        .await
        .map_err(AsterError::from)?;

    Ok(rows.into_iter().flatten().collect())
}

pub async fn find_active_file_ids<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    file_ids: &[i64],
) -> Result<HashSet<i64>> {
    find_active_ids_in_scope(
        db,
        ShareScope::Personal { user_id },
        share::Column::FileId,
        file_ids,
    )
    .await
}

pub async fn find_active_team_file_ids<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    file_ids: &[i64],
) -> Result<HashSet<i64>> {
    find_active_ids_in_scope(
        db,
        ShareScope::Team { team_id },
        share::Column::FileId,
        file_ids,
    )
    .await
}

pub async fn find_active_folder_ids<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    folder_ids: &[i64],
) -> Result<HashSet<i64>> {
    find_active_ids_in_scope(
        db,
        ShareScope::Personal { user_id },
        share::Column::FolderId,
        folder_ids,
    )
    .await
}

pub async fn find_active_team_folder_ids<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    folder_ids: &[i64],
) -> Result<HashSet<i64>> {
    find_active_ids_in_scope(
        db,
        ShareScope::Team { team_id },
        share::Column::FolderId,
        folder_ids,
    )
    .await
}

pub async fn create<C: ConnectionTrait>(db: &C, model: share::ActiveModel) -> Result<share::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn update<C: ConnectionTrait>(db: &C, model: share::ActiveModel) -> Result<share::Model> {
    model.update(db).await.map_err(AsterError::from)
}

pub async fn delete<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    Share::delete_by_id(id)
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

pub async fn delete_many<C: ConnectionTrait>(db: &C, ids: &[i64]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    Share::delete_many()
        .filter(share::Column::Id.is_in(ids.iter().copied()))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

pub async fn delete_by_file_ids<C: ConnectionTrait>(db: &C, file_ids: &[i64]) -> Result<u64> {
    if file_ids.is_empty() {
        return Ok(0);
    }
    let res = Share::delete_many()
        .filter(share::Column::FileId.is_in(file_ids.iter().copied()))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(res.rows_affected)
}

pub async fn delete_by_folder_ids<C: ConnectionTrait>(db: &C, folder_ids: &[i64]) -> Result<u64> {
    if folder_ids.is_empty() {
        return Ok(0);
    }
    let res = Share::delete_many()
        .filter(share::Column::FolderId.is_in(folder_ids.iter().copied()))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(res.rows_affected)
}

/// 原子递增 view_count
pub async fn increment_view_count<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    Share::update_many()
        .col_expr(
            share::Column::ViewCount,
            Expr::col(share::Column::ViewCount).add(1i64),
        )
        .filter(share::Column::Id.eq(id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

/// 原子递增 download_count，同时校验下载限制。
/// 返回 false 表示已达上限未递增。
pub async fn increment_download_count<C: ConnectionTrait>(db: &C, id: i64) -> Result<bool> {
    let result = Share::update_many()
        .col_expr(
            share::Column::DownloadCount,
            Expr::col(share::Column::DownloadCount).add(1i64),
        )
        .filter(share::Column::Id.eq(id))
        // 只在未达上限时递增（max_downloads=0 表示不限）
        .filter(available_downloads_condition())
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected > 0)
}

/// 回滚多次 `download_count` 递增。
/// 返回 false 表示分享不存在或计数已经是 0。
pub async fn decrement_download_count_by<C: ConnectionTrait>(
    db: &C,
    id: i64,
    count: u64,
) -> Result<bool> {
    if count == 0 {
        return Ok(false);
    }

    let decrement_by = u64_to_i64(count, "share download rollback count")?;
    let result = Share::update_many()
        .col_expr(
            share::Column::DownloadCount,
            Expr::case(Expr::col(share::Column::DownloadCount).lt(decrement_by), 0)
                .finally(Expr::col(share::Column::DownloadCount).sub(decrement_by))
                .into(),
        )
        .filter(share::Column::Id.eq(id))
        .filter(share::Column::DownloadCount.gt(0))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected > 0)
}

/// 批量删除用户的所有分享链接
pub async fn delete_all_by_user<C: ConnectionTrait>(db: &C, user_id: i64) -> Result<u64> {
    let res = Share::delete_many()
        .filter(share::Column::UserId.eq(user_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(res.rows_affected)
}

pub async fn delete_all_by_team<C: ConnectionTrait>(db: &C, team_id: i64) -> Result<u64> {
    let res = Share::delete_many()
        .filter(share::Column::TeamId.eq(team_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(res.rows_affected)
}

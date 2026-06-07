//! `folder_repo` 仓储子模块：`query`。

use std::collections::HashSet;

use sea_orm::{
    ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect,
};
use unicode_normalization::{UnicodeNormalization, is_nfc, is_nfd};

use crate::api::pagination::{SortBy, SortOrder};
use crate::entities::folder::{self, Entity as Folder};
use crate::errors::{AsterError, Result};

use super::common::{FolderScope, active_scope_condition, apply_parent_condition, scope_condition};

pub async fn lock_by_id<C: ConnectionTrait>(db: &C, id: i64) -> Result<folder::Model> {
    match db.get_database_backend() {
        DbBackend::Postgres | DbBackend::MySql => Folder::find_by_id(id)
            .lock_exclusive()
            .one(db)
            .await
            .map_err(AsterError::from)?
            .ok_or_else(|| AsterError::folder_not_found(format!("folder #{id}"))),
        DbBackend::Sqlite => {
            // AsterDrive forces SQLite to a single pooled connection in the DB connect path,
            // so an open transaction already serializes all writers at connection acquisition.
            // There is no row-level lock to emulate here.
            find_by_id(db, id).await
        }
        _ => find_by_id(db, id).await,
    }
}

pub async fn find_by_id<C: ConnectionTrait>(db: &C, id: i64) -> Result<folder::Model> {
    Folder::find_by_id(id)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::record_not_found(format!("folder #{id}")))
}

pub async fn find_by_ids<C: ConnectionTrait>(db: &C, ids: &[i64]) -> Result<Vec<folder::Model>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    Folder::find()
        .filter(folder::Column::Id.is_in(ids.iter().copied()))
        .all(db)
        .await
        .map_err(AsterError::from)
}

async fn find_by_ids_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: FolderScope,
    ids: &[i64],
) -> Result<Vec<folder::Model>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    Folder::find()
        .filter(scope_condition(scope))
        .filter(folder::Column::Id.is_in(ids.iter().copied()))
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_ids_in_personal_scope<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    ids: &[i64],
) -> Result<Vec<folder::Model>> {
    find_by_ids_in_scope(db, FolderScope::Personal { user_id }, ids).await
}

pub async fn find_by_ids_in_team_scope<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    ids: &[i64],
) -> Result<Vec<folder::Model>> {
    find_by_ids_in_scope(db, FolderScope::Team { team_id }, ids).await
}

async fn find_children_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: FolderScope,
    parent_id: Option<i64>,
) -> Result<Vec<folder::Model>> {
    Folder::find()
        .filter(apply_parent_condition(
            active_scope_condition(scope),
            parent_id,
        ))
        .order_by_asc(folder::Column::Name)
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// 查询子文件夹（排除已删除）
pub async fn find_children<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    parent_id: Option<i64>,
) -> Result<Vec<folder::Model>> {
    find_children_in_scope(db, FolderScope::Personal { user_id }, parent_id).await
}

pub async fn find_team_children<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    parent_id: Option<i64>,
) -> Result<Vec<folder::Model>> {
    find_children_in_scope(db, FolderScope::Team { team_id }, parent_id).await
}

/// 批量查询多个父文件夹下的未删除子文件夹
async fn find_children_in_parents_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: FolderScope,
    parent_ids: &[i64],
) -> Result<Vec<folder::Model>> {
    if parent_ids.is_empty() {
        return Ok(vec![]);
    }
    Folder::find()
        .filter(active_scope_condition(scope))
        .filter(folder::Column::ParentId.is_in(parent_ids.iter().copied()))
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_children_in_parents<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    parent_ids: &[i64],
) -> Result<Vec<folder::Model>> {
    find_children_in_parents_in_scope(db, FolderScope::Personal { user_id }, parent_ids).await
}

pub async fn find_team_children_in_parents<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    parent_ids: &[i64],
) -> Result<Vec<folder::Model>> {
    find_children_in_parents_in_scope(db, FolderScope::Team { team_id }, parent_ids).await
}

/// 详情占用统计专用的轻量子目录查询。
///
/// 只返回子目录 id；调用方按当前 BFS 层分批调用，避免为了统计占用空间加载完整
/// folder 记录或累计整棵树的目录模型。
pub(crate) async fn find_child_ids_in_parents<C: ConnectionTrait>(
    db: &C,
    scope: FolderScope,
    parent_ids: &[i64],
) -> Result<Vec<i64>> {
    if parent_ids.is_empty() {
        return Ok(vec![]);
    }
    Folder::find()
        .select_only()
        .column(folder::Column::Id)
        .filter(active_scope_condition(scope))
        .filter(folder::Column::ParentId.is_in(parent_ids.iter().copied()))
        .into_tuple::<i64>()
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// 查询子文件夹（排除已删除，分页）
async fn find_children_paginated_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: FolderScope,
    parent_id: Option<i64>,
    limit: u64,
    offset: u64,
    sort_by: SortBy,
    sort_order: SortOrder,
) -> Result<(Vec<folder::Model>, u64)> {
    let base = Folder::find().filter(apply_parent_condition(
        active_scope_condition(scope),
        parent_id,
    ));

    let total = base.clone().count(db).await.map_err(AsterError::from)?;
    if total == 0 || limit == 0 {
        return Ok((vec![], total));
    }

    let is_asc = sort_order == SortOrder::Asc;
    let items = match sort_by {
        SortBy::CreatedAt => {
            if is_asc {
                base.order_by_asc(folder::Column::CreatedAt)
                    .order_by_asc(folder::Column::Id)
            } else {
                base.order_by_desc(folder::Column::CreatedAt)
                    .order_by_desc(folder::Column::Id)
            }
        }
        SortBy::UpdatedAt => {
            if is_asc {
                base.order_by_asc(folder::Column::UpdatedAt)
                    .order_by_asc(folder::Column::Id)
            } else {
                base.order_by_desc(folder::Column::UpdatedAt)
                    .order_by_desc(folder::Column::Id)
            }
        }
        _ => {
            if is_asc {
                base.order_by_asc(folder::Column::Name)
                    .order_by_asc(folder::Column::Id)
            } else {
                base.order_by_desc(folder::Column::Name)
                    .order_by_desc(folder::Column::Id)
            }
        }
    }
    .offset(offset)
    .limit(limit)
    .all(db)
    .await
    .map_err(AsterError::from)?;

    Ok((items, total))
}

pub async fn find_children_paginated<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    parent_id: Option<i64>,
    limit: u64,
    offset: u64,
    sort_by: SortBy,
    sort_order: SortOrder,
) -> Result<(Vec<folder::Model>, u64)> {
    find_children_paginated_in_scope(
        db,
        FolderScope::Personal { user_id },
        parent_id,
        limit,
        offset,
        sort_by,
        sort_order,
    )
    .await
}

pub async fn find_team_children_paginated<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    parent_id: Option<i64>,
    limit: u64,
    offset: u64,
    sort_by: SortBy,
    sort_order: SortOrder,
) -> Result<(Vec<folder::Model>, u64)> {
    find_children_paginated_in_scope(
        db,
        FolderScope::Team { team_id },
        parent_id,
        limit,
        offset,
        sort_by,
        sort_order,
    )
    .await
}

/// 按名称查文件夹（排除已删除）
async fn find_by_name_in_parent_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: FolderScope,
    parent_id: Option<i64>,
    name: &str,
) -> Result<Option<folder::Model>> {
    let exact = Folder::find()
        .filter(apply_parent_condition(
            active_scope_condition(scope),
            parent_id,
        ))
        .filter(folder::Column::Name.eq(name))
        .one(db)
        .await
        .map_err(AsterError::from)?;
    if exact.is_some() {
        return Ok(exact);
    }

    let normalized_name = crate::utils::normalize_name(name);
    Ok(find_children_in_scope(db, scope, parent_id)
        .await?
        .into_iter()
        .find(|folder| crate::utils::normalize_name(&folder.name) == normalized_name))
}

async fn find_by_names_in_parent_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: FolderScope,
    parent_id: Option<i64>,
    names: &[String],
) -> Result<Vec<folder::Model>> {
    if names.is_empty() {
        return Ok(vec![]);
    }
    let query_names = add_normalization_query_variants(names);
    let normalized_names = normalized_non_ascii_names(names);

    let mut folders = Folder::find()
        .filter(apply_parent_condition(
            active_scope_condition(scope),
            parent_id,
        ))
        .filter(folder::Column::Name.is_in(query_names.iter().cloned()))
        .all(db)
        .await
        .map_err(AsterError::from)?;

    if !normalized_names.is_empty() {
        let existing_ids: HashSet<i64> = folders.iter().map(|folder| folder.id).collect();
        folders.extend(
            find_children_in_scope(db, scope, parent_id)
                .await?
                .into_iter()
                .filter(|folder| !existing_ids.contains(&folder.id))
                .filter(|folder| {
                    normalized_names.contains(&crate::utils::normalize_name(&folder.name))
                }),
        );
    }

    Ok(folders)
}

fn push_unique_normalization_variant(variants: &mut Vec<String>, variant: &str) {
    if variants.iter().all(|existing| existing.as_str() != variant) {
        variants.push(variant.to_string());
    }
}

fn push_unique_owned_normalization_variant(variants: &mut Vec<String>, variant: String) {
    if variants
        .iter()
        .all(|existing| existing.as_str() != variant.as_str())
    {
        variants.push(variant);
    }
}

fn add_normalization_query_variants(names: &[String]) -> std::borrow::Cow<'_, [String]> {
    if names.iter().all(|name| name.is_ascii()) {
        return std::borrow::Cow::Borrowed(names);
    }

    let mut variants = Vec::with_capacity(names.len());
    for name in names {
        push_unique_normalization_variant(&mut variants, name);
        if name.is_ascii() {
            continue;
        }
        if !is_nfc(name) {
            push_unique_owned_normalization_variant(&mut variants, name.nfc().collect());
        }
        if !is_nfd(name) {
            push_unique_owned_normalization_variant(&mut variants, name.nfd().collect());
        }
    }
    std::borrow::Cow::Owned(variants)
}

fn normalized_non_ascii_names(names: &[String]) -> HashSet<String> {
    names
        .iter()
        .filter(|name| !name.is_ascii())
        .map(|name| crate::utils::normalize_name(name))
        .collect()
}

pub async fn find_by_name_in_parent<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    parent_id: Option<i64>,
    name: &str,
) -> Result<Option<folder::Model>> {
    find_by_name_in_parent_in_scope(db, FolderScope::Personal { user_id }, parent_id, name).await
}

pub async fn find_by_names_in_parent<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    parent_id: Option<i64>,
    names: &[String],
) -> Result<Vec<folder::Model>> {
    find_by_names_in_parent_in_scope(db, FolderScope::Personal { user_id }, parent_id, names).await
}

pub async fn find_by_name_in_team_parent<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    parent_id: Option<i64>,
    name: &str,
) -> Result<Option<folder::Model>> {
    find_by_name_in_parent_in_scope(db, FolderScope::Team { team_id }, parent_id, name).await
}

pub async fn find_by_names_in_team_parent<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    parent_id: Option<i64>,
    names: &[String],
) -> Result<Vec<folder::Model>> {
    find_by_names_in_parent_in_scope(db, FolderScope::Team { team_id }, parent_id, names).await
}

/// 查找某文件夹下的所有子文件夹（含已删除，递归收集用）
pub async fn find_all_children<C: ConnectionTrait>(
    db: &C,
    parent_id: i64,
) -> Result<Vec<folder::Model>> {
    Folder::find()
        .filter(folder::Column::ParentId.eq(parent_id))
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// 批量查询多个父文件夹下的子文件夹（含已删除）
pub async fn find_all_children_in_parents<C: ConnectionTrait>(
    db: &C,
    parent_ids: &[i64],
) -> Result<Vec<folder::Model>> {
    if parent_ids.is_empty() {
        return Ok(vec![]);
    }
    Folder::find()
        .filter(folder::Column::ParentId.is_in(parent_ids.iter().copied()))
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// 查找某文件夹下的所有文件（含已删除，递归收集用）
pub async fn find_all_files_in_folder<C: ConnectionTrait>(
    db: &C,
    folder_id: i64,
) -> Result<Vec<crate::entities::file::Model>> {
    use crate::entities::file::{self, Entity as File};

    File::find()
        .filter(file::Column::FolderId.eq(folder_id))
        .all(db)
        .await
        .map_err(AsterError::from)
}

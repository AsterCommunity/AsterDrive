use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect,
};
use std::collections::{BTreeMap, HashMap};

use crate::api::pagination::{AdminFileSortBy, SortOrder};
use crate::db::repository::search_query::lower_like_condition;
use crate::db::repository::sort::{order_by_column_with_id, order_by_id};
use crate::entities::{
    file::{self, Entity as File},
    file_blob,
};
use crate::errors::{AsterError, Result};

#[derive(Debug, Clone, Copy)]
pub struct AdminFileFilters<'a> {
    pub name: Option<&'a str>,
    pub blob_id: Option<i64>,
    pub policy_id: Option<i64>,
    pub owner_user_id: Option<i64>,
    pub team_id: Option<i64>,
    pub deleted: Option<bool>,
    pub sort_by: AdminFileSortBy,
    pub sort_order: SortOrder,
}

#[derive(Debug, Clone)]
pub struct AdminBlobUploaderRef {
    pub blob_id: i64,
    pub user_id: i64,
}

pub async fn find_admin_files_paginated<C: ConnectionTrait>(
    db: &C,
    limit: u64,
    offset: u64,
    filters: AdminFileFilters<'_>,
) -> Result<(Vec<(file::Model, file_blob::Model)>, u64)> {
    let mut query = File::find().find_also_related(file_blob::Entity);

    let mut condition = Condition::all();
    if let Some(name) = filters.name {
        condition = condition.add(lower_like_condition(file::Column::Name, name));
    }
    if let Some(blob_id) = filters.blob_id {
        condition = condition.add(file::Column::BlobId.eq(blob_id));
    }
    if let Some(policy_id) = filters.policy_id {
        condition = condition.add(file_blob::Column::PolicyId.eq(policy_id));
    }
    if let Some(owner_user_id) = filters.owner_user_id {
        condition = condition.add(file::Column::OwnerUserId.eq(owner_user_id));
    }
    if let Some(team_id) = filters.team_id {
        condition = condition.add(file::Column::TeamId.eq(team_id));
    }
    if let Some(deleted) = filters.deleted {
        condition = if deleted {
            condition.add(file::Column::DeletedAt.is_not_null())
        } else {
            condition.add(file::Column::DeletedAt.is_null())
        };
    }
    query = query.filter(condition);
    query = apply_admin_file_order(query, filters.sort_by, filters.sort_order);

    let total = query.clone().count(db).await.map_err(AsterError::from)?;
    let rows = query
        .limit(limit)
        .offset(offset)
        .all(db)
        .await
        .map_err(AsterError::from)?;

    let items = rows
        .into_iter()
        .map(|(file, blob)| match blob {
            Some(blob) => Ok((file, blob)),
            None => Err(AsterError::internal_error(format!(
                "file #{} references missing blob #{}",
                file.id, file.blob_id
            ))),
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((items, total))
}

pub async fn find_admin_file_by_id<C: ConnectionTrait>(
    db: &C,
    id: i64,
) -> Result<(file::Model, file_blob::Model)> {
    let (file, blob) = File::find_by_id(id)
        .find_also_related(file_blob::Entity)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::file_not_found(format!("file #{id}")))?;

    let blob = blob.ok_or_else(|| {
        AsterError::internal_error(format!(
            "file #{} references missing blob #{}",
            file.id, file.blob_id
        ))
    })?;
    Ok((file, blob))
}

pub async fn find_by_blob_id<C: ConnectionTrait>(db: &C, blob_id: i64) -> Result<Vec<file::Model>> {
    File::find()
        .filter(file::Column::BlobId.eq(blob_id))
        .order_by_asc(file::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_admin_blob_uploader_refs_for_blobs<C: ConnectionTrait>(
    db: &C,
    blob_ids: &[i64],
) -> Result<HashMap<i64, Vec<AdminBlobUploaderRef>>> {
    if blob_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = File::find()
        .select_only()
        .column(file::Column::BlobId)
        .column(file::Column::CreatedByUserId)
        .filter(file::Column::BlobId.is_in(blob_ids.iter().copied()))
        .filter(file::Column::CreatedByUserId.is_not_null())
        .group_by(file::Column::BlobId)
        .group_by(file::Column::CreatedByUserId)
        .order_by_asc(file::Column::BlobId)
        .order_by_asc(file::Column::CreatedByUserId)
        .into_tuple::<(i64, i64)>()
        .all(db)
        .await
        .map_err(AsterError::from)?;

    let mut grouped = BTreeMap::<i64, Vec<AdminBlobUploaderRef>>::new();
    for (blob_id, user_id) in rows {
        grouped
            .entry(blob_id)
            .or_default()
            .push(AdminBlobUploaderRef { blob_id, user_id });
    }

    Ok(grouped.into_iter().collect())
}

fn apply_admin_file_order(
    query: sea_orm::SelectTwo<file::Entity, file_blob::Entity>,
    sort_by: AdminFileSortBy,
    sort_order: SortOrder,
) -> sea_orm::SelectTwo<file::Entity, file_blob::Entity> {
    match sort_by {
        AdminFileSortBy::Id => order_by_id(query, file::Column::Id, sort_order),
        AdminFileSortBy::Name => {
            order_by_column_with_id(query, file::Column::Name, sort_order, file::Column::Id)
        }
        AdminFileSortBy::Size => {
            order_by_column_with_id(query, file::Column::Size, sort_order, file::Column::Id)
        }
        AdminFileSortBy::BlobId => {
            order_by_column_with_id(query, file::Column::BlobId, sort_order, file::Column::Id)
        }
        AdminFileSortBy::PolicyId => order_by_column_with_id(
            query,
            file_blob::Column::PolicyId,
            sort_order,
            file::Column::Id,
        ),
        AdminFileSortBy::OwnerUserId => order_by_column_with_id(
            query,
            file::Column::OwnerUserId,
            sort_order,
            file::Column::Id,
        ),
        AdminFileSortBy::TeamId => {
            order_by_column_with_id(query, file::Column::TeamId, sort_order, file::Column::Id)
        }
        AdminFileSortBy::CreatedAt => {
            order_by_column_with_id(query, file::Column::CreatedAt, sort_order, file::Column::Id)
        }
        AdminFileSortBy::UpdatedAt => {
            order_by_column_with_id(query, file::Column::UpdatedAt, sort_order, file::Column::Id)
        }
        AdminFileSortBy::DeletedAt => {
            order_by_column_with_id(query, file::Column::DeletedAt, sort_order, file::Column::Id)
        }
    }
}

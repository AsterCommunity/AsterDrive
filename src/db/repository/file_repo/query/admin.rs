use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect,
};
use std::collections::{BTreeMap, HashMap};

use crate::api::pagination::AdminFileSortBy;
use crate::errors::{AsterError, Result};
use aster_drive_model::entities::{
    file::{self, Entity as File},
    file_blob,
};
use aster_forge_api::SortOrder;
use aster_forge_db::search_query::lower_like_condition;
use aster_forge_db::sort::{order_by_column_with_id, order_by_id};

/// Filters for the admin file listing; `None` fields are not applied.
#[derive(Debug, Clone, Copy)]
pub struct AdminFileFilters<'a> {
    /// Case-insensitive substring match on the file name.
    pub name: Option<&'a str>,
    /// Restrict to files backed by this blob.
    pub blob_id: Option<i64>,
    /// Restrict to files whose blob is stored under this policy.
    pub policy_id: Option<i64>,
    /// Restrict to files owned by this user.
    pub owner_user_id: Option<i64>,
    /// Restrict to files in this team workspace.
    pub team_id: Option<i64>,
    /// `Some(true)` lists only trashed files, `Some(false)` only live files, `None` both.
    pub deleted: Option<bool>,
    /// Column the listing is sorted by.
    pub sort_by: AdminFileSortBy,
    /// Sort direction; file id is the stable tiebreaker.
    pub sort_order: SortOrder,
}

/// One (blob, uploading user) attribution pair used by admin blob inspection to show who
/// created files referencing a blob.
#[derive(Debug, Clone)]
pub struct AdminBlobUploaderRef {
    /// The referenced blob.
    pub blob_id: i64,
    /// The file creator recorded on a referencing file row.
    pub user_id: i64,
}

/// Lists files joined with their blob rows under [`AdminFileFilters`], returning the page
/// and total. A file whose blob row is missing is an internal error, not a skipped row.
pub async fn find_admin_files_paginated(
    db: &DatabaseConnection,
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

/// Loads one admin file view as a (file, blob) pair; missing file is a not-found error,
/// a file with a missing blob row is an internal error.
pub async fn find_admin_file_by_id(
    db: &DatabaseConnection,
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

/// Lists every file referencing `blob_id`, ordered by file id; admin blob inspection and
/// cleanup checks use it to show what holds a blob.
pub async fn find_by_blob_id(db: &DatabaseConnection, blob_id: i64) -> Result<Vec<file::Model>> {
    File::find()
        .filter(file::Column::BlobId.eq(blob_id))
        .order_by_asc(file::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// Groups distinct (blob, creator) pairs of the given blobs into a per-blob uploader
/// list, sorted by blob id then user id. Files without a recorded creator are excluded.
pub async fn find_admin_blob_uploader_refs_for_blobs(
    db: &DatabaseConnection,
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
